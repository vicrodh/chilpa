//! I borrow the running facade's catalog without owning another player.
use std::{cell::{Cell, RefCell}, collections::BTreeSet, sync::{Arc,Weak}};
use crate::{source_catalog::Catalog, Reader};
use super::worker::Suggester;

/// The thread answering a facade thread's suggestion rounds, the lists it
/// answers, and the catalog it answers with.
struct Answering { worker: Suggester, consumers: BTreeSet<u64>, catalog: Weak<Catalog> }

thread_local! {
    // One weak handle per facade thread; I retain no catalog or query here.
    static CATALOG: RefCell<Weak<Catalog>> = RefCell::new(Weak::new());
    // Started by the first round; closed by the last list to go, and kept
    // so the next round brings its thread back or waits for it to leave.
    static SUGGESTER: RefCell<Option<Answering>> = const { RefCell::new(None) };
    static CONSUMERS: Cell<u64> = const { Cell::new(0) };
}

pub(crate) fn attach(catalog: &Arc<Catalog>) {
    CATALOG.with(|held| {
        let same = held.borrow().upgrade().is_some_and(|known| Arc::ptr_eq(&known, catalog));
        // Another facade's catalog: the thread answering with the old one
        // answers nothing to what waits, and the next round replaces it.
        if !same {
            SUGGESTER.with(|slot| if let Some(answering) = slot.borrow_mut().as_mut() { answering.worker.close(); answering.catalog = Weak::new(); });
        }
        *held.borrow_mut()=Arc::downgrade(catalog);
    });
}

impl Reader {
    /// I open one read-only connection per request and release it on return.
    pub fn for_refine() -> Option<Self> {
        let catalog = CATALOG.with(|held| held.borrow().upgrade())?;
        let mut reader = Self::open(&chilpa_library::Library::default_path()).ok()?;
        reader.catalog=Some(catalog); Some(reader)
    }
}

/// A list that will ask suggestion rounds: the handle its rounds carry, and
/// that `suggest_release` takes back when the list goes.
pub fn suggest_consumer() -> u64 {
    CONSUMERS.with(|count| { let id = count.get() + 1; count.set(id); id })
}

/// I ask a suggestion round of the facade's answering thread for `consumer`;
/// `deliver` gets the answer there, or never when the consumer asked again
/// before it started. Without a facade on this thread, or when the thread
/// cannot start, it gets the empty answer before this returns, as a request
/// without a reader does. The caller's thread never waits.
pub fn suggest_later(consumer: u64, query: &str, typed: &str, deliver: impl FnOnce(serde_json::Value) + Send + 'static) {
    let catalog = CATALOG.with(|held| held.borrow().clone());
    if catalog.upgrade().is_none() { return deliver(super::data::nothing()); }
    SUGGESTER.with(|slot| {
        let mut slot = slot.borrow_mut();
        // The thread closed by the last list, brought back while it is
        // there; gone, or another facade's, the next waits for it to leave.
        let back = slot.as_ref().is_some_and(|answering| Weak::ptr_eq(&answering.catalog, &catalog) && answering.worker.revive());
        if !back {
            let previous = slot.take().map(|answering| answering.worker);
            *slot = Suggester::start(chilpa_library::Library::default_path(), catalog.clone(), previous)
                .map_err(|e| eprintln!("chilpa-suggest: could not start the thread: {e}; will retry on the next round")).ok()
                .map(|worker| Answering { worker, consumers: BTreeSet::new(), catalog });
        }
        match slot.as_mut() {
            Some(answering) => { answering.consumers.insert(consumer); answering.worker.ask(consumer, query, typed, deliver) }
            None => deliver(super::data::nothing()),
        }
    });
}

/// The list is gone: the round it had waiting is dropped, and with the last
/// list the thread closes, letting its reader and what the reader kept go.
pub fn suggest_release(consumer: u64) {
    SUGGESTER.with(|slot| {
        if let Some(answering) = slot.borrow_mut().as_mut() {
            answering.worker.forget(consumer);
            answering.consumers.remove(&consumer);
            if answering.consumers.is_empty() { answering.worker.close(); }
        }
    });
}

/// Whether this thread's answering thread takes rounds now.
#[cfg(test)]
fn answering() -> bool { SUGGESTER.with(|slot| slot.borrow().as_ref().is_some_and(|answering| answering.worker.open())) }

#[cfg(test)]
mod tests {
    use super::*;

    fn ask(consumer: u64) { suggest_later(consumer, "", " ", |_| {}); }

    #[test]
    fn the_thread_closes_with_the_last_list_and_with_another_facades_catalog() {
        let first = Catalog::new();
        attach(&first);
        let (one, two) = (suggest_consumer(), suggest_consumer());
        assert_ne!(one, two);
        ask(one); ask(two);
        assert!(answering(), "the first round started the thread");
        suggest_release(one);
        assert!(answering(), "a list is still open");
        suggest_release(two);
        assert!(!answering(), "the last list closed it");
        ask(one);
        assert!(answering(), "a round brings it back or replaces it");
        attach(&first);
        assert!(answering(), "the same catalog again changes nothing");
        let second = Catalog::new();
        attach(&second);
        assert!(!answering(), "the thread held the first catalog; the next round starts one with the second");
        ask(one);
        assert!(answering());
        suggest_release(one);
        assert!(!answering());
    }
}
