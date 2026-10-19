//! K-means palette extraction from images, ported 1:1 from QBZ 2.1.2
//! (`qbz-theme/src/auto/palette.rs`): downsample, k-means, role assignment.

use image::GenericImageView;
use std::path::Path;

use super::{PaletteColor, ThemePalette};

/// Load an image, downsample, and extract dominant colors via k-means.
pub fn extract_palette(image_path: &str) -> Result<ThemePalette, String> {
    let path = Path::new(image_path);
    if !path.exists() {
        return Err(format!("Image not found: {}", image_path));
    }

    // Bounded decode: a decompression-bomb PNG (65k x 65k) would otherwise
    // expand to tens of GB before the 100x100 downsample. 12k x 12k / 512 MB
    // covers any real wallpaper while keeping the worst case survivable.
    let mut reader = image::ImageReader::open(path)
        .map_err(|e| format!("Failed to open image: {}", e))?
        .with_guessed_format()
        .map_err(|e| format!("Failed to read image: {}", e))?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(12_000);
    limits.max_image_height = Some(12_000);
    limits.max_alloc = Some(512 * 1024 * 1024);
    reader.limits(limits);
    let img = reader.decode().map_err(|e| format!("Failed to decode image: {}", e))?;

    // Downsample to 100x100 for fast processing.
    let thumb = img.resize_exact(100, 100, image::imageops::FilterType::Lanczos3);

    let mut pixels: Vec<[f64; 3]> = Vec::with_capacity(10_000);
    for (_x, _y, rgba) in thumb.pixels() {
        // Skip semi-transparent pixels.
        if rgba[3] < 200 {
            continue;
        }
        pixels.push([rgba[0] as f64, rgba[1] as f64, rgba[2] as f64]);
    }

    if pixels.is_empty() {
        return Err("Image contains no opaque pixels".to_string());
    }

    let clusters = kmeans(&pixels, 6, 25);
    if clusters.is_empty() {
        return Err("K-means produced no clusters".to_string());
    }

    build_palette(clusters)
}

/// Extract a palette directly from raw RGB pixel data (testing / custom sources).
pub fn extract_palette_from_pixels(pixels: &[[f64; 3]]) -> Result<ThemePalette, String> {
    if pixels.is_empty() {
        return Err("No pixels provided".to_string());
    }
    let clusters = kmeans(pixels, 6, 25);
    if clusters.is_empty() {
        return Err("K-means produced no clusters".to_string());
    }
    build_palette(clusters)
}

/// A cluster result: centroid + pixel count.
#[derive(Debug, Clone)]
struct Cluster {
    centroid: [f64; 3],
    count: usize,
}

/// Simple k-means clustering on RGB values.
fn kmeans(pixels: &[[f64; 3]], k: usize, max_iters: usize) -> Vec<Cluster> {
    let n = pixels.len();
    if n == 0 || k == 0 {
        return Vec::new();
    }
    let k = k.min(n);

    // Initialize centroids by evenly sampling from the pixel list.
    let mut centroids: Vec<[f64; 3]> = Vec::with_capacity(k);
    let step = n / k;
    for i in 0..k {
        centroids.push(pixels[i * step]);
    }

    let mut assignments = vec![0usize; n];

    for _ in 0..max_iters {
        let mut changed = false;

        // Assignment step.
        for (idx, pixel) in pixels.iter().enumerate() {
            let mut best_cluster = 0;
            let mut best_dist = f64::MAX;
            for (ci, centroid) in centroids.iter().enumerate() {
                let dist = rgb_dist_sq(pixel, centroid);
                if dist < best_dist {
                    best_dist = dist;
                    best_cluster = ci;
                }
            }
            if assignments[idx] != best_cluster {
                assignments[idx] = best_cluster;
                changed = true;
            }
        }

        if !changed {
            break;
        }

        // Update step.
        let mut sums = vec![[0.0f64; 3]; k];
        let mut counts = vec![0usize; k];
        for (idx, pixel) in pixels.iter().enumerate() {
            let ci = assignments[idx];
            sums[ci][0] += pixel[0];
            sums[ci][1] += pixel[1];
            sums[ci][2] += pixel[2];
            counts[ci] += 1;
        }

        for ci in 0..k {
            if counts[ci] > 0 {
                centroids[ci][0] = sums[ci][0] / counts[ci] as f64;
                centroids[ci][1] = sums[ci][1] / counts[ci] as f64;
                centroids[ci][2] = sums[ci][2] / counts[ci] as f64;
            }
        }
    }

    let mut counts = vec![0usize; k];
    for &a in &assignments {
        counts[a] += 1;
    }

    centroids
        .into_iter()
        .zip(counts)
        .filter(|(_, count)| *count > 0)
        .map(|(centroid, count)| Cluster { centroid, count })
        .collect()
}

fn rgb_dist_sq(a: &[f64; 3], b: &[f64; 3]) -> f64 {
    let red = a[0] - b[0];
    let green = a[1] - b[1];
    let blue = a[2] - b[2];
    red * red + green * green + blue * blue
}

/// Build a [`ThemePalette`] from k-means clusters.
fn build_palette(mut clusters: Vec<Cluster>) -> Result<ThemePalette, String> {
    // Sort by pixel count (dominant first).
    clusters.sort_by(|a, b| b.count.cmp(&a.count));

    let all_colors: Vec<PaletteColor> = clusters
        .iter()
        .map(|c| {
            PaletteColor::new(
                c.centroid[0].round() as u8,
                c.centroid[1].round() as u8,
                c.centroid[2].round() as u8,
            )
        })
        .collect();

    let dominant = all_colors[0];
    let is_dark = dominant.luminance() < 0.5;

    // Monochromatic check (all clusters very similar to the dominant).
    let max_distance = all_colors
        .iter()
        .skip(1)
        .map(|c| dominant.distance(c))
        .fold(0.0f64, f64::max);
    let is_monochrome = max_distance < 40.0;

    // Background roles from the dominant color with lightness shifts.
    let (bg_primary, bg_secondary, bg_tertiary, bg_hover) = if is_dark {
        (
            dominant.shift_lightness(-0.05),
            dominant.shift_lightness(0.03),
            dominant.shift_lightness(0.08),
            dominant.shift_lightness(0.02),
        )
    } else {
        (
            dominant.shift_lightness(0.05),
            dominant.shift_lightness(-0.03),
            dominant.shift_lightness(-0.08),
            dominant.shift_lightness(-0.02),
        )
    };

    // Accent: most saturated cluster with AA contrast, monochrome → standard blue.
    let accent = if is_monochrome {
        if is_dark {
            PaletteColor::new(66, 133, 244)
        } else {
            PaletteColor::new(26, 115, 232)
        }
    } else {
        find_best_accent(&all_colors, &bg_primary, is_dark)
    };

    Ok(ThemePalette {
        bg_primary,
        bg_secondary,
        bg_tertiary,
        bg_hover,
        accent,
        is_dark,
        all_colors,
    })
}

/// Find the best accent: prioritize saturation, then ensure WCAG AA contrast.
fn find_best_accent(colors: &[PaletteColor], bg: &PaletteColor, is_dark: bool) -> PaletteColor {
    let mut candidates: Vec<(usize, f64)> = colors
        .iter()
        .enumerate()
        .skip(1)
        .map(|(i, c)| (i, c.saturation()))
        .collect();
    candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    for (idx, _sat) in &candidates {
        let color = colors[*idx];
        if color.contrast_ratio(bg) >= 4.5 {
            return color;
        }
        let adjusted = adjust_for_contrast(&color, bg, is_dark);
        if adjusted.contrast_ratio(bg) >= 4.5 {
            return adjusted;
        }
    }

    if let Some((idx, _)) = candidates.first() {
        let color = colors[*idx];
        adjust_for_contrast(&color, bg, is_dark)
    } else if is_dark {
        PaletteColor::new(66, 133, 244)
    } else {
        PaletteColor::new(26, 115, 232)
    }
}

/// Adjust a color's lightness to reach WCAG AA contrast against a background.
fn adjust_for_contrast(color: &PaletteColor, bg: &PaletteColor, is_dark: bool) -> PaletteColor {
    let (h, s, l) = color.to_hsl();
    let direction = if is_dark { 0.05 } else { -0.05 };

    let mut new_l = l;
    for _ in 0..20 {
        new_l = (new_l + direction).clamp(0.0, 1.0);
        let candidate = PaletteColor::from_hsl(h, s, new_l);
        if candidate.contrast_ratio(bg) >= 4.5 {
            return candidate;
        }
    }

    PaletteColor::from_hsl(h, s, new_l)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kmeans_basic_two_clusters() {
        let pixels: Vec<[f64; 3]> = vec![
            [0.0, 0.0, 0.0],
            [10.0, 10.0, 10.0],
            [5.0, 5.0, 5.0],
            [250.0, 250.0, 250.0],
            [240.0, 240.0, 240.0],
            [245.0, 245.0, 245.0],
        ];
        let clusters = kmeans(&pixels, 2, 20);
        assert_eq!(clusters.len(), 2);
    }

    #[test]
    fn extract_from_pixels_dark_dominant() {
        let mut pixels = Vec::new();
        for _ in 0..500 {
            pixels.push([20.0, 30.0, 80.0]);
        }
        for _ in 0..300 {
            pixels.push([240.0, 160.0, 40.0]);
        }
        for _ in 0..200 {
            pixels.push([10.0, 10.0, 15.0]);
        }
        let palette = extract_palette_from_pixels(&pixels).unwrap();
        assert!(palette.is_dark);
        assert!(palette.accent.saturation() > 0.1);
    }

    #[test]
    fn monochrome_fallback_accent() {
        let pixels: Vec<[f64; 3]> = (0..1000).map(|_| [50.0, 52.0, 48.0]).collect();
        let palette = extract_palette_from_pixels(&pixels).unwrap();
        assert!(palette.accent.saturation() > 0.1 || palette.accent.luminance() > 0.3);
    }
}
