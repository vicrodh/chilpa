//! Covers: checking them, finding them, and keeping one in wait.
//!
//! A cover is kept byte for byte as it came. It is decoded to know that it
//! is one and how large it is, and never encoded again.

use super::worker::{Done, Job};
use super::State;
use crate::Chilpa;
use chilpa_meta::{cover_art, discogs, musicbrainz, Failure, Http, Net, Provider};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub(crate) const LARGEST: usize = 25 * 1024 * 1024;
pub(crate) const LONGEST_SIDE: u32 = 12_000;
/// The most a preview may weigh.
const LARGEST_PREVIEW: usize = 2 * 1024 * 1024;
/// How many candidates a search keeps.
const MOST: usize = 10;

/// A cover in wait: chosen, checked, and not yet saved.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Waiting {
    pub bytes: Vec<u8>,
    /// "jpg" | "png"
    pub extension: &'static str,
    /// "image/jpeg" | "image/png"
    pub mime: &'static str,
    pub width: u32,
    pub height: u32,
    /// The same bytes on disk, for the window to show.
    pub preview: PathBuf,
    /// "file" | "musicbrainz" | "discogs"
    pub from: &'static str,
}

/// A cover found, with its preview already on disk.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Candidate {
    pub preview: PathBuf,
    pub title: String,
    pub detail: String,
    /// Where the cover itself is fetched from.
    pub url: String,
}

/// What a cover is: its extension, its kind and its size. `Err` is why it
/// is not one, in the user's language.
pub(crate) fn check(bytes: &[u8]) -> Result<(&'static str, &'static str, u32, u32), String> {
    if bytes.is_empty() || bytes.len() > LARGEST {
        return Err(chilpa_i18n::t("the cover has to be an image of up to 25 MiB"));
    }
    let (extension, mime, format) = match image::guess_format(bytes) {
        Ok(image::ImageFormat::Jpeg) => ("jpg", "image/jpeg", image::ImageFormat::Jpeg),
        Ok(image::ImageFormat::Png) => ("png", "image/png", image::ImageFormat::Png),
        _ => return Err(chilpa_i18n::t("the cover has to be a JPEG or a PNG")),
    };
    let Ok(decoded) = image::load_from_memory_with_format(bytes, format) else {
        return Err(chilpa_i18n::t("that file is not an image"));
    };
    let (width, height) = (decoded.width(), decoded.height());
    if width == 0 || height == 0 || width > LONGEST_SIDE || height > LONGEST_SIDE {
        return Err(chilpa_i18n::t("the cover is larger than 12,000 pixels on a side"));
    }
    Ok((extension, mime, width, height))
}

/// Where what the editor keeps in wait goes.
pub(crate) fn folder() -> PathBuf {
    chilpa_dirs::cache().join("editor")
}

/// Empties it. What is there belongs to a record that is no longer open.
pub(crate) fn sweep() {
    let Ok(entries) = std::fs::read_dir(folder()) else { return };
    for entry in entries.flatten() {
        let _ = std::fs::remove_file(entry.path());
    }
}

/// A name of its own for every cover put in wait: an answer that arrives
/// late for a cover no longer wanted removes its own file, never the one
/// that replaced it.
fn waiting_path(session: u64, extension: &str) -> PathBuf {
    static TAKEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = TAKEN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    folder().join(format!("{session}-{n}-cover.{extension}"))
}

pub(crate) fn forget_waiting(session: u64) {
    let Ok(entries) = std::fs::read_dir(folder()) else { return };
    let prefix = format!("{session}-");
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(&prefix) && name.contains("-cover.") {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// What a finished job left on disk for a record that is no longer the
/// one waiting for it.
pub(crate) fn discard(done: &Done) {
    match done {
        Done::Candidates { candidates: Ok(found), .. } => {
            for candidate in found {
                let _ = std::fs::remove_file(&candidate.preview);
            }
        }
        Done::Cover { cover: Ok(waiting), .. } => {
            let _ = std::fs::remove_file(&waiting.preview);
        }
        _ => {}
    }
}

/// Checks a cover and leaves it in wait.
pub(crate) fn wait(session: u64, bytes: Vec<u8>, from: &'static str) -> Result<Waiting, String> {
    let (extension, mime, width, height) = check(&bytes)?;
    let preview = waiting_path(session, extension);
    forget_waiting(session);
    let kept = std::fs::create_dir_all(folder()).and_then(|()| std::fs::write(&preview, &bytes));
    if let Err(e) = kept {
        return Err(chilpa_i18n::t_args("the cover could not be kept: {}", &[&e.to_string()]));
    }
    Ok(Waiting { bytes, extension, mime, width, height, preview, from })
}

/// A cover from a file somebody picked.
pub(crate) fn from_file(session: u64, path: &Path) -> Result<Waiting, String> {
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    if size == 0 || size > LARGEST as u64 {
        return Err(chilpa_i18n::t("the cover has to be an image of up to 25 MiB"));
    }
    match std::fs::read(path) {
        Ok(bytes) => wait(session, bytes, "file"),
        Err(e) => Err(chilpa_i18n::t_args("the cover could not be read: {}", &[&e.to_string()])),
    }
}

/// A picture, fetched from where it is. `None` for anything that is not one.
fn picture_at(http: &dyn Http, url: &str, timeout: Duration, most: usize) -> Option<Vec<u8>> {
    let reply = http.get(url, "image/*", timeout, most).ok()?;
    (reply.status == 200 && check(&reply.body).is_ok()).then_some(reply.body)
}

/// Keeps a candidate's preview on disk. `None` when it has none to show:
/// a candidate that cannot be seen is not listed.
#[allow(clippy::too_many_arguments)]
fn candidate(http: &dyn Http, session: u64, turn: u64, at: usize, preview: &str, url: String, title: String, detail: String) -> Option<Candidate> {
    let bytes = picture_at(http, preview, Duration::from_secs(10), LARGEST_PREVIEW)?;
    let path = folder().join(format!("{session}-{turn}-{at}.img"));
    std::fs::create_dir_all(folder()).ok()?;
    std::fs::write(&path, bytes).ok()?;
    Some(Candidate { preview: path, title, detail, url })
}

/// Every candidate's preview at once, in the order asked. Each is a request
/// to a host with no pace of its own, and one after the other a search of
/// eleven took the time of all eleven: the archive answers slowly.
fn candidates(http: &dyn Http, session: u64, turn: u64, wanted: Vec<(String, String, String, String)>) -> Vec<Candidate> {
    std::thread::scope(|scope| {
        let running: Vec<_> = wanted
            .into_iter()
            .enumerate()
            .map(|(at, (preview, url, title, detail))| scope.spawn(move || candidate(http, session, turn, at, &preview, url, title, detail)))
            .collect();
        running.into_iter().filter_map(|asked| asked.join().ok().flatten()).collect()
    })
}

/// The covers a search finds.
pub(crate) fn find(
    http: &dyn Http,
    session: u64,
    turn: u64,
    provider: Provider,
    title: &str,
    artist: &str,
    catalog_number: &str,
) -> Result<Vec<Candidate>, Failure> {
    let mut found = match provider {
        Provider::MusicBrainz => {
            let hits = musicbrainz::search(&Net::musicbrainz(http), title, artist, catalog_number)?;
            let mut wanted: Vec<(String, String, String)> = Vec::new();
            // The cover the archive picks for the group comes first.
            if let Some(first) = hits.first().filter(|h| !h.release_group_id.is_empty()) {
                if let Ok(url) = cover_art::group_front(&first.release_group_id, cover_art::PREVIEW) {
                    wanted.push((url, first.title.clone(), first.date.clone()));
                }
            }
            for hit in hits.iter().take(MOST) {
                if let Ok(url) = cover_art::release_front(&hit.id, cover_art::PREVIEW) {
                    wanted.push((url, hit.title.clone(), hit.date.clone()));
                }
            }
            let wanted = wanted.into_iter().map(|(preview, title, detail)| {
                let url = cover_art::original_of(&preview);
                (preview, url, title, detail)
            });
            candidates(http, session, turn, wanted.collect())
        }
        Provider::Discogs => {
            let covers = discogs::images(&Net::proxy(http), artist, title, catalog_number)?;
            // The preview comes straight from where the image is. The cover
            // itself goes through the proxy.
            let wanted = covers.into_iter().map(|cover| {
                let detail = match cover.year.as_str() {
                    "" => format!("{} × {}", cover.width, cover.height),
                    year => format!("{} × {} · {year}", cover.width, cover.height),
                };
                (cover.url.clone(), cover.url, cover.release, detail)
            });
            candidates(http, session, turn, wanted.collect())
        }
    };
    found.truncate(MOST + 1);
    Ok(found)
}

/// The cover itself, fetched and left in wait.
pub(crate) fn fetch(http: &dyn Http, session: u64, provider: Provider, url: &str) -> Result<Waiting, String> {
    let could_not = || chilpa_i18n::t("that cover could not be fetched");
    let (address, from) = match provider {
        Provider::MusicBrainz => (url.to_string(), "musicbrainz"),
        Provider::Discogs => {
            // The proxy counts this request with every other it is asked.
            Net::proxy(http).pace.wait();
            (discogs::image_url(url).map_err(|_| could_not())?, "discogs")
        }
    };
    let reply = http.get(&address, "image/*", Duration::from_secs(30), LARGEST).map_err(|_| could_not())?;
    if reply.status != 200 {
        return Err(could_not());
    }
    wait(session, reply.body, from)
}

impl Chilpa {
    /// "musicbrainz" | "discogs"
    pub fn metadata_set_artwork_provider(&mut self, provider: &str) -> bool {
        let Some(provider) = Provider::from_name(provider) else { return false };
        if provider == Provider::MusicBrainz && !self.musicbrainz_enabled() { return false; }
        let editor = &mut self.metadata;
        editor.artwork_provider = provider;
        editor.artwork_turn += 1;
        editor.artwork_searching = false;
        editor.artwork_loading = false;
        for candidate in editor.candidates.drain(..) {
            let _ = std::fs::remove_file(candidate.preview);
        }
        editor.refresh();
        true
    }

    /// Looks for covers with what the form holds.
    pub fn metadata_search_artwork(&mut self) -> bool {
        let off = self.metadata_lookups_off();
        let musicbrainz_off = !self.musicbrainz_enabled();
        let editor = &mut self.metadata;
        let Some(draft) = editor.draft.as_ref().filter(|_| editor.state == State::Ready) else { return false };
        if off {
            return editor.refuse(chilpa_i18n::t("looking records up is turned off in Settings"));
        }
        if musicbrainz_off && editor.artwork_provider == Provider::MusicBrainz {
            return editor.refuse(chilpa_i18n::t("MusicBrainz is turned off in Integrations"));
        }
        let (title, artist, catalog_number) = (
            draft.album_cell("title").now.clone(),
            draft.album_cell("album_artist").now.clone(),
            draft.album_cell("catalog_number").now.clone(),
        );
        if title.is_empty() && artist.is_empty() && catalog_number.is_empty() {
            return editor.refuse(chilpa_i18n::t("type a title, an artist or a catalog number to search"));
        }
        editor.artwork_turn += 1;
        editor.artwork_searching = true;
        editor.artwork_loading = false;
        for candidate in editor.candidates.drain(..) {
            let _ = std::fs::remove_file(candidate.preview);
        }
        editor.error.clear();
        editor.notice.clear();
        let job = Job::ArtworkSearch { session: editor.session, turn: editor.artwork_turn, provider: editor.artwork_provider, title, artist, catalog_number };
        editor.ask(job);
        editor.refresh();
        true
    }

    /// Fetches one of the covers found and leaves it in wait.
    pub fn metadata_choose_artwork(&mut self, candidate: usize) -> bool {
        let off = self.metadata_lookups_off();
        let musicbrainz_off = !self.musicbrainz_enabled();
        let editor = &mut self.metadata;
        if editor.state != State::Ready {
            return false;
        }
        if off {
            return editor.refuse(chilpa_i18n::t("looking records up is turned off in Settings"));
        }
        if musicbrainz_off && editor.artwork_provider == Provider::MusicBrainz {
            return editor.refuse(chilpa_i18n::t("MusicBrainz is turned off in Integrations"));
        }
        let Some(url) = editor.candidates.get(candidate).map(|c| c.url.clone()) else { return false };
        editor.artwork_turn += 1;
        editor.artwork_loading = true;
        editor.error.clear();
        let job = Job::ArtworkFetch { session: editor.session, turn: editor.artwork_turn, provider: editor.artwork_provider, url };
        editor.ask(job);
        editor.refresh();
        true
    }

    /// Takes a cover from a file somebody picked.
    pub fn metadata_artwork_from_file(&mut self, path: &Path) -> bool {
        let editor = &mut self.metadata;
        if editor.state != State::Ready {
            return false;
        }
        editor.artwork_turn += 1;
        editor.artwork_loading = true;
        editor.error.clear();
        let job = Job::ArtworkFile { session: editor.session, turn: editor.artwork_turn, path: path.to_path_buf() };
        editor.ask(job);
        editor.refresh();
        true
    }

    /// Lets go of the cover in wait. The cover the record has is left alone.
    pub fn metadata_clear_artwork(&mut self) {
        let editor = &mut self.metadata;
        editor.artwork_turn += 1;
        editor.artwork_loading = false;
        editor.waiting = None;
        forget_waiting(editor.session);
        editor.refresh();
    }
}

#[cfg(test)]
pub(crate) mod testing {
    /// A picture of that many pixels a side, in that format.
    pub(crate) fn picture(side: u32, format: image::ImageFormat) -> Vec<u8> {
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::RgbImage::from_fn(side, side, |x, y| image::Rgb([(x % 256) as u8, (y % 256) as u8, 90])).write_to(&mut bytes, format).unwrap();
        bytes.into_inner()
    }
}

#[cfg(test)]
mod tests {
    use super::testing::picture;
    use super::*;

    /// A network that answers by address: a search with three releases,
    /// and a preview for every cover but the second release's.
    struct ByAddress;

    const RELEASES: [&str; 3] =
        ["11111111-1111-4111-8111-111111111111", "22222222-2222-4222-8222-222222222222", "33333333-3333-4333-8333-333333333333"];

    impl Http for ByAddress {
        fn get(&self, url: &str, _: &str, _: Duration, _: usize) -> Result<chilpa_meta::Reply, String> {
            let (status, body) = if url.contains("/ws/2/release?") {
                let releases: Vec<String> = RELEASES.iter().map(|id| format!(r#"{{"id":"{id}","title":"{id}"}}"#)).collect();
                (200, format!(r#"{{"releases":[{}]}}"#, releases.join(",")).into_bytes())
            } else if url.contains(RELEASES[1]) {
                (404, Vec::new())
            } else {
                (200, picture(8, image::ImageFormat::Png))
            };
            Ok(chilpa_meta::Reply { status, headers: Default::default(), body })
        }
    }

    #[test]
    fn covers_found_at_once_keep_their_order_and_a_missing_one_is_left_out() {
        let found = find(&ByAddress, u64::MAX, 1, Provider::MusicBrainz, "A record", "Someone", "").unwrap();
        let titles: Vec<&str> = found.iter().map(|c| c.title.as_str()).collect();
        assert_eq!(titles, [RELEASES[0], RELEASES[2]]);
        assert!(found.iter().all(|c| c.url.ends_with("/front")), "the cover kept is the original, not the preview");
        for candidate in &found {
            let _ = std::fs::remove_file(&candidate.preview);
        }
    }

    #[test]
    fn a_cover_says_what_it_is() {
        assert_eq!(check(&picture(40, image::ImageFormat::Png)), Ok(("png", "image/png", 40, 40)));
        assert_eq!(check(&picture(12, image::ImageFormat::Jpeg)), Ok(("jpg", "image/jpeg", 12, 12)));
    }

    #[test]
    fn what_is_not_an_image_is_refused() {
        assert!(check(b"").is_err());
        assert!(check(b"not a picture at all").is_err());
        // A PNG's first bytes and nothing of a PNG after them.
        assert!(check(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 1, 2, 3]).is_err());
    }

    #[test]
    fn a_picture_of_another_kind_is_refused_by_name() {
        let other = b"GIF89a\x01\x00\x01\x00\x00\x00\x00;";
        assert_eq!(check(other), Err(chilpa_i18n::t("the cover has to be a JPEG or a PNG")));
    }
}
