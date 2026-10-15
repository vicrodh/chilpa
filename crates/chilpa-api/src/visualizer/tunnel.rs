//! Copyright (c) 2024 blitzkriegfc. MIT; see licenses/immersive-renderers-MIT.txt.
//! The four artwork colors used by the feedback tunnel.

use std::collections::HashMap;

/// The Tauri fallback (`DEFAULT_LINE_PALETTE`, TunnelFlowPanel.svelte:63-68):
/// #ff6a6a / #ffcd5c / #68dcaa / #6eb0ff.
pub const DEFAULT_LINE_PALETTE: [(u8, u8, u8); 4] = [
    (255, 106, 106),
    (255, 205, 92),
    (104, 220, 170),
    (110, 176, 255),
];

/// The Tauri sample size (`:123`).
pub const SAMPLE_SIZE: u32 = 36;

fn color_saturation(r: u8, g: u8, b: u8) -> f32 {
    let max = r.max(g).max(b) as f32;
    let min = r.min(g).min(b) as f32;
    if max <= 0.0 {
        return 0.0;
    }
    (max - min) / max
}

fn color_distance(a: (u8, u8, u8), b: (u8, u8, u8)) -> f32 {
    let red_delta = a.0 as f32 - b.0 as f32;
    let green_delta = a.1 as f32 - b.1 as f32;
    let blue_delta = a.2 as f32 - b.2 as f32;
    (red_delta * red_delta + green_delta * green_delta + blue_delta * blue_delta).sqrt()
}

struct Bucket {
    count: u32,
    red: u64,
    green: u64,
    blue: u64,
    sat_sum: f32,
}

/// The port of `extractLinePaletteFromArtwork` operating on the 36x36 sample.
/// Always returns exactly 4 colors.
pub fn line_palette(img: &image::RgbaImage) -> Vec<(u8, u8, u8)> {
    let mut buckets: Vec<Bucket> = Vec::new();
    let mut index: HashMap<(u8, u8, u8), usize> = HashMap::new();

    for pixel in img.pixels() {
        let [r, g, b, a] = pixel.0;
        if a < 120 {
            continue;
        }
        let luminance = (r as f32 * 0.2126 + g as f32 * 0.7152 + b as f32 * 0.0722) / 255.0;
        if !(0.05..=0.97).contains(&luminance) {
            continue;
        }
        let sat = color_saturation(r, g, b);
        let key = (r / 32 * 32, g / 32 * 32, b / 32 * 32);
        match index.get(&key) {
            Some(&i) => {
                let bk = &mut buckets[i];
                bk.count += 1;
                bk.red += r as u64;
                bk.green += g as u64;
                bk.blue += b as u64;
                bk.sat_sum += sat;
            }
            None => {
                index.insert(key, buckets.len());
                buckets.push(Bucket {
                    count: 1,
                    red: r as u64,
                    green: g as u64,
                    blue: b as u64,
                    sat_sum: sat,
                });
            }
        }
    }

    if buckets.is_empty() {
        return DEFAULT_LINE_PALETTE.to_vec();
    }

    // Rank by count * (0.72 + avgSat * 1.28) descending; stable on ties.
    let mut order: Vec<usize> = (0..buckets.len()).collect();
    order.sort_by(|&a, &b| {
        let score = |bk: &Bucket| {
            let avg_sat = bk.sat_sum / bk.count as f32;
            bk.count as f32 * (0.72 + avg_sat * 1.28)
        };
        score(&buckets[b])
            .partial_cmp(&score(&buckets[a]))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut palette: Vec<(u8, u8, u8)> = Vec::new();
    for &i in &order {
        let bk = &buckets[i];
        let candidate = (
            (bk.red as f32 / bk.count as f32).round() as u8,
            (bk.green as f32 / bk.count as f32).round() as u8,
            (bk.blue as f32 / bk.count as f32).round() as u8,
        );
        let candidate_sat = color_saturation(candidate.0, candidate.1, candidate.2);
        if candidate_sat < 0.12 && !palette.is_empty() {
            continue;
        }
        if palette
            .iter()
            .any(|&existing| color_distance(existing, candidate) < 44.0)
        {
            continue;
        }
        palette.push(candidate);
        if palette.len() >= 4 {
            break;
        }
    }

    if palette.is_empty() {
        return DEFAULT_LINE_PALETTE.to_vec();
    }
    while palette.len() < 4 {
        palette.push(palette[palette.len() - 1]);
    }
    palette
}

/// The batched publish form: `["#ff6a6a","#ffcd5c","#68dcaa","#6eb0ff"]` —
/// one JSON document, one notify (the pack_json batching pattern).
pub fn line_palette_json(palette: &[(u8, u8, u8)]) -> String {
    let parts: Vec<String> = palette
        .iter()
        .map(|c| format!("\"#{:02x}{:02x}{:02x}\"", c.0, c.1, c.2))
        .collect();
    format!("[{}]", parts.join(","))
}

/// Samples the cover for the tunnel independently of the ambient triad.
pub fn palette_of(path: &std::path::Path) -> Option<String> {
    let image = image::ImageReader::open(path).ok()?.with_guessed_format().ok()?.decode().ok()?.into_rgba8();
    let sample = image::imageops::resize(&image, SAMPLE_SIZE, SAMPLE_SIZE, image::imageops::FilterType::Triangle);
    Some(line_palette_json(&line_palette(&sample)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 36x36 image tiled from the given pixels (the ambient_qt.rs test
    /// idiom, at the Tauri sample size).
    fn img_from(pixels: &[(u8, u8, u8)]) -> image::RgbaImage {
        let mut img = image::RgbaImage::new(SAMPLE_SIZE, SAMPLE_SIZE);
        for (i, pixel) in img.pixels_mut().enumerate() {
            let (r, g, b) = pixels[i % pixels.len()];
            *pixel = image::Rgba([r, g, b, 255]);
        }
        img
    }

    #[test]
    fn coverage_beats_saturation_speck() {
        // 3/4 muted red + 1/4 vivid blue: score = count*(0.72+avgSat*1.28) —
        // the red bucket (count ~972, sat ~0.5) must outrank the blue speck
        // (count ~324, sat ~0.97).
        let img = img_from(&[(140, 70, 70), (140, 70, 70), (140, 70, 70), (30, 40, 235)]);
        let palette = line_palette(&img);
        assert_eq!(palette[0], (140, 70, 70));
        assert_eq!(palette[1], (30, 40, 235));
    }

    #[test]
    fn dedup_drops_colors_closer_than_44() {
        // Two reds 30 apart (distance 30 < 44) collapse to one pick; the
        // second pick must be the distant green.
        let img = img_from(&[
            (200, 40, 40),
            (200, 40, 40),
            (170, 40, 40), // same 32-bucket family, distance 30 from the first
            (40, 200, 40),
        ]);
        let palette = line_palette(&img);
        assert_eq!(palette.len(), 4);
        let red = palette[0];
        assert!((200 - red.0 as i32).abs() <= 30 && red.1 == 40);
        // No second pick within distance 44 of the first.
        assert!(color_distance(palette[1], red) >= 44.0);
    }

    #[test]
    fn low_saturation_candidates_are_skipped_after_the_first_pick() {
        // Dominant vivid red + several grays: the grays (sat 0 < 0.12) may
        // never follow the first pick, so the palette pads by repeating.
        let img = img_from(&[(220, 30, 30), (220, 30, 30), (220, 30, 30), (120, 120, 120)]);
        let palette = line_palette(&img);
        assert_eq!(palette[0], (220, 30, 30));
        assert!(palette.iter().all(|&c| c == palette[0]));
    }

    #[test]
    fn first_pick_may_be_low_saturation() {
        // An all-gray cover: no chroma at all. The first bucket is kept
        // (the sat<0.12 skip only applies once a pick exists) and then pads.
        // NOTE the ambient triad falls back on gray covers; the Tauri line
        // palette deliberately does NOT.
        let img = img_from(&[(120, 120, 120)]);
        let palette = line_palette(&img);
        assert_eq!(palette[0], (120, 120, 120));
        assert_eq!(palette.len(), 4);
    }

    #[test]
    fn alpha_and_luminance_gates_feed_the_fallback() {
        // Every pixel skipped: alpha < 120, luminance < 0.05, luminance >
        // 0.97 -> the DEFAULT palette, not an empty/gray one.
        let mut img = image::RgbaImage::new(SAMPLE_SIZE, SAMPLE_SIZE);
        for (i, pixel) in img.pixels_mut().enumerate() {
            *pixel = match i % 3 {
                0 => image::Rgba([200, 30, 30, 60]),    // alpha gate
                1 => image::Rgba([8, 8, 10, 255]),      // luminance < 0.05
                _ => image::Rgba([252, 252, 250, 255]), // luminance > 0.97
            };
        }
        assert_eq!(line_palette(&img), DEFAULT_LINE_PALETTE.to_vec());
    }

    #[test]
    fn quantize_and_mean_match_the_reference() {
        // 32-step buckets: (35,…) and (63,…) share bucket 32; the bucket
        // color is the ROUNDED mean of its members. (A same-family neighbor
        // bucket would sit inside the 44 dedup radius, so the second pick
        // here is a distant red.)
        let img = img_from(&[(35, 200, 40), (63, 200, 40), (250, 40, 40)]);
        let palette = line_palette(&img);
        // Bucket (32,192,32): mean r = (35+63)/2 = 49 -> (49,200,40).
        assert_eq!(palette[0], (49, 200, 40));
        assert!(palette.contains(&(250, 40, 40)));
    }

    #[test]
    fn json_is_the_batched_four_hex_array() {
        let json = line_palette_json(&DEFAULT_LINE_PALETTE);
        assert_eq!(json, "[\"#ff6a6a\",\"#ffcd5c\",\"#68dcaa\",\"#6eb0ff\"]");
        let doc: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(doc.as_array().unwrap().len(), 4);
    }
}
