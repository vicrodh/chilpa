//! The native catalog and user-selected compatible plugin catalogs.

use chilpa_plugin::catalog::{read_catalog, repository_url, CatalogEntry, MAX_CATALOG_BYTES};
use serde::{Deserialize, Serialize};
use std::{io::{Read, Write}, path::{Path, PathBuf}, time::Duration};

const MAX_REPOSITORIES: usize = 128;
const MAX_CACHE: usize = 4 * 1024 * 1024;
const NATIVE_CATALOG: &str = "https://raw.githubusercontent.com/vicrodh/chilpa-plugins/main/index.json";
/// The catalog of native plugins that others publish, offered in every
/// profile beside the ones the listener adds. It is not saved in the list.
/// Tests point it at a closed local port: they never reach the network.
#[cfg(not(test))]
const THIRD_PARTY_CATALOG: &str = "https://raw.githubusercontent.com/vicrodh/chilpa-3rd-party-plugins/main/index.json";
#[cfg(test)]
const THIRD_PARTY_CATALOG: &str = "http://127.0.0.1:9/index.json";
pub(crate) const THIRD_PARTY: &str = "third-party";
const BUNDLED_CATALOG: &[u8] = include_bytes!("../../../packaging/plugins/index.json");

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginRepository {
    pub id: String,
    pub url: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    #[serde(default = "default_native_catalog")] native_catalog: String,
    #[serde(default)] compatible: Vec<PluginRepository>,
}

fn default_native_catalog() -> String { NATIVE_CATALOG.into() }

impl Default for Saved {
    fn default() -> Self { Self { native_catalog: default_native_catalog(), compatible: Vec::new() } }
}

pub struct PluginRepositories {
    root: PathBuf,
    saved: Saved,
}

impl PluginRepositories {
    pub fn load(root: PathBuf) -> Result<Self, String> {
        let path = root.join("repositories.json");
        let mut saved = match read_bounded(&path, MAX_CATALOG_BYTES) {
            Ok(bytes) => serde_json::from_slice::<Saved>(&bytes).map_err(|_| "The plugin repository list is invalid.")?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Saved::default(),
            Err(_) => return Err("The plugin repository list could not be read.".into()),
        };
        if saved.native_catalog.is_empty() { saved.native_catalog = default_native_catalog(); }
        if saved.compatible.len() > MAX_REPOSITORIES { return Err("Too many plugin repositories.".into()); }
        let mut ids = std::collections::BTreeSet::new();
        for repository in &saved.compatible {
            if repository.id.len() != 16 || !repository.id.bytes().all(|byte| byte.is_ascii_hexdigit())
                || !ids.insert(&repository.id) || repository_url(&repository.url).is_err() {
                return Err("The plugin repository list is invalid.".into());
            }
        }
        if !saved.native_catalog.is_empty() && repository_url(&saved.native_catalog).is_err() {
            return Err("The native plugin catalog URL is invalid.".into());
        }
        Ok(Self { root, saved })
    }

    pub fn compatible(&self) -> &[PluginRepository] { &self.saved.compatible }
    /// The third-party catalog's address, empty when the listener added it
    /// by hand and it is offered as one of theirs.
    pub fn third_party_catalog(&self) -> &str {
        if self.saved.compatible.iter().any(|repository| repository.url == THIRD_PARTY_CATALOG) { "" } else { THIRD_PARTY_CATALOG }
    }
    /// Every repository whose plugins are offered: the third-party catalog,
    /// unless the listener added it by hand, and the ones added.
    pub fn sources(&self) -> Vec<PluginRepository> {
        let offered = (!self.third_party_catalog().is_empty())
            .then(|| PluginRepository { id: THIRD_PARTY.into(), url: THIRD_PARTY_CATALOG.into() });
        offered.into_iter().chain(self.saved.compatible.iter().cloned()).collect()
    }
    pub fn native_catalog(&self) -> &str { &self.saved.native_catalog }
    /// The plugins the third-party catalog offers, however it is offered. A
    /// plugin with one of these names is a third party's, installed from
    /// the catalog or from a ZIP of it.
    pub fn third_party_ids(&self) -> std::collections::HashSet<String> {
        let id = self.saved.compatible.iter().find(|repository| repository.url == THIRD_PARTY_CATALOG).map_or(THIRD_PARTY, |repository| repository.id.as_str());
        self.cached(id).unwrap_or_default().into_iter().map(|entry| entry.id).collect()
    }

    /// A local index may be loaded for offline development. Compatible
    /// repositories cannot select this path or import native entries.
    pub fn load_native_file(&self, path: &Path, language: &str) -> Result<Vec<CatalogEntry>, String> {
        let bytes = read_bounded(path, MAX_CATALOG_BYTES).map_err(|_| "The native plugin catalog could not be read.")?;
        let document: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| "The native plugin catalog must be JSON.")?;
        if document.get("version").and_then(serde_json::Value::as_u64) != Some(1) {
            return Err("The native plugin catalog version is unsupported.".into());
        }
        let entries = read_catalog(&bytes, language).map_err(|_| "The native plugin catalog is invalid.")?;
        self.cache("native", &entries)?;
        Ok(entries)
    }

    pub fn set_native_catalog(&mut self, url: &str) -> Result<(), String> {
        let url = repository_url(url).map_err(|_| "Enter an HTTP or HTTPS catalog URL without credentials.")?;
        let previous = std::mem::replace(&mut self.saved.native_catalog, url);
        if let Err(error) = self.persist() { self.saved.native_catalog = previous; return Err(error); }
        Ok(())
    }

    pub fn add(&mut self, url: &str) -> Result<PluginRepository, String> {
        let url = repository_url(url).map_err(|_| "Enter an HTTP or HTTPS catalog URL without credentials.")?;
        if let Some(existing) = self.saved.compatible.iter().find(|repository| repository.url == url) {
            return Ok(existing.clone());
        }
        if self.saved.compatible.len() >= MAX_REPOSITORIES { return Err("Too many plugin repositories.".into()); }
        let repository = PluginRepository { id: format!("{:016x}", rand::random::<u64>()), url };
        if self.saved.compatible.iter().any(|existing| existing.id == repository.id) {
            return Err("A repository identity could not be allocated.".into());
        }
        self.saved.compatible.push(repository.clone());
        if let Err(error) = self.persist() { self.saved.compatible.pop(); return Err(error); }
        Ok(repository)
    }

    pub fn remove(&mut self, id: &str) -> Result<(), String> {
        let index = self.saved.compatible.iter().position(|repository| repository.id == id)
            .ok_or("The plugin repository is not in your list.")?;
        let removed = self.saved.compatible.remove(index);
        if let Err(error) = self.persist() { self.saved.compatible.insert(index, removed); return Err(error); }
        // Installed packages and their approvals have an independent lifetime.
        // Cached entries from a removed source are no longer exposed.
        let _ = std::fs::remove_file(self.cache_path(id));
        Ok(())
    }

    pub fn cached(&self, id: &str) -> Result<Vec<CatalogEntry>, String> {
        if id != "native" { self.source(id)?; }
        match read_bounded(&self.cache_path(id), MAX_CACHE) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| "The cached catalog is invalid.".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if id == "native" && self.saved.native_catalog == NATIVE_CATALOG {
                    read_catalog(BUNDLED_CATALOG, "EN").map_err(|_| "The bundled catalog is invalid.".into())
                } else { Ok(Vec::new()) }
            },
            Err(_) => Err("The cached catalog could not be read.".into()),
        }
    }

    /// Called on a worker thread after an explicit add or refresh. Catalog
    /// content cannot add more repositories, grant permissions or run code.
    pub fn refresh(&self, id: &str, language: &str) -> Result<Vec<CatalogEntry>, String> {
        if id == "native" && self.saved.native_catalog.is_empty() { return self.cached(id); }
        let source = self.source(id)?;
        let bytes = fetch(source, MAX_CATALOG_BYTES)?;
        let entries = read_catalog(&bytes, language).map_err(|_| "The repository does not contain a supported plugin catalog.")?;
        self.cache(id, &entries)?;
        Ok(entries)
    }

    fn cache(&self, id: &str, entries: &[CatalogEntry]) -> Result<(), String> {
        let bytes = serde_json::to_vec(entries).map_err(|_| "The catalog could not be saved.")?;
        if bytes.len() > MAX_CACHE { return Err("The catalog is too large.".into()); }
        atomic_write(&self.cache_path(id), &bytes).map_err(|_| "The catalog could not be saved.")?;
        Ok(())
    }

    /// A selection must still exist in a configured repository. Downloading
    /// and staging never enables the plugin or modifies an installation.
    pub fn stage(&self, repository: &str, plugin: &str) -> Result<crate::StagedPackage, String> {
        let entries = self.cached(repository)?;
        let mut matches = entries.iter().filter(|entry| entry.id == plugin);
        let entry = matches.next().ok_or("The plugin is no longer in this catalog.")?;
        if matches.next().is_some() { return Err("The catalog contains an ambiguous plugin identity.".into()); }
        if !entry.problem.is_empty() { return Err(entry.problem.clone()); }
        let address = repository_url(&entry.package).map_err(|_| "The package URL is invalid.")?;
        let bytes = fetch(&address, crate::plugin_package::MAX_PACKAGE_BYTES)?;
        crate::StagedPackage::unpack(&self.root.join("staging"), &bytes, &entry.digest)
    }

    fn source(&self, id: &str) -> Result<&str, String> {
        if id == "native" && !self.saved.native_catalog.is_empty() { return Ok(&self.saved.native_catalog); }
        if id == THIRD_PARTY { return Ok(THIRD_PARTY_CATALOG); }
        self.saved.compatible.iter().find(|repository| repository.id == id).map(|repository| repository.url.as_str())
            .ok_or_else(|| "The plugin repository is not configured.".into())
    }
    fn cache_path(&self, id: &str) -> PathBuf {
        self.root.join("catalogs").join(format!("{id}-v2-{}-{}.json", std::env::consts::OS, std::env::consts::ARCH))
    }
    fn persist(&self) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(&self.saved).map_err(|_| "The repository list could not be saved.")?;
        atomic_write(&self.root.join("repositories.json"), &bytes).map_err(|_| "The repository list could not be saved.".into())
    }
}

fn read_bounded(path: &Path, maximum: usize) -> std::io::Result<Vec<u8>> {
    let file = std::fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.take(maximum as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > maximum { return Err(std::io::ErrorKind::InvalidData.into()); }
    Ok(bytes)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().ok_or(std::io::ErrorKind::InvalidInput)?;
    std::fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}

fn fetch(source: &str, maximum: usize) -> Result<Vec<u8>, String> {
    let mut at = source.to_string();
    for _ in 0..6 {
        let response = chilpa_core::call_server(&chilpa_core::Call { method: "GET", url: &at,
            headers: &[], body: None, timeout: Duration::from_secs(15), max_bytes: maximum })
            .map_err(|_| "The plugin repository could not be reached.")?;
        if (300..400).contains(&response.status) {
            let location = response.headers.get("location").ok_or("The catalog redirect is invalid.")?;
            at = chilpa_plugin::catalog::redirect_url(&at, location).map_err(|_| "The catalog redirect is invalid.")?;
        } else if response.status == 200 {
            return Ok(response.body);
        } else { return Err("The plugin repository did not return a catalog.".into()); }
    }
    Err("The plugin repository redirected too many times.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_official_catalog_is_available_offline_on_new_and_existing_profiles() {
        let root = tempfile::tempdir().unwrap();
        for saved in [None, Some(br#"{"compatible":[]}"#.as_slice()), Some(br#"{"native_catalog":"","compatible":[]}"#.as_slice())] {
            if let Some(saved) = saved { std::fs::write(root.path().join("repositories.json"), saved).unwrap(); }
            let repositories = PluginRepositories::load(root.path().into()).unwrap();
            assert_eq!(repositories.native_catalog(), NATIVE_CATALOG);
            let entries = repositories.cached("native").unwrap();
            let workers = if cfg!(all(target_os = "linux", target_arch = "x86_64")) { 2 }
                else if cfg!(all(target_os = "macos", target_arch = "aarch64")) { 1 } else { 0 };
            assert_eq!(entries.len(), 3 + workers);
            assert!(entries.iter().all(|entry| entry.problem.is_empty()));
            assert!(repositories.compatible().is_empty());
        }
    }

    #[test]
    fn the_third_party_catalog_is_offered_once_and_never_saved() {
        let root = tempfile::tempdir().unwrap();
        let mut repositories = PluginRepositories::load(root.path().into()).unwrap();
        assert_eq!(repositories.sources(), [PluginRepository { id: THIRD_PARTY.into(), url: THIRD_PARTY_CATALOG.into() }]);
        assert!(repositories.cached(THIRD_PARTY).unwrap().is_empty(), "nothing is offered before it is read");
        assert!(repositories.remove(THIRD_PARTY).is_err(), "it cannot be removed");
        let added = repositories.add("https://example.org/repo.xml").unwrap();
        assert_eq!(repositories.sources().len(), 2);
        assert_eq!(repositories.compatible(), &[added]);
        repositories.add(THIRD_PARTY_CATALOG).unwrap();
        let sources = repositories.sources();
        assert_eq!(sources.len(), 2, "added by hand, it is offered once");
        assert!(sources.iter().all(|repository| repository.id != THIRD_PARTY));
        assert_eq!(repositories.third_party_catalog(), "");
    }

    #[test]
    fn the_third_party_catalog_names_its_plugins_however_it_is_offered() {
        let root = tempfile::tempdir().unwrap();
        let mut repositories = PluginRepositories::load(root.path().into()).unwrap();
        let entry = |id: &str| CatalogEntry { id: id.into(), name: id.into(), version: "1.0.0".into(), description: String::new(),
            category: String::new(), author: String::new(), repository: String::new(), package: "https://example.org/one.zip".into(),
            digest: chilpa_plugin::catalog::Digest { algorithm: "sha256".into(), value: "00".repeat(32) }, adapter: "native".into(), problem: String::new() };
        assert!(repositories.third_party_ids().is_empty(), "nothing is named before it is read");
        repositories.cache(THIRD_PARTY, &[entry("app.example.stream")]).unwrap();
        assert!(repositories.third_party_ids().contains("app.example.stream"));
        let added = repositories.add(THIRD_PARTY_CATALOG).unwrap();
        repositories.cache(&added.id, &[entry("app.example.other")]).unwrap();
        assert_eq!(repositories.third_party_ids(), ["app.example.other".to_string()].into(), "added by hand, its own copy names them");
    }

    #[test]
    fn the_default_does_not_replace_a_custom_catalog_or_compatible_repositories() {
        let root = tempfile::tempdir().unwrap();
        let saved = br#"{"native_catalog":"https://native.example/index.json","compatible":[{"id":"0123456789abcdef","url":"https://compatible.example/index.json"}]}"#;
        std::fs::write(root.path().join("repositories.json"), saved).unwrap();
        let repositories = PluginRepositories::load(root.path().into()).unwrap();
        assert_eq!(repositories.native_catalog(), "https://native.example/index.json");
        assert_eq!(repositories.compatible()[0].url, "https://compatible.example/index.json");
        assert!(repositories.cached("native").unwrap().is_empty());
        assert_eq!(std::fs::read(root.path().join("repositories.json")).unwrap(), saved);
    }

    #[test]
    fn a_local_native_index_keeps_the_public_default_separate_from_compatible_repositories() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("index.json");
        std::fs::write(&path, br#"{"version":1,"plugins":[]}"#).unwrap();
        let repositories = PluginRepositories::load(root.path().join("repositories")).unwrap();
        assert!(repositories.load_native_file(&path, "EN").unwrap().is_empty());
        assert_eq!(repositories.native_catalog(), NATIVE_CATALOG);
        assert!(repositories.compatible().is_empty());
        std::fs::write(&path, b"<extensions><plugins/></extensions>").unwrap();
        assert!(repositories.load_native_file(&path, "EN").is_err());
    }

    #[test]
    fn compatible_repositories_are_empty_until_explicitly_added_and_remain_separate_from_native() {
        let root = tempfile::tempdir().unwrap();
        let mut repositories = PluginRepositories::load(root.path().into()).unwrap();
        assert!(repositories.compatible().is_empty());
        repositories.set_native_catalog("https://native.example/index.json").unwrap();
        assert!(repositories.compatible().is_empty());
        let added = repositories.add("https://EXAMPLE.org/repo.xml#section").unwrap();
        assert_eq!(repositories.add("https://example.org/repo.xml").unwrap(), added);
        let mut restored = PluginRepositories::load(root.path().into()).unwrap();
        assert_eq!(restored.compatible(), &[added.clone()]);
        restored.remove(&added.id).unwrap();
        assert!(restored.cached(&added.id).is_err());
        assert!(PluginRepositories::load(root.path().into()).unwrap().compatible().is_empty());
        assert_eq!(restored.native_catalog(), "https://native.example/index.json");
    }

    #[test]
    fn an_added_native_repository_does_not_cache_or_stage_an_unsupported_worker() {
        let root = tempfile::tempdir().unwrap();
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let mut repositories = PluginRepositories::load(root.path().into()).unwrap();
        let added = repositories.add(&format!("http://{}/index.json", server.server_addr())).unwrap();
        let portable = serde_json::json!({"id":"app.example.portable","name":"Portable",
            "repository":"https://example.org/source","package":"https://example.org/portable.zip",
            "sha256":"a".repeat(64),"api":{"min":1,"max":1}});
        let mut worker = portable.clone();
        worker["id"] = "app.example.worker".into();
        worker["packages"] = serde_json::json!({"unsupported-unknown":{
            "package":"https://example.org/worker.zip","sha256":"b".repeat(64)}});
        let document = serde_json::json!({"version":1,"plugins":[worker,portable]}).to_string();
        let thread = std::thread::spawn(move || {
            let request = server.recv_timeout(Duration::from_secs(3)).unwrap().unwrap();
            request.respond(tiny_http::Response::from_string(document)).unwrap();
        });
        let entries = repositories.refresh(&added.id, "EN").unwrap();
        thread.join().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "app.example.portable");
        assert_eq!(repositories.cached(&added.id).unwrap(), entries);
        assert!(repositories.stage(&added.id, "app.example.worker").is_err());
    }

    #[test]
    fn a_catalog_cannot_add_its_own_repository_sources() {
        let root = tempfile::tempdir().unwrap();
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let mut repositories = PluginRepositories::load(root.path().into()).unwrap();
        let added = repositories.add(&format!("http://{}/extensions.xml", server.server_addr())).unwrap();
        let thread = std::thread::spawn(move || {
            let request = server.recv_timeout(Duration::from_secs(3)).unwrap().unwrap();
            request.respond(tiny_http::Response::from_string("<extensions><plugins/><repos><repo>https://elsewhere.example/repo.xml</repo></repos></extensions>")).unwrap();
        });
        assert!(repositories.refresh(&added.id, "EN").unwrap().is_empty());
        thread.join().unwrap();
        assert_eq!(repositories.compatible().len(), 1);
        assert!(repositories.cached(&added.id).unwrap().is_empty());
    }
}
