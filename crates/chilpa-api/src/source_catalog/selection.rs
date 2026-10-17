//! I retain the exact identities a menu or playlist picker is using.
use super::{Catalog, Inner};
use std::collections::HashSet;
use std::sync::Arc;

#[derive(Default)]
pub(super) struct SelectionRoots {
    pub(super) cards: HashSet<(String, String)>,
    pub(super) tracks: HashSet<String>,
}

/// I release a menu's catalog roots when its request or picker closes.
pub struct SelectionHold {
    catalog: Arc<Catalog>,
    id: u64,
}

impl SelectionHold {
    pub(crate) fn add_tracks(&self, paths: impl IntoIterator<Item = String>) {
        let mut inner = self.catalog.lock();
        if let Some(roots) = inner.selections.get_mut(&self.id) {
            roots.tracks.extend(paths);
            inner.residency.changed();
            self.catalog.wake.notify_one();
        }
    }
}

impl Drop for SelectionHold {
    fn drop(&mut self) {
        let mut inner = self.catalog.lock();
        inner.selections.remove(&self.id);
        inner.residency.changed();
        self.catalog.wake.notify_one();
    }
}

impl Catalog {
    pub(crate) fn hold(self: &Arc<Self>, cards: impl IntoIterator<Item = (String, String)>, tracks: impl IntoIterator<Item = String>) -> SelectionHold {
        self.hold_locked(&mut self.lock(), cards, tracks)
    }

    pub(super) fn hold_locked(self: &Arc<Self>, inner: &mut Inner, cards: impl IntoIterator<Item = (String, String)>, tracks: impl IntoIterator<Item = String>) -> SelectionHold {
        inner.selection_serial = inner.selection_serial.checked_add(1).expect("selection identifiers exhausted");
        let id = inner.selection_serial;
        inner.selections.insert(id, SelectionRoots { cards: cards.into_iter().collect(), tracks: tracks.into_iter().collect() });
        inner.residency.changed();
        self.wake.notify_one();
        SelectionHold { catalog: self.clone(), id }
    }

    pub(crate) fn request_selection_tracks(&self, id: i64) {
        let mut inner = self.lock();
        if inner.store.cards.get(&id).is_some_and(|card| card.tracks.is_none()) {
            inner.card_errors.remove(&id);
            self.queue(&mut inner, super::Job::Tracks(id), true);
        }
    }
}

impl Inner {
    pub(super) fn protected_shelves(&self) -> HashSet<String> {
        self.navigation.shelves.iter().cloned().chain(self.running.iter().chain(self.queue.queued.iter()).filter_map(|job| match job {
            super::Job::Shelf(key) | super::Job::Append(key) => Some(key.clone()), _ => None,
        })).collect()
    }

    pub(super) fn protection(&self) -> (HashSet<String>, HashSet<i64>) {
        let mut tracks = self.protected_tracks.clone();
        tracks.extend(self.navigation.playlists.values().flat_map(|paths| paths.iter().cloned()));
        let identities: HashSet<_> = self.selections.values().flat_map(|roots| roots.cards.iter().cloned())
            .chain(self.navigation.identities.iter().cloned()).collect();
        tracks.extend(self.selections.values().flat_map(|roots| roots.tracks.iter().cloned()));
        let cards = self.store.cards.values().filter(|card| !identities.is_empty() && identities.contains(&(card.plugin.clone(), card.identity.clone()))).map(|card| card.id)
            .chain(self.navigation.cards.iter().copied())
            .chain(self.running.iter().chain(self.queue.queued.iter()).filter_map(|job| match job { super::Job::Tracks(id) => Some(*id), _ => None }))
            .chain(self.deferred.iter().flat_map(|action| action.albums.iter().copied()).filter(|id| *id < 0)).collect();
        (tracks, cards)
    }
}

#[cfg(test)]
mod tests {
    use crate::{Chilpa, FoundRow, SelectionState};
    use crate::source_search::RemoteFound;
    use serde_json::json;

    fn row(source: &str) -> FoundRow {
        FoundRow::Remote(RemoteFound { source:source.into(), id:"album:1".into(), title:"Selected".into(), collection:true, entity:"album".into(),
            item:Some(json!({"kind":"album","ref":"album:1","title":"Selected"})), ..Default::default() })
    }

    #[test]
    fn loading_and_picker_roots_survive_pruning_and_release_on_cancel() {
        let mut api = Chilpa::new();
        let mut request = api.found_selection(&row("source-a")).unwrap();
        assert_eq!(request.poll(), SelectionState::Pending);
        let selected = api.catalog.adopt("source-a", None, match &row("source-a") { FoundRow::Remote(row) => row, _ => unreachable!() });
        let other = api.catalog.adopt("source-b", None, match &row("source-b") { FoundRow::Remote(row) => row, _ => unreachable!() });
        {
            let mut inner = api.catalog.lock();
            inner.store.cards.get_mut(&selected).unwrap().seen_at = 0;
            inner.store.cards.get_mut(&other).unwrap().seen_at = 0;
            for number in 0..6001 { let id = inner.store.card_id("extra", &format!("album:{number}")); inner.store.cards.get_mut(&id).unwrap().seen_at = number + 1; }
            let (tracks, cards) = inner.protection();
            inner.store.prune(&tracks, &cards);
            assert!(inner.store.cards.contains_key(&selected));
            assert!(!inner.store.cards.contains_key(&other), "the same opaque ref in another source is not protected");
            inner.card_errors.insert(selected, "Source failed".into());
        }
        assert_eq!(request.poll(), SelectionState::Failed("Source failed".into()));
        drop(request);
        let mut inner = api.catalog.lock();
        assert!(inner.selections.is_empty(), "cancel or failure releases all request roots");
        let pending = super::super::Job::Tracks(selected);
        assert!(inner.queue.queued.contains(&pending));
        assert!(inner.protection().1.contains(&selected), "shared queued work still owns a separate root");
        inner.queue.urgent.retain(|job| *job != pending);
        inner.queue.later.retain(|job| *job != pending);
        inner.queue.queued.remove(&pending);
        assert!(!inner.protection().1.contains(&selected), "completed work releases the last independent root");
        let newest = inner.store.card_id("extra", "album:last");
        inner.store.cards.get_mut(&newest).unwrap().seen_at = u64::MAX;
        let (tracks, cards) = inner.protection();
        inner.store.prune(&tracks, &cards);
        assert!(!inner.store.cards.contains_key(&selected));
    }

    #[test]
    fn deferred_collections_and_ready_picker_tracks_remain_roots() {
        let api = Chilpa::new();
        let remote = row("source-a");
        let FoundRow::Remote(remote) = &remote else { unreachable!() };
        let id = api.catalog.adopt("source-a", None, remote);
        api.catalog.defer(vec![id], "later", false, None);
        assert!(api.catalog.lock().protection().1.contains(&id));
        let track = {
            let mut inner = api.catalog.lock();
            let track = inner.store.take_item_track("source-a", &json!({"kind":"track","ref":"track:1","title":"Song"}), None, 1).unwrap();
            inner.store.cards.get_mut(&id).unwrap().tracks = Some(vec![track]);
            track
        };
        let (ready, ready_hold) = api.catalog.ready();
        assert_eq!(ready.len(), 1);
        assert!(ready_hold.is_some());
        assert!(api.catalog.lock().protection().1.contains(&id), "ready actions keep roots after leaving the deferred list");
        drop(ready_hold);
        {
            let mut inner = api.catalog.lock();
            assert!(inner.selections.is_empty(), "the ready action released its selection hold");
            let pending = super::super::Job::Tracks(id);
            assert!(inner.queue.queued.contains(&pending));
            assert!(inner.protection().1.contains(&id), "queued work outlives the ready action");
            inner.queue.urgent.retain(|job| *job != pending);
            inner.queue.later.retain(|job| *job != pending);
            inner.queue.queued.remove(&pending);
            assert!(!inner.protection().1.contains(&id), "finishing the queued job releases its root");
        }
        let hold = api.hold_playlist_tracks(&[track]);
        let path = crate::source_opener::entry_of("source-a", "track:1");
        assert!(api.catalog.lock().protection().0.contains(&path));
        drop(hold);
        assert!(!api.catalog.lock().protection().0.contains(&path));
    }
}
