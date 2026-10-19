use super::*;

// ---------------------------------------------------------------------------
// Dark (branded / community) themes
// ---------------------------------------------------------------------------

pub(super) fn warm() -> ThemeColors {
    let s = StdSpec {
        bg_primary: Rgba::rgb(0x2b, 0x1a, 0x14),
        bg_secondary: Rgba::rgb(0x3a, 0x24, 0x1a),
        bg_tertiary: Rgba::rgb(0x4a, 0x2f, 0x23),
        bg_hover: Rgba::rgb(0x5b, 0x3a, 0x2e),
        text_primary: Rgba::rgb(0xf5, 0xe9, 0xe2),
        text_secondary: Rgba::rgb(0xd8, 0xc3, 0xb7),
        text_muted: Rgba::rgb(0xbf, 0xa3, 0x96),
        text_disabled: Rgba::rgb(0x8d, 0x73, 0x67),
        accent: Rgba::rgb(0xe5, 0x98, 0x66),
        accent_hover: Rgba::rgb(0xf0, 0xa7, 0x7b),
        accent_pressed: Rgba::rgb(0xd8, 0x86, 0x52),
        accent_text: Rgba::rgb(0x00, 0x00, 0x00),
        danger: Rgba::rgb(0xbf, 0x4f, 0x4f),
        warning: Rgba::rgb(0xd6, 0xa9, 0x4f),
        border_subtle: Rgba::rgb(0x4a, 0x2f, 0x23),
        border_strong: Rgba::rgb(0x5b, 0x3a, 0x2e),
        ..Default::default()
    };
    s.build(bg_is_light(s.bg_primary))
}

pub(super) fn nord() -> ThemeColors {
    let s = StdSpec {
        bg_primary: Rgba::rgb(0x1d, 0x22, 0x30),
        bg_secondary: Rgba::rgb(0x2a, 0x2f, 0x3c),
        bg_tertiary: Rgba::rgb(0x32, 0x38, 0x4c),
        bg_hover: Rgba::rgb(0x3c, 0x42, 0x56),
        text_primary: Rgba::rgb(0xec, 0xec, 0xec),
        text_secondary: Rgba::rgb(0xc6, 0xc6, 0xc6),
        text_muted: Rgba::rgb(0x99, 0x99, 0xa3),
        text_disabled: Rgba::rgb(0x6f, 0x6f, 0x7b),
        accent: Rgba::rgb(0x35, 0x84, 0xe4),
        accent_hover: Rgba::rgb(0x5f, 0x9e, 0xe6),
        accent_pressed: Rgba::rgb(0x1a, 0x5f, 0xb4),
        accent_text: Rgba::rgb(0x24, 0x1f, 0x31),
        danger: Rgba::rgb(0xc0, 0x1c, 0x28),
        warning: Rgba::rgb(0xf5, 0xc2, 0x11),
        border_subtle: Rgba::rgb(0x2a, 0x2f, 0x3c),
        border_strong: Rgba::rgb(0x32, 0x38, 0x4c),
        ..Default::default()
    };
    s.build(bg_is_light(s.bg_primary))
}

pub(super) fn dracula() -> ThemeColors {
    // NON-STANDARD tint fractions: bg .15 / border .4 / hover .25.
    let s = StdSpec {
        bg_primary: Rgba::rgb(0x28, 0x2a, 0x36),
        bg_secondary: Rgba::rgb(0x21, 0x22, 0x2c),
        bg_tertiary: Rgba::rgb(0x34, 0x37, 0x46),
        bg_hover: Rgba::rgb(0x44, 0x47, 0x5a),
        text_primary: Rgba::rgb(0xf8, 0xf8, 0xf2),
        text_secondary: Rgba::rgb(0xe2, 0xe2, 0xdc),
        text_muted: Rgba::rgb(0x62, 0x72, 0xa4),
        text_disabled: Rgba::rgb(0x44, 0x47, 0x5a),
        accent: Rgba::rgb(0xbd, 0x93, 0xf9),
        accent_hover: Rgba::rgb(0xff, 0x79, 0xc6),
        accent_pressed: Rgba::rgb(0x8b, 0xe9, 0xfd),
        accent_text: Rgba::rgb(0x28, 0x2a, 0x36),
        danger: Rgba::rgb(0xff, 0x55, 0x55),
        warning: Rgba::rgb(0xff, 0xb8, 0x6c),
        tint_bg: 0.15,
        tint_border: 0.4,
        tint_hover: 0.25,
        border_subtle: Rgba::rgb(0x34, 0x37, 0x46),
        border_strong: Rgba::rgb(0x44, 0x47, 0x5a),
        ..Default::default()
    };
    s.build(bg_is_light(s.bg_primary))
}

pub(super) fn catppuccin_mocha() -> ThemeColors {
    let s = StdSpec {
        bg_primary: Rgba::rgb(0x1e, 0x1e, 0x2e),
        bg_secondary: Rgba::rgb(0x18, 0x18, 0x25),
        bg_tertiary: Rgba::rgb(0x11, 0x11, 0x1b),
        bg_hover: Rgba::rgb(0x31, 0x32, 0x44),
        text_primary: Rgba::rgb(0xcd, 0xd6, 0xf4),
        text_secondary: Rgba::rgb(0xba, 0xc2, 0xde),
        text_muted: Rgba::rgb(0xa6, 0xad, 0xc8),
        text_disabled: Rgba::rgb(0x73, 0x79, 0x94),
        accent: Rgba::rgb(0xcb, 0xa6, 0xf7),
        accent_hover: Rgba::rgb(0x89, 0xb4, 0xfa),
        accent_pressed: Rgba::rgb(0xf3, 0x8b, 0xa8),
        accent_text: Rgba::rgb(0x1e, 0x1e, 0x2e),
        danger: Rgba::rgb(0xf3, 0x8b, 0xa8),
        warning: Rgba::rgb(0xf9, 0xe2, 0xaf),
        border_subtle: Rgba::rgb(0x31, 0x32, 0x44),
        border_strong: Rgba::rgb(0x45, 0x47, 0x5a),
        ..Default::default()
    };
    s.build(bg_is_light(s.bg_primary))
}
/// catppuccin-latte — light theme (Part B §latte). Black alpha base.
pub(super) fn catppuccin_latte() -> ThemeColors {
    let s = StdSpec {
        bg_primary: Rgba::rgb(0xef, 0xf1, 0xf5),
        bg_secondary: Rgba::rgb(0xe6, 0xe9, 0xef),
        bg_tertiary: Rgba::rgb(0xdc, 0xe0, 0xe8),
        bg_hover: Rgba::rgb(0xcc, 0xd0, 0xda),
        text_primary: Rgba::rgb(0x4c, 0x4f, 0x69),
        text_secondary: Rgba::rgb(0x5c, 0x5f, 0x77),
        text_muted: Rgba::rgb(0x6c, 0x6f, 0x85),
        text_disabled: Rgba::rgb(0x9c, 0xa0, 0xb0),
        accent: Rgba::rgb(0x88, 0x39, 0xef),
        accent_hover: Rgba::rgb(0x1e, 0x66, 0xf5),
        accent_pressed: Rgba::rgb(0xd2, 0x0f, 0x39),
        accent_text: Rgba::rgb(0xef, 0xf1, 0xf5),
        danger: Rgba::rgb(0xd2, 0x0f, 0x39),
        warning: Rgba::rgb(0xdf, 0x8e, 0x1d),
        border_subtle: Rgba::rgb(0xbc, 0xc0, 0xcc),
        border_strong: Rgba::rgb(0xac, 0xb0, 0xbe),
        ..Default::default()
    };
    s.build(bg_is_light(s.bg_primary))
}

/// catppuccin-frappe — dark theme. White alpha base.
pub(super) fn catppuccin_frappe() -> ThemeColors {
    let s = StdSpec {
        bg_primary: Rgba::rgb(0x30, 0x34, 0x46),
        bg_secondary: Rgba::rgb(0x29, 0x2c, 0x3c),
        bg_tertiary: Rgba::rgb(0x23, 0x26, 0x34),
        bg_hover: Rgba::rgb(0x41, 0x45, 0x59),
        text_primary: Rgba::rgb(0xc6, 0xd0, 0xf5),
        text_secondary: Rgba::rgb(0xb5, 0xbf, 0xe2),
        text_muted: Rgba::rgb(0xa5, 0xad, 0xce),
        text_disabled: Rgba::rgb(0x73, 0x79, 0x94),
        accent: Rgba::rgb(0xca, 0x9e, 0xe6),
        accent_hover: Rgba::rgb(0x8c, 0xaa, 0xee),
        accent_pressed: Rgba::rgb(0xe7, 0x82, 0x84),
        accent_text: Rgba::rgb(0x30, 0x34, 0x46),
        danger: Rgba::rgb(0xe7, 0x82, 0x84),
        warning: Rgba::rgb(0xe5, 0xc8, 0x90),
        border_subtle: Rgba::rgb(0x51, 0x57, 0x6d),
        border_strong: Rgba::rgb(0x62, 0x68, 0x80),
        ..Default::default()
    };
    s.build(bg_is_light(s.bg_primary))
}

/// catppuccin-macchiato — dark theme. White alpha base.
pub(super) fn catppuccin_macchiato() -> ThemeColors {
    let s = StdSpec {
        bg_primary: Rgba::rgb(0x24, 0x27, 0x3a),
        bg_secondary: Rgba::rgb(0x1e, 0x20, 0x30),
        bg_tertiary: Rgba::rgb(0x18, 0x19, 0x26),
        bg_hover: Rgba::rgb(0x36, 0x3a, 0x4f),
        text_primary: Rgba::rgb(0xca, 0xd3, 0xf5),
        text_secondary: Rgba::rgb(0xb8, 0xc0, 0xe0),
        text_muted: Rgba::rgb(0xa5, 0xad, 0xcb),
        text_disabled: Rgba::rgb(0x6e, 0x73, 0x8d),
        accent: Rgba::rgb(0xc6, 0xa0, 0xf6),
        accent_hover: Rgba::rgb(0x8a, 0xad, 0xf4),
        accent_pressed: Rgba::rgb(0xed, 0x87, 0x96),
        accent_text: Rgba::rgb(0x24, 0x27, 0x3a),
        danger: Rgba::rgb(0xed, 0x87, 0x96),
        warning: Rgba::rgb(0xee, 0xd4, 0x9f),
        border_subtle: Rgba::rgb(0x49, 0x4d, 0x64),
        border_strong: Rgba::rgb(0x5b, 0x60, 0x78),
        ..Default::default()
    };
    s.build(bg_is_light(s.bg_primary))
}

/// breeze-dark — OMITS the danger/warning families AND the alpha scale →
/// inherits them from `:root` Dark. We materialize the inherited danger/warning
/// hues (red `#ef4444`, amber `#fbbf24`) explicitly.
pub(super) fn breeze_dark() -> ThemeColors {
    let s = StdSpec {
        bg_primary: Rgba::rgb(0x14, 0x16, 0x18),
        bg_secondary: Rgba::rgb(0x20, 0x23, 0x26),
        bg_tertiary: Rgba::rgb(0x29, 0x2c, 0x30),
        bg_hover: Rgba::rgb(0x31, 0x36, 0x3b),
        text_primary: Rgba::rgb(0xff, 0xff, 0xff),
        text_secondary: Rgba::rgb(0xfc, 0xfc, 0xfc),
        text_muted: Rgba::rgb(0xa1, 0xa9, 0xb1),
        text_disabled: Rgba::rgb(0x31, 0x36, 0x3b),
        accent: Rgba::rgb(0x3d, 0xae, 0xe9),
        accent_hover: Rgba::rgb(0x9b, 0x59, 0xb6),
        accent_pressed: Rgba::rgb(0x1d, 0x99, 0xf3),
        accent_text: Rgba::rgb(0x14, 0x16, 0x18),
        // inherited from :root Dark:
        danger: Rgba::rgb(0xef, 0x44, 0x44),
        warning: Rgba::rgb(0xfb, 0xbf, 0x24),
        border_subtle: Rgba::rgb(0x29, 0x2c, 0x30),
        border_strong: Rgba::rgb(0x31, 0x36, 0x3b),
        ..Default::default()
    };
    s.build(bg_is_light(s.bg_primary))
}

/// adwaita-dark — OMITS the danger/warning families + alpha scale → inherits
/// from `:root` Dark.
pub(super) fn adwaita_dark() -> ThemeColors {
    let s = StdSpec {
        bg_primary: Rgba::rgb(0x1d, 0x1d, 0x20),
        bg_secondary: Rgba::rgb(0x22, 0x22, 0x26),
        bg_tertiary: Rgba::rgb(0x28, 0x28, 0x2c),
        bg_hover: Rgba::rgb(0x2e, 0x2e, 0x32),
        text_primary: Rgba::rgb(0xff, 0xff, 0xff),
        text_secondary: Rgba::rgb(0xff, 0xff, 0xff),
        text_muted: Rgba::rgb(0xb3, 0xb3, 0xb8),
        text_disabled: Rgba::rgb(0x2e, 0x2e, 0x32),
        accent: Rgba::rgb(0x35, 0x84, 0xe4),
        accent_hover: Rgba::rgb(0x1c, 0x71, 0xd8),
        accent_pressed: Rgba::rgb(0x1a, 0x5f, 0xb4),
        accent_text: Rgba::rgb(0xff, 0xff, 0xff),
        // inherited from :root Dark:
        danger: Rgba::rgb(0xef, 0x44, 0x44),
        warning: Rgba::rgb(0xfb, 0xbf, 0x24),
        border_subtle: Rgba::rgb(0x28, 0x28, 0x2c),
        border_strong: Rgba::rgb(0x2e, 0x2e, 0x32),
        ..Default::default()
    };
    s.build(bg_is_light(s.bg_primary))
}

pub(super) fn aurora() -> ThemeColors {
    let s = StdSpec {
        bg_primary: Rgba::rgb(0x2e, 0x34, 0x40),
        bg_secondary: Rgba::rgb(0x3b, 0x42, 0x52),
        bg_tertiary: Rgba::rgb(0x43, 0x4c, 0x5e),
        bg_hover: Rgba::rgb(0x4c, 0x56, 0x6a),
        text_primary: Rgba::rgb(0xd8, 0xde, 0xe9),
        text_secondary: Rgba::rgb(0xe5, 0xe9, 0xf0),
        text_muted: Rgba::rgb(0xb4, 0x8e, 0xad),
        text_disabled: Rgba::rgb(0x4c, 0x56, 0x6a),
        accent: Rgba::rgb(0xa3, 0xbe, 0x8c),
        accent_hover: Rgba::rgb(0xeb, 0xcb, 0x8b),
        accent_pressed: Rgba::rgb(0xd0, 0x87, 0x70),
        accent_text: Rgba::rgb(0x2e, 0x34, 0x40),
        danger: Rgba::rgb(0xbf, 0x61, 0x6a),
        warning: Rgba::rgb(0xeb, 0xcb, 0x8b),
        border_subtle: Rgba::rgb(0x4c, 0x56, 0x6a),
        border_strong: Rgba::rgb(0x43, 0x4c, 0x5e),
        ..Default::default()
    };
    s.build(bg_is_light(s.bg_primary))
}

pub(super) fn ikari() -> ThemeColors {
    let s = StdSpec {
        bg_primary: Rgba::rgb(0x1c, 0x12, 0x39),
        bg_secondary: Rgba::rgb(0x24, 0x1a, 0x48),
        bg_tertiary: Rgba::rgb(0x30, 0x24, 0x58),
        bg_hover: Rgba::rgb(0x3c, 0x2f, 0x71),
        text_primary: Rgba::rgb(0xe8, 0xe6, 0xf2),
        text_secondary: Rgba::rgb(0xc6, 0xc2, 0xd8),
        text_muted: Rgba::rgb(0x95, 0x8f, 0xb5),
        text_disabled: Rgba::rgb(0x57, 0x4b, 0x79),
        accent: Rgba::rgb(0x7e, 0xda, 0x53),
        accent_hover: Rgba::rgb(0xa5, 0xf0, 0x66),
        accent_pressed: Rgba::rgb(0xd5, 0x8e, 0x27),
        accent_text: Rgba::rgb(0x1c, 0x12, 0x39),
        danger: Rgba::rgb(0xd8, 0x4a, 0x4a),
        warning: Rgba::rgb(0xe5, 0x9b, 0x2f),
        border_subtle: Rgba::rgb(0x30, 0x24, 0x58),
        border_strong: Rgba::rgb(0x3c, 0x2f, 0x71),
        ..Default::default()
    };
    s.build(bg_is_light(s.bg_primary))
}

pub(super) fn ayanami() -> ThemeColors {
    let s = StdSpec {
        bg_primary: Rgba::rgb(0x0f, 0x25, 0x3f),
        bg_secondary: Rgba::rgb(0x16, 0x3e, 0x60),
        bg_tertiary: Rgba::rgb(0x21, 0x4f, 0x7d),
        bg_hover: Rgba::rgb(0x2d, 0x63, 0x9f),
        text_primary: Rgba::rgb(0xf2, 0xf0, 0xe5),
        text_secondary: Rgba::rgb(0xd6, 0xd2, 0xc2),
        text_muted: Rgba::rgb(0x95, 0xa4, 0xb7),
        text_disabled: Rgba::rgb(0x2d, 0x63, 0x9f),
        accent: Rgba::rgb(0xe5, 0xb8, 0x2e),
        accent_hover: Rgba::rgb(0xf0, 0xcd, 0x63),
        accent_pressed: Rgba::rgb(0xcf, 0xa2, 0x2e),
        accent_text: Rgba::rgb(0x0f, 0x25, 0x3f),
        danger: Rgba::rgb(0xc0, 0x39, 0x2b),
        warning: Rgba::rgb(0xd8, 0x9b, 0x1c),
        border_subtle: Rgba::rgb(0x21, 0x4f, 0x7d),
        border_strong: Rgba::rgb(0x2d, 0x63, 0x9f),
        ..Default::default()
    };
    s.build(bg_is_light(s.bg_primary))
}

pub(super) fn iscariot() -> ThemeColors {
    let s = StdSpec {
        bg_primary: Rgba::rgb(0x2a, 0x10, 0x2a),
        bg_secondary: Rgba::rgb(0x38, 0x13, 0x3b),
        bg_tertiary: Rgba::rgb(0x45, 0x18, 0x46),
        bg_hover: Rgba::rgb(0x5d, 0x20, 0x60),
        text_primary: Rgba::rgb(0xf4, 0xea, 0xf5),
        text_secondary: Rgba::rgb(0xcf, 0xaa, 0xcb),
        text_muted: Rgba::rgb(0xa2, 0x78, 0xa6),
        text_disabled: Rgba::rgb(0x5d, 0x20, 0x60),
        accent: Rgba::rgb(0xe9, 0x4f, 0x94),
        accent_hover: Rgba::rgb(0xff, 0x7a, 0xbf),
        accent_pressed: Rgba::rgb(0xc9, 0x45, 0xa3),
        accent_text: Rgba::rgb(0x2a, 0x10, 0x2a),
        danger: Rgba::rgb(0xc0, 0x39, 0x2b),
        warning: Rgba::rgb(0xe5, 0xb6, 0x4b),
        border_subtle: Rgba::rgb(0x38, 0x13, 0x3b),
        border_strong: Rgba::rgb(0x5d, 0x20, 0x60),
        ..Default::default()
    };
    s.build(bg_is_light(s.bg_primary))
}

pub(super) fn stratego() -> ThemeColors {
    let s = StdSpec {
        bg_primary: Rgba::rgb(0x0a, 0x0a, 0x0b),
        bg_secondary: Rgba::rgb(0x14, 0x14, 0x18),
        bg_tertiary: Rgba::rgb(0x1d, 0x1e, 0x22),
        bg_hover: Rgba::rgb(0x28, 0x2a, 0x30),
        text_primary: Rgba::rgb(0xec, 0xe6, 0xd6),
        text_secondary: Rgba::rgb(0xb5, 0xaf, 0xa0),
        text_muted: Rgba::rgb(0x8a, 0x85, 0x7a),
        text_disabled: Rgba::rgb(0x4a, 0x48, 0x42),
        accent: Rgba::rgb(0xed, 0x2f, 0x3d),
        accent_hover: Rgba::rgb(0xf7, 0x4a, 0x58),
        accent_pressed: Rgba::rgb(0xc4, 0x1e, 0x2a),
        accent_text: Rgba::rgb(0xff, 0xff, 0xff),
        danger: Rgba::rgb(0xe6, 0x39, 0x46),
        warning: Rgba::rgb(0xc4, 0xa5, 0x6a),
        border_subtle: Rgba::rgb(0x2a, 0x2a, 0x30),
        border_strong: Rgba::rgb(0x3a, 0x3a, 0x42),
        ..Default::default()
    };
    s.build(bg_is_light(s.bg_primary))
}

pub(super) fn rumi() -> ThemeColors {
    let s = StdSpec {
        bg_primary: Rgba::rgb(0x00, 0x00, 0x00),
        bg_secondary: Rgba::rgb(0x0d, 0x0d, 0x0d),
        bg_tertiary: Rgba::rgb(0x1a, 0x1a, 0x1a),
        bg_hover: Rgba::rgb(0x33, 0x33, 0x33),
        text_primary: Rgba::rgb(0xf0, 0xf0, 0xf0),
        text_secondary: Rgba::rgb(0xb2, 0xb2, 0xb2),
        text_muted: Rgba::rgb(0x80, 0x80, 0x80),
        text_disabled: Rgba::rgb(0x5a, 0x5a, 0x5a),
        accent: Rgba::rgb(0xe5, 0x8f, 0x24),
        accent_hover: Rgba::rgb(0xf0, 0xa5, 0x3c),
        accent_pressed: Rgba::rgb(0xcc, 0x7a, 0x12),
        accent_text: Rgba::rgb(0x00, 0x00, 0x00),
        danger: Rgba::rgb(0xe7, 0x4c, 0x3c),
        warning: Rgba::rgb(0xf3, 0x9c, 0x12),
        border_subtle: Rgba::rgb(0x1a, 0x1a, 0x1a),
        border_strong: Rgba::rgb(0x33, 0x33, 0x33),
        ..Default::default()
    };
    s.build(bg_is_light(s.bg_primary))
}

pub(super) fn zoey() -> ThemeColors {
    let s = StdSpec {
        bg_primary: Rgba::rgb(0x15, 0x1e, 0x2d),
        bg_secondary: Rgba::rgb(0x0e, 0x14, 0x1e),
        bg_tertiary: Rgba::rgb(0x10, 0x1a, 0x2a),
        bg_hover: Rgba::rgb(0x1b, 0x29, 0x3e),
        text_primary: Rgba::rgb(0xe0, 0xe2, 0xd5),
        text_secondary: Rgba::rgb(0xb5, 0xb7, 0xaa),
        text_muted: Rgba::rgb(0x8e, 0x90, 0x80),
        text_disabled: Rgba::rgb(0x60, 0x63, 0x54),
        accent: Rgba::rgb(0x46, 0xb4, 0xd3),
        accent_hover: Rgba::rgb(0x5c, 0xc0, 0xd9),
        accent_pressed: Rgba::rgb(0x3a, 0x97, 0xb6),
        accent_text: Rgba::rgb(0x15, 0x1e, 0x2d),
        danger: Rgba::rgb(0xbf, 0x61, 0x6a),
        warning: Rgba::rgb(0xd0, 0x87, 0x70),
        border_subtle: Rgba::rgb(0x0e, 0x14, 0x1e),
        border_strong: Rgba::rgb(0x1b, 0x29, 0x3e),
        ..Default::default()
    };
    s.build(bg_is_light(s.bg_primary))
}

pub(super) fn mira() -> ThemeColors {
    let s = StdSpec {
        bg_primary: Rgba::rgb(0x10, 0x18, 0x20),
        bg_secondary: Rgba::rgb(0x14, 0x1a, 0x28),
        bg_tertiary: Rgba::rgb(0x1d, 0x26, 0x35),
        bg_hover: Rgba::rgb(0x2a, 0x34, 0x48),
        text_primary: Rgba::rgb(0xe5, 0xe5, 0xe5),
        text_secondary: Rgba::rgb(0xb0, 0xb3, 0xc6),
        text_muted: Rgba::rgb(0x8a, 0x8d, 0xa0),
        text_disabled: Rgba::rgb(0x5c, 0x5e, 0x72),
        accent: Rgba::rgb(0xd9, 0x46, 0x85),
        accent_hover: Rgba::rgb(0xff, 0x00, 0x7f),
        accent_pressed: Rgba::rgb(0xff, 0xd7, 0x00), // intentional yellow
        accent_text: Rgba::rgb(0x10, 0x18, 0x20),
        danger: Rgba::rgb(0xc5, 0x30, 0x32),
        warning: Rgba::rgb(0xff, 0xd7, 0x00),
        border_subtle: Rgba::rgb(0x20, 0x2c, 0x3d),
        border_strong: Rgba::rgb(0x34, 0x40, 0x5a),
        ..Default::default()
    };
    s.build(bg_is_light(s.bg_primary))
}

/// frost — registered `type:light` in Tauri but a DARK Nord-polar canvas
/// (`#2e3440`). Polarity is luminance-derived, so it correctly resolves to a
/// white alpha base. (doc 01 §frost; corrected light/dark flag.)
pub(super) fn frost() -> ThemeColors {
    let s = StdSpec {
        bg_primary: Rgba::rgb(0x2e, 0x34, 0x40),
        bg_secondary: Rgba::rgb(0x3b, 0x42, 0x52),
        bg_tertiary: Rgba::rgb(0x43, 0x4c, 0x5e),
        bg_hover: Rgba::rgb(0x4c, 0x56, 0x6a),
        text_primary: Rgba::rgb(0xd8, 0xde, 0xe9),
        text_secondary: Rgba::rgb(0xe5, 0xe9, 0xf0),
        text_muted: Rgba::rgb(0x8f, 0xbc, 0xbb),
        text_disabled: Rgba::rgb(0x4c, 0x56, 0x6a),
        accent: Rgba::rgb(0x88, 0xc0, 0xd0),
        accent_hover: Rgba::rgb(0x81, 0xa1, 0xc1),
        accent_pressed: Rgba::rgb(0x5e, 0x81, 0xac),
        accent_text: Rgba::rgb(0x2e, 0x34, 0x40),
        danger: Rgba::rgb(0xbf, 0x61, 0x6a),
        warning: Rgba::rgb(0xd0, 0x87, 0x70),
        border_subtle: Rgba::rgb(0x4c, 0x56, 0x6a),
        border_strong: Rgba::rgb(0x43, 0x4c, 0x5e),
        ..Default::default()
    };
    s.build(bg_is_light(s.bg_primary))
}

/// langley — registered `type:light` in Tauri but a DEEP-MAROON dark canvas
/// (`#2c0a0a`). Luminance-derived polarity -> white alpha base. (doc 01 §langley)
pub(super) fn langley() -> ThemeColors {
    let s = StdSpec {
        bg_primary: Rgba::rgb(0x2c, 0x0a, 0x0a),
        bg_secondary: Rgba::rgb(0x3a, 0x0e, 0x0e),
        bg_tertiary: Rgba::rgb(0x4c, 0x14, 0x13),
        bg_hover: Rgba::rgb(0x71, 0x1c, 0x1c),
        text_primary: Rgba::rgb(0xf2, 0xda, 0xda),
        text_secondary: Rgba::rgb(0xd9, 0xa3, 0xa3),
        text_muted: Rgba::rgb(0xa9, 0x7b, 0x7b),
        text_disabled: Rgba::rgb(0x7a, 0x3d, 0x3d),
        accent: Rgba::rgb(0xe6, 0x7e, 0x22),
        accent_hover: Rgba::rgb(0xf3, 0x9c, 0x4d),
        accent_pressed: Rgba::rgb(0xd8, 0x6b, 0x1f),
        accent_text: Rgba::rgb(0x2c, 0x0a, 0x0a),
        danger: Rgba::rgb(0xc0, 0x39, 0x2b),
        warning: Rgba::rgb(0xe5, 0xa6, 0x3d),
        border_subtle: Rgba::rgb(0x3a, 0x0e, 0x0e),
        border_strong: Rgba::rgb(0x4c, 0x14, 0x13),
        ..Default::default()
    };
    s.build(bg_is_light(s.bg_primary))
}

