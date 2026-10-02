//! Plugin images are installation data, never bundled provider assets.

use std::{io::{Cursor, Read}, path::{Path, PathBuf}};

pub(crate) const INCLUDED_ICON: &str = ".host-icon.png";
pub(crate) struct IconImage { pub bytes: Vec<u8>, pub extension: &'static str }

pub(crate) fn letter(name: &str) -> String {
    let initial = name.chars().find(char::is_ascii_alphabetic).unwrap_or('X').to_ascii_uppercase();
    format!("letter{initial}")
}

pub(crate) fn selected(root: &Path, id: &str) -> Option<PathBuf> {
    let custom = root.join(".icons").join(format!("{id}.png"));
    let included = root.join(id).join(INCLUDED_ICON);
    [custom.clone(), custom.with_extension("svg"), included.clone(), included.with_extension("svg")].into_iter().find(|path| path.is_file())
}

pub(crate) fn image_url(root: &Path, id: &str) -> String {
    let Some(path) = selected(root, id) else { return String::new() };
    let Ok(mut url) = url::Url::from_file_path(&path) else { return String::new() };
    if let Some(revision) = path.metadata().ok().and_then(|metadata| metadata.modified().ok())
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok()) {
        url.set_query(Some(&format!("revision={}", revision.as_nanos())));
    }
    url.to_string()
}

pub(crate) fn included(root: &Path, declared: &str) -> Option<IconImage> {
    if declared.is_empty() { return None; }
    let relative = Path::new(declared);
    if relative.components().any(|part| !matches!(part, std::path::Component::Normal(_))) { return None; }
    let root = root.canonicalize().ok()?;
    [root.join(relative), root.join("HTML").join("EN").join(relative)].into_iter()
        .filter_map(|path| path.canonicalize().ok()).filter(|path| path.starts_with(&root))
        .find_map(|path| normalize(&path).ok())
}

fn normalize(path: &Path) -> Result<IconImage, String> {
    let file = std::fs::File::open(path).map_err(|_| "The icon could not be read.")?;
    let mut bytes = Vec::new();
    file.take(4 * 1024 * 1024 + 1).read_to_end(&mut bytes).map_err(|_| "The icon could not be read.")?;
    if bytes.len() > 4 * 1024 * 1024 { return Err("The icon is too large.".into()); }
    if path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("svg")) {
        crate::svg_icon::validate(&bytes)?;
        return Ok(IconImage { bytes, extension: "svg" });
    }
    let mut reader = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format().map_err(|_| "The icon format is unsupported.")?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(32 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader.decode().map_err(|_| "Use a supported PNG or JPEG icon.")?;
    let mut output = Cursor::new(Vec::new());
    decoded.thumbnail(256, 256).write_to(&mut output, image::ImageFormat::Png).map_err(|_| "The icon could not be prepared.")?;
    Ok(IconImage { bytes: output.into_inner(), extension: "png" })
}

pub(crate) fn set(root: &Path, id: &str, path: Option<&Path>) -> Result<(), String> {
        if id.is_empty() || id.contains("..") || !id.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-'))
            || !root.join(id).join("manifest.json").is_file() { return Err("The plugin is not installed.".into()); }
        let directory = root.join(".icons");
        let target = directory.join(format!("{id}.png"));
        let Some(path) = path else {
            let _ = std::fs::remove_file(target.with_extension("svg"));
            return match std::fs::remove_file(target) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(_) => Err("The icon override could not be removed.".into()),
            };
        };
        let bytes = normalize(path)?;
        std::fs::create_dir_all(&directory).map_err(|_| "The icon directory could not be created.")?;
        let mut temporary = tempfile::NamedTempFile::new_in(&directory).map_err(|_| "The icon could not be saved.")?;
        use std::io::Write;
        temporary.write_all(&bytes.bytes).and_then(|_| temporary.as_file().sync_all()).map_err(|_| "The icon could not be saved.")?;
        temporary.persist(target.with_extension(bytes.extension)).map_err(|_| "The icon could not be saved.")?;
        let _ = std::fs::remove_file(target.with_extension(if bytes.extension == "svg" { "png" } else { "svg" }));
        Ok(())
}

pub(crate) fn tint(root: &Path, id: &str) -> bool {
    root.join(".icons").join(format!("{id}.tint")).is_file()
}

pub(crate) fn set_tint(root: &Path, id: &str, enabled: bool) -> Result<(), String> {
    if id.is_empty() || id.contains("..") || !id.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-'))
        || !root.join(id).join("manifest.json").is_file() { return Err("The plugin is not installed.".into()); }
    let directory = root.join(".icons");
    std::fs::create_dir_all(&directory).map_err(|_| "The icon preference could not be saved.")?;
    let path = directory.join(format!("{id}.tint"));
    if enabled { std::fs::write(path, b"1").map_err(|_| "The icon preference could not be saved.".into()) }
    else { match std::fs::remove_file(path) { Ok(()) => Ok(()), Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()), Err(_) => Err("The icon preference could not be saved.".into()) } }
}

impl crate::Chilpa {
    pub fn set_source_icon(&self, id: &str, path: Option<&Path>) -> Result<(), String> {
        set(&Self::plugins_dir(), id, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_preferences_are_explicit_persistent_and_bound_to_an_installation() {
        let root = tempfile::tempdir().unwrap();
        let id = "org.example.guest";
        let guest = root.path().join(id);
        std::fs::create_dir(&guest).unwrap();
        std::fs::write(guest.join("manifest.json"), b"{}").unwrap();
        let input = root.path().join("picked.png");
        image::RgbaImage::new(8, 8).save(&input).unwrap();
        assert!(!tint(root.path(), id));
        set(root.path(), id, Some(&input)).unwrap();
        assert!(selected(root.path(), id).unwrap().starts_with(root.path().join(".icons")));
        set_tint(root.path(), id, true).unwrap();
        assert!(tint(root.path(), id));
        set_tint(root.path(), id, false).unwrap();
        assert!(!tint(root.path(), id));
        set(root.path(), id, None).unwrap();
        assert!(selected(root.path(), id).is_none());
        assert!(set(root.path(), "../outside", Some(&input)).is_err());
        assert!(set_tint(root.path(), "not-installed", true).is_err());
    }

    #[test]
    fn the_package_is_default_and_only_an_explicit_override_takes_precedence() {
        let root = tempfile::tempdir().unwrap();
        let guest = root.path().join("org.example.guest");
        std::fs::create_dir_all(&guest).unwrap();
        assert!(selected(root.path(), "org.example.guest").is_none());
        assert_eq!(letter("Guest"), "letterG");
        assert_eq!(letter("123"), "letterX");
        let image = image::RgbaImage::new(2, 2);
        image.save(guest.join(INCLUDED_ICON)).unwrap();
        assert_eq!(selected(root.path(), "org.example.guest").unwrap(), guest.join(INCLUDED_ICON));
        let custom = root.path().join(".icons");
        std::fs::create_dir(&custom).unwrap();
        image.save(custom.join("org.example.guest.png")).unwrap();
        assert_eq!(selected(root.path(), "org.example.guest").unwrap(), custom.join("org.example.guest.png"));
        assert!(included(&guest, "../.icons/org.example.guest.png").is_none());
        assert!(included(&guest, INCLUDED_ICON).is_some());
    }
}
