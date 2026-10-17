//! One explicit download destination per native plugin, outside its private store.
use std::path::{Path, PathBuf};

fn file(root: &Path, id: &str) -> PathBuf { root.join(".exports").join(format!("{id}.json")) }
fn valid_id(id: &str) -> bool {
    !id.is_empty() && !id.contains("..") && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'-'))
}

pub(crate) fn read(root: &Path, id: &str) -> Option<PathBuf> {
    if !valid_id(id) { return None; }
    let path: String = serde_json::from_slice(&std::fs::read(file(root, id)).ok()?).ok()?;
    let path = PathBuf::from(path);
    path.is_dir().then(|| path.canonicalize().ok()).flatten()
}

pub(crate) fn enabled(root: &Path, id: &str) -> bool {
    if !valid_id(id) { return false; }
    let Ok(bytes) = std::fs::read(root.join(id).join("manifest.json")) else { return false };
    let Ok(text) = std::str::from_utf8(&bytes) else { return false };
    chilpa_plugin::Manifest::parse_for_runtime(text, chilpa_plugin::Runtime::Process)
        .is_ok_and(|manifest| manifest.has(&chilpa_plugin::Capability::FolderWrite))
}

pub(crate) fn set(root: &Path, id: &str, selected: &Path) -> Result<(), String> {
    if !enabled(root, id) { return Err("This plugin cannot write to a selected folder.".into()); }
    let selected = selected.canonicalize().map_err(|_| "Choose an existing folder.")?;
    if !selected.is_dir() { return Err("Choose an existing folder.".into()); }
    if selected.parent().is_none() || [chilpa_dirs::config(), chilpa_dirs::data(), chilpa_dirs::cache()]
        .iter().any(|private| private.starts_with(&selected)) {
        return Err("Choose a music folder instead of a system or application folder.".into());
    }
    let parent = root.join(".exports");
    std::fs::create_dir_all(&parent).map_err(|_| "The destination could not be saved.")?;
    let value = serde_json::to_vec(&selected.to_string_lossy().to_string()).map_err(|_| "Invalid destination.")?;
    std::fs::write(file(root, id), value).map_err(|_| "The destination could not be saved.".into())
}

pub(crate) fn clear(root: &Path, id: &str) {
    if valid_id(id) { let _ = std::fs::remove_file(file(root, id)); }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plugin_needs_the_folder_permission_and_a_selected_directory() {
        let root = tempfile::tempdir().unwrap();
        let plugin = root.path().join("app.example.download");
        std::fs::create_dir(&plugin).unwrap();
        let destination = root.path().join("music");
        std::fs::create_dir(&destination).unwrap();
        let manifest = |capability| format!(r#"{{"id":"app.example.download","name":"Download","version":"1","api_version":1,"kind":"source","runtime":"process","capabilities":[{{"type":"{capability}"}}]}}"#);
        std::fs::write(plugin.join("manifest.json"), manifest("storage")).unwrap();
        assert!(set(root.path(), "app.example.download", &destination).is_err());
        std::fs::write(plugin.join("manifest.json"), manifest("folder_write")).unwrap();
        assert!(set(root.path(), "app.example.download", root.path().join("missing").as_path()).is_err());
        set(root.path(), "app.example.download", &destination).unwrap();
        assert_eq!(read(root.path(), "app.example.download"), Some(destination.canonicalize().unwrap()));
    }
}
