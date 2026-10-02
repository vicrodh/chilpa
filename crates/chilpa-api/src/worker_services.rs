//! Services bound to one approved compatibility installation.

use chilpa_plugin::{bridge::{BridgeError, ServiceOperation, WorkerServices}, Manifest};
use chilpa_secrets::PrivateStore;
use serde_json::Value;
use std::{sync::{Arc, Mutex}, time::Instant, collections::{BTreeMap, BTreeSet}};
pub(crate) type NetworkRequests = Arc<Mutex<BTreeMap<String, BTreeSet<String>>>>;

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
                let started = Instant::now();
                let reply = chilpa_core::send_worker_request(&request, &domains, timeout);
                // The host only: an address can carry a signature or a token.
                let place = request["url"].as_str().and_then(|a| url::Url::parse(a).ok())
                    .map(|u| format!("{}{}", u.host_str().unwrap_or_default(), u.path().chars().take(48).collect::<String>())).unwrap_or_default();
                eprintln!("worker-http: at={place} status={} elapsed_ms={}", reply.as_ref().ok().and_then(|r| r["status"].as_u64()).unwrap_or(0), started.elapsed().as_millis());
                reply.map_err(|_| BridgeError::Denied)
            }
        }
    }
}
