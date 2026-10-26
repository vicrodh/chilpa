//! I retain one bounded metadata snapshot and publish at most 200 rows per navigation step.
use super::{
    catalog::{Pick, Profile, Store},
    failure, library, UiError,
};
use chilpa_sync::{AssetKind, Cancellation, State, MAX_ASSETS, MAX_METADATA_BYTES};
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path};

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
struct Album {
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
        let mut groups = BTreeMap::<(String, String), Vec<usize>>::new();
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
                .entry((track.artist.clone(), title))
                .or_default()
                .push(index);
        }
        let albums = groups
            .into_iter()
            .map(|((artist, title), mut indices)| {
                indices.sort_by_key(|i| (tracks[*i].disc, tracks[*i].number, *i));
                Album {
                    title,
                    artist,
                    tracks: indices,
                }
            })
            .collect();
        Ok(Self {
            source,
            title,
            recorded,
            tracks,
            albums,
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
    fn visible(&self) -> Vec<usize> {
        if let Some(album) = self.album {
            self.albums[album]
                .tracks
                .iter()
                .copied()
                .filter(|i| self.matches(*i))
                .collect()
        } else {
            self.albums
                .iter()
                .enumerate()
                .filter(|(_, a)| a.tracks.iter().any(|i| self.matches(*i)))
                .map(|(i, _)| i)
                .collect()
        }
    }
    pub fn open(&mut self, id: &str) -> Result<(), UiError> {
        let index = id
            .strip_prefix("album:")
            .and_then(|i| i.parse::<usize>().ok());
        let index = index
            .filter(|i| *i < self.albums.len() && self.album.is_none())
            .ok_or_else(|| {
                failure(
                    chilpa_i18n::mark("This album is no longer in the current view."),
                    "",
                )
            })?;
        self.album = Some(index);
        self.parent = Some((std::mem::take(&mut self.query), self.page));
        self.search.clear();
        self.page = 0;
        Ok(())
    }
    /// I return false at the selection root so its owner can return to the picker.
    pub fn back(&mut self) -> bool {
        if self.album.take().is_none() {
            return false;
        }
        let (query, page) = self.parent.take().unwrap_or_default();
        self.search = query.to_lowercase();
        self.query = query;
        self.page = page;
        true
    }
    pub fn filter(&mut self, query: &str) {
        self.query = query.chars().take(256).collect();
        self.search = self.query.to_lowercase();
        self.page = 0;
    }
    pub fn change_page(&mut self, page: usize) {
        self.page = page.min(self.visible().len().saturating_sub(1) / PAGE);
    }
    pub fn document(&self) -> Value {
        let indices = self.visible();
        let pages = indices.len().div_ceil(PAGE).max(1);
        let page = self.page.min(pages - 1);
        let rows: Vec<_> = indices.iter().skip(page * PAGE).take(PAGE).map(|i| {
            if self.album.is_none() {
                let album = &self.albums[*i];
                let count = chilpa_i18n::tf("{} track", "{} tracks", album.tracks.len() as i64, &[&album.tracks.len().to_string()]);
                let detail = [album.artist.clone(),count].into_iter().filter(|s|!s.is_empty()).collect::<Vec<_>>().join(" · ");
                json!({"id":format!("album:{i}"),"title":album.title,"detail":detail,"kind":"album","open":true})
            } else {
                let track = &self.tracks[*i];
                let disc = if track.disc > 0 { chilpa_i18n::t_args("Disc {}", &[&track.disc.to_string()]) } else { String::new() };
                let detail = [track.artist.clone(),disc,track.size.clone()].into_iter().filter(|s|!s.is_empty()).collect::<Vec<_>>().join(" · ");
                json!({"id":format!("track:{i}"),"title":track.title,"detail":detail,
                    "path":track.path,"number":track.number,"kind":if track.playlist{"playlist"}else{"track"},"open":false})
            }
        }).collect();
        json!({"source":self.source,"recorded":self.recorded,"title":self.title,"album":self.album.map(|i|&self.albums[i].title),
            "can_back":self.album.is_some() || !self.recorded,"rows":rows,"page":page,"pages":pages,"total":indices.len(),
            "level":if self.album.is_some(){"tracks"}else{"albums"},"query":self.query,
            "notice":if self.recorded {chilpa_i18n::t("Last synced with Chilpa. Files changed elsewhere may differ.")}else{String::new()}})
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
