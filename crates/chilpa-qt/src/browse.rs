//! What the Library screen has chosen and what it leaves, shared between
//! the objects that show it.
//!
//! The `Library` singleton changes the filter and asks the worker for the
//! facets and the snapshot; the four facet columns and the track list read
//! the result from here when `rev` moves. Everything lives on the window's
//! thread, so a `RefCell` is enough, and an `Rc` lets a list hold the
//! snapshot without copying fifty thousand ids.

use chilpa_api::{Facets, Filter, Snapshot, TrackOrder};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Default)]
pub struct State {
    pub filter: Filter,
    pub order: TrackOrder,
    pub facets: Rc<Facets>,
    pub snapshot: Rc<Snapshot>,
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
