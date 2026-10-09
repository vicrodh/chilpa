//! The covers and pictures the listener chooses: a record's, of the library
//! or of a plugin, and an artist's.
//!
//! The image picked is copied into Chilpa's own folder under a name made of
//! its bytes, so moving or deleting the one picked changes nothing, and the
//! library keeps which goes where. What is shown prefers it: the library's
//! queries, the catalog's cards, the artists' rows and what is playing. What
//! a scan finds never sees it, so taking it away brings the original back.

use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

/// A cover is a picture; past this it is something else.
const LARGEST: u64 = 64 * 1024 * 1024;

fn folder() -> PathBuf {
    chilpa_dirs::data().join("artwork")
}

/// What is playing and the cover chosen for it.
#[derive(Default)]
pub(crate) struct State {
    /// Moves on every choice, so the window knows to read again.
    rev: i32,
    /// The record last looked up, at which revision, and its chosen cover.
    looked: Option<(i64, i32, Option<String>)>,
    /// The chosen cover put over what the player found, and what it found.
    dressed: Option<(String, String)>,
}

/// The extension a file of these bytes takes, or `None` when the window
/// could not draw them.
fn kind(bytes: &[u8]) -> Option<&'static str> {
    match image::guess_format(bytes).ok()? {
        image::ImageFormat::Jpeg => Some("jpg"),
        image::ImageFormat::Png => Some("png"),
        image::ImageFormat::WebP => Some("webp"),
        _ => None,
    }
}

/// Copies the image into the folder, named by its bytes. The error is what
/// the listener reads.
pub(crate) fn keep(picked: &Path, into: &Path) -> Result<String, String> {
    let unreadable = || chilpa_i18n::t("The image could not be read");
    let size = std::fs::metadata(picked).map_err(|_| unreadable())?.len();
    if size > LARGEST {
        return Err(chilpa_i18n::t("The image is too large"));
    }
    let bytes = std::fs::read(picked).map_err(|_| unreadable())?;
    let Some(extension) = kind(&bytes) else {
        return Err(chilpa_i18n::t("That file is not a JPEG, PNG or WebP image"));
    };
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    let path = into.join(format!("{:016x}.{extension}", hasher.finish()));
    if !path.is_file() {
        let unwritable = |_| chilpa_i18n::t("The image could not be kept");
        std::fs::create_dir_all(into).map_err(unwritable)?;
        let part = path.with_extension("part");
        std::fs::write(&part, &bytes).map_err(unwritable)?;
        std::fs::rename(&part, &path).map_err(unwritable)?;
    }
    Ok(path.display().to_string())
}

/// The name "Save as…" suggests: `{title} - Cover` for a record, the name
/// for an artist, with the extension the bytes really have.
pub(crate) fn suggested_name(stem: &str, file: &Path) -> String {
    let extension = std::fs::read(file).ok().and_then(|bytes| kind(&bytes)).unwrap_or("jpg");
    let stem: String = stem.chars().map(|c| if matches!(c, '/' | '\\' | '\0') { '-' } else { c }).collect();
    format!("{}.{extension}", stem.trim())
}

impl crate::Chilpa {
    /// Moves on every choice the listener makes.
    pub fn artwork_rev(&self) -> i32 {
        self.artwork.rev
    }

    /// The cover chosen for a record, if one was: of the library when the
    /// id is positive, of a plugin when it is negative.
    pub(crate) fn chosen_album_art(&mut self, album_id: i64) -> Option<String> {
        match album_id {
            id if id > 0 => self.library()?.album_artwork(id).ok().flatten(),
            id if id < 0 => self.catalog.chosen_art(id),
            _ => None,
        }
    }

    /// Whether the record shows a cover the listener chose.
    pub fn album_art_chosen(&mut self, album_id: i64) -> bool {
        self.chosen_album_art(album_id).is_some()
    }

    /// Gives a record the image picked, or with `None` takes the chosen one
    /// away. Says whether it changed; why not goes to the toast.
    pub fn choose_album_art(&mut self, album_id: i64, picked: Option<&Path>) -> bool {
        let path = match picked.map(|p| keep(p, &folder())).transpose() {
            Ok(path) => path,
            Err(why) => {
                self.now.error = why;
                return false;
            }
        };
        let before = self.chosen_album_art(album_id);
        let changed = if album_id > 0 {
            self.library().is_some_and(|lib| lib.set_album_artwork(album_id, path.as_deref()).unwrap_or(false))
        } else {
            let Some((plugin, identity)) = self.catalog.identity_of(album_id) else { return false };
            self.library().is_some_and(|lib| lib.set_remote_artwork(&plugin, &identity, path.as_deref()).unwrap_or(false))
        };
        if changed {
            self.chosen_art_changed(before);
        }
        changed
    }

    /// The picture chosen for an artist, or empty.
    pub fn artist_art(&mut self, name: &str) -> String {
        self.library().and_then(|lib| lib.artist_artwork(name).ok().flatten()).unwrap_or_default()
    }

    /// Gives an artist the image picked, or with `None` takes it away.
    pub fn choose_artist_art(&mut self, name: &str, picked: Option<&Path>) -> bool {
        let path = match picked.map(|p| keep(p, &folder())).transpose() {
            Ok(path) => path,
            Err(why) => {
                self.now.error = why;
                return false;
            }
        };
        let before = self.library().and_then(|lib| lib.artist_artwork(name).ok().flatten());
        let changed = self.library().is_some_and(|lib| lib.set_artist_artwork(name, path.as_deref()).unwrap_or(false));
        if changed {
            self.chosen_art_changed(before);
        }
        changed
    }

    /// The address a record's cover came from, for "Open in browser":
    /// a record of a plugin has one, a file on disk does not.
    pub fn album_art_address(&mut self, album_id: i64) -> String {
        if album_id < 0 { self.catalog.image_of(album_id) } else { String::new() }
    }

    /// What "Save as…" copies for a record: the cover it shows and the name
    /// to suggest.
    pub fn album_art_to_save(&mut self, album_id: i64, title: &str) -> Option<(PathBuf, String)> {
        let shown = PathBuf::from(self.album(album_id)?.artwork_path);
        if !shown.is_file() {
            return None;
        }
        let stem = if title.trim().is_empty() { "album".to_string() } else { format!("{} - Cover", title.trim()) };
        let name = suggested_name(&stem, &shown);
        Some((shown, name))
    }

    /// What "Save as…" copies for an artist: the picture shown on its page,
    /// which the window passes, and the name to suggest.
    pub fn artist_art_to_save(&mut self, name: &str, shown: &Path) -> Option<(PathBuf, String)> {
        if !shown.is_file() {
            return None;
        }
        let stem = if name.trim().is_empty() { "artist" } else { name.trim() };
        Some((shown.to_path_buf(), suggested_name(stem, shown)))
    }

    /// Copies a picture where "Save as…" was told.
    pub fn save_art(&mut self, from: &Path, to: &Path) -> bool {
        let saved = std::fs::copy(from, to).is_ok();
        if !saved {
            self.now.error = chilpa_i18n::t("The image could not be saved");
        }
        saved
    }

    /// The covers chosen for records of plugins, read again into the
    /// catalog, which shows them.
    pub(crate) fn load_chosen_remote_art(&mut self) {
        let chosen = self.library().and_then(|lib| lib.remote_artworks().ok()).unwrap_or_default();
        self.catalog.choose_art(chosen);
    }

    /// Whatever shows a cover reads again, and a file no choice names any
    /// more goes.
    fn chosen_art_changed(&mut self, before: Option<String>) {
        self.artwork.rev += 1;
        self.mark_library_changed();
        self.load_chosen_remote_art();
        let Some(before) = before.map(PathBuf::from) else { return };
        let ours = before.starts_with(folder());
        let named = self.library().is_none_or(|lib| lib.artwork_in_use(&before.display().to_string()).unwrap_or(true));
        if ours && !named {
            let _ = std::fs::remove_file(&before);
        }
    }

    /// The last of a tick, after the player's events: the listen's own
    /// bookkeeping, then the cover chosen for the record that sounds put
    /// over the one the player found, or the found one back when the
    /// choice went away.
    pub(crate) fn finish_tick(&mut self, started: bool) {
        self.finish_buffer_switch();
        self.persist_progress(started);
        self.dress_now_art();
    }

    fn dress_now_art(&mut self) {
        let album = self.now.album_id;
        let rev = self.artwork.rev;
        if self.artwork.looked.as_ref().is_none_or(|(id, at, _)| *id != album || *at != rev) {
            let chosen = self.chosen_album_art(album);
            self.artwork.looked = Some((album, rev, chosen));
        }
        let chosen = self.artwork.looked.as_ref().and_then(|(_, _, chosen)| chosen.clone());
        // Something else set the cover since: that one is what was found.
        if self.artwork.dressed.as_ref().is_some_and(|(shown, _)| *shown != self.now.artwork_path) {
            self.artwork.dressed = None;
        }
        match chosen {
            Some(chosen) if chosen != self.now.artwork_path => {
                let found = std::mem::replace(&mut self.now.artwork_path, chosen.clone());
                let found = self.artwork.dressed.take().map_or(found, |(_, found)| found);
                self.artwork.dressed = Some((chosen, found));
            }
            None => {
                if let Some((_, found)) = self.artwork.dressed.take() {
                    self.now.artwork_path = found;
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png() -> Vec<u8> {
        let mut bytes = Vec::new();
        image::RgbImage::new(2, 2).write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png).unwrap();
        bytes
    }

    #[test]
    fn a_picked_image_is_copied_under_a_name_made_of_its_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let picked = dir.path().join("my cover.PNG");
        std::fs::write(&picked, png()).unwrap();
        let kept = keep(&picked, &dir.path().join("kept")).unwrap();
        assert!(kept.ends_with(".png"), "{kept}");
        assert_eq!(std::fs::read(&kept).unwrap(), png());
        assert_eq!(keep(&picked, &dir.path().join("kept")).unwrap(), kept, "the same bytes, the same file");
        std::fs::remove_file(&picked).unwrap();
        assert!(Path::new(&kept).is_file(), "the copy does not depend on the one picked");
    }

    #[test]
    fn what_is_not_an_image_is_refused_with_a_reason() {
        let dir = tempfile::tempdir().unwrap();
        let picked = dir.path().join("notes.jpg");
        std::fs::write(&picked, b"not a picture").unwrap();
        assert!(keep(&picked, dir.path()).is_err());
        assert!(keep(&dir.path().join("missing.jpg"), dir.path()).is_err());
    }

    #[test]
    fn save_as_suggests_the_real_extension_and_a_name_a_file_can_have() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("cover.jpg");
        std::fs::write(&file, png()).unwrap();
        assert_eq!(suggested_name("AC/DC - Cover", &file), "AC-DC - Cover.png");
    }
}
