//! Pictures that come from an address and not from a file: a station's
//! logo, the cover of the song it is playing.
//!
//! The window draws covers from disk, so a picture is fetched once, checked
//! to be one, and kept in the cache under a name made from its address. It
//! is best effort all the way: whatever fails gives no picture and says
//! nothing, because a missing logo is not a problem the listener can fix.

use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::io::Write;
use std::sync::{mpsc, OnceLock};
use std::time::Duration;

/// A logo is small. More than this is somebody serving something else.
const LARGEST: usize = 2 * 1024 * 1024;
static TOUCHES: OnceLock<mpsc::SyncSender<PathBuf>> = OnceLock::new();

fn touch_later(path: &PathBuf, accessed: std::time::SystemTime) {
    if accessed.elapsed().is_ok_and(|age| age < Duration::from_secs(24 * 60 * 60)) { return }
    let sender = TOUCHES.get_or_init(|| {
        let (sender, receiver) = mpsc::sync_channel::<PathBuf>(1024);
        let _ = std::thread::Builder::new().name("chilpa-cache-touch".into()).spawn(move || {
            for path in receiver {
                if let Ok(file) = std::fs::File::options().write(true).open(path) {
                    let _ = file.set_times(std::fs::FileTimes::new().set_accessed(std::time::SystemTime::now()));
                }
            }
        });
        sender
    });
    let _ = sender.try_send(path.clone());
}

fn folder() -> PathBuf {
    chilpa_core::artwork_cache().join("remote")
}

/// What the bytes are, by looking at them: the extension a file of them
/// takes, or `None` when the window could not draw them.
fn kind(bytes: &[u8]) -> Option<&'static str> {
    match image::guess_format(bytes).ok()? {
        image::ImageFormat::Jpeg => Some("jpg"),
        image::ImageFormat::Png => Some("png"),
        image::ImageFormat::WebP => Some("webp"),
        _ => None,
    }
}

fn name_of(address: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    address.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// The picture at that address if it is already on disk.
pub(crate) fn kept(address: &str) -> Option<String> {
    if address.is_empty() {
        return None;
    }
    let (dir, name) = (folder(), name_of(address));
    ["jpg", "png", "webp"].into_iter().find_map(|extension| {
        let path = dir.join(format!("{name}.{extension}"));
        let meta = path.metadata().ok().filter(|meta| meta.is_file())?;
        if let Ok(accessed) = meta.accessed() { touch_later(&path, accessed); }
        Some(path.to_string_lossy().into_owned())
    })
}

fn store(address: &str, extension: &str, bytes: &[u8]) -> Option<String> {
    let dir = folder();
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join(format!("{}.{}", name_of(address), extension));
    let mut temporary = tempfile::NamedTempFile::new_in(&dir).ok()?;
    temporary.write_all(bytes).ok()?;
    temporary.persist(&path).ok()?;
    crate::storage_cache::note_stored(bytes.len() as u64);
    Some(path.to_string_lossy().into_owned())
}

/// The picture at that address, on disk. Fetched the first time and read
/// from the cache after. `None` when there is none to show.
pub(crate) fn keep(address: &str) -> Option<String> {
    if let Some(kept) = kept(address) {
        return Some(kept);
    }
    let asked = chilpa_core::Fetch {
        url: address,
        user_agent: "Chilpa",
        accept: "image/*",
        timeout: Duration::from_secs(8),
        max_bytes: LARGEST,
    };
    let got = chilpa_core::fetch(&asked).ok().filter(|reply| reply.status == 200)?;
    let extension = kind(&got.body)?;
    image::load_from_memory(&got.body).ok()?;
    store(address, extension, &got.body)
}

/// Guest artwork obeys the installation's domain permissions on every hop.
pub(crate) fn keep_approved(address: &str, domains: &[String]) -> Option<String> {
    if !chilpa_core::worker_url_allowed(address, domains) { return None; }
    if let Some(path) = kept(address) { return Some(path); }
    let started = std::time::Instant::now();
    let mut url = url::Url::parse(address).ok()?;
    for _ in 0..5 {
        if !chilpa_core::worker_url_allowed(url.as_str(), domains) { return None; }
        let timeout = Duration::from_secs(8).checked_sub(started.elapsed())?;
        let reply = chilpa_core::call_server(&chilpa_core::Call { method: "GET", url: url.as_str(),
            headers: &[], body: None, timeout, max_bytes: LARGEST }).ok()?;
        if (300..400).contains(&reply.status) {
            url = url.join(reply.headers.get("location")?).ok()?;
            continue;
        }
        if reply.status != 200 { return None; }
        let extension = kind(&reply.body)?;
        let mut reader = image::ImageReader::new(std::io::Cursor::new(&reply.body)).with_guessed_format().ok()?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(4096);
        limits.max_image_height = Some(4096);
        limits.max_alloc = Some(32 * 1024 * 1024);
        reader.limits(limits);
        reader.decode().ok()?;
        return store(address, extension, &reply.body);
    }
    None
}

/// The same, off the caller's thread: `then` gets the path if there is one.
pub(crate) fn keep_later(address: String, then: impl FnOnce(String) + Send + 'static) {
    let _ = std::thread::Builder::new().name("chilpa-pictures".into()).spawn(move || {
        if let Some(path) = keep(&address) {
            then(path);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_what_the_window_can_draw_is_a_picture() {
        assert_eq!(kind(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0]), Some("png"));
        assert_eq!(kind(&[0xff, 0xd8, 0xff, 0xe0, 0, 0]), Some("jpg"));
        assert_eq!(kind(b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>"), None);
        assert_eq!(kind(b"<!doctype html>"), None);
    }

    #[test]
    fn a_modern_portrait_is_recognized_and_decoded() {
        let picture = image::DynamicImage::new_rgb8(2, 2);
        let mut bytes = std::io::Cursor::new(Vec::new());
        picture.write_to(&mut bytes, image::ImageFormat::WebP).unwrap();
        assert_eq!(kind(bytes.get_ref()), Some("webp"));
        assert!(image::load_from_memory(bytes.get_ref()).is_ok());
    }

    #[test]
    fn an_address_that_is_not_https_gives_no_picture_and_no_error() {
        assert_eq!(keep("http://example.invalid/logo.png"), None);
        assert_eq!(keep("file:///etc/passwd"), None);
    }
}
