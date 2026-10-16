//! Services bound to one approved compatibility installation.

use chilpa_plugin::{bridge::{BridgeError, ServiceOperation, WorkerServices}, Manifest};
use chilpa_secrets::PrivateStore;
use serde_json::Value;
use std::{sync::{Arc, Mutex}, time::Instant, collections::{BTreeMap, BTreeSet}};
pub(crate) type NetworkRequests = Arc<Mutex<BTreeMap<String, BTreeSet<String>>>>;

/// How long a plugin's GET answer is reused, and how many are kept. A
/// guest asks the same thing over and over while it builds one menu (the
/// same artist three times for one record); a few minutes is short enough
/// that nothing it shows goes stale in a way anyone notices.
const ANSWER_FRESH: std::time::Duration = std::time::Duration::from_secs(300);
const ANSWERS_KEPT: usize = 512;
/// An answer larger than this is not kept.
const ANSWER_LARGEST: usize = 1 << 20;

/// GET answers by plugin, address and headers: the headers carry the
/// account, so one account's answer is never another's.
fn answers() -> &'static Mutex<std::collections::HashMap<u64, (Instant, Value)>> {
    static ANSWERS: std::sync::OnceLock<Mutex<std::collections::HashMap<u64, (Instant, Value)>>> = std::sync::OnceLock::new();
    ANSWERS.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

fn answer_key(plugin: &str, request: &Value) -> Option<u64> {
    use std::hash::{Hash, Hasher};
    let method = request.get("method").and_then(Value::as_str).unwrap_or("GET");
    let body = request.get("body_base64").and_then(Value::as_str).unwrap_or("");
    let url = request.get("url").and_then(Value::as_str)?;
    // A sign-in asks the same place again and again for a different answer.
    // Only the path says so: a query can carry a token on every request.
    let lower = url::Url::parse(url).ok()?.path().to_ascii_lowercase();
    if method != "GET" || !body.is_empty() || ["token", "login", "auth", "session"].iter().any(|w| lower.contains(w)) {
        return None;
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (plugin, url, request.get("headers").map(Value::to_string)).hash(&mut hasher);
    Some(hasher.finish())
}

pub struct WorkerContext {
    manifest: Manifest,
    store: Arc<PrivateStore>,
    requests: Option<NetworkRequests>,
}

impl WorkerContext {
    /// The installer constructs this after approval, using its normalized
    /// manifest. The worker cannot replace the owner or its permissions.
    pub fn new(manifest: Manifest, store: Arc<PrivateStore>) -> Self {
        Self { manifest, store, requests: None }
    }
    pub(crate) fn with_requests(mut self, requests: NetworkRequests) -> Self { self.requests = Some(requests); self }
}

impl WorkerServices for WorkerContext {
    fn handle(&mut self, operation: ServiceOperation, deadline: Instant) -> Result<Value, BridgeError> {
        if Instant::now() >= deadline { return Err(BridgeError::Timeout); }
        match operation {
            ServiceOperation::PrivateGet { key } => {
                if !self.manifest.can_store() { return Err(BridgeError::Denied); }
                let bytes = self.store.get(&self.manifest.id, &key).map_err(|_| BridgeError::Storage)?;
                match bytes {
                    Some(bytes) => serde_json::from_slice(&bytes).map_err(|_| BridgeError::Storage),
                    None => Ok(Value::Null),
                }
            }
            ServiceOperation::PrivateSet { key, value } => {
                if !self.manifest.can_store() { return Err(BridgeError::Denied); }
                let bytes = serde_json::to_vec(&value).map_err(|_| BridgeError::InvalidMessage)?;
                self.store.set(&self.manifest.id, &key, &bytes).map_err(|_| BridgeError::Storage)?;
                Ok(Value::Bool(true))
            }
            ServiceOperation::Http { request } => {
                let timeout = deadline.checked_duration_since(Instant::now()).ok_or(BridgeError::Timeout)?;
                let domains = self.manifest.allowed_domains().into_iter().map(str::to_string).collect::<Vec<_>>();
                if let Some(address) = request["url"].as_str() {
                    if !chilpa_core::worker_url_allowed(address, &domains) {
                        if let (Some(requests), Some(host)) = (&self.requests, url::Url::parse(address).ok().and_then(|url| url.host_str().map(str::to_string))) {
                            if let Ok(mut requests) = requests.lock() {
                                let pending = requests.entry(self.manifest.id.clone()).or_default();
                                if pending.len() < 128 { pending.insert(host); }
                            }
                        }
                    }
                }
                let key = answer_key(&self.manifest.id, &request);
                if let Some(kept) = key.and_then(|k| answers().lock().ok()?.get(&k).filter(|(at, _)| at.elapsed() < ANSWER_FRESH).map(|(_, v)| v.clone())) {
                    return Ok(kept);
                }
                let started = Instant::now();
                let reply = chilpa_core::send_worker_request(&request, &domains, timeout);
                if let (Some(key), Ok(answer)) = (key, &reply) {
                    let fits = answer["body_base64"].as_str().is_some_and(|b| b.len() < ANSWER_LARGEST);
                    if answer["status"].as_u64() == Some(200) && fits {
                        if let Ok(mut kept) = answers().lock() {
                            if kept.len() >= ANSWERS_KEPT { kept.retain(|_, (at, _)| at.elapsed() < ANSWER_FRESH); }
                            if kept.len() < ANSWERS_KEPT { kept.insert(key, (Instant::now(), answer.clone())); }
                        }
                    }
                }
                // The host only: an address can carry a signature or a token.
                let place = request["url"].as_str().and_then(|a| url::Url::parse(a).ok())
                    .map(|u| format!("{}{}", u.host_str().unwrap_or_default(), u.path().chars().take(48).collect::<String>())).unwrap_or_default();
                eprintln!("worker-http: at={place} status={} elapsed_ms={}", reply.as_ref().ok().and_then(|r| r["status"].as_u64()).unwrap_or(0), started.elapsed().as_millis());
                reply.map_err(|_| BridgeError::Denied)
            }
        }
    }
}
