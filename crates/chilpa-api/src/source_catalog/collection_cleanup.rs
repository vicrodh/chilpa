//! Reclaim only IDs allocated by an action, after its temporary roots close.
//! Runs on the collection worker, including after close with no UI polling.
use super::{Catalog, Inner, Job};
use std::collections::HashSet;

#[derive(Default)]
pub(super) struct Candidates {
    pub cards: HashSet<i64>,
    pub tracks: HashSet<i64>,
}

fn track_rooted(inner: &Inner, id: i64) -> bool {
    let Some(track) = inner.store.tracks.get(&id) else {
        return false;
    };
    inner.protected_tracks.contains(&track.path)
        || inner
            .navigation
            .playlists
            .values()
            .any(|paths| paths.contains(&track.path))
        || inner
            .selections
            .values()
            .any(|roots| roots.tracks.contains(&track.path))
        || inner
            .running
            .iter()
            .chain(&inner.queue.queued)
            .any(|job| matches!(job, Job::Track(path) if *path == track.path))
}

fn card_rooted(inner: &Inner, id: i64) -> bool {
    let Some(card) = inner.store.cards.get(&id) else {
        return false;
    };
    let same = |(plugin, reference): &(String, String)| {
        *plugin == card.plugin && *reference == card.identity
    };
    inner.navigation.cards.contains(&id)
        || inner.navigation.identities.iter().any(same)
        || inner
            .selections
            .values()
            .any(|roots| roots.cards.iter().any(same))
        || inner
            .deferred
            .iter()
            .any(|action| action.albums.contains(&id))
        || inner
            .running
            .iter()
            .chain(&inner.queue.queued)
            .any(|job| matches!(job, Job::Tracks(asked) if *asked == id))
}

impl Catalog {
    pub(crate) fn reap_collection_actions(&self) {
        let mut inner = self.lock();
        let mut candidates = std::mem::take(&mut inner.collection_ephemeral);
        if candidates.cards.is_empty() && candidates.tracks.is_empty() {
            return;
        }
        candidates.cards.retain(|id| !card_rooted(&inner, *id));
        candidates.tracks.retain(|id| !track_rooted(&inner, *id));
        // Membership is preserved even for a shelf which is currently hidden.
        // The integrated source index derives its entities from these shelves.
        for shelf in inner.store.shelves.values() {
            for id in shelf.cards.iter().chain(shelf.membership.keys()) {
                candidates.cards.remove(id);
            }
            for id in &shelf.tracks {
                candidates.tracks.remove(id);
            }
        }
        for card in inner.store.cards.values() {
            for id in card.tracks.iter().flatten() {
                candidates.tracks.remove(id);
            }
        }
        // A queued, selected, indexed, or pre-existing track keeps its album.
        for (id, track) in &inner.store.tracks {
            if !candidates.tracks.contains(id) {
                candidates.cards.remove(&track.album_id);
            }
        }
        inner
            .store
            .remove_action_orphans(&candidates.cards, &candidates.tracks);
        for id in &candidates.cards {
            inner.published.cards.remove(id);
            inner.card_errors.remove(id);
            inner.card_retry_at.remove(id);
            inner.entity_revisions.remove(id);
        }
        for id in &candidates.tracks {
            inner.published.tracks.remove(id);
        }
        inner.dirty = true;
        drop(inner);
        self.publish(true);
    }
}

#[cfg(test)]
mod tests {
    use super::super::store::Shelf;
    use super::*;
    use serde_json::json;

    #[test]
    fn closing_actions_reclaims_new_orphans_and_preserves_existing_queue_picker_and_index_roots() {
        let catalog = super::super::collection_window::ready_test_catalog();
        let (old_card, old_track) = {
            let mut inner = catalog.lock();
            let card = inner
                .store
                .take_item(
                    "fixture",
                    &json!({"kind":"album","ref":"album:old","title":"Old"}),
                    1,
                )
                .unwrap();
            let track = inner
                .store
                .take_item_track(
                    "fixture",
                    &json!({"ref":"track:old","title":"Old"}),
                    None,
                    1,
                )
                .unwrap();
            (card, track)
        };
        let wire = json!({"layout":"mixed","items":[
            {"kind":"album","ref":"album:old","title":"Old"},
            {"kind":"track","ref":"track:old","title":"Old"},
            {"kind":"album","ref":"album:orphan","title":"Orphan"},
            {"kind":"track","ref":"track:orphan","title":"Orphan"},
            {"kind":"track","ref":"track:queue","title":"Queue","album_ref":"album:queue"},
            {"kind":"track","ref":"track:picker","title":"Picker"},
            {"kind":"track","ref":"track:index","title":"Index"},
            {"kind":"album","ref":"album:open","title":"Open"}]});
        let (lease, _) = catalog.collection_action("fixture", &wire, 0, 1).unwrap();
        let path = |reference| crate::source_opener::entry_of("fixture", reference);
        catalog.protect_tracks([path("track:queue")].into());
        let picker = catalog.hold([], [path("track:picker")]);
        let (indexed, opened) = {
            let mut inner = catalog.lock();
            let indexed = inner.store.track_by_path(&path("track:index")).unwrap().id;
            let opened = inner
                .store
                .cards
                .values()
                .find(|card| card.identity == "album:open")
                .unwrap()
                .id;
            inner.store.shelves.insert(
                "fixture section:tracks".into(),
                Shelf {
                    tracks: vec![indexed],
                    ..Default::default()
                },
            );
            (indexed, opened)
        };
        catalog.protect_navigation(&[crate::navigation::Route {
            screen: crate::navigation::Screen::Album,
            id: opened,
            key: String::new(),
        }]);
        drop(lease);
        catalog.reap_collection_actions();
        let inner = catalog.lock();
        assert!(
            inner.store.cards.contains_key(&old_card)
                && inner.store.tracks.contains_key(&old_track)
        );
        assert!(inner.store.track_by_path(&path("track:orphan")).is_none());
        assert!(
            !inner
                .store
                .cards
                .values()
                .any(|card| card.identity == "album:orphan")
        );
        let queued = inner.store.track_by_path(&path("track:queue")).unwrap();
        assert!(inner.store.cards.contains_key(&queued.album_id));
        assert!(inner.store.track_by_path(&path("track:picker")).is_some());
        assert_eq!(
            inner.store.track_by_path(&path("track:index")).unwrap().id,
            indexed
        );
        assert!(inner.store.cards.contains_key(&opened));
        assert!(
            inner.collection_ephemeral.cards.is_empty()
                && inner.collection_ephemeral.tracks.is_empty()
        );
        drop(inner);
        drop(picker);
    }

    #[test]
    fn late_bootstrap_rejects_actions_before_allocating_and_path_panels_keep_their_targets() {
        let api = crate::Chilpa::new();
        let catalog = &api.catalog;
        let page = json!({"layout":"collection","items":[
            {"kind":"track","ref":"track:panel","album_ref":"album:panel","title":"Song"}]});
        assert!(
            catalog
                .collection_action("fixture", &page, 0, 1)
                .err()
                .unwrap()
                .contains("still loading")
        );
        {
            let mut inner = catalog.lock();
            assert!(inner.store.cards.is_empty() && inner.store.tracks.is_empty());
            // A delayed bootstrap can replace the store safely before retry.
            inner.store = Default::default();
            inner.loaded = true;
        }
        let (lease, _) = catalog.collection_action("fixture", &page, 0, 2).unwrap();
        let path = crate::source_opener::entry_of("fixture", "track:panel");
        let panel = api.hold_catalog_path(&path);
        let id = catalog.track_by_path(&path).unwrap().0.id;
        drop(lease);
        catalog.reap_collection_actions();
        let inner = catalog.lock();
        let track = inner.store.track_by_path(&path).unwrap();
        assert_eq!(track.id, id);
        assert!(inner.store.cards.contains_key(&track.album_id));
        drop(inner);
        drop(panel);
    }

    #[test]
    fn repeated_cancelled_menus_do_not_accumulate_cards_or_tracks() {
        let catalog = super::super::collection_window::ready_test_catalog();
        for page in 0..32 {
            let items = (0..100)
                .map(|at| {
                    json!({"kind":"track","ref":format!("track:{page}:{at}"),
                "album_ref":format!("album:{page}:{at}"),"title":"Song"})
                })
                .collect::<Vec<_>>();
            let (lease, _) = catalog
                .collection_action(
                    "fixture",
                    &json!({"layout":"collection","items":items}),
                    0,
                    page + 1,
                )
                .unwrap();
            assert_eq!(catalog.lock().store.tracks.len(), 100);
            drop(lease);
            catalog.reap_collection_actions();
            let inner = catalog.lock();
            assert!(inner.store.cards.is_empty() && inner.store.tracks.is_empty());
            assert!(inner.published.cards.is_empty() && inner.published.tracks.is_empty());
        }
    }
}
