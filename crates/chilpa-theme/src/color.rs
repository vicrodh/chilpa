//! Plain RGBA color + hand-rolled contrast math (WCAG 2.x + APCA approximation).
//!
//! No external color crate: the few formulas the registry needs live here,
//! as in QBZ 2.1.2 (`qbz-theme/src/color.rs`).

/// 8-bit-per-channel color with straight (non-premultiplied) alpha.
///
/// `a == 255` is fully opaque, `a == 0` fully transparent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    /// Opaque color from 8-bit channels.
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// Color with explicit alpha.
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Parse `#rrggbb` / `rrggbb` / `#rrggbbaa` (case-insensitive). Returns
    /// `None` on any malformed input. `const`-incompatible (loops), used only
    /// in tests + as a convenience.
    pub fn from_hex(s: &str) -> Option<Self> {
        let h = s.strip_prefix('#').unwrap_or(s);
        let bytes = h.as_bytes();
        let hx = |hi: u8, lo: u8| -> Option<u8> {
            let v = |c: u8| -> Option<u8> {
                match c {
                    b'0'..=b'9' => Some(c - b'0'),
                    b'a'..=b'f' => Some(c - b'a' + 10),
                    b'A'..=b'F' => Some(c - b'A' + 10),
                    _ => None,
                }
            };
            Some(v(hi)? * 16 + v(lo)?)
        };
        match bytes.len() {
            6 => Some(Self::rgb(
                hx(bytes[0], bytes[1])?,
                hx(bytes[2], bytes[3])?,
                hx(bytes[4], bytes[5])?,
            )),
            8 => Some(Self::rgba(
                hx(bytes[0], bytes[1])?,
                hx(bytes[2], bytes[3])?,
                hx(bytes[4], bytes[5])?,
                hx(bytes[6], bytes[7])?,
            )),
            _ => None,
        }
    }

    /// Format as `#rrggbb` (lowercase, alpha dropped). The custom-theme base
    /// tokens are all opaque, so the alpha channel is intentionally not
    /// serialized — the picker HEX field and the on-disk `custom_theme.json`
    /// both round-trip through this 6-digit form.
    pub fn to_hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

/// sRGB 0..=255 channel -> linear-light 0.0..=1.0 (WCAG 2.x transfer function).
fn srgb_to_linear(c: u8) -> f64 {
    let cs = c as f64 / 255.0;
    if cs <= 0.040_45 {
        cs / 12.92
    } else {
        ((cs + 0.055) / 1.055).powf(2.4)
    }
}

/// WCAG 2.x relative luminance of an OPAQUE color (alpha ignored).
/// `Y = 0.2126 R + 0.7152 G + 0.0722 B` on linear channels.
pub fn relative_luminance(c: Rgba) -> f64 {
    0.2126 * srgb_to_linear(c.r) + 0.7152 * srgb_to_linear(c.g) + 0.0722 * srgb_to_linear(c.b)
}

/// WCAG 2.x contrast ratio between two opaque colors, in `[1.0, 21.0]`.
/// `(L_lighter + 0.05) / (L_darker + 0.05)`. Order-independent.
pub fn contrast_ratio(a: Rgba, b: Rgba) -> f64 {
    let la = relative_luminance(a);
    let lb = relative_luminance(b);
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

// --- APCA (Lc) approximation ------------------------------------------------
//
// A compact port of the APCA-W3 0.1.9 "Lc" contrast estimator. Sign convention:
// negative Lc = light text on a dark background, positive Lc = dark text on a
// light background. We only ever consume |Lc| against thresholds, so the sign is
// informational. This is an APPROXIMATION (the official lookup-table clamps and
// the soft-black/low-contrast roll-offs are reproduced); it is used as a
// secondary gate in a11y unit tests, never for production rendering.

const APCA_SRGB_R: f64 = 0.2126729;
const APCA_SRGB_G: f64 = 0.7151522;
const APCA_SRGB_B: f64 = 0.0721750;

const APCA_MAIN_TRC: f64 = 2.4;
const APCA_NORM_BG: f64 = 0.56;
const APCA_NORM_TXT: f64 = 0.57;
const APCA_REV_BG: f64 = 0.62;
const APCA_REV_TXT: f64 = 0.65;

const APCA_BLK_THRS: f64 = 0.022;
const APCA_BLK_CLMP: f64 = 1.414;
const APCA_SCALE: f64 = 1.14;
const APCA_LO_CLIP: f64 = 0.1;
const APCA_DELTA_Y_MIN: f64 = 0.0005;

fn apca_screen_y(c: Rgba) -> f64 {
    let lin = |v: u8| (v as f64 / 255.0).powf(APCA_MAIN_TRC);
    APCA_SRGB_R * lin(c.r) + APCA_SRGB_G * lin(c.g) + APCA_SRGB_B * lin(c.b)
}

fn apca_soft_clamp(mut y: f64) -> f64 {
    if y < 0.0 {
        y = 0.0;
    }
    if y < APCA_BLK_THRS {
        y += (APCA_BLK_THRS - y).powf(APCA_BLK_CLMP);
    }
    y
}

/// APCA Lc estimate for `text` on `bg`. See module note on sign + accuracy.
pub fn apca_lc(text: Rgba, bg: Rgba) -> f64 {
    let txt_y = apca_soft_clamp(apca_screen_y(text));
    let bg_y = apca_soft_clamp(apca_screen_y(bg));

    if (bg_y - txt_y).abs() < APCA_DELTA_Y_MIN {
        return 0.0;
    }

    let out;
    if bg_y > txt_y {
        // Normal polarity: dark text on light bg -> positive Lc.
        let c = (bg_y.powf(APCA_NORM_BG) - txt_y.powf(APCA_NORM_TXT)) * APCA_SCALE;
        out = if c < APCA_LO_CLIP { 0.0 } else { c - APCA_LO_CLIP * 0.027 };
    } else {
        // Reverse polarity: light text on dark bg -> negative Lc.
        let c = (bg_y.powf(APCA_REV_BG) - txt_y.powf(APCA_REV_TXT)) * APCA_SCALE;
        out = if c > -APCA_LO_CLIP { 0.0 } else { c + APCA_LO_CLIP * 0.027 };
    }
    out * 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip() {
        assert_eq!(Rgba::from_hex("#0f0f0f"), Some(Rgba::rgb(15, 15, 15)));
        assert_eq!(Rgba::from_hex("ffffff"), Some(Rgba::rgb(255, 255, 255)));
        assert_eq!(Rgba::from_hex("#ffffff14"), Some(Rgba::rgba(255, 255, 255, 0x14)));
        assert_eq!(Rgba::from_hex("nope"), None);
    }

    #[test]
    fn black_white_contrast_is_21() {
        let r = contrast_ratio(Rgba::rgb(0, 0, 0), Rgba::rgb(255, 255, 255));
        assert!((r - 21.0).abs() < 0.01, "got {r}");
    }

    #[test]
    fn contrast_is_symmetric_and_min_one() {
        let a = Rgba::rgb(0x42, 0x85, 0xf4);
        let b = Rgba::rgb(0x0f, 0x0f, 0x0f);
        assert!((contrast_ratio(a, b) - contrast_ratio(b, a)).abs() < 1e-9);
        assert!(contrast_ratio(a, a) >= 0.999);
    }

    #[test]
    fn apca_sign_convention() {
        // light text on dark bg -> negative
        let lc = apca_lc(Rgba::rgb(255, 255, 255), Rgba::rgb(15, 15, 15));
        assert!(lc < -60.0, "white on near-black should be strongly negative, got {lc}");
        // dark text on light bg -> positive
        let lc = apca_lc(Rgba::rgb(0, 0, 0), Rgba::rgb(255, 255, 255));
        assert!(lc > 90.0, "black on white should be strongly positive, got {lc}");
    }
}
