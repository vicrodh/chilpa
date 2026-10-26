use super::{Browser, UiError};
use crate::device_sync::failure;
use sha2::{Digest, Sha256};

pub(crate) struct Bookmark {
    view: &'static str,
    artist: Option<[u8; 32]>,
    album: Option<[u8; 32]>,
    query: String,
    page: usize,
    parent: Option<(String, usize)>,
    artist_parent: Option<(String, usize)>,
}
fn identity(artist: &str, title: &str, playlist: bool) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(artist.len().to_le_bytes());
    hash.update(artist.as_bytes());
    hash.update(title.len().to_le_bytes());
    hash.update(title.as_bytes());
    hash.update([u8::from(playlist)]);
    hash.finalize().into()
}

impl Browser {
    pub fn bookmark(&self) -> Bookmark {
        Bookmark {
            view: self.view,
            artist: self
                .artist
                .map(|i| identity(&self.artists[i].name, "", false)),
            album: self.album.map(|i| {
                let a = &self.albums[i];
                identity(&a.artist, &a.title, a.playlist)
            }),
            query: self.query.clone(),
            page: self.page,
            parent: self.parent.clone(),
            artist_parent: self.artist_parent.clone(),
        }
    }
    pub fn restore(&mut self, bookmark: Bookmark) {
        self.set_view(bookmark.view)
            .expect("a saved view belongs to this browser");
        let artist = bookmark.artist.and_then(|id| {
            self.artists
                .iter()
                .position(|a| identity(&a.name, "", false) == id)
        });
        let album = bookmark.album.and_then(|id| {
            self.albums
                .iter()
                .position(|a| identity(&a.artist, &a.title, a.playlist) == id)
        });
        if bookmark.artist.is_some() && artist.is_none()
            || bookmark.album.is_some() && album.is_none()
        {
            return;
        }
        self.artist = artist;
        self.album = album;
        self.parent = bookmark.parent;
        self.artist_parent = bookmark.artist_parent;
        self.filter(&bookmark.query);
        self.change_page(bookmark.page);
    }
    pub fn focused_playlist(&self) -> Option<String> {
        self.playlist.map(|i| format!("playlist:{i}"))
    }
    pub fn set_view(&mut self, view: &str) -> Result<(), UiError> {
        self.view = match view {
            "music" => "music",
            "artists" => "artists",
            "albums" => "albums",
            "tracks" => "tracks",
            "playlists" => "playlists",
            _ => return Err(unavailable()),
        };
        self.artist = None;
        self.album = None;
        self.playlist = None;
        self.playlist_members = Vec::new();
        self.artist_parent = None;
        self.parent = None;
        self.playlist_parent = None;
        self.filter("");
        Ok(())
    }
    pub fn open(&mut self, id: &str) -> Result<(), UiError> {
        if self.playlist.is_some() {
            return Err(unavailable());
        }
        if let Some(index) = index(id, "artist:").filter(|i| {
            *i < self.artists.len()
                && self.view == "artists"
                && self.artist.is_none()
                && self.album.is_none()
        }) {
            self.artist = Some(index);
            self.artist_parent = Some((std::mem::take(&mut self.query), self.page));
        } else if let Some(index) = index(id, "album:").filter(|i| {
            *i < self.albums.len()
                && self.album.is_none()
                && self.view != "tracks"
                && self.view != "playlists"
                && self
                    .artist
                    .is_none_or(|a| self.artists[a].albums.contains(i))
        }) {
            self.album = Some(index);
            self.parent = Some((std::mem::take(&mut self.query), self.page));
        } else {
            return Err(unavailable());
        }
        self.search.clear();
        self.page = 0;
        Ok(())
    }
    /// I return false at the selection root so its owner can return to the picker.
    pub fn back(&mut self) -> bool {
        let parent = if self.playlist.take().is_some() {
            self.playlist_members = Vec::new();
            self.playlist_parent.take()
        } else if self.album.take().is_some() {
            self.parent.take()
        } else if self.artist.take().is_some() {
            self.artist_parent.take()
        } else {
            return false;
        };
        let (query, page) = parent.unwrap_or_default();
        self.search = query.to_lowercase();
        self.query = query;
        self.page = page;
        true
    }
}
pub(super) fn index(id: &str, prefix: &str) -> Option<usize> {
    id.strip_prefix(prefix)?.parse().ok()
}
pub(super) fn unavailable() -> UiError {
    failure(
        chilpa_i18n::mark("This album is no longer in the current view."),
        "",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device_sync::browser::Track;
    use chilpa_sync::Cancellation;

    fn browser(names: &[&str]) -> Browser {
        Browser::new(
            "recorded".into(),
            "Card".into(),
            true,
            names
                .iter()
                .map(|name| Track {
                    title: format!("Song {name}"),
                    artist: "Artist".into(),
                    album: (*name).into(),
                    disc: 1,
                    number: 1,
                    path: String::new(),
                    size: String::new(),
                    playlist: false,
                })
                .collect(),
            &Cancellation::default(),
        )
        .unwrap()
    }
    #[test]
    fn refresh_restores_group_identity_instead_of_a_previous_row_index() {
        let mut original = browser(&["Record Z"]);
        original.set_view("albums").unwrap();
        original.filter("Record");
        original.open("album:0").unwrap();
        original.filter("Song");
        let mut changed = browser(&["Record A", "Record Z"]);
        changed.restore(original.bookmark());
        let view = changed.document();
        assert_eq!(view["album"], "Record Z");
        assert_eq!(view["rows"][0]["title"], "Song Record Z");
        assert_eq!(view["query"], "Song");
        assert!(changed.back());
        assert_eq!(changed.document()["view"], "albums");
        assert_eq!(changed.document()["query"], "Record");
        let mut missing = browser(&["Record A"]);
        missing.restore(original.bookmark());
        assert!(missing.document()["album"].is_null());
        assert_eq!(missing.document()["query"], "");
    }
}
