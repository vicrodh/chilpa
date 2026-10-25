//! I serialize settings mutations and replace complete files beside their destination.
use super::Settings;
use std::{io::{self, Read, Write}, path::Path, sync::Mutex};

/// I reserve this value for checked catalog transactions, never stale settings snapshots.
pub const SAVED_QUERIES_KEY: &str = "ui.refine.saved_queries";
static WRITER: Mutex<()> = Mutex::new(());
const MAX_SETTINGS_BYTES: u64 = 16 * 1024 * 1024;

fn read(file: &Path) -> io::Result<String> {
    let input = match std::fs::File::open(file) {
        Ok(input) => input,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(String::new()),
        Err(error) => return Err(error),
    };
    let mut bytes = Vec::new();
    input.take(MAX_SETTINGS_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_SETTINGS_BYTES { return Err(io::Error::other("settings file is too large")); }
    String::from_utf8(bytes).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn value(text: &str, key: &str) -> io::Result<Option<String>> {
    let mut found = None;
    for line in text.lines().filter(|line| !line.trim_start().starts_with('#')) {
        if let Some((name, held)) = line.split_once('=') {
            if name.trim() == key {
                if found.is_some() { return Err(io::Error::other("duplicate settings key")); }
                found = Some(held.trim().to_owned());
            }
        }
    }
    Ok(found)
}

fn replace(text: &str, key: &str, replacement: &str) -> io::Result<String> {
    if key.contains(['\n', '\r', '=']) || replacement.contains(['\n', '\r']) {
        return Err(io::Error::other("settings values must fit on one line"));
    }
    value(text, key)?;
    let mut out = String::with_capacity(text.len() + replacement.len());
    let mut replaced = false;
    for line in text.split_inclusive('\n') {
        if !line.trim_start().starts_with('#') && line.split_once('=').is_some_and(|(name, _)| name.trim() == key) {
            out.push_str(&format!("{key} = {replacement}\n"));
            replaced = true;
        } else { out.push_str(line); }
    }
    if !replaced {
        if !out.is_empty() && !out.ends_with('\n') { out.push('\n'); }
        out.push_str(&format!("{key} = {replacement}\n"));
    }
    Ok(out)
}

fn write(file: &Path, text: &str) -> io::Result<()> {
    write_before_replace(file, text, || Ok(()))
}

fn write_before_replace(file: &Path, text: &str, before: impl FnOnce() -> io::Result<()>) -> io::Result<()> {
    let parent = file.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(text.as_bytes())?;
    temporary.as_file().sync_all()?;
    before()?;
    // tempfile uses atomic replacement on Unix and Windows; dropping a failed persist removes the temporary.
    temporary.persist(file).map_err(|error| error.error)?;
    Ok(())
}

/// I report damaged or unreadable settings instead of treating them as an empty catalog.
pub fn read_value(key: &str) -> io::Result<Option<String>> { read_value_from(&super::path(), key) }

pub fn read_value_from(file: &Path, key: &str) -> io::Result<Option<String>> {
    let _guard = WRITER.lock().map_err(|_| io::Error::other("settings writer unavailable"))?;
    value(&read(file)?, key)
}

/// I publish the result only after writing; the closure runs under the settings mutation lock.
pub fn update_value<T>(key: &str, change: impl FnOnce(Option<&str>) -> Result<(String, T), String>) -> Result<T, String> {
    update_value_at(&super::path(), key, change)
}

pub fn update_value_at<T>(file: &Path, key: &str, change: impl FnOnce(Option<&str>) -> Result<(String, T), String>) -> Result<T, String> {
    let _guard = WRITER.lock().map_err(|_| "settings writer unavailable".to_string())?;
    let text = read(file).map_err(|e| e.to_string())?;
    let old = value(&text, key).map_err(|e| e.to_string())?;
    let (new, result) = change(old.as_deref())?;
    if old.as_deref() != Some(new.as_str()) {
        write(file, &replace(&text, key, &new).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    }
    Ok(result)
}

pub(super) fn save(file: &Path, settings: &Settings) -> io::Result<()> {
    let _guard = WRITER.lock().map_err(|_| io::Error::other("settings writer unavailable"))?;
    let mut text = read(file)?;
    for (key, held) in [
        ("output", &settings.output), ("last_folder", &settings.last_folder), ("layout", &settings.layout),
        ("album_sort", &settings.album_sort), ("visualizer", &settings.visualizer), ("language", &settings.language),
    ].into_iter().chain(settings.window.iter().map(|(k, v)| (k.as_str(), v))) {
        // A legacy read/save pair may predate a catalog mutation. I preserve its exact current bytes.
        if ![SAVED_QUERIES_KEY, "library.scanPolicies", "library.scanIntent"].contains(&key) {
            text = replace(&text, key, held)?;
        }
    }
    write(file, &text)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_replacement_keeps_the_original_and_cleans_the_temporary() {
        let dir = tempfile::tempdir().unwrap(); let file = dir.path().join("settings.conf");
        std::fs::write(&file, "appearance.theme = dark\n").unwrap();
        assert!(write_before_replace(&file, "replacement", || Err(io::Error::other("injected failure"))).is_err());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "appearance.theme = dark\n");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }
    #[test]
    fn stale_preferences_do_not_replace_a_catalog_or_its_damaged_bytes() {
        let dir = tempfile::tempdir().unwrap(); let file = dir.path().join("settings.conf");
        let stale = Settings::default();
        let original = format!("{SAVED_QUERIES_KEY} =  {{broken bytes  \n");
        std::fs::write(&file, &original).unwrap();
        save(&file, &stale).unwrap();
        assert!(std::fs::read_to_string(&file).unwrap().starts_with(&original));
        update_value_at(&file, "appearance.theme", |_| Ok(("light".into(), ()))).unwrap();
        assert!(std::fs::read_to_string(&file).unwrap().starts_with(&original));
    }
    #[test]
    fn stale_preferences_preserve_checked_scan_policy_and_intent() {
        let dir = tempfile::tempdir().unwrap(); let file = dir.path().join("settings.conf");
        let mut stale = Settings::default();
        for key in ["library.scanPolicies", "library.scanIntent"] {
            stale.window.insert(key.into(), "outdated".into());
            update_value_at(&file, key, |_| Ok(("current".into(), ()))).unwrap();
        }
        save(&file, &stale).unwrap();
        for key in ["library.scanPolicies", "library.scanIntent"] {
            assert_eq!(read_value_from(&file, key).unwrap().as_deref(), Some("current"));
        }
    }
    #[test]
    fn unreadable_and_duplicate_values_are_not_replaced() {
        let dir = tempfile::tempdir().unwrap(); let file = dir.path().join("settings.conf");
        for bytes in [b"\xff".as_slice(), b"a.b = x\na.b = y\n".as_slice()] {
            std::fs::write(&file, bytes).unwrap();
            assert!(update_value_at(&file, "a.b", |_| Ok(("z".into(), ()))).is_err());
            assert_eq!(std::fs::read(&file).unwrap(), bytes);
        }
    }
}
