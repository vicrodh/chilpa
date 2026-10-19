//! The theme registry: `ThemeId` -> fully-materialized [`ThemeColors`].
//!
//! Ported 1:1 from QBZ 2.1.2 (`qbz-theme/src/registry.rs`), split by
//! category so no file passes the size cap: the core rows and the shared
//! builder live here, the branded dark rows in `dark`, the light ones in
//! `light` and the five accessibility rows in `accessibility`.
//!
//! Every row is FULLY materialized: the themes QBZ inherited from as a CSS
//! cascade (`light` omits the accent trio, `oled`/`breeze-dark`/
//! `adwaita-dark` omit whole danger/warning families) were resolved against
//! `dark()` at transcription time, here, not at runtime.
//!
//! Derived tokens for the standard rows:
//!   - `success` family: `#3fae6a` on dark themes and a darker `#1f8a4c` on
//!     light ones (so success text clears >=3:1 on a light surface), with the
//!     same 0.1/0.3/0.2 tint shape for bg/border/hover.
//!   - `focus_ring`: the theme accent.
//!   - `favorite`: the theme `danger` hue.
//!   - `danger_bg/border/hover`, `warning_*`: straight-alpha tints of the
//!     solid hue at 0.1/0.3/0.2 (dracula uses 0.15/0.4/0.25).
//!   - `border_muted`: a polarity-aware translucent edge (white ~22% on dark,
//!     black ~22% on light).
//!   - `surface_hover`: an alpha hover overlay, polarity-aware (white ~6% on
//!     dark, black ~6% on light), distinct from the opaque `bg_hover`.

mod accessibility;
mod dark;
mod light;
#[cfg(test)]
mod tests;

use accessibility::*;
use dark::*;
use light::*;

use crate::color::{relative_luminance, Rgba};
use crate::colors::{alpha_ramp, ThemeColors};
use crate::id::ThemeId;

/// The translucent edges the four original dark themes use directly:
///   surface-hover  = ~6% white  (#ffffff10)
///   border-subtle  = ~8% white  (#ffffff14)
///   border-muted   = ~22% white (#ffffff38)
///   card-shadow    = rgba(0,0,0,0.4) (#00000066)
const LEGACY_SURFACE_HOVER: Rgba = Rgba::rgba(255, 255, 255, 0x10);
const LEGACY_BORDER_SUBTLE: Rgba = Rgba::rgba(255, 255, 255, 0x14);
const LEGACY_BORDER_MUTED: Rgba = Rgba::rgba(255, 255, 255, 0x38);
const LEGACY_CARD_SHADOW: Rgba = Rgba::rgba(0, 0, 0, 0x66);

/// Resolve a theme id to its concrete color set.
///
/// `System` has no static palette: it follows the desktop. This returns the
/// Dark set for any caller that needs a concrete struct for it.
pub fn palette(id: ThemeId) -> ThemeColors {
    match id {
        // --- Core (P1 + the standard Light) ---
        ThemeId::Dark => dark(),
        ThemeId::Oled => oled(),
        ThemeId::TokyoNight => tokyo_night(),
        ThemeId::System => dark(),
        ThemeId::Light => light(),
        // --- Dark (branded / community) ---
        ThemeId::Warm => warm(),
        ThemeId::Nord => nord(),
        ThemeId::Dracula => dracula(),
        ThemeId::CatppuccinMocha => catppuccin_mocha(),
        ThemeId::CatppuccinLatte => catppuccin_latte(),
        ThemeId::CatppuccinFrappe => catppuccin_frappe(),
        ThemeId::CatppuccinMacchiato => catppuccin_macchiato(),
        ThemeId::BreezeDark => breeze_dark(),
        ThemeId::AdwaitaDark => adwaita_dark(),
        ThemeId::Aurora => aurora(),
        ThemeId::Ikari => ikari(),
        ThemeId::Ayanami => ayanami(),
        ThemeId::Iscariot => iscariot(),
        ThemeId::Stratego => stratego(),
        ThemeId::Rumi => rumi(),
        ThemeId::Zoey => zoey(),
        ThemeId::Mira => mira(),
        ThemeId::Frost => frost(),
        ThemeId::Langley => langley(),
        // --- Light (branded / community) ---
        ThemeId::Alucard => alucard(),
        ThemeId::RosePineDawn => rose_pine_dawn(),
        ThemeId::BreezeLight => breeze_light(),
        ThemeId::AdwaitaLight => adwaita_light(),
        ThemeId::DuotoneSnow => duotone_snow(),
        ThemeId::SnowStorm => snow_storm(),
        ThemeId::Kurosaki => kurosaki(),
        // --- Accessibility (REDESIGNED in P3): final verified palettes from
        // 99-MIGRATION-PLAN.md Part B (adversarial corrections folded in). ---
        ThemeId::WcagLight => wcag_light(),
        ThemeId::WcagDark => wcag_dark(),
        ThemeId::HighContrast => high_contrast(),
        ThemeId::HighContrastLight => high_contrast_light(),
        ThemeId::Colorblind => colorblind(),
    }
}

// ---------------------------------------------------------------------------
// Shared builder for the standard themes
// ---------------------------------------------------------------------------

/// The Tauri token set for one standard theme, as read from doc 01. Only the
/// named hues are carried here; the derived families (success/focus/favorite),
/// the polarity-driven alpha ramp + translucent edges, and the status tints are
/// materialized by [`StdSpec::build`].
#[derive(Clone, Copy)]
struct StdSpec {
    // surfaces (--bg-*)
    bg_primary: Rgba,
    bg_secondary: Rgba,
    bg_tertiary: Rgba,
    bg_hover: Rgba,
    // text (--text-*)
    text_primary: Rgba,
    text_secondary: Rgba,
    text_muted: Rgba,
    text_disabled: Rgba,
    // accent (--accent-* + --btn-primary-text)
    accent: Rgba,
    accent_hover: Rgba,
    accent_pressed: Rgba,
    accent_text: Rgba,
    // status hues (--danger / --warning); families derived as rgba() tints
    danger: Rgba,
    warning: Rgba,
    /// Tint fractions for the danger/warning bg/border/hover families.
    /// Standard themes use (0.1, 0.3, 0.2); dracula uses (0.15, 0.4, 0.25).
    tint_bg: f32,
    tint_border: f32,
    tint_hover: f32,
    // borders (--border-*)
    border_subtle: Rgba,
    border_strong: Rgba,
}

impl StdSpec {
    /// Default status-tint fractions (every theme except dracula).
    const TINT_BG: f32 = 0.1;
    const TINT_BORDER: f32 = 0.3;
    const TINT_HOVER: f32 = 0.2;

    /// Materialize a complete [`ThemeColors`] row. `is_light` is the corrected
    /// (luminance-derived) polarity — it drives the alpha ramp base (black on
    /// light, white on dark), the translucent edge/hover bases, and the derived
    /// `success` hue. NOTE: do NOT trust the Tauri `type` flag for this; pass the
    /// real luminance (Frost/Langley are registered light but are dark canvases).
    fn build(self, is_light: bool) -> ThemeColors {
        // success: NEW token, no Tauri parity. Theme-appropriate green that
        // clears >=3:1 on the theme surface; darker on light themes. Polished P4.
        let success = if is_light {
            Rgba::rgb(0x1f, 0x8a, 0x4c)
        } else {
            Rgba::rgb(0x3f, 0xae, 0x6a)
        };

        // Polarity-aware translucent edges (legacy Slint-only tokens). On light
        // themes a white hairline is invisible, so flip the base to black.
        let (eh, eg, eb) = if is_light { (0, 0, 0) } else { (255, 255, 255) };
        let surface_hover = Rgba::rgba(eh, eg, eb, 0x10); // ~6%
        let border_muted = Rgba::rgba(eh, eg, eb, 0x38); // ~22%

        ThemeColors {
            surface_main: self.bg_primary,
            surface_card: self.bg_secondary,
            surface_elevated: self.bg_tertiary,
            surface_hover,
            bg_hover: self.bg_hover,

            text_primary: self.text_primary,
            text_secondary: self.text_secondary,
            text_muted: self.text_muted,
            text_disabled: self.text_disabled,

            accent: self.accent,
            accent_hover: self.accent_hover,
            accent_pressed: self.accent_pressed,
            accent_text: self.accent_text,

            danger: self.danger,
            danger_bg: with_alpha(self.danger, self.tint_bg),
            danger_border: with_alpha(self.danger, self.tint_border),
            danger_hover: with_alpha(self.danger, self.tint_hover),

            warning: self.warning,
            warning_bg: with_alpha(self.warning, self.tint_bg),
            warning_border: with_alpha(self.warning, self.tint_border),
            warning_hover: with_alpha(self.warning, self.tint_hover),

            success,
            success_bg: with_alpha(success, self.tint_bg),
            success_border: with_alpha(success, self.tint_border),
            success_hover: with_alpha(success, self.tint_hover),

            // Standard rows feed the theme `--border-subtle` hex (NOT the legacy
            // translucent hairline the 4 P1 rows kept).
            border_subtle: self.border_subtle,
            border_muted,
            border_strong: self.border_strong,

            focus_ring: self.accent, // = accent (no Tauri token; new)

            favorite: self.danger, // loved-heart uses danger red
            card_shadow: LEGACY_CARD_SHADOW,

            alpha: alpha_ramp(is_light),
        }
    }
}

impl Default for StdSpec {
    /// All-black placeholder; every field is overwritten per theme. The default
    /// only exists so theme functions can use struct-update syntax for the tint
    /// fractions without repeating them.
    fn default() -> Self {
        let z = Rgba::rgb(0, 0, 0);
        StdSpec {
            bg_primary: z,
            bg_secondary: z,
            bg_tertiary: z,
            bg_hover: z,
            text_primary: z,
            text_secondary: z,
            text_muted: z,
            text_disabled: z,
            accent: z,
            accent_hover: z,
            accent_pressed: z,
            accent_text: z,
            danger: z,
            warning: z,
            tint_bg: StdSpec::TINT_BG,
            tint_border: StdSpec::TINT_BORDER,
            tint_hover: StdSpec::TINT_HOVER,
            border_subtle: z,
            border_strong: z,
        }
    }
}

/// True when a `bg-primary` reads as light (luminance >= 0.5). Drives polarity
/// for the standard rows. Matches `lib::is_light` (which calls through
/// `palette()`), but used internally to avoid a recursive `palette()` call.
fn bg_is_light(bg_primary: Rgba) -> bool {
    relative_luminance(bg_primary) >= 0.5
}

// ---------------------------------------------------------------------------
// Core themes (P1 originals + standard Light)
// ---------------------------------------------------------------------------

/// `:root` Dark — the base every other theme inherits omissions from.
/// All hex values cite `src/app.css :root` via the inventory doc.
fn dark() -> ThemeColors {
    let danger = Rgba::rgb(0xef, 0x44, 0x44); // --danger
    let warning = Rgba::rgb(0xfb, 0xbf, 0x24); // --warning
    let success = Rgba::rgb(0x3f, 0xae, 0x6a); // NEW (project green)
    ThemeColors {
        surface_main: Rgba::rgb(0x0f, 0x0f, 0x0f),     // --bg-primary
        surface_card: Rgba::rgb(0x1a, 0x1a, 0x1a),     // --bg-secondary
        surface_elevated: Rgba::rgb(0x2a, 0x2a, 0x2a), // --bg-tertiary
        surface_hover: LEGACY_SURFACE_HOVER,
        bg_hover: Rgba::rgb(0x1f, 0x1f, 0x1f), // --bg-hover

        text_primary: Rgba::rgb(0xff, 0xff, 0xff),   // --text-primary
        text_secondary: Rgba::rgb(0xcc, 0xcc, 0xcc), // --text-secondary
        text_muted: Rgba::rgb(0x88, 0x88, 0x88),     // --text-muted
        text_disabled: Rgba::rgb(0x55, 0x55, 0x55),  // --text-disabled

        accent: Rgba::rgb(0x42, 0x85, 0xf4),         // --accent-primary
        accent_hover: Rgba::rgb(0x5a, 0x9b, 0xf4),   // --accent-hover
        accent_pressed: Rgba::rgb(0x32, 0x75, 0xe4), // --accent-active
        accent_text: Rgba::rgb(0xff, 0xff, 0xff),    // --btn-primary-text

        danger,
        danger_bg: with_alpha(danger, 0.1),
        danger_border: with_alpha(danger, 0.3),
        danger_hover: with_alpha(danger, 0.2),

        warning,
        warning_bg: with_alpha(warning, 0.1),
        warning_border: with_alpha(warning, 0.3),
        warning_hover: with_alpha(warning, 0.2),

        success,
        success_bg: with_alpha(success, 0.1),
        success_border: with_alpha(success, 0.3),
        success_hover: with_alpha(success, 0.2),

        border_subtle: LEGACY_BORDER_SUBTLE,
        border_muted: LEGACY_BORDER_MUTED,
        border_strong: Rgba::rgb(0x3a, 0x3a, 0x3a), // --border-strong

        focus_ring: Rgba::rgb(0x42, 0x85, 0xf4), // = accent (no Tauri token; new)

        favorite: danger, // the loved-heart uses danger red
        card_shadow: LEGACY_CARD_SHADOW,

        alpha: alpha_ramp(false), // dark theme -> white-based overlays
    }
}

/// OLED Black — inherits everything from Dark except backgrounds + borders.
/// The legacy Slint OLED only overrode the three surfaces; keep that exactly,
/// inherit the rest from `dark()`.
fn oled() -> ThemeColors {
    ThemeColors {
        surface_main: Rgba::rgb(0x00, 0x00, 0x00),     // --bg-primary
        surface_card: Rgba::rgb(0x0a, 0x0a, 0x0a),     // --bg-secondary
        surface_elevated: Rgba::rgb(0x1a, 0x1a, 0x1a), // --bg-tertiary
        bg_hover: Rgba::rgb(0x11, 0x11, 0x11),         // --bg-hover (oled)
        border_strong: Rgba::rgb(0x2a, 0x2a, 0x2a),    // --border-strong (oled)
        ..dark()
    }
}

/// Tokyo Night — full recolor. Surfaces/text/accent transcribed from the legacy
/// Slint ternary (which matches `src/app.css [data-theme="tokyo-night"]`).
fn tokyo_night() -> ThemeColors {
    let danger = Rgba::rgb(0xdb, 0x4b, 0x4b); // --danger
    let warning = Rgba::rgb(0xe0, 0xaf, 0x68); // --warning
    let success = Rgba::rgb(0x3f, 0xae, 0x6a);
    ThemeColors {
        surface_main: Rgba::rgb(0x1a, 0x1b, 0x26),     // --bg-primary
        surface_card: Rgba::rgb(0x16, 0x16, 0x1e),     // --bg-secondary
        surface_elevated: Rgba::rgb(0x1c, 0x1d, 0x29), // --bg-tertiary
        surface_hover: LEGACY_SURFACE_HOVER,
        bg_hover: Rgba::rgb(0x20, 0x23, 0x30), // --bg-hover

        text_primary: Rgba::rgb(0xa9, 0xb1, 0xd6),   // --text-primary
        text_secondary: Rgba::rgb(0x78, 0x7c, 0x99), // --text-secondary
        text_muted: Rgba::rgb(0x54, 0x5c, 0x7e),     // --text-muted
        text_disabled: Rgba::rgb(0x3d, 0x42, 0x5e),  // --text-disabled

        accent: Rgba::rgb(0x7a, 0xa2, 0xf7),         // --accent-primary
        accent_hover: Rgba::rgb(0x7d, 0xcf, 0xff),   // --accent-hover
        accent_pressed: Rgba::rgb(0xbb, 0x9a, 0xf7), // --accent-active
        accent_text: Rgba::rgb(0x1a, 0x1b, 0x26),    // --btn-primary-text

        danger,
        danger_bg: with_alpha(danger, 0.1),
        danger_border: with_alpha(danger, 0.3),
        danger_hover: with_alpha(danger, 0.2),

        warning,
        warning_bg: with_alpha(warning, 0.1),
        warning_border: with_alpha(warning, 0.3),
        warning_hover: with_alpha(warning, 0.2),

        success,
        success_bg: with_alpha(success, 0.1),
        success_border: with_alpha(success, 0.3),
        success_hover: with_alpha(success, 0.2),

        border_subtle: LEGACY_BORDER_SUBTLE,
        border_muted: LEGACY_BORDER_MUTED,
        border_strong: Rgba::rgb(0x20, 0x23, 0x30), // --border-strong

        focus_ring: Rgba::rgb(0x7a, 0xa2, 0xf7), // = accent

        favorite: danger,
        card_shadow: LEGACY_CARD_SHADOW,

        alpha: alpha_ramp(false), // dark theme -> white-based overlays
    }
}

/// `light` — core Light theme. OMITS the accent trio (inherits the Dark blue
/// `#4285F4` family from `:root`); alpha base flips to black. (doc 01 §light)
fn light() -> ThemeColors {
    let s = StdSpec {
        bg_primary: Rgba::rgb(0xff, 0xff, 0xff),
        bg_secondary: Rgba::rgb(0xf5, 0xf5, 0xf5),
        bg_tertiary: Rgba::rgb(0xe8, 0xe8, 0xe8),
        bg_hover: Rgba::rgb(0xf0, 0xf0, 0xf0),
        text_primary: Rgba::rgb(0x0f, 0x0f, 0x0f),
        text_secondary: Rgba::rgb(0x44, 0x44, 0x44),
        text_muted: Rgba::rgb(0x66, 0x66, 0x66),
        text_disabled: Rgba::rgb(0x99, 0x99, 0x99),
        // accent trio inherited from :root Dark:
        accent: Rgba::rgb(0x42, 0x85, 0xf4),
        accent_hover: Rgba::rgb(0x5a, 0x9b, 0xf4),
        accent_pressed: Rgba::rgb(0x32, 0x75, 0xe4),
        accent_text: Rgba::rgb(0xff, 0xff, 0xff), // --btn-primary-text
        // light defines its own danger/warning hues (darker):
        danger: Rgba::rgb(0xdc, 0x26, 0x26),
        warning: Rgba::rgb(0xd9, 0x77, 0x06),
        // light uses 0.1/0.3/0.15 in app.css (hover is 0.15 not 0.2). Keep faithful.
        tint_hover: 0.15,
        border_subtle: Rgba::rgb(0xe0, 0xe0, 0xe0),
        border_strong: Rgba::rgb(0xcc, 0xcc, 0xcc),
        ..Default::default()
    };
    s.build(true)
}


/// Straight-alpha overlay of an opaque hue at `frac` opacity (0.0..=1.0).
/// Used to reproduce Tauri's `rgba(hue, frac)` danger/warning/success tints.
const fn with_alpha(c: Rgba, frac: f32) -> Rgba {
    let a = (frac * 255.0 + 0.5) as u8;
    Rgba::rgba(c.r, c.g, c.b, a)
}

