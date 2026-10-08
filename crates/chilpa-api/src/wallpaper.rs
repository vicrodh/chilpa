//! The desktop's wallpaper, for the background looks that paint it, as
//! QBZ's shell resolves it: KDE Plasma's `plasma-org.kde.plasma.desktop-
//! appletsrc` (the containment on screen 0 wins; a wallpaper package
//! resolves to its largest `contents/images/*` bitmap, a slideshow folder
//! to its first image), GNOME through `gsettings` (the dark variant under a
//! dark scheme), Windows' current `TranscodedWallpaper`, macOS through
//! System Events. An image the user chose outranks the desktop's.
//!
//! The wallpaper look also wants to know where the window sits on the
//! screen, which Wayland hides from clients; KDE Plasma tells it through
//! its window-management protocol, read by the desktop crate.

use std::path::{Path, PathBuf};

const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "bmp", "avif", "jxl"];

/// The wallpaper to paint: the user's own `custom` image when it names a
/// file, else the desktop's. `None` when neither can be found.
pub fn resolve(custom: &str) -> Option<PathBuf> {
    let path = if custom.trim().is_empty() { resolve_system()? } else { PathBuf::from(custom.trim()) };
    path.is_file().then_some(path)
}

/// The desktop's current wallpaper, if this desktop tells us.
pub fn resolve_system() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    return windows_wallpaper();
    #[cfg(target_os = "macos")]
    return macos_wallpaper();
    // Android has no desktop and no wallpaper a window could sit over.
    #[cfg(target_os = "android")]
    return None;
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "android")))]
    return plasma_wallpaper().or_else(gnome_wallpaper);
}

/// Starts following this process's windows on a Plasma Wayland desktop:
/// `publish` gets `[{"x","y","w","h"}, ...]` whenever their places change.
/// Nothing to follow anywhere else; the window then centres its crop.
pub fn track_window_position(publish: impl Fn(String) + Send + 'static) {
    #[cfg(target_os = "linux")]
    chilpa_mpris::wayland_window::track(publish);
    #[cfg(not(target_os = "linux"))]
    let _ = publish;
}

/// `%20` and friends back to bytes: the desktops store wallpaper paths as
/// URLs. Anything malformed passes through untouched.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A wallpaper setting's value (`file:///...` or a bare path) as a path.
fn path_from_setting(value: &str) -> PathBuf {
    let v = value.trim().trim_matches('\'').trim_matches('"');
    let v = v.strip_prefix("file://").unwrap_or(v);
    PathBuf::from(percent_decode(v))
}

fn is_image(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| IMAGE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// A file stays a file; a directory is either a KDE wallpaper package
/// (`contents/images/<WxH>.<ext>`, the largest wins) or a slideshow folder
/// (its first image, sorted).
fn resolve_image_source(path: PathBuf) -> Option<PathBuf> {
    if path.is_file() {
        return Some(path);
    }
    if !path.is_dir() {
        return None;
    }
    let package = path.join("contents").join("images");
    let dir = if package.is_dir() { package } else { path };
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir).ok()?.flatten().map(|e| e.path()).filter(|p| p.is_file() && is_image(p)).collect();
    if files.is_empty() {
        return None;
    }
    files.sort();
    let pixels = |p: &Path| -> u64 {
        p.file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| {
                let (w, h) = s.split_once('x')?;
                Some(w.parse::<u64>().ok()? * h.parse::<u64>().ok()?)
            })
            .unwrap_or(0)
    };
    // The first of equals wins: a slideshow folder has no sizes in its names.
    let mut best = files[0].clone();
    let mut best_pixels = pixels(&best);
    for file in &files[1..] {
        let count = pixels(file);
        if count > best_pixels {
            best = file.clone();
            best_pixels = count;
        }
    }
    Some(best)
}

fn section_ids(section: &str) -> Vec<&str> {
    section.trim_matches(|c| c == '[' || c == ']').split("][").collect()
}

/// The image Plasma paints on screen 0 (else the lowest screen, else any):
/// `[Containments][N][Wallpaper][org.kde.image][General] Image=`, for the
/// containments whose wallpaper plugin is the image one.
fn kde_image_from_config(text: &str) -> Option<String> {
    use std::collections::HashMap;
    let mut section = String::new();
    let mut screen_of: HashMap<String, i32> = HashMap::new();
    let mut plugin_of: HashMap<String, String> = HashMap::new();
    let mut image_of: HashMap<String, String> = HashMap::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with('[') {
            section = line.to_string();
            continue;
        }
        let Some((key, value)) = line.split_once('=') else { continue };
        let ids = section_ids(&section);
        if ids.len() == 2 && ids[0] == "Containments" {
            match key {
                "lastScreen" => {
                    if let Ok(n) = value.trim().parse::<i32>() {
                        screen_of.insert(ids[1].to_string(), n);
                    }
                }
                "wallpaperplugin" => {
                    plugin_of.insert(ids[1].to_string(), value.trim().to_string());
                }
                _ => {}
            }
        } else if ids.len() == 5 && ids[0] == "Containments" && ids[2] == "Wallpaper" && ids[3] == "org.kde.image" && ids[4] == "General" && key == "Image" && !value.trim().is_empty() {
            image_of.insert(ids[1].to_string(), value.trim().to_string());
        }
    }
    let mut best: Option<(i32, String)> = None;
    for (id, image) in image_of {
        if plugin_of.get(&id).is_some_and(|p| p != "org.kde.image") {
            continue;
        }
        let screen = screen_of.get(&id).copied().unwrap_or(i32::MAX);
        let rank = if screen < 0 { i32::MAX } else { screen };
        if best.as_ref().is_none_or(|(r, _)| rank < *r) {
            best = Some((rank, image));
        }
    }
    best.map(|(_, image)| image)
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "android")))]
fn plasma_wallpaper() -> Option<PathBuf> {
    let config = chilpa_dirs::config_base().join("plasma-org.kde.plasma.desktop-appletsrc");
    let value = kde_image_from_config(&std::fs::read_to_string(config).ok()?)?;
    resolve_image_source(path_from_setting(&value))
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "android")))]
fn gnome_setting(schema: &str, key: &str) -> Option<String> {
    let out = std::process::Command::new("gsettings").args(["get", schema, key]).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "android")))]
fn gnome_wallpaper() -> Option<PathBuf> {
    let dark = gnome_setting("org.gnome.desktop.interface", "color-scheme").is_some_and(|v| v.contains("prefer-dark"));
    let key = if dark { "picture-uri-dark" } else { "picture-uri" };
    let value = gnome_setting("org.gnome.desktop.background", key)
        .filter(|v| !v.trim_matches('\'').is_empty())
        .or_else(|| gnome_setting("org.gnome.desktop.background", "picture-uri"))?;
    resolve_image_source(path_from_setting(&value))
}

#[cfg(target_os = "windows")]
fn windows_wallpaper() -> Option<PathBuf> {
    // The roaming profile is the config base chilpa-dirs resolves on Windows.
    let transcoded = chilpa_dirs::config_base().join("Microsoft").join("Windows").join("Themes").join("TranscodedWallpaper");
    if transcoded.is_file() {
        return Some(transcoded);
    }
    let out = std::process::Command::new("reg").args(["query", r"HKCU\Control Panel\Desktop", "/v", "WallPaper"]).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().find(|l| l.contains("WallPaper"))?;
    let value = line.split("REG_SZ").nth(1)?.trim();
    resolve_image_source(PathBuf::from(value))
}

#[cfg(target_os = "macos")]
fn macos_wallpaper() -> Option<PathBuf> {
    let out = std::process::Command::new("osascript")
        .args(["-e", "tell application \"System Events\" to get picture of current desktop"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if value.is_empty() {
        return None;
    }
    resolve_image_source(PathBuf::from(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLASMA_CONFIG: &str = r#"
[Containments][1]
activityId=abc
lastScreen=1
plugin=org.kde.plasma.folder
wallpaperplugin=org.kde.image

[Containments][1][Wallpaper][org.kde.image][General]
Image=file:///home/me/Pictures/Second%20Screen.png

[Containments][2]
activityId=abc
lastScreen=0
plugin=org.kde.plasma.folder
wallpaperplugin=org.kde.image

[Containments][2][Wallpaper][org.kde.color][General]
Color=0,0,0

[Containments][2][Wallpaper][org.kde.image][General]
Image=file:///usr/share/wallpapers/MilkyWay/

[Containments][3]
lastScreen=0
wallpaperplugin=org.kde.color

[Containments][3][Wallpaper][org.kde.image][General]
Image=file:///home/me/stale.png
"#;

    #[test]
    fn plasma_picks_the_image_containment_on_screen_zero() {
        assert_eq!(kde_image_from_config(PLASMA_CONFIG).as_deref(), Some("file:///usr/share/wallpapers/MilkyWay/"));
        // Without screen 0 the lowest screen wins; a color containment never does.
        let only_second = PLASMA_CONFIG.replace("lastScreen=0", "lastScreen=5");
        assert_eq!(kde_image_from_config(&only_second).as_deref(), Some("file:///home/me/Pictures/Second%20Screen.png"));
        assert!(kde_image_from_config("[Containments][9]\nlastScreen=0\n").is_none());
    }

    #[test]
    fn settings_values_become_decoded_paths() {
        assert_eq!(path_from_setting("file:///home/me/Pictures/Second%20Screen.png"), PathBuf::from("/home/me/Pictures/Second Screen.png"));
        assert_eq!(path_from_setting("'file:///usr/share/backgrounds/a.jpg'"), PathBuf::from("/usr/share/backgrounds/a.jpg"));
        assert_eq!(path_from_setting("/plain/path.png"), PathBuf::from("/plain/path.png"));
        assert_eq!(percent_decode("100%"), "100%");
    }

    #[test]
    fn a_package_resolves_to_its_largest_bitmap_and_a_folder_to_its_first_image() {
        let temp = tempfile::tempdir().unwrap();
        let package = temp.path().join("MilkyWay");
        let images = package.join("contents").join("images");
        std::fs::create_dir_all(&images).unwrap();
        for name in ["1080x1920.png", "5120x2880.png", "notes.txt"] {
            std::fs::write(images.join(name), b"x").unwrap();
        }
        assert_eq!(resolve_image_source(package.clone()), Some(images.join("5120x2880.png")));
        let slides = temp.path().join("slides");
        std::fs::create_dir_all(&slides).unwrap();
        for name in ["b.jpg", "a.png", "readme.md"] {
            std::fs::write(slides.join(name), b"x").unwrap();
        }
        assert_eq!(resolve_image_source(slides.clone()), Some(slides.join("a.png")));
        let file = temp.path().join("one.webp");
        std::fs::write(&file, b"x").unwrap();
        assert_eq!(resolve_image_source(file.clone()), Some(file.clone()));
        assert!(resolve_image_source(temp.path().join("missing")).is_none());
        // The user's own image outranks the desktop's, and only when it exists.
        assert_eq!(resolve(&file.to_string_lossy()), Some(file));
        assert_eq!(resolve(&temp.path().join("gone.png").to_string_lossy()), None);
    }
}
