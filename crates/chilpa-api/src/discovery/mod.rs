//! Library discovery queries, run by the window's shared read-only worker.
//! Pages contain at most 60 small navigation entries. No network is started,
//! no new cache/worker is retained, and hidden screens submit no queries.
mod genres;
mod genre_summary;
mod works;
#[cfg(test)]
mod tests;
use crate::{AlbumsFound, Filter, Reader, Snapshot, TrackOrder};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};

const PAGE: usize = 60;

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
}

impl Selection {
    pub fn parse(key: &str) -> Option<Self> {
        (key.len() <= 8192).then(|| serde_json::from_str(key).ok()).flatten()
    }
}

impl Reader {
    pub fn collection_tracks(&self, kind: &str, key: &str, extras: &Filter, mut emit: impl FnMut(crate::FoundTracks)) {
        if kind == "discovery" { emit(crate::FoundTracks { snapshot: self.discovery_snapshot(key), ..Default::default() }); }
        else { self.tracks_progressive(key, extras, emit); }
    }

    pub fn collection_albums(&self, kind: &str, order: &str, group: &str, text: &str, extras: &Filter, scope: &str) -> AlbumsFound {
        if kind == "discovery" { self.discovery_albums(scope, text) }
        else { self.albums_view_scoped(order, group, text, extras, scope) }
    }

    fn discovery_names(&self, composers: bool) -> Vec<(String, usize)> {
        // Genre spellings remain separate inputs to the derived index. SQL's
        // NOCASE does not equate every Unicode case variant when filtering.
        let key = |name: &str| if composers { name.to_lowercase() } else { name.to_string() };
        let local = if composers { self.lib.facet_composers(&Filter::default()) } else { self.lib.facet_genres(&Filter::default()) };
        let mut names: BTreeMap<String, (String, usize)> = local.unwrap_or_default().into_iter()
            .map(|row| (key(&row.label), (row.label, row.albums))).collect();
        let (_, index) = self.source_index();
        let mut counted = HashSet::new();
        for item in &index.items {
            let tags = if composers { &item.composers } else { &item.genres };
            for tag in tags.iter().filter(|tag| !tag.trim().is_empty()) {
                let key = key(tag);
                if counted.insert((key.clone(), if item.album != 0 { item.album } else { item.track })) {
                    names.entry(key).or_insert_with(|| (tag.clone(), 0)).1 += 1;
                }
            }
        }
        names.into_values().collect()
    }

    fn discovery_filter(&self, selected: &Selection) -> Filter {
        match selected.kind.as_str() {
            "genre" => Filter { genres: genres::Inventory::new(
                &self.discovery_names(false).into_iter().map(|(name, _)| name).collect::<Vec<_>>()).matching(&selected.name), ..Default::default() },
            "composer" => Filter { composers: vec![selected.name.clone()], ..Default::default() },
            _ => Filter::default(),
        }
    }

    /// The snapshot keeps all parts, including albumless tracks. Existing
    /// source integration and copy grouping apply to genre/composer journeys.
    pub fn discovery_snapshot(&self, key: &str) -> Snapshot {
        let Some(selected) = Selection::parse(key) else { return Snapshot::default() };
        if selected.kind == "work" { return self.work_snapshot(&selected); }
        if !matches!(selected.kind.as_str(), "genre" | "composer") || selected.name.is_empty() { return Snapshot::default(); }
        let filter = self.discovery_filter(&selected);
        if selected.kind == "genre" && filter.genres.is_empty() { return Snapshot::default(); }
        self.refresh(&filter, TrackOrder::Album, || true).map(|(_, snapshot)| snapshot).unwrap_or_default()
    }

    pub fn discovery_artists(&self, key: &str) -> Vec<crate::ListedArtist> {
        let ids: HashSet<_> = self.discovery_snapshot(key).album_ids.into_iter().collect();
        let pictures = self.lib.artist_pictures().unwrap_or_default();
        let local = self.lib.artists_by_role_within(chilpa_library::ArtistRole::Album, Some(&ids)).unwrap_or_default()
            .into_iter().map(|artist| crate::browse::artist_of(artist, &pictures)).collect();
        let (_, source) = self.source_index();
        crate::albums_view::artists::listed_within(local, &source, &Filter::default(), false, Some(&ids),
            |id| self.catalog.as_ref().and_then(|catalog| catalog.card(id)).map(|card| card.image).unwrap_or_default())
    }

    pub fn discovery_albums(&self, key: &str, text: &str) -> AlbumsFound {
        let mut ids = self.discovery_snapshot(key).album_ids;
        if !text.trim().is_empty() {
            let matching: HashSet<_> = self.albums_view("title", "", text, &Filter::default()).ids.into_iter().collect();
            ids.retain(|id| matching.contains(id));
        }
        AlbumsFound { ids, groups: Vec::new() }
    }

    /// JSON for a virtualized page. Search examines the full inventory before
    /// pagination; a genre search can jump directly to a deeply nested style.
    pub fn discovery_page(&self, kind: &str, key: &str, text: &str, page: usize) -> Value {
        let selected = Selection::parse(key).unwrap_or_default();
        let text = chilpa_library::store::query::fold(text.trim());
        if kind == "recordings" { return self.recordings_page(&selected, &text, page); }
        if kind == "compositions" || selected.kind == "composer" {
            let composer = if selected.kind == "composer" { selected.name.as_str() } else { "" };
            return match self.discovery_compositions(composer, &text, page) {
                Ok((works, total)) => json!({"total":total, "items":works.into_iter().map(|work| json!({
                    "kind":"work", "name":work.title, "work":work.key, "composers":work.composers,
                    "albums":work.albums, "tracks":work.tracks})).collect::<Vec<_>>()}),
                Err(error) => json!({"error":error.to_string(), "items":[], "total":0}),
            };
        }
        if kind == "composers" {
            let names: Vec<_> = self.discovery_names(true).into_iter()
                .filter(|(name, _)| chilpa_library::store::query::fold(name).contains(&text)).collect();
            let total = names.len();
            let items: Vec<_> = names.into_iter().skip(page.saturating_mul(PAGE)).take(PAGE).map(|(name, albums)| {
                let portrait = self.lib.artist_artwork(&name).ok().flatten().unwrap_or_default();
                json!({"kind":"composer", "name":name, "albums":albums, "portrait":portrait})
            }).collect();
            return json!({"total":total,"items":items});
        }
        let tags: Vec<_> = self.discovery_names(false).into_iter().map(|(name, _)| name).collect();
        let inventory = genres::Inventory::new(&tags);
        let names = inventory.names((selected.kind == "genre").then_some(selected.name.as_str()), &text);
        let total = names.len();
        let names: Vec<_> = names.into_iter().skip(page.saturating_mul(PAGE)).take(PAGE).collect();
        let mut summarized = names.clone();
        if selected.kind == "genre" { summarized.push(selected.name.clone()); }
        let summaries = match self.genre_summaries(&summarized, &inventory) {
            Ok(summaries) => summaries,
            Err(error) => return json!({"error":error,"items":[],"total":0}),
        };
        let items: Vec<_> = names.into_iter().zip(&summaries).map(|(name, summary)| {
            let styles = inventory.records_for(&name).len();
            json!({"kind":"genre", "name":name, "styles":styles, "albums":summary.albums, "looseTracks":summary.loose_tracks, "images":summary.images})
        }).collect();
        let resolutions = inventory.records_for(&selected.name);
        json!({"total":total, "taxonomyVersion":genres::VERSION, "resolutions":resolutions.iter().take(PAGE).collect::<Vec<_>>(),
            "resolutionTotal":resolutions.len(), "parents":genres::parents(&selected.name), "parentRelations":genres::parent_relations(&selected.name), "breadcrumb":genres::breadcrumb(&selected.name),
            "summary":if selected.kind == "genre" { summaries.last() } else { None }, "items":items})
    }
}
