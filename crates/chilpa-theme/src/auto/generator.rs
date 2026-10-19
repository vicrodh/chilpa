//! Assemble a full [`ThemeColors`] from an extracted palette or a DE color
//! scheme.
//!
//! Ported 1:1 from QBZ 2.1.2 (`qbz-theme/src/auto/generator.rs`): text-
//! contrast enforcement, the accent triplet, the status hues and the border
//! shifts, populating [`ThemeColors`] and deriving `success`, `focus_ring`,
//! `favorite`, `border_muted` and the polarity alpha ramp exactly as
//! `registry::StdSpec::build` does, so a generated theme composites
//! identically to a static one.

use super::{PaletteColor, SystemColorScheme, ThemePalette};
use crate::colors::{alpha_ramp, ThemeColors};
use crate::color::Rgba;

/// Legacy card shadow (`rgba(0,0,0,0.4)`), identical to the registry constant so
/// generated themes drop the same shadow as static ones.
const CARD_SHADOW: Rgba = Rgba::rgba(0, 0, 0, 0x66);

/// Convert an opaque palette color to `Rgba`.
fn opaque(c: PaletteColor) -> Rgba {
    Rgba::rgb(c.r, c.g, c.b)
}

/// Straight-alpha overlay of an opaque hue at `frac` opacity (0.0..=1.0).
/// Mirrors `registry::with_alpha` / Tauri's `rgba(hue, frac)` tint. Shared with
/// the custom-theme builder (`crate::custom`) so the status-family tint shape is
/// defined once.
pub(crate) fn tint(c: Rgba, frac: f32) -> Rgba {
    let a = (frac * 255.0 + 0.5) as u8;
    Rgba::rgba(c.r, c.g, c.b, a)
}

/// Assemble the derived families shared by both entry points, given the solid
/// surface/text/accent/status colors already chosen. `is_dark` drives polarity
/// (alpha ramp base, translucent edges, success hue). `status_hover` is the
/// per-polarity hover alpha the Tauri generator uses (0.2 dark / 0.15 light) —
/// applied to danger/warning AND the derived success family so the whole status
/// group shares one hover strength.
#[allow(clippy::too_many_arguments)]
fn assemble(
    is_dark: bool,
    surface_main: Rgba,
    surface_card: Rgba,
    surface_elevated: Rgba,
    bg_hover: Rgba,
    text_primary: Rgba,
    text_secondary: Rgba,
    text_muted: Rgba,
    text_disabled: Rgba,
    accent: Rgba,
    accent_hover: Rgba,
    accent_pressed: Rgba,
    accent_text: Rgba,
    danger: Rgba,
    warning: Rgba,
    status_hover: f32,
    border_subtle: Rgba,
    border_strong: Rgba,
) -> ThemeColors {
    let is_light = !is_dark;

    // Polarity-aware translucent edges + hover overlay (legacy Slint-only
    // tokens, no CSS-map parity). White base on dark, black base on light —
    // identical to `StdSpec::build`.
    let (eh, eg, eb) = if is_light { (0, 0, 0) } else { (255, 255, 255) };
    let surface_hover = Rgba::rgba(eh, eg, eb, 0x10); // ~6%
    let border_muted = Rgba::rgba(eh, eg, eb, 0x38); // ~22%

    // success: NEW token (no Tauri parity). Same theme-green the registry uses,
    // darker on light canvases so it clears >=3:1. Tint shape follows the status
    // family (bg 0.1 / border 0.3 / hover = status_hover).
    let success = if is_light {
        Rgba::rgb(0x1f, 0x8a, 0x4c)
    } else {
        Rgba::rgb(0x3f, 0xae, 0x6a)
    };

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

        focus_ring: accent, // = accent (matches registry / P1)
        favorite: danger,   // loved-heart uses danger red (matches registry)
        card_shadow: CARD_SHADOW,

        alpha: alpha_ramp(is_light), // black base on light, white base on dark
    }
}

/// Build a [`ThemeColors`] from a k-means–extracted image/wallpaper palette.
///
/// Port of `auto_theme::generator::generate_theme`.
pub fn theme_from_palette(palette: &ThemePalette) -> ThemeColors {
    let is_dark = palette.is_dark;

    // Text tiers by polarity, then contrast-enforced against bg_primary. Disabled
    // stays intentionally low-contrast (a visual cue), so it is not adjusted.
    let (text_primary, text_secondary, text_muted, text_disabled) = if is_dark {
        (
            PaletteColor::new(255, 255, 255),
            PaletteColor::new(204, 204, 204),
            PaletteColor::new(136, 136, 136),
            PaletteColor::new(85, 85, 85),
        )
    } else {
        (
            PaletteColor::new(15, 15, 15),
            PaletteColor::new(68, 68, 68),
            PaletteColor::new(102, 102, 102),
            PaletteColor::new(153, 153, 153),
        )
    };
    let text_primary = ensure_text_contrast(text_primary, &palette.bg_primary, is_dark);
    let text_secondary =
        ensure_text_contrast_target(text_secondary, &palette.bg_primary, is_dark, 4.5);
    let text_muted = ensure_text_contrast_target(text_muted, &palette.bg_primary, is_dark, 3.0);

    // Accent triplet — hover +10% L, active -10% L. Text picked across the whole
    // triplet so hover/active stay legible.
    let accent = palette.accent;
    let accent_hover = accent.shift_lightness(0.10);
    let accent_active = accent.shift_lightness(-0.10);
    let accent_text = pick_btn_text_for_accent_set(&accent, &accent_hover, &accent_active);

    // Status hues by polarity (identical to generate_theme).
    let (danger, warning) = if is_dark {
        (PaletteColor::new(239, 68, 68), PaletteColor::new(251, 191, 36))
    } else {
        (PaletteColor::new(220, 38, 38), PaletteColor::new(217, 119, 6))
    };
    let status_hover = if is_dark { 0.2 } else { 0.15 };

    // Borders: subtle/strong lightness shifts from bg_primary.
    let border_subtle = if is_dark {
        palette.bg_primary.shift_lightness(0.08)
    } else {
        palette.bg_primary.shift_lightness(-0.08)
    };
    let border_strong = if is_dark {
        palette.bg_primary.shift_lightness(0.14)
    } else {
        palette.bg_primary.shift_lightness(-0.14)
    };

    assemble(
        is_dark,
        opaque(palette.bg_primary),
        opaque(palette.bg_secondary),
        opaque(palette.bg_tertiary),
        opaque(palette.bg_hover),
        opaque(text_primary),
        opaque(text_secondary),
        opaque(text_muted),
        opaque(text_disabled),
        opaque(accent),
        opaque(accent_hover),
        opaque(accent_active),
        opaque(accent_text),
        opaque(danger),
        opaque(warning),
        status_hover,
        opaque(border_subtle),
        opaque(border_strong),
    )
}

/// Build a [`ThemeColors`] directly from a DE color scheme (KDE/GNOME).
///
/// Port of `auto_theme::generator::generate_theme_from_scheme`.
pub fn theme_from_scheme(scheme: &SystemColorScheme) -> ThemeColors {
    let window_bg = scheme.window_bg.unwrap_or(PaletteColor::new(40, 40, 40));
    let is_dark = window_bg.luminance() < 0.5;

    // Surfaces
    let bg_secondary = scheme.view_bg.unwrap_or_else(|| {
        if is_dark {
            window_bg.shift_lightness(0.03)
        } else {
            window_bg.shift_lightness(-0.03)
        }
    });
    let bg_tertiary = scheme.button_bg.unwrap_or_else(|| {
        if is_dark {
            window_bg.shift_lightness(0.10)
        } else {
            window_bg.shift_lightness(-0.10)
        }
    });
    let bg_hover = scheme.window_bg_alt.unwrap_or_else(|| {
        PaletteColor::new(
            ((window_bg.r as u16 + bg_secondary.r as u16) / 2) as u8,
            ((window_bg.g as u16 + bg_secondary.g as u16) / 2) as u8,
            ((window_bg.b as u16 + bg_secondary.b as u16) / 2) as u8,
        )
    });

    // Text
    let text_primary = scheme.window_fg.unwrap_or(if is_dark {
        PaletteColor::new(223, 223, 223)
    } else {
        PaletteColor::new(36, 36, 36)
    });
    let text_primary = ensure_text_contrast(text_primary, &window_bg, is_dark);

    let text_secondary_raw = scheme
        .view_fg
        .unwrap_or_else(|| text_primary.shift_lightness(if is_dark { -0.10 } else { 0.10 }));
    let text_secondary = ensure_text_contrast_target(text_secondary_raw, &window_bg, is_dark, 4.5);

    let text_muted_raw = scheme
        .window_fg_inactive
        .unwrap_or_else(|| text_primary.shift_lightness(if is_dark { -0.25 } else { 0.25 }));
    let text_muted = ensure_text_contrast_target(text_muted_raw, &window_bg, is_dark, 3.0);

    let text_disabled = text_muted.shift_lightness(if is_dark { -0.10 } else { 0.10 });

    // Accent triplet (selection)
    let accent = scheme
        .accent
        .or(scheme.selection_bg)
        .unwrap_or(PaletteColor::new(0, 120, 215));
    let accent_hover = scheme
        .selection_hover
        .unwrap_or_else(|| accent.shift_lightness(0.10));
    let accent_active = accent.shift_lightness(-0.10);
    // Trust DE selection_fg if present, else compute across the triplet.
    let accent_text = scheme
        .selection_fg
        .unwrap_or_else(|| pick_btn_text_for_accent_set(&accent, &accent_hover, &accent_active));

    // Status hues from system negative/neutral, else polarity fallbacks.
    let danger = scheme.fg_negative.unwrap_or(if is_dark {
        PaletteColor::new(239, 68, 68)
    } else {
        PaletteColor::new(220, 38, 38)
    });
    let warning = scheme.fg_neutral.unwrap_or(if is_dark {
        PaletteColor::new(251, 191, 36)
    } else {
        PaletteColor::new(217, 119, 6)
    });
    let status_hover = if is_dark { 0.2 } else { 0.15 };

    // Borders
    let border_subtle = if is_dark {
        window_bg.shift_lightness(0.06)
    } else {
        window_bg.shift_lightness(-0.06)
    };
    let border_strong = if is_dark {
        window_bg.shift_lightness(0.12)
    } else {
        window_bg.shift_lightness(-0.12)
    };

    assemble(
        is_dark,
        opaque(window_bg),
        opaque(bg_secondary),
        opaque(bg_tertiary),
        opaque(bg_hover),
        opaque(text_primary),
        opaque(text_secondary),
        opaque(text_muted),
        opaque(text_disabled),
        opaque(accent),
        opaque(accent_hover),
        opaque(accent_active),
        opaque(accent_text),
        opaque(danger),
        opaque(warning),
        status_hover,
        opaque(border_subtle),
        opaque(border_strong),
    )
}

// --- contrast helpers (ported 1:1 from auto_theme::generator) ---------------

/// Pick the best foreground for text on the accent triplet (base, hover, active),
/// considering the worst case across all three so `:hover`/`:active` stay legible.
/// Shared with the custom-theme builder so `accent_text` is derived identically.
pub(crate) fn pick_btn_text_for_accent_set(
    accent: &PaletteColor,
    accent_hover: &PaletteColor,
    accent_active: &PaletteColor,
) -> PaletteColor {
    let white = PaletteColor::new(255, 255, 255);
    let black = PaletteColor::new(0, 0, 0);

    let white_worst = white
        .contrast_ratio(accent)
        .min(white.contrast_ratio(accent_hover))
        .min(white.contrast_ratio(accent_active));
    let black_worst = black
        .contrast_ratio(accent)
        .min(black.contrast_ratio(accent_hover))
        .min(black.contrast_ratio(accent_active));

    if white_worst >= 3.0 {
        white
    } else if black_worst > white_worst {
        black
    } else {
        white
    }
}

/// Ensure text has at least WCAG AA contrast (4.5:1) against the background.
fn ensure_text_contrast(text: PaletteColor, bg: &PaletteColor, is_dark: bool) -> PaletteColor {
    ensure_text_contrast_target(text, bg, is_dark, 4.5)
}

/// Ensure text meets `target` contrast against `bg`, shifting lightness toward
/// white (dark) / black (light) up to 20 steps, then clamping to pure white/black.
/// Shared with the custom-theme builder for its derived muted/secondary tiers.
pub(crate) fn ensure_text_contrast_target(
    text: PaletteColor,
    bg: &PaletteColor,
    is_dark: bool,
    target: f64,
) -> PaletteColor {
    if text.contrast_ratio(bg) >= target {
        return text;
    }

    let (h, s, l) = text.to_hsl();
    let direction = if is_dark { 0.05 } else { -0.05 };
    let mut new_l = l;

    for _ in 0..20 {
        new_l = (new_l + direction).clamp(0.0, 1.0);
        let candidate = PaletteColor::from_hsl(h, s, new_l);
        if candidate.contrast_ratio(bg) >= target {
            return candidate;
        }
    }

    if is_dark {
        PaletteColor::new(255, 255, 255)
    } else {
        PaletteColor::new(0, 0, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colors::ALPHA_COUNT;

    fn dark_palette() -> ThemePalette {
        ThemePalette {
            bg_primary: PaletteColor::new(15, 15, 20),
            bg_secondary: PaletteColor::new(26, 26, 30),
            bg_tertiary: PaletteColor::new(42, 42, 48),
            bg_hover: PaletteColor::new(31, 31, 35),
            accent: PaletteColor::new(66, 133, 244),
            is_dark: true,
            all_colors: vec![],
        }
    }

    fn light_palette() -> ThemePalette {
        ThemePalette {
            bg_primary: PaletteColor::new(245, 245, 245),
            bg_secondary: PaletteColor::new(235, 235, 235),
            bg_tertiary: PaletteColor::new(220, 220, 220),
            bg_hover: PaletteColor::new(240, 240, 240),
            accent: PaletteColor::new(26, 115, 232),
            is_dark: false,
            all_colors: vec![],
        }
    }

    #[test]
    fn dark_polarity_white_alpha_base() {
        let c = theme_from_palette(&dark_palette());
        assert_eq!(c.alpha.len(), ALPHA_COUNT);
        // Dark themes get a WHITE-based alpha ramp + translucent edges.
        assert_eq!(c.alpha[c.alpha.len() - 1].r, 255);
        assert_eq!(c.surface_hover, Rgba::rgba(255, 255, 255, 0x10));
        assert_eq!(c.border_muted, Rgba::rgba(255, 255, 255, 0x38));
        // Surfaces map straight through.
        assert_eq!(c.surface_main, Rgba::rgb(15, 15, 20));
        assert_eq!(c.surface_card, Rgba::rgb(26, 26, 30));
        assert_eq!(c.surface_elevated, Rgba::rgb(42, 42, 48));
        assert_eq!(c.bg_hover, Rgba::rgb(31, 31, 35));
        // Accent maps straight; focus_ring == accent; favorite == danger.
        assert_eq!(c.accent, Rgba::rgb(66, 133, 244));
        assert_eq!(c.focus_ring, c.accent);
        assert_eq!(c.favorite, c.danger);
        // Dark success hue.
        assert_eq!(c.success, Rgba::rgb(0x3f, 0xae, 0x6a));
    }

    #[test]
    fn light_polarity_black_alpha_base() {
        let c = theme_from_palette(&light_palette());
        assert_eq!(c.alpha[c.alpha.len() - 1].r, 0);
        assert_eq!(c.surface_hover, Rgba::rgba(0, 0, 0, 0x10));
        assert_eq!(c.border_muted, Rgba::rgba(0, 0, 0, 0x38));
        // Light success hue (darker so it clears >=3:1 on a light surface).
        assert_eq!(c.success, Rgba::rgb(0x1f, 0x8a, 0x4c));
        // Light danger hue.
        assert_eq!(c.danger, Rgba::rgb(220, 38, 38));
    }

    #[test]
    fn derived_status_families_have_expected_tints() {
        let c = theme_from_palette(&dark_palette());
        // bg = 0.1, border = 0.3, hover = 0.2 (dark) — straight-alpha of the hue.
        assert_eq!(c.danger_bg.a, (0.1f32 * 255.0 + 0.5) as u8);
        assert_eq!(c.danger_border.a, (0.3f32 * 255.0 + 0.5) as u8);
        assert_eq!(c.danger_hover.a, (0.2f32 * 255.0 + 0.5) as u8);
        assert_eq!(c.danger_bg.r, c.danger.r);
        assert_eq!(c.success_hover.a, c.danger_hover.a);
    }

    #[test]
    fn deterministic_for_fixed_seed() {
        let a = theme_from_palette(&dark_palette());
        let b = theme_from_palette(&dark_palette());
        assert_eq!(a, b);
    }

    #[test]
    fn scheme_polarity_from_window_bg() {
        let mut scheme = SystemColorScheme {
            window_bg: Some(PaletteColor::new(30, 30, 30)),
            window_bg_alt: None,
            view_bg: None,
            button_bg: None,
            header_bg: None,
            header_bg_inactive: None,
            tooltip_bg: None,
            window_fg: None,
            window_fg_inactive: None,
            view_fg: None,
            button_fg: None,
            selection_bg: None,
            selection_fg: None,
            selection_hover: None,
            accent: Some(PaletteColor::new(66, 133, 244)),
            fg_link: None,
            fg_negative: None,
            fg_neutral: None,
            fg_positive: None,
            wm_active_bg: None,
            wm_active_fg: None,
            wm_inactive_bg: None,
        };
        let dark = theme_from_scheme(&scheme);
        assert_eq!(dark.surface_main, Rgba::rgb(30, 30, 30));
        assert_eq!(dark.alpha[dark.alpha.len() - 1].r, 255); // dark -> white base
        assert_eq!(dark.accent, Rgba::rgb(66, 133, 244));

        scheme.window_bg = Some(PaletteColor::new(240, 240, 240));
        let light = theme_from_scheme(&scheme);
        assert_eq!(light.alpha[light.alpha.len() - 1].r, 0); // light -> black base
    }
}
