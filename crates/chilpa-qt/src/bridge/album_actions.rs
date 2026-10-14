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

        #[qinvokable] fn information(self: &AlbumActions, id: i64, rev: i32) -> QString;
        #[qinvokable] fn artist_information(self: &AlbumActions, id: i64, rev: i32) -> QString;
        #[qinvokable]
        fn sources(self: &AlbumActions, id: i64, rev: i32) -> QString;
        #[qinvokable]
        fn quality(self: &AlbumActions, id: i64, rev: i32) -> QString;
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
        /// Puts the record, or the selection, after what is queued next, as
        /// a group with that label.
        #[qinvokable]
        fn play_later(self: &AlbumActions, id: i64, label: &QString);
        /// Plays these records, in this order, as a JSON list of ids.
        #[qinvokable]
        fn play_many(self: &AlbumActions, ids_json: &QString, shuffled: bool);
        /// The extra queues a record can go to, as menu entries.
        #[qinvokable]
        fn listen_targets_json(self: &AlbumActions) -> QString;
        /// Sends the record, or the selection, to an extra queue (`buffer:…`).
        #[qinvokable]
        fn list_action(self: &AlbumActions, id: i64, action: &QString);
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
}

impl qobject::AlbumActions {
    pub fn quality(&self, id: i64, _rev: i32) -> cxx_qt_lib::QString {
        app::with(|api| {
            let quality = api.album_card_quality(id);
            cxx_qt_lib::QString::from(serde_json::json!({"tier":quality.tier.rank(),"label":quality.long()}).to_string())
        })
    }
    pub fn available(&self, id: i64, _rev: i32) -> bool {
        app::with(|api| api.album_available(id))
    }

    pub fn explain(&self, id: i64) {
        app::with(|api| api.explain_unavailable_album(id));
    }

    pub fn information(&self, id: i64, _rev: i32) -> cxx_qt_lib::QString { cxx_qt_lib::QString::from(app::with(|api| api.album_information(id))) }
    pub fn artist_information(&self, id: i64, _rev: i32) -> cxx_qt_lib::QString { cxx_qt_lib::QString::from(app::with(|api| api.album_artist_information(id))) }

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

    /// Plays, puts next or queues the record, or the selection it is in.
    fn act(&self, id: i64, act: &str, shuffled: bool) {
        let ids = self.rust().targets(id);
        self.act_on(ids, act.to_string(), shuffled, String::new());
    }

    fn act_on(&self, ids: Vec<i64>, action: String, shuffled: bool, label: String) {
        chilpa_qt_kit::bridge::queue_activity::albums(ids, action, shuffled, label);
    }

    pub fn play(&self, id: i64, shuffled: bool) { self.act(id, "play", shuffled) }
    pub fn play_next(&self, id: i64) { self.act(id, "next", false) }
    pub fn queue(&self, id: i64) { self.act(id, "queue", false) }
    pub fn play_many(&self, ids_json: &cxx_qt_lib::QString, shuffled: bool) {
        let ids: Vec<i64> = serde_json::from_str(&ids_json.to_string()).unwrap_or_default();
        self.act_on(ids, "play".into(), shuffled, String::new());
    }
    pub fn play_later(&self, id: i64, label: &cxx_qt_lib::QString) {
        let ids = self.rust().targets(id);
        self.act_on(ids, "later".into(), false, label.to_string());
    }
    pub fn listen_targets_json(&self) -> cxx_qt_lib::QString {
        cxx_qt_lib::QString::from(serde_json::to_string(&app::with(|api| api.listen_targets())).unwrap_or_default())
    }
    pub fn list_action(&self, id: i64, action: &cxx_qt_lib::QString) {
        let ids = self.rust().targets(id);
        self.act_on(ids, action.to_string(), false, String::new());
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
