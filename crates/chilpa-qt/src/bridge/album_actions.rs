//! What can be done with records from wherever one is drawn: a card, its
//! quick view, its menu, the selection bar (frontend ADR-0007).
//!
//! The selection lives here, one for the window: selecting in one grid
//! replaces what another had selected. `rev` moves with it, for the
//! bindings that read it.
//!
//! An action names a record and acts on the whole selection when that
//! record is part of it, or when it names none (zero), as the selection bar
//! does.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }
    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(i32, rev)]
        #[qproperty(i32, selected_count)]
        type AlbumActions = super::AlbumActionsRust;

        #[qinvokable]
        fn sources(self: &AlbumActions, id: i64, rev: i32) -> QString;
        #[qinvokable]
        fn selected(self: &AlbumActions, id: i64, rev: i32) -> bool;
        #[qinvokable]
        fn clear_selection(self: Pin<&mut AlbumActions>);
        #[qinvokable]
        fn play(self: &AlbumActions, id: i64, shuffled: bool);
        #[qinvokable]
        fn play_next(self: &AlbumActions, id: i64);
        #[qinvokable]
        fn queue(self: &AlbumActions, id: i64);
        #[qinvokable]
        fn track_ids_json(self: &AlbumActions, id: i64) -> QString;
        /// Whether the record opens: some track of it plays now (ADR-017).
        #[qinvokable]
        fn available(self: &AlbumActions, id: i64, rev: i32) -> bool;
        /// Says why it does not, where the window shows problems.
        #[qinvokable]
        fn explain(self: &AlbumActions, id: i64);
    }

    impl cxx_qt::Threading for AlbumActions {}
    impl cxx_qt::Initialize for AlbumActions {}
}

use crate::{app, registry};
use cxx_qt::{CxxQtType as _, Threading as _};
use std::pin::Pin;

#[derive(Default)]
pub struct AlbumActionsRust {
    rev: i32,
    selected_count: i32,
    /// In the order it was picked, which is the order it plays in.
    selection: Vec<i64>,
}

impl AlbumActionsRust {
    fn targets(&self, id: i64) -> Vec<i64> {
        if id == 0 || self.selection.contains(&id) {
            self.selection.clone()
        } else {
            vec![id]
        }
    }

    fn paths(&self, id: i64) -> Vec<String> {
        let ids = self.targets(id);
        app::with(|api| api.album_paths(&ids))
    }
}

impl qobject::AlbumActions {
    pub fn available(&self, id: i64, _rev: i32) -> bool {
        app::with(|api| api.album_available(id))
    }

    pub fn explain(&self, id: i64) {
        app::with(|api| api.explain_unavailable_album(id));
    }

    pub fn sources(&self, id: i64, _rev: i32) -> cxx_qt_lib::QString {
        cxx_qt_lib::QString::from(app::with(|api| serde_json::to_string(&api.album_sources(id)).unwrap_or_default()))
    }

    pub fn selected(&self, id: i64, _rev: i32) -> bool {
        self.rust().selection.contains(&id)
    }

    /// Flips each record in or out of the selection, or makes it exactly
    /// those records.
    pub fn select(mut self: Pin<&mut Self>, ids: Vec<i64>, toggle: bool) {
        let mut this = self.as_mut().rust_mut();
        if !toggle {
            this.selection = ids;
        } else {
            for id in ids {
                match this.selection.iter().position(|s| *s == id) {
                    Some(at) => drop(this.selection.remove(at)),
                    None => this.selection.push(id),
                }
            }
        }
        self.bump();
    }

    pub fn clear_selection(self: Pin<&mut Self>) {
        if !self.rust().selection.is_empty() {
            self.select(Vec::new(), false);
        }
    }

    pub fn play(&self, id: i64, shuffled: bool) {
        let paths = self.rust().paths(id);
        app::with(|api| {
            api.set_shuffle(shuffled);
            api.play_from(&paths, 0);
        });
    }

    pub fn play_next(&self, id: i64) {
        let paths = self.rust().paths(id);
        app::with(|api| api.play_next_here(&paths));
    }

    pub fn queue(&self, id: i64) {
        let paths = self.rust().paths(id);
        app::with(|api| api.enqueue_here(&paths));
    }

    pub fn track_ids_json(&self, id: i64) -> cxx_qt_lib::QString {
        let ids: Vec<i64> = self.rust().targets(id).into_iter().flat_map(|album| app::with(|api| api.album_track_rows(album))).map(|track| track.id).collect();
        cxx_qt_lib::QString::from(serde_json::to_string(&ids).unwrap_or_default())
    }

    fn bump(mut self: Pin<&mut Self>) {
        let count = self.rust().selection.len() as i32;
        self.as_mut().set_selected_count(count);
        let next = self.rev().wrapping_add(1);
        self.set_rev(next);
    }
}

impl cxx_qt::Initialize for qobject::AlbumActions {
    fn initialize(self: Pin<&mut Self>) {
        registry::register_album_actions(self.qt_thread());
    }
}
