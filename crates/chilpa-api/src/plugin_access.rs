//! Where a native download view appears in the application's navigation.
use std::path::{Path, PathBuf};

fn file(root: &Path, id: &str) -> Option<PathBuf> {
    (!id.is_empty() && !id.contains("..") && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'-')))
        .then(|| root.join(".download-access").join(format!("{id}.json")))
}

pub(crate) fn read(root: &Path, id: &str) -> &'static str {
    let Some(path) = file(root, id) else { return "plugin"; };
    let value: String = std::fs::read(path).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default();
    match value.as_str() { "settings" => "settings", "sidebar" => "sidebar", _ => "plugin" }
}

pub(crate) fn set(root: &Path, id: &str, value: &str) -> Result<(), String> {
    if !matches!(value, "plugin" | "settings" | "sidebar") { return Err("Choose a valid download location.".into()); }
    let path = file(root, id).ok_or("Invalid plugin ID")?;
    let parent = path.parent().ok_or("Invalid plugin ID")?;
    std::fs::create_dir_all(parent).map_err(|_| "The download location could not be saved.")?;
    let staged = path.with_extension("json.part");
    std::fs::write(&staged, serde_json::to_vec(value).map_err(|_| "The download location could not be saved.")?).map_err(|_| "The download location could not be saved.")?;
    std::fs::rename(staged, path).map_err(|_| "The download location could not be saved.".into())
}

pub(crate) fn clear(root: &Path, id: &str) {
    if let Some(path) = file(root, id) { let _ = std::fs::remove_file(path); }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn locations_are_scoped_to_an_installed_plugin() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(read(root.path(), "app.example"), "plugin");
        set(root.path(), "app.example", "sidebar").unwrap();
        assert_eq!(read(root.path(), "app.example"), "sidebar");
        assert!(set(root.path(), "../escape", "settings").is_err());
        assert!(set(root.path(), "app.example", "elsewhere").is_err());
        clear(root.path(), "app.example");
        assert_eq!(read(root.path(), "app.example"), "plugin");
    }
}
