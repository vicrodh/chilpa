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
use std::sync::{Condvar, Mutex};
use image::ImageDecoder;

/// JPEG quality of a derivative: indistinguishable at the size it is drawn.
const QUALITY: u8 = 88;
// I limit encoded originals separately: the JPEG decoder buffers its input.
const ENCODED_BYTES: u64 = 128 * 1024 * 1024;
const DECODE_BYTES: u64 = 128 * 1024 * 1024;
const DECODE_JOBS: usize = 2;
static DECODE_BUDGET: DecodeBudget = DecodeBudget::new();

struct DecodeBudget { used: Mutex<(usize, u64)>, wake: Condvar }

impl DecodeBudget {
    const fn new() -> Self { Self { used: Mutex::new((0, 0)), wake: Condvar::new() } }

    fn acquire(&self) -> DecodePermit<'_> {
        let mut used = self.used.lock().unwrap_or_else(|error| error.into_inner());
        while used.0 >= DECODE_JOBS {
            used = self.wake.wait(used).unwrap_or_else(|error| error.into_inner());
        }
        used.0 += 1;
        DecodePermit { budget: self, bytes: 0 }
    }
}

struct DecodePermit<'a> { budget: &'a DecodeBudget, bytes: u64 }

impl DecodePermit<'_> {
    fn reserve(&mut self, bytes: u64) -> Option<()> {
        if bytes > DECODE_BYTES { return None; }
        let mut used = self.budget.used.lock().unwrap_or_else(|error| error.into_inner());
        while bytes > DECODE_BYTES - used.1 {
            used = self.budget.wake.wait(used).unwrap_or_else(|error| error.into_inner());
        }
        used.1 += bytes;
        self.bytes = bytes;
        Some(())
    }
}

impl Drop for DecodePermit<'_> {
    fn drop(&mut self) {
        let mut used = self.budget.used.lock().unwrap_or_else(|error| error.into_inner());
        used.0 -= 1;
        used.1 -= self.bytes;
        self.budget.wake.notify_all();
    }
}

/// I keep the raster permit through validation or resizing. Decoder scratch
/// buffers and the small resized output sit outside this raw-pixel budget.
pub(crate) fn with_decoded<R: std::io::BufRead + std::io::Seek, T>(
    mut reader: image::ImageReader<R>, mut limits: image::Limits,
    use_picture: impl FnOnce(image::DynamicImage) -> T,
) -> Option<T> {
    let mut permit = DECODE_BUDGET.acquire();
    reader.limits(limits.clone());
    let mut decoder = reader.into_decoder().ok()?;
    let bytes = decoder.total_bytes();
    limits.reserve(bytes).ok()?;
    permit.reserve(bytes)?;
    decoder.set_limits(limits).ok()?;
    let picture = image::DynamicImage::from_decoder(decoder).ok()?;
    Some(use_picture(picture))
}

fn with_cover<T>(path: &Path, use_picture: impl FnOnce(image::DynamicImage) -> T) -> Option<T> {
    let file = std::fs::File::open(path).ok()?;
    if file.metadata().ok()?.len() > ENCODED_BYTES { return None; }
    let reader = image::ImageReader::new(std::io::BufReader::new(file)).with_guessed_format().ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(DECODE_BYTES);
    with_decoded(reader, limits, use_picture)
}

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
    let field = with_cover(&small, |art| contain_image(&art.into_rgb8(), size))?;
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
    // A cover smaller than asked is kept at its size: making it bigger
    // only makes it blurrier and heavier.
    let pixels = with_cover(path, |cover| {
        let small = if cover.width().max(cover.height()) > size { cover.thumbnail(size, size) } else { cover };
        small.into_rgb8()
    })?;
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
    let tiny = with_cover(path, |source| source.resize_exact(8, 8, image::imageops::FilterType::Triangle).into_rgba8())?;
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
    fn a_large_legitimate_cover_keeps_its_color_at_display_size() {
        let dir = tempfile::tempdir().unwrap();
        let original = dir.path().join("large.png");
        image::RgbaImage::from_pixel(5670, 5670, image::Rgba([180, 60, 90, 255]))
            .save(&original).unwrap();
        let result = derivative_in(&dir.path().join("cache"), &original, 64).unwrap();
        let small = image::open(result).unwrap().into_rgb8();
        assert_eq!(small.dimensions(), (64, 64));
        for (actual, expected) in small.get_pixel(32, 32).0.into_iter().zip([180u8, 60, 90]) {
            assert!(actual.abs_diff(expected) <= 3, "a derivative keeps the cover's color");
        }
    }

    #[test]
    fn excessive_dimensions_and_raster_bytes_do_not_make_derivatives() {
        let dir = tempfile::tempdir().unwrap();
        let mut original = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(1, 1).write_to(&mut original, image::ImageFormat::Bmp).unwrap();
        for (width, height) in [(8193u32, 1u32), (8192, 8192)] {
            let mut bytes = original.get_ref().clone();
            bytes[18..22].copy_from_slice(&width.to_le_bytes());
            bytes[22..26].copy_from_slice(&height.to_le_bytes());
            let dimensions = image::ImageReader::new(std::io::Cursor::new(&bytes))
                .with_guessed_format().unwrap().into_dimensions().unwrap();
            assert_eq!(dimensions, (width, height));
            let path = dir.path().join(format!("{width}-{height}.bmp"));
            std::fs::write(&path, bytes).unwrap();
            assert!(derivative_in(&dir.path().join("cache"), &path, 64).is_none());
            assert!(atmosphere_in(&dir.path().join("atmosphere"), &path).is_none());
        }
        assert!(!dir.path().join("cache").exists());
    }

    #[test]
    fn an_oversized_encoded_original_is_rejected_before_decode() {
        let dir = tempfile::tempdir().unwrap();
        let original = cover(dir.path(), "padded.png", 1);
        std::fs::OpenOptions::new().write(true).open(&original).unwrap()
            .set_len(ENCODED_BYTES + 1).unwrap();
        assert_eq!(image::ImageReader::open(&original).unwrap().with_guessed_format().unwrap()
            .into_dimensions().unwrap(), (1, 1));
        assert!(derivative_in(&dir.path().join("cache"), &original, 64).is_none());
        assert!(atmosphere_in(&dir.path().join("atmosphere"), &original).is_none());
    }

    #[test]
    fn a_large_raster_waits_for_the_shared_byte_budget() {
        let budget = DecodeBudget::new();
        let mut large = budget.acquire();
        large.reserve(96 * 1024 * 1024).unwrap();
        std::thread::scope(|scope| {
            let (sender, receiver) = std::sync::mpsc::channel();
            let shared = &budget;
            let waiting = scope.spawn(move || {
                let mut next = shared.acquire();
                sender.send(false).unwrap();
                next.reserve(64 * 1024 * 1024).unwrap();
                sender.send(true).unwrap();
            });
            assert!(!receiver.recv_timeout(std::time::Duration::from_secs(5)).unwrap());
            assert!(receiver.recv_timeout(std::time::Duration::from_millis(50)).is_err());
            drop(large);
            assert!(receiver.recv_timeout(std::time::Duration::from_secs(5)).unwrap());
            waiting.join().unwrap();
        });
        assert_eq!(*budget.used.lock().unwrap(), (0, 0));
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
