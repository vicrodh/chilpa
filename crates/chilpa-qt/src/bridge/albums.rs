//! The rows of an album grid (frontend ADR-0002, `AlbumGrid`).
//!
//! A grid is a list of rows, not a GridView: the view recycles whole rows
//! and each row lays out a fixed number of cards, which is what keeps the
//! recycling cheap and what lets a quick view open between two rows. Each
//! row is its first album and how many it holds; a card reads its album by
//! index, passing `rev` so its bindings run again when the albums change.
//!
//! `columns` comes from the view's width. While it is zero nothing is laid
//! out, instead of one column that reflows a frame later.

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
        include!("cxx-qt-lib/qurl.h");
        type QUrl = cxx_qt_lib::QUrl;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[base = QAbstractListModel]
        #[qml_element]
        /// "recentlyPlayed" or "recentlyAdded".
        #[qproperty(QString, kind)]
        #[qproperty(i32, limit)]
        #[qproperty(i32, columns)]
        /// Zero lays out every row; a shelf asks for one.
        #[qproperty(i32, max_rows)]
        #[qproperty(i32, albums)]
        #[qproperty(i32, rev)]
        /// Bound to `Library.contentRev`: the library changed.
        #[qproperty(i32, content_rev)]
        type AlbumGridModel = super::AlbumGridRust;

        #[qinvokable]
        fn title(self: &AlbumGridModel, index: i32, rev: i32) -> QString;
        #[qinvokable]
        fn subtitle(self: &AlbumGridModel, index: i32, rev: i32) -> QString;
        #[qinvokable]
        fn art(self: &AlbumGridModel, index: i32, rev: i32) -> QUrl;
        #[qinvokable]
        fn album_id(self: &AlbumGridModel, index: i32, rev: i32) -> i64;

        /// Plays a record from its first track.
        #[qinvokable]
        fn play(self: &AlbumGridModel, index: i32);
    }

    unsafe extern "RustQt" {
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut AlbumGridModel>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut AlbumGridModel>);
    }

    unsafe extern "RustQt" {
        #[qinvokable]
        #[cxx_override]
        fn data(self: &AlbumGridModel, index: &QModelIndex, role: i32) -> QVariant;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &AlbumGridModel) -> QHash_i32_QByteArray;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &AlbumGridModel, parent: &QModelIndex) -> i32;
    }

    impl cxx_qt::Threading for AlbumGridModel {}
    impl cxx_qt::Initialize for AlbumGridModel {}
}

use crate::app;
use chilpa_api::Album;
use cxx_qt::{CxxQtType as _, Threading as _};
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QUrl, QVariant};
use std::pin::Pin;

const USER_ROLE: i32 = 256;

#[derive(Default)]
pub struct AlbumGridRust {
    kind: QString,
    limit: i32,
    columns: i32,
    max_rows: i32,
    albums: i32,
    rev: i32,
    content_rev: i32,
    list: Vec<Album>,
    rows: usize,
    pending: bool,
}

impl AlbumGridRust {
    fn get(&self, index: i32) -> Option<&Album> {
        self.list.get(usize::try_from(index).ok()?)
    }

    fn row_total(&self) -> usize {
        let cols = self.columns.max(0) as usize;
        if cols == 0 {
            return 0;
        }
        let rows = self.list.len().div_ceil(cols);
        match self.max_rows {
            n if n > 0 => rows.min(n as usize),
            _ => rows,
        }
    }
}

impl qobject::AlbumGridModel {
    pub fn title(&self, index: i32, _rev: i32) -> QString {
        self.rust().get(index).map(|a| QString::from(&a.title)).unwrap_or_default()
    }

    pub fn subtitle(&self, index: i32, _rev: i32) -> QString {
        self.rust().get(index).map(|a| QString::from(&a.artist)).unwrap_or_default()
    }

    pub fn art(&self, index: i32, _rev: i32) -> QUrl {
        match self.rust().get(index) {
            Some(a) if !a.artwork_path.is_empty() => QUrl::from_local_file(&QString::from(&a.artwork_path)),
            _ => QUrl::default(),
        }
    }

    pub fn album_id(&self, index: i32, _rev: i32) -> i64 {
        self.rust().get(index).map(|a| a.id).unwrap_or(0)
    }

    pub fn play(&self, index: i32) {
        let Some(id) = self.rust().get(index).map(|a| a.id) else { return };
        app::with(|api| {
            let paths: Vec<String> = api.album_track_rows(id).into_iter().map(|t| t.path).collect();
            api.play_from(&paths, 0);
        });
    }

    pub fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let this = self.rust();
        let cols = this.columns.max(1) as usize;
        let first = index.row().max(0) as usize * cols;
        match role - USER_ROLE {
            0 => QVariant::from(&(first as i32)),
            1 => QVariant::from(&(this.list.len().saturating_sub(first).min(cols) as i32)),
            _ => QVariant::default(),
        }
    }

    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut roles = QHash::<QHashPair_i32_QByteArray>::default();
        roles.insert(USER_ROLE, QByteArray::from("first"));
        roles.insert(USER_ROLE + 1, QByteArray::from("span"));
        roles
    }

    pub fn row_count(&self, _parent: &QModelIndex) -> i32 {
        self.rust().rows as i32
    }

    fn schedule(mut self: Pin<&mut Self>) {
        if self.rust().pending {
            return;
        }
        self.as_mut().rust_mut().pending = true;
        let _ = self.qt_thread().queue(|model| model.reload());
    }

    fn reload(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().pending = false;
        let limit = self.rust().limit.max(1) as usize;
        let list = match self.rust().kind.to_string().as_str() {
            "recentlyPlayed" => app::with(|api| api.recently_played_albums(limit)),
            "recentlyAdded" => app::with(|api| api.recently_added_albums(limit)),
            _ => Vec::new(),
        };
        let count = list.len() as i32;
        self.as_mut().rust_mut().list = list;
        self.as_mut().split_rows();
        self.as_mut().set_albums(count);
        let next = self.rev().wrapping_add(1);
        self.as_mut().set_rev(next);
    }

    /// Splits the albums into rows again. Only the row count depends on the
    /// width, so nothing is read again.
    fn split_rows(mut self: Pin<&mut Self>) {
        let rows = self.rust().row_total();
        // SAFETY: the reset is begun and ended around the swap, as Qt asks.
        unsafe { self.as_mut().begin_reset_model() };
        self.as_mut().rust_mut().rows = rows;
        unsafe { self.as_mut().end_reset_model() };
    }
}

impl cxx_qt::Initialize for qobject::AlbumGridModel {
    fn initialize(mut self: Pin<&mut Self>) {
        self.as_mut().on_kind_changed(|m| m.schedule()).release();
        self.as_mut().on_limit_changed(|m| m.schedule()).release();
        self.as_mut().on_content_rev_changed(|m| m.schedule()).release();
        self.as_mut().on_columns_changed(|m| m.split_rows()).release();
        self.as_mut().on_max_rows_changed(|m| m.split_rows()).release();
    }
}
