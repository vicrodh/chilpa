//! I retain one bounded metadata snapshot and publish at most 200 rows per navigation step.
use super::{
    catalog::{Pick, Profile, Store},
    failure, library, UiError,
};
use chilpa_sync::{AssetKind, Cancellation, State, MAX_ASSETS, MAX_METADATA_BYTES};
use std::{collections::BTreeMap, path::Path};

mod navigation;
pub(crate) use navigation::Bookmark;
mod presentation;
mod playlist;

const PAGE: usize = 200;
pub(super) struct Track {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub disc: u32,
    pub number: u32,
    pub path: String,
    pub size: String,
    pub playlist: bool,
}
struct Artist {
    name: String,
    albums: Vec<usize>,
}
struct Album {
    playlist: bool,
    title: String,
    artist: String,
    tracks: Vec<usize>,
}
pub(super) struct Browser {
    pub source: String,
    title: String,
    recorded: bool,
    tracks: Vec<Track>,
    albums: Vec<Album>,
    artists: Vec<Artist>,
    view: &'static str,
    artist: Option<usize>,
    artist_parent: Option<(String, usize)>,
    playlist: Option<usize>,
    playlist_members: Vec<usize>,
    playlist_parent: Option<(String, usize)>,
    album: Option<usize>,
    query: String,
    search: String,
    parent: Option<(String, usize)>,
    page: usize,
}
impl Browser {
    fn new(
        source: String,
        title: String,
        recorded: bool,
        tracks: Vec<Track>,
        cancel: &Cancellation,
    ) -> Result<Self, UiError> {
        cancel.check().map_err(UiError::from)?;
        let mut bytes = 0usize;
        if tracks.len() > MAX_ASSETS {
            return Err(limit());
        }
        let mut groups = BTreeMap::<(bool, String, String), Vec<usize>>::new();
        for (index, track) in tracks.iter().enumerate() {
            cancel.check().map_err(UiError::from)?;
            bytes = bytes.saturating_add(
                track.title.len()
                    + track.artist.len()
                    + track.album.len()
                    + track.path.len()
                    + track.size.len(),
            );
            if bytes > MAX_METADATA_BYTES {
                return Err(limit());
            }
            let title = if track.playlist {
                chilpa_i18n::t("Playlists")
            } else if track.album.is_empty() {
                chilpa_i18n::t("Unknown album")
            } else {
                track.album.clone()
            };
            groups
                .entry((track.playlist, track.artist.clone(), title))
                .or_default()
                .push(index);
        }
        let albums: Vec<Album> = groups
            .into_iter()
            .map(|((playlist, artist, title), mut indices)| {
                indices.sort_by_key(|i| (tracks[*i].disc, tracks[*i].number, *i));
                Album {
                    playlist,
                    title,
                    artist,
                    tracks: indices,
                }
            })
            .collect();
        let mut artists = BTreeMap::<String, Vec<usize>>::new();
        for (index, album) in albums.iter().enumerate() {
            bytes = bytes.saturating_add(album.title.len() + album.artist.len());
            if !album.playlist { artists.entry(album.artist.clone()).or_default().push(index); }
        }
        let artists: Vec<_> = artists.into_iter().map(|(name, albums)| Artist { name, albums }).collect();
        bytes = bytes.saturating_add(artists.iter().map(|a| a.name.len()).sum::<usize>());
        if bytes > MAX_METADATA_BYTES { return Err(limit()); }
        Ok(Self {
            source,
            title,
            recorded,
            tracks,
            albums,
            artists,
            view: "music",
            artist: None,
            artist_parent: None,
            playlist: None,
            playlist_members: Vec::new(),
            playlist_parent: None,
            album: None,
            query: String::new(),
            search: String::new(),
            parent: None,
            page: 0,
        })
    }
    pub fn selection(path: &Path, pick: &Pick, cancel: &Cancellation) -> Result<Self, UiError> {
        let tracks = library::tracks(path, pick, cancel)?;
        let title = if matches!(pick, Pick::Tracks { .. }) {
            library::describe(&tracks).0
        } else {
            pick.name().into()
        };
        let rows = tracks
            .into_iter()
            .map(|t| Track {
                title: t.title,
                artist: t.artist,
                album: t.album,
                disc: t.disc,
                number: t.number,
                path: String::new(),
                size: String::new(),
                playlist: false,
            })
            .collect();
        Self::new(pick.id().into(), title, false, rows, cancel)
    }
    pub fn recorded(
        store: &Store,
        profile: &Profile,
        cancel: &Cancellation,
    ) -> Result<Self, UiError> {
        let files = State::recorded(&store.state_path(&profile.id)?, &profile.target_id, cancel)
            .map_err(|e| {
                failure(
                    chilpa_i18n::mark("I could not read the last synced music for this storage."),
                    e,
                )
            })?;
        let rows = files
            .into_iter()
            .map(|f| Track {
                title: if f.title.is_empty() {
                    f.path.filename().into()
                } else {
                    f.title
                },
                artist: f.artist,
                album: f.album,
                disc: 0,
                number: 0,
                path: f.path.to_string(),
                size: super::bytes(f.size),
                playlist: f.kind == AssetKind::Playlist,
            })
            .collect();
        Self::new("recorded".into(), profile.name.clone(), true, rows, cancel)
    }
    fn matches(&self, index: usize) -> bool {
        let t = &self.tracks[index];
        self.search.is_empty()
            || [&t.title, &t.artist, &t.album, &t.path]
                .iter()
                .any(|s| s.to_lowercase().contains(&self.search))
    }
    pub fn filter(&mut self, query: &str) {
        self.query = query.chars().take(256).collect();
        self.search = self.query.to_lowercase();
        self.page = 0;
    }
    pub fn change_page(&mut self, page: usize) {
        self.page = page.min(self.items().len().saturating_sub(1) / PAGE);
    }

}
fn limit() -> UiError {
    failure(
        chilpa_i18n::mark("This music view exceeds 50,000 files or 16 MiB of metadata."),
        "",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn track(title: &str, artist: &str, album: &str, disc: u32, number: u32) -> Track {
        Track {
            title: title.into(),
            artist: artist.into(),
            album: album.into(),
            disc,
            number,
            path: String::new(),
            size: String::new(),
            playlist: false,
        }
    }
    #[test]
    fn oversized_snapshots_never_reach_a_frontend() {
        let too_many = (0..=MAX_ASSETS).map(|_| track("", "", "", 0, 0)).collect();
        assert!(Browser::new(
            "selection".into(),
            "Music".into(),
            false,
            too_many,
            &Cancellation::default()
        )
        .is_err());
        let too_large = vec![track(&"a".repeat(MAX_METADATA_BYTES + 1), "", "", 0, 0)];
        assert!(Browser::new(
            "selection".into(),
            "Music".into(),
            false,
            too_large,
            &Cancellation::default()
        )
        .is_err());
        let cancel = Cancellation::default();
        cancel.cancel();
        assert!(Browser::new("selection".into(), "Music".into(), false, vec![], &cancel).is_err());
    }
    #[test]
    fn albums_keep_artists_separate_and_discs_and_duplicate_occurrences_visible() {
        let mut view = Browser::new(
            "selection".into(),
            "Music".into(),
            false,
            vec![
                track("Second disc", "Artist", "Record", 2, 1),
                track("First disc", "Artist", "Record", 1, 2),
                track("First disc", "Artist", "Record", 1, 2),
                track("Other", "Another", "Record", 1, 1),
            ],
            &Cancellation::default(),
        )
        .unwrap();
        let root = view.document();
        assert_eq!(root["rows"].as_array().unwrap().len(), 2);
        let album = root["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["detail"].as_str().unwrap().starts_with("Artist"))
            .unwrap();
        view.open(album["id"].as_str().unwrap()).unwrap();
        let rows = view.document()["rows"].as_array().unwrap().clone();
        assert_eq!(
            rows.iter()
                .map(|r| r["title"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["First disc", "First disc", "Second disc"]
        );
        assert_ne!(rows[0]["id"], rows[1]["id"]);
        assert!(view.back());
        assert!(!view.back());
    }
    #[test]
    fn filtering_and_paging_are_bounded_and_reset_on_navigation() {
        let tracks = (0..450)
            .map(|i| track(&format!("Song {i:03}"), "Artist", "Record", 1, i))
            .collect();
        let mut view = Browser::new(
            "selection".into(),
            "Record".into(),
            false,
            tracks,
            &Cancellation::default(),
        )
        .unwrap();
        view.open("album:0").unwrap();
        assert_eq!(view.document()["rows"].as_array().unwrap().len(), 200);
        view.change_page(usize::MAX);
        assert_eq!(view.document()["rows"].as_array().unwrap().len(), 50);
        view.filter("Song 44");
        assert_eq!(view.document()["page"], 0);
        assert_eq!(view.document()["total"], 10);
        view.back();
        assert_eq!(view.document()["level"], "albums");
        assert!(view.open("album:9000").is_err());
    }
}
