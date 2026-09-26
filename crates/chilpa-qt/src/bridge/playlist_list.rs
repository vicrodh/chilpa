//! The playlists as the sidebar lists them, newest first, narrowed by its
//! filter. Read on the worker when `rev` moves; filtered here, since the
//! list is a few hundred names at most.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!(<QtCore/QAbstractListModel>);
        type QAbstractListModel;
        include!("cxx-qt-lib/qhash.h");
        type QHash_i32_QByteArray = cxx_qt_lib::QHash<cxx_qt_lib::QHashPair_i32_QByteArray>;
        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
        include!("cxx-qt-lib/qmodelindex.h");
        type QModelIndex = cxx_qt_lib::QModelIndex;
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[base = QAbstractListModel]
        #[qml_element]
        /// Bound to `Playlists.rev`.
        #[qproperty(i32, rev)]
        /// Only the names that hold every word of it.
        #[qproperty(QString, filter)]
        #[qproperty(i32, count)]
        type PlaylistListModel = super::PlaylistListRust;
    }

    unsafe extern "RustQt" {
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut PlaylistListModel>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut PlaylistListModel>);
    }

    unsafe extern "RustQt" {
        #[qinvokable]
        #[cxx_override]
        fn data(self: &PlaylistListModel, index: &QModelIndex, role: i32) -> QVariant;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &PlaylistListModel) -> QHash_i32_QByteArray;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &PlaylistListModel, parent: &QModelIndex) -> i32;
    }

    impl cxx_qt::Threading for PlaylistListModel {}
    impl cxx_qt::Initialize for PlaylistListModel {}
}

use crate::worker;
use chilpa_api::Playlist;
use cxx_qt::{CxxQtType as _, Threading as _};
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant};
use std::pin::Pin;

const ROLES: &[&str] = &["playlistId", "name", "tracks"];
const USER_ROLE: i32 = 256;

#[derive(Default)]
pub struct PlaylistListRust {
    rev: i32,
    filter: QString,
    count: i32,
    all: Vec<Playlist>,
    shown: Vec<Playlist>,
    generation: u64,
}

impl qobject::PlaylistListModel {
    pub fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let Some(p) = self.rust().shown.get(index.row().max(0) as usize) else { return QVariant::default() };
        match role - USER_ROLE {
            0 => QVariant::from(&p.id),
            1 => QVariant::from(&QString::from(&p.name)),
            2 => QVariant::from(&(p.track_count as i32)),
            _ => QVariant::default(),
        }
    }

    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut roles = QHash::<QHashPair_i32_QByteArray>::default();
        for (i, name) in ROLES.iter().enumerate() {
            roles.insert(USER_ROLE + i as i32, QByteArray::from(*name));
        }
        roles
    }

    pub fn row_count(&self, _parent: &QModelIndex) -> i32 {
        self.rust().shown.len() as i32
    }

    fn load(mut self: Pin<&mut Self>) {
        let generation = self.rust().generation + 1;
        self.as_mut().rust_mut().generation = generation;
        let thread = self.qt_thread();
        worker::submit(move |reader| {
            let all = reader.local_playlists();
            let _ = thread.queue(move |model| model.arrive(generation, all));
        });
    }

    fn arrive(mut self: Pin<&mut Self>, generation: u64, all: Vec<Playlist>) {
        if generation == self.rust().generation {
            self.as_mut().rust_mut().all = all;
            self.show();
        }
    }

    fn show(mut self: Pin<&mut Self>) {
        let words: Vec<String> =
            self.rust().filter.to_string().to_lowercase().split_whitespace().map(String::from).collect();
        let shown: Vec<Playlist> = self
            .rust()
            .all
            .iter()
            .filter(|p| {
                let name = p.name.to_lowercase();
                words.iter().all(|w| name.contains(w.as_str()))
            })
            .cloned()
            .collect();
        let count = shown.len() as i32;
        // SAFETY: the reset is begun and ended around the swap, as Qt asks.
        unsafe { self.as_mut().begin_reset_model() };
        self.as_mut().rust_mut().shown = shown;
        unsafe { self.as_mut().end_reset_model() };
        self.as_mut().set_count(count);
    }
}

impl cxx_qt::Initialize for qobject::PlaylistListModel {
    fn initialize(mut self: Pin<&mut Self>) {
        self.as_mut().on_rev_changed(|m| m.load()).release();
        self.as_mut().on_filter_changed(|m| m.show()).release();
        self.load();
    }
}
