//! Which theme is on: the choice Settings keeps, the list it offers, and the
//! two themes that are not a fixed palette. The auto theme is generated from
//! the desktop's colours, its wallpaper or an image; the custom one is
//! edited in Settings. Both are QBZ 2.1.2's (`theme_qt.rs`,
//! `custom_theme_qt.rs`), without Qt.

use super::{for_choice, parse_color, Theme};
use chilpa_theme::{palette, AutoSource, CustomThemeBase, ThemeColors, ThemeId};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

/// The theme chosen: a slug of QBZ's list, `auto` or `custom`.
pub const THEME: &str = "appearance.theme";
/// The accent that replaces a theme's own, empty to follow the theme.
pub const ACCENT: &str = "appearance.accent";
/// The list's filter: 0 every theme, 1 the dark ones, 2 the light ones.
pub const FILTER: &str = "appearance.themeFilter";
/// Where the auto theme takes its colours from: `system`, `wallpaper` or `image`.
pub const AUTO_SOURCE: &str = "appearance.autoThemeSource";
/// The image the auto theme reads when its source is `image`.
pub const AUTO_IMAGE: &str = "appearance.autoThemeImage";
/// Whether the boxes of the auto and custom options are folded.
pub const AUTO_FOLDED: &str = "appearance.themeAutoCollapsed";
pub const CUSTOM_FOLDED: &str = "appearance.themeCustomCollapsed";

pub const AUTO: &str = "auto";
pub const CUSTOM: &str = "custom";

/// The themes Chilpa draws from its own files. Their slugs are QBZ's core
/// four, so a choice kept by either reads the same.
const OWN: [&str; 4] = ["dark", "oled", "light", "system"];

/// The auto theme's sources, as kept and as Settings names them.
pub const AUTO_SOURCES: [&str; 3] = ["system", "wallpaper", "image"];

fn preference(key: &str) -> Option<String> {
    chilpa_core::settings::load().window.get(key).cloned()
}

/// One theme of the list in Settings.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Entry {
    pub slug: String,
    pub label: String,
    /// What the list's light/dark filter goes by.
    pub light: bool,
}

/// Every theme Settings offers, in QBZ's order: its 36, Chilpa's own four in
/// the place of QBZ's core four, then Auto and Custom.
pub fn entries() -> Vec<Entry> {
    let mut out: Vec<Entry> = chilpa_theme::theme_list()
        .into_iter()
        .map(|e| {
            let label = match e.slug {
                "dark" => chilpa_i18n::t("Dark"),
                "oled" => chilpa_i18n::t("Dark OLED"),
                "light" => chilpa_i18n::t("Light"),
                "system" => chilpa_i18n::t("System"),
                _ => e.display_name.to_string(),
            };
            Entry { slug: e.slug.to_string(), label, light: e.is_light }
        })
        .collect();
    out.push(Entry { slug: AUTO.into(), label: chilpa_i18n::t("Auto (dynamic)"), light: false });
    out.push(Entry { slug: CUSTOM.into(), label: chilpa_i18n::t("Custom"), light: false });
    out
}

/// The theme a choice stands for, with the chosen accent over it. "system"
/// follows the desktop, and so does a choice nobody knows. The custom theme
/// keeps its own accent: its editor is where that one is chosen.
pub fn resolve(choice: &str, system_dark: bool, accent: Option<&str>) -> Theme {
    let mut theme = match choice {
        AUTO => Theme::from_palette(AUTO, &auto::colors()),
        CUSTOM => Theme::from_palette(CUSTOM, &chilpa_theme::theme_from_base(&custom::base())),
        other => match ThemeId::from_slug(other).filter(|_| !OWN.contains(&other)) {
            Some(id) => Theme::from_palette(other, &palette(id)),
            None => Theme::built_in(for_choice(other, system_dark)),
        },
    };
    if choice != CUSTOM {
        theme.set_accent(accent.and_then(parse_color));
    }
    theme
}

/// The custom theme: eleven base colours and a polarity, kept in
/// `custom_theme.json` beside the library, everything else derived.
pub mod custom {
    use super::*;

    /// The base colours' keys, in the editor's reading order (QBZ's
    /// `TOKEN_KEYS`).
    pub const KEYS: [&str; 11] = [
        "surface-main",
        "surface-card",
        "surface-elevated",
        "text-primary",
        "text-secondary",
        "accent",
        "danger",
        "warning",
        "success",
        "border",
        "favorite",
    ];

    /// How long an edit waits before it is written: a drag sends one per
    /// frame, and its release writes at once anyway.
    const WRITE_DEBOUNCE: Duration = Duration::from_millis(250);

    static BASE: Mutex<Option<CustomThemeBase>> = Mutex::new(None);
    /// Bumped by every edit; the waiting writer goes when it stops moving.
    static WRITE_SEQ: AtomicU64 = AtomicU64::new(0);
    static DIRTY: AtomicBool = AtomicBool::new(false);
    static WAITING: AtomicBool = AtomicBool::new(false);

    pub fn path() -> PathBuf {
        chilpa_dirs::data().join("custom_theme.json")
    }

    fn lock() -> std::sync::MutexGuard<'static, Option<CustomThemeBase>> {
        BASE.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The file, when there is one that reads; a hand-edited file that does
    /// not parse is QBZ's OLED seed, never an error.
    fn read_file() -> Option<CustomThemeBase> {
        let text = std::fs::read_to_string(path()).ok()?;
        serde_json::from_str(&text).map_err(|e| eprintln!("chilpa-theme: custom_theme.json does not parse, using the default: {e}")).ok()
    }

    /// The base as it is being edited; read from the file once.
    pub fn base() -> CustomThemeBase {
        lock().get_or_insert_with(|| read_file().unwrap_or_else(CustomThemeBase::default_oled)).clone()
    }

    /// Read the file again the next time: done when Custom is chosen, so a
    /// file another Chilpa wrote is the one shown.
    pub fn forget() {
        *lock() = None;
    }

    pub fn exists() -> bool {
        path().is_file()
    }

    /// Written whole, through a temporary file beside it and a rename, so a
    /// reader never finds half of it.
    fn save(base: &CustomThemeBase) {
        use std::io::Write as _;
        let path = path();
        let Ok(text) = serde_json::to_string_pretty(base) else { return };
        if let Some(parent) = path.parent() {
            if std::fs::create_dir_all(parent).is_err() {
                return;
            }
        }
        let mut tmp = path.as_os_str().to_owned();
        tmp.push(format!(".{}.tmp", std::process::id()));
        let tmp = PathBuf::from(tmp);
        let written = std::fs::File::create(&tmp).and_then(|mut f| {
            f.write_all(text.as_bytes())?;
            f.sync_all()
        });
        if written.and_then(|_| std::fs::rename(&tmp, &path)).is_err() {
            let _ = std::fs::remove_file(&tmp);
            eprintln!("chilpa-theme: custom_theme.json could not be written");
        }
    }

    fn save_now() {
        DIRTY.store(false, Ordering::SeqCst);
        save(&base());
    }

    /// One writer waits until the edits stop for the debounce, then writes
    /// whatever the base is by then.
    fn schedule_save() {
        DIRTY.store(true, Ordering::SeqCst);
        WRITE_SEQ.fetch_add(1, Ordering::SeqCst);
        if WAITING.swap(true, Ordering::SeqCst) {
            return;
        }
        std::thread::spawn(|| {
            let mut seen = 0;
            loop {
                let now = WRITE_SEQ.load(Ordering::SeqCst);
                if now == seen {
                    break;
                }
                seen = now;
                std::thread::sleep(WRITE_DEBOUNCE);
            }
            WAITING.store(false, Ordering::SeqCst);
            if DIRTY.load(Ordering::SeqCst) {
                save_now();
            }
        });
    }

    /// Write now what is pending: a drag that ends, a colour typed in.
    pub fn flush() {
        WRITE_SEQ.fetch_add(1, Ordering::SeqCst);
        if DIRTY.load(Ordering::SeqCst) {
            save_now();
        }
    }

    fn field<'a>(base: &'a mut CustomThemeBase, key: &str) -> Option<&'a mut String> {
        Some(match key {
            "surface-main" => &mut base.surface_main,
            "surface-card" => &mut base.surface_card,
            "surface-elevated" => &mut base.surface_elevated,
            "text-primary" => &mut base.text_primary,
            "text-secondary" => &mut base.text_secondary,
            "accent" => &mut base.accent,
            "danger" => &mut base.danger,
            "warning" => &mut base.warning,
            "success" => &mut base.success,
            "border" => &mut base.border,
            "favorite" => &mut base.favorite,
            _ => return None,
        })
    }

    /// One base colour, from `#rrggbb` (an alpha is dropped). A colour half
    /// typed, or a key nobody knows, changes nothing. True when it changed.
    pub fn set(key: &str, hex: &str) -> bool {
        let Some(c) = chilpa_theme::Rgba::from_hex(hex) else { return false };
        let hex6 = chilpa_theme::Rgba::rgb(c.r, c.g, c.b).to_hex();
        {
            let mut guard = lock();
            let base = guard.get_or_insert_with(|| read_file().unwrap_or_else(CustomThemeBase::default_oled));
            let Some(slot) = field(base, key) else { return false };
            if *slot == hex6 {
                return false;
            }
            *slot = hex6;
        }
        schedule_save();
        true
    }

    /// The polarity: the base colours stay, what is derived flips. Written
    /// at once. True when it changed.
    pub fn set_dark(dark: bool) -> bool {
        {
            let mut guard = lock();
            let base = guard.get_or_insert_with(|| read_file().unwrap_or_else(CustomThemeBase::default_oled));
            if base.is_dark == dark {
                return false;
            }
            base.is_dark = dark;
        }
        DIRTY.store(true, Ordering::SeqCst);
        flush();
        true
    }

    /// The base becomes `seed` and is written at once: "Use current colors",
    /// and the first time Custom is chosen.
    pub fn seed(seed: CustomThemeBase) {
        *lock() = Some(seed.clone());
        DIRTY.store(false, Ordering::SeqCst);
        WRITE_SEQ.fetch_add(1, Ordering::SeqCst);
        save(&seed);
    }

    /// The editor's swatches: `{"dark": bool, "colors": {key: "#rrggbb"}}`.
    pub fn state() -> serde_json::Value {
        let mut base = base();
        let colors: serde_json::Map<String, serde_json::Value> = KEYS
            .iter()
            .map(|key| {
                let hex = field(&mut base, key).map(|v| v.clone()).unwrap_or_default();
                // A stored colour that does not parse shows as black.
                let shown = chilpa_theme::Rgba::from_hex(&hex).map_or_else(|| "#000000".to_string(), |c| chilpa_theme::Rgba::rgb(c.r, c.g, c.b).to_hex());
                (key.to_string(), serde_json::Value::String(shown))
            })
            .collect();
        serde_json::json!({ "dark": base.is_dark, "colors": colors })
    }
}

/// The auto theme: generated off the window's thread, kept in memory and in
/// the cache, so it is on screen from the first frame and regenerated after.
pub mod auto {
    use super::*;

    static COLORS: Mutex<Option<ThemeColors>> = Mutex::new(None);
    /// Bumped by each generation started; only the newest one is kept.
    static RUN: AtomicU64 = AtomicU64::new(0);

    fn cache_path() -> PathBuf {
        chilpa_dirs::cache().join("auto_theme.txt")
    }

    /// Where the colours come from now, as kept: anything unknown is the
    /// system's (QBZ's `auto_source`).
    pub fn source() -> AutoSource {
        match preference(AUTO_SOURCE).as_deref() {
            Some("wallpaper") => AutoSource::Wallpaper,
            Some("image") => AutoSource::Image(preference(AUTO_IMAGE).unwrap_or_default()),
            _ => AutoSource::System,
        }
    }

    /// The auto theme's colours: the last ones generated, else the ones the
    /// cache kept from the run before, else OLED Black, what QBZ shows when
    /// generation fails.
    pub fn colors() -> ThemeColors {
        let mut guard = COLORS.lock().unwrap_or_else(|e| e.into_inner());
        *guard.get_or_insert_with(|| {
            std::fs::read_to_string(cache_path())
                .ok()
                .and_then(|text| ThemeColors::from_hex_list(&text))
                .unwrap_or_else(|| palette(chilpa_theme::default_theme_id()))
        })
    }

    /// The desktop's wallpaper: the resolver the wallpaper background uses,
    /// then QBZ's own readers for the desktops it does not know.
    fn wallpaper() -> Result<String, String> {
        match crate::wallpaper::resolve_system() {
            Some(path) => Ok(path.to_string_lossy().into_owned()),
            None => chilpa_theme::auto::get_system_wallpaper(),
        }
    }

    /// Generates the auto theme again from its source, on a thread of its
    /// own, and calls `done` once the colours are in place. A failure keeps
    /// the theme selectable and shows OLED Black, as QBZ does.
    pub fn regenerate(done: impl FnOnce() + Send + 'static) {
        let run = RUN.fetch_add(1, Ordering::SeqCst) + 1;
        let source = source();
        std::thread::spawn(move || {
            let colors = match chilpa_theme::generate_auto_theme(&source, wallpaper) {
                Ok(colors) => {
                    let _ = std::fs::create_dir_all(chilpa_dirs::cache());
                    let _ = std::fs::write(cache_path(), colors.to_hex_list());
                    colors
                }
                Err(error) => {
                    eprintln!("chilpa-theme: the auto theme could not be generated ({error}); showing OLED Black");
                    palette(chilpa_theme::default_theme_id())
                }
            };
            if RUN.load(Ordering::SeqCst) != run {
                return;
            }
            *COLORS.lock().unwrap_or_else(|e| e.into_inner()) = Some(colors);
            done();
        });
    }

    /// The desktop the auto theme reads, by name; empty when it has none.
    pub fn desktop() -> String {
        chilpa_theme::auto::detect_desktop_environment().display_name().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_puts_the_own_four_first_then_auto_and_custom() {
        let list = entries();
        assert_eq!(list.len(), chilpa_theme::ALL.len() + 2);
        let slugs: Vec<&str> = list.iter().map(|e| e.slug.as_str()).collect();
        assert_eq!(&slugs[..4], ["dark", "oled", "light", "system"]);
        assert_eq!(&slugs[slugs.len() - 2..], [AUTO, CUSTOM]);
        assert!(list.iter().find(|e| e.slug == "light").unwrap().light);
        assert!(!list.iter().find(|e| e.slug == "frost").unwrap().light, "Frost is a dark canvas");
        assert!(list.iter().find(|e| e.slug == "alucard").unwrap().light);
        assert_eq!(list.iter().find(|e| e.slug == "tokyo-night").unwrap().label, "Tokyo Night");
    }

    #[test]
    fn the_own_four_stay_files_and_the_rest_are_palettes() {
        assert_eq!(resolve("oled", false, None).id(), "dark-oled");
        assert_eq!(resolve("dark", false, None).id(), "breeze-dark");
        assert_eq!(resolve("system", true, None).id(), "breeze-dark");
        assert_eq!(resolve("gone", false, None).id(), "breeze-light");
        let nord = resolve("nord", false, None);
        assert_eq!(nord.id(), "nord");
        assert!(nord.palette().is_some());
    }

    #[test]
    fn the_chosen_accent_goes_over_every_theme_but_custom() {
        let accent = super::parse_color("#3DAEE9");
        assert_eq!(resolve("dracula", false, Some("#3DAEE9")).color("accent"), accent);
        assert_eq!(resolve("light", false, Some("#3DAEE9")).color("accent"), accent);
        let custom = resolve(CUSTOM, false, Some("#3DAEE9"));
        assert_ne!(custom.color("accent"), accent, "the editor chooses the custom accent");
    }
}
