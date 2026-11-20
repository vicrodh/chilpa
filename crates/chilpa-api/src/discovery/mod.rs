//! Library discovery queries, run by the window's shared read-only worker.
//! Pages contain at most 60 small navigation entries. No additional network
//! worker/cache is retained; existing source refresh policy still applies.
//! Hidden screens submit no queries.
mod genres;
mod genre_summary;
mod album_population;
mod works;
mod options;
mod sources;
mod presentation;
mod chosen_art;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod measurement;
#[cfg(test)]
mod source_tests;
#[cfg(test)]
mod album_genres_tests;
#[cfg(test)]
mod presentation_tests;
#[cfg(test)]
mod album_population_tests;
use crate::{AlbumsFound, Filter, Reader, Snapshot, TrackOrder};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};

const PAGE: usize = 60;

pub(crate) fn genre_identity(name: &str) -> String { genres::strong_identity(name) }

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Selection {
    pub kind: String,
    pub name: String,
    pub work: String,
    pub trail: Vec<String>,
    pub composers: Vec<String>,
    pub recording: String,
    #[serde(rename = "recordingTitle")]
    pub recording_title: String,
    pub performer: String,
    pub album: i64,
    #[serde(rename = "genreToken", skip_serializing_if = "String::is_empty")]
    pub genre_token: String,
    #[serde(rename = "genreId")]
    pub genre_id: i64,
    #[serde(rename = "genreTrack")]
    pub genre_track: i64,
    #[serde(rename = "albumScope")]
    pub album_scope: bool,
    pub excluded_sources: Vec<String>,
}

impl Selection {
    pub fn parse(key: &str) -> Option<Self> {
        (key.len() <= 65536).then(|| serde_json::from_str::<Self>(key).ok()).flatten()
            .filter(|selected| options::valid_sources(&selected.excluded_sources))
            .filter(Self::valid_genre_anchor)
    }

    fn valid_genre_anchor(&self) -> bool {
        if self.genre_token.is_empty() {
            return !self.album_scope && self.genre_id == 0 && self.genre_track == 0;
        }
        let anchors = usize::from(self.album != 0) + usize::from(self.genre_id > 0) + usize::from(self.genre_track < 0);
        self.kind == "genre" && anchors == 1 && self.genre_id >= 0 && self.genre_track <= 0
            && (!self.album_scope || self.album != 0) && self.genre_token.len() == 64
            && self.genre_token.bytes().all(|byte|byte.is_ascii_hexdigit())
    }
}

/// Separates a discovery selection from the Tracks query whose column
/// selections narrow its tracks: `selection` U+001E `query`.
pub(crate) const SHEET: char = '\u{1e}';

impl Reader {
    pub fn collection_tracks(&self, kind: &str, key: &str, extras: &Filter, mut emit: impl FnMut(crate::FoundTracks)) {
        if kind == "discovery" { emit(crate::FoundTracks { snapshot: self.discovery_snapshot_with(key, extras), ..Default::default() }); }
        else { self.tracks_progressive(key, extras, emit); }
    }

    /// A genre's or composer's library filter, narrowed by a Tracks query's
    /// column selections. The tracks keep the records' order and the
    /// selection's own sources; the Library's shared scope does not apply.
    fn discovery_scope_using(&self, selected: &Selection, sheet: &str, extras: &Filter, index: &crate::library_sources::SourceIndex) -> Option<Filter> {
        if !matches!(selected.kind.as_str(), "genre" | "composer") || selected.name.is_empty() { return None; }
        let mut filter = self.discovery_filter_using(selected, index);
        if selected.kind == "genre" && filter.genres.is_empty() { return None; }
        if !sheet.is_empty() { filter.sheet = crate::TracksQuery::parse(sheet).filter(extras).sheet; }
        Some(filter)
    }

    /// The filter a Tracks query names, or the one of a discovery selection
    /// narrowed by it. `None` when the selection names nothing.
    pub(crate) fn sheet_filter(&self, query: &str, extras: &Filter, index: &crate::library_sources::SourceIndex) -> Option<Filter> {
        let Some((key, sheet)) = query.split_once(SHEET) else { return Some(crate::TracksQuery::parse(query).filter(extras)) };
        let selected = self.resolve_album_genre_using(Selection::parse(key)?, index).ok()?;
        self.discovery_scope_using(&selected, sheet, extras, index)
    }

    pub fn collection_albums(&self, kind: &str, order: &str, group: &str, text: &str, extras: &Filter, scope: &str) -> AlbumsFound {
        if kind == "discovery" { self.discovery_albums_ordered(scope, text, order) }
        else { self.albums_view_scoped(order, group, text, extras, scope) }
    }

    fn discovery_names(&self, composers: bool) -> Vec<(String, usize)> {
        self.discovery_names_using(composers, &[], &self.source_index().1)
    }

    fn discovery_names_using(&self, composers: bool, excluded: &[String], index: &crate::library_sources::SourceIndex) -> Vec<(String, usize)> {
        self.discovery_names_checked(composers, excluded, index).unwrap_or_default()
    }

    fn discovery_names_checked(&self, composers: bool, excluded: &[String], index: &crate::library_sources::SourceIndex) -> Result<Vec<(String, usize)>, String> {
        // Genre spellings remain separate inputs to the derived index. SQL's
        // NOCASE does not equate every Unicode case variant when filtering.
        let key = |name: &str| if composers { name.to_lowercase() } else { name.to_string() };
        let local: Vec<_> = if composers { self.lib.facet_composers(&Filter::default()).map_err(|error|error.to_string())?.into_iter().map(|row| (row.label,row.albums)).collect() }
            else { self.lib.genre_names_excluding(excluded).map_err(|error|error.to_string())?.into_iter().map(|name| (name,0)).collect() };
        let mut names: BTreeMap<String, (String, usize)> = local.into_iter()
            .map(|(name, albums)| (key(&name), (name, albums))).collect();
        let through = sources::through(excluded);
        let stars = crate::library_sources::Stars::default();
        let mut counted = HashSet::new();
        for item in index.items.iter().filter(|item| through.passes(item, &stars)) {
            let tags = if composers { &item.composers } else { &item.genres };
            for tag in tags.iter().filter(|tag| !tag.trim().is_empty()) {
                let key = key(tag);
                if !composers { names.entry(key).or_insert_with(|| (tag.clone(),0)); continue; }
                if counted.insert((key.clone(), if item.album != 0 { item.album } else { item.track })) {
                    names.entry(key).or_insert_with(|| (tag.clone(), 0)).1 += 1;
                }
            }
        }
        Ok(names.into_values().collect())
    }

    #[cfg(test)]
    fn discovery_filter(&self, selected: &Selection) -> Filter {
        self.discovery_filter_using(selected, &self.source_index().1)
    }

    fn discovery_filter_using(&self, selected: &Selection, index: &crate::library_sources::SourceIndex) -> Filter {
        match selected.kind.as_str() {
            "genre" => Filter { genres: genres::Inventory::new(
                &self.discovery_names_using(false, &selected.excluded_sources, index).into_iter().map(|(name, _)| name).collect::<Vec<_>>()).matching(&selected.name), excluded_sources:selected.excluded_sources.clone(), ..Default::default() },
            "composer" => Filter { composers: vec![selected.name.clone()], ..Default::default() },
            _ => Filter::default(),
        }
    }

    /// The snapshot keeps all parts, including albumless tracks. Existing
    /// source integration and copy grouping apply to genre/composer journeys.
    pub fn discovery_snapshot(&self, key: &str) -> Snapshot {
        self.discovery_snapshot_with(key, &Filter::default())
    }

    /// The same, narrowed by the column selections of a Tracks query that
    /// follows the selection after U+001E.
    pub fn discovery_snapshot_with(&self, key: &str, extras: &Filter) -> Snapshot {
        let (key, sheet) = key.split_once(SHEET).unwrap_or((key, ""));
        let Some(selected) = Selection::parse(key) else { return Snapshot::default() };
        let selected = if selected.genre_token.is_empty() { selected } else {
            let Ok(selected) = self.resolve_album_genre_using(selected,&self.source_index().1) else { return Snapshot::default() };
            selected
        };
        if selected.kind == "work" { return self.work_snapshot(&selected); }
        let Some(filter) = self.discovery_scope_using(&selected, sheet, extras, &self.source_index().1) else { return Snapshot::default() };
        self.refresh(&filter, TrackOrder::Album, || true).map(|(_, snapshot)| snapshot).unwrap_or_default()
    }

    pub fn discovery_artists(&self, key: &str) -> Vec<crate::ListedArtist> {
        let selected = Selection::parse(key).unwrap_or_default();
        let Ok(selected) = self.resolve_album_genre_using(selected,&self.source_index().1) else { return Vec::new() };
        let extras = Filter { excluded_sources:if selected.kind == "genre" { selected.excluded_sources } else { Vec::new() }, ..Default::default() };
        let ids: HashSet<_> = self.discovery_snapshot(key).album_ids.into_iter().collect();
        let pictures = self.lib.artist_pictures().unwrap_or_default();
        let local = self.lib.album_artists_excluding(&ids, &extras.excluded_sources).unwrap_or_default()
            .into_iter().map(|artist| crate::browse::artist_of(artist, &pictures)).collect();
        let (_, source) = self.source_index();
        crate::albums_view::artists::listed_within(local, &source, &extras, false, Some(&ids),
            |id| self.catalog.as_ref().and_then(|catalog| catalog.card(id)).map(|card| card.image).unwrap_or_default())
    }

    pub fn discovery_albums(&self, key: &str, text: &str) -> AlbumsFound {
        self.discovery_albums_ordered(key, text, "title")
    }

    fn discovery_albums_ordered(&self, key: &str, text: &str, order: &str) -> AlbumsFound {
        let within: HashSet<_> = self.discovery_snapshot(key).album_ids.into_iter().collect();
        if within.is_empty() { return AlbumsFound::default(); }
        self.albums_view_within(order, text, &within, &self.source_index().1)
    }

    fn genre_plugin_page(&self, key: &str, selected: &Selection, page: usize) -> Value {
        let empty = || json!({"items": [], "total": 0, "plugins": []});
        if page != 0 || selected.kind != "genre" || selected.name.trim().is_empty() { return empty(); }
        let Some(catalog) = &self.catalog else { return empty(); };
        let plugins = catalog.genre_plugin_cards(&selected.name);
        let in_library = if plugins.iter().any(|plugin| !plugin.artists.is_empty()) {
            self.discovery_artists(key).into_iter().map(|entry| chilpa_library::store::query::fold(&entry.artist.name)).collect()
        } else { HashSet::new() };
        let rows = plugins.into_iter().filter_map(|plugin| plugin.rails(&in_library)).collect::<Vec<_>>();
        json!({"items": [], "total": 0, "plugins": rows})
    }

    /// JSON for a virtualized page. Search examines the full inventory before
    /// pagination; a genre search can jump directly to a deeply nested style.
    pub fn discovery_page(&self, kind: &str, key: &str, text: &str, page: usize) -> Value {
        self.discovery_page_options(kind, key, text, "{}", page)
    }

    pub fn discovery_page_options(&self, kind: &str, key: &str, text: &str, options: &str, page: usize) -> Value {
        let Some(options) = options::Options::parse(options) else { return json!({"error":chilpa_i18n::t("Invalid discovery options."), "items":[], "total":0}); };
        if kind == "album_genres" && (key.len() > 32 || text.len() > 1024) {
            return json!({"error":chilpa_i18n::t("Invalid discovery selection."),"items":[],"total":0});
        }
        let text = chilpa_library::store::query::fold(text.trim());
        if kind == "album_genres" {
            return match key.parse::<i64>() {
                Ok(album) if album != 0 => self.album_genres_page_using(album,&text,page,PAGE,options.descending(),&self.source_index().1),
                _ => json!({"error":chilpa_i18n::t("Invalid discovery selection."),"items":[],"total":0}),
            };
        }
        let selected = if key.is_empty() { Selection::default() } else {
            let Some(selected) = Selection::parse(key) else { return json!({"error":chilpa_i18n::t("Invalid discovery selection."), "items":[], "total":0}); };
            selected
        };
        let (_, index) = self.source_index();
        let selected = match self.resolve_album_genre_using(selected,&index) {
            Ok(selected) => selected,
            Err(error) => return json!({"error":error,"items":[],"total":0}),
        };
        if kind == "genre_plugins" { return self.genre_plugin_page(key, &selected, page); }
        if kind == "recordings" { return self.recordings_page_ordered(&selected, &text, page, options.descending(), &index); }
        if kind == "compositions" || selected.kind == "composer" {
            let composer = if selected.kind == "composer" { selected.name.as_str() } else { "" };
            return self.compositions_page(composer, &text, page, options.descending(), &index);
        }
        if kind == "composers" {
            let mut names: Vec<_> = self.discovery_names(true).into_iter()
                .filter(|(name, _)| chilpa_library::store::query::fold(name).contains(&text)).collect();
            names.sort_by_cached_key(|(name, albums)| (if options.order == "albums" { std::cmp::Reverse(*albums) } else { std::cmp::Reverse(0) }, name.to_lowercase(), name.clone()));
            if options.descending() { names.reverse(); }
            let total = names.len();
            let items: Vec<_> = names.into_iter().skip(page.saturating_mul(PAGE)).take(PAGE).map(|(name, albums)| {
                let portrait = self.lib.artist_artwork(&name).ok().flatten().unwrap_or_default();
                json!({"kind":"composer", "name":name, "albums":albums, "portrait":portrait})
            }).collect();
            return json!({"total":total,"items":items});
        }
        let excluded: &[String] = if selected.album_scope { &selected.excluded_sources }
            else { options.excluded_sources.as_deref().unwrap_or(&selected.excluded_sources) };
        self.genre_page_using(&selected, &text, &options, excluded, page, &index)
    }

    fn genre_page_using(&self, selected: &Selection, text: &str, options: &options::Options, excluded: &[String], page: usize, index: &crate::library_sources::SourceIndex) -> Value {
        let tags = match self.discovery_names_checked(false, excluded, index) {
            Ok(names) => names.into_iter().map(|(name, _)| name).collect::<Vec<_>>(),
            Err(error) => return json!({"error":error,"items":[],"total":0}),
        };
        let inventory = genres::Inventory::new(&tags);
        let populated = match self.populated_genres(&inventory, excluded, index) {
            Ok(populated) => populated,
            Err(error) => return json!({"error":error,"items":[],"total":0}),
        };
        let mut names = inventory.browse((selected.kind == "genre").then_some(selected.name.as_str()), &text, &options.scope);
        names.retain(|name| inventory.populated_by(name, &populated));
        if options.descending() { names.reverse(); }
        let total = names.len();
        let names: Vec<_> = names.into_iter().skip(page.saturating_mul(PAGE)).take(PAGE).collect();
        let mut summarized = names.clone();
        if selected.kind == "genre" { summarized.push(selected.name.clone()); }
        let summaries = match self.genre_summaries_using(&summarized, &inventory, excluded, index) {
            Ok(summaries) => summaries,
            Err(error) => return json!({"error":error,"items":[],"total":0}),
        };
        let chosen = chosen_art::choices();
        let items = names.into_iter().zip(&summaries).map(|(name, summary)| {
            let styles = inventory.records_for(&name).len();
            let label = presentation::Label::new(&name);
            let key = self.genre_entry_key(&name, &label, excluded, index)?;
            let portrait = chosen_art::portrait(&chosen, &name);
            Ok(json!({"kind":"genre", "name":label.text, "nameTruncated":label.truncated, "nameChars":label.chars,
                "key":key, "portraitChosen":!portrait.is_empty(), "portrait":portrait, "styles":styles, "albums":summary.albums, "looseTracks":summary.loose_tracks, "images":summary.images, "art":genres::art(&name)}))
        }).collect::<Result<Vec<_>,String>>();
        let items = match items {
            Ok(items) => items,
            Err(error) => return json!({"error":error,"items":[],"total":0}),
        };
        let resolutions = inventory.records_for(&selected.name);
        let breadcrumb: Vec<_> = genres::breadcrumb(&selected.name).iter().map(|name|presentation::Label::new(name)).collect();
        let portrait = chosen_art::portrait(&chosen, &selected.name);
        json!({"portraitChosen":!portrait.is_empty(), "portrait":portrait, "total":total, "excluded_sources":excluded, "sources":self.discovery_sources_using(index), "art":genres::art(&selected.name), "taxonomyVersion":genres::VERSION, "resolutions":resolutions.iter().take(PAGE).map(|record|presentation::resolution(record)).collect::<Vec<_>>(),
            "resolutionTotal":resolutions.len(), "parents":genres::parents(&selected.name), "parentRelations":genres::parent_relations(&selected.name),
            "breadcrumb":breadcrumb.iter().map(|label|&label.text).collect::<Vec<_>>(),
            "breadcrumbTruncated":breadcrumb.iter().map(|label|label.truncated).collect::<Vec<_>>(),
            "breadcrumbChars":breadcrumb.iter().map(|label|label.chars).collect::<Vec<_>>(),
            "summary":if selected.kind == "genre" { summaries.last() } else { None }, "items":items})
    }
}
