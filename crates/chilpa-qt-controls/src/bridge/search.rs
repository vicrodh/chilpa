
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
        #[qproperty(QString, quality_filter)]
        #[qproperty(bool, hide_explicit)]
        #[qproperty(i32, count)]
        #[qproperty(bool, selection_pending)]
        #[qproperty(QString, selection_error)]
        type SearchModel = super::SearchRust;

        #[qinvokable]
        fn act(self: Pin<&mut SearchModel>, row: i32, act: &QString);
        #[qinvokable]
        fn menu_json(self: Pin<&mut SearchModel>, row: i32) -> QString;
        #[qinvokable]
        fn track_ids_json(self: &SearchModel, row: i32) -> QString;
        #[qinvokable]
        fn refresh(self: Pin<&mut SearchModel>);
        #[qinvokable]
        fn menu_act(self: Pin<&mut SearchModel>, act: &QString);
        #[qinvokable]
        fn release_menu(self: Pin<&mut SearchModel>);
        #[qinvokable]
        fn pick_menu(self: Pin<&mut SearchModel>);
        #[qinvokable]
        fn poll_selection(self: Pin<&mut SearchModel>);
        #[qinvokable]
        fn cancel_selection(self: Pin<&mut SearchModel>);
        #[qsignal]
        fn playlist_ready(self: Pin<&mut SearchModel>, tracks: QString);
        #[qinvokable]
        fn release_navigation(self: Pin<&mut SearchModel>, token: &QString);
        #[qsignal]
        fn navigated(self: Pin<&mut SearchModel>, kind: QString, target: QString, id: i64, token: QString);
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

use chilpa_qt_kit::{app, worker};
use chilpa_api::{Found, FoundRow, FoundSelection, Leads, SelectionState};
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
    quality_filter: QString,
    hide_explicit: bool,
    count: i32,
    found: Found,
    rows: Vec<FoundRow>,
    generation: u64,
    menu: Option<(Vec<FoundRow>, usize)>,
    menu_hold: Option<chilpa_api::SelectionHold>,
    navigation_serial: u64,
    navigation_holds: std::collections::HashMap<u64, chilpa_api::SelectionHold>,
    selection: Option<FoundSelection>,
    selection_pending: bool,
    selection_error: QString,
}

impl SearchRust {
    fn keep_navigation(&mut self, hold: chilpa_api::SelectionHold) -> u64 {
        self.navigation_serial = self.navigation_serial.checked_add(1).expect("navigation identifiers exhausted");
        self.navigation_holds.insert(self.navigation_serial, hold);
        self.navigation_serial
    }
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
        chilpa_qt_kit::format::roles(USER_ROLE, ROLES)
    }

    pub fn row_count(&self, _parent: &QModelIndex) -> i32 {
        self.rust().rows.len() as i32
    }

    pub fn act(self: Pin<&mut Self>, row: i32, act: &QString) {
        let at = row.max(0) as usize;
        let (leads, hold) = app::with(|api| {
            let hold = self.rust().rows.get(at).map(|row| api.hold_found_menu(row));
            (api.act_on_found(&self.rust().rows, at, &act.to_string()), hold)
        });
        self.publish_lead(leads, hold);
    }

    fn publish_lead(mut self: Pin<&mut Self>, lead: Leads, hold: Option<chilpa_api::SelectionHold>) {
        let (kind, target, id) = match lead {
            Leads::Screen(screen, id) => ("screen", screen.to_string(), id),
            Leads::Tab(screen, id) => ("tab", screen.to_string(), id),
            Leads::Artist(name) => ("artist", name, 0),
            Leads::Credit(credit, place) => ("credit", credit, i64::from(place)),
            Leads::Folder(path) => ("folder", path, 0),
            Leads::Source(key) => ("source", key, 0),
            Leads::Nowhere => return,
        };
        let Some(hold) = hold else { return };
        let token = self.as_mut().rust_mut().keep_navigation(hold);
        self.navigated(QString::from(kind), QString::from(&target), id, QString::from(token.to_string()));
    }

    pub fn release_navigation(self: Pin<&mut Self>, token: &QString) {
        if let Ok(token) = token.to_string().parse::<u64>() { self.rust_mut().navigation_holds.remove(&token); }
    }

    pub fn menu_json(mut self: Pin<&mut Self>, row: i32) -> QString {
        let at = usize::try_from(row).ok().filter(|at| *at < self.rust().rows.len());
        let menu = at.map(|at| app::with(|api| api.found_menu(&self.rust().rows[at]))).unwrap_or_default();
        let snapshot = at.map(|at| (self.rust().rows.clone(), at));
        let hold = at.map(|at| app::with(|api| api.hold_found_menu(&self.rust().rows[at])));
        self.as_mut().rust_mut().menu_hold = hold;
        self.as_mut().rust_mut().menu = snapshot;
        json(menu.into())
    }

    pub fn release_menu(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().menu = None;
        self.rust_mut().menu_hold = None;
    }

    pub fn menu_act(self: Pin<&mut Self>, act: &QString) {
        let result = self.rust().menu.as_ref().map(|(rows, at)| app::with(|api| {
            let hold = rows.get(*at).map(|row| api.hold_found_menu(row));
            (api.act_on_found(rows, *at, &act.to_string()), hold)
        }));
        if let Some((lead, hold)) = result { self.publish_lead(lead, hold); }
    }

    pub fn pick_menu(mut self: Pin<&mut Self>) {
        let result = self.rust().menu.as_ref().and_then(|(rows, at)| rows.get(*at))
            .map(|row| app::with(|api| api.found_selection(row)));
        self.as_mut().cancel_selection();
        match result {
            Some(Ok(selection)) => {
                self.as_mut().rust_mut().selection = Some(selection);
                self.as_mut().set_selection_pending(true);
                self.poll_selection();
            }
            Some(Err(error)) => self.set_selection_error(QString::from(&error)),
            None => {}
        }
    }

    pub fn poll_selection(mut self: Pin<&mut Self>) {
        let state = self.as_mut().rust_mut().selection.as_mut().map(FoundSelection::poll);
        match state {
            Some(SelectionState::Ready(ids)) => {
                // The synchronous signal transfers protection to the picker's
                // singleton before I release this request's catalog hold.
                self.as_mut().playlist_ready(json(ids.into()));
                self.cancel_selection();
            }
            Some(SelectionState::Failed(error)) => {
                self.as_mut().cancel_selection();
                self.set_selection_error(QString::from(&error));
            }
            _ => {}
        }
    }

    pub fn cancel_selection(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().selection = None;
        self.as_mut().set_selection_pending(false);
        self.set_selection_error(QString::default());
    }

    pub fn track_ids_json(&self, row: i32) -> QString {
        json(self.rust().rows.get(row.max(0) as usize).map(|row| app::with(|api| api.found_track_ids(row))).into())
    }

    fn ask(mut self: Pin<&mut Self>) {
        let generation = self.rust().generation + 1;
        self.as_mut().rust_mut().generation = generation;
        let text = self.rust().text.to_string();
        let source = self.rust().source.to_string();
        app::with(|api| api.search_source_catalogs_filtered(&text, &source, &self.rust().quality_filter.to_string(), self.rust().hide_explicit));
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
        self.as_mut().on_quality_filter_changed(|m| m.ask()).release();
        self.as_mut().on_hide_explicit_changed(|m| m.ask()).release();
        self.as_mut().on_scope_changed(|m| m.rebuild()).release();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_navigations_have_separate_roots_after_the_menu_closes() {
        let api = chilpa_api::Chilpa::new();
        let mut state = SearchRust::default();
        let row = FoundRow::Header("Fixture".into());
        state.menu_hold = Some(api.hold_found_menu(&row));
        let first = state.keep_navigation(api.hold_found_menu(&row));
        let second = state.keep_navigation(api.hold_found_menu(&row));
        state.menu_hold = None;
        assert_ne!(first, second);
        assert_eq!(state.navigation_holds.len(), 2);
        state.navigation_holds.remove(&second);
        assert!(state.navigation_holds.contains_key(&first), "a later callback must not release an earlier request");
        state.navigation_holds.remove(&first);
        assert!(state.navigation_holds.is_empty());
    }

    #[test]
    fn a_menu_keeps_its_destination_and_row_context_after_results_are_reordered() {
        let row = |id, title: &str| FoundRow::Album(chilpa_api::Album {
            id, title:title.into(), artist:"Artist".into(), year:None, track_count:1, artwork_path:String::new(), play_count:0
        });
        let mut state = SearchRust { rows:vec![row(1, "First album"), row(2, "Selected album")], ..Default::default() };
        state.menu = Some((state.rows.clone(), 1));
        state.rows.reverse();
        let (snapshot, at) = state.menu.as_ref().unwrap();
        let FoundRow::Album(selected) = &snapshot[*at] else { panic!("album selection missing") };
        assert_eq!(selected.id, 2);
        assert_eq!(state.rows[*at].title(), "First album", "the live row now refers to another destination");
        state.rows.clear();
        assert_eq!(snapshot.len(), 2, "the menu retains the complete original playback context");
    }
}
