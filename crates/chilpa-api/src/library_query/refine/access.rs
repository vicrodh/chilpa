//! I borrow the running facade's catalog without owning another player.
use std::{cell::{Cell, RefCell}, collections::BTreeSet, sync::{Arc,Weak}};
use crate::{source_catalog::Catalog, Reader};
use super::worker::Suggester;

thread_local! {
    // One weak handle per facade thread; I retain no catalog or query here.
    static CATALOG: RefCell<Weak<Catalog>> = RefCell::new(Weak::new());
    // The thread answering this facade thread's suggestion rounds, and the
    // lists it answers: it starts with the first round and goes with the
    // last list, or with the facade.
    static SUGGESTER: RefCell<Option<(Suggester, BTreeSet<u64>)>> = const { RefCell::new(None) };
    static CONSUMERS: Cell<u64> = const { Cell::new(0) };
}

pub(crate) fn attach(catalog: &Arc<Catalog>) {
    CATALOG.with(|held| {
        // Another facade's catalog: the thread answering with the old one ends.
        let same = held.borrow().upgrade().is_some_and(|known| Arc::ptr_eq(&known, catalog));
        if !same { SUGGESTER.with(|slot| slot.borrow_mut().take()); }
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
        if slot.is_none() {
            *slot = Suggester::start(chilpa_library::Library::default_path(), catalog)
                .map_err(|e| eprintln!("chilpa-suggest: could not start the thread: {e}; will retry on the next round")).ok()
                .map(|worker| (worker, BTreeSet::new()));
        }
        match slot.as_mut() {
            Some((worker, consumers)) => { consumers.insert(consumer); worker.ask(consumer, query, typed, deliver) }
            None => deliver(super::data::nothing()),
        }
    });
}

/// The list is gone: the round it had waiting is dropped, and with the last
/// list the thread, its reader and what the reader kept go too.
pub fn suggest_release(consumer: u64) {
    SUGGESTER.with(|slot| {
        let mut slot = slot.borrow_mut();
        let last = match slot.as_mut() {
            Some((worker, consumers)) => { worker.forget(consumer); consumers.remove(&consumer); consumers.is_empty() }
            None => false,
        };
        if last { *slot = None; }
    });
}

#[cfg(test)]
fn answering() -> bool { SUGGESTER.with(|slot| slot.borrow().is_some()) }

#[cfg(test)]
mod tests {
    use super::*;

    fn ask(consumer: u64) { suggest_later(consumer, "", " ", |_| {}); }

    #[test]
    fn the_thread_goes_with_the_last_list_and_with_another_facades_catalog() {
        let first = Catalog::new();
        attach(&first);
        let (one, two) = (suggest_consumer(), suggest_consumer());
        assert_ne!(one, two);
        ask(one); ask(two);
        assert!(answering(), "the first round started the thread");
        suggest_release(one);
        assert!(answering(), "a list is still open");
        suggest_release(two);
        assert!(!answering(), "the last list took the thread with it");
        ask(one);
        assert!(answering());
        attach(&first);
        assert!(answering(), "the same catalog again changes nothing");
        let second = Catalog::new();
        attach(&second);
        assert!(!answering(), "the thread held the first catalog; the next round starts one with the second");
        ask(one);
        assert!(answering());
        suggest_release(one);
    }
}
