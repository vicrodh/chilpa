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
        self.album_genres_page_using(album, "", 0, 8, false, index)
    }

    pub(super) fn album_genres_page_using(&self, album: i64, text: &str, page: usize, limit: usize, descending: bool, index: &SourceIndex) -> Value {
        let result = (|| {
            let inventory = self.album_genres_index(album, text, index)?;
            let (rows, total) = inventory.index.page(page.saturating_mul(PAGE), limit, descending).map_err(|error|error.to_string())?;
            let items: Vec<_> = rows.into_iter().map(|row| {
                let name = if row.truncated { format!("{}…", row.name) } else { row.name };
                let key = json!({"kind":"genre","name":name,"album":album,"genreToken":row.token,"albumScope":true}).to_string();
                json!({"name":name,"key":key,"originalsCount":row.originals,"nameTruncated":row.truncated})
            }).collect();
            Ok::<_,String>(json!({"items":items,"total":total,"hasMore":total > page.saturating_mul(PAGE).saturating_add(limit)}))
        })();
        result.unwrap_or_else(|error|json!({"error":error,"items":[],"total":0,"hasMore":false}))
    }

    fn album_genres_index(&self, album: i64, text: &str, index: &SourceIndex) -> Result<AlbumGenres,String> {
        let inventory = AlbumGenres::new()?;
        if album > 0 {
            self.lib.visit_album_genres(album, |tag, source| {
                inventory.index.source(source)?;
                inventory.insert(tag,text)
            }).map_err(|error|error.to_string())?;
        } else if album < 0 {
            let card = self.catalog.as_ref().and_then(|catalog|catalog.cards(&[album]).into_iter().next());
            if let Some(card) = card {
                inventory.index.source(&format!("{}:{}",crate::source_opener::PROVIDER,card.plugin)).map_err(|error|error.to_string())?;
                let listed = card.metadata["genres"].as_array();
                if listed.is_some_and(|values| values.iter().filter_map(Value::as_str).any(|name|!name.trim().is_empty())) {
                    for name in listed.into_iter().flatten().filter_map(Value::as_str).map(str::trim) {
                        inventory.insert(name,text).map_err(|error|error.to_string())?;
                    }
                } else {
                    let single = card.metadata["genre"].as_str().filter(|name|!name.trim().is_empty()).unwrap_or(&card.genre);
                    for name in single.split(", ").map(str::trim) {
                        inventory.insert(name,text).map_err(|error|error.to_string())?;
                    }
                }
            }
            for item in index.items.iter().filter(|item|item.album == album) {
                inventory.index.source(&item.source_key()).map_err(|error|error.to_string())?;
                for tag in &item.genres { inventory.insert(tag,text).map_err(|error|error.to_string())?; }
            }
        }
        Ok(inventory)
    }

    pub(super) fn resolve_album_genre_using(&self, mut selected: Selection, index: &SourceIndex) -> Result<Selection,String> {
        if selected.genre_token.is_empty() { return Ok(selected); }
        let inventory = self.album_genres_index(selected.album,"",index)?;
        selected.name = inventory.index.canonical(&selected.genre_token).map_err(|error|error.to_string())?
            .ok_or_else(||chilpa_i18n::t("This genre is no longer present on the album."))?;
        if selected.album_scope {
            let mut excluded = Vec::new();
            for source in self.discovery_sources_using(index) {
                if let Some(key) = source["key"].as_str() {
                    if !inventory.index.has_source(key).map_err(|error|error.to_string())? { excluded.push(key.to_string()); }
                }
            }
            selected.excluded_sources = excluded;
        }
        Ok(selected)
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

// This query owns one disk index, released on success, failure, or cancellation
// of publication. No page inventory survives in Reader. Each existing query
// worker serializes its jobs; its generation rejects stale results.
struct AlbumGenres {
    index: chilpa_library::genre_index::GenreIndex,
    _directory: tempfile::TempDir,
}

impl AlbumGenres {
    fn new() -> Result<Self,String> {
        let cache = chilpa_dirs::cache().join("genre-queries");
        std::fs::create_dir_all(&cache).map_err(|error|error.to_string())?;
        let directory = tempfile::Builder::new().prefix("album-").tempdir_in(cache).map_err(|error|error.to_string())?;
        let index = chilpa_library::genre_index::GenreIndex::open(&directory.path().join("genres.sqlite")).map_err(|error|error.to_string())?;
        Ok(Self { index, _directory:directory })
    }

    fn insert(&self, tag: &str, text: &str) -> Result<(),chilpa_library::genre_index::Error> {
        use sha2::{Digest,Sha256};
        if tag.trim().is_empty() { return Ok(()); }
        // Validate before allocating normalization, folding, and hash inputs.
        chilpa_library::genre_index::validate_tag(tag)?;
        let (identity,canonical) = genres::link(tag);
        let token = format!("{:x}",Sha256::digest(identity.as_bytes()));
        let matched = text.is_empty() || chilpa_library::store::query::fold(tag).contains(text)
            || chilpa_library::store::query::fold(canonical).contains(text);
        self.index.insert(&token,tag,canonical,matched)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn album_query_index_is_removed_after_success_and_failure() {
        let inventory = AlbumGenres::new().unwrap();
        let path = inventory._directory.path().to_path_buf();
        inventory.insert("Heavy Metal", "").unwrap();
        assert_eq!(inventory.index.page(0,8,false).unwrap().1,1);
        drop(inventory);
        assert!(!path.exists());
        let inventory = AlbumGenres::new().unwrap();
        let path = inventory._directory.path().to_path_buf();
        assert!(inventory.insert(&"x".repeat(chilpa_library::genre_index::MAX_TAG_BYTES+1), "").is_err());
        drop(inventory);
        assert!(!path.exists());
    }
}
