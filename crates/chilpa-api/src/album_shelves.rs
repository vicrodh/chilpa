//! I read local recommendation shelves away from the window's thread.

use crate::{Album, Reader};
use crate::source_catalog::{Catalog, store::CardKind};

impl Reader {
    pub fn album_shelf(&self, kind: &str, id: i64, limit: usize) -> Vec<Album> {
        self.lib.read_snapshot(|| match kind {
            "related" => grouped(&self.lib, self.lib.related_albums(id, limit).unwrap_or_default().into_iter().map(crate::browse::album_of).collect()),
            "sameArtist" => {
                let catalog = self.catalog.as_deref();
                let album = if id < 0 {
                    catalog.and_then(|catalog| catalog.card(id)).filter(|card| card.kind == CardKind::Album)
                        .map(|card| crate::source_catalog::view::album_of(&card))
                } else { self.lib.album(id).ok().flatten().map(crate::browse::album_of) };
                let Some(album) = album else { return Vec::new() };
                let local = self.lib.albums_filtered(chilpa_library::AlbumSort::Year, Some(&album.artist), 10_000, 0)
                    .unwrap_or_default().into_iter().map(crate::browse::album_of).collect();
                let local = grouped(&self.lib, local);
                let sources = catalog.and_then(Catalog::natives).map(|natives| natives.ids().into_iter().filter(|id| natives.searches(id)).collect()).unwrap_or_default();
                same_artist(&album, local, catalog, sources, limit)
            }
            _ => Vec::new(),
        }).map_err(|error| eprintln!("chilpa-queries: album shelf failed: {error}")).unwrap_or_default()
    }
}

fn grouped(lib: &chilpa_library::Library, cards: Vec<Album>) -> Vec<Album> {
    let ids: Vec<_> = cards.iter().map(|card| card.id).collect();
    let ids = lib.grouped_album_ids(&ids).unwrap_or(ids);
    ids.chunks(500).flat_map(|ids| lib.albums_by_ids(ids).unwrap_or_default()).map(crate::browse::album_of).collect()
}

pub(crate) fn same_artist(album: &Album, local: Vec<Album>, catalog: Option<&Catalog>, mut sources: Vec<String>, limit: usize) -> Vec<Album> {
    let local: Vec<_> = local.into_iter().filter(|candidate| candidate.artist.trim().to_lowercase() == album.artist.trim().to_lowercase()).collect();
    let mut groups = vec![local];
    sources.sort();
    if let Some(catalog) = catalog {
        for source in sources {
            if let Some(shelf) = catalog.shelf(&format!("{source} matching-artist-v3:{}", album.artist)) {
                groups.push(catalog.cards(&shelf.cards).iter().map(crate::source_catalog::view::album_of).collect());
            }
        }
    }
    merge_shelves(album.id, groups, limit)
}

fn merge_shelves(id: i64, mut groups: Vec<Vec<Album>>, limit: usize) -> Vec<Album> {
    for group in &mut groups {
        group.retain(|candidate| candidate.id != id);
        group.sort_by_key(|candidate| candidate.id.wrapping_mul(1_103_515_245).wrapping_add(id) & 0x7fff_ffff);
    }
    let mut result = Vec::new();
    for at in 0..groups.iter().map(Vec::len).max().unwrap_or(0) {
        for group in &groups { if let Some(card) = group.get(at) { if result.len() < limit { result.push(card.clone()); } } }
        if result.len() >= limit { break; }
    }
    result
}

#[cfg(test)]
mod tests;
