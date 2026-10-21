//! I borrow the running facade's catalog without owning another player.
use std::{cell::RefCell, sync::{Arc,Weak}};
use crate::{source_catalog::Catalog, Reader};

thread_local! {
    // One weak handle per facade thread; I retain no catalog or query here.
    static CATALOG: RefCell<Weak<Catalog>> = RefCell::new(Weak::new());
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
