//! Host-owned inspection history, invalidated by package or runtime changes.
use chilpa_plugin::catalog::CatalogEntry;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io::{Read, Write}, path::Path, sync::OnceLock};

pub(crate) struct Evidence {
    records: BTreeMap<String, Value>,
    revision: String,
}

impl Evidence {
    pub fn load(root: &Path, adapter: Option<String>) -> Self {
        static HOST: OnceLock<String> = OnceLock::new();
        let host = HOST.get_or_init(|| {
            // The build stamp avoids reading a large debug executable at startup.
            std::env::current_exe().ok().and_then(|path| std::fs::metadata(path).ok())
                .and_then(|metadata| Some(format!("{}:{:?}", metadata.len(), metadata.modified().ok()?)))
                .map(|stamp| format!("{:x}", Sha256::digest(stamp))).unwrap_or_default()
        });
        let revision = adapter.map(|adapter| format!("{host}:{adapter}")).unwrap_or_default();
        let records = std::fs::File::open(root.join("inspections.json")).ok().and_then(|file| {
            let mut bytes = Vec::new();
            file.take(4 * 1024 * 1024 + 1).read_to_end(&mut bytes).ok()?;
            if bytes.len() > 4 * 1024 * 1024 { return None; }
            serde_json::from_slice(&bytes).ok()
        }).unwrap_or_default();
        Self { records, revision }
    }
    fn key(repository: &str, entry: &CatalogEntry) -> String {
        format!("{:x}", Sha256::digest(format!("{repository}\0{}", entry.id)))
    }
    pub fn get(&self, repository: &str, entry: &CatalogEntry) -> Option<&Value> {
        let record = self.records.get(&Self::key(repository, entry))?;
        (!self.revision.is_empty() && record["revision"] == self.revision
            && record["digest"] == entry.digest.value).then_some(record)
    }
    pub fn record(&mut self, root: &Path, repository: &str, entry: &CatalogEntry, result: &Result<crate::PluginInspection, String>) -> Result<(), String> {
        if self.records.len() >= 5000 { self.records.clear(); }
        self.records.insert(Self::key(repository, entry), json!({"revision":self.revision,
            "digest":entry.digest.value,"valid":result.is_ok(),"problem":result.as_ref().err()}));
        let bytes = serde_json::to_vec(&self.records).map_err(|_| "The inspection history could not be encoded.")?;
        let save = || -> std::io::Result<()> {
            let mut file = tempfile::NamedTempFile::new_in(root)?;
            file.write_all(&bytes)?;
            file.as_file().sync_all()?;
            file.persist(root.join("inspections.json")).map_err(|e| e.error)?;
            Ok(())
        };
        save().map_err(|_| "The inspection history could not be saved.".into())
    }
}

/// Only host operation names and timings enter this bounded diagnostic log.
pub(crate) fn log_event(root: &Path, action: &str, success: bool, elapsed: std::time::Duration) {
    let action = match action {
        "add" | "remove_repository" | "refresh_repository" | "prepare" | "install" | "enable" | "disable"
        | "grant_network" | "icon" | "tint" | "library_visibility" | "restart" | "settings" | "submit" | "tick" | "filter" | "refresh" => action,
        _ => "unknown",
    };
    let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    let line = format!("{timestamp} plugin-center: operation={action} success={success} elapsed_ms={}\n", elapsed.as_millis());
    eprintln!("{}", line.trim_end());
    let path = root.join("events.log");
    if std::fs::metadata(&path).is_ok_and(|metadata| metadata.len() >= 1024 * 1024) {
        let _ = std::fs::rename(&path, root.join("events.previous.log"));
    }
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = file.write_all(line.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inspection_history_requires_the_same_package_and_runtime() {
        let root = tempfile::tempdir().unwrap();
        let mut entry: CatalogEntry = serde_json::from_value(json!({"id":"guest","name":"Guest","version":"1",
            "description":"","author":"","repository":"","package":"https://example.org/package.zip",
            "digest":{"algorithm":"sha256","value":"abc"},"adapter":"perl","problem":""})).unwrap();
        let mut evidence = Evidence::load(root.path(), Some("adapter-a".into()));
        evidence.record(root.path(), "repository", &entry, &Err("Initialization failed.".into())).unwrap();
        let restored = Evidence::load(root.path(), Some("adapter-a".into()));
        assert_eq!(restored.get("repository", &entry).unwrap()["valid"], false);
        assert!(restored.get("another-repository", &entry).is_none());
        assert!(Evidence::load(root.path(), Some("adapter-b".into())).get("repository", &entry).is_none());
        assert!(Evidence::load(root.path(), None).get("repository", &entry).is_none());
        entry.digest.value = "changed".into();
        assert!(restored.get("repository", &entry).is_none());
    }
}
