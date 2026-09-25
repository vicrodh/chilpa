//! The one facade the whole window talks to.
//!
//! Every QObject the window instantiates lives on the GUI thread, and they
//! all need the same `chilpa_api::Chilpa`: one player, one library, one
//! queue. It is kept here, per thread, instead of inside one QObject that
//! the others would have to find.
//!
//! The rule that keeps this sound: nothing inside [`with`] touches Qt. A
//! closure reads or orders the facade and returns plain data; setting a
//! property, which emits a signal, which can run a binding that calls back
//! in here, happens after the closure returns. Breaking it panics on the
//! `RefCell` borrow the first time it happens, which is where it should be
//! found.

use std::cell::RefCell;

thread_local! {
    static API: RefCell<Option<chilpa_api::Chilpa>> = const { RefCell::new(None) };
}

/// Runs `f` with the facade, building it the first time it is asked for.
pub fn with<R>(f: impl FnOnce(&mut chilpa_api::Chilpa) -> R) -> R {
    API.with(|cell| {
        let mut slot = cell.borrow_mut();
        let api = slot.get_or_insert_with(chilpa_api::Chilpa::new);
        f(api)
    })
}

/// A msgid in the current language. The shorthand every model uses for the
/// labels it builds on this side.
pub fn t(msgid: &str) -> String {
    with(|api| api.t(msgid))
}
