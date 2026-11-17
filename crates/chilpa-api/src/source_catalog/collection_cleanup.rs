//! Reclaim only IDs allocated by an action, after its temporary roots close.
//! Runs on the collection and catalog workers when their owners release them.
use super::{Catalog, Inner, Job, collection_window::action_shelf, store::Store};
use std::collections::HashSet;

#[derive(Clone, Default)]
pub(super) struct Candidates {
    pub cards: HashSet<i64>,
    pub tracks: HashSet<i64>,
    pub(super) released: bool,
}

// Only allocations made by collection actions and their own hydration are candidates.
enum Entity { Card(i64), Track(i64) }

impl Candidates {
    pub(super) fn own_allocations(&mut self, before: (i64, i64), after: (i64, i64)) {
        self.cards.extend(after.0..before.0);
        self.tracks.extend(after.1..before.1);
    }

    fn is_empty(&self) -> bool { self.cards.is_empty() && self.tracks.is_empty() }

    // Move the reachable candidates into their owner's set. Albums and tracks can
    // refer to each other; only an edge from a live owner starts a traversal.
    fn claim(&mut self, store: &Store, mut roots: Vec<Entity>) -> Self {
        let mut reached = Self::default();
        while let Some(root) = roots.pop() {
            match root {
                Entity::Card(id) if self.cards.remove(&id) => {
                    if let Some(card) = store.cards.get(&id) {
                        reached.cards.insert(id);
                        roots.extend(card.tracks.iter().flatten().copied().map(Entity::Track));
                    }
                }
                Entity::Track(id) if self.tracks.remove(&id) => {
                    if let Some(track) = store.tracks.get(&id) {
                        reached.tracks.insert(id);
                        roots.push(Entity::Card(track.album_id));
                    }
                }
                _ => {}
            }
        }
        reached
    }
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
            .any(|roots| !roots.shelves.iter().any(|key| action_shelf(key)) && roots.tracks.contains(&track.path))

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
            .any(|roots| !roots.shelves.iter().any(|key| action_shelf(key)) && roots.cards.iter().any(same))
        || inner
            .deferred
            .iter()
            .any(|action| action.albums.contains(&id))

}

impl Inner {
    pub(super) fn collection_job(&self, job: &Job) -> bool {
        match job {
            Job::Tracks(id) => self.collection_ephemeral.cards.contains(id),
            Job::Track(path) => self.store.track_by_path(path).is_some_and(|track| self.collection_ephemeral.tracks.contains(&track.id)),
            _ => false,
        }
    }

    fn keep_durable_entities(&self, owned: &mut Candidates) {
        let mut durable = Vec::new();
        for (_, shelf) in self.store.shelves.iter().filter(|(key, _)| !action_shelf(key)) {
            durable.extend(shelf.cards.iter().chain(shelf.membership.keys()).filter(|id| owned.cards.contains(id)).copied().map(Entity::Card));
            durable.extend(shelf.tracks.iter().filter(|id| owned.tracks.contains(id)).copied().map(Entity::Track));
        }
        durable.extend(owned.cards.iter().copied().filter(|id| card_rooted(self, *id)).map(Entity::Card));
        durable.extend(owned.tracks.iter().copied().filter(|id| track_rooted(self, *id)).map(Entity::Track));
        for (id, card) in &self.store.cards {
            if !owned.cards.contains(id) {
                durable.extend(card.tracks.iter().flatten().filter(|id| owned.tracks.contains(id)).copied().map(Entity::Track));
            }
        }
        for (id, track) in &self.store.tracks {
            if !owned.tracks.contains(id) && owned.cards.contains(&track.album_id) {
                durable.push(Entity::Card(track.album_id));
            }
        }
        // Queues, navigation, pickers, existing entities and durable shelves
        // transfer their reachable entities to ordinary catalog ownership.
        owned.claim(&self.store, durable);
    }

    pub(super) fn omit_transient_entities(&self, snapshot: &mut Store) {
        snapshot.shelves.retain(|key, _| !action_shelf(key));
        if !self.collection_ephemeral.is_empty() {
            let mut temporary = self.collection_ephemeral.clone();
            self.keep_durable_entities(&mut temporary);
            snapshot.remove_action_orphans(&temporary.cards, &temporary.tracks);
        }
    }

    pub(super) fn reap_collection_actions(&mut self) -> bool {
        let mut owned = std::mem::take(&mut self.collection_ephemeral);
        if owned.is_empty() { return false; }
        let mut temporary = Vec::new();
        for (_, shelf) in self.store.shelves.iter().filter(|(key, _)| action_shelf(key)) {
            temporary.extend(shelf.cards.iter().copied().map(Entity::Card));
            temporary.extend(shelf.tracks.iter().copied().map(Entity::Track));
        }
        for job in self.running.iter().chain(&self.queue.queued) {
            match job {
                Job::Tracks(id) => temporary.push(Entity::Card(*id)),
                Job::Track(path) => {
                    if let Some(track) = self.store.track_by_path(path) { temporary.push(Entity::Track(track.id)); }
                }
                _ => {}
            }
        }
        // A second lease or an in-flight read can outlive the first lease.
        // Keep this provenance until those owners close or the read finishes.
        self.collection_ephemeral = owned.claim(&self.store, temporary);
        if owned.is_empty() { return false; }
        self.keep_durable_entities(&mut owned);
        if owned.is_empty() { return false; }
        self.store.remove_action_orphans(&owned.cards, &owned.tracks);
        for id in &owned.cards {
            self.published.cards.remove(id);
            self.card_errors.remove(id);
            self.card_retry_at.remove(id);
            self.entity_revisions.remove(id);
        }
        for id in &owned.tracks { self.published.tracks.remove(id); }
        self.dirty = true;
        true
    }
}

impl Catalog {
    pub(crate) fn reap_collection_actions(&self) {
        let mut inner = self.lock();
        if !inner.collection_ephemeral.released { return; }
        let removed = inner.reap_collection_actions();
        drop(inner);
        if removed { self.publish(true); }
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
        let (lease, _) = catalog.collection_action("fixture", &wire, 1, 1).unwrap();
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
