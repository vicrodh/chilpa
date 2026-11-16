//! Only an explicit action adopts a lazy collection page into the catalog.
use super::{Catalog, Job, SelectionHold, store::Shelf};
use serde_json::{Value, json};
use std::{collections::VecDeque, sync::Arc, time::Duration};

pub(crate) fn validate(value: &Value) -> Result<(), String> {
    if !super::collection_page::collection(value) {
        return Err("The source did not return a collection.".into());
    }
    super::collection_page::validate(value)
}

pub(crate) struct Lease {
    pub id: u64,
    catalog: Arc<Catalog>,
    key: String,
    hold: Option<SelectionHold>,
    created_cards: Vec<i64>,
    created_tracks: Vec<i64>,
}

impl Drop for Lease {
    fn drop(&mut self) {
        {
            let mut inner = self.catalog.lock();
            inner.store.shelves.remove(&self.key);
            inner.published.shelves.remove(&self.key);
            inner.dirty = true;
        }
        self.hold.take();
        {
            let mut inner = self.catalog.lock();
            inner
                .collection_ephemeral
                .cards
                .extend(self.created_cards.drain(..));
            inner
                .collection_ephemeral
                .tracks
                .extend(self.created_tracks.drain(..));
        }
        self.catalog.changed();
    }
}

pub(crate) struct Permit {
    catalog: Arc<Catalog>,
    job: Job,
}
impl Drop for Permit {
    fn drop(&mut self) {
        self.catalog.lock().native_running.remove(&self.job);
        self.catalog.wake.notify_all();
    }
}

/// At most as many source stamps as the disk cache's query descriptors.
/// Events for an unobserved source cannot affect an active collection.
#[derive(Default)]
pub(super) struct Epochs {
    serial: u64,
    sources: VecDeque<(String, (u64, u64))>,
}
impl Epochs {
    fn read(&mut self, plugin: &str) -> (u64, u64) {
        let entry = self
            .sources
            .iter()
            .position(|(id, _)| id == plugin)
            .and_then(|at| self.sources.remove(at))
            .unwrap_or_else(|| {
                self.serial += 1;
                (plugin.to_string(), (self.serial, self.serial))
            });
        let epochs = entry.1;
        if self.sources.len() == 32 {
            self.sources.pop_front();
        }
        self.sources.push_back(entry);
        epochs
    }
    pub(super) fn invalidate(&mut self, plugin: &str, personal_only: bool) {
        let Some((_, epochs)) = self.sources.iter_mut().find(|(id, _)| id == plugin) else {
            return;
        };
        self.serial += 1;
        if personal_only {
            epochs.1 = self.serial;
        } else {
            epochs.0 = self.serial;
        }
    }
}

impl Catalog {
    pub(crate) fn collection_epochs(&self, plugin: &str) -> (u64, u64) {
        self.lock().collection_epochs.read(plugin)
    }

    pub(crate) fn collection_permit(
        self: &Arc<Self>,
        plugin: &str,
        alive: impl Fn() -> bool,
    ) -> Option<Permit> {
        let job = Job::Shelf(format!("{plugin} _collection-rpc"));
        let mut inner = self.lock();
        while alive() {
            if inner.native_running.len() < super::fetch::NATIVE_JOBS
                && inner
                    .native_running
                    .values()
                    .filter(|running| *running == plugin)
                    .count()
                    < super::fetch::NATIVE_PER_PLUGIN
            {
                inner.native_running.insert(job.clone(), plugin.to_string());
                return Some(Permit {
                    catalog: self.clone(),
                    job,
                });
            }
            inner = self
                .wake
                .wait_timeout(inner, Duration::from_millis(100))
                .unwrap_or_else(|error| error.into_inner())
                .0;
        }
        None
    }

    pub(crate) fn collection_action(
        self: &Arc<Self>,
        plugin: &str,
        page: &Value,
        local: usize,
        lease: u64,
    ) -> Result<(Lease, Value), String> {
        // The broker serializes actions. Retired allocations are reclaimed
        // before another page can create a new batch of ephemeral entities.
        self.reap_collection_actions();
        validate(page)?;
        let item = page["items"]
            .get(local)
            .ok_or("The selected item is no longer available.")?;
        let key = format!("{plugin} _collection-action:{lease}");
        let mut shelf = Shelf {
            document: page.clone(),
            read_at: super::now(),
            epoch: 1,
            ..Default::default()
        };
        let mut inner = self.lock();
        if !inner.loaded {
            return Err("The source library is still loading. Please try again.".into());
        }
        let before = inner.store.allocation_watermarks();
        super::collection_page::ingest(&mut inner.store, plugin, &mut shelf, super::now());
        let after = inner.store.allocation_watermarks();
        let created_cards = (after.0..before.0).collect();
        let created_tracks = (after.1..before.1).collect();
        let host = &shelf.document["items"][local]["_host"];
        let response = json!({"lease":lease,"key":key,"trackRow":host["track_row"],"cardId":host["card_id"],
            "kind":item["kind"],"ref":item["ref"],"navigationKey":format!("{plugin} native:{}",item["ref"].as_str().unwrap_or_default())});
        let cards = shelf
            .cards
            .iter()
            .filter_map(|id| inner.store.cards.get(id))
            .map(|card| (card.plugin.clone(), card.identity.clone()))
            .collect::<Vec<_>>();
        let tracks = shelf
            .tracks
            .iter()
            .filter_map(|id| inner.store.tracks.get(id))
            .map(|track| track.path.clone())
            .collect::<Vec<_>>();
        let hold = self.hold_locked(&mut inner, cards, tracks);
        inner
            .selections
            .get_mut(&hold.id)
            .expect("new action roots")
            .shelves
            .insert(key.clone());
        shelf.document = Value::Null;
        inner.store.shelves.insert(key.clone(), shelf);
        inner.dirty = true;
        drop(inner);
        self.publish(false);
        Ok((
            Lease {
                id: lease,
                catalog: self.clone(),
                key,
                hold: Some(hold),
                created_cards,
                created_tracks,
            },
            response,
        ))
    }
}

#[cfg(test)]
pub(crate) fn ready_test_catalog() -> Arc<Catalog> {
    let catalog = Catalog::new();
    catalog.lock().loaded = true;
    catalog
}

/// Articles must not mutate catalog ownership or request an integrated index.
#[cfg(test)]
pub(crate) fn article_test_probe(catalog: &Arc<Catalog>) -> impl FnOnce() {
    let catalog = catalog.clone();
    let inner = catalog.lock();
    let cards = inner.store.cards.generation();
    let tracks = inner.store.tracks.generation();
    let shelves = inner.store.shelves.generation();
    let captures = catalog.index_captures.load(std::sync::atomic::Ordering::Relaxed);
    drop(inner);
    move || {
        let inner = catalog.lock();
        assert!(inner.store.cards.matches_generation(&cards));
        assert!(inner.store.tracks.matches_generation(&tracks));
        assert!(inner.store.shelves.matches_generation(&shelves));
        assert_eq!(catalog.index_captures.load(std::sync::atomic::Ordering::Relaxed), captures);
    }
}

#[cfg(test)]
mod tests {
    use super::Epochs;

    #[test]
    fn source_epochs_are_bounded_and_evicted_stamps_are_never_reused() {
        let mut epochs = Epochs::default();
        let first = epochs.read("first");
        for at in 0..1000 {
            epochs.invalidate(&format!("unopened:{at}"), false);
        }
        assert_eq!(epochs.sources.len(), 1);
        assert_eq!(epochs.read("first"), first);
        for at in 0..32 {
            epochs.read(&format!("source:{at}"));
        }
        assert_eq!(epochs.sources.len(), 32);
        let reopened = epochs.read("first");
        assert_ne!(reopened, first);
        assert_eq!(epochs.sources.len(), 32);
        epochs.invalidate("first", true);
        let personal = epochs.read("first");
        assert_eq!(personal.0, reopened.0);
        assert_ne!(personal.1, reopened.1);
        epochs.invalidate("first", false);
        assert_ne!(epochs.read("first").0, personal.0);
    }
    #[test]
    fn chosen_reference_cover_does_not_adopt_a_card_or_load_an_index() {
        let catalog=super::ready_test_catalog();
        catalog.choose_art(std::collections::HashMap::from([(("fixture".into(),"label:one".into()),"/tmp/chosen-cover.png".into())]));
        assert_eq!(catalog.chosen_art_reference("fixture","label:one").as_deref(),Some("/tmp/chosen-cover.png"));
        assert!(catalog.chosen_art_reference("fixture","label:other").is_none());
        let inner=catalog.lock();
        assert!(inner.store.cards.is_empty());
        assert!(inner.store.tracks.is_empty());
        assert!(inner.store.shelves.is_empty());
    }

}
