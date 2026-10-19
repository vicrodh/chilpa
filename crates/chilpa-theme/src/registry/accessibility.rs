use super::*;

// ---------------------------------------------------------------------------
// Accessibility (REDESIGNED) themes — final verified palettes (Part B)
// ---------------------------------------------------------------------------
//
// Unlike the standard rows, the a11y themes specify SOLID (opaque) status
// surfaces and borders, not rgba() alpha tints — accessible contrast can't be
// guaranteed through translucency over an arbitrary backdrop. So these rows are
// materialized directly (like dark()/tokyo_night()) rather than via StdSpec.
//
// `success` family: Part B tables only fix `success` for `colorblind`. For the
// other four a11y themes a theme-appropriate green is derived to clear the same
// contrast bar as the theme's other foregrounds (AAA for wcag-*, ≥ wcag bar for
// HC), with solid bg/border/hover tints matching each theme's status pattern.
// These derived greens are covered by the contrast unit tests below.
//
// `focus_ring`: exactly as Part B (reuses the accent for wcag-*/HC-light;
// dedicated cyan/blue elsewhere; bright yellow for HC-dark).

/// `wcag-light` — WCAG AAA Light (Part B §B.1). Body text AAA (7:1) + APCA ≥75;
/// non-text ≥3:1. text-primary `#1a1a1a` (not pure black) to avoid reverse-halation.
pub(super) fn wcag_light() -> ThemeColors {
    let danger = Rgba::rgb(0xa3, 0x00, 0x00);
    let warning = Rgba::rgb(0x6b, 0x45, 0x00);
    // derived success: deep green clearing AAA on white (7.36:1).
    let success = Rgba::rgb(0x13, 0x63, 0x2f);
    let accent = Rgba::rgb(0x0a, 0x4e, 0xa3);
    ThemeColors {
        surface_main: Rgba::rgb(0xff, 0xff, 0xff),     // bg-primary
        surface_card: Rgba::rgb(0xf4, 0xf5, 0xf7),     // bg-secondary
        surface_elevated: Rgba::rgb(0xe7, 0xe9, 0xee), // bg-tertiary
        surface_hover: Rgba::rgba(0, 0, 0, 0x10),      // ~6% black (light polarity)
        bg_hover: Rgba::rgb(0xdd, 0xe0, 0xe6),         // bg-hover

        text_primary: Rgba::rgb(0x1a, 0x1a, 0x1a),
        text_secondary: Rgba::rgb(0x3a, 0x3a, 0x3a),
        text_muted: Rgba::rgb(0x59, 0x59, 0x59),
        text_disabled: Rgba::rgb(0x76, 0x76, 0x76),

        accent,
        accent_hover: Rgba::rgb(0x08, 0x3d, 0x80),
        accent_pressed: Rgba::rgb(0x06, 0x2e, 0x60),
        accent_text: Rgba::rgb(0xff, 0xff, 0xff), // btn-primary-text

        danger,
        danger_bg: Rgba::rgb(0xfb, 0xe9, 0xe9),     // solid
        danger_border: Rgba::rgb(0xaa, 0x60, 0x60), // solid
        danger_hover: Rgba::rgb(0x85, 0x00, 0x00),

        warning,
        warning_bg: Rgba::rgb(0xff, 0xf7, 0xe6),     // solid
        warning_border: Rgba::rgb(0x9c, 0x73, 0x20), // solid
        warning_hover: Rgba::rgb(0x55, 0x37, 0x00),

        success,
        success_bg: Rgba::rgb(0xe6, 0xf4, 0xea),     // solid
        success_border: Rgba::rgb(0x5a, 0x9c, 0x72), // solid
        success_hover: Rgba::rgb(0x0f, 0x4f, 0x25),

        border_subtle: Rgba::rgb(0xc9, 0xcc, 0xd2), // decorative divider
        border_muted: Rgba::rgba(0, 0, 0, 0x38),    // ~22% black
        border_strong: Rgba::rgb(0x6e, 0x6e, 0x6e), // control border

        focus_ring: accent, // reuses accent

        favorite: danger,
        card_shadow: LEGACY_CARD_SHADOW,

        alpha: alpha_ramp(true), // light theme -> black-based overlays
    }
}

/// `wcag-dark` — WCAG AAA Dark (Part B §B.2). AAA (7:1) + APCA content/body;
/// non-text ≥3:1. bg `#0d1117` + text `#e6edf3` to kill halation.
pub(super) fn wcag_dark() -> ThemeColors {
    let danger = Rgba::rgb(0xff, 0x9d, 0x9d);
    let warning = Rgba::rgb(0xff, 0xca, 0x6a);
    // derived success: lightened green clearing the AAA bar on dark (11.77:1).
    let success = Rgba::rgb(0x7e, 0xe0, 0xa0);
    let accent = Rgba::rgb(0x9e, 0xc1, 0xff);
    ThemeColors {
        surface_main: Rgba::rgb(0x0d, 0x11, 0x17),
        surface_card: Rgba::rgb(0x16, 0x1b, 0x22),
        surface_elevated: Rgba::rgb(0x21, 0x26, 0x2d),
        surface_hover: Rgba::rgba(255, 255, 255, 0x10), // ~6% white
        bg_hover: Rgba::rgb(0x2a, 0x31, 0x3a),

        text_primary: Rgba::rgb(0xe6, 0xed, 0xf3),
        text_secondary: Rgba::rgb(0xc9, 0xd1, 0xd9),
        text_muted: Rgba::rgb(0xb8, 0xc0, 0xcc),
        text_disabled: Rgba::rgb(0x7d, 0x87, 0x94),

        accent,
        accent_hover: Rgba::rgb(0xb9, 0xd2, 0xff),
        accent_pressed: Rgba::rgb(0xcf, 0xe0, 0xff),
        accent_text: Rgba::rgb(0x06, 0x09, 0x0f), // dark text on light-blue

        danger,
        danger_bg: Rgba::rgb(0x2d, 0x14, 0x16),     // opaque dark-red tint
        danger_border: Rgba::rgb(0xa8, 0x56, 0x56), // see adjacency constraint
        danger_hover: Rgba::rgb(0xff, 0xb3, 0xb3),

        warning,
        warning_bg: Rgba::rgb(0x2d, 0x24, 0x10),     // opaque dark-amber tint
        warning_border: Rgba::rgb(0x9c, 0x74, 0x30),
        warning_hover: Rgba::rgb(0xff, 0xd9, 0x8a),

        success,
        success_bg: Rgba::rgb(0x11, 0x24, 0x1a),     // opaque dark-green tint
        success_border: Rgba::rgb(0x3f, 0x7d, 0x56),
        success_hover: Rgba::rgb(0x9a, 0xed, 0xb6),

        border_subtle: Rgba::rgb(0x2d, 0x33, 0x3b), // decorative separator
        border_muted: Rgba::rgba(255, 255, 255, 0x38),
        border_strong: Rgba::rgb(0x6b, 0x76, 0x86), // control border (≥3:1 all tiers)

        focus_ring: accent,

        favorite: danger,
        card_shadow: LEGACY_CARD_SHADOW,

        alpha: alpha_ramp(false), // dark theme -> white-based overlays
    }
}

/// `high-contrast` (DARK) — Part B §B.3a. Lifted off pure black (`#0a0a0a`),
/// reciprocal cyan accent, bright yellow demoted to the focus-ring slot.
pub(super) fn high_contrast() -> ThemeColors {
    let danger = Rgba::rgb(0xff, 0x8a, 0x8a);
    let warning = Rgba::rgb(0xff, 0xb0, 0x00);
    // derived success: bright green ≥ wcag-dark bar (15.51:1 on bg-primary).
    let success = Rgba::rgb(0x62, 0xff, 0xb0);
    let accent = Rgba::rgb(0x62, 0xd4, 0xff);
    ThemeColors {
        surface_main: Rgba::rgb(0x0a, 0x0a, 0x0a),     // lifted off pure black
        surface_card: Rgba::rgb(0x14, 0x14, 0x14),
        surface_elevated: Rgba::rgb(0x1f, 0x1f, 0x1f),
        surface_hover: Rgba::rgba(255, 255, 255, 0x10),
        bg_hover: Rgba::rgb(0x2b, 0x2b, 0x2b),

        text_primary: Rgba::rgb(0xff, 0xff, 0xff),
        text_secondary: Rgba::rgb(0xf0, 0xf0, 0xf0), // near-primary, NOT gray
        text_muted: Rgba::rgb(0xe0, 0xe0, 0xe0),     // near-primary, NOT gray
        text_disabled: Rgba::rgb(0x8c, 0x8c, 0x8c),  // the only reserved gray

        accent,                                      // reciprocal cyan
        accent_hover: Rgba::rgb(0x8c, 0xe3, 0xff),
        accent_pressed: Rgba::rgb(0xae, 0xed, 0xff),
        accent_text: Rgba::rgb(0x00, 0x00, 0x00),    // reads on cyan fill

        danger,
        danger_bg: Rgba::rgb(0x2a, 0x00, 0x00),      // opaque; always bordered
        danger_border: danger,                       // = danger hue
        danger_hover: Rgba::rgb(0xff, 0xb3, 0xb3),

        warning,
        warning_bg: Rgba::rgb(0x2a, 0x1d, 0x00),     // opaque; always bordered
        warning_border: warning,                     // = warning hue
        warning_hover: Rgba::rgb(0xff, 0xc9, 0x4d),

        success,
        success_bg: Rgba::rgb(0x00, 0x26, 0x1a),     // opaque; always bordered
        success_border: success,                     // = success hue
        success_hover: Rgba::rgb(0x8a, 0xff, 0xc8),

        border_subtle: Rgba::rgb(0x7a, 0x7a, 0x7a), // still clearly visible (4.61:1)
        border_muted: Rgba::rgba(255, 255, 255, 0x38),
        border_strong: Rgba::rgb(0xff, 0xff, 0xff), // = text color

        focus_ring: Rgba::rgb(0xff, 0xee, 0x32),    // bright yellow's correct home

        favorite: danger,
        card_shadow: LEGACY_CARD_SHADOW,

        alpha: alpha_ramp(false), // dark theme -> white-based overlays
    }
}

/// `high-contrast-light` (LIGHT, new) — Part B §B.3b. Reciprocal deep-blue
/// accent. Warning corrected `#735c00` → `#5e4b00` (AA-only → AAA on white).
pub(super) fn high_contrast_light() -> ThemeColors {
    let danger = Rgba::rgb(0xa3, 0x00, 0x00);
    let warning = Rgba::rgb(0x5e, 0x4b, 0x00); // CORRECTED from #735c00
    // derived success: deep green ≥ HC bar (8.47:1 on white).
    let success = Rgba::rgb(0x00, 0x5a, 0x1c);
    let accent = Rgba::rgb(0x00, 0x00, 0xcc);
    ThemeColors {
        surface_main: Rgba::rgb(0xff, 0xff, 0xff),
        surface_card: Rgba::rgb(0xf2, 0xf2, 0xf2),
        surface_elevated: Rgba::rgb(0xe6, 0xe6, 0xe6),
        surface_hover: Rgba::rgba(0, 0, 0, 0x10),
        bg_hover: Rgba::rgb(0xda, 0xda, 0xda),

        text_primary: Rgba::rgb(0x00, 0x00, 0x00),
        text_secondary: Rgba::rgb(0x1a, 0x1a, 0x1a), // near-primary
        text_muted: Rgba::rgb(0x33, 0x33, 0x33),     // near-primary, NOT gray
        text_disabled: Rgba::rgb(0x59, 0x59, 0x59),  // reserved gray (7.00:1 AAA)

        accent,                                      // reciprocal deep blue
        accent_hover: Rgba::rgb(0x00, 0x00, 0xa3),
        accent_pressed: Rgba::rgb(0x00, 0x00, 0x80),
        accent_text: Rgba::rgb(0xff, 0xff, 0xff),    // reads on blue fill

        danger,
        danger_bg: Rgba::rgb(0xff, 0xe5, 0xe5),      // opaque; always bordered
        danger_border: danger,                       // = danger hue
        danger_hover: Rgba::rgb(0x7a, 0x00, 0x00),

        warning,
        warning_bg: Rgba::rgb(0xff, 0xf4, 0xd6),     // opaque; always bordered
        warning_border: warning,                     // = corrected warning hue
        warning_hover: Rgba::rgb(0x5c, 0x49, 0x00),

        success,
        success_bg: Rgba::rgb(0xdc, 0xf2, 0xe2),     // opaque; always bordered
        success_border: success,                     // = success hue
        success_hover: Rgba::rgb(0x00, 0x44, 0x17),

        border_subtle: Rgba::rgb(0x66, 0x66, 0x66), // clearly visible (5.74:1)
        border_muted: Rgba::rgba(0, 0, 0, 0x38),
        border_strong: Rgba::rgb(0x00, 0x00, 0x00), // = text color

        focus_ring: accent, // accent doubles as focus ring

        favorite: danger,
        card_shadow: LEGACY_CARD_SHADOW,

        alpha: alpha_ramp(true), // light theme -> black-based overlays
    }
}

/// `colorblind` — universal Okabe-Ito (Part B §B.4). danger (reddish-purple)
/// split from warning (amber) across CVD confusion axes; foregrounds lightened
/// to clear AA on all tiers. `success` kept `#009e73`; body text routes to
/// `success-hover #33c397` on the lightest tier (C.6).
pub(super) fn colorblind() -> ThemeColors {
    let danger = Rgba::rgb(0xd4, 0x88, 0xb1);
    let warning = Rgba::rgb(0xe6, 0x9f, 0x00);
    let success = Rgba::rgb(0x00, 0x9e, 0x73); // Okabe-Ito bluish green
    let accent = Rgba::rgb(0x62, 0xa5, 0xe4);
    ThemeColors {
        surface_main: Rgba::rgb(0x1a, 0x1a, 0x2e),     // retained dark navy
        surface_card: Rgba::rgb(0x22, 0x22, 0x3a),
        surface_elevated: Rgba::rgb(0x2c, 0x2c, 0x46),
        surface_hover: Rgba::rgba(255, 255, 255, 0x10),
        bg_hover: Rgba::rgb(0x36, 0x36, 0x52),

        text_primary: Rgba::rgb(0xff, 0xff, 0xff),
        text_secondary: Rgba::rgb(0xdc, 0xdc, 0xe0),
        text_muted: Rgba::rgb(0xaa, 0xaa, 0xb8),    // lightened -> AA on bg-tertiary
        text_disabled: Rgba::rgb(0x6f, 0x6f, 0x86), // exempt, perceptible

        accent,                                     // lightened Okabe-Ito blue
        accent_hover: Rgba::rgb(0x7e, 0xb4, 0xe8),
        accent_pressed: Rgba::rgb(0x8b, 0xbc, 0xec),
        accent_text: Rgba::rgb(0x0a, 0x0a, 0x14),   // near-black on light-blue

        danger,                                     // lightened reddish-purple
        danger_bg: Rgba::rgb(0x3a, 0x1a, 0x2e),     // solid tint
        danger_border: Rgba::rgb(0x8a, 0x4a, 0x70), // solid tint
        danger_hover: Rgba::rgb(0xe0, 0xa3, 0xc5),

        warning,                                    // Okabe-Ito amber
        warning_bg: Rgba::rgb(0x3a, 0x2e, 0x00),    // solid tint
        warning_border: Rgba::rgb(0x8a, 0x6e, 0x00),
        warning_hover: Rgba::rgb(0xf0, 0xb6, 0x30),

        success,                                    // AA on primary/secondary
        success_bg: Rgba::rgb(0x0a, 0x33, 0x29),    // solid tint
        success_border: Rgba::rgb(0x1f, 0x6b, 0x54),
        success_hover: Rgba::rgb(0x33, 0xc3, 0x97), // body-size success on bg-tertiary

        border_subtle: Rgba::rgb(0x3e, 0x3e, 0x56), // decorative (1.65:1)
        border_muted: Rgba::rgba(255, 255, 255, 0x38),
        border_strong: Rgba::rgb(0x6e, 0x6e, 0x88), // control boundary (3.45:1)

        focus_ring: Rgba::rgb(0x8b, 0xbc, 0xec),    // high-tone blue (8.53:1)

        favorite: danger,
        card_shadow: LEGACY_CARD_SHADOW,

        alpha: alpha_ramp(false), // dark theme -> white-based overlays
    }
}
