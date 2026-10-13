//! Covers at the size they are drawn (frontend ADR-0014, rule 5).
//!
//! A cover as it comes is anything from a 300 px JPEG to a 5,670 px one of
//! 13 MB, and a grid cell draws it at a few hundred pixels. Decoding the
//! whole thing for every cell that scrolls into view was what made a real
//! library stutter -- 20 frames late in three seconds of scrolling Albums,
//! against one with the covers turned off. So each cover is decoded once
//! per size, made that size and kept on disk as a small JPEG, and that is
//! what the window loads.
//!
//! It is called off the window's thread, as many at once as the caller
//! allows: a big cover takes a fifth of a second to decode and a hundred MB
//! while it does.

use std::path::{Path, PathBuf};

/// JPEG quality of a derivative: indistinguishable at the size it is drawn.
const QUALITY: u8 = 88;

/// The cover at `path`, `size` pixels on its longer side, from the cache;
/// made the first time it is asked for. `None` if it cannot be read.
pub fn cover_derivative(path: &Path, size: u32) -> Option<PathBuf> {
    derivative_in(&chilpa_dirs::cache().join("covers"), path, size)
}

/// A small, deliberately color-heavy field for an album header. The 8 px
/// intermediate is the look: no recognizable cover detail survives it.
pub fn header_atmosphere(path: &Path) -> Option<PathBuf> {
    atmosphere_in(&chilpa_dirs::cache().join("atmosphere"), path)
}

/// A complete cover on a square field blended from its two dominant colors.
pub fn contained_cover(path: &Path, size: u32) -> Option<PathBuf> {
    let size = size.clamp(32, 2048);
    let small = cover_derivative(path, size)?;
    let directory = chilpa_dirs::cache().join("contained-covers");
    let output = directory.join(small.file_name()?);
    if output.is_file() { return Some(output); }
    let art = image::open(&small).ok()?.to_rgb8();
    let field = contain_image(&art, size);
    std::fs::create_dir_all(directory).ok()?;
    let mut file = tempfile::NamedTempFile::new_in(output.parent()?).ok()?;
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut file, QUALITY).encode_image(&field).ok()?;
    let bytes = file.as_file().metadata().ok()?.len();
    file.persist(&output).ok()?;
    crate::storage_cache::note_stored(bytes);
    Some(output)
}

fn contain_image(art: &image::RgbImage, size: u32) -> image::RgbImage {
    let sample = image::imageops::thumbnail(art, 64, 64);
    let mut bins = std::collections::BTreeMap::<[u8;3], (u32,[u32;3])>::new();
    for pixel in sample.pixels() {
        let bin = bins.entry([pixel[0]/32,pixel[1]/32,pixel[2]/32]).or_default();
        bin.0 += 1;
        for channel in 0..3 { bin.1[channel] += u32::from(pixel[channel]); }
    }
    let mut colors: Vec<_> = bins.into_values().collect();
    colors.sort_by_key(|(count,_)| std::cmp::Reverse(*count));
    let average = |index: usize| {
        let (count,sum) = colors.get(index).or_else(|| colors.first()).copied().unwrap_or((1,[0;3]));
        sum.map(|value| (value / count) as u8)
    };
    let (top,bottom) = (average(0),average(1));
    let mut field = image::RgbImage::from_fn(size,size,|_,y| image::Rgb(std::array::from_fn(|c| {
        ((u32::from(top[c]) * (size-1-y) + u32::from(bottom[c]) * y) / (size-1)) as u8
    })));
    let ratio = size as f64 / f64::from(art.width().max(art.height()).max(1));
    let fitted = image::imageops::resize(art, (f64::from(art.width())*ratio).round().max(1.0) as u32, (f64::from(art.height())*ratio).round().max(1.0) as u32, image::imageops::FilterType::Lanczos3);
    image::imageops::overlay(&mut field, &fitted, i64::from((size-fitted.width())/2), i64::from((size-fitted.height())/2));
    field
}

fn derivative_in(dir: &Path, path: &Path, size: u32) -> Option<PathBuf> {
    let size = size.clamp(32, 2048);
    // The file's own date is in the name, so a cover replaced on disk gets
    // a new derivative instead of the old one forever.
    let modified = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    let out = dir.join(format!("{:016x}-{size}.jpg", key(path, modified)));
    if out.exists() {
        return Some(out);
    }
    let cover = image::ImageReader::open(path).ok()?.with_guessed_format().ok()?.decode().ok()?;
    // A cover smaller than asked is kept at its size: making it bigger
    // only makes it blurrier and heavier.
    let small = if cover.width().max(cover.height()) > size { cover.thumbnail(size, size) } else { cover };
    let pixels = small.into_rgb8();
    std::fs::create_dir_all(dir).ok()?;
    // Written beside and moved over: a half-written file is never loaded.
    let next = out.with_extension("jpg.new");
    let file = std::fs::File::create(&next).ok()?;
    let mut writer = std::io::BufWriter::new(file);
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut writer, QUALITY).encode_image(&pixels).ok()?;
    drop(writer);
    std::fs::rename(&next, &out).ok()?;
    if let Ok(meta) = std::fs::metadata(&out) { crate::storage_cache::note_stored(meta.len()); }
    Some(out)
}

fn atmosphere_in(dir: &Path, path: &Path) -> Option<PathBuf> {
    let modified = std::fs::metadata(path).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
    let out = dir.join(format!("{:016x}.png", key(path, modified)));
    if out.exists() { return Some(out) }
    let source = image::ImageReader::open(path).ok()?.with_guessed_format().ok()?.decode().ok()?.into_rgba8();
    let tiny = image::imageops::resize(&source, 8, 8, image::imageops::FilterType::Triangle);
    let large = image::imageops::resize(&tiny, 128, 128, image::imageops::FilterType::CatmullRom);
    let mut field = image::imageops::blur(&large, 16.0);
    color_field(&mut field);
    vignette(&mut field);
    std::fs::create_dir_all(dir).ok()?;
    let next = out.with_extension("png.new");
    field.save_with_format(&next, image::ImageFormat::Png).ok()?;
    std::fs::rename(next, &out).ok()?;
    if let Ok(meta) = std::fs::metadata(&out) { crate::storage_cache::note_stored(meta.len()); }
    Some(out)
}

fn color_field(image: &mut image::RgbaImage) {
    let (mut low, mut high, mut light) = ([255.0f32; 3], [0.0f32; 3], 0.0f32);
    for pixel in image.pixels_mut() {
        let gray = (f32::from(pixel[0]) + f32::from(pixel[1]) + f32::from(pixel[2])) / 3.0;
        for channel in 0..3 {
            let saturated = gray + (f32::from(pixel[channel]) - gray) * 2.45;
            pixel[channel] = ((saturated * 0.92 - 128.0) * 1.18 + 128.0).clamp(0.0, 255.0) as u8;
            low[channel] = low[channel].min(f32::from(pixel[channel]));
            high[channel] = high[channel].max(f32::from(pixel[channel]));
            light += f32::from(pixel[channel]) / 3.0;
        }
    }
    let strength = (light / (image.width() * image.height()).max(1) as f32 / 80.0).min(1.0);
    for pixel in image.pixels_mut() {
        for channel in 0..3 {
            let value = f32::from(pixel[channel]);
            let normalized = 18.0 + (value - low[channel]) / (high[channel] - low[channel]).max(1.0) * 232.0;
            let lifted = (value * 1.5).min(255.0);
            let adjusted = lifted + (normalized - lifted) * strength;
            pixel[channel] = (if channel == 0 { adjusted * 1.08 } else { adjusted }).min(255.0) as u8;
        }
    }
}

fn vignette(image: &mut image::RgbaImage) {
    let (width, height) = (image.width() as f32, image.height() as f32);
    let inner = width.min(height) * 0.20;
    let outer = width.min(height) * 0.70;
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        let distance = ((x as f32 - width / 2.0).powi(2) + (y as f32 - height / 2.0).powi(2)).sqrt();
        let factor = 1.0 - 0.20 * ((distance - inner) / (outer - inner)).clamp(0.0, 1.0);
        for channel in 0..3 { pixel[channel] = (f32::from(pixel[channel]) * factor) as u8; }
    }
}

fn key(path: &Path, modified: u64) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut h);
    modified.hash(&mut h);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cover(dir: &Path, name: &str, side: u32) -> PathBuf {
        let path = dir.join(name);
        image::RgbImage::from_fn(side, side, |x, y| image::Rgb([(x % 256) as u8, (y % 256) as u8, 90]))
            .save(&path)
            .unwrap();
        path
    }

    #[test]
    fn a_big_cover_is_kept_at_the_size_it_is_drawn_and_made_once() {
        let dir = std::env::temp_dir().join("chilpa-covers-big");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let original = cover(&dir, "cover.png", 1500);
        let cache = dir.join("cache");
        let small = derivative_in(&cache, &original, 256).unwrap();
        let made = image::open(&small).unwrap();
        assert_eq!((made.width(), made.height()), (256, 256));
        let first = std::fs::metadata(&small).unwrap().modified().unwrap();
        // Asked again, the same file comes back without being made again.
        assert_eq!(derivative_in(&cache, &original, 256).unwrap(), small);
        assert_eq!(std::fs::metadata(&small).unwrap().modified().unwrap(), first);
        // Another size is another file.
        assert_ne!(derivative_in(&cache, &original, 512).unwrap(), small);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_cover_smaller_than_asked_is_not_blown_up() {
        let dir = std::env::temp_dir().join("chilpa-covers-small");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let original = cover(&dir, "cover.jpg", 200);
        let made = image::open(derivative_in(&dir.join("cache"), &original, 512).unwrap()).unwrap();
        assert_eq!(made.width(), 200);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn what_is_not_an_image_gives_nothing() {
        let dir = std::env::temp_dir().join("chilpa-covers-junk");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("cover.jpg"), b"not a picture").unwrap();
        assert!(derivative_in(&dir.join("cache"), &dir.join("cover.jpg"), 256).is_none());
        assert!(derivative_in(&dir.join("cache"), &dir.join("missing.jpg"), 256).is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn an_atmosphere_is_small_cached_and_has_no_cover_detail() {
        let dir = std::env::temp_dir().join("chilpa-atmosphere");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let original = cover(&dir, "cover.png", 300);
        let made = atmosphere_in(&dir.join("cache"), &original).unwrap();
        let image = image::open(&made).unwrap();
        assert_eq!((image.width(), image.height()), (128, 128));
        assert_eq!(atmosphere_in(&dir.join("cache"), &original).unwrap(), made);
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[cfg(test)]
mod contained_tests {
    #[test]
    fn a_wide_picture_preserves_both_edges_and_fills_the_gutter() {
        let art = image::RgbImage::from_fn(80,40,|x,_| if x<40 {image::Rgb([200,20,20])} else {image::Rgb([20,20,200])});
        let result = super::contain_image(&art,80);
        assert_eq!(result.dimensions(),(80,80));
        assert_eq!(result.get_pixel(0,40),art.get_pixel(0,20));
        assert_eq!(result.get_pixel(79,40),art.get_pixel(79,20));
        assert_ne!(result.get_pixel(0,0),result.get_pixel(0,79));
    }
}
