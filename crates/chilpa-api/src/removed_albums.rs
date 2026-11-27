//! Albums taken out of the library without touching their files, and the
//! list in Settings > Library that brings them back.

/// The Settings button that opens the list of removed albums.
pub const SHOW: &str = "library.removedAlbums";

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

impl crate::Chilpa {
    /// Takes an album out of the library: it stops being listed, counted and
    /// found, and its files stay where they are.
    pub fn remove_album_from_library(&mut self, album_id: i64) -> bool {
        if album_id <= 0 {
            return false;
        }
        let at = now();
        match self.library().map(|lib| lib.remove_album(album_id, at)) {
            Some(Ok(taken)) if taken > 0 => {
                self.mark_library_changed();
                true
            }
            Some(Err(error)) => {
                self.now.error = chilpa_i18n::t_args("the album could not be removed: {}", &[&error.to_string()]);
                false
            }
            _ => false,
        }
    }

    /// Brings a removed album back as it was.
    pub fn restore_removed_album(&mut self, album_id: i64) -> bool {
        match self.library().map(|lib| lib.restore_album(album_id)) {
            Some(Ok(back)) if back > 0 => {
                self.mark_library_changed();
                true
            }
            Some(Err(error)) => {
                self.now.error = chilpa_i18n::t_args("the album could not be brought back: {}", &[&error.to_string()]);
                false
            }
            _ => false,
        }
    }

    /// The removed albums, the latest first, as JSON.
    pub fn removed_albums_json(&mut self) -> String {
        let albums = self.library().and_then(|lib| lib.removed_albums().ok()).unwrap_or_default();
        serde_json::Value::Array(albums.into_iter().map(|album| serde_json::json!({
            "id": album.id, "title": album.title, "artist": album.artist,
            "artwork_path": album.artwork_path, "tracks": album.tracks, "removed_at": album.removed_at,
        })).collect()).to_string()
    }

    /// Settings > Library asks for the list; the window opens it once it
    /// takes the request.
    pub(crate) fn removed_albums_maintenance(&mut self, key: &str) -> Option<String> {
        if key != SHOW {
            return None;
        }
        self.removed_albums_requested = true;
        Some(String::new())
    }

    pub fn take_removed_albums_request(&mut self) -> bool {
        std::mem::take(&mut self.removed_albums_requested)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn settings_asks_once_for_the_list_of_removed_albums() {
        let mut api = crate::Chilpa::new();
        assert_eq!(api.maintain(super::SHOW), Some(String::new()));
        assert!(api.take_removed_albums_request());
        assert!(!api.take_removed_albums_request(), "the list opens once per press");
    }

    #[test]
    fn nothing_is_removed_or_brought_back_without_an_album() {
        let mut api = crate::Chilpa::new();
        assert!(!api.remove_album_from_library(0));
        assert!(!api.remove_album_from_library(-4), "a source's record is not the library's");
        assert!(!api.restore_removed_album(12));
        assert_eq!(api.removed_albums_json(), "[]");
    }
}
