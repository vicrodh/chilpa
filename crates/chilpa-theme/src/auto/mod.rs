//! Auto-theme generation: derive a full [`ThemeColors`] set from the desktop
//! environment (accent / full color scheme), the wallpaper, or a user-picked
//! image. Ported 1:1 from QBZ 2.1.2 (`qbz-theme/src/auto/`).
//!
//! The palette math (k-means, WCAG contrast, HSL shifts) is QBZ's; the final
//! assembly mirrors the registry's `StdSpec::build`, so a generated theme
//! composites identically to a static one (same success/focus/favorite/
//! border-muted derivations, same polarity-driven alpha ramp).

pub mod generator;
pub mod palette;
pub mod system;

use serde::{Deserialize, Serialize};

pub use generator::{theme_from_palette, theme_from_scheme};
pub use system::{
    detect_desktop_environment, get_system_accent_color, get_system_color_scheme,
    get_system_wallpaper, DesktopEnvironment,
};

use crate::colors::ThemeColors;

/// Where an auto theme sources its colors from: `system`, `wallpaper` or
/// `image`, as QBZ persists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutoSource {
    /// Full DE color scheme, with a wallpaper-extraction fallback (the store's
    /// `system` cascade: scheme → wallpaper → error).
    System,
    /// Extract a palette from the current desktop wallpaper.
    Wallpaper,
    /// Extract a palette from a user-picked image at this path.
    Image(String),
}

/// Generate a [`ThemeColors`] set for the given source.
///
/// QBZ's three sources plus its `system` cascade: a `system` request falls
/// back to wallpaper extraction when the desktop exposes no readable color
/// scheme, and only errors when both fail.
///
/// `wallpaper` finds the desktop's wallpaper. The facade hands in the
/// resolver its wallpaper background already uses, which knows more
/// desktops and platforms, falling back to [`system::get_system_wallpaper`].
pub fn generate(source: &AutoSource, wallpaper: impl FnOnce() -> Result<String, String>) -> Result<ThemeColors, String> {
    match source {
        AutoSource::System => match system::get_system_color_scheme() {
            Ok(scheme) => Ok(generator::theme_from_scheme(&scheme)),
            Err(scheme_err) => {
                // Cascade: full color scheme -> wallpaper -> error.
                let wallpaper = wallpaper().map_err(|wp_err| {
                    format!(
                        "Could not read system color scheme ({scheme_err}) or wallpaper ({wp_err})"
                    )
                })?;
                let palette = palette::extract_palette(&wallpaper)?;
                Ok(generator::theme_from_palette(&palette))
            }
        },
        AutoSource::Wallpaper => {
            let wallpaper = wallpaper()?;
            let palette = palette::extract_palette(&wallpaper)?;
            Ok(generator::theme_from_palette(&palette))
        }
        AutoSource::Image(path) => {
            let palette = palette::extract_palette(path)?;
            Ok(generator::theme_from_palette(&palette))
        }
    }
}

/// A single RGB color used throughout palette extraction and generation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct PaletteColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl PaletteColor {
    pub fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Relative luminance (ITU-R BT.709) in [0.0, 1.0].
    pub fn luminance(&self) -> f64 {
        fn linearize(c: u8) -> f64 {
            let s = c as f64 / 255.0;
            if s <= 0.04045 {
                s / 12.92
            } else {
                ((s + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * linearize(self.r) + 0.7152 * linearize(self.g) + 0.0722 * linearize(self.b)
    }

    /// HSL saturation in [0.0, 1.0].
    pub fn saturation(&self) -> f64 {
        let (r, g, b) = (
            self.r as f64 / 255.0,
            self.g as f64 / 255.0,
            self.b as f64 / 255.0,
        );
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let delta = max - min;
        if delta < 1e-6 {
            return 0.0;
        }
        let l = (max + min) / 2.0;
        if l <= 0.5 {
            delta / (max + min)
        } else {
            delta / (2.0 - max - min)
        }
    }

    /// WCAG contrast ratio against another color (range [1, 21]).
    pub fn contrast_ratio(&self, other: &PaletteColor) -> f64 {
        let l1 = self.luminance();
        let l2 = other.luminance();
        let (lighter, darker) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
        (lighter + 0.05) / (darker + 0.05)
    }

    /// Shift lightness by `amount` (-1.0 to 1.0) in HSL space. Returns a new color.
    pub fn shift_lightness(&self, amount: f64) -> PaletteColor {
        let (h, s, l) = self.to_hsl();
        let new_l = (l + amount).clamp(0.0, 1.0);
        PaletteColor::from_hsl(h, s, new_l)
    }

    /// Convert to HSL (h in [0, 360), s and l in [0, 1]).
    pub fn to_hsl(&self) -> (f64, f64, f64) {
        let (r, g, b) = (
            self.r as f64 / 255.0,
            self.g as f64 / 255.0,
            self.b as f64 / 255.0,
        );
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let l = (max + min) / 2.0;
        let delta = max - min;

        if delta < 1e-6 {
            return (0.0, 0.0, l);
        }

        let s = if l <= 0.5 {
            delta / (max + min)
        } else {
            delta / (2.0 - max - min)
        };

        let h = if (max - r).abs() < 1e-6 {
            ((g - b) / delta) % 6.0
        } else if (max - g).abs() < 1e-6 {
            (b - r) / delta + 2.0
        } else {
            (r - g) / delta + 4.0
        };
        let h = (h * 60.0 + 360.0) % 360.0;

        (h, s, l)
    }

    /// Construct from HSL values.
    pub fn from_hsl(h: f64, s: f64, l: f64) -> PaletteColor {
        if s < 1e-6 {
            let v = (l * 255.0).round() as u8;
            return PaletteColor::new(v, v, v);
        }

        let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
        let h_prime = h / 60.0;
        let x = c * (1.0 - (h_prime % 2.0 - 1.0).abs());
        let m = l - c / 2.0;

        let (r1, g1, b1) = match h_prime as u32 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };

        PaletteColor::new(
            ((r1 + m) * 255.0).round() as u8,
            ((g1 + m) * 255.0).round() as u8,
            ((b1 + m) * 255.0).round() as u8,
        )
    }

    /// Euclidean distance in RGB space.
    pub fn distance(&self, other: &PaletteColor) -> f64 {
        let red = self.r as f64 - other.r as f64;
        let green = self.g as f64 - other.g as f64;
        let blue = self.b as f64 - other.b as f64;
        (red * red + green * green + blue * blue).sqrt()
    }
}

/// Extracted palette from an image (dominant surfaces + accent + polarity).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemePalette {
    pub bg_primary: PaletteColor,
    pub bg_secondary: PaletteColor,
    pub bg_tertiary: PaletteColor,
    pub bg_hover: PaletteColor,
    pub accent: PaletteColor,
    pub is_dark: bool,
    pub all_colors: Vec<PaletteColor>,
}

/// Full color scheme read from the desktop environment (KDE kdeglobals, GNOME
/// dconf). Each field is optional because not all DEs expose all roles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemColorScheme {
    // Backgrounds
    pub window_bg: Option<PaletteColor>,
    pub window_bg_alt: Option<PaletteColor>,
    pub view_bg: Option<PaletteColor>,
    pub button_bg: Option<PaletteColor>,
    pub header_bg: Option<PaletteColor>,
    pub header_bg_inactive: Option<PaletteColor>,
    pub tooltip_bg: Option<PaletteColor>,

    // Foregrounds (text)
    pub window_fg: Option<PaletteColor>,
    pub window_fg_inactive: Option<PaletteColor>,
    pub view_fg: Option<PaletteColor>,
    pub button_fg: Option<PaletteColor>,

    // Selection / accent
    pub selection_bg: Option<PaletteColor>,
    pub selection_fg: Option<PaletteColor>,
    pub selection_hover: Option<PaletteColor>,
    pub accent: Option<PaletteColor>,

    // Semantic
    pub fg_link: Option<PaletteColor>,
    pub fg_negative: Option<PaletteColor>,
    pub fg_neutral: Option<PaletteColor>,
    pub fg_positive: Option<PaletteColor>,

    // Window manager
    pub wm_active_bg: Option<PaletteColor>,
    pub wm_active_fg: Option<PaletteColor>,
    pub wm_inactive_bg: Option<PaletteColor>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn luminance_black_white() {
        assert!((PaletteColor::new(0, 0, 0).luminance() - 0.0).abs() < 1e-4);
        assert!((PaletteColor::new(255, 255, 255).luminance() - 1.0).abs() < 1e-4);
    }

    #[test]
    fn contrast_ratio_bw_is_21() {
        let ratio = PaletteColor::new(0, 0, 0).contrast_ratio(&PaletteColor::new(255, 255, 255));
        assert!((ratio - 21.0).abs() < 0.1);
    }

    #[test]
    fn hsl_roundtrip() {
        let c = PaletteColor::new(66, 133, 244);
        let (h, s, l) = c.to_hsl();
        let back = PaletteColor::from_hsl(h, s, l);
        assert!((c.r as i16 - back.r as i16).unsigned_abs() <= 1);
        assert!((c.g as i16 - back.g as i16).unsigned_abs() <= 1);
        assert!((c.b as i16 - back.b as i16).unsigned_abs() <= 1);
    }

    #[test]
    fn saturation_gray_is_zero() {
        assert!(PaletteColor::new(128, 128, 128).saturation() < 0.01);
    }
}
