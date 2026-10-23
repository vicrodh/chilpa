//! One bounded update inbox, persisted independently of plugin account data.
use crate::{plugins::Plugins, PluginRepositories};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use chilpa_plugin::catalog::CatalogEntry;
use std::{collections::{BTreeMap, BTreeSet}, io::{Read, Write}, path::Path, sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}}};

const MAX_NOTICES: usize = 128;
const CHECK_INTERVAL: u64 = 86400;
/// A catalog listing more than this is not watched for new plugins: its ids
/// would crowd the inbox file.
const MAX_WATCHED: usize = 1024;

#[derive(Clone, Default, Serialize, Deserialize)]
pub(crate) struct Inbox {
    #[serde(default)] automatic: bool,
    #[serde(default)] checked: u64,
    #[serde(default)] notices: Vec<Notice>,
    #[serde(default)] error: String,
    #[serde(default)] seen_app: String,
    /// What each catalog Chilpa offers listed at its last reading.
    #[serde(default)] catalogs: BTreeMap<String, BTreeSet<String>>,
    #[serde(skip)] revision: u64,
}
#[derive(Clone, Serialize, Deserialize)]
struct Notice {
    id: String, repository: String, name: String, version: String, adapter: String,
    phase: String, message: String, read: bool,
    #[serde(default)] requested: bool,
    #[serde(default)] third_party: bool,
}

fn now() -> u64 { std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() }
impl Inbox {
    pub(crate) fn load(root: &Path) -> Self {
        let mut bytes = Vec::new();
        let mut state: Self = std::fs::File::open(root.join("updates.json")).ok()
            .and_then(|file| file.take(256 * 1024 + 1).read_to_end(&mut bytes).ok())
            .filter(|_| bytes.len() <= 256 * 1024).and_then(|_| serde_json::from_slice(&bytes).ok()).unwrap_or_default();
        state.notices.truncate(MAX_NOTICES);
        state.revision = 1;
        state
    }
    fn save(&mut self, root: &Path) -> Result<(), String> {
        std::fs::create_dir_all(root).map_err(|_| "The plugin notification folder could not be created.")?;
        let mut file = tempfile::NamedTempFile::new_in(root).map_err(|_| "The plugin notification settings could not be saved.")?;
        file.write_all(&serde_json::to_vec(self).map_err(|_| "The plugin notification settings are invalid.")?)
            .and_then(|_| file.as_file().sync_all()).map_err(|_| "The plugin notification settings could not be saved.")?;
        file.persist(root.join("updates.json")).map_err(|_| "The plugin notification settings could not be saved.")?;
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }
    pub(crate) fn snapshot(&self, revision: u64) -> Option<(u64, String)> {
        (revision != self.revision).then(|| (self.revision, json!({"automatic":self.automatic,
            "checked":self.checked,"error":self.error,"seen_app":self.seen_app,"unread":self.notices.iter().filter(|notice|!notice.read).count(),"items":self.notices}).to_string()))
    }
    pub(crate) fn due(&self) -> bool { now().saturating_sub(self.checked) >= CHECK_INTERVAL }
    pub(crate) fn revision(&self) -> u64 { self.revision }
    pub(crate) fn action(&mut self, action: &str, input: &Value, root: &Path) -> Result<(), String> {
        match action {
            "updates_automatic" => self.automatic = input["enabled"].as_bool().ok_or("Invalid update preference.")?,
            "updates_read" => { for notice in &mut self.notices { notice.read = true; } }
            "updates_app_read" => self.seen_app = input["identity"].as_str().unwrap_or("").chars().take(128).collect(),
            "updates_apply" => { for notice in &mut self.notices { if matches!(notice.phase.as_str(), "available" | "failed" | "deferred") { notice.requested = true; notice.phase = "deferred".into(); } } }
            _ => return Err("Unknown plugin update operation.".into()),
        }
        self.save(root)
    }
    pub(crate) fn check(&mut self, root: &Path, plugins: &Arc<Mutex<Plugins>>, repositories: &PluginRepositories) -> Result<(), String> {
        if chilpa_core::network::current().offline { return Ok(()); }
        let installed = plugins.lock().map_err(|_| "The plugin host is unavailable.")?.list();
        let installed_ids: Vec<String> = installed.iter().map(|plugin| plugin.id.clone()).collect();
        let mut sources = BTreeSet::new();
        let mut origins = Vec::new();
        for plugin in installed {
            for file in ["source.json", "origin.json"] {
                let origin: Value = std::fs::read(root.join(&plugin.id).join(file)).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default();
                if let Some(repo) = origin["repository"].as_str().filter(|repo| *repo != "local") {
                    sources.insert(repo.to_string());
                    origins.push((plugin.id.clone(), repo.to_string(), if file == "origin.json" { origin["catalog_entry"]["id"].as_str().unwrap_or("").to_string() } else { plugin.id.clone() }, plugin.version.clone()));
                }
            }
        }
        let mut failure = None;
        // The catalogs Chilpa offers on its own are read for new plugins even
        // when nothing installed comes from them; not reaching one of those is
        // not an update failure.
        let third_party = repositories.third_party_id();
        let offered = ["native".to_string(), third_party.clone()];
        for repo in sources.iter().chain(offered.iter().filter(|repo| !sources.contains(*repo))) {
            if let Err(error) = repositories.refresh(repo, "EN") { if sources.contains(repo) { failure = Some(error); } continue; }
            let entries = repositories.cached(repo)?;
            if offered.contains(repo) { self.note_new(repo, &entries, &installed_ids, *repo == third_party); }
            let repo = repo.clone();
            for entry in entries {
                let Some((id, _, _, _)) = origins.iter().find(|(_, source, catalog_id, version)| source == &repo && catalog_id == &entry.id && crate::plugin_updates::newer(&entry.version, version)) else { continue };
                if self.notices.iter().any(|notice| notice.id == *id && notice.repository == repo && notice.version == entry.version) { continue; }
                self.notices.retain(|notice| notice.id != *id);
                if self.notices.len() == MAX_NOTICES { self.notices.remove(0); }
                self.notices.push(Notice { id:id.clone(), repository:repo.clone(), name:entry.name.chars().take(128).collect(), version:entry.version.chars().take(64).collect(), adapter:entry.adapter,
                    phase:"available".into(), message:String::new(), read:false, requested:false, third_party:false });
            }
        }
        self.checked = now();
        self.error = failure.clone().unwrap_or_default();
        self.save(&root.join(".repositories"))?;
        failure.map_or(Ok(()), Err)
    }
    /// Announces what a catalog lists that it did not list before. The first
    /// reading only learns it, so a new profile or an upgrade does not
    /// announce every plugin at once; only what this Chilpa can install and
    /// does not have is announced, and installing it takes the notice away.
    fn note_new(&mut self, catalog: &str, entries: &[CatalogEntry], installed: &[String], third_party: bool) {
        self.notices.retain(|notice| notice.phase != "new" || !installed.contains(&notice.id));
        let listed: BTreeSet<String> = entries.iter().map(|entry| entry.id.clone()).collect();
        if listed.len() > MAX_WATCHED { return; }
        if let Some(known) = self.catalogs.get(catalog) {
            for entry in entries.iter().filter(|entry| !known.contains(&entry.id) && entry.adapter == "native" && entry.problem.is_empty() && !installed.contains(&entry.id)) {
                if self.notices.iter().any(|notice| notice.id == entry.id && notice.phase == "new") { continue; }
                if self.notices.len() == MAX_NOTICES { self.notices.remove(0); }
                self.notices.push(Notice { id:entry.id.clone(), repository:catalog.into(), name:entry.name.chars().take(128).collect(), version:entry.version.chars().take(64).collect(),
                    adapter:entry.adapter.clone(), phase:"new".into(), message:String::new(), read:false, requested:false, third_party });
            }
        }
        self.catalogs.insert(catalog.into(), listed);
    }
    pub(crate) fn apply(&mut self, root: &Path, plugins: &Arc<Mutex<Plugins>>, repositories: &PluginRepositories,
        active: &AtomicBool, state: &mut Value) -> Result<bool, String> {
        if chilpa_core::network::current().offline { return Ok(false); }
        let mut changed = false;
        for notice in self.notices.iter_mut().filter(|n| (self.automatic || n.requested) && matches!(n.phase.as_str(), "available" | "deferred")).take(1) {
            if active.load(Ordering::Acquire) {
                if notice.phase != "deferred" { notice.phase = "deferred".into(); changed = true; }
                continue;
            }
            let result = if notice.adapter == "native" {
                crate::plugin_updates::native(root, plugins, repositories, &json!({"id":notice.id,"repository":notice.repository}), state, true, active)
            } else { update_compatible(root, plugins, repositories, notice, active) };
            match result {
                Ok(()) => {
                    notice.phase = "updated".into(); notice.message.clear(); notice.requested = false;
                    state["form"] = Value::Null; state["settings_loading_id"] = Value::Null; state["settings_id"] = Value::Null;
                }
                Err(error) => {
                    notice.phase = if error.contains("permission review") { "review" } else if error.contains("playback") { "deferred" } else { "failed" }.into();
                    notice.message = error.chars().take(512).collect();
                }
            }
            notice.read = false;
            changed = true;
        }
        if changed { self.save(&root.join(".repositories"))?; }
        Ok(changed)
    }
}

fn update_compatible(root: &Path, plugins: &Arc<Mutex<Plugins>>, repositories: &PluginRepositories, notice: &Notice, active: &AtomicBool) -> Result<(), String> {
    let origin: Value = serde_json::from_slice(&std::fs::read(root.join(&notice.id).join("origin.json")).map_err(|_| "The plugin origin could not be read.")?).map_err(|_| "The plugin origin is invalid.")?;
    let catalog_id = origin["catalog_entry"]["id"].as_str().ok_or("The catalog identity is missing.")?;
    let entry = repositories.cached(&notice.repository)?.into_iter().find(|entry| entry.id == catalog_id && entry.version == notice.version).ok_or("The catalog entry is missing.")?;
    let package = repositories.stage(&notice.repository, catalog_id)?;
    let launcher = plugins.lock().map_err(|_| "The plugin host is unavailable.")?.compatibility_launcher().ok_or("The compatibility runtime is unavailable.")?;
    let inspection = package.inspect(launcher.as_ref(), "EN")?;
    let prepared = package.prepare_extension(&notice.repository, &entry, &inspection, vec![])?;
    if active.load(Ordering::Acquire) { return Err("Waiting for playback to stop.".into()); }
    let mut host = plugins.lock().map_err(|_| "The plugin host is unavailable.")?;
    crate::plugin_updates::compatible(root, &mut host, prepared, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn entry(id: &str, problem: &str) -> chilpa_plugin::catalog::CatalogEntry {
        serde_json::from_value(json!({"id":id,"name":id,"version":"1.0.0","description":"","author":"","repository":"","package":"",
            "digest":{"algorithm":"sha256","value":""},"adapter":"native","problem":problem})).unwrap()
    }
    #[test]
    fn a_catalog_is_learned_first_and_only_later_additions_are_announced() {
        let mut inbox = Inbox::default();
        inbox.note_new("native", &[entry("app.test.a", "")], &[], false);
        inbox.note_new("third-party", &[entry("app.test.outside", "")], &[], true);
        assert!(inbox.notices.is_empty(), "the first reading of a catalog only learns it");
        let mine = ["app.test.mine".to_string()];
        inbox.note_new("native", &[entry("app.test.a", ""), entry("app.test.b", ""), entry("app.test.mine", ""),
            entry("app.test.old", "This plugin requires another version of the plugin API.")], &mine, false);
        let announced: Vec<_> = inbox.notices.iter().map(|n| (n.id.as_str(), n.repository.as_str(), n.phase.as_str(), n.read)).collect();
        assert_eq!(announced, [("app.test.b", "native", "new", false)], "only what this Chilpa can install and does not have");
        inbox.note_new("native", &[entry("app.test.a", ""), entry("app.test.b", "")], &[], false);
        assert_eq!(inbox.notices.len(), 1, "a plugin is announced once");
        inbox.note_new("native", &[entry("app.test.a", ""), entry("app.test.b", "")], &["app.test.b".to_string()], false);
        assert!(inbox.notices.is_empty(), "installing it takes the announcement away");
    }
    #[test]
    fn authorization_and_read_state_survive_restart_without_repeated_snapshots() {
        let dir = tempfile::tempdir().unwrap();
        let mut inbox = Inbox::load(dir.path());
        assert!(!inbox.automatic);
        let (revision, _) = inbox.snapshot(0).unwrap();
        assert!(inbox.snapshot(revision).is_none());
        inbox.action("updates_automatic", &json!({"enabled":true}), dir.path()).unwrap();
        inbox.notices.push(Notice {id:"app.test.one".into(),repository:"native".into(),name:"One".into(),version:"2.0.0".into(),adapter:"native".into(),phase:"available".into(),message:String::new(),read:false,requested:false,third_party:false});
        inbox.action("updates_read", &json!({}), dir.path()).unwrap();
        let loaded = Inbox::load(dir.path());
        assert!(loaded.automatic && loaded.notices[0].read);
        assert_eq!(serde_json::from_str::<Value>(&loaded.snapshot(0).unwrap().1).unwrap()["unread"], 0);
    }
    #[test]
    fn playback_defers_authorized_updates_and_opt_out_leaves_them_available() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("plugins");
        let mut inbox = Inbox::default();
        inbox.automatic = true;
        inbox.notices.push(Notice {id:"app.test.one".into(),repository:"native".into(),name:"One".into(),version:"2.0.0".into(),adapter:"native".into(),phase:"available".into(),message:String::new(),read:false,requested:false,third_party:false});
        let host = Arc::new(Mutex::new(Plugins::new(root.clone()).unwrap()));
        let repositories = PluginRepositories::load(root.join(".repositories")).unwrap();
        let active = AtomicBool::new(true);
        assert!(inbox.apply(&root, &host, &repositories, &active, &mut json!({})).unwrap());
        assert_eq!(inbox.notices[0].phase, "deferred");
        let revision = inbox.revision;
        assert!(!inbox.apply(&root, &host, &repositories, &active, &mut json!({})).unwrap());
        assert_eq!(inbox.revision, revision, "an idle maintenance pass must not rewrite or republish the inbox");
        inbox.automatic = false;
        active.store(false, Ordering::Release);
        assert!(!inbox.apply(&root, &host, &repositories, &active, &mut json!({})).unwrap());
        assert!(!root.join("app.test.one").exists(), "deferred updates never execute before authorization and idle playback");
    }
}
