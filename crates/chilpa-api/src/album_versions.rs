//! Version selection belongs to the facade, shared by cards and details.

use crate::{Album, Chilpa};
use chilpa_i18n as i18n;

/// How a month is named in a record's release date.
const MONTH_NAMES: [&str; 12] = [
    i18n::mark("January"), i18n::mark("February"), i18n::mark("March"), i18n::mark("April"),
    i18n::mark("May"), i18n::mark("June"), i18n::mark("July"), i18n::mark("August"),
    i18n::mark("September"), i18n::mark("October"), i18n::mark("November"), i18n::mark("December"),
];

/// "April 26, 1981" when the date is whole, the year when only the year is
/// known, nothing otherwise. The month is named in the window's language
/// and the holes are month, day and year, as QBZ's release dates
/// (`qbz-text-utils/src/dates.rs`); a language that puts the day first
/// says so in its catalog ("{2} de {1} de {3}").
pub(crate) fn release_label(raw: &str) -> String {
    let raw = raw.trim();
    let whole = raw.get(..10).and_then(|head| {
        let mut parts = head.split('-');
        let year = parts.next().filter(|y| y.len() == 4)?.parse::<u32>().ok()?;
        let month = parts.next().filter(|m| m.len() == 2)?.parse::<usize>().ok()?;
        let day = parts.next().filter(|d| d.len() == 2)?.parse::<u32>().ok()?;
        ((1..=12).contains(&month) && (1..=31).contains(&day)).then_some((year, month, day))
    });
    match whole {
        Some((year, month, day)) => i18n::t_args(i18n::mark("{} {}, {}"), &[&i18n::t(MONTH_NAMES[month - 1]), &day.to_string(), &year.to_string()]),
        None => raw.get(..4).filter(|year| year.bytes().all(|b| b.is_ascii_digit())).unwrap_or_default().to_string(),
    }
}

pub struct AlbumHeading {
    pub kind: String,
    pub meta: String,
    pub discs: String,
    pub info: String,
    pub summary: String,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct AlbumSource {
    pub key: String,
    pub label: String,
    pub icon: String,
    pub local: bool,
    pub contain_art: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_release_date_says_as_much_as_it_knows() {
        assert_eq!(release_label("1981-04-26"), "April 26, 1981");
        assert_eq!(release_label("2025-11-06T00:00:00Z"), "November 6, 2025", "the time is not shown");
        assert_eq!(release_label("1999"), "1999");
        assert_eq!(release_label("1999-00-10"), "1999", "a month that is no month leaves the year");
        assert_eq!(release_label(""), "");
        assert_eq!(release_label("soon"), "");
    }

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
        assert_eq!(api.artist_release_cards("Band", false).len(), 1);
        assert!(api.set_group_artist_versions(false));
        assert_eq!(api.artist_release_cards("Band", false).len(), 2);
        assert_eq!(api.albums(20, 0).len(), 1, "the artist toggle does not change Library or Albums");
        assert!(api.set_group_artist_versions(true));
        let versions = api.album_versions(a);
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0].id, b);
        assert!(versions[0].label.starts_with("Home server · FLAC"));
        assert_eq!(versions[0].brief, "Home server · FLAC 24-96");
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
        assert_eq!(api.artist_release_cards("Band", false).len(), 1);
        assert_eq!(api.album_versions(a).len(), 2, "the selector is independent of grid grouping");
        assert!(api.set_group_artist_versions(false));
        assert_eq!(api.artist_release_cards("Band", false).len(), 2);
        assert_eq!(api.album_sources(a).len(), 1);
        assert!(api.set_group_album_versions(true));
        assert_eq!(api.albums(20, 0)[0].id, a);
    }
}

impl AlbumSource {
    fn of(v: &chilpa_library::store::versions::AlbumVersion) -> Self {
        let label = if !v.source_label.is_empty() { v.source_label.clone() }
            else if v.local { chilpa_i18n::t("Local library") } else { v.source.clone() };
        Self { key: v.source.clone(), label, icon: v.source_icon.clone(), local: v.local, contain_art: false }
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct AlbumVersion {
    pub id: i64,
    pub label: String,
    /// What the closed selector shows beside the source's icon.
    pub brief: String,
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
    /// When a record came out, as the library or its source keeps it:
    /// "1981-04-26", or only "1981".
    pub(crate) fn album_release_date(&mut self, id: i64) -> Option<String> {
        if id < 0 {
            let card = self.catalog.card(id)?;
            return card.metadata["release_date"].as_str().filter(|date| !date.trim().is_empty()).map(str::to_owned)
                .or_else(|| card.year.map(|year| year.to_string()));
        }
        self.library()?.album_release_date(id).ok().flatten()
    }

    pub fn album_heading(&mut self, detail: &crate::AlbumDetail, length: &str) -> AlbumHeading {
        let songs = self.tf("{} song", "{} songs", detail.album.track_count as i64, &[&detail.album.track_count.to_string()]);
        let mut meta = Vec::new();
        if !detail.genre.is_empty() { meta.push(detail.genre.clone()); }
        if !detail.label.is_empty() { meta.push(detail.label.clone()); }
        meta.extend([songs.clone(), length.to_owned()]);
        let quality = detail.quality.long();
        if !quality.is_empty() { meta.push(quality); }
        let remote = detail.album.id < 0;
        let kind = if remote { self.source_album_kind(detail.album.id) } else { self.t("Album").to_uppercase() };
        let released = self.album_release_date(detail.album.id).map(|raw| release_label(&raw)).filter(|label| !label.is_empty())
            .or_else(|| detail.album.year.map(|year| year.to_string()));
        AlbumHeading {
            kind: released.map_or(kind.clone(), |released| format!("{kind} · {released}")),
            meta: meta.join(" · "),
            discs: if detail.discs > 1 { self.tf("{} disc", "{} discs", detail.discs as i64, &[&detail.discs.to_string()]) } else { String::new() },
            info: if remote { self.source_album_info(detail.album.id) } else { self.t("Info: Embedded tags") },
            summary: format!("{songs} · {length}"),
        }
    }

    pub fn group_artist_versions(&mut self) -> bool {
        self.library().is_none_or(|lib| lib.group_artist_versions())
    }

    pub fn set_group_artist_versions(&mut self, on: bool) -> bool {
        self.invalidate_artist_collections();
        match self.library().map(|lib| lib.set_group_artist_versions(on)) {
            Some(Ok(())) => true,
            Some(Err(error)) => { self.now.error = error.to_string(); false }
            None => false,
        }
    }

    /// I apply the artist page's preference without changing other grids.
    pub fn artist_release_cards(&mut self, name: &str, appearances: bool) -> Vec<Album> {
        let Some(lib) = self.library() else { return Vec::new() };
        crate::artist_library::releases(lib, name, appearances)
    }

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
        if id < 0 {
            return id;
        }
        self.library().map(|lib| lib.selected_album(id)).unwrap_or(id)
    }

    pub fn select_album_version(&mut self, album: i64, version: i64) -> bool {
        if album < 0 || version < 0 {
            return false;
        }
        match self.library().map(|lib| lib.select_album_version(album, version)) {
            Some(Ok(accepted)) => accepted,
            Some(Err(error)) => { self.now.error = error.to_string(); false }
            None => false,
        }
    }

    pub fn album_versions(&mut self, album: i64) -> Vec<AlbumVersion> {
        if album < 0 {
            return self.source_album_versions(album);
        }
        let Some(lib) = self.library() else { return Vec::new() };
        let versions = lib.version_catalog().map(|c| c.choices(album)).unwrap_or_default();
        versions.into_iter().map(|v| {
            // A server's version has an id for a path: its codec says the format.
            let quality = if v.local { crate::quality::Quality::of_file(&v.path, v.bits, v.rate) }
                else { crate::quality::Quality::of_codec(&v.codec, v.bits, v.rate) };
            let (quality, brief) = (quality.long(), quality.brief());
            let location = std::path::Path::new(&v.location).file_name().and_then(|s| s.to_str()).unwrap_or(&v.location);
            let tracks = chilpa_i18n::tf("{} track", "{} tracks", v.tracks as i64, &[&v.tracks.to_string()]);
            let source = AlbumSource::of(&v);
            let annotation = edition(&v.title);
            let annotation = if annotation.is_empty() { edition(location) } else { annotation };
            let annotation = if annotation.is_empty() { annotation } else { format!(" - {annotation}") };
            AlbumVersion { id: v.id, brief: format!("{} · {brief}", source.label), label: format!("{} · {quality}{annotation} · {tracks}", source.label),
                detail: format!("{} · {location}", v.title), source }
        }).collect()
    }

    pub fn album_sources(&mut self, album: i64) -> Vec<AlbumSource> {
        if album < 0 {
            return self.source_album_sources(album);
        }
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
