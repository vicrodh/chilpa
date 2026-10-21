//! A Library facet, with "All" first, reread from shared state on `rev`.

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
        /// "year", "genre", "label", "composer", "rating", "artist" or "album".
        #[qproperty(QString, column)]
        /// Bound to `Library.rev`.
        #[qproperty(i32, rev)]
        #[qproperty(QString, applied_query)]
        /// Text typed in this column's header.
        #[qproperty(QString, query)]
        /// Entries, "All" excluded: the number in the column's header.
        #[qproperty(i32, total)]
        type FacetModel = super::FacetRust;

        /// Toggle inclusion, or select a Shift range. Alt toggles exclusion.
        #[qinvokable] fn choose(self: Pin<&mut FacetModel>, row: i32, modifiers: i32);
        #[qinvokable] fn exclude(self: Pin<&mut FacetModel>, row: i32);
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
use chilpa_api::{FacetEntry, library_query::columns::{self, ColumnSelection}};
use cxx_qt::CxxQtType as _;
use cxx_qt_lib::{QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant, QVector};
use std::pin::Pin;

const ROLES: &[&str] = &["key", "label", "meta", "selected", "all", "albums", "excluded"];
const USER_ROLE: i32 = 256;

#[derive(Default)]
pub struct FacetRust {
    column: QString,
    rev: i32,
    applied_query: QString,
    query: QString,
    total: i32,
    entries: Vec<FacetEntry>,
    selection: ColumnSelection, anchor: Option<usize>,
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
                3 => QVariant::from(&!this.selection.active),
                4 => QVariant::from(&true),
                6 => QVariant::from(&false),
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
            3 => QVariant::from(&this.selection.included.contains(&e.key)),
            4 => QVariant::from(&false),
            5 => text(&e.albums.to_string()),
            6 => QVariant::from(&this.selection.excluded.contains(&e.key)),
            _ => QVariant::default(),
        }
    }

    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        crate::format::roles(USER_ROLE, ROLES)
    }

    pub fn row_count(&self, _parent: &QModelIndex) -> i32 {
        self.rust().entries.len() as i32 + 1
    }

    fn reload(mut self: Pin<&mut Self>) {
        let column = self.rust().column.to_string();
        let query = self.rust().query.to_string().trim().to_lowercase();
        let (entries, selection) = browse::read(|s| {
            let entries = chilpa_api::filter_facets(columns::entries(&s.facets, &column), &query);
            let selection = columns::selection(&s.filter, &column, &entries);
            (entries, selection)
        });
        let n = entries.len().to_string();
        let all = app::with(|api| api.t_args("All ({})", &[&n]));
        if entries == self.rust().entries {
            // The same entries: only the choice moved, and the column keeps
            // its scroll position.
            {
                let mut this = self.as_mut().rust_mut();
                this.selection = selection;
                this.all_label = all;
            }
            self.as_mut().set_total(entries.len() as i32);
            let parent = QModelIndex::default();
            let last = self.rust().entries.len() as i32;
            let (top, bottom) = (self.index(0, 0, &parent), self.index(last, 0, &parent));
            let mut roles = QVector::<i32>::default();
            roles.append(USER_ROLE + 1);
            roles.append(USER_ROLE + 3);
            roles.append(USER_ROLE + 6);
            self.as_mut().data_changed(&top, &bottom, &roles);
            return;
        }
        // SAFETY: the reset is begun and ended around the swap, as Qt asks.
        unsafe { self.as_mut().begin_reset_model() };
        {
            let mut this = self.as_mut().rust_mut();
            this.entries = entries;
            this.selection = selection;
            this.all_label = all;
            this.anchor = None;
        }
        unsafe { self.as_mut().end_reset_model() };
        let total = self.rust().entries.len() as i32;
        self.as_mut().set_total(total);
    }

    pub fn choose(mut self: Pin<&mut Self>, row: i32, modifiers: i32) {
        if row < 0 { return; }
        let column = self.rust().column.to_string();
        let anchor = browse::write(|s| columns::choose(&mut s.filter, &column, &self.rust().entries, self.rust().anchor, row as usize, modifiers));
        self.as_mut().rust_mut().anchor = anchor;
        crate::registry::library(|library| library.query_changed());
    }

    pub fn exclude(self: Pin<&mut Self>, row: i32) { self.choose(row, 4); }

}

impl cxx_qt::Initialize for qobject::FacetModel {
    fn initialize(mut self: Pin<&mut Self>) {
        self.as_mut().on_rev_changed(|m| m.reload()).release();
        self.as_mut().on_applied_query_changed(|m| m.reload()).release();
        self.as_mut().on_column_changed(|m| m.reload()).release();
        self.as_mut().on_query_changed(|m| m.reload()).release();
    }
}
