
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
        #[qproperty(QString, text)]
        #[qproperty(QString, scope)]
        #[qproperty(QString, source)]
        #[qproperty(i32, count)]
        type SearchModel = super::SearchRust;

        #[qinvokable]
        fn act(self: Pin<&mut SearchModel>, row: i32, act: &QString);
        #[qinvokable]
        fn menu_json(self: &SearchModel, row: i32) -> QString;
        #[qinvokable]
        fn track_ids_json(self: &SearchModel, row: i32) -> QString;
        #[qinvokable]
        fn refresh(self: Pin<&mut SearchModel>);
    }

    unsafe extern "RustQt" {
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut SearchModel>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut SearchModel>);
    }

    unsafe extern "RustQt" {
        #[qinvokable]
        #[cxx_override]
        fn data(self: &SearchModel, index: &QModelIndex, role: i32) -> QVariant;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &SearchModel) -> QHash_i32_QByteArray;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &SearchModel, parent: &QModelIndex) -> i32;
    }

    impl cxx_qt::Threading for SearchModel {}
    impl cxx_qt::Initialize for SearchModel {}
}

use crate::{app, registry, worker};
use chilpa_api::{Found, FoundRow, Leads};
use cxx_qt::{CxxQtType as _, Threading as _};
use cxx_qt_lib::{QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QUrl, QVariant};
use std::pin::Pin;

const ROLES: &[&str] = &["kind", "title", "detail", "art", "duration", "sourceIcon", "sourceName"];
const USER_ROLE: i32 = 256;
const LIMIT: usize = 12;

#[derive(Default)]
pub struct SearchRust {
    text: QString,
    scope: QString,
    source: QString,
    count: i32,
    found: Found,
    rows: Vec<FoundRow>,
    generation: u64,
}

fn json(value: serde_json::Value) -> QString {
    QString::from(value.to_string())
}

impl qobject::SearchModel {
    pub fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let Some(row) = self.rust().rows.get(index.row().max(0) as usize) else { return QVariant::default() };
        let text = |s: &str| QVariant::from(&QString::from(s));
        match role - USER_ROLE {
            0 => QVariant::from(&row.kind()),
            1 => text(row.title()),
            2 => text(&row.detail()),
            3 if row.artwork().is_empty() => QVariant::from(&QUrl::default()),
            3 => QVariant::from(&if row.artwork().starts_with("https://") { QUrl::from(row.artwork()) } else { QUrl::from_local_file(&QString::from(row.artwork())) }),
            5 => text(&row.source_icon()),
            6 => text(&row.source_name()),
            _ => text(&row.duration()),
        }
    }

    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        crate::format::roles(USER_ROLE, ROLES)
    }

    pub fn row_count(&self, _parent: &QModelIndex) -> i32 {
        self.rust().rows.len() as i32
    }

    pub fn act(self: Pin<&mut Self>, row: i32, act: &QString) {
        let leads = app::with(|api| api.act_on_found(&self.rust().rows, row.max(0) as usize, &act.to_string()));
        registry::navigation(move |mut nav| match leads {
            Leads::Screen(screen, id) => nav.as_mut().go(&QString::from(screen), id),
            Leads::Tab(screen, id) => nav.as_mut().open(&QString::from(screen), id),
            Leads::Artist(name) => nav.as_mut().open_artist(&QString::from(&name)),
            Leads::Credit(credit, place) => nav.as_mut().open_credit(&QString::from(&credit), place),
            Leads::Folder(dir) => nav.as_mut().open_folder(&QString::from(&dir)),
            Leads::Source(source) => nav.as_mut().go_key(&QString::from("source"), &QString::from(source)),
            Leads::Nowhere => {}
        });
    }

    pub fn menu_json(&self, row: i32) -> QString {
        json(self.rust().rows.get(row.max(0) as usize).map(|row| app::with(|api| api.found_menu(row))).into())
    }

    pub fn track_ids_json(&self, row: i32) -> QString {
        json(self.rust().rows.get(row.max(0) as usize).map(|row| app::with(|api| api.found_track_ids(row))).into())
    }

    fn ask(mut self: Pin<&mut Self>) {
        let generation = self.rust().generation + 1;
        self.as_mut().rust_mut().generation = generation;
        let text = self.rust().text.to_string();
        let source = self.rust().source.to_string();
        app::with(|api| api.search_source_catalogs(&text, &source));
        if text.trim().is_empty() || !source.split(',').any(|s| s == "all" || s == "local") {
            self.arrive(generation, Found::default());
            return;
        }
        let thread = self.qt_thread();
        worker::submit(move |reader| {
            let found = reader.find(&text, LIMIT);
            let _ = thread.queue(move |model| model.arrive(generation, found));
        });
    }

    fn arrive(mut self: Pin<&mut Self>, generation: u64, found: Found) {
        if generation != self.rust().generation {
            return;
        }
        self.as_mut().rust_mut().found = found;
        self.rebuild();
    }

    pub fn refresh(self: Pin<&mut Self>) { self.rebuild(); }
    fn rebuild(mut self: Pin<&mut Self>) {
        let rows = app::with(|api| api.unified_found_rows(&self.rust().found, &self.rust().scope.to_string()));
        if rows == self.rust().rows { return; }
        let count = rows.iter().filter(|row| row.kind() > 0).count() as i32;
        unsafe { self.as_mut().begin_reset_model() };
        self.as_mut().rust_mut().rows = rows;
        unsafe { self.as_mut().end_reset_model() };
        self.as_mut().set_count(count);
    }
}

impl cxx_qt::Initialize for qobject::SearchModel {
    fn initialize(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().scope = QString::from("all");
        self.as_mut().rust_mut().source = QString::from("all");
        self.as_mut().on_source_changed(|m| m.ask()).release();
        self.as_mut().on_text_changed(|m| m.ask()).release();
        self.as_mut().on_scope_changed(|m| m.rebuild()).release();
    }
}
