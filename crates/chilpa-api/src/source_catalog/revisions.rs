//! I publish content changes separately from cache timestamps and artwork.

use super::{store::{Card, Shelf, Track}, Catalog};
use std::sync::{atomic::Ordering, Arc};
use std::hash::{Hash, Hasher};

pub(super) fn same_card(a: &Card, b: &Card) -> bool {
    a.id == b.id && a.plugin == b.plugin && a.identity == b.identity && a.kind == b.kind
        && a.title == b.title && a.metadata == b.metadata && a.artist == b.artist && a.year == b.year
        && a.genre == b.genre && a.last_played_at == b.last_played_at && a.image == b.image
        && a.format == b.format && a.tracks == b.tracks
}
fn same_shelf(a: &Shelf, b: &Shelf) -> bool {
    a.document == b.document && a.title == b.title && a.cards == b.cards && a.tracks == b.tracks
        && a.folders == b.folders && a.membership == b.membership
}

/// I exclude freshness stamps while retaining durable library rows and ownership.
pub(super) fn library_changed(after: &super::store::Store, before: &super::store::Store) -> bool {
    !after.cards.changed_keys(&before.cards, |a, b| same_card(a, b) && a.art == b.art && a.seen_in == b.seen_in).is_empty()
        || !after.tracks.changed_keys(&before.tracks, |a, b| a == b).is_empty()
        || !after.shelves.changed_keys(&before.shelves, same_shelf).is_empty()
}

#[derive(Default)]
pub(super) struct EntityRevision {
    card: Option<Arc<Card>>,
    tracks: Vec<Arc<Track>>,
    inherited_formats: u64,
    content: i32,
    genres: u64,
    genres_revision: u64,
    composers_revision: u64,
    error: Option<String>,
    art: i32,
    art_signature: u64,
}

impl EntityRevision {
    pub(super) fn retains_current_cache(&self, id: i64, store: &super::store::Store) -> bool {
        store.cards.get(&id).is_some_and(|card| self.card.as_ref().is_none_or(|previous| previous.tracks == card.tracks))
            && self.tracks.iter().all(|track| store.tracks.shared(&track.id).is_some_and(|current| Arc::ptr_eq(&current, track)))
    }
}

impl Catalog {
    pub(super) fn publish(&self, status_changed: bool) {
        let mut inner = self.lock();
        let before = &inner.published;
        let changed = !inner.store.cards.changed_keys(&before.cards, same_card).is_empty()
            || !inner.store.tracks.changed_keys(&before.tracks, |a, b| a == b).is_empty()
            || !inner.store.shelves.changed_keys(&before.shelves, same_shelf).is_empty()
            || inner.store.drawn != before.drawn || inner.store.names != before.names;
        inner.published = inner.store.snapshot();
        drop(inner);
        if changed || status_changed { self.content_rev.fetch_add(1, Ordering::Relaxed); }
    }

    pub(crate) fn entity_revision(&self, id: i64) -> (i32, i32) {
        let mut inner = self.lock();
        let card = inner.store.cards.shared(&id);
        let tracks: Vec<_> = card.as_ref().into_iter().flat_map(|card| card.tracks.iter().flatten())
            .filter_map(|id| inner.store.tracks.shared(id)).collect();
        let mut artwork = std::collections::hash_map::DefaultHasher::new();
        let mut inherited_formats = std::collections::hash_map::DefaultHasher::new();
        let mut hash_card = |card: &Card| {
            card.art.hash(&mut artwork);
            inner.store.chosen.get(&(card.plugin.clone(), card.identity.clone())).hash(&mut artwork);
            inner.art_received.contains(&card.image).hash(&mut artwork);
        };
        if let Some(card) = &card { hash_card(card); }
        for track in &tracks {
            if let Some(parent) = inner.store.cards.get(&track.album_id) {
                hash_card(parent);
                if track.format.is_none() && parent.kind == super::store::CardKind::Album && parent.plugin == track.plugin {
                    parent.format.hash(&mut inherited_formats);
                }
            }
        }
        for track in &tracks { inner.art_received.contains(&track.image).hash(&mut artwork); }
        let artwork = artwork.finish();
        let inherited_formats = inherited_formats.finish();
        if inner.entity_revisions.len() >= 64 && !inner.entity_revisions.contains_key(&id) {
            inner.entity_revisions.clear();
        }
        let error = inner.card_errors.get(&id).cloned();
        let old = inner.entity_revisions.get(&id);
        let same = match (old.and_then(|revision| revision.card.as_ref()), &card) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b) || same_card(a, b),
            (None, None) => true,
            _ => false,
        };
        let content_changed = !same || old.is_some_and(|revision| revision.error != error || revision.inherited_formats != inherited_formats) || old.is_none_or(|revision| revision.tracks.len() != tracks.len()
            || revision.tracks.iter().zip(&tracks).any(|(a, b)| !Arc::ptr_eq(a, b) && a != b));
        // I compare displayed metadata only when content or either cache changed.
        let genres = old.filter(|revision| !content_changed && revision.genres_revision == inner.genres.revision
                && revision.composers_revision == inner.composers.revision)
            .map(|revision| revision.genres).unwrap_or_else(|| {
                let mut hash = std::collections::hash_map::DefaultHasher::new();
                let now = super::now();
                if let Some(card) = &card { inner.genres.of_card(card, now).hash(&mut hash); }
                for track in &tracks {
                    inner.composers.of_track(track, now).hash(&mut hash);
                    let own = super::genres::own_genres(&track.metadata, "");
                    if !own.is_empty() { own.hash(&mut hash); }
                    else if let Some(parent) = inner.store.cards.get(&track.album_id) { inner.genres.of_card(parent, now).hash(&mut hash); }
                }
                hash.finish()
            });
        let content_changed = content_changed || old.is_some_and(|revision| revision.genres != genres);
        let art_changed = old.is_none_or(|revision| revision.art_signature != artwork);
        if content_changed || art_changed { inner.entity_revision_clock = inner.entity_revision_clock.wrapping_add(1); }
        let clock = inner.entity_revision_clock;
        let genres_revision = inner.genres.revision;
        let composers_revision = inner.composers.revision;
        let revision = inner.entity_revisions.entry(id).or_default();
        revision.inherited_formats = inherited_formats;
        revision.genres = genres;
        revision.genres_revision = genres_revision;
        revision.composers_revision = composers_revision;
        if content_changed { revision.content = clock; }
        if art_changed { revision.art = clock; revision.art_signature = artwork; }
        revision.error = error;
        revision.card = card;
        revision.tracks = tracks;
        (revision.content, revision.art)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn identical_refreshes_do_not_publish_content_or_entity_changes() {
        let catalog = Catalog::new();
        let item = json!({"kind":"playlist","ref":"list:1","title":"List","tracks":1,"updated_at":7});
        let id = catalog.lock().store.take_item("fixture", &item, 1).unwrap();
        catalog.publish(false);
        let content = catalog.revisions();
        let entity = catalog.entity_revision(id);
        catalog.lock().store.take_item("fixture", &item, 200);
        catalog.publish(false);
        assert_eq!(catalog.revisions(), content);
        assert_eq!(catalog.entity_revision(id), entity);
        catalog.lock().store.take_item("other", &json!({"kind":"album","ref":"album:9","title":"Other"}), 201);
        catalog.publish(false);
        assert_eq!(catalog.entity_revision(id), entity, "unrelated catalog additions do not rebuild this playlist");
    }

    #[test]
    fn row_changes_reach_each_playlist_and_parent_art_has_its_own_revision() {
        let catalog = Catalog::new();
        let (first, second, track, album) = {
            let mut inner = catalog.lock();
            let first = inner.store.card_id("fixture", "list:1");
            let second = inner.store.card_id("fixture", "list:2");
            let track = inner.store.take_item_track("fixture", &json!({"kind":"track","ref":"track:1","title":"Song","album_ref":"album:1"}), None, 1).unwrap();
            for id in [first, second] { inner.store.cards.get_mut(&id).unwrap().tracks = Some(vec![track]); }
            (first, second, track, inner.store.tracks[&track].album_id)
        };
        let a = catalog.entity_revision(first);
        let b = catalog.entity_revision(second);
        catalog.lock().store.tracks.get_mut(&track).unwrap().format = Some(super::super::store::Format { codec:"flac".into(), bits:Some(24), rate:Some(96000), advertised_hires:false });
        let a_changed = catalog.entity_revision(first);
        let b_changed = catalog.entity_revision(second);
        assert_ne!(a.0, a_changed.0);
        assert_ne!(b.0, b_changed.0);
        assert_eq!(a.1, a_changed.1);
        catalog.lock().store.cards.get_mut(&album).unwrap().art = "cover.png".into();
        let artwork = catalog.entity_revision(first);
        assert_eq!(a_changed.0, artwork.0);
        assert_ne!(a_changed.1, artwork.1);
        catalog.lock().card_errors.insert(first, "Source failed".into());
        assert_ne!(catalog.entity_revision(first).0, artwork.0, "a failed refresh remains visible");
    }
}
