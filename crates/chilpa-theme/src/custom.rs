//! User-authored custom theme: derive a full [`ThemeColors`] set from a small
//! set of hand-picked base tokens.
//!
//! Ported 1:1 from QBZ 2.1.2 (`qbz-theme/src/custom.rs`). The user edits
//! eleven semantic BASE tokens and a polarity; the whole rest of the contract
//! is DERIVED from them (accent triplet, status families, muted/disabled text
//! tiers, focus ring, translucent edges, polarity alpha ramp), with the same
//! math the auto-theme generator and the static registry use
//! (`generator::{tint, pick_btn_text_for_accent_set, ensure_text_contrast_target}`,
//! `PaletteColor::shift_lightness`, `alpha_ramp`).
//!
//! Colors are stored as `#rrggbb` HEX STRINGS (opaque; alpha is never part of a
//! base token): the on-disk `custom_theme.json` stays readable by hand, and
//! the value round-trips 1:1 with the picker's HEX field. Malformed strings
//! fall back to the dark-theme default for that token, so a hand-edited file
//! can never panic the app.

use crate::auto::generator::{ensure_text_contrast_target, pick_btn_text_for_accent_set, tint};
use crate::auto::PaletteColor;
use crate::color::Rgba;
use crate::colors::{alpha_ramp, ThemeColors};
use crate::id::ThemeId;

use serde::{Deserialize, Serialize};

/// Legacy card shadow (`rgba(0,0,0,0.4)`), identical to the registry/generator
/// constant so a custom theme drops the same shadow as every other theme.
const CARD_SHADOW: Rgba = Rgba::rgba(0, 0, 0, 0x66);

/// The user-editable BASE of a custom theme. Twelve semantic tokens the editor
/// exposes as swatches; everything else in [`ThemeColors`] is derived from these
/// by [`theme_from_base`]. All colors are opaque `#rrggbb` hex strings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomThemeBase {
    /// Polarity. Drives the alpha-ramp base (white on dark / black on light), the
    /// translucent edge/hover bases, and the direction every derived shift takes.
    pub is_dark: bool,

    // --- surfaces (SURFACES group in the editor) ---
    pub surface_main: String,
    pub surface_card: String,
    pub surface_elevated: String,

    // --- text (TEXT group) ---
    pub text_primary: String,
    pub text_secondary: String,

    // --- accent (ACCENT group) ---
    pub accent: String,

    // --- status (STATUS group) ---
    pub danger: String,
    pub warning: String,
    pub success: String,

    // --- other (OTHER group) ---
    pub border: String,
    pub favorite: String,
}

// --- hex <-> color plumbing --------------------------------------------------

/// Parse an opaque `#rrggbb` token, dropping any alpha, falling back to `fallback`
/// on malformed input (a hand-edited JSON can never crash the theme pipeline).
fn parse(hex: &str, fallback: Rgba) -> Rgba {
    Rgba::from_hex(hex)
        .map(|c| Rgba::rgb(c.r, c.g, c.b))
        .unwrap_or(fallback)
}

fn to_pal(c: Rgba) -> PaletteColor {
    PaletteColor::new(c.r, c.g, c.b)
}

fn from_pal(c: PaletteColor) -> Rgba {
    Rgba::rgb(c.r, c.g, c.b)
}

impl CustomThemeBase {
    /// The seed a fresh custom theme starts from: the default OLED Black palette
    /// reduced to its base tokens. Used when the user first selects "Custom" and
    /// no `custom_theme.json` exists yet.
    pub fn default_oled() -> Self {
        base_from_theme(&crate::registry::palette(ThemeId::Oled), true)
    }
}

/// Reduce an existing fully-materialized [`ThemeColors`] to its editable base
/// tokens — the "Start from current theme" seed. Reads the tokens that
/// [`theme_from_base`] treats as inputs, so `theme_from_base(base_from_theme(c))`
/// reproduces every base token exactly for any theme this module authored.
///
/// `border` reads `border_subtle` when it is opaque (custom themes always are),
/// else falls back to the opaque `border_strong` — the four legacy P1 themes
/// store a translucent-white hairline in `border_subtle`, which would seed as a
/// jarring pure-white edge.
pub fn base_from_theme(colors: &ThemeColors, is_dark: bool) -> CustomThemeBase {
    let border = if colors.border_subtle.a == 255 {
        colors.border_subtle
    } else {
        colors.border_strong
    };
    CustomThemeBase {
        is_dark,
        surface_main: colors.surface_main.to_hex(),
        surface_card: colors.surface_card.to_hex(),
        surface_elevated: colors.surface_elevated.to_hex(),
        text_primary: colors.text_primary.to_hex(),
        text_secondary: colors.text_secondary.to_hex(),
        accent: colors.accent.to_hex(),
        danger: colors.danger.to_hex(),
        warning: colors.warning.to_hex(),
        success: colors.success.to_hex(),
        border: border.to_hex(),
        favorite: colors.favorite.to_hex(),
    }
}

/// Derive the complete [`ThemeColors`] contract from the base tokens.
///
/// Derivation table (source → rule; polarity is `base.is_dark`):
///   surface_main/card/elevated  base            direct
///   surface_hover               polarity        white|black @ ~6%  (0x10)
///   bg_hover                    surface_main    shift_lightness(±0.06)
///   text_primary/secondary      base            direct
///   text_muted                  text_primary    shift(∓0.25) then contrast ≥ 3.0 vs surface_main
///   text_disabled               text_muted      shift(∓0.10)
///   accent                      base            direct
///   accent_hover                accent          shift_lightness(+0.10)
///   accent_pressed              accent          shift_lightness(-0.10)
///   accent_text                 accent triplet  worst-case white/black contrast pick
///   danger/warning/success      base            direct
///   *_bg / *_border / *_hover   the hue         tint 0.1 / 0.3 / (0.2 dark | 0.15 light)
///   border_subtle               base border     direct
///   border_strong               base border     shift_lightness(±0.08)
///   border_muted                polarity        white|black @ ~22% (0x38)
///   focus_ring                  accent          = accent
///   favorite                    base            direct
///   card_shadow                 const           #00000066
///   alpha[]                     polarity        alpha_ramp(is_light)
pub fn theme_from_base(base: &CustomThemeBase) -> ThemeColors {
    let is_dark = base.is_dark;
    let is_light = !is_dark;

    // Base surfaces + hues (opaque). Fallbacks are the :root Dark values.
    let surface_main = parse(&base.surface_main, Rgba::rgb(0x0f, 0x0f, 0x0f));
    let surface_card = parse(&base.surface_card, Rgba::rgb(0x1a, 0x1a, 0x1a));
    let surface_elevated = parse(&base.surface_elevated, Rgba::rgb(0x2a, 0x2a, 0x2a));
    let text_primary = parse(&base.text_primary, Rgba::rgb(0xff, 0xff, 0xff));
    let text_secondary = parse(&base.text_secondary, Rgba::rgb(0xcc, 0xcc, 0xcc));
    let accent = parse(&base.accent, Rgba::rgb(0x42, 0x85, 0xf4));
    let danger = parse(&base.danger, Rgba::rgb(0xef, 0x44, 0x44));
    let warning = parse(&base.warning, Rgba::rgb(0xfb, 0xbf, 0x24));
    let success = parse(&base.success, Rgba::rgb(0x3f, 0xae, 0x6a));
    let border = parse(&base.border, Rgba::rgb(0x3a, 0x3a, 0x3a));
    let favorite = parse(&base.favorite, danger);

    let sm_pal = to_pal(surface_main);

    // Opaque hover background: nudge the main surface toward the elevated tier.
    let bg_hover = from_pal(sm_pal.shift_lightness(if is_dark { 0.06 } else { -0.06 }));

    // Text tiers derived from text_primary, contrast-enforced vs the main surface
    // (muted must clear >= 3:1; disabled is intentionally lower — a visual cue).
    let tp_pal = to_pal(text_primary);
    let muted_raw = tp_pal.shift_lightness(if is_dark { -0.25 } else { 0.25 });
    let text_muted = from_pal(ensure_text_contrast_target(muted_raw, &sm_pal, is_dark, 3.0));
    let text_disabled =
        from_pal(to_pal(text_muted).shift_lightness(if is_dark { -0.10 } else { 0.10 }));

    // Accent triplet + contrast-picked text (worst case across the triplet).
    let acc_pal = to_pal(accent);
    let accent_hover = from_pal(acc_pal.shift_lightness(0.10));
    let accent_pressed = from_pal(acc_pal.shift_lightness(-0.10));
    let accent_text = from_pal(pick_btn_text_for_accent_set(
        &acc_pal,
        &to_pal(accent_hover),
        &to_pal(accent_pressed),
    ));

    // Borders: subtle = the base token, strong = a polarity-aware shift of it.
    let border_subtle = border;
    let border_strong = from_pal(to_pal(border).shift_lightness(if is_dark { 0.08 } else { -0.08 }));

    // Polarity translucent edges (white base on dark, black base on light) — the
    // exact registry/generator pattern.
    let (eh, eg, eb) = if is_light { (0, 0, 0) } else { (255, 255, 255) };
    let surface_hover = Rgba::rgba(eh, eg, eb, 0x10); // ~6%
    let border_muted = Rgba::rgba(eh, eg, eb, 0x38); // ~22%

    // One hover strength for the whole status group (0.2 dark / 0.15 light).
    let status_hover = if is_dark { 0.2 } else { 0.15 };

    ThemeColors {
        surface_main,
        surface_card,
        surface_elevated,
        surface_hover,
        bg_hover,

        text_primary,
        text_secondary,
        text_muted,
        text_disabled,

        accent,
        accent_hover,
        accent_pressed,
        accent_text,

        danger,
        danger_bg: tint(danger, 0.1),
        danger_border: tint(danger, 0.3),
        danger_hover: tint(danger, status_hover),

        warning,
        warning_bg: tint(warning, 0.1),
        warning_border: tint(warning, 0.3),
        warning_hover: tint(warning, status_hover),

        success,
        success_bg: tint(success, 0.1),
        success_border: tint(success, 0.3),
        success_hover: tint(success, status_hover),

        border_subtle,
        border_muted,
        border_strong,

        focus_ring: accent,

        favorite,
        card_shadow: CARD_SHADOW,

        alpha: alpha_ramp(is_light),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::contrast_ratio;
    use crate::colors::ALPHA_COUNT;

    #[test]
    fn default_seed_is_oled_dark() {
        let base = CustomThemeBase::default_oled();
        assert!(base.is_dark);
        // OLED surfaces are pure/near black.
        assert_eq!(base.surface_main, "#000000");
        assert_eq!(base.surface_card, "#0a0a0a");
        assert_eq!(base.accent, "#4285f4");
    }

    #[test]
    fn base_tokens_map_straight_through() {
        let base = CustomThemeBase::default_oled();
        let c = theme_from_base(&base);
        assert_eq!(c.surface_main, Rgba::rgb(0, 0, 0));
        assert_eq!(c.accent, Rgba::rgb(0x42, 0x85, 0xf4));
        assert_eq!(c.danger, Rgba::rgb(0xef, 0x44, 0x44));
        // focus_ring == accent; favorite is its own base token.
        assert_eq!(c.focus_ring, c.accent);
        assert_eq!(c.favorite, parse(&base.favorite, Rgba::rgb(0, 0, 0)));
    }

    #[test]
    fn derived_status_families_have_expected_tints() {
        let c = theme_from_base(&CustomThemeBase::default_oled());
        // dark => hover 0.2
        assert_eq!(c.danger_bg.a, (0.1f32 * 255.0 + 0.5) as u8);
        assert_eq!(c.danger_border.a, (0.3f32 * 255.0 + 0.5) as u8);
        assert_eq!(c.danger_hover.a, (0.2f32 * 255.0 + 0.5) as u8);
        assert_eq!(c.danger_bg.r, c.danger.r);
        // whole status group shares one hover strength
        assert_eq!(c.success_hover.a, c.danger_hover.a);
        assert_eq!(c.warning_hover.a, c.danger_hover.a);
    }

    #[test]
    fn polarity_drives_alpha_and_edges() {
        let dark = theme_from_base(&CustomThemeBase::default_oled());
        assert_eq!(dark.alpha.len(), ALPHA_COUNT);
        assert_eq!(dark.alpha[ALPHA_COUNT - 1].r, 255); // dark -> white base
        assert_eq!(dark.surface_hover, Rgba::rgba(255, 255, 255, 0x10));
        assert_eq!(dark.border_muted, Rgba::rgba(255, 255, 255, 0x38));

        let mut light_base = CustomThemeBase::default_oled();
        light_base.is_dark = false;
        light_base.surface_main = "#ffffff".into();
        light_base.text_primary = "#0f0f0f".into();
        let light = theme_from_base(&light_base);
        assert_eq!(light.alpha[ALPHA_COUNT - 1].r, 0); // light -> black base
        assert_eq!(light.surface_hover, Rgba::rgba(0, 0, 0, 0x10));
        // light hover strength is 0.15
        assert_eq!(light.danger_hover.a, (0.15f32 * 255.0 + 0.5) as u8);
    }

    #[test]
    fn seed_derive_roundtrip_is_coherent() {
        // For any theme this module authored, reducing the derived colors back to
        // a base reproduces every base token exactly (idempotent seed).
        let base = CustomThemeBase::default_oled();
        let derived = theme_from_base(&base);
        let base2 = base_from_theme(&derived, base.is_dark);
        assert_eq!(base2, base);
    }

    #[test]
    fn deterministic() {
        let base = CustomThemeBase::default_oled();
        assert_eq!(theme_from_base(&base), theme_from_base(&base));
    }

    #[test]
    fn accent_text_is_legible() {
        let c = theme_from_base(&CustomThemeBase::default_oled());
        // The picked accent-text is pure white or pure black...
        assert!(c.accent_text == Rgba::rgb(255, 255, 255) || c.accent_text == Rgba::rgb(0, 0, 0));
        // ...and it is the more legible of the two against the accent fill.
        let white = Rgba::rgb(255, 255, 255);
        let black = Rgba::rgb(0, 0, 0);
        let picked = contrast_ratio(c.accent_text, c.accent);
        let other = if c.accent_text == white {
            contrast_ratio(black, c.accent)
        } else {
            contrast_ratio(white, c.accent)
        };
        // pick_btn prefers white when it clears 3:1; otherwise the higher one.
        assert!(picked >= 3.0 || picked >= other);
    }

    #[test]
    fn malformed_hex_falls_back_not_panics() {
        let mut base = CustomThemeBase::default_oled();
        base.accent = "not-a-color".into();
        let c = theme_from_base(&base);
        // Falls back to the :root Dark accent.
        assert_eq!(c.accent, Rgba::rgb(0x42, 0x85, 0xf4));
    }

    #[test]
    fn json_roundtrip() {
        let base = CustomThemeBase::default_oled();
        let json = serde_json::to_string(&base).unwrap();
        let back: CustomThemeBase = serde_json::from_str(&json).unwrap();
        assert_eq!(back, base);
    }
}
