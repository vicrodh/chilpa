//! Version selection belongs to the facade, shared by cards and details.

use crate::{Album, Chilpa};

#[derive(Clone, Debug, serde::Serialize)]
pub struct AlbumSource {
    pub key: String,
    pub label: String,
    pub icon: String,
    pub local: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cards_details_and_source_badges_share_one_persisted_choice() {
        let lib = chilpa_library::Library::in_memory().unwrap();
        let local = lib.source_id("local", "default").unwrap();
        let server = lib.source_id("server", "home").unwrap();
        lib.set_source_label(server, "Home server").unwrap();
        lib.set_source_icon(server, "qrc:/source.svg").unwrap();
        let a = lib.source_edition(local, "coda", "Coda", "Band", Some(1982)).unwrap();
        let b = lib.source_edition(server, "coda", "Coda [1994 Remaster]", "Band", Some(1982)).unwrap();
        assert_ne!(a, b, "opaque release ids belong to a source instance");
        for (album, source, bits, rate) in [(a, local, 16, 44100), (b, server, 24, 96000)] {
            lib.connection().execute("INSERT INTO tracks(source_id, external_id, title, artist, album_id, duration_ms, added_at, bit_depth, sample_rate_hz)
                VALUES (?1, '1.flac', 'Track', 'Band', ?2, 1000, 1, ?3, ?4)", rusqlite::params![source, album, bits, rate]).unwrap();
        }
        let mut api = Chilpa::new();
        api.library = Some(lib);
        assert_eq!(api.albums(20, 0).iter().map(|v| v.id).collect::<Vec<_>>(), [b]);
        assert_eq!(api.artist_albums("Band")[0].id, b);
        let versions = api.album_versions(a);
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0].id, b);
        assert!(versions[0].label.starts_with("Home server · FLAC"));
        assert!(versions[0].label.contains("1994 Remaster"));
        assert!(versions[0].label.ends_with("1 track"));
        assert_eq!(versions[0].source.icon, "qrc:/source.svg");
        assert!(!versions[0].source.local);
        assert_eq!(api.album_sources(a).len(), 2);
        assert!(api.select_album_version(b, a));
        assert_eq!(api.albums(20, 0)[0].id, a);
        assert_eq!(api.artist_albums("Band")[0].id, a);
        assert_eq!(api.album_track_rows(a).len(), 1);
        assert_eq!(api.album_detail(a).unwrap().album.track_count, 1);
        assert_eq!(api.album_paths(&[a]), ["1.flac"]);
        assert!(api.set_group_album_versions(false));
        assert_eq!(api.albums(20, 0).len(), 2);
        assert!(api.album_versions(a).is_empty());
        assert_eq!(api.album_sources(a).len(), 1);
        assert!(api.set_group_album_versions(true));
        assert_eq!(api.albums(20, 0)[0].id, a);
    }
}

impl AlbumSource {
    fn of(v: &chilpa_library::store::versions::AlbumVersion) -> Self {
        let label = if !v.source_label.is_empty() { v.source_label.clone() }
            else if v.local { chilpa_i18n::t("Local library") } else { v.source.clone() };
        Self { key: v.source.clone(), label, icon: v.source_icon.clone(), local: v.local }
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct AlbumVersion {
    pub id: i64,
    pub label: String,
    pub detail: String,
    pub source: AlbumSource,
}

fn edition(title: &str) -> String {
    for (open, close) in [('(', ')'), ('[', ']')] {
        for part in title.split(open).skip(1) {
            if let Some((annotation, _)) = part.split_once(close) {
                let lower = annotation.to_lowercase();
                if ["remaster", "deluxe", "edition", "expanded", "anniversary", "mono", "stereo", "reissue"]
                    .iter().any(|word| lower.contains(word)) { return annotation.to_string(); }
            }
        }
    }
    for separator in [" - ", " – ", " — "] {
        if let Some((_, tail)) = title.rsplit_once(separator) {
            if ["remaster", "deluxe", "edition", "expanded", "anniversary", "reissue"].iter().any(|word| tail.to_lowercase().contains(word)) {
                return tail.to_string();
            }
        }
    }
    String::new()
}

impl Chilpa {
    pub fn group_album_versions(&mut self) -> bool {
        self.library().is_none_or(|lib| lib.group_album_versions())
    }

    pub fn set_group_album_versions(&mut self, on: bool) -> bool {
        match self.library().map(|lib| lib.set_group_album_versions(on)) {
            Some(Ok(())) => true,
            Some(Err(error)) => { self.now.error = error.to_string(); false }
            None => false,
        }
    }

    pub fn selected_album_version(&mut self, id: i64) -> i64 {
        self.library().map(|lib| lib.selected_album(id)).unwrap_or(id)
    }

    pub fn select_album_version(&mut self, album: i64, version: i64) -> bool {
        match self.library().map(|lib| lib.select_album_version(album, version)) {
            Some(Ok(accepted)) => accepted,
            Some(Err(error)) => { self.now.error = error.to_string(); false }
            None => false,
        }
    }

    pub fn album_versions(&mut self, album: i64) -> Vec<AlbumVersion> {
        let Some(lib) = self.library() else { return Vec::new() };
        if !lib.group_album_versions() { return Vec::new(); }
        let versions = lib.version_catalog().map(|c| c.choices(album)).unwrap_or_default();
        versions.into_iter().map(|v| {
            let quality = crate::quality::Quality::of_file(&v.path, v.bits, v.rate).long();
            let location = std::path::Path::new(&v.location).file_name().and_then(|s| s.to_str()).unwrap_or(&v.location);
            let tracks = chilpa_i18n::tf("{} track", "{} tracks", v.tracks as i64, &[&v.tracks.to_string()]);
            let source = AlbumSource::of(&v);
            let annotation = edition(&v.title);
            let annotation = if annotation.is_empty() { edition(location) } else { annotation };
            let annotation = if annotation.is_empty() { annotation } else { format!(" - {annotation}") };
            AlbumVersion { id: v.id, label: format!("{} · {quality}{annotation} · {tracks}", source.label),
                detail: format!("{} · {location}", v.title), source }
        }).collect()
    }

    pub fn album_sources(&mut self, album: i64) -> Vec<AlbumSource> {
        let Some(lib) = self.library() else { return Vec::new() };
        let Ok(catalog) = lib.version_catalog() else { return Vec::new() };
        let versions = if lib.group_album_versions() { catalog.choices(album) }
            else { catalog.versions.get(&album).cloned().into_iter().collect() };
        let mut seen = std::collections::HashSet::new();
        versions.iter().filter(|v| seen.insert(&v.source)).map(AlbumSource::of).collect()
    }

    pub(crate) fn group_album_cards(&mut self, cards: Vec<Album>) -> Vec<Album> {
        let Some(lib) = self.library() else { return cards };
        let ids: Vec<i64> = cards.iter().map(|a| a.id).collect();
        let ids = lib.grouped_album_ids(&ids).unwrap_or(ids);
        ids.chunks(500).flat_map(|ids| lib.albums_by_ids(ids).unwrap_or_default()).map(crate::browse::album_of).collect()
    }
}
