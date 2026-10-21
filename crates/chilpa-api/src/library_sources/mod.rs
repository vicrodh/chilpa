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
mod query;
mod versions;
#[cfg(test)]
mod tests;

pub(crate) use versions::{chosen as chosen_copies, copies, dropped as dropped_copies, grouping_sources, words, Clusters, Grouping, SourceVersion,
    CHOSEN as CHOSEN_COPIES, GROUP_SOURCES};

pub use groups::AlbumGroup;
pub(crate) use facets::merge as merge_facets;
pub(crate) use merge::{snapshot as merge_snapshot, Scope};

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
    /// What the source calls its record, which the record's stars are kept under.
    pub album_identity: String,
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
    /// I keep a normalized release kind, or an empty string when unknown.
    pub release_type: String,
    /// I keep a count only when the source supplies it for this track.
    pub plays: Option<i64>,
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
    /// Its names in lower case, for the filters; set by `SourceIndex::new`.
    pub folded: Folded,
}

/// What a filter compares an item by, folded once when the index is read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Folded {
    pub release_kind: &'static str,
    pub quality: i32,
    pub genres: Vec<String>,
    pub label: String,
    pub composers: Vec<String>,
    pub artists: Vec<String>,
    pub album_title: String,
    /// Title, artists and record, where typed words are looked for.
    pub text: String,
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
    /// What each item is ordered by, aligned with `items`.
    pub keys: Vec<chilpa_library::OrderKey>,
    /// Each integrated record as a copy among the library's, by its id.
    pub versions: std::collections::HashMap<i64, SourceVersion>,
    /// Each source with something in the index, as the Sources list shows it.
    pub sources: Vec<FacetEntry>,
}

impl SourceIndex {
    /// The items with their names folded and their order keys made, once.
    pub(crate) fn new(items: Vec<SourceItem>, sources: Vec<FacetEntry>) -> Self {
        let mut items: Vec<SourceItem> = items.into_iter().map(|mut item| {
            item.folded = Folded {
                genres: folded(item.genres.iter().cloned()),
                label: item.label.to_lowercase(),
                composers: folded(item.composers.iter().cloned()),
                artists: folded(item.artists.iter().cloned()),
                album_title: item.album_title.to_lowercase(),
                text: format!("{} {} {}", item.title, item.artists.join(" "), item.album_title).to_lowercase(),
                ..Folded::default()
            };
            item
        }).collect();
        groups::classify(&mut items);
        let keys = items.iter().map(merge::key_of).collect();
        let versions = Self::versions(&items);
        Self { items, keys, versions, sources }
    }
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
        let records = catalog.integrated_records(chosen);
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
        let mut index = Self::new(items, sources);
        index.versions.extend(records.into_iter().map(|record| (record.id, record.version())));
        index
    }
}

/// Which plugins have their saved albums and which their saved tracks in the
/// library now.
pub(crate) fn integrated_now(natives: &crate::natives::Natives) -> Vec<(String, bool, bool)> {
    integrated(natives)
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
        let kept: std::collections::HashMap<(&str, &str, &str), u8> = kept.iter().map(|((source, kind, id), n)| ((source.as_str(), kind.as_str(), id.as_str()), *n)).collect();
        let mut stars = Self::default();
        for item in &index.items {
            if item.track != 0 {
                if let Some(n) = kept.get(&(item.plugin.as_str(), "track", item.identity.as_str())) { stars.tracks.insert(item.track, *n); }
            }
            if item.album != 0 && !stars.albums.contains_key(&item.album) {
                if let Some(n) = kept.get(&(item.plugin.as_str(), "album", item.album_identity.as_str())) { stars.albums.insert(item.album, *n); }
            }
        }
        stars
    }
}

/// A filter read once for a pass over the index: its names folded, its
/// lists joined with their single choices, so an item costs comparisons and
/// no allocation.
pub(crate) struct Prepared {
    off: bool,
    queries: Vec<(usize, String)>,
    years: Vec<i32>,
    genres: Vec<String>,
    labels: Vec<String>,
    composers: Vec<String>,
    artists: Vec<String>,
    albums: Vec<i64>,
    ratings: Vec<u8>,
    track_ratings: Vec<u8>,
    excluded: Vec<String>,
    qualities: Vec<String>,
    formats: Vec<String>,
    words: Vec<String>,
    /// The conditions stacked beyond the columns, and the time a relative
    /// date counts back from.
    query: Option<chilpa_library::store::query::Node>,
    now: i64,
}

fn folded(values: impl IntoIterator<Item = String>) -> Vec<String> {
    values.into_iter().map(|value| value.to_lowercase()).collect()
}

impl Prepared {
    pub(crate) fn new(f: &Filter) -> Self {
        let excluded = f.excluded_sources.iter()
            .filter_map(|key| key.strip_prefix(crate::source_opener::PROVIDER).and_then(|rest| rest.strip_prefix(':')).map(String::from)).collect();
        Self {
            off: f.performer.is_some(),
            queries: f.queries.iter().enumerate().map(|(at, q)| (at, q.trim().to_lowercase())).filter(|(at, q)| *at != 4 && !q.is_empty()).collect(),
            years: f.years.iter().copied().chain(f.year).collect(),
            genres: folded(f.genres.iter().chain(&f.genre).cloned()),
            labels: folded(f.labels.iter().cloned()),
            composers: folded(f.composers.iter().cloned()),
            artists: folded(f.artists.iter().chain(&f.artist).cloned()),
            albums: f.albums.iter().copied().chain(f.album).collect(),
            ratings: f.ratings.clone(),
            track_ratings: f.track_ratings.clone(),
            excluded,
            qualities: f.qualities.clone(),
            formats: folded(f.formats.iter().cloned()),
            words: f.text.to_lowercase().split_whitespace().map(String::from).collect(),
            query: f.query.clone(),
            now: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64),
        }
    }

    /// Whether an item passes, as the database's `WHERE` would pass a track
    /// of its own: the same columns, chosen keys and extras.
    pub(crate) fn passes(&self, item: &SourceItem, stars: &Stars) -> bool {
        if self.off { return false; }
        let names = &item.folded;
        let holds = |values: &[String], query: &str| values.iter().any(|value| value.contains(query));
        for (column, query) in &self.queries {
            let found = match column {
                0 => item.year.is_some_and(|y| y.to_string().contains(query.as_str())),
                1 => holds(&names.genres, query),
                2 => names.label.contains(query.as_str()),
                3 => holds(&names.composers, query),
                5 => holds(&names.artists, query),
                _ => names.album_title.contains(query.as_str()),
            };
            if !found { return false; }
        }
        let among = |values: &[String], chosen: &[String]| chosen.is_empty() || values.iter().any(|v| chosen.contains(v));
        if !self.years.is_empty() && !item.year.is_some_and(|y| self.years.contains(&y)) { return false; }
        if !among(&names.genres, &self.genres) || !among(std::slice::from_ref(&names.label), &self.labels)
            || !among(&names.composers, &self.composers) || !among(&names.artists, &self.artists) { return false; }
        if !self.ratings.is_empty() && !self.ratings.contains(&stars.albums.get(&item.album).copied().unwrap_or(0)) { return false; }
        if !self.track_ratings.is_empty() && !self.track_ratings.contains(&stars.tracks.get(&item.track).copied().unwrap_or(0)) { return false; }
        if !self.albums.is_empty() && !self.albums.contains(&item.record()) { return false; }
        if self.excluded.contains(&item.plugin) { return false; }
        if !self.qualities.is_empty() && !self.qualities.iter().any(|q| q == item.tier) { return false; }
        if !self.formats.is_empty() && !self.formats.iter().any(|q| q == item.format) { return false; }
        self.words.iter().all(|word| names.text.contains(word.as_str()))
            && self.query.as_ref().is_none_or(|node| query::holds(node, item, stars, self.now))
    }
}

/// The same, for one item.
#[cfg(test)]
pub(crate) fn passes(item: &SourceItem, f: &Filter, stars: &Stars) -> bool {
    Prepared::new(f).passes(item, stars)
}
