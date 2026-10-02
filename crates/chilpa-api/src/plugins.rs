//! The plugins, as the facade sees them.
//!
//! Here the two halves meet: the sandbox, which lives in `chilpa-plugin` and
//! knows neither network nor disk, and what Chilpa lends it. Native guests
//! never receive credentials. Compatibility workers may handle their own
//! credentials only with separate approval and encrypted private storage.
//!
//! Plugins are read from `~/.local/share/chilpa/plugins/<id>/`, with their
//! `manifest.json` and their `plugin.wasm`. One that does not load is noted
//! with its reason and does not stop the rest from loading.

use super::*;
use crate::accounts::Account;
use crate::approvals::Approvals;
use chilpa_plugin::{Host, HostServices, Limits, Level, Registry, Services};
use chilpa_auth::Auth;
use chilpa_secrets::{BackendKind, SecretBox};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// What Chilpa lends a plugin.
///
/// What is not here does not exist for the guest.
pub struct ChilpaServices {
    root: PathBuf,
    /// How each plugin connects its account, taken from its manifest at
    /// load time. It is needed here so an expired access can be renewed
    /// without the plugin taking part.
    auth: Arc<Mutex<BTreeMap<String, Auth>>>,
    /// Where the credentials are opened. Shared with `Plugins`, which is
    /// what keeps them: one box so both shores use the same master key.
    secrets: Option<Arc<SecretBox>>,
}

impl ChilpaServices {
    pub fn new(root: PathBuf, secrets: Option<Arc<SecretBox>>) -> Self {
        Self { root, secrets, auth: Arc::new(Mutex::new(BTreeMap::new())) }
    }

    /// The file where one key of a plugin's store lives.
    ///
    /// The key is checked before it becomes a file name: one carrying
    /// slashes or double dots would escape the plugin's folder, which is
    /// exactly what the sandbox is there to prevent.
    fn file(&self, plugin: &str, key: &str) -> Option<PathBuf> {
        if !valid_key(key) || !valid_id(plugin) {
            return None;
        }
        Some(self.root.join(plugin).join("store").join(key))
    }

    /// The credential kept for a plugin, if there is one.
    ///
    /// It lives encrypted in the plugin's folder but **outside its store**,
    /// and the guest has no way to ask for it: there is no import that
    /// returns it, and its `store_get` does not reach this file (ADR-008).
    ///
    /// Failing to open it is not papered over in any way: the answer is
    /// that there is no credential, and the request goes out without
    /// `Authorization`. Inventing something here would be sending junk to
    /// the user's service.
    fn credential(&self, plugin: &str) -> Option<String> {
        let account = self.account(plugin)?;
        let now = now_unix();

        // If the access expired and there is something to renew it with, it
        // is renewed here, kept encrypted, and the request goes out with the
        // new one. The plugin never finds out a renewal happened, which is
        // exactly the point.
        if account.stale(now) {
            let auth = self.auth.lock().ok()?.get(plugin).cloned()?;
            match account.renew(&auth, now, &post) {
                Ok(fresh) => {
                    let _ = self.save(plugin, &fresh);
                    return fresh.header();
                }
                // A failed renewal is not papered over by inventing
                // something: the request goes out with no credential and
                // the service will say no, which is more honest than
                // sending an expired token.
                Err(_) => return None,
            }
        }
        account.header()
    }

    /// What is kept for that plugin, decrypted.
    fn account(&self, plugin: &str) -> Option<Account> {
        open_account(&self.root, self.secrets.as_deref(), plugin)
    }

    /// Keeps it, encrypted.
    fn save(&self, plugin: &str, account: &Account) -> Result<(), String> {
        seal_account(&self.root, self.secrets.as_deref(), plugin, account)
    }
}

/// Reads a plugin's encrypted account, if there is one.
fn open_account(root: &Path, secrets: Option<&SecretBox>, plugin: &str) -> Option<Account> {
    let path = credential_file(root, plugin)?;
    let sealed = std::fs::read(path).ok()?;
    let plain = secrets?.open_sealed(&sealed).ok()?;
    serde_json::from_slice(&plain).ok()
}

/// Keeps an encrypted account in its plugin's folder.
///
/// It lives on its own because both sides use it: whoever connects the
/// account and whoever renews it mid request, and one of them arrives
/// through a `dyn HostServices` that knows nothing about this.
fn seal_account(
    root: &Path,
    secrets: Option<&SecretBox>,
    plugin: &str,
    account: &Account,
) -> Result<(), String> {
    let secrets = secrets
        .ok_or_else(|| chilpa_i18n::t("there is nowhere to keep a secret on this machine"))?;
    let path = credential_file(root, plugin)
        .ok_or_else(|| chilpa_i18n::t("that id does not work as a folder"))?;
    let plain = serde_json::to_vec(account).map_err(|e| e.to_string())?;
    let sealed = secrets.seal(&plain).map_err(|e| e.to_string())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, sealed).map_err(|e| e.to_string())
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// The request `chilpa-auth` does not carry inside: that crate knows
/// nothing of the network, and here it is lent the same one everything
/// else uses.
fn post(url: &str, body: &str) -> Result<(u16, String), String> {
    let request = serde_json::json!({
        "url": url,
        "method": "POST",
        "headers": {
            "Content-Type": "application/x-www-form-urlencoded",
            "Accept": "application/json",
        },
        "body": body,
    });
    let raw = chilpa_core::send_request(&serde_json::to_vec(&request).unwrap(), None);
    let reply: serde_json::Value = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
    if let Some(e) = reply.get("error").and_then(|e| e.as_str()) {
        return Err(e.to_string());
    }
    Ok((
        reply.get("status").and_then(|s| s.as_u64()).unwrap_or(0) as u16,
        reply.get("body").and_then(|b| b.as_str()).unwrap_or_default().to_string(),
    ))
}

/// Where a plugin's encrypted credential lives.
///
/// The id is checked before it becomes a path, just like a store key:
/// one with slashes would escape its folder.
fn credential_file(root: &Path, plugin: &str) -> Option<PathBuf> {
    valid_id(plugin).then(|| root.join(plugin).join("credential.sealed"))
}

impl ChilpaServices {
    fn auth_map_inner(&self) -> Arc<Mutex<chilpa_plugin::AuthMap>> {
        Arc::clone(&self.auth)
    }
}

impl HostServices for ChilpaServices {
    fn auth_map(&self) -> Option<Arc<Mutex<chilpa_plugin::AuthMap>>> {
        Some(self.auth_map_inner())
    }

    fn log(&mut self, plugin: &str, level: Level, message: &str) {
        // With the id in front: a log line with no owner is no use for
        // knowing who to take it up with.
        eprintln!("[plugin {plugin}] {level:?}: {message}");
    }

    fn now_unix(&mut self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }

    fn store_get(&mut self, plugin: &str, key: &str) -> Option<Vec<u8>> {
        std::fs::read(self.file(plugin, key)?).ok()
    }

    fn store_set(&mut self, plugin: &str, key: &str, value: &[u8]) -> bool {
        let Some(path) = self.file(plugin, key) else { return false };
        // A plugin cannot fill the disk a bit at a time: the store is for
        // settings and markers, not for a copy of the catalogue.
        if value.len() > 1024 * 1024 {
            return false;
        }
        if let Some(dir) = path.parent() {
            if std::fs::create_dir_all(dir).is_err() {
                return false;
            }
        }
        std::fs::write(path, value).is_ok()
    }

    fn http(&mut self, plugin: &str, request: &[u8]) -> Vec<u8> {
        // The credential is added here. The plugin said where to; with what
        // permission is Chilpa's call.
        let credential = self.credential(plugin);
        chilpa_core::send_request(request, credential.as_deref())
    }
}

/// A store key works if it can become a file name without escaping its
/// folder.
fn valid_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 64
        && !key.contains("..")
        && key.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-' || c == '_')
}

/// The same for the id, which is also a folder.
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && !id.contains("..")
        && !id.contains('/')
        && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-')
}

/// A plugin, as the view sees it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    /// What it says it is, in its own words. Empty if it says nothing.
    pub description: String,
    /// "awaiting approval", "fine", "stumbling" or "quarantined".
    pub state: String,
    /// Why it is in the corner, or empty.
    pub problem: String,
    /// What it asks for, in words, so it can be approved or refused.
    pub asks: Vec<String>,
    /// Of what it asks for, what has not been approved yet. Empty on one
    /// already approved; everything it asks for on a new one.
    pub pending: Vec<String>,
    /// True if something had been approved for it and now it asks for
    /// more. It is different from a new one, and it is said differently.
    pub wants_more: bool,
    /// True if it has a credential kept. It does not say which, and the
    /// facade never shows it: only whether an account is connected or not.
    pub connected: bool,
    /// True if the account expired and there is nothing to renew it with
    /// on its own: the user has to connect it again. It is said instead of
    /// letting everything start failing with no explanation.
    pub account_expired: bool,
    /// Where the user is sent to grant permission, empty if it does not use
    /// OAuth or needs no account.
    pub account_host: String,
    /// "token", "oauth" or empty when the plugin needs no Chilpa-managed
    /// account.  The frontend needs the distinction because one flow asks
    /// for text and the other opens the browser.
    pub account_kind: String,
}

/// A half finished account connection: the user has been sent to the
/// browser and it is waiting for them to come back.
///
/// The wait runs on its own thread because the user can take as long as
/// they like -- up to five minutes -- and the window cannot sit frozen
/// meanwhile. What it answers arrives through `poll`, just like search.
pub struct Connecting {
    pub id: String,
    pub answer: std::sync::mpsc::Receiver<Result<chilpa_auth::Tokens, String>>,
    pub thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Connecting {
    fn drop(&mut self) {
        // The thread is not waited on: if the user walked away from the
        // browser, going on waiting would leave the app unable to close. The
        // thread ends on its own when its patience runs out.
        self.thread.take();
    }
}

/// Everything about plugins the facade keeps.
pub struct Plugins {
    host: Host,
    registry: Registry,
    services: Services,
    approvals: Approvals,
    /// The secret box, or `None` if none could be opened. Without it
    /// nothing is kept: rather than leave a token in the clear, nothing is
    /// kept at all.
    secrets: Option<Arc<SecretBox>>,
    root: PathBuf,
    private: Option<Arc<chilpa_secrets::PrivateStore>>,
    worker_launcher: Option<Arc<dyn chilpa_plugin::bridge::WorkerLauncher>>,
    pub(crate) layouts: BTreeMap<String, crate::source_layout::Layout>,
    pub(crate) network_requests: crate::worker_services::NetworkRequests,
    /// When each worker was last brought back on its own, to bound it.
    revived: BTreeMap<String, Vec<std::time::Instant>>,
    /// Counts the times each plugin was loaded. A handle a worker gave out
    /// is only good for the epoch it was given in.
    epochs: BTreeMap<String, u64>,
}

/// How many times a dead worker is started again on its own, and within
/// what time, before it is left to the watchdog and the user.
const REVIVALS: usize = 3;
const REVIVAL_WINDOW: std::time::Duration = std::time::Duration::from_secs(600);

impl Plugins {
    /// Builds the host. It only fails if the engine could not be created, which
    /// would be a problem with the machine and not with a plugin.
    pub fn new(root: PathBuf) -> Option<Self> {
        let host = Host::new().ok()?;
        let secrets = SecretBox::open("chilpa", &root).ok().map(Arc::new);
        let services: Services =
            Arc::new(Mutex::new(ChilpaServices::new(root.clone(), secrets.clone())));
        let approvals = Approvals::load(&root);
        let private = secrets.clone().map(|secrets| Arc::new(chilpa_secrets::PrivateStore::new(root.join(".worker-private"), secrets)));
        let worker_launcher: Option<Arc<dyn chilpa_plugin::bridge::WorkerLauncher>> = None;
        #[cfg(target_os = "linux")]
        let worker_launcher = chilpa_bwrap::RuntimePaths::installed(&root.join(".runtime"))
            .map(|paths| Arc::new(paths) as Arc<dyn chilpa_plugin::bridge::WorkerLauncher>).or(worker_launcher);
        Some(Self { host, registry: Registry::new(), services, approvals, secrets, root, private, worker_launcher,
            layouts: BTreeMap::new(), network_requests: Arc::new(Mutex::new(BTreeMap::new())),
            revived: BTreeMap::new(), epochs: BTreeMap::new() })
    }

    /// The application supplies a packaged, isolated runtime. Package
    /// metadata never supplies executable paths or command-line arguments.
    pub fn set_worker_launcher(&mut self, launcher: Arc<dyn chilpa_plugin::bridge::WorkerLauncher>) {
        self.worker_launcher = Some(launcher);
    }

    pub(crate) fn compatibility_launcher(&self) -> Option<Arc<dyn chilpa_plugin::bridge::WorkerLauncher>> { self.worker_launcher.clone() }
    pub(crate) fn reload_one(&mut self, id: &str) {
        if valid_id(id) { self.load_one(&self.root.join(id)); self.remember_auth(); }
    }
    pub(crate) fn grant_network(&mut self, id: &str, domain: &str) -> Result<(), String> {
        let mut manifest = self.registry.installed().find(|plugin| plugin.id == id)
            .and_then(|plugin| plugin.manifest.clone()).ok_or("The plugin is not installed.")?;
        let mut domains = manifest.allowed_domains().into_iter().map(str::to_string).collect::<Vec<_>>();
        if !domains.iter().any(|existing| existing == domain) { domains.push(domain.to_string()); }
        manifest.capabilities.retain(|cap| !matches!(cap, chilpa_plugin::Capability::Network { .. }));
        manifest.capabilities.push(chilpa_plugin::Capability::Network { domains });
        let encoded = serde_json::to_string_pretty(&manifest).map_err(|_| "Invalid manifest.")?;
        chilpa_plugin::Manifest::parse_for_runtime(&encoded, manifest.runtime).map_err(|error| error.to_string())?;
        let mut temporary = tempfile::NamedTempFile::new_in(self.root.join(id)).map_err(|_| "Could not update permissions.")?;
        use std::io::Write;
        temporary.write_all(encoded.as_bytes()).and_then(|_| temporary.as_file().sync_all()).map_err(|_| "Could not update permissions.")?;
        self.approvals.approve(&manifest).map_err(|error| error.to_string())?;
        temporary.persist(self.root.join(id).join("manifest.json")).map_err(|_| "Could not update permissions.")?;
        if let Ok(mut requests) = self.network_requests.lock() { if let Some(pending) = requests.get_mut(id) { if domain == "*" { pending.clear(); } else { pending.remove(domain); } } }
        self.reload_one(id);
        Ok(())
    }

    /// Reads the plugins folder and loads whatever it finds.
    ///
    /// It returns how many ended up usable. A folder that does not exist is
    /// not an error: it means there are no plugins.
    pub fn load_all(&mut self) -> usize {
        self.load_all_with(|_, _, _| {})
    }

    /// The same, saying before each one how many there are, how many are
    /// loaded and which comes next, for a start that shows its progress.
    pub fn load_all_with(&mut self, mut step: impl FnMut(usize, usize, &str)) -> usize {
        let Ok(entries) = std::fs::read_dir(&self.root) else { return 0 };
        let dirs: Vec<_> = entries.flatten().filter(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
            .map(|entry| entry.path()).filter(|dir| dir.is_dir()).collect();
        for (done, dir) in dirs.iter().enumerate() {
            let name = std::fs::read_to_string(dir.join("manifest.json")).ok()
                .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
                .and_then(|value| value["name"].as_str().map(str::to_string)).unwrap_or_default();
            step(done, dirs.len(), &name);
            self.load_one(dir);
        }
        step(dirs.len(), dirs.len(), "");
        self.remember_auth();
        self.registry.usable().len()
    }

    fn load_one(&mut self, dir: &Path) {
        let manifest = match std::fs::read_to_string(dir.join("manifest.json")) {
            Ok(m) => m,
            // A folder with no manifest is not a broken plugin: it is not a
            // plugin. It is walked past without noting anything.
            Err(_) => return,
        };
        if let Some(id) = serde_json::from_str::<serde_json::Value>(&manifest).ok().and_then(|value| value["id"].as_str().filter(|id| valid_id(id)).map(str::to_string)) {
            self.layouts.insert(id.clone(), crate::source_layout::Layout::load(&self.root, &id));
            *self.epochs.entry(id).or_default() += 1;
        }
        if serde_json::from_str::<serde_json::Value>(&manifest).ok()
            .is_some_and(|value| value["runtime"] == "bridge") {
            let approvals = &self.approvals;
            crate::worker_install::install(&mut self.registry, &manifest, dir, self.worker_launcher.as_ref(), self.private.as_ref(),
                self.network_requests.clone(),
                |manifest| approvals.covers(manifest));
            return;
        }
        let wasm = std::fs::read(dir.join("plugin.wasm")).unwrap_or_default();
        // A plugin nobody approved is not even compiled: the closure
        // answers no and its wasm is never touched (ADR-008).
        let approvals = &self.approvals;
        self.registry.install(&self.host, &manifest, &wasm, |m| approvals.covers(m));
    }

    /// Hands the services how each plugin connects its account.
    ///
    /// It is all that is needed to renew an expired access mid request, and
    /// it is what avoids having to ask the registry from inside a call that
    /// already holds the lock.
    fn remember_auth(&mut self) {
        let known: BTreeMap<String, Auth> = self
            .registry
            .installed()
            .filter_map(|i| Some((i.id.clone(), i.manifest.as_ref()?.auth.clone()?)))
            .collect();
        if let Ok(services) = self.services.lock() {
            if let Some(map) = services.auth_map() {
                if let Ok(mut m) = map.lock() {
                    *m = known;
                }
            }
        }
    }

    /// What there is, for showing.
    pub fn list(&self) -> Vec<PluginInfo> {
        self.registry
            .installed()
            .map(|i| {
                let state = self.registry.state(&i.id);
                PluginInfo {
                    id: i.id.clone(),
                    name: if i.name.is_empty() { i.id.clone() } else { i.name.clone() },
                    description: i.manifest.as_ref().map(|m| m.description.trim().to_string()).unwrap_or_default(),
                    state: match &state {
                        chilpa_plugin::State::PendingApproval => chilpa_i18n::mark("awaiting approval").into(),
                        chilpa_plugin::State::Healthy => chilpa_i18n::mark("fine").into(),
                        chilpa_plugin::State::Stumbling { .. } => chilpa_i18n::mark("stumbling").into(),
                        chilpa_plugin::State::Quarantined { .. } => chilpa_i18n::mark("quarantined").into(),
                    },
                    problem: match (&state, &i.problem) {
                        (chilpa_plugin::State::Quarantined { reason }, _) => reason.clone(),
                        (_, Some(p)) => p.clone(),
                        _ => String::new(),
                    },
                    asks: i.asks(),
                    pending: i
                        .manifest
                        .as_ref()
                        .map(|m| self.approvals.beyond(m))
                        .unwrap_or_default(),
                    wants_more: self.approvals.knows(&i.id),
                    connected: self.has_credential(&i.id),
                    account_expired: self.expired(&i.id),
                    account_host: i
                        .manifest
                        .as_ref()
                        .and_then(|m| m.auth.as_ref())
                        .map(|a| a.host())
                        .unwrap_or_default(),
                    account_kind: i
                        .manifest
                        .as_ref()
                        .and_then(|m| m.auth.as_ref())
                        .map(|a| match a {
                            Auth::Token => "token",
                            Auth::OAuth2 { .. } => "oauth",
                        })
                        .unwrap_or_default()
                        .into(),
                }
            })
            .collect()
    }

    /// Takes it out of quarantine. Only the user calls it.
    pub fn reactivate(&mut self, id: &str) {
        self.registry.reactivate(id);
        let worker = self.registry.installed().find(|plugin| plugin.id == id)
            .and_then(|plugin| plugin.manifest.as_ref())
            .is_some_and(|manifest| manifest.runtime == chilpa_plugin::Runtime::Bridge);
        if worker {
            // A revoked transport is intentionally terminal. Reactivating
            // creates a fresh isolated worker with the current approvals.
            self.load_one(&self.root.join(id));
        }
    }

    /// Keeps a plugin's credential, encrypted.
    ///
    /// The plugin never sees it: all that happens with it is that Chilpa
    /// puts it in its requests (ADR-008).
    pub fn set_credential(&mut self, id: &str, value: &str) -> Result<(), String> {
        self.save_account(id, &Account::Token { header: value.to_string() })
    }

    /// Keeps an already connected account, fixed or OAuth.
    pub fn save_account(&mut self, id: &str, account: &Account) -> Result<(), String> {
        seal_account(&self.root, self.secrets.as_deref(), id, account)
    }

    /// Deletes a plugin's credential. Revoking it deletes it too.
    pub fn forget_credential(&mut self, id: &str) {
        if let Some(path) = credential_file(&self.root, id) {
            let _ = std::fs::remove_file(path);
        }
        if let Some(store) = &self.private {
            let _ = store.remove_owner(id);
        }
    }

    /// True if that plugin's account expired with no way to renew itself.
    fn expired(&self, id: &str) -> bool {
        open_account(&self.root, self.secrets.as_deref(), id)
            .is_some_and(|a| a.dead(now_unix()))
    }

    /// True if that plugin has a credential kept.
    pub fn has_credential(&self, id: &str) -> bool {
        credential_file(&self.root, id).is_some_and(|p| p.exists())
    }

    /// Where the secrets are being kept, so it can be stated.
    pub fn secrets_backend(&self) -> Option<BackendKind> {
        self.secrets.as_ref().map(|s| s.backend())
    }

    /// Forgets everything loaded. It is used on revoke: a plugin that
    /// stopped being approved has to stop existing in this session, not
    /// stay instantiated until the next startup.
    pub fn forget(&mut self) {
        self.registry = Registry::new();
    }

    /// The user approves what that plugin asks for today.
    ///
    /// It does not load it: loading means rereading the folder, and the
    /// caller does that, so approving two in a row does not recompile
    /// everything twice.
    pub(crate) fn enabled(&self, id: &str) -> bool {
        self.registry.installed().find(|plugin| plugin.id == id).and_then(|plugin| plugin.manifest.as_ref())
            .is_some_and(|manifest| self.approvals.covers(manifest))
    }

    pub fn approve(&mut self, id: &str) -> Result<(), String> {
        let manifest = self
            .registry
            .installed()
            .find(|i| i.id == id)
            .and_then(|i| i.manifest.clone())
            .ok_or_else(|| chilpa_i18n::t_args("there is no plugin \"{}\" with a manifest to approve", &[id]))?;
        self.approvals.approve(&manifest).map_err(|e| e.to_string())
    }

    /// Withdraws the approval. The plugin stops loading and what it asks
    /// for is put to the user again.
    pub fn revoke(&mut self, id: &str) -> Result<(), String> {
        self.approvals.revoke(id).map_err(|e| e.to_string())
    }

    /// How that plugin connects its account, according to its manifest.
    pub fn auth_of(&self, id: &str) -> Option<chilpa_auth::Auth> {
        self.registry.installed().find(|i| i.id == id)?.manifest.as_ref()?.auth.clone()
    }

    /// Takes the plugin's folder away. Returns true if it is gone.
    pub fn uninstall(&mut self, id: &str) -> bool {
        let Some(dir) = valid_id(id).then(|| self.root.join(id)) else { return false };
        if !dir.exists() {
            return true;
        }
        if std::fs::remove_dir_all(&dir).is_err() { return false; }
        self.registry.remove(id);
        self.layouts.remove(id);
        if let Ok(mut requests) = self.network_requests.lock() { requests.remove(id); }
        true
    }

    /// True if that plugin said it handles its own account.
    pub fn manages_own_account(&self, id: &str) -> bool {
        self.registry
            .installed()
            .find(|i| i.id == id)
            .and_then(|i| i.manifest.as_ref())
            .is_some_and(|m| {
                m.capabilities
                    .iter()
                    .any(|c| {
                        matches!(c, chilpa_plugin::manifest::Capability::OwnAccount { .. })
                    })
            })
    }

    /// True if that source says it can deliver audio.
    pub fn delivers_audio(&self, id: &str) -> bool {
        self.registry
            .installed()
            .find(|i| i.id == id)
            .and_then(|i| i.manifest.as_ref())
            .is_none_or(|m| m.delivers_audio)
    }

    /// The catalogue plugins that work now, with their name.
    pub fn searchers(&self) -> Vec<(String, String)> {
        self.registry.usable_of_kind(chilpa_plugin::manifest::Kind::Source)
    }

    /// The plugins that put an entry of their own in the window: id, name
    /// and icon hint.
    pub fn panels(&self) -> Vec<(String, String, String)> {
        self.registry.panels()
    }

    pub(crate) fn worker_domains(&self, id: &str) -> Option<Vec<String>> {
        let manifest = self.registry.installed().find(|plugin| plugin.id == id)?.manifest.as_ref()?;
        (manifest.runtime == chilpa_plugin::Runtime::Bridge)
            .then(|| manifest.allowed_domains().into_iter().map(str::to_string).collect())
    }

    /// Asks a plugin for something that goes out to the network and back
    /// before it answers, like listing a folder of its tree.
    ///
    /// The compute and memory limits are the usual ones. The clock is not:
    /// it also runs while the host waits on the network for the plugin, and
    /// a directory with a slow mirror takes longer than the five seconds a
    /// plain call gets. It is only ever called off the window's thread.
    pub fn call_waiting(
        &mut self,
        id: &str,
        operation: &str,
        input: &[u8],
    ) -> Result<Vec<u8>, String> {
        let limits = Limits { timeout: std::time::Duration::from_secs(45), ..Limits::default() };
        self.call_with(id, operation, input, limits)
    }

    /// Asks a plugin for something.
    pub fn call(&mut self, id: &str, operation: &str, input: &[u8]) -> Result<Vec<u8>, String> {
        self.call_with(id, operation, input, Limits::default())
    }

    /// The same, with the short limits: it is what is used when someone is
    /// waiting in front of the window, as when searching.
    pub fn call_short(
        &mut self,
        id: &str,
        operation: &str,
        input: &[u8],
    ) -> Result<Vec<u8>, String> {
        self.call_with(id, operation, input, Limits::short())
    }

    fn call_with(
        &mut self,
        id: &str,
        operation: &str,
        input: &[u8],
        limits: Limits,
    ) -> Result<Vec<u8>, String> {
        self.revive_if_dead(id);
        let started = std::time::Instant::now();
        let result = self.registry.call(&self.host, &self.services, id, operation, input, limits);
        eprintln!("plugin-call: plugin={id} operation={operation} ok={} elapsed_ms={}", result.is_ok(), started.elapsed().as_millis());
        if result.is_err() && !self.registry.alive(id) {
            // A worker that died on this call is not left dead for the next
            // one: that is what used to need a restart from Settings. One
            // found already dead is asked again, when the question only reads.
            let found_dead = started.elapsed() < std::time::Duration::from_secs(1);
            let reads = matches!(operation, chilpa_plugin::op::BROWSE | chilpa_plugin::op::SEARCH | chilpa_plugin::op::RESOLVER | "source.metadata");
            self.revive_if_dead(id);
            if found_dead && reads && self.registry.alive(id) {
                let result = self.registry.call(&self.host, &self.services, id, operation, input, limits);
                eprintln!("plugin-call: plugin={id} operation={operation} retried=true ok={}", result.is_ok());
                return match result {
                    Ok(chilpa_plugin::Reply::Ok(bytes)) => Ok(bytes),
                    Ok(chilpa_plugin::Reply::Error(e)) => Err(e),
                    Err(e) => Err(e.to_string()),
                };
            }
        }
        match result {
            Ok(chilpa_plugin::Reply::Ok(bytes)) => Ok(bytes),
            Ok(chilpa_plugin::Reply::Error(e)) => Err(e),
            Err(e) => Err(e.to_string()),
        }
    }

    /// Which load of a plugin is running. It changes whenever its worker is
    /// started again, and with it every handle the worker gave out.
    pub(crate) fn epoch(&self, id: &str) -> u64 {
        self.epochs.get(id).copied().unwrap_or_default()
    }

    /// Starts a compatibility worker again when its session ended, with its
    /// private storage and approvals as they are. It is bounded, so a guest
    /// that dies on every call ends in quarantine instead of a loop.
    fn revive_if_dead(&mut self, id: &str) {
        if self.registry.alive(id) {
            return;
        }
        let now = std::time::Instant::now();
        let times = self.revived.entry(id.to_string()).or_default();
        times.retain(|at| now.duration_since(*at) < REVIVAL_WINDOW);
        if times.len() >= REVIVALS {
            return;
        }
        times.push(now);
        eprintln!("plugin-revive: plugin={id} attempt={}", times.len());
        self.reactivate(id);
    }
}

impl Chilpa {
    /// Supplies an application-owned isolation backend before workers load.
    /// I keep the same interface on every platform; an absent backend never
    /// falls back to executing an unrestricted child process.
    pub fn set_worker_launcher(&mut self, launcher: Arc<dyn chilpa_plugin::bridge::WorkerLauncher>) -> bool {
        if self.plugins.is_none() {
            self.plugins = Plugins::new(Self::plugins_dir()).map(|p| Arc::new(Mutex::new(p)));
            self.install_openers();
        }
        let Some(mut plugins) = self.plugins.as_ref().and_then(|plugins| plugins.lock().ok()) else { return false };
        plugins.set_worker_launcher(launcher);
        true
    }

    /// Where the plugins live.
    pub fn plugins_dir() -> PathBuf {
        chilpa_dirs::data().join("plugins")
    }

    /// Loads whatever plugins there are. It is called once at startup.
    ///
    /// It returns how many ended up usable. Having none is the normal case
    /// today and is not a problem.
    pub fn load_plugins(&mut self) -> usize {
        self.invalidate_source_menus();
        if self.plugins.is_none() {
            self.plugins = Plugins::new(Self::plugins_dir()).map(|p| Arc::new(Mutex::new(p)));
            // The player can now be told where something of a plugin is.
            self.install_openers();
        }
        match self.plugins.as_ref().and_then(|p| p.lock().ok()) {
            Some(mut p) => p.load_all(),
            None => {
                self.now.error = chilpa_i18n::t("the plugin sandbox could not be started").into();
                0
            }
        }
    }

    /// The installed plugins, loaded or not.
    pub fn plugins(&self) -> Vec<PluginInfo> {
        self.plugins
            .as_ref()
            .and_then(|p| p.lock().ok())
            .map(|p| p.list())
            .unwrap_or_default()
    }

    /// The user approves what a plugin asks for, and the folder is read
    /// again so it starts running without a restart.
    ///
    /// It returns how many ended up usable, just like loading does.
    pub fn approve_plugin(&mut self, id: &str) -> usize {
        let Some(p) = self.plugins.clone() else {
            self.now.error = chilpa_i18n::t("the plugin sandbox is not started").into();
            return 0;
        };
        let Ok(mut p) = p.lock() else { return 0 };
        if let Err(e) = p.approve(id) {
            self.now.error = chilpa_i18n::t_args("the approval could not be saved: {}", &[&e.to_string()]);
            return 0;
        }
        let count = p.load_all();
        drop(p);
        self.invalidate_source_menus();
        count
    }

    /// Withdraws a plugin's approval: it stops running right now and what
    /// it asks for will be put to the user again.
    pub fn revoke_plugin(&mut self, id: &str) -> usize {
        let Some(p) = self.plugins.clone() else { return 0 };
        let Ok(mut p) = p.lock() else { return 0 };
        if let Err(e) = p.revoke(id) {
            self.now.error = chilpa_i18n::t_args("the approval could not be withdrawn: {}", &[&e.to_string()]);
            return 0;
        }
        // Withdrawing permission deletes the secret: keeping one for a
        // plugin that no longer runs is keeping a token for nothing.
        //
        // Withdrawing permission does not uninstall: the plugin stays in its
        // folder and so does what is its own. What deletes everything is
        // uninstalling, which asks it to clean up first.
        p.forget_credential(id);
        p.forget();
        let count = p.load_all();
        drop(p);
        self.invalidate_source_menus();
        count
    }

    /// Asks a plugin for an operation with the short limits.
    ///
    /// It is what is used when someone is waiting in front of the window,
    /// as when resolving a track to start it playing.
    /// The catalogue sources that work now, with their name.
    pub fn sources(&mut self) -> Vec<(String, String)> {
        self.plugins.as_ref().and_then(|p| p.lock().ok()).map(|p| p.searchers()).unwrap_or_default()
    }

    pub fn call_plugin_short(
        &mut self,
        id: &str,
        operation: &str,
        input: &[u8],
    ) -> Result<Vec<u8>, String> {
        match self.plugins.as_ref().and_then(|p| p.lock().ok()) {
            Some(mut p) => p.call_short(id, operation, input),
            None => Err(chilpa_i18n::t("the plugin sandbox is not started").into()),
        }
    }

    /// Starts connecting an account and answers where the user has to be
    /// sent.
    ///
    /// It does not open the browser: how one is opened is the system's
    /// business, and the caller knows whether there is a window. From here
    /// on the wait runs on another thread, and the result arrives through
    /// [`Chilpa::poll`].
    pub fn connect_account(&mut self, id: &str) -> Result<String, String> {
        let auth = self
            .auth_of(id)
            .ok_or_else(|| chilpa_i18n::t_args("\"{}\" does not say how an account connects", &[id]))?;
        let back = chilpa_auth::Loopback::bind().map_err(|e| e.to_string())?;
        let pkce = chilpa_auth::Pkce::new();
        let url =
            chilpa_auth::authorize_url(&auth, &pkce, &back).ok_or(chilpa_i18n::t("that source does not use OAuth"))?;

        let (tx, rx) = std::sync::mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("chilpa-connect".into())
            .spawn(move || {
                let answer = chilpa_auth::wait_for_code(&back)
                    .map_err(|e| e.to_string())
                    .and_then(|code| {
                        chilpa_auth::exchange(&auth, &code, &pkce, &back, now_unix(), &post)
                            .map_err(|e| e.to_string())
                    });
                let _ = tx.send(answer);
            })
            .ok();

        self.connecting = Some(Connecting { id: id.to_string(), answer: rx, thread });
        Ok(url)
    }

    /// True while waiting for the user to finish in the browser.
    pub fn connecting(&self) -> bool {
        self.connecting.is_some()
    }

    /// Stops waiting. The user closed the browser or changed their mind.
    pub fn cancel_connect(&mut self) {
        self.connecting = None;
    }

    /// Collects whatever the browser answered. `poll` calls it.
    ///
    /// It returns the id of the plugin that ended up connected, if any.
    pub(crate) fn advance_connect(&mut self) -> Option<String> {
        let answer = self.connecting.as_ref()?.answer.try_recv().ok()?;
        let c = self.connecting.take()?;
        match answer {
            Ok(tokens) => {
                let saved = self
                    .plugins
                    .clone()
                    .and_then(|p| p.lock().ok().map(|mut p| p.save_account(&c.id, &Account::OAuth { tokens })));
                match saved {
                    Some(Ok(())) => Some(c.id.clone()),
                    Some(Err(e)) => {
                        self.now.error = chilpa_i18n::t_args("the account connected but could not be saved: {}", &[&e.to_string()]);
                        None
                    }
                    None => None,
                }
            }
            Err(e) => {
                self.now.error = chilpa_i18n::t_args("the account did not connect: {}", &[&e.to_string()]);
                None
            }
        }
    }

    /// How that plugin connects its account, according to its manifest.
    fn auth_of(&self, id: &str) -> Option<chilpa_auth::Auth> {
        let p = self.plugins.as_ref()?.lock().ok()?;
        p.auth_of(id)
    }

    /// Keeps a plugin's credential, encrypted, and says where it ended up.
    ///
    /// The plugin never receives it: the only thing that changes for it is
    /// that its requests go out authenticated (ADR-008).
    pub fn connect_plugin(&mut self, id: &str, credential: &str) -> bool {
        let Some(p) = self.plugins.clone() else {
            self.now.error = chilpa_i18n::t("the plugin sandbox is not started").into();
            return false;
        };
        let Ok(mut p) = p.lock() else { return false };
        match p.set_credential(id, credential) {
            Ok(()) => true,
            Err(e) => {
                self.now.error = chilpa_i18n::t_args("the credential could not be saved: {}", &[&e.to_string()]);
                false
            }
        }
    }

    /// Deletes a plugin's credential. The plugin stays installed.
    pub fn disconnect_plugin(&mut self, id: &str) {
        if let Some(mut p) = self.plugins.as_ref().and_then(|p| p.lock().ok()) {
            p.forget_credential(id);
        }
    }

    /// Where the secrets are being kept on this machine, in words. Empty if
    /// there is nowhere.
    pub fn secrets_location(&self) -> String {
        self.plugins
            .as_ref()
            .and_then(|p| p.lock().ok())
            .and_then(|p| p.secrets_backend())
            .map(|b| b.describe().to_string())
            .unwrap_or_default()
    }

    /// Uninstalls it: asks it to delete what is its own, withdraws its
    /// permission and takes its folder away.
    ///
    /// The order matters. It is told first, **while it can still run**: a
    /// plugin that handles its own account kept its credential somewhere
    /// Chilpa cannot reach, and the only way for it to go is for the plugin
    /// to delete it. Withdrawing permission first would leave it unable to.
    ///
    /// It not answering stops nothing: it is uninstalled anyway and what
    /// was left uncleaned is stated, because keeping quiet would leave the
    /// user believing everything went with it.
    pub fn uninstall_plugin(&mut self, id: &str) -> bool {
        let Some(shared) = self.plugins.clone() else { return false };
        let Ok(mut p) = shared.lock() else { return false };

        let own = p.manages_own_account(id);
        let told = p
            .call_short(id, chilpa_plugin::messages::op::REVOKE, b"{}")
            .is_ok();
        p.forget_credential(id);
        let _ = p.revoke(id);
        p.forget();
        let gone = p.uninstall(id);
        p.load_all();
        drop(p);
        self.invalidate_source_menus();

        if !gone {
            self.now.error = chilpa_i18n::t_args("the folder for \"{}\" could not be deleted", &[id]);
            return false;
        }
        if own && !told {
            self.now.error = chilpa_i18n::t(
                    "I uninstalled it, but it handled its account on its own and did not \
                     answer when I asked it to delete it: your credential may still be \
                     kept wherever it put it",
                )
                .into();
        }
        true
    }

    /// Takes a plugin out of quarantine.
    pub fn reactivate_plugin(&mut self, id: &str) {
        if let Some(mut p) = self.plugins.as_ref().and_then(|p| p.lock().ok()) {
            p.reactivate(id);
        }
    }

    /// Asks a plugin for an operation.
    ///
    /// The error it returns comes already written: it can be the plugin's
    /// ("I did not find that track") or Chilpa's ("it went over the allowed
    /// compute").
    pub fn call_plugin(
        &mut self,
        id: &str,
        operation: &str,
        input: &[u8],
    ) -> Result<Vec<u8>, String> {
        match self.plugins.as_ref().and_then(|p| p.lock().ok()) {
            Some(mut p) => p.call(id, operation, input),
            None => Err(chilpa_i18n::t("the plugin sandbox is not started").into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_with_slashes_or_dot_dots_never_becomes_a_file() {
        let s = ChilpaServices::new(PathBuf::from("/tmp/chilpa-plugins"), None);
        for bad in ["../outside", "with/slash", "..", "WITH.UPPERCASE", "", &"x".repeat(65)] {
            assert!(s.file("com.a.b", bad).is_none(), "\"{bad}\" should not pass");
        }
        assert!(s.file("com.a.b", "token").is_some());
        assert!(s.file("com.a.b", "last-sync.json").is_some());
    }

    #[test]
    fn nor_does_an_id_with_slashes() {
        let s = ChilpaServices::new(PathBuf::from("/tmp/chilpa-plugins"), None);
        assert!(s.file("../another", "token").is_none());
        assert!(s.file("with/slash", "token").is_none());
    }

    #[test]
    fn a_plugin_store_round_trips() {
        let dir = std::env::temp_dir().join("chilpa-plugins-store");
        let _ = std::fs::remove_dir_all(&dir);
        let mut s = ChilpaServices::new(dir.clone(), None);
        assert!(s.store_set("com.a.b", "token", b"abc"));
        assert_eq!(s.store_get("com.a.b", "token").as_deref(), Some(&b"abc"[..]));
        assert_eq!(s.store_get("com.a.b", "other"), None);
        // And each plugin has its own.
        assert_eq!(s.store_get("com.another.plugin", "token"), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_store_is_not_for_keeping_a_catalogue() {
        let dir = std::env::temp_dir().join("chilpa-plugins-grande");
        let _ = std::fs::remove_dir_all(&dir);
        let mut s = ChilpaServices::new(dir.clone(), None);
        assert!(!s.store_set("com.a.b", "huge", &vec![0u8; 2 * 1024 * 1024]));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn no_plugin_folder_means_no_plugins_and_is_not_an_error() {
        let dir = std::env::temp_dir().join("chilpa-plugins-that-does-not-exist");
        let _ = std::fs::remove_dir_all(&dir);
        let mut p = Plugins::new(dir).expect("the engine starts");
        assert_eq!(p.load_all(), 0);
        assert!(p.list().is_empty());
    }

    #[test]
    fn a_folder_without_a_manifest_is_not_listed_as_a_broken_plugin() {
        let dir = std::env::temp_dir().join("chilpa-plugins-sin-manifest");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("junk")).unwrap();
        std::fs::write(dir.join("junk").join("readme.txt"), b"hello").unwrap();
        let mut p = Plugins::new(dir.clone()).unwrap();
        assert_eq!(p.load_all(), 0);
        assert!(p.list().is_empty(), "it is not a broken plugin: it is not a plugin");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_broken_plugin_is_listed_with_its_reason() {
        let dir = std::env::temp_dir().join("chilpa-plugins-broken");
        let _ = std::fs::remove_dir_all(&dir);
        let own = dir.join("com.broken.plugin");
        std::fs::create_dir_all(&own).unwrap();
        std::fs::write(
            own.join("manifest.json"),
            r#"{"id":"com.broken.plugin","name":"Broken","version":"1","api_version":99,"kind":"source"}"#,
        )
        .unwrap();
        std::fs::write(own.join("plugin.wasm"), b"").unwrap();

        let mut p = Plugins::new(dir.clone()).unwrap();
        assert_eq!(p.load_all(), 0, "it did not end up usable");
        let list = p.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].state, "quarantined");
        assert!(list[0].problem.contains("version"), "{}", list[0].problem);
        let _ = std::fs::remove_dir_all(dir);
    }
}
