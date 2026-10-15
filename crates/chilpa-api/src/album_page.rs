//! I read a record once and share its committed header and track rows.

use crate::{AlbumDetail, AlbumVersion, Chilpa, Reader, TrackEntry, TrackRow};

#[derive(Clone, Debug)]
pub struct AlbumPage {
    pub detail: AlbumDetail,
    pub heading: crate::album_versions::AlbumHeading,
    pub versions: Vec<AlbumVersion>,
    pub artist_information: String,
    pub entries: Vec<TrackEntry>,
}

impl Reader {
    /// I resolve the preferred edition and every part of its page together.
    /// A filtered grid can explicitly retain the edition that was clicked.
    pub fn album_page(&self, album: i64, retain_edition: bool) -> Option<AlbumPage> {
        if album <= 0 { return None; }
        self.lib.read_snapshot(|| {
            let id = if retain_edition { album } else { self.lib.selected_album(album) };
            let album = crate::browse::album_of(self.lib.album(id).ok()??);
            let filter = crate::Filter { album: Some(id), ..Default::default() };
            let rows: Vec<_> = self.lib.browse_tracks(&filter, crate::TrackOrder::Disc, 10_000).ok()?.into_iter().map(crate::browse::row_of).collect();
            let genre = self.lib.album_genre(id).ok().flatten().unwrap_or_default();
            let label = self.lib.album_label(id).ok().flatten().unwrap_or_default();
            let detail = detail(album, genre, label, &rows, None);
            let heading = crate::album_versions::heading(&detail, &crate::format::span(detail.duration_ms),
                self.lib.album_release_date(id).ok().flatten(), chilpa_i18n::t("Album").to_uppercase(), chilpa_i18n::t("Info: Embedded tags"));
            let info = serde_json::json!({"track_credits": rows.iter().map(|row| serde_json::json!({"artist":row.artist})).collect::<Vec<_>>()});
            let artist_information = crate::album_information::artist_document(&detail.album.artist, &info, &rows,
                |credit| self.lib.credit_names(credit).unwrap_or_else(|_| vec![credit.to_string()]),
                |path| self.catalog.as_ref().and_then(|catalog| catalog.track_by_path(path)).map(|(track, _)| track.metadata).unwrap_or(serde_json::Value::Null)).to_string();
            let versions = crate::album_versions::version_labels(self.lib.version_catalog().ok()?.choices(id));
            let discs = if crate::discs::several(&rows) { self.lib.album_discs(id).ok()? } else { Vec::new() };
            let entries = entries(rows, discs, |name| self.lib.has_artist(name).unwrap_or(false));
            Some(AlbumPage { detail, heading, versions, entries, artist_information })
        }).map_err(|error| eprintln!("chilpa-queries: album page failed: {error}")).ok().flatten()
    }

    /// I validate and persist the edition on a writer owned by this worker.
    pub fn select_album_version(&self, album: i64, version: i64) -> bool {
        self.lib.remember_album_version(album, version)
            .map_err(|error| eprintln!("chilpa-queries: album selection failed: {error}")).unwrap_or(false)
    }
}

impl Chilpa {
    /// Source cards already live in the shared in-memory catalog.
    pub fn cached_album_page(&mut self, id: i64) -> Option<AlbumPage> {
        if id >= 0 { return None; }
        let card = self.catalog.card(id)?;
        let rows = self.source_album_rows(id);
        let detail = detail(crate::source_catalog::view::album_of(&card), card.genre.clone(), String::new(), &rows,
            card.format.as_ref().map(|format| format.quality()));
        let heading = self.album_heading(&detail, &crate::format::span(detail.duration_ms));
        let versions = self.source_album_versions_for(&detail);
        let mut info = card.metadata.clone();
        if !info.is_object() { info = serde_json::json!({}); }
        info["plugin"] = serde_json::json!(card.plugin);
        if info["track_credits"].as_array().is_none_or(Vec::is_empty) {
            info["track_credits"] = serde_json::json!(rows.iter().map(|row| serde_json::json!({"artist":row.artist})).collect::<Vec<_>>());
        }
        let artist_information = self.album_artist_document(&detail.album.artist, &info, &rows).to_string();
        Some(AlbumPage { detail, heading, versions, artist_information, entries: entries(rows, Vec::new(), |_| false) })
    }
}

fn detail(album: crate::Album, genre: String, label: String, rows: &[TrackRow], fallback: Option<crate::Quality>) -> AlbumDetail {
    let discs: std::collections::HashSet<_> = rows.iter().map(|row| row.disc.max(1)).collect();
    let quality = rows.iter().map(|row| &row.quality).max_by_key(|quality| (quality.tier.rank(), quality.bits, quality.rate_hz))
        .cloned().or(fallback).unwrap_or_default();
    AlbumDetail { album, genre, label, discs: discs.len().max(1), quality, duration_ms: rows.iter().map(|row| row.duration_ms).sum() }
}

pub(crate) fn entries(rows: Vec<TrackRow>, said: Vec<chilpa_library::AlbumDisc>, mut has_artist: impl FnMut(&str) -> bool) -> Vec<TrackEntry> {
    use crate::discs::{named, only_distinct_art, several, with_headers, Disc};
    if rows.is_empty() { return Vec::new(); }
    let multi = several(&rows);
    let mut discs: Vec<_> = said.iter().map(|disc| Disc {
        number: disc.number, title: named(&disc.path, &disc.subtitle, &disc.tagged, &rows[0].album), art: disc.artwork.clone(),
    }).collect();
    only_distinct_art(&mut discs);
    let mut entries = with_headers(rows, &discs, multi);
    let mut composers = std::collections::HashMap::new();
    for entry in &mut entries {
        if let TrackEntry::Header(header) = entry {
            if header.work && !header.composer.is_empty() && header.composer_target.is_empty() {
                if *composers.entry(header.composer.clone()).or_insert_with(|| has_artist(&header.composer)) {
                    header.composer_target = format!("artist:{}", header.composer);
                }
            }
        }
    }
    entries
}

#[cfg(test)]
mod tests;
