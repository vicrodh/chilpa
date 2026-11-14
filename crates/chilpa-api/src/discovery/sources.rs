//! Source keys reuse the Albums filter; no second source identity or cache.
use super::*;
use crate::library_sources::{Prepared, SourceIndex};

pub(super) fn through(excluded: &[String]) -> Prepared {
    Prepared::new(&Filter { excluded_sources:excluded.to_vec(), ..Default::default() })
}

impl Reader {
    /// Genre links for the exact album shown; the worker owns every DB read.
    /// Remote pages may call this after publishing their cached header.
    pub fn album_genre_links(&self, album: i64) -> Value {
        let (_, index) = self.source_index();
        self.album_genre_links_using(album, &index)
    }

    pub(super) fn album_genre_links_using(&self, album: i64, index: &SourceIndex) -> Value {
        let (mut tags, own) = if album > 0 {
            self.lib.album_genre_sources(album).unwrap_or_default()
        } else if album < 0 {
            let card = self.catalog.as_ref().and_then(|catalog| catalog.cards(&[album]).into_iter().next());
            let mut tags = card.as_ref().map(|card| crate::source_catalog::genres::own_genres(&card.metadata, &card.genre)).unwrap_or_default();
            let mut sources = HashSet::new();
            if let Some(card) = &card { sources.insert(format!("{}:{}", crate::source_opener::PROVIDER, card.plugin)); }
            for item in index.items.iter().filter(|item| item.album == album) {
                tags.extend(item.genres.iter().cloned()); sources.insert(item.source_key());
            }
            (tags, sources.into_iter().collect())
        } else { Default::default() };
        let excluded: Vec<_> = self.discovery_sources_using(index).iter().filter_map(|source| source["key"].as_str())
            .filter(|key| !own.iter().any(|held| held == key)).map(str::to_string).collect();
        tags.sort_by_cached_key(|name| (name.to_lowercase(), name.clone()));
        tags.dedup();
        let mut groups = BTreeMap::<genres::Identity, (String, Vec<String>)>::new();
        for tag in tags.into_iter().filter(|tag| !tag.trim().is_empty()) {
            // Literal format variants share a chip without asserting ancestry
            // or merging an uncertain spelling into a recognized concept.
            let (identity, name) = genres::link(&tag);
            groups.entry(identity).or_insert_with(|| (name, Vec::new())).1.push(tag);
        }
        let mut groups: Vec<_> = groups.into_values().collect();
        groups.sort_by_cached_key(|(_, originals)| (originals[0].to_lowercase(), originals[0].clone()));
        let items: Vec<_> = groups.into_iter().map(|(name, originals)| {
            let selected = Selection { kind:"genre".into(), name, excluded_sources:excluded.clone(), ..Default::default() };
            json!({"name":originals[0],"originals":originals,"key":serde_json::to_string(&selected).unwrap_or_default()})
        }).collect();
        json!({"excluded_sources":excluded,"items":items})
    }

    pub(super) fn discovery_sources_using(&self, index: &SourceIndex) -> Vec<Value> {
        let mut sources: BTreeMap<_, _> = self.lib.facet_sources().unwrap_or_default().into_iter()
            .chain(index.sources.iter().cloned()).map(|entry| (entry.key.clone(), json!({"key":entry.key,"label":entry.label,"icon":entry.icon}))).collect();
        // An index assembled from resident items may not carry presentation rows.
        let mut seen = HashSet::new();
        for item in index.items.iter().filter(|item| seen.insert(item.plugin.as_str())) {
            let key = item.source_key();
            sources.entry(key.clone()).or_insert_with(|| json!({"key":key,"label":item.plugin,"icon":""}));
        }
        sources.into_values().collect()
    }
}
