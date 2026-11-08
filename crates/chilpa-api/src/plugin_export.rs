//! One explicit download destination per native plugin, outside its private store.
//!
//! A destination is never one of Chilpa's own folders, inside one or above
//! one: not when it is chosen, and not when the plugin starts, when the path
//! recorded is resolved again and a folder above it may have become a link.
use std::path::{Path, PathBuf};

fn file(root: &Path, id: &str) -> PathBuf { root.join(".exports").join(format!("{id}.json")) }
fn valid_id(id: &str) -> bool {
    !id.is_empty() && !id.contains("..") && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'-'))
}

pub(crate) fn read(root: &Path, id: &str) -> Option<PathBuf> {
    read_with(root, id, &private())
}

/// Chilpa's own folders, canonical where they exist.
pub(crate) fn private() -> Vec<PathBuf> {
    [chilpa_dirs::config(), chilpa_dirs::data(), chilpa_dirs::cache()].into_iter()
        .map(|folder| folder.canonicalize().unwrap_or(folder)).collect()
}

/// Whether `folder` is one of `private`, lies inside one or holds one.
fn is_private(folder: &Path, private: &[PathBuf]) -> bool {
    private.iter().any(|own| own.starts_with(folder) || folder.starts_with(own))
}

fn read_with(root: &Path, id: &str, private: &[PathBuf]) -> Option<PathBuf> {
    if !valid_id(id) { return None; }
    let path: String = serde_json::from_slice(&std::fs::read(file(root, id)).ok()?).ok()?;
    let path = PathBuf::from(path);
    let folder = std::fs::symlink_metadata(&path).ok()?.file_type().is_dir().then(|| path.canonicalize().ok()).flatten()?;
    if is_private(&folder, private) {
        eprintln!("plugin-export: {id}'s destination now leads into Chilpa's own folders; it is not handed over");
        return None;
    }
    Some(folder)
}

pub(crate) fn enabled(root: &Path, id: &str) -> bool {
    if !valid_id(id) { return false; }
    let Ok(bytes) = std::fs::read(root.join(id).join("manifest.json")) else { return false };
    let Ok(text) = std::str::from_utf8(&bytes) else { return false };
    chilpa_plugin::Manifest::parse_for_runtime(text, chilpa_plugin::Runtime::Process)
        .is_ok_and(|manifest| manifest.has(&chilpa_plugin::Capability::FolderWrite))
}

pub(crate) fn set(root: &Path, id: &str, selected: &Path) -> Result<(), String> {
    set_with(root, id, selected, &private())
}

fn set_with(root: &Path, id: &str, selected: &Path, private: &[PathBuf]) -> Result<(), String> {
    if !enabled(root, id) { return Err("This plugin cannot write to a selected folder.".into()); }
    let selected = selected.canonicalize().map_err(|_| "Choose an existing folder.")?;
    if !selected.is_dir() { return Err("Choose an existing folder.".into()); }
    if selected.parent().is_none() || is_private(&selected, private) {
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

    #[cfg(unix)]
    #[test]
    fn a_replaced_export_directory_loses_its_grant() {
        let root = tempfile::tempdir().unwrap();
        let selected = root.path().join("selected");
        let outside = root.path().join("outside");
        std::fs::create_dir(&selected).unwrap();
        std::fs::create_dir(&outside).unwrap();
        std::fs::create_dir(root.path().join(".exports")).unwrap();
        std::fs::write(file(root.path(), "test.export"), serde_json::to_vec(&selected.to_string_lossy()).unwrap()).unwrap();
        assert_eq!(read(root.path(), "test.export"), Some(selected.canonicalize().unwrap()));
        std::fs::remove_dir(&selected).unwrap();
        std::os::unix::fs::symlink(&outside, &selected).unwrap();
        assert_eq!(read(root.path(), "test.export"), None);
    }

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
    const FOLDER_WRITE: &str = r#"{"id":"app.example.download","name":"Download","version":"1","api_version":1,"kind":"source","runtime":"process","capabilities":[{"type":"folder_write"}]}"#;

    #[test]
    fn a_folder_of_chilpa_s_own_is_never_an_export() {
        let root = tempfile::tempdir().unwrap();
        let base = root.path().canonicalize().unwrap();
        let plugins = base.join("plugins");
        std::fs::create_dir_all(plugins.join("app.example.download")).unwrap();
        std::fs::write(plugins.join("app.example.download/manifest.json"), FOLDER_WRITE).unwrap();
        let private = base.join("data");
        for folder in [private.join("plugin-data/other"), base.join("music")] { std::fs::create_dir_all(folder).unwrap(); }
        let own = [private.clone()];
        for refused in [private.join("plugin-data/other"), private.clone(), base.clone()] {
            assert!(set_with(&plugins, "app.example.download", &refused, &own).is_err(), "{} is in, is, or holds Chilpa's own", refused.display());
        }
        set_with(&plugins, "app.example.download", &base.join("music"), &own).unwrap();
        assert_eq!(read_with(&plugins, "app.example.download", &own), Some(base.join("music")));
    }

    #[cfg(unix)]
    #[test]
    fn an_export_that_leads_into_chilpa_s_own_folders_by_launch_is_dropped() {
        let root = tempfile::tempdir().unwrap();
        let base = root.path().canonicalize().unwrap();
        let plugins = base.join("plugins");
        std::fs::create_dir_all(plugins.join(".exports")).unwrap();
        let private = base.join("data");
        for folder in [base.join("music/staging"), private.join("staging")] { std::fs::create_dir_all(folder).unwrap(); }
        std::fs::write(file(&plugins, "app.example.download"), serde_json::to_vec(&base.join("music/staging").to_string_lossy()).unwrap()).unwrap();
        let own = [private.clone()];
        assert_eq!(read_with(&plugins, "app.example.download", &own), Some(base.join("music/staging")));
        // Whoever can write the music folder swaps the folder above for a link.
        std::fs::remove_dir_all(base.join("music")).unwrap();
        std::os::unix::fs::symlink(&private, base.join("music")).unwrap();
        assert_eq!(read_with(&plugins, "app.example.download", &own), None, "it now leads into Chilpa's data");
    }
}
