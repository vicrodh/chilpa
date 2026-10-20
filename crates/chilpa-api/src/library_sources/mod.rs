//! A source's saved records and tracks on the library's own screens.
//!
//! What the listener integrated -- a source's saved albums into Albums, its
//! saved tracks into Tracks -- is read from the catalog into a compact index
//! once per catalog revision. Library, Albums and Tracks filter it with the
//! same `Filter` the database answers, cascade its facets column by column as
//! the database does, and merge it into their snapshots by the keys the
//! snapshot carries. Nothing here asks a plugin: a stale listing is queued
//! with the catalog, and what arrives shows on the next read.
//!
//! Bounded by what the catalog keeps (6,000 records, 30,000 tracks); the
//! index lives in the worker's reader and is replaced, never grown.

mod facets;
mod groups;
mod merge;
#[cfg(test)]
mod tests;

pub use groups::AlbumGroup;
pub(crate) use facets::merge as merge_facets;
pub(crate) use merge::{letters as merged_letters, snapshot as merge_snapshot, Scope};

use crate::{FacetEntry, Filter};

/// One record or track a source keeps for the listener, as a filter reads it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SourceItem {
    /// The catalog's track id, or zero for a record whose tracks are not read.
    pub track: i64,
    /// The catalog's record id, or zero when the track names none.
    pub album: i64,
    pub plugin: String,
    /// What the source calls it, which its stars are kept under.
    pub identity: String,
    pub album_artist: String,
    pub album_title: String,
    pub title: String,
    /// Who plays it: the track's artist and its record's.
    pub artists: Vec<String>,
    pub year: Option<i32>,
    pub genres: Vec<String>,
    pub label: String,
    pub composers: Vec<String>,
    /// When the listener saved it, in Unix seconds; zero when unknown.
    pub added_at: i64,
    /// "album", "ep", "single", "compilation", "live" or empty.
    pub release_type: String,
    pub disc: Option<i64>,
    pub number: Option<i64>,
    pub duration_ms: u64,
    /// "lossy", "lossless", "hires", "dsd" or "unknown".
    pub tier: &'static str,
    /// The library's format key ("flac", "ogg", ...), empty when unknown.
    pub format: &'static str,
    /// In the source's saved tracks: Tracks shows it.
    pub saved_track: bool,
    /// On one of the source's saved records: Albums shows that record.
    pub saved_album: bool,
    /// How many tracks it stands for: one, or a record's count.
    pub tracks: usize,
    pub playable: bool,
}

impl SourceItem {
    pub(crate) fn source_key(&self) -> String {
        format!("{}:{}", crate::source_opener::PROVIDER, self.plugin)
    }

    /// The record it counts for in a facet: its own, or itself.
    fn record(&self) -> i64 {
        if self.album != 0 { self.album } else { self.track }
    }
}

/// The integrated items and the sources they come from.
#[derive(Debug, Default)]
pub(crate) struct SourceIndex {
    pub items: Vec<SourceItem>,
    /// Each source with something in the index, as the Sources list shows it.
    pub sources: Vec<FacetEntry>,
}

/// What the integrated listings hold and what is integrated from which
/// plugin: while both hold, the index holds.
pub(crate) type Stamp = (u64, Vec<(String, bool, bool)>);

/// Which plugins have their saved albums and which their saved tracks in the
/// library, by the listener's choice and what each plugin lists.
fn integrated(natives: &crate::natives::Natives) -> Vec<(String, bool, bool)> {
    let root = crate::Chilpa::plugins_dir();
    natives.ids().into_iter().filter_map(|id| {
        let kinds = natives.library_kinds(&id);
        let chosen = crate::source_integration::read(&root, &id);
        let albums = chosen.integrate_albums && kinds.iter().any(|k| k == "albums");
        let tracks = chosen.integrate_tracks && kinds.iter().any(|k| k == "tracks");
        (albums || tracks).then_some((id, albums, tracks))
    }).collect()
}

impl crate::Reader {
    /// The index, read again only when an integrated listing moved or the
    /// listener changed what is integrated. Nothing integrated is one stamp
    /// whatever the catalog does, so the refreshes it causes are answered
    /// from the cache.
    pub(crate) fn source_index(&self) -> (Stamp, std::rc::Rc<SourceIndex>) {
        let Some(catalog) = &self.catalog else { return Default::default() };
        let Some(natives) = catalog.natives() else { return Default::default() };
        let chosen = integrated(&natives);
        if chosen.is_empty() { return Default::default(); }
        let stamp = (catalog.integrated_signature(&chosen), chosen);
        if let Some((held, index)) = self.sources.borrow().as_ref().filter(|(held, _)| *held == stamp) {
            return (held.clone(), index.clone());
        }
        let index = std::rc::Rc::new(SourceIndex::read(catalog, &natives, &stamp.1));
        *self.sources.borrow_mut() = Some((stamp.clone(), index.clone()));
        (stamp, index)
    }
}

impl SourceIndex {
    /// The integrated listings of those plugins, queued for a read when they
    /// are stale.
    pub(crate) fn read(catalog: &crate::source_catalog::Catalog, natives: &crate::natives::Natives, chosen: &[(String, bool, bool)]) -> Self {
        let root = crate::Chilpa::plugins_dir();
        let albums: Vec<String> = chosen.iter().filter(|(_, albums, _)| *albums).map(|(id, ..)| id.clone()).collect();
        let tracks: Vec<String> = chosen.iter().filter(|(.., tracks)| *tracks).map(|(id, ..)| id.clone()).collect();
        for key in albums.iter().map(|id| format!("{id} section:albums")).chain(tracks.iter().map(|id| format!("{id} section:tracks"))) {
            catalog.shelf(&key);
        }
        let items = catalog.integrated_items(&albums, &tracks);
        let mut sources: Vec<FacetEntry> = Vec::new();
        for item in &items {
            let key = item.source_key();
            match sources.iter_mut().find(|entry| entry.key == key) {
                Some(entry) => entry.albums += item.tracks,
                None => sources.push(FacetEntry {
                    key, albums: item.tracks, year: None,
                    label: catalog.name(&item.plugin).unwrap_or_else(|| natives.display_name(&item.plugin)),
                    icon: crate::plugin_icons::image_url(&root, &item.plugin),
                }),
            }
        }
        Self { items, sources }
    }
}

/// What a refresh reads the index with: its stars, by catalog id.
#[derive(Default)]
pub(crate) struct Stars {
    pub albums: std::collections::HashMap<i64, u8>,
    pub tracks: std::collections::HashMap<i64, u8>,
}

impl Stars {
    pub(crate) fn read(lib: &chilpa_library::Library, index: &SourceIndex) -> Self {
        let kept = lib.external_ratings().unwrap_or_default();
        if kept.is_empty() { return Self::default(); }
        let mut stars = Self::default();
        for item in &index.items {
            if item.track != 0 {
                if let Some(n) = kept.get(&(item.plugin.clone(), "track".into(), item.identity.clone())) { stars.tracks.insert(item.track, *n); }
            } else if let Some(n) = kept.get(&(item.plugin.clone(), "album".into(), item.identity.clone())) {
                stars.albums.insert(item.album, *n);
            }
        }
        stars
    }
}

fn holds(values: &[String], query: &str) -> bool {
    values.iter().any(|value| value.to_lowercase().contains(query))
}

fn among(values: &[String], chosen: &[String]) -> bool {
    values.iter().any(|value| chosen.iter().any(|c| c.to_lowercase() == value.to_lowercase()))
}

/// Whether an item passes a filter as the database's `WHERE` would pass a
/// track of its own: the same columns, chosen keys and extras.
pub(crate) fn passes(item: &SourceItem, f: &Filter, stars: &Stars) -> bool {
    if f.performer.is_some() { return false; }
    for (column, query) in f.queries.iter().enumerate() {
        let query = query.trim().to_lowercase();
        if query.is_empty() { continue; }
        let found = match column {
            0 => item.year.is_some_and(|y| y.to_string().contains(&query)),
            1 => holds(&item.genres, &query),
            2 => item.label.to_lowercase().contains(&query),
            3 => holds(&item.composers, &query),
            4 => true,
            5 => holds(&item.artists, &query),
            _ => item.album_title.to_lowercase().contains(&query),
        };
        if !found { return false; }
    }
    let mut years = f.years.clone();
    years.extend(f.year);
    if !years.is_empty() && !item.year.is_some_and(|y| years.contains(&y)) { return false; }
    let mut genres = f.genres.clone();
    genres.extend(f.genre.clone());
    if !genres.is_empty() && !among(&item.genres, &genres) { return false; }
    if !f.labels.is_empty() && !f.labels.iter().any(|l| l.to_lowercase() == item.label.to_lowercase()) { return false; }
    if !f.composers.is_empty() && !among(&item.composers, &f.composers) { return false; }
    let album_stars = || stars.albums.get(&item.album).copied().unwrap_or(0);
    if !f.ratings.is_empty() && !f.ratings.contains(&album_stars()) { return false; }
    if !f.track_ratings.is_empty() && !f.track_ratings.contains(&stars.tracks.get(&item.track).copied().unwrap_or(0)) { return false; }
    let mut artists = f.artists.clone();
    artists.extend(f.artist.clone());
    if !artists.is_empty() && !among(&item.artists, &artists) { return false; }
    let mut albums = f.albums.clone();
    albums.extend(f.album);
    if !albums.is_empty() && !albums.contains(&item.record()) { return false; }
    if f.excluded_sources.contains(&item.source_key()) { return false; }
    if !f.qualities.is_empty() && !f.qualities.iter().any(|q| q == item.tier) { return false; }
    if !f.formats.is_empty() && !f.formats.iter().any(|q| q.eq_ignore_ascii_case(item.format)) { return false; }
    let words: Vec<String> = f.text.to_lowercase().split_whitespace().map(String::from).collect();
    if !words.is_empty() {
        let text = format!("{} {} {}", item.title, item.artists.join(" "), item.album_title).to_lowercase();
        if !words.iter().all(|word| text.contains(word.as_str())) { return false; }
    }
    true
}
