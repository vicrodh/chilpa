//! What can be done with records from wherever one is drawn: a card, its
//! quick view, its menu, the selection bar (frontend ADR-0007).
//!
//! The favorites and the selection live here, one of each for the window:
//! a record marked in one grid is marked in every other, and selecting in
//! one grid replaces what another had selected. `rev` moves with either,
//! for the bindings that read them.
//!
//! An action names a record and acts on the whole selection when that
//! record is part of it, or when it names none (zero), as the selection bar
//! does.

#[cxx_qt::bridge]
pub mod qobject {
    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(i32, rev)]
        #[qproperty(i32, selected_count)]
        type AlbumActions = super::AlbumActionsRust;

        #[qinvokable]
        fn favorite(self: &AlbumActions, id: i64, rev: i32) -> bool;
        #[qinvokable]
        fn set_favorite(self: Pin<&mut AlbumActions>, id: i64, on: bool);
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
        /// Zero makes a new playlist.
        #[qinvokable]
        fn add_to_playlist(self: &AlbumActions, id: i64, playlist: i64);
    }

    impl cxx_qt::Threading for AlbumActions {}
    impl cxx_qt::Initialize for AlbumActions {}
}

use crate::{app, registry};
use cxx_qt::{CxxQtType as _, Threading as _};
use std::collections::HashSet;
use std::pin::Pin;

#[derive(Default)]
pub struct AlbumActionsRust {
    rev: i32,
    selected_count: i32,
    favorites: HashSet<i64>,
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
    pub fn favorite(&self, id: i64, _rev: i32) -> bool {
        self.rust().favorites.contains(&id)
    }

    pub fn set_favorite(mut self: Pin<&mut Self>, id: i64, on: bool) {
        app::with(|api| api.set_album_favorite(id, on));
        let mut this = self.as_mut().rust_mut();
        if on {
            this.favorites.insert(id);
        } else {
            this.favorites.remove(&id);
        }
        self.bump();
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

    pub fn add_to_playlist(&self, id: i64, playlist: i64) {
        let paths = self.rust().paths(id);
        registry::playlists(move |mut lists| {
            let id = if playlist == 0 { lists.as_mut().create() } else { playlist };
            if id != 0 {
                app::with(|api| api.add_to_playlist(id, &paths));
                lists.changed();
            }
        });
    }

    fn bump(mut self: Pin<&mut Self>) {
        let count = self.rust().selection.len() as i32;
        self.as_mut().set_selected_count(count);
        let next = self.rev().wrapping_add(1);
        self.set_rev(next);
    }
}

impl cxx_qt::Initialize for qobject::AlbumActions {
    fn initialize(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().favorites = app::with(|api| api.favorite_albums());
        registry::register_album_actions(self.qt_thread());
    }
}
