//! Trusted visual plugins run in the host, with a separate explicit permission.
use chilpa_plugin::{Capability, Manifest};
use std::path::{Path, PathBuf};

pub(crate) fn resolve(root: &Path, manifest: &Manifest, approved: bool, enabled: bool) -> Option<PathBuf> {
    if !approved || !enabled || !manifest.has(&Capability::RegisterPanel) { return None; }
    let Capability::TrustedUi { entry } = manifest.capabilities.iter().find(|c| matches!(c, Capability::TrustedUi { .. }))? else { return None; };
    // Validate again because a registry adapter may construct a Manifest directly.
    Manifest::parse_for_runtime(&serde_json::to_string(manifest).ok()?, manifest.runtime).ok()?;
    let root = root.canonicalize().ok()?;
    let package = root.join(&manifest.id).canonicalize().ok()?;
    if !package.starts_with(&root) { return None; }
    let path = package.join(entry).canonicalize().ok()?;
    let metadata = path.metadata().ok()?;
    (path.starts_with(&package) && metadata.is_file() && metadata.len() <= 1024 * 1024).then_some(path)
}

impl crate::Chilpa {
    /// A trusted visual entry, only while its approved plugin is running.
    pub fn plugin_display(&self, id: &str) -> Option<(PathBuf, u64)> {
        // Keep the last approved descriptor across transient registry contention.
        // Source-menu invalidation clears it on disable, uninstall and replacement.
        self.sources.displays.get(id).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(root: &Path) -> Manifest {
        std::fs::create_dir_all(root.join("app.test.display/qml")).unwrap();
        std::fs::write(root.join("app.test.display/qml/Main.qml"), "import QtQuick\nItem {}").unwrap();
        Manifest::parse_for_runtime(r#"{"id":"app.test.display","name":"Display","version":"1","api_version":1,"kind":"metadata","runtime":"process","capabilities":[{"type":"register_panel"},{"type":"trusted_ui","entry":"qml/Main.qml"}]}"#, chilpa_plugin::Runtime::Process).unwrap()
    }
    #[test]
    fn approval_running_state_and_the_exact_entry_are_required() {
        let root = tempfile::tempdir().unwrap();
        let mut manifest = fixture(root.path());
        assert!(resolve(root.path(), &manifest, false, true).is_none());
        assert!(resolve(root.path(), &manifest, true, false).is_none());
        assert!(resolve(root.path(), &manifest, true, true).unwrap().ends_with("qml/Main.qml"));
        let before = manifest.asks_for();
        manifest.capabilities[1] = Capability::TrustedUi { entry: "qml/Other.qml".into() };
        assert!(!manifest.asks_for().beyond(&before).is_empty());
        assert!(resolve(root.path(), &manifest, true, true).is_none());
        manifest.capabilities.pop();
        assert!(resolve(root.path(), &manifest, true, true).is_none());
    }
    #[test]
    fn traversal_and_incompatible_runtime_are_refused() {
        let root = tempfile::tempdir().unwrap();
        let mut manifest = fixture(root.path());
        for entry in ["../Main.qml", "/Main.qml", "qml/../../Main.qml", "qml\\Main.qml", "qml//Main.qml", "qml/Main.js", "https://example.com/Main.qml"] {
            manifest.capabilities[1] = Capability::TrustedUi { entry: entry.into() };
            assert!(resolve(root.path(), &manifest, true, true).is_none(), "{entry}");
        }
        manifest.capabilities[1] = Capability::TrustedUi { entry: "qml/Main.qml".into() };
        manifest.runtime = chilpa_plugin::Runtime::Wasm;
        assert!(resolve(root.path(), &manifest, true, true).is_none());
    }
    #[cfg(unix)]
    #[test]
    fn an_entry_symlink_cannot_escape_the_package() {
        let root = tempfile::tempdir().unwrap();
        let manifest = fixture(root.path());
        let entry = root.path().join("app.test.display/qml/Main.qml");
        std::fs::remove_file(&entry).unwrap();
        std::fs::write(root.path().join("Outside.qml"), "import QtQuick\nItem {}").unwrap();
        std::os::unix::fs::symlink(root.path().join("Outside.qml"), &entry).unwrap();
        assert!(resolve(root.path(), &manifest, true, true).is_none());
    }
}
