//! I borrow the running facade's catalog without owning another player.
use std::{cell::RefCell, sync::{Arc,Weak}};
use crate::{source_catalog::Catalog, Reader};

thread_local! {
    // One weak handle per facade thread; I retain no catalog or query here.
    static CATALOG: RefCell<Weak<Catalog>> = RefCell::new(Weak::new());
    // The thread that answers suggestion rounds for this facade thread,
    // started by the first round and ended with the thread.
    static SUGGESTER: RefCell<Option<super::worker::Suggester>> = const { RefCell::new(None) };
}

pub(crate) fn attach(catalog: &Arc<Catalog>) { CATALOG.with(|held| *held.borrow_mut()=Arc::downgrade(catalog)); }

impl Reader {
    /// I open one read-only connection per request and release it on return.
    pub fn for_refine() -> Option<Self> {
        let catalog = CATALOG.with(|held| held.borrow().upgrade())?;
        let mut reader = Self::open(&chilpa_library::Library::default_path()).ok()?;
        reader.catalog=Some(catalog); Some(reader)
    }
}

/// I ask a suggestion round of the facade's answering thread; `deliver`
/// gets the answer there, or never when a later round replaced this one
/// before it started. Without a facade on this thread, or when the thread
/// cannot start, it gets the empty answer before this returns, as a
/// request without a reader does. The caller's thread never waits.
pub fn suggest_later(query: &str, typed: &str, deliver: impl FnOnce(serde_json::Value) + Send + 'static) {
    let catalog = CATALOG.with(|held| held.borrow().clone());
    if catalog.upgrade().is_none() { return deliver(super::data::nothing()); }
    SUGGESTER.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_none() {
            *slot = super::worker::Suggester::start(chilpa_library::Library::default_path(), catalog)
                .map_err(|e| eprintln!("chilpa-suggest: could not start the thread: {e}; will retry on the next round")).ok();
        }
        match slot.as_ref() { Some(worker) => worker.ask(query, typed, deliver), None => deliver(super::data::nothing()) }
    });
}
