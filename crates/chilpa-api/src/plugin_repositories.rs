//! User-selected plugin catalogs. There are no implicit compatible sources.

use chilpa_plugin::catalog::{read_catalog, repository_url, CatalogEntry, MAX_CATALOG_BYTES};
use serde::{Deserialize, Serialize};
use std::{io::{Read, Write}, path::{Path, PathBuf}, time::Duration};

const MAX_REPOSITORIES: usize = 128;
const MAX_CACHE: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginRepository {
    pub id: String,
    pub url: String,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    #[serde(default)] native_catalog: String,
    #[serde(default)] compatible: Vec<PluginRepository>,
}

pub struct PluginRepositories {
    root: PathBuf,
    saved: Saved,
}

impl PluginRepositories {
    pub fn load(root: PathBuf) -> Result<Self, String> {
        let path = root.join("repositories.json");
        let saved = match read_bounded(&path, MAX_CATALOG_BYTES) {
            Ok(bytes) => serde_json::from_slice::<Saved>(&bytes).map_err(|_| "The plugin repository list is invalid.")?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Saved::default(),
            Err(_) => return Err("The plugin repository list could not be read.".into()),
        };
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
    pub fn native_catalog(&self) -> &str { &self.saved.native_catalog }

    /// Until the public catalog is deployed, the application supplies its
    /// native index as a local file. Compatible repositories cannot select
    /// this path or import entries into the native section.
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
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
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
        self.saved.compatible.iter().find(|repository| repository.id == id).map(|repository| repository.url.as_str())
            .ok_or_else(|| "The plugin repository is not configured.".into())
    }
    fn cache_path(&self, id: &str) -> PathBuf { self.root.join("catalogs").join(format!("{id}.json")) }
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
    fn a_local_native_index_does_not_require_a_public_url_or_add_compatible_repositories() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("index.json");
        std::fs::write(&path, br#"{"version":1,"plugins":[]}"#).unwrap();
        let repositories = PluginRepositories::load(root.path().join("repositories")).unwrap();
        assert!(repositories.load_native_file(&path, "EN").unwrap().is_empty());
        assert!(repositories.refresh("native", "EN").unwrap().is_empty());
        assert!(repositories.native_catalog().is_empty());
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
