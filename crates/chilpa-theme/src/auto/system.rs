//! Desktop-environment detection, wallpaper path retrieval, and system accent /
//! color-scheme reading, ported 1:1 from QBZ 2.1.2 (`qbz-theme/src/auto/
//! system.rs`): gsettings, kdeglobals, COSMIC and xfconf probing.
//!
//! The desktop's name and its configuration directory come from
//! `chilpa-dirs`, the one place that knows each platform's conventions
//! (ADR-010). Away from a Linux desktop every reader here answers with an
//! error, and the auto theme falls back the way QBZ's does.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use super::{PaletteColor, SystemColorScheme};

/// Supported desktop environments.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DesktopEnvironment {
    Gnome,
    KdePlasma,
    Cosmic,
    Xfce,
    Cinnamon,
    Unknown(String),
}

impl DesktopEnvironment {
    /// Human-readable name.
    pub fn display_name(&self) -> &str {
        match self {
            Self::Gnome => "GNOME",
            Self::KdePlasma => "KDE Plasma",
            Self::Cosmic => "COSMIC",
            Self::Xfce => "Xfce",
            Self::Cinnamon => "Cinnamon",
            Self::Unknown(name) => name.as_str(),
        }
    }
}

/// Detect the current desktop environment.
pub fn detect_desktop_environment() -> DesktopEnvironment {
    let candidates = chilpa_dirs::desktop_names();

    for candidate in &candidates {
        if let Some(val) = candidate {
            let upper = val.to_uppercase();
            if upper.contains("GNOME") || upper.contains("UNITY") || upper.contains("UBUNTU") {
                return DesktopEnvironment::Gnome;
            }
            if upper.contains("KDE") || upper.contains("PLASMA") {
                return DesktopEnvironment::KdePlasma;
            }
            if upper.contains("COSMIC") {
                return DesktopEnvironment::Cosmic;
            }
            if upper.contains("XFCE") {
                return DesktopEnvironment::Xfce;
            }
            if upper.contains("CINNAMON") || upper.contains("X-CINNAMON") {
                return DesktopEnvironment::Cinnamon;
            }
        }
    }

    let name = candidates
        .iter()
        .find_map(|c| c.clone())
        .unwrap_or_else(|| "unknown".to_string());

    DesktopEnvironment::Unknown(name)
}

/// Get the current wallpaper path for the detected DE.
pub fn get_system_wallpaper() -> Result<String, String> {
    let de = detect_desktop_environment();
    get_wallpaper_for_de(&de)
}

fn get_wallpaper_for_de(de: &DesktopEnvironment) -> Result<String, String> {
    match de {
        DesktopEnvironment::Gnome => get_gnome_wallpaper(),
        DesktopEnvironment::KdePlasma => get_kde_wallpaper(),
        DesktopEnvironment::Cosmic => get_cosmic_wallpaper(),
        DesktopEnvironment::Cinnamon => get_cinnamon_wallpaper(),
        DesktopEnvironment::Xfce => get_xfce_wallpaper(),
        DesktopEnvironment::Unknown(name) => {
            Err(format!("Unsupported desktop environment: {}", name))
        }
    }
}

/// Get the system accent color for the detected DE.
pub fn get_system_accent_color() -> Result<PaletteColor, String> {
    let de = detect_desktop_environment();
    get_accent_for_de(&de)
}

fn get_accent_for_de(de: &DesktopEnvironment) -> Result<PaletteColor, String> {
    match de {
        DesktopEnvironment::Gnome => get_gnome_accent(),
        DesktopEnvironment::KdePlasma => get_kde_accent(),
        DesktopEnvironment::Cosmic => get_cosmic_accent(),
        _ => Err(format!(
            "Accent color not supported for {}",
            de.display_name()
        )),
    }
}

// --- GNOME ---

fn get_gnome_wallpaper() -> Result<String, String> {
    for key in &["picture-uri-dark", "picture-uri"] {
        let output = Command::new("gsettings")
            .args(["get", "org.gnome.desktop.background", key])
            .output()
            .map_err(|e| format!("Failed to run gsettings: {}", e))?;

        if output.status.success() {
            let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if let Some(path) = parse_gsettings_uri(&raw) {
                if PathBuf::from(&path).exists() {
                    return Ok(path);
                }
            }
        }
    }
    Err("Could not determine GNOME wallpaper".into())
}

fn get_gnome_accent() -> Result<PaletteColor, String> {
    let output = Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "accent-color"])
        .output()
        .map_err(|e| format!("Failed to run gsettings: {}", e))?;

    if !output.status.success() {
        return Err("gsettings accent-color not available (requires GNOME 47+)".into());
    }

    let raw = String::from_utf8_lossy(&output.stdout)
        .trim()
        .trim_matches('\'')
        .to_lowercase();

    let color = match raw.as_str() {
        "blue" => PaletteColor::new(53, 132, 228),
        "teal" => PaletteColor::new(38, 162, 105),
        "green" => PaletteColor::new(51, 209, 122),
        "yellow" => PaletteColor::new(246, 211, 45),
        "orange" => PaletteColor::new(255, 120, 0),
        "red" => PaletteColor::new(224, 27, 36),
        "pink" => PaletteColor::new(220, 138, 221),
        "purple" => PaletteColor::new(145, 65, 172),
        "slate" => PaletteColor::new(111, 131, 150),
        _ => return Err(format!("Unknown GNOME accent color: {}", raw)),
    };

    Ok(color)
}

// --- KDE Plasma ---

fn get_kde_wallpaper() -> Result<String, String> {
    let config_path = chilpa_dirs::config_base().join("plasma-org.kde.plasma.desktop-appletsrc");

    let content = fs::read_to_string(&config_path)
        .map_err(|e| format!("Cannot read Plasma config: {}", e))?;

    let mut in_wallpaper_section = false;
    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with('[') {
            in_wallpaper_section = trimmed.contains("Wallpaper")
                && trimmed.contains("org.kde.image")
                && trimmed.contains("General");
        }

        if in_wallpaper_section && trimmed.starts_with("Image=") {
            let value = trimmed.trim_start_matches("Image=").trim();
            if let Some(path) = parse_file_uri(value) {
                if PathBuf::from(&path).exists() {
                    return Ok(path);
                }
            }
            if PathBuf::from(value).exists() {
                return Ok(value.to_string());
            }
        }
    }

    Err("Could not find wallpaper in Plasma config".into())
}

fn get_kde_accent() -> Result<PaletteColor, String> {
    let config_path = chilpa_dirs::config_base().join("kdeglobals");

    let content =
        fs::read_to_string(&config_path).map_err(|e| format!("Cannot read kdeglobals: {}", e))?;

    // 1. Explicit AccentColor in [General] (Plasma 6 custom accent).
    let mut in_general = false;
    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed == "[General]" {
            in_general = true;
            continue;
        }
        if trimmed.starts_with('[') {
            in_general = false;
            continue;
        }

        if in_general && trimmed.starts_with("AccentColor=") {
            let value = trimmed.trim_start_matches("AccentColor=").trim();
            return parse_rgb_csv(value);
        }
    }

    // 2. Fallback: color-scheme sections (DecorationFocus / Selection background).
    let fallback_sections = [
        ("[Colors:Selection]", "DecorationFocus"),
        ("[Colors:Selection]", "BackgroundNormal"),
        ("[Colors:View]", "DecorationFocus"),
    ];

    for (section, key) in &fallback_sections {
        if let Some(color) = read_kde_color_key(&content, section, key) {
            return Ok(color);
        }
    }

    Err("AccentColor not found in kdeglobals (no explicit accent or color scheme)".into())
}

/// Read a specific key from a KDE config section, parsing "r,g,b" format.
fn read_kde_color_key(content: &str, section: &str, key: &str) -> Option<PaletteColor> {
    let mut in_section = false;
    let prefix = format!("{}=", key);

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed == section {
            in_section = true;
            continue;
        }
        if trimmed.starts_with('[') {
            in_section = false;
            continue;
        }

        if in_section && trimmed.starts_with(&prefix) {
            let value = trimmed[prefix.len()..].trim();
            return parse_rgb_csv(value).ok();
        }
    }

    None
}

// --- COSMIC ---

fn get_cosmic_wallpaper() -> Result<String, String> {
    let base = chilpa_dirs::config_base().join("cosmic").join("com.system76.CosmicBackground").join("v1");
    let config_paths = [base.join("all"), base.join("backgrounds")];

    for config_path in &config_paths {
        if let Ok(content) = fs::read_to_string(config_path) {
            if let Some(path) = extract_path_from_cosmic_config(&content) {
                if PathBuf::from(&path).exists() {
                    return Ok(path);
                }
            }
        }
    }

    Err("Could not find wallpaper in COSMIC config".into())
}

fn get_cosmic_accent() -> Result<PaletteColor, String> {
    let base = chilpa_dirs::config_base().join("cosmic");
    let accent_paths = [
        base.join("com.system76.CosmicTheme.Dark").join("v1").join("accent"),
        base.join("com.system76.CosmicTheme.Light").join("v1").join("accent"),
    ];

    for path in &accent_paths {
        if let Ok(content) = fs::read_to_string(path) {
            if let Some(color) = parse_cosmic_color(&content) {
                return Ok(color);
            }
        }
    }

    Err("Could not read COSMIC accent color".into())
}

// --- Cinnamon ---

fn get_cinnamon_wallpaper() -> Result<String, String> {
    let output = Command::new("gsettings")
        .args(["get", "org.cinnamon.desktop.background", "picture-uri"])
        .output()
        .map_err(|e| format!("Failed to run gsettings: {}", e))?;

    if !output.status.success() {
        return Err("Could not get Cinnamon wallpaper via gsettings".into());
    }

    let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if let Some(path) = parse_gsettings_uri(&raw) {
        if PathBuf::from(&path).exists() {
            return Ok(path);
        }
    }

    Err("Could not determine Cinnamon wallpaper".into())
}

// --- XFCE ---

fn get_xfce_wallpaper() -> Result<String, String> {
    let output = Command::new("xfconf-query")
        .args([
            "-c",
            "xfce4-desktop",
            "-p",
            "/backdrop/screen0/monitoreDP-1/workspace0/last-image",
        ])
        .output()
        .map_err(|e| format!("Failed to run xfconf-query: {}", e))?;

    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if PathBuf::from(&path).exists() {
            return Ok(path);
        }
    }

    let output = Command::new("xfconf-query")
        .args(["-c", "xfce4-desktop", "-l", "-v"])
        .output()
        .map_err(|e| format!("Failed to list xfce4-desktop properties: {}", e))?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if line.contains("last-image") {
                let parts: Vec<&str> = line.splitn(2, char::is_whitespace).collect();
                if parts.len() == 2 {
                    let path = parts[1].trim();
                    if PathBuf::from(path).exists() {
                        return Ok(path.to_string());
                    }
                }
            }
        }
    }

    Err("Could not determine XFCE wallpaper".into())
}

// --- Parsing helpers ---

/// Parse gsettings output like `'file:///path/to/wallpaper.jpg'` into a path.
fn parse_gsettings_uri(raw: &str) -> Option<String> {
    let trimmed = raw.trim().trim_matches('\'').trim_matches('"');
    parse_file_uri(trimmed).or_else(|| {
        if PathBuf::from(trimmed).is_absolute() {
            Some(trimmed.to_string())
        } else {
            None
        }
    })
}

/// Extract filesystem path from a `file:///path` URI.
fn parse_file_uri(uri: &str) -> Option<String> {
    uri.strip_prefix("file://").map(|path| path.replace("%20", " "))
}

/// Parse "r,g,b" CSV format (KDE).
fn parse_rgb_csv(value: &str) -> Result<PaletteColor, String> {
    let parts: Vec<&str> = value.split(',').collect();
    if parts.len() < 3 {
        return Err(format!("Invalid RGB CSV: {}", value));
    }
    let r = parts[0]
        .trim()
        .parse::<u8>()
        .map_err(|_| format!("Invalid R value: {}", parts[0]))?;
    let g = parts[1]
        .trim()
        .parse::<u8>()
        .map_err(|_| format!("Invalid G value: {}", parts[1]))?;
    let b = parts[2]
        .trim()
        .parse::<u8>()
        .map_err(|_| format!("Invalid B value: {}", parts[2]))?;
    Ok(PaletteColor::new(r, g, b))
}

/// Best-effort extraction of an image path from COSMIC config content.
fn extract_path_from_cosmic_config(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim().trim_matches('"').trim_matches('\'');

        if let Some(path) = parse_file_uri(trimmed) {
            if is_image_path(&path) {
                return Some(path);
            }
        }

        if trimmed.starts_with('/') && is_image_path(trimmed) {
            return Some(trimmed.to_string());
        }

        if let Some(start) = trimmed.find('/') {
            let potential = &trimmed[start..];
            let end = potential
                .find(|c: char| c == '"' || c == '\'' || c == ')' || c == ',')
                .unwrap_or(potential.len());
            let path = &potential[..end];
            if is_image_path(path) && PathBuf::from(path).is_absolute() {
                return Some(path.to_string());
            }
        }
    }
    None
}

/// Parse a COSMIC color (RON-like, RGBA floats or ints).
fn parse_cosmic_color(content: &str) -> Option<PaletteColor> {
    let nums: Vec<f64> = content
        .split(|c: char| !c.is_ascii_digit() && c != '.' && c != '-')
        .filter_map(|s| s.parse::<f64>().ok())
        .collect();

    if nums.len() >= 3 {
        let (r, g, b) = if nums[0] <= 1.0 && nums[1] <= 1.0 && nums[2] <= 1.0 {
            (
                (nums[0] * 255.0).round() as u8,
                (nums[1] * 255.0).round() as u8,
                (nums[2] * 255.0).round() as u8,
            )
        } else {
            (nums[0] as u8, nums[1] as u8, nums[2] as u8)
        };
        Some(PaletteColor::new(r, g, b))
    } else {
        None
    }
}

// --- Full color scheme ---

/// Read the full system color scheme from the current DE (KDE / GNOME only).
pub fn get_system_color_scheme() -> Result<SystemColorScheme, String> {
    let de = detect_desktop_environment();
    match de {
        DesktopEnvironment::KdePlasma => get_kde_color_scheme(),
        DesktopEnvironment::Gnome => get_gnome_color_scheme(),
        _ => Err(format!(
            "Full color scheme not supported for {}",
            de.display_name()
        )),
    }
}

fn get_kde_color_scheme() -> Result<SystemColorScheme, String> {
    let config_path = chilpa_dirs::config_base().join("kdeglobals");
    let content =
        fs::read_to_string(&config_path).map_err(|e| format!("Cannot read kdeglobals: {}", e))?;

    let accent_explicit = read_kde_color_key(&content, "[General]", "AccentColor");

    let scheme = SystemColorScheme {
        window_bg: read_kde_color_key(&content, "[Colors:Window]", "BackgroundNormal"),
        window_bg_alt: read_kde_color_key(&content, "[Colors:Window]", "BackgroundAlternate"),
        view_bg: read_kde_color_key(&content, "[Colors:View]", "BackgroundNormal"),
        button_bg: read_kde_color_key(&content, "[Colors:Button]", "BackgroundNormal"),
        header_bg: read_kde_color_key(&content, "[Colors:Header]", "BackgroundNormal"),
        header_bg_inactive: read_kde_color_key(
            &content,
            "[Colors:Header][Inactive]",
            "BackgroundNormal",
        ),
        tooltip_bg: read_kde_color_key(&content, "[Colors:Tooltip]", "BackgroundNormal"),

        window_fg: read_kde_color_key(&content, "[Colors:Window]", "ForegroundNormal"),
        window_fg_inactive: read_kde_color_key(&content, "[Colors:Window]", "ForegroundInactive"),
        view_fg: read_kde_color_key(&content, "[Colors:View]", "ForegroundNormal"),
        button_fg: read_kde_color_key(&content, "[Colors:Button]", "ForegroundNormal"),

        selection_bg: read_kde_color_key(&content, "[Colors:Selection]", "BackgroundNormal"),
        selection_fg: read_kde_color_key(&content, "[Colors:Selection]", "ForegroundNormal"),
        selection_hover: read_kde_color_key(&content, "[Colors:Selection]", "DecorationHover"),
        accent: accent_explicit
            .or_else(|| read_kde_color_key(&content, "[Colors:Selection]", "DecorationFocus")),

        fg_link: read_kde_color_key(&content, "[Colors:Window]", "ForegroundLink"),
        fg_negative: read_kde_color_key(&content, "[Colors:Window]", "ForegroundNegative"),
        fg_neutral: read_kde_color_key(&content, "[Colors:Window]", "ForegroundNeutral"),
        fg_positive: read_kde_color_key(&content, "[Colors:Window]", "ForegroundPositive"),

        wm_active_bg: read_kde_color_key(&content, "[WM]", "activeBackground"),
        wm_active_fg: read_kde_color_key(&content, "[WM]", "activeForeground"),
        wm_inactive_bg: read_kde_color_key(&content, "[WM]", "inactiveBackground"),
    };

    if scheme.window_bg.is_none() {
        return Err("KDE color scheme missing Colors:Window BackgroundNormal".into());
    }

    Ok(scheme)
}

fn get_gnome_color_scheme() -> Result<SystemColorScheme, String> {
    // GNOME exposes little via dconf: detect dark/light + accent, fill the rest
    // with Adwaita defaults.
    let accent = get_gnome_accent().ok();

    let is_dark = Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "color-scheme"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                let val = String::from_utf8_lossy(&o.stdout).trim().to_lowercase();
                Some(val.contains("dark"))
            } else {
                None
            }
        })
        .unwrap_or(true);

    let (bg, bg_alt, view_bg, btn_bg, fg, fg_inactive) = if is_dark {
        (
            PaletteColor::new(36, 36, 36),
            PaletteColor::new(48, 48, 48),
            PaletteColor::new(30, 30, 30),
            PaletteColor::new(60, 60, 60),
            PaletteColor::new(255, 255, 255),
            PaletteColor::new(140, 140, 140),
        )
    } else {
        (
            PaletteColor::new(246, 245, 244),
            PaletteColor::new(235, 235, 235),
            PaletteColor::new(255, 255, 255),
            PaletteColor::new(225, 225, 225),
            PaletteColor::new(36, 36, 36),
            PaletteColor::new(120, 120, 120),
        )
    };

    Ok(SystemColorScheme {
        window_bg: Some(bg),
        window_bg_alt: Some(bg_alt),
        view_bg: Some(view_bg),
        button_bg: Some(btn_bg),
        header_bg: None,
        header_bg_inactive: None,
        tooltip_bg: None,
        window_fg: Some(fg),
        window_fg_inactive: Some(fg_inactive),
        view_fg: Some(fg),
        button_fg: Some(PaletteColor::new(255, 255, 255)),
        selection_bg: accent,
        selection_fg: Some(PaletteColor::new(255, 255, 255)),
        selection_hover: None,
        accent,
        fg_link: None,
        fg_negative: Some(PaletteColor::new(224, 27, 36)),
        fg_neutral: Some(PaletteColor::new(205, 147, 9)),
        fg_positive: Some(PaletteColor::new(38, 162, 105)),
        wm_active_bg: None,
        wm_active_fg: None,
        wm_inactive_bg: None,
    })
}

fn is_image_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.ends_with(".png")
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".webp")
        || lower.ends_with(".bmp")
        || lower.ends_with(".tiff")
        || lower.ends_with(".tif")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_de_does_not_panic() {
        let _de = detect_desktop_environment();
    }

    #[test]
    fn parse_gsettings_uri_variants() {
        assert_eq!(
            parse_gsettings_uri("'file:///home/user/wallpaper.jpg'"),
            Some("/home/user/wallpaper.jpg".to_string())
        );
        assert_eq!(
            parse_gsettings_uri("'file:///home/user/my%20wallpaper.png'"),
            Some("/home/user/my wallpaper.png".to_string())
        );
    }

    #[test]
    fn parse_file_uri_only_file_scheme() {
        assert_eq!(
            parse_file_uri("file:///home/user/pic.jpg"),
            Some("/home/user/pic.jpg".to_string())
        );
        assert_eq!(parse_file_uri("/just/a/path"), None);
    }

    #[test]
    fn parse_rgb_csv_ok() {
        assert_eq!(parse_rgb_csv("66,133,244").unwrap(), PaletteColor::new(66, 133, 244));
        assert_eq!(
            parse_rgb_csv(" 66 , 133 , 244 ").unwrap(),
            PaletteColor::new(66, 133, 244)
        );
    }

    #[test]
    fn parse_cosmic_color_float() {
        let color = parse_cosmic_color("(0.26, 0.52, 0.96, 1.0)").unwrap();
        assert_eq!(color.r, 66);
        assert_eq!(color.g, 133);
        assert_eq!(color.b, 245);
    }

    #[test]
    fn is_image_path_matches() {
        assert!(is_image_path("/home/user/wall.jpg"));
        assert!(is_image_path("/home/user/wall.PNG"));
        assert!(is_image_path("/home/user/wall.webp"));
        assert!(!is_image_path("/home/user/wall.mp4"));
    }
}
