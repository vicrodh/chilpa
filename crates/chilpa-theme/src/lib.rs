//! The theme palettes Chilpa takes from QBZ 2.1.2 (`qbz-theme`), with the
//! colour math they are built with.
//!
//! Pure Rust data and hand-rolled colour and contrast math: no Qt, no I/O
//! besides what the auto theme reads from the desktop, so it compiles and
//! tests on its own on every platform.
//!
//! A [`ThemeId`] maps to one fully-materialized [`ThemeColors`] (every field
//! populated, no cascade). The facade turns that into the window's tokens.
//! Two themes are not rows of the registry: the auto theme, generated from
//! the desktop's colours, its wallpaper or an image ([`auto`]), and the one
//! the user edits ([`custom`]).

pub mod auto;
mod color;
mod colors;
pub mod custom;
mod id;
mod registry;

pub use auto::{generate as generate_auto_theme, AutoSource};
pub use color::{apca_lc, contrast_ratio, relative_luminance, Rgba};
pub use colors::{alpha_byte, alpha_index, alpha_ramp, ThemeColors, ALPHA_COUNT, ALPHA_PERCENTS};
pub use custom::{base_from_theme, theme_from_base, CustomThemeBase};
pub use id::{default_slug, ThemeCategory, ThemeId, ALL};
pub use registry::palette;

/// A single entry in the Settings theme list: the stable id plus the data the
/// dropdown needs (display name, category, light/dark, ready flag).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemeListEntry {
    pub id: ThemeId,
    pub display_name: &'static str,
    pub slug: &'static str,
    pub category: ThemeCategory,
    /// Luminance-derived: `true` when the theme's `surface_main` is light.
    /// Drives the dark/light list filter.
    pub is_light: bool,
    /// Whether the registry fully materializes this row.
    pub implemented: bool,
}

/// QBZ's default theme on a fresh profile: OLED Black. It is what a failed
/// auto theme falls back to and what a fresh custom theme starts from.
pub fn default_theme_id() -> ThemeId {
    ThemeId::default_id()
}

/// Whether a theme reads as "light" from its actual base surface luminance
/// (Frost and Langley were registered light in QBZ's first frontend but are
/// visually dark; Alucard is genuinely light).
pub fn is_light(id: ThemeId) -> bool {
    // System has no static palette; treat it as dark for filter purposes (it
    // follows the desktop at runtime).
    if id == ThemeId::System {
        return false;
    }
    relative_luminance(palette(id).surface_main) >= 0.5
}

/// Whether a theme is one of the two High-Contrast accessibility themes.
pub fn is_high_contrast(id: ThemeId) -> bool {
    matches!(id, ThemeId::HighContrast | ThemeId::HighContrastLight)
}

/// The full Settings theme list in display order. The window filters it by
/// `is_light`.
pub fn theme_list() -> Vec<ThemeListEntry> {
    ALL.iter()
        .map(|&id| ThemeListEntry {
            id,
            display_name: id.display_name(),
            slug: id.slug(),
            category: id.category(),
            is_light: is_light(id),
            implemented: id.is_implemented(),
        })
        .collect()
}

/// The implemented-only theme list.
pub fn implemented_theme_list() -> Vec<ThemeListEntry> {
    theme_list().into_iter().filter(|e| e.implemented).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_oled() {
        assert_eq!(default_theme_id(), ThemeId::Oled);
    }

    #[test]
    fn registry_returns_populated_struct_for_default() {
        let c = palette(default_theme_id());
        // OLED: pure-black base, white text, full alpha ramp.
        assert_eq!(c.surface_main, Rgba::rgb(0, 0, 0));
        assert_eq!(c.text_primary, Rgba::rgb(0xff, 0xff, 0xff));
        assert_eq!(c.alpha.len(), ALPHA_COUNT);
    }

    #[test]
    fn light_dark_flag_from_luminance() {
        // The 4 P1 themes are all dark.
        assert!(!is_light(ThemeId::Dark));
        assert!(!is_light(ThemeId::Oled));
        assert!(!is_light(ThemeId::TokyoNight));
        assert!(!is_light(ThemeId::System));
    }

    #[test]
    fn implemented_list_is_every_theme() {
        // After P3 every theme is materialized — the standard rows AND the 5
        // redesigned accessibility themes (WcagLight/WcagDark/HighContrast/
        // HighContrastLight/Colorblind).
        let list = implemented_theme_list();
        assert_eq!(list.len(), ALL.len());
        let slugs: Vec<&str> = list.iter().map(|e| e.slug).collect();
        // P1 originals still present:
        assert!(slugs.contains(&"dark"));
        assert!(slugs.contains(&"oled"));
        assert!(slugs.contains(&"tokyo-night"));
        assert!(slugs.contains(&"system"));
        // P2 additions (spot-check across categories):
        assert!(slugs.contains(&"light"));
        assert!(slugs.contains(&"nord"));
        assert!(slugs.contains(&"dracula"));
        assert!(slugs.contains(&"frost"));
        assert!(slugs.contains(&"langley"));
        assert!(slugs.contains(&"alucard"));
        assert!(slugs.contains(&"kurosaki"));
        // P3 accessibility themes now implemented:
        assert!(slugs.contains(&"wcag-light"));
        assert!(slugs.contains(&"wcag-dark"));
        assert!(slugs.contains(&"high-contrast"));
        assert!(slugs.contains(&"high-contrast-light"));
        assert!(slugs.contains(&"colorblind"));
    }

    #[test]
    fn light_dark_filter_is_luminance_correct() {
        // Corrected flags: Alucard light; Frost/Langley dark despite Tauri type.
        assert!(is_light(ThemeId::Alucard));
        assert!(is_light(ThemeId::Light));
        assert!(is_light(ThemeId::SnowStorm));
        assert!(!is_light(ThemeId::Frost));
        assert!(!is_light(ThemeId::Langley));
        assert!(!is_light(ThemeId::Nord));
    }

    #[test]
    fn full_list_has_all_entries() {
        assert_eq!(theme_list().len(), ALL.len());
    }

    #[test]
    fn high_contrast_flag_only_for_hc_themes() {
        // True for exactly the two High-Contrast themes.
        assert!(is_high_contrast(ThemeId::HighContrast));
        assert!(is_high_contrast(ThemeId::HighContrastLight));
        // False for everything else, including the other a11y themes.
        assert!(!is_high_contrast(ThemeId::WcagLight));
        assert!(!is_high_contrast(ThemeId::WcagDark));
        assert!(!is_high_contrast(ThemeId::Colorblind));
        assert!(!is_high_contrast(ThemeId::Dark));
        assert!(!is_high_contrast(ThemeId::Oled));
        assert!(!is_high_contrast(ThemeId::System));
        // Exactly two themes in ALL are high-contrast.
        assert_eq!(ALL.iter().filter(|&&id| is_high_contrast(id)).count(), 2);
    }
}
