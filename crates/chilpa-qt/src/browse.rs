//! I share the Library filter, facets, snapshot and bounded page caches
//! across its views. State lives on the UI thread; Rc avoids copying ids.

use chilpa_api::{Facets, Filter, Snapshot, TrackOrder};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Default)]
pub struct State {
    pub filter: Filter,
    pub order: TrackOrder,
    pub facets: Rc<Facets>,
    pub snapshot: Rc<Snapshot>,
    pub album_pages: Rc<RefCell<chilpa_api::PageCache<chilpa_api::Album>>>,
    pub track_pages: Rc<RefCell<chilpa_api::PageCache<chilpa_api::TrackRow>>>,
    pub page_revision: i32,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

pub fn read<R>(f: impl FnOnce(&State) -> R) -> R {
    STATE.with(|s| f(&s.borrow()))
}

pub fn write<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|s| f(&mut s.borrow_mut()))
}
