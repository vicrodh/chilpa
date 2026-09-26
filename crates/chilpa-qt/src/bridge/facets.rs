//! One column of the Library screen: its entries, with "All" first.
//!
//! It holds nothing of its own: on every move of `Library.rev` it rereads
//! its column from the shared state, which the worker filled.

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
        include!("cxx-qt-lib/qvector.h");
        type QVector_i32 = cxx_qt_lib::QVector<i32>;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[base = QAbstractListModel]
        #[qml_element]
        /// "year", "genre", "artist" or "album".
        #[qproperty(QString, column)]
        /// Bound to `Library.rev`.
        #[qproperty(i32, rev)]
        /// Text typed in this column's header.
        #[qproperty(QString, query)]
        /// Entries, "All" excluded: the number in the column's header.
        #[qproperty(i32, total)]
        type FacetModel = super::FacetRust;
    }

    unsafe extern "RustQt" {
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut FacetModel>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut FacetModel>);
        #[inherit]
        #[qsignal]
        #[cxx_name = "dataChanged"]
        fn data_changed(
            self: Pin<&mut FacetModel>,
            top_left: &QModelIndex,
            bottom_right: &QModelIndex,
            roles: &QVector_i32,
        );
        #[inherit]
        fn index(self: &FacetModel, row: i32, column: i32, parent: &QModelIndex) -> QModelIndex;
    }

    unsafe extern "RustQt" {
        #[qinvokable]
        #[cxx_override]
        fn data(self: &FacetModel, index: &QModelIndex, role: i32) -> QVariant;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &FacetModel) -> QHash_i32_QByteArray;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &FacetModel, parent: &QModelIndex) -> i32;
    }

    impl cxx_qt::Initialize for FacetModel {}
}

use crate::{app, browse};
use chilpa_api::FacetEntry;
use cxx_qt::CxxQtType as _;
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant, QVector};
use std::pin::Pin;

const ROLES: &[&str] = &["key", "label", "meta", "selected", "all"];
const USER_ROLE: i32 = 256;

#[derive(Default)]
pub struct FacetRust {
    column: QString,
    rev: i32,
    query: QString,
    total: i32,
    entries: Vec<FacetEntry>,
    /// The key chosen in this column; empty is "All".
    chosen: String,
    all_label: String,
}

impl qobject::FacetModel {
    pub fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let this = self.rust();
        let row = index.row().max(0) as usize;
        let text = |s: &str| QVariant::from(&QString::from(s));
        if row == 0 {
            return match role - USER_ROLE {
                0 => text(""),
                1 => text(&this.all_label),
                3 => QVariant::from(&this.chosen.is_empty()),
                4 => QVariant::from(&true),
                _ => text(""),
            };
        }
        let Some(e) = this.entries.get(row - 1) else { return QVariant::default() };
        match role - USER_ROLE {
            0 => text(&e.key),
            1 => text(&e.label),
            // An album shows its year; an artist, how many records.
            2 => match (this.column.to_string().as_str(), e.year) {
                ("album", Some(y)) => text(&y.to_string()),
                ("artist", _) => text(&e.albums.to_string()),
                _ => text(""),
            },
            3 => QVariant::from(&(e.key == this.chosen)),
            4 => QVariant::from(&false),
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
        self.rust().entries.len() as i32 + 1
    }

    fn reload(mut self: Pin<&mut Self>) {
        let column = self.rust().column.to_string();
        let query = self.rust().query.to_string().trim().to_lowercase();
        let (entries, chosen) = browse::read(|s| {
            let f = &s.filter;
            let pick = |list: &Vec<FacetEntry>| list.clone();
            match column.as_str() {
                "year" => (pick(&s.facets.years), f.year.map(|y| y.to_string())),
                "genre" => (pick(&s.facets.genres), f.genre.clone()),
                "artist" => (pick(&s.facets.artists), f.artist.clone()),
                "album" => (pick(&s.facets.albums), f.album.map(|a| a.to_string())),
                _ => (Vec::new(), None),
            }
        });
        let entries = chilpa_api::filter_facets(entries, &query);
        let chosen = chosen.unwrap_or_default();
        let n = entries.len().to_string();
        let all = app::with(|api| api.t_args("All ({})", &[&n]));
        if entries == self.rust().entries {
            // The same entries: only the choice moved, and the column keeps
            // its scroll position.
            {
                let mut this = self.as_mut().rust_mut();
                this.chosen = chosen;
                this.all_label = all;
            }
            self.as_mut().set_total(entries.len() as i32);
            let parent = QModelIndex::default();
            let last = self.rust().entries.len() as i32;
            let (top, bottom) = (self.index(0, 0, &parent), self.index(last, 0, &parent));
            let mut roles = QVector::<i32>::default();
            roles.append(USER_ROLE + 1);
            roles.append(USER_ROLE + 3);
            self.as_mut().data_changed(&top, &bottom, &roles);
            return;
        }
        // SAFETY: the reset is begun and ended around the swap, as Qt asks.
        unsafe { self.as_mut().begin_reset_model() };
        {
            let mut this = self.as_mut().rust_mut();
            this.entries = entries;
            this.chosen = chosen;
            this.all_label = all;
        }
        unsafe { self.as_mut().end_reset_model() };
        let total = self.rust().entries.len() as i32;
        self.as_mut().set_total(total);
    }
}

impl cxx_qt::Initialize for qobject::FacetModel {
    fn initialize(mut self: Pin<&mut Self>) {
        self.as_mut().on_rev_changed(|m| m.reload()).release();
        self.as_mut().on_column_changed(|m| m.reload()).release();
        self.as_mut().on_query_changed(|m| m.reload()).release();
    }
}
