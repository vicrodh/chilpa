//! The dynamic background behind the window: which of its four looks is
//! kept, the picture of the user's own behind the wallpaper looks, how
//! blurred that wallpaper is, and the three colors a cover gives the
//! album-colored look. The looks are drawn by the window; this is what
//! they are drawn from, as QBZ's shell has it.

use crate::settings_rows::SettingsRow;
use crate::Chilpa;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

/// Which look: one of [`MODES`].
pub const MODE: &str = "appearance.background";
/// A picture of the user's own behind the wallpaper looks; empty for the
/// desktop's.
pub const IMAGE: &str = "appearance.backgroundImage";
/// How blurred the wallpaper look is, 0 to 100.
pub const BLUR: &str = "appearance.wallpaperBlur";
/// The looks, by index: off, the album-colored field, the blurred cover,
/// the desktop's wallpaper, the wallpaper blurred.
pub const MODES: [&str; 5] = ["off", "ambient", "blurred", "wallpaper", "wallpaper-blurred"];
pub const BLUR_DEFAULT: u8 = 75;

/// Whether a look paints the desktop's wallpaper, and so needs no track.
pub fn paints_wallpaper(mode: usize) -> bool {
    mode >= 3
}

/// The three colors of a cover, as `#rrggbb`: the dominant hue, a second
/// one or the same deeper, and the dominant's complement.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Triad {
    pub primary: String,
    pub secondary: String,
    pub accent: String,
    /// The halo behind the cover in the immersive view: its most
    /// saturated color that is neither dark nor pale, with its alpha.
    pub glow: String,
}

/// Until a cover yields colors: teal, purple and a lighter teal, and a
/// soft blue halo.
impl Default for Triad {
    fn default() -> Self {
        Self { primary: "#00dcc8".into(), secondary: "#9632ff".into(), accent: "#3fd9c8".into(), glow: GLOW_DEFAULT.into() }
    }
}

/// The halo with nothing better: blue at 0x59 of alpha, alpha last as the
/// theme parses colors.
pub const GLOW_DEFAULT: &str = "#6464ff59";

/// The last cover read, so a track played again costs nothing.
static LAST: Mutex<Option<(PathBuf, Option<SystemTime>, Triad)>> = Mutex::new(None);

/// The colors of the cover at `path`, read off a 16 by 16 sample of it.
/// `None` for a file that is not an image. Slow: call it off the window's
/// thread.
pub fn triad_of(path: &Path) -> Option<Triad> {
    let modified = std::fs::metadata(path).ok().and_then(|m| m.modified().ok());
    if let Some((last, at, triad)) = LAST.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        if last == path && *at == modified {
            return Some(triad.clone());
        }
    }
    let source = image::ImageReader::open(path).ok()?.with_guessed_format().ok()?.decode().ok()?.into_rgba8();
    let tiny = image::imageops::resize(&source, 16, 16, image::imageops::FilterType::Triangle);
    let (primary, secondary) = spectrum_colors(&tiny);
    let halo = image::imageops::resize(&source, 8, 8, image::imageops::FilterType::Triangle);
    let triad = Triad { primary: hex(primary), secondary: hex(secondary), accent: hex(lyrics_accent_color(&tiny)), glow: glow_hex(glow_color(&halo)) };
    *LAST.lock().unwrap_or_else(|e| e.into_inner()) = Some((path.to_path_buf(), modified, triad.clone()));
    Some(triad)
}

pub(crate) fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let (r, g, b) = (f32::from(r) / 255.0, f32::from(g) / 255.0, f32::from(b) / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let d = max - min;
    if d < 1.0e-6 {
        return (0.0, 0.0, l);
    }
    let s = (d / (1.0 - (2.0 * l - 1.0).abs())).clamp(0.0, 1.0);
    let h = if max == r {
        60.0 * (((g - b) / d) % 6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    (h.rem_euclid(360.0), s, l)
}

pub(crate) fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let sector = h.rem_euclid(360.0) / 60.0;
    let x = c * (1.0 - (sector.rem_euclid(2.0) - 1.0).abs());
    let (r1, g1, b1) = match sector as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let channel = |v: f32| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    (channel(r1), channel(g1), channel(b1))
}

const BINS: usize = 24;

/// A histogram of the hue over the chromatic pixels (lightness 0.10 to
/// 0.93, saturation at least 0.08), one vote each, in 24 bins.
fn hue_histogram(img: &image::RgbaImage) -> ([f32; BINS], u32) {
    let mut hist = [0.0f32; BINS];
    let mut chromatic = 0u32;
    for pixel in img.pixels() {
        let (h, s, l) = rgb_to_hsl(pixel[0], pixel[1], pixel[2]);
        if !(0.10..=0.93).contains(&l) || s < 0.08 {
            continue;
        }
        let bin = ((h / 360.0 * BINS as f32) as usize).min(BINS - 1);
        hist[bin] += 1.0;
        chromatic += 1;
    }
    (hist, chromatic)
}

/// A bin's score: its votes and half of its two neighbours'.
fn score(hist: &[f32; BINS], i: usize) -> f32 {
    hist[i] + 0.5 * (hist[(i + BINS - 1) % BINS] + hist[(i + 1) % BINS])
}

/// The bin that dominates, its score and how many pixels were chromatic.
fn dominant_hue_bin(img: &image::RgbaImage) -> (usize, f32, u32) {
    let (hist, chromatic) = hue_histogram(img);
    let (mut best_i, mut best) = (0usize, -1.0f32);
    for i in 0..BINS {
        let candidate = score(&hist, i);
        if candidate > best {
            best = candidate;
            best_i = i;
        }
    }
    (best_i, best, chromatic)
}

fn hue_of(bin: usize) -> f32 {
    (bin as f32 + 0.5) * (360.0 / BINS as f32)
}

/// Primary and secondary: the most abundant hue, vivid and mid-bright, and
/// a second genuine cluster at least three bins away with at least 35 % of
/// the peak's score; without one, the same hue deeper. Fewer than four
/// chromatic pixels keep the defaults.
fn spectrum_colors(img: &image::RgbaImage) -> ((u8, u8, u8), (u8, u8, u8)) {
    let default = ((0, 220, 200), (150, 50, 255));
    let (best_i, best, chromatic) = dominant_hue_bin(img);
    if chromatic < 4 {
        return default;
    }
    let (hist, _) = hue_histogram(img);
    let mut second: Option<usize> = None;
    let mut second_best = 0.0f32;
    for i in 0..BINS {
        let around = (i as i32 - best_i as i32).rem_euclid(BINS as i32);
        if around.min(BINS as i32 - around) < 3 {
            continue;
        }
        let candidate = score(&hist, i);
        if candidate > second_best {
            second_best = candidate;
            second = Some(i);
        }
    }
    let primary = hsl_to_rgb(hue_of(best_i), 0.85, 0.58);
    let secondary = match second.filter(|_| second_best >= best * 0.35) {
        Some(si) => hsl_to_rgb(hue_of(si), 0.88, 0.62),
        None => hsl_to_rgb(hue_of(best_i), 0.95, 0.40),
    };
    (primary, secondary)
}

/// The accent: the dominant hue complemented, vivid and mid-bright.
fn lyrics_accent_color(img: &image::RgbaImage) -> (u8, u8, u8) {
    let (best_i, _, chromatic) = dominant_hue_bin(img);
    if chromatic < 4 {
        return (0x3f, 0xd9, 0xc8);
    }
    hsl_to_rgb((hue_of(best_i) + 180.0).rem_euclid(360.0), 0.85, 0.62)
}

/// The most saturated sample of the eight by eight cover whose lightness
/// sits between 50 and 220 of 255, as QBZ picks the halo; blue when none
/// qualifies. Its own saturation, on the 0 to 255 scale: not `rgb_to_hsl`.
fn glow_color(tiny: &image::RgbaImage) -> (u8, u8, u8) {
    let mut best_saturation = 0.0f32;
    let mut best = (100u8, 100u8, 255u8);
    for pixel in tiny.pixels() {
        let (r, g, b) = (f32::from(pixel[0]), f32::from(pixel[1]), f32::from(pixel[2]));
        let (max, min) = (r.max(g).max(b), r.min(g).min(b));
        let lightness = (max + min) / 2.0;
        let saturation = if (max - min).abs() < f32::EPSILON {
            0.0
        } else if lightness > 127.0 {
            (max - min) / (510.0 - max - min).max(1.0)
        } else {
            (max - min) / (max + min).max(1.0)
        };
        if lightness > 50.0 && lightness < 220.0 && saturation > best_saturation {
            best_saturation = saturation;
            best = (pixel[0], pixel[1], pixel[2]);
        }
    }
    best
}

/// The halo's color with its alpha of 0x59 last, as the theme parses it.
fn glow_hex(c: (u8, u8, u8)) -> String {
    format!("#{:02x}{:02x}{:02x}59", c.0, c.1, c.2)
}

pub(crate) fn hex(c: (u8, u8, u8)) -> String {
    format!("#{:02x}{:02x}{:02x}", c.0, c.1, c.2)
}

/// What a setting's new value is kept as, or `None` for a key that is not
/// the background's or a value it cannot take.
fn accepted(key: &str, value: &str) -> Option<String> {
    match key {
        MODE => Some(if MODES.contains(&value) { value } else { "off" }.to_string()),
        IMAGE => Some(value.trim().to_string()),
        BLUR => value.trim().parse::<i32>().ok().map(|n| n.clamp(0, 100).to_string()),
        _ => None,
    }
}

impl Chilpa {
    /// The look kept, as an index into [`MODES`]; 0 when none is.
    pub fn background_mode(&self) -> usize {
        self.preference(MODE).and_then(|v| MODES.iter().position(|m| *m == v)).unwrap_or(0)
    }

    /// The user's own picture behind the wallpaper looks, or empty.
    pub fn background_image(&self) -> String {
        self.preference(IMAGE).unwrap_or_default()
    }

    /// How blurred the wallpaper look is, 0 to 100.
    pub fn wallpaper_blur(&self) -> u8 {
        self.preference(BLUR).and_then(|v| v.parse::<i32>().ok()).map_or(BLUR_DEFAULT, |v| v.clamp(0, 100) as u8)
    }

    /// Takes one of the background's settings; `false` for a key that is
    /// not its, or a value it cannot take.
    pub(crate) fn ambient_setting(&mut self, key: &str, value: &str) -> bool {
        let Some(kept) = accepted(key, value) else { return false };
        self.remember_preference(key, &kept);
        true
    }

    /// What each of the background's rows says: the looks to choose from
    /// and the one kept; the picture, only under the wallpaper looks; the
    /// blur, only under the plain wallpaper.
    pub(crate) fn ambient_row(&self, row: &mut SettingsRow) {
        let mode = self.background_mode();
        match row.key.as_str() {
            MODE => {
                for (value, label) in crate::settings_rows::fixed(&row.key) {
                    row.values.push(value.to_string());
                    row.options.push(label);
                }
                row.value = MODES[mode].to_string();
            }
            IMAGE => {
                row.value = self.background_image();
                if !paints_wallpaper(mode) {
                    row.control.clear();
                } else if !row.value.is_empty() {
                    row.description = row.value.clone();
                }
            }
            BLUR => {
                row.value = self.wallpaper_blur().to_string();
                if mode != 3 {
                    row.control.clear();
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picture(pixels: &[(u8, u8, u8)]) -> image::RgbaImage {
        let mut img = image::RgbaImage::new(8, 8);
        for (i, pixel) in img.pixels_mut().enumerate() {
            let (r, g, b) = pixels[i % pixels.len()];
            *pixel = image::Rgba([r, g, b, 255]);
        }
        img
    }

    #[test]
    fn the_halo_is_the_most_saturated_color_that_is_neither_dark_nor_pale() {
        let img = picture(&[(200, 30, 30), (120, 120, 120), (10, 10, 10), (250, 250, 250), (90, 90, 140)]);
        assert_eq!(glow_color(&img), (200, 30, 30));
        assert_eq!(glow_color(&picture(&[(0, 0, 0), (255, 255, 255)])), (100, 100, 255), "nothing qualifies: blue");
        assert_eq!(glow_hex((200, 30, 30)), "#c81e1e59");
        assert_eq!(Triad::default().glow, GLOW_DEFAULT);
    }

    fn img_from(pixels: &[(u8, u8, u8)]) -> image::RgbaImage {
        let mut img = image::RgbaImage::new(16, 16);
        for (i, pixel) in img.pixels_mut().enumerate() {
            let (r, g, b) = pixels[i % pixels.len()];
            *pixel = image::Rgba([r, g, b, 255]);
        }
        img
    }

    #[test]
    fn dominant_hue_wins_over_speck() {
        // Three quarters red, one quarter blue: the coverage-weighted
        // primary is the red, not the more saturated speck.
        let img = img_from(&[(200, 30, 30), (200, 30, 30), (200, 30, 30), (30, 30, 220)]);
        let (primary, _) = spectrum_colors(&img);
        let (h, _, _) = rgb_to_hsl(primary.0, primary.1, primary.2);
        assert!(h < 15.0 || h > 345.0, "primary hue {h} should be red");
    }

    #[test]
    fn gray_cover_falls_back_to_defaults() {
        let img = img_from(&[(128, 128, 128)]);
        let (primary, secondary) = spectrum_colors(&img);
        assert_eq!(primary, (0, 220, 200));
        assert_eq!(secondary, (150, 50, 255));
        assert_eq!(lyrics_accent_color(&img), (0x3f, 0xd9, 0xc8));
        assert_eq!(Triad::default(), Triad { primary: hex(primary), secondary: hex(secondary), accent: hex((0x3f, 0xd9, 0xc8)), glow: GLOW_DEFAULT.into() });
    }

    #[test]
    fn accent_complements_the_dominant_hue() {
        let img = img_from(&[(200, 30, 30), (200, 30, 30), (200, 30, 30), (210, 40, 40)]);
        let accent = lyrics_accent_color(&img);
        let (h, _, _) = rgb_to_hsl(accent.0, accent.1, accent.2);
        assert!((170.0..=190.0).contains(&h), "accent hue {h} should be cyan");
    }

    #[test]
    fn secondary_stays_on_album_for_mono_covers() {
        // A single-hue cover: the secondary is the same hue, deeper, never
        // a fabricated rotation.
        let img = img_from(&[(220, 40, 120), (200, 30, 110)]);
        let (primary, secondary) = spectrum_colors(&img);
        let (primary_hue, _, _) = rgb_to_hsl(primary.0, primary.1, primary.2);
        let (secondary_hue, _, secondary_lightness) = rgb_to_hsl(secondary.0, secondary.1, secondary.2);
        let distance = (primary_hue - secondary_hue).abs().min(360.0 - (primary_hue - secondary_hue).abs());
        assert!(distance < 30.0, "mono cover: secondary hue {secondary_hue} against primary {primary_hue}");
        assert!(secondary_lightness < 0.45, "mono cover: the secondary is deeper (L = {secondary_lightness})");
    }

    #[test]
    fn the_three_keys_are_taken_and_their_values_kept_in_range() {
        assert_eq!(accepted(MODE, "wallpaper").as_deref(), Some("wallpaper"));
        assert_eq!(accepted(MODE, "nonsense").as_deref(), Some("off"), "an unknown look is off");
        assert_eq!(accepted(BLUR, "250").as_deref(), Some("100"));
        assert_eq!(accepted(BLUR, "-3").as_deref(), Some("0"));
        assert_eq!(accepted(BLUR, " 42 ").as_deref(), Some("42"));
        assert_eq!(accepted(BLUR, "some"), None);
        assert_eq!(accepted(IMAGE, " /pictures/a.png ").as_deref(), Some("/pictures/a.png"));
        assert_eq!(accepted("appearance.theme", "dark"), None, "another feature's key");
        assert!(paints_wallpaper(3) && paints_wallpaper(4) && !paints_wallpaper(2));
    }
}
