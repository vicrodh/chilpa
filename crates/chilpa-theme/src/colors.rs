//! The fully-materialized per-theme color set.
//!
//! Every theme row in the registry returns one of these with EVERY field
//! populated: there is no cascade, so what a theme omitted in QBZ's first
//! frontend is resolved against Dark at registry-build time.

use crate::color::Rgba;

/// The 24 alpha tiers, in ascending percentage order. Index into
/// [`ThemeColors::alpha`] with [`alpha_index`].
pub const ALPHA_PERCENTS: [u8; 24] = [
    4, 5, 6, 8, 10, 12, 15, 18, 20, 25, 30, 35, 40, 45, 50, 55, 60, 65, 70, 75, 80, 85, 90, 95,
];

/// Number of alpha tiers (= `ALPHA_PERCENTS.len()`).
pub const ALPHA_COUNT: usize = 24;

/// Map an alpha percentage to its `0xAA` byte (rounded). `pct * 255 / 100`.
pub const fn alpha_byte(pct: u8) -> u8 {
    ((pct as u16 * 255 + 50) / 100) as u8
}

/// Position of an alpha percentage within [`ALPHA_PERCENTS`], or `None`.
pub fn alpha_index(pct: u8) -> Option<usize> {
    let mut i = 0;
    while i < ALPHA_COUNT {
        if ALPHA_PERCENTS[i] == pct {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// The complete color contract for one theme. Field order groups by family
/// (surfaces, text, accent, danger, warning, success, borders, focus,
/// extras, alpha).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemeColors {
    // --- surfaces ---
    pub surface_main: Rgba,
    pub surface_card: Rgba,
    pub surface_elevated: Rgba,
    /// Alpha-based hover overlay (translucent, polarity-correct).
    pub surface_hover: Rgba,
    /// Opaque theme `--bg-hover` hex (distinct from the alpha `surface_hover`).
    pub bg_hover: Rgba,

    // --- text ---
    pub text_primary: Rgba,
    pub text_secondary: Rgba,
    pub text_muted: Rgba,
    pub text_disabled: Rgba,

    // --- accent ---
    pub accent: Rgba,
    pub accent_hover: Rgba,
    pub accent_pressed: Rgba,
    /// Text drawn ON an accent fill (`--btn-primary-text`).
    pub accent_text: Rgba,

    // --- danger family ---
    pub danger: Rgba,
    pub danger_bg: Rgba,
    pub danger_border: Rgba,
    pub danger_hover: Rgba,

    // --- warning family ---
    pub warning: Rgba,
    pub warning_bg: Rgba,
    pub warning_border: Rgba,
    pub warning_hover: Rgba,

    // --- success family (NEW vs Tauri parity) ---
    pub success: Rgba,
    pub success_bg: Rgba,
    pub success_border: Rgba,
    pub success_hover: Rgba,

    // --- borders ---
    /// The subtle border. The four original dark themes keep a translucent
    /// white hairline here; the standard and accessibility rows an opaque hex.
    pub border_subtle: Rgba,
    /// A stronger translucent edge, used by popovers and dropdowns.
    pub border_muted: Rgba,
    pub border_strong: Rgba,

    // --- focus (NEW) ---
    pub focus_ring: Rgba,

    // --- extras ---
    pub favorite: Rgba,
    pub card_shadow: Rgba,

    // --- alpha overlays (polarity baked in: white on dark, black on light) ---
    pub alpha: [Rgba; ALPHA_COUNT],
}

impl ThemeColors {
    /// Look up an alpha overlay by percentage (e.g. `8`, `55`). Falls back to
    /// the nearest standard tier if an exact match is absent.
    pub fn alpha_pct(&self, pct: u8) -> Rgba {
        if let Some(i) = alpha_index(pct) {
            return self.alpha[i];
        }
        // Nearest tier by absolute distance.
        let mut best = 0usize;
        let mut best_d = u8::MAX as i32;
        for (i, &p) in ALPHA_PERCENTS.iter().enumerate() {
            let d = (p as i32 - pct as i32).abs();
            if d < best_d {
                best_d = d;
                best = i;
            }
        }
        self.alpha[best]
    }
}

impl ThemeColors {
    /// Every colour as `#rrggbbaa`, one per line, in field order: how the
    /// facade keeps the last auto theme between runs.
    pub fn to_hex_list(&self) -> String {
        let hex = |c: &Rgba| format!("#{:02x}{:02x}{:02x}{:02x}", c.r, c.g, c.b, c.a);
        self.named().iter().map(|c| hex(c)).chain(self.alpha.iter().map(hex)).collect::<Vec<_>>().join("\n")
    }

    /// The colours [`Self::to_hex_list`] wrote; `None` unless every line
    /// reads.
    pub fn from_hex_list(text: &str) -> Option<ThemeColors> {
        let list: Vec<Rgba> = text.lines().map(Rgba::from_hex).collect::<Option<_>>()?;
        let named = list.get(..NAMED)?;
        let alpha: [Rgba; ALPHA_COUNT] = list.get(NAMED..)?.try_into().ok()?;
        let n = |i: usize| named[i];
        Some(ThemeColors {
            surface_main: n(0),
            surface_card: n(1),
            surface_elevated: n(2),
            surface_hover: n(3),
            bg_hover: n(4),
            text_primary: n(5),
            text_secondary: n(6),
            text_muted: n(7),
            text_disabled: n(8),
            accent: n(9),
            accent_hover: n(10),
            accent_pressed: n(11),
            accent_text: n(12),
            danger: n(13),
            danger_bg: n(14),
            danger_border: n(15),
            danger_hover: n(16),
            warning: n(17),
            warning_bg: n(18),
            warning_border: n(19),
            warning_hover: n(20),
            success: n(21),
            success_bg: n(22),
            success_border: n(23),
            success_hover: n(24),
            border_subtle: n(25),
            border_muted: n(26),
            border_strong: n(27),
            focus_ring: n(28),
            favorite: n(29),
            card_shadow: n(30),
            alpha,
        })
    }

    fn named(&self) -> [Rgba; NAMED] {
        [
            self.surface_main,
            self.surface_card,
            self.surface_elevated,
            self.surface_hover,
            self.bg_hover,
            self.text_primary,
            self.text_secondary,
            self.text_muted,
            self.text_disabled,
            self.accent,
            self.accent_hover,
            self.accent_pressed,
            self.accent_text,
            self.danger,
            self.danger_bg,
            self.danger_border,
            self.danger_hover,
            self.warning,
            self.warning_bg,
            self.warning_border,
            self.warning_hover,
            self.success,
            self.success_bg,
            self.success_border,
            self.success_hover,
            self.border_subtle,
            self.border_muted,
            self.border_strong,
            self.focus_ring,
            self.favorite,
            self.card_shadow,
        ]
    }
}

/// The named colours of [`ThemeColors`], before its alpha tiers.
const NAMED: usize = 31;

/// Build the 24-tier alpha ramp for a theme of the given polarity.
/// `is_light` themes get a BLACK base (dark hairlines/hovers read on light
/// surfaces); dark themes get a WHITE base.
pub fn alpha_ramp(is_light: bool) -> [Rgba; ALPHA_COUNT] {
    let (r, g, b) = if is_light { (0, 0, 0) } else { (255, 255, 255) };
    let mut out = [Rgba::rgba(r, g, b, 0); ALPHA_COUNT];
    let mut i = 0;
    while i < ALPHA_COUNT {
        out[i] = Rgba::rgba(r, g, b, alpha_byte(ALPHA_PERCENTS[i]));
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alpha_byte_rounds() {
        assert_eq!(alpha_byte(8), 0x14); // 8% -> 20
        assert_eq!(alpha_byte(10), 0x1a); // 10% -> 26
        assert_eq!(alpha_byte(12), 0x1f); // 12% -> 31
        assert_eq!(alpha_byte(18), 0x2e); // 18% -> 46
        assert_eq!(alpha_byte(55), 0x8c); // 55% -> 140
        assert_eq!(alpha_byte(65), 0xa6); // 65% -> 166
        assert_eq!(alpha_byte(70), 0xb3); // 70% -> 179
        assert_eq!(alpha_byte(75), 0xbf); // 75% -> 191
    }

    #[test]
    fn ramp_polarity() {
        let dark = alpha_ramp(false);
        let light = alpha_ramp(true);
        assert_eq!(dark[alpha_index(8).unwrap()], Rgba::rgba(255, 255, 255, 0x14));
        assert_eq!(light[alpha_index(8).unwrap()], Rgba::rgba(0, 0, 0, 0x14));
    }

    #[test]
    fn the_hex_list_round_trips_every_color() {
        for &id in crate::ALL {
            let colors = crate::palette(id);
            assert_eq!(ThemeColors::from_hex_list(&colors.to_hex_list()), Some(colors), "{}", id.slug());
        }
        assert_eq!(ThemeColors::from_hex_list("#000000ff\nnope"), None);
        assert_eq!(ThemeColors::from_hex_list(""), None);
    }

    #[test]
    fn alpha_count_is_24() {
        assert_eq!(ALPHA_COUNT, 24);
        assert_eq!(ALPHA_PERCENTS.len(), 24);
    }
}
