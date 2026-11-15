//! Album eligibility precedes navigation pagination. Only borrowed tag pointers
//! and one index per original tag are retained during this read. No additional
//! copies of long spellings, per-album sets, worker, or persistent cache.
use super::*;

impl Reader {
    pub(super) fn populated_genres(&self, inventory: &genres::Inventory, excluded: &[String], index: &crate::library_sources::SourceIndex) -> Result<HashSet<usize>, String> {
        let originals = inventory.original_indices();
        let mut populated = HashSet::new();
        let mut accept = |name: &str| {
            if let Some(at) = originals.get(name) { populated.insert(*at); }
        };
        self.lib.visit_album_genre_names_excluding(excluded, &mut accept).map_err(|error| error.to_string())?;
        let through = sources::through(excluded);
        let stars = crate::library_sources::Stars::default();
        for item in index.items.iter().filter(|item| item.album != 0 && through.passes(item, &stars)) {
            for tag in &item.genres { accept(tag); }
        }
        Ok(populated)
    }
}
