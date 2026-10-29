//! Every track list (frontend ADR-0006). Small lists are loaded whole; the
//! library pages an id snapshot through a bounded cache. Inputs coalesce into
//! one reload, while playing and selection update roles in place.

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
        include!("cxx-qt-lib/qvector.h");
        type QVector_i32 = cxx_qt_lib::QVector<i32>;
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[base = QAbstractListModel]
        #[qml_element]
        #[qproperty(QString, kind)]
        #[qproperty(i64, album_id)]
        #[qproperty(i32, album_page)]
        #[qproperty(QString, key)]
        /// Playlist presentation order: custom, title, artist, album,
        /// duration or plays.
        #[qproperty(QString, order)]
        #[qproperty(bool, ascending)]
        #[qproperty(bool, playlist_editable)]
        #[qproperty(i32, limit)]
        #[qproperty(i32, filter_rev)]
        /// Static Home keeps its rows until the next explicit snapshot revision.
        #[qproperty(bool, stable_items)]
        #[qproperty(i32, snapshot_rev)]
        #[qproperty(QString, playing_path)]
        #[qproperty(i32, count)]
        #[qproperty(bool, ready)]
        #[qproperty(QString, summary)]
        #[qproperty(i32, selected_count)]
        type TrackListModel = super::TrackListRust;

        #[qinvokable]
        fn play(self: Pin<&mut TrackListModel>, row: i32);
        #[qinvokable]
        fn list_action(self: Pin<&mut TrackListModel>, action: &QString, row: i32, to: i32);
        #[qinvokable]
        fn play_all(self: Pin<&mut TrackListModel>, shuffled: bool);
        #[qinvokable]
        fn queue_all(self: Pin<&mut TrackListModel>);
        #[qinvokable]
        fn play_next_all(self: Pin<&mut TrackListModel>);
        #[qinvokable]
        fn play_later_all(self: Pin<&mut TrackListModel>, label: &QString);
        #[qinvokable]
        fn group_action(self: Pin<&mut TrackListModel>, row: i32, action: &QString, label: &QString);

        /// Selects a row. `modifiers`: 1 toggles it (Ctrl or Cmd), 2 extends
        /// from the anchor (Shift), 4 leaves the selection alone when the
        /// row is already in it (a menu opening on it).
        #[qinvokable]
        fn select(self: Pin<&mut TrackListModel>, row: i32, modifiers: i32);
        #[qinvokable]
        fn queue(self: Pin<&mut TrackListModel>, row: i32);
        #[qinvokable]
        fn play_chosen(self: Pin<&mut TrackListModel>, row: i32);
        #[qinvokable]
        fn play_next(self: Pin<&mut TrackListModel>, row: i32);
        #[qinvokable]
        fn play_later(self: Pin<&mut TrackListModel>, row: i32, label: &QString);
        #[qinvokable]
        fn remove_from_playlist(self: Pin<&mut TrackListModel>, row: i32);
        #[qinvokable]
        fn remove_chosen(self: Pin<&mut TrackListModel>, row: i32);
        #[qinvokable]
        fn favorite_chosen(self: Pin<&mut TrackListModel>, row: i32, favorite: bool);
        #[qinvokable]
        fn move_in_playlist(self: Pin<&mut TrackListModel>, row: i32, by: i32);
        #[qinvokable]
        fn move_in_playlist_to(self: Pin<&mut TrackListModel>, from: i32, to: i32);
        #[qinvokable]
        fn selected_row(self: &TrackListModel) -> i32;
        #[qinvokable]
        fn is_track(self: &TrackListModel, row: i32) -> bool;
        #[qinvokable] fn playing_row(self: &TrackListModel) -> i32;
        #[qinvokable]
        fn selected_any_row(self: &TrackListModel) -> i32;
        #[qinvokable]
        fn selected_track_ids_json(self: &TrackListModel) -> QString;
        #[qinvokable]
        fn chosen_track_ids_json(self: &TrackListModel, row: i32) -> QString;
        #[qinvokable]
        fn menu_json(self: &TrackListModel, row: i32) -> QString;

        /// Whether the row has an "album", an "artist" or a "folder" to go
        /// to. A record's page does not offer the record, nor an artist's
        /// page the artist.
        #[qinvokable]
        fn can_go(self: &TrackListModel, row: i32, target: &QString) -> bool;

        #[qinvokable]
        fn go(self: Pin<&mut TrackListModel>, row: i32, target: &QString);

        /// Where each letter starts in a grouped list, as `[["A", 12]]`.
        #[qinvokable]
        fn letters(self: &TrackListModel, rev: i32) -> QString;

        /// The row's artist, for the list to tell which one a click is on.
        #[qinvokable]
        fn credit_of(self: &TrackListModel, row: i32) -> QString;
        #[qinvokable]
        fn clear_selection(self: Pin<&mut TrackListModel>);
        /// Selects every track of the list.
        #[qinvokable]
        fn select_all(self: Pin<&mut TrackListModel>);
    }

    unsafe extern "RustQt" {
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut TrackListModel>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut TrackListModel>);
        #[inherit]
        #[cxx_name = "beginRemoveRows"]
        unsafe fn begin_remove_rows(self: Pin<&mut TrackListModel>, parent: &QModelIndex, first: i32, last: i32);
        #[inherit]
        #[cxx_name = "endRemoveRows"]
        unsafe fn end_remove_rows(self: Pin<&mut TrackListModel>);
        #[inherit]
        #[cxx_name = "beginMoveRows"]
        unsafe fn begin_move_rows(self: Pin<&mut TrackListModel>, parent: &QModelIndex, first: i32, last: i32, destination: &QModelIndex, child: i32) -> bool;
        #[inherit]
        #[cxx_name = "endMoveRows"]
        unsafe fn end_move_rows(self: Pin<&mut TrackListModel>);
        #[inherit]
        #[cxx_name = "beginInsertRows"]
        unsafe fn begin_insert_rows(self: Pin<&mut TrackListModel>, parent: &QModelIndex, first: i32, last: i32);
        #[inherit]
        #[cxx_name = "endInsertRows"]
        unsafe fn end_insert_rows(self: Pin<&mut TrackListModel>);
        #[inherit]
        #[qsignal]
        #[cxx_name = "dataChanged"]
        fn data_changed(self: Pin<&mut TrackListModel>, top_left: &QModelIndex, bottom_right: &QModelIndex, roles: &QVector_i32);
        #[inherit]
        fn index(self: &TrackListModel, row: i32, column: i32, parent: &QModelIndex) -> QModelIndex;
    }

    unsafe extern "RustQt" {
        #[qinvokable]
        #[cxx_override]
        fn data(self: &TrackListModel, index: &QModelIndex, role: i32) -> QVariant;
        /// The longest texts of each column in the rows it holds, as JSON;
        /// `rev` only asks again when the rows change.
        #[qinvokable]
        fn widest(self: &TrackListModel, rev: i32) -> QString;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &TrackListModel) -> QHash_i32_QByteArray;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &TrackListModel, parent: &QModelIndex) -> i32;
    }

    impl cxx_qt::Threading for TrackListModel {}
    impl cxx_qt::Initialize for TrackListModel {}
}

use crate::{app, browse, format, registry, worker};
use chilpa_api::{TrackEntry as Entry, TrackOrder, TrackRow};
use cxx_qt::{CxxQtType as _, Threading as _};
use cxx_qt_lib::{QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant, QVector};
use std::collections::HashSet;
use std::pin::Pin;
use chilpa_api::list_changes::{self, Step};
use std::rc::Rc;
use chilpa_qt_kit::bridge::queue_activity;

/// The roles, in order from `Qt::UserRole`.
const ROLES: &[&str] = &["rowKind", "number", "title", "artist", "album", "duration", "plays", "quality", "tier", "playing", "selected", "headerLabel", "headerMeta", "trackId", "albumId", "art", "available", "group", "sourceName", "sourceInfo", "composer", "headerWork", "headerComposer", "headerComposerTarget", "explicitContent", "credits"];
const USER_ROLE: i32 = 256;
const ROLE_PLAYING: i32 = USER_ROLE + 9;
const ROLE_SELECTED: i32 = USER_ROLE + 10;

use chilpa_qt_kit::track_rows::{Rows, At};

#[derive(Default)]
pub struct TrackListRust {
    kind: QString,
    album_id: i64,
    album_page: i32,
    key: QString,
    order: QString,
    ascending: bool,
    playlist_editable: bool,
    limit: i32,
    filter_rev: i32,
    stable_items: bool,
    snapshot_rev: i32,
    playing_path: QString,
    count: i32,
    ready: bool,
    summary: QString,
    selected_count: i32,
    rows: Rows,
    playing: String,
    /// Albums use track numbers; other lists use row positions.
    by_track_number: bool,
    selected: HashSet<i64>,
    anchor: Option<usize>,
    /// Which list the rows are: kind, id and key.
    loaded: Option<(String, i64, String)>,
    pending: bool,
    /// Tracks grouping, letter boundaries and latest query generation.
    group: String,
    letters: String,
    sections: Vec<(usize, String)>,
    asked: u64,
    queue_keys: Vec<String>,
    queue_numbers: Vec<i32>,
}

impl TrackListRust {
    fn len(&self) -> usize { self.rows.len() }
    fn at(&self, row: usize) -> Option<At<'_>> { self.rows.at(row) }

    /// Every track's file and id, in list order, when they are loaded.
    fn loaded_tracks(&self) -> Vec<&TrackRow> {
        match &self.rows {
            Rows::Loaded(v) => v.iter().filter_map(|e| if let Entry::Track(t) = e { Some(t) } else { None }).collect(),
            Rows::Snapshot { .. } => Vec::new(),
        }
    }

    fn track(&self, row: usize) -> Option<TrackRow> {
        match self.at(row)? {
            At::Track(page, i) => Some(page[i].clone()),
            At::Loaded(t) => Some(t.clone()),
            At::Header(..) => None,
        }
    }

    /// Whether the row's track can play now. A row not loaded yet, or a
    /// header, counts as one that can.
    fn available(&self, row: usize) -> bool {
        self.track(row).is_none_or(|t| t.available)
    }

    fn track_id(&self, row: usize) -> Option<i64> {
        match self.at(row)? {
            At::Track(page, i) => Some(page[i].id),
            At::Loaded(t) => Some(t.id),
            At::Header(..) => None,
        }
    }

    /// Tracks belonging to a loaded group. A disc includes its work sections;
    /// a work stops at the next work or disc header.
    fn group_paths(&self, row: usize) -> Vec<String> {
        let Rows::Loaded(entries) = &self.rows else {
            return Vec::new();
        };
        let Some(Entry::Header(group)) = entries.get(row) else { return Vec::new() };
        entries[row + 1..].iter().take_while(|entry| match entry {
            Entry::Header(next) => !group.work && next.work,
            Entry::Track(_) => true,
        }).filter_map(|entry| match entry {
            Entry::Track(track) => Some(track.path.clone()),
            Entry::Header(_) => None,
        }).collect()
    }
}

fn role_value(list: &TrackListRust, row: usize, role: i32) -> QVariant {
    let Some(at) = list.at(row) else {
        return QVariant::default();
    };
    let track = match &at {
        // A divider is kind 1; a header that stands for a record, with
        // its cover and its artist, is kind 2.
        At::Header(header) => {
            return chilpa_qt_kit::track_roles::header(header, role);
        }
        At::Track(page, i) => &page[*i],
        At::Loaded(t) => *t,
    };
    let number = if list.by_track_number { i32::from(track.number) } else { list.queue_numbers.get(row).copied().unwrap_or(row as i32 + 1) };
    let playing = role == ROLE_PLAYING && if let Some(key) = list.queue_keys.get(row) { app::with(|api| api.listen_is_playing(key)) } else { !list.playing.is_empty() && track.path == list.playing };
    let selected = if list.queue_keys.is_empty() { list.selected.contains(&track.id) } else { list.anchor == Some(row) };
    chilpa_qt_kit::track_roles::track(track, role, number, playing, selected, &list.group)
}

/// Adds files to the end of the queue, on the output in use.
fn enqueue(api: &mut chilpa_api::Chilpa, paths: &[String]) {
    api.enqueue_here(paths);
}

/// Puts files right after what is playing, on the output in use.
fn next(api: &mut chilpa_api::Chilpa, paths: &[String]) {
    api.play_next_here(paths);
}

fn summary(tracks: usize, ms: u64) -> String {
    let n = tracks.to_string();
    let songs = app::with(|api| api.tf("{} song", "{} songs", tracks as i64, &[&n]));
    format!("{songs} · {}", format::span(ms))
}

impl qobject::TrackListModel {
    pub fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        // A group's name is known without reading the row's page.
        let section = (role == USER_ROLE + 17).then(|| chilpa_api::section_at(&self.rust().sections, index.row().max(0) as usize)).flatten();
        section.map_or_else(|| role_value(self.rust(), index.row().max(0) as usize, role), |name| QVariant::from(&QString::from(name)))
    }

    pub fn widest(&self, _rev: i32) -> QString {
        QString::from(&app::with(|api| api.widest_texts(&self.rust().loaded_tracks())).to_string())
    }

    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        crate::format::roles(USER_ROLE, ROLES)
    }

    pub fn row_count(&self, _parent: &QModelIndex) -> i32 {
        self.rust().len() as i32
    }

    pub fn play(self: Pin<&mut Self>, row: i32) {
        let row = row.max(0) as usize;
        if let Some(key) = self.rust().queue_keys.get(row) {
            app::with(|api| api.listen_row_action(key, "play", ""));
            return;
        }
        // A row that cannot play now does not play, by click or by key (ADR-017).
        if self.rust().track(row).is_some_and(|t| !t.available) {
            return;
        }
        self.playlist_started();
        match &self.rust().rows {
            Rows::Loaded(entries) => {
                // A header row plays the first track under it.
                let start = entries[..row.min(entries.len())].iter().filter(|e| matches!(e, Entry::Track(_))).count();
                let paths: Vec<String> = self.rust().loaded_tracks().iter().map(|t| t.path.clone()).collect();
                app::with(|api| api.play_from(&paths, start));
            }
            Rows::Snapshot { ids, .. } => {
                // Resolving every file of a large snapshot is a query of its
                // own, so it runs on the worker and plays when it is back.
                let ids = ids.ids.clone();
                let thread = self.qt_thread();
                let origin = app::with(|api| api.browse_origin());
                worker::submit(move |reader| {
                    let paths = reader.paths(&ids);
                    let _ = thread.queue(move |_| app::with(|api| api.with_queue_origin(origin, |api| api.play_from(&paths, row))));
                });
            }
        }
    }

    pub fn queue_all(self: Pin<&mut Self>) {
        self.with_all_paths(enqueue);
    }
    pub fn play_next_all(self: Pin<&mut Self>) {
        self.with_all_paths(next);
    }

    pub fn play_later_all(self: Pin<&mut Self>, label: &QString) {
        let label = label.to_string();
        self.with_all_paths(move |api, paths| { api.play_later_here(paths, &label); });
    }

    pub fn group_action(self: Pin<&mut Self>, row: i32, action: &QString, label: &QString) {
        let paths = self.rust().group_paths(row.max(0) as usize);
        if paths.is_empty() {
            return;
        }
        app::with(|api| match action.to_string().as_str() {
            "play" => api.play_from(&paths, 0),
            "shuffle" => api.play_list(&paths, true),
            "queue" => api.enqueue_here(&paths),
            "next" => api.play_next_here(&paths),
            "later" => { api.play_later_here(&paths, &label.to_string()); }
            action if action.starts_with("buffer:") => api.listen_paths_action(action, &paths),
            _ => {}
        });
    }

    fn with_all_paths(&self, then: impl FnOnce(&mut chilpa_api::Chilpa, &[String]) + Send + 'static) {
        let paths = match &self.rust().rows {
            Rows::Loaded(_) => queue_activity::QueuePaths::Known(self.rust().loaded_tracks().iter().map(|t| t.path.clone()).collect()),
            Rows::Snapshot { ids, .. } => queue_activity::QueuePaths::Ids(ids.ids.clone()),
        };
        queue_activity::paths(paths, then);
    }

    pub fn list_action(mut self: Pin<&mut Self>, action: &QString, row: i32, to: i32) {
        let action = action.to_string();
        if row < 0 && action.starts_with("buffer:") { self.with_all_paths(move |api, paths| api.listen_paths_action(&action, paths)); return; }
        if row < 0 || (action == "move" && to < 0) { return; }
        if action.starts_with("buffer:") {
            if self.in_selection(row) {
                self.with_selection(move |api, paths| api.listen_paths_action(&action, paths));
            } else if let Some(track) = self.rust().track(row.max(0) as usize) {
                app::with(|api| api.listen_paths_action(&action, &[track.path]));
            }
        } else if let Some(key) = self.rust().queue_keys.get(row.max(0) as usize) {
            let target = self.rust().queue_keys.get(to.max(0) as usize).map(String::as_str).unwrap_or("");
            app::with(|api| api.listen_row_action(key, &action, target));
            self.as_mut().schedule();
        }
    }

    pub fn play_all(self: Pin<&mut Self>, shuffled: bool) {
        self.playlist_started();
        self.with_all_paths(move |api, paths| api.play_list(paths, shuffled));
    }

    /// A playlist's own list that starts playing is a listen of the playlist.
    /// A source's playlist is shown as a record's list.
    fn playlist_started(&self) {
        let (id, kind) = (self.rust().album_id, self.rust().kind.to_string());
        if (kind == "playlist" && id != 0) || (kind == "album" && id < 0) {
            app::with(|api| api.playlist_started(id));
        }
    }

    pub fn select(mut self: Pin<&mut Self>, row: i32, modifiers: i32) {
        let row = row.max(0) as usize;
        let modifiers = if self.rust().queue_keys.is_empty() { modifiers } else { 0 };
        let (selected, anchor) = chilpa_api::select_tracks(&self.rust().selected, self.rust().anchor, row, modifiers, |at| self.rust().track_id(at));
        let mut this = self.as_mut().rust_mut();
        this.selected = selected;
        this.anchor = anchor;
        self.changed(ROLE_SELECTED);
    }

    pub fn queue(self: Pin<&mut Self>, row: i32) {
        if !self.in_selection(row) {
            if let Some(track) = self.rust().track(row.max(0) as usize) {
                app::with(|api| enqueue(api, &[track.path]));
            }
            return;
        }
        self.with_selection(enqueue);
    }

    pub fn play_chosen(self: Pin<&mut Self>, row: i32) {
        if !self.rust().queue_keys.is_empty() || !self.in_selection(row) {
            self.play(row);
            return;
        }
        self.playlist_started();
        self.with_selection(|api, paths| api.play_from(paths, 0));
    }

    pub fn play_next(self: Pin<&mut Self>, row: i32) {
        if !self.in_selection(row) {
            if let Some(track) = self.rust().track(row.max(0) as usize) {
                app::with(|api| next(api, &[track.path]));
            }
            return;
        }
        self.with_selection(next);
    }

    pub fn play_later(self: Pin<&mut Self>, row: i32, label: &QString) {
        let mut label = label.to_string();
        if label.trim().is_empty() {
            label = self.rust().track(row.max(0) as usize).map(|track| track.album).unwrap_or_default();
            if label.is_empty() { label = app::t("Play later"); }
        }
        let later = move |api: &mut chilpa_api::Chilpa, paths: &[String]| {
            api.play_later_here(paths, &label);
        };
        if !self.in_selection(row) {
            if let Some(track) = self.rust().track(row.max(0) as usize) {
                app::with(|api| later(api, &[track.path]));
            }
            return;
        }
        self.with_selection(later);
    }

    pub fn remove_from_playlist(mut self: Pin<&mut Self>, row: i32) {
        let playlist = self.rust().album_id;
        if self.rust().kind.to_string() != "playlist" || row < 0 {
            return;
        }
        if app::with(|api| api.remove_from_playlist(playlist, row as usize)) {
            self.as_mut().rust_mut().selected.clear();
            registry::playlists(|lists| lists.changed());
        }
    }

    pub fn remove_chosen(mut self: Pin<&mut Self>, row: i32) {
        let playlist = self.rust().album_id;
        if self.rust().kind.to_string() != "playlist" || row < 0 {
            return;
        }
        let ids = self.chosen_track_ids(row);
        if app::with(|api| api.remove_tracks_from_playlist(playlist, &ids)) > 0 {
            self.as_mut().rust_mut().selected.clear();
            registry::playlists(|lists| lists.changed());
        }
    }

    pub fn favorite_chosen(self: Pin<&mut Self>, row: i32, favorite: bool) {
        let ids = self.chosen_track_ids(row);
        app::with(|api| api.set_tracks_favorite(&ids, favorite));
    }

    pub fn move_in_playlist(self: Pin<&mut Self>, row: i32, by: i32) {
        if !self.rust().queue_keys.is_empty() {
            let mut to = row + by;
            while to >= 0 && self.rust().queue_keys.get(to as usize).is_some_and(String::is_empty) { to += by; }
            self.list_action(&QString::from("move"), row, to); return;
        }
        if self.rust().kind.to_string() != "playlist" || row < 0 || !self.rust().available(row as usize) {
            return;
        }
        let to = row + by;
        if to < 0 || to >= self.rust().len() as i32 {
            return;
        }
        let playlist = self.rust().album_id;
        if app::with(|api| api.move_in_playlist(playlist, row as usize, to as usize)) {
            registry::playlists(|lists| lists.changed());
        }
    }

    pub fn move_in_playlist_to(self: Pin<&mut Self>, from: i32, to: i32) {
        if !self.rust().queue_keys.is_empty() {
            self.list_action(&QString::from("move"), from, to);
            return;
        }
        if self.rust().kind.to_string() != "playlist" || self.rust().order.to_string() != "custom" || from < 0 || to < 0 || from == to {
            return;
        }
        if !self.rust().available(from as usize) {
            return;
        }
        let playlist = self.rust().album_id;
        // A row dragged out of a selection takes the selection with it.
        let rows: Vec<usize> = if self.in_selection(from) {
            let this = self.rust();
            (0..this.len()).filter(|&at| this.track_id(at).is_some_and(|id| this.selected.contains(&id))).collect()
        } else { vec![from as usize] };
        if app::with(|api| api.move_group_in_playlist(playlist, &rows, to as usize)) {
            registry::playlists(|lists| lists.changed());
        }
    }

    pub fn is_track(&self, row: i32) -> bool { row >= 0 && self.rust().track(row as usize).is_some() }

    pub fn selected_row(&self) -> i32 {
        let this = self.rust();
        if !this.queue_keys.is_empty() { return this.anchor.map_or(-1, |r| r as i32); }
        if this.selected.len() != 1 {
            return -1;
        }
        (0..this.len()).find(|&r| this.track_id(r).is_some_and(|id| this.selected.contains(&id))).map_or(-1, |r| r as i32)
    }

    pub fn playing_row(&self) -> i32 {
        let playing = app::with(|api| api.listen_playing_key());
        if playing.is_empty() { return -1; }
        self.rust().queue_keys.iter().position(|key| key == &playing).map_or(-1, |row| row as i32)
    }

    pub fn selected_any_row(&self) -> i32 {
        let this = self.rust();
        (0..this.len()).find(|&row| this.track_id(row).is_some_and(|id| this.selected.contains(&id))).map_or(-1, |row| row as i32)
    }

    pub fn selected_track_ids_json(&self) -> QString {
        let this = self.rust();
        let ids: Vec<i64> = (0..this.len()).filter_map(|row| this.track_id(row)).filter(|id| this.selected.contains(id)).collect();
        QString::from(serde_json::to_string(&ids).unwrap_or_default())
    }

    pub fn chosen_track_ids_json(&self, row: i32) -> QString {
        QString::from(serde_json::to_string(&self.chosen_track_ids(row)).unwrap_or_default())
    }

    pub fn menu_json(&self, row: i32) -> QString {
        if row < 0 { return QString::from(app::with(|api| api.collection_queue_menu()).to_string()); }
        let this = self.rust();
        let editable = this.kind.to_string() == "playlist" && this.playlist_editable;
        let context = chilpa_api::TrackMenuContext {
            count: if row < 0 { 0 } else { self.chosen_track_ids(row).len() },
            queued: !this.queue_keys.is_empty(),
            editable,
            move_up: editable && this.order.to_string() == "custom" && row > 0,
            move_down: editable && this.order.to_string() == "custom" && row < this.len() as i32 - 1,
            album: self.can_go(row, &QString::from("album")),
            artist: self.can_go(row, &QString::from("artist")),
            artists: this.track(row.max(0) as usize).map(|t| t.credits.as_ref().map(|c| c.artists.iter().chain(&c.featured).map(|p| p.name.clone()).collect()).unwrap_or_else(|| app::with(|api| api.credit_names(&t.artist)))).unwrap_or_default(),
            folder: self.can_go(row, &QString::from("folder")),
            metadata: self.can_go(row, &QString::from("metadata")),
            remote: this.track(row.max(0) as usize).is_some_and(|t| t.id < 0),
            // Over the whole selection it acts on, not the row clicked: one
            // that cannot play among several leaves only what removes it.
            unavailable: self.acts_on_unavailable(row),
        };
        let mut entries = app::with(|api| api.track_queue_menu(context));
        if let Some(key) = this.queue_keys.get(row.max(0) as usize) {
            entries.retain(|e| !matches!(e["act"].as_str(), Some("play" | "dequeue")));
            let mut queue = app::with(|api| api.listen_menu(key));
            queue.extend(entries); entries = queue;
        }
        QString::from(serde_json::to_string(&entries).unwrap_or_default())
    }

    /// Whether any track the menu of that row would act on cannot play.
    fn acts_on_unavailable(&self, row: i32) -> bool {
        let this = self.rust();
        if self.in_selection(row) {
            (0..this.len()).filter_map(|at| this.track(at)).any(|t| this.selected.contains(&t.id) && !t.available)
        } else {
            !this.available(row.max(0) as usize)
        }
    }

    /// Whether the row is one of several selected.
    fn in_selection(&self, row: i32) -> bool {
        let this = self.rust();
        this.selected.len() > 1 && this.track_id(row.max(0) as usize).is_some_and(|id| this.selected.contains(&id))
    }

    pub fn chosen_track_ids(&self, row: i32) -> Vec<i64> {
        let this = self.rust();
        if self.in_selection(row) {
            (0..this.len()).filter_map(|at| this.track_id(at)).filter(|id| this.selected.contains(id)).collect()
        } else {
            this.track_id(row.max(0) as usize).into_iter().collect()
        }
    }

    /// The selected files, in list order -- which the selection does not
    /// keep -- handed to `then` on the window's thread.
    fn with_selection(&self, then: impl FnOnce(&mut chilpa_api::Chilpa, &[String]) + Send + 'static) {
        let this = self.rust();
        let paths = match &this.rows {
            Rows::Loaded(_) => queue_activity::QueuePaths::Known(this.loaded_tracks().iter().filter(|t| this.selected.contains(&t.id)).map(|t| t.path.clone()).collect()),
            Rows::Snapshot { ids, .. } => queue_activity::QueuePaths::Ids(ids.ids.iter().copied().filter(|id| this.selected.contains(id)).collect()),
        };
        queue_activity::paths(paths, then);
    }

    pub fn can_go(&self, row: i32, target: &QString) -> bool {
        let Some(track) = self.rust().track(row.max(0) as usize) else {
            return false;
        };
        let kind = self.rust().kind.to_string();
        // A source playlist uses the record screen, but each song has its own album.
        let context = if kind == "album" && track.album_id != self.rust().album_id { "collection" } else { &kind };
        chilpa_api::track_can_go(&track, context, &self.rust().key.to_string(), &target.to_string())
    }

    pub fn letters(&self, _rev: i32) -> QString {
        QString::from(&self.rust().letters)
    }

    pub fn credit_of(&self, row: i32) -> QString {
        QString::from(&self.rust().track(row.max(0) as usize).map(|t| t.artist.clone()).unwrap_or_default())
    }

    pub fn go(self: Pin<&mut Self>, row: i32, target: &QString) {
        if !self.can_go(row, target) {
            return;
        }
        let Some(track) = self.rust().track(row.max(0) as usize) else {
            return;
        };
        let target = target.to_string();
        if target == "metadata" {
            // The editor opens on this track before its screen shows.
            let (album, id) = (track.album_id, track.id);
            chilpa_qt_metadata::bridge::metadata::ask(move |editor| {
                // A record already open keeps what was typed into it; the
                // screen asks before another's changes are lost.
                let kept = app::with(|api| {
                    let view = api.metadata_view();
                    view.state != "closed" && (view.album_id == album || view.dirty)
                });
                if !kept {
                    editor.open_album(album, id);
                }
            });
        }
        let place = target.strip_prefix("artist:").map_or(0, |at| at.parse().unwrap_or(-1));
        let source_artist = app::with(|api| api.source_track_artist(&track.path, &track.artist, place));
        // A source's track leads to its source's artist page, found by name
        // when the source names none: the library has no page for it.
        let of_source = chilpa_api::on_a_source(&track.path);
        registry::navigation(move |mut nav| match target.as_str() {
            "artist" if source_artist.is_some() => nav.as_mut().go_key(&QString::from("source"), &QString::from(source_artist.as_deref().unwrap_or_default())),
            "album" => nav.as_mut().go(&QString::from("album"), track.album_id),
            "metadata" => nav.as_mut().go(&QString::from("metadata"), track.album_id),
            "artist" => nav.as_mut().open_credit(&QString::from(&track.artist), 0),
            one if one.starts_with("artist:") && source_artist.is_some() => nav.as_mut().go_key(&QString::from("source"), &QString::from(source_artist.as_deref().unwrap_or_default())),
            one if one.starts_with("artist") && of_source => {}
            one if one.starts_with("artist:") => nav.as_mut().open_credit(&QString::from(&track.artist), one[7..].parse().unwrap_or(-1)),
            "folder" => {
                let dir = std::path::Path::new(&track.path).parent().map(|d| d.display().to_string());
                if let Some(dir) = dir {
                    nav.as_mut().open_folder(&QString::from(&dir));
                }
            }
            _ => {}
        });
    }

    pub fn select_all(mut self: Pin<&mut Self>) {
        let all: HashSet<i64> = match &self.rust().rows {
            Rows::Loaded(_) => self.rust().loaded_tracks().iter().map(|t| t.id).collect(),
            Rows::Snapshot { ids, .. } => ids.ids.iter().copied().collect(),
        };
        self.as_mut().rust_mut().selected = all;
        self.changed(ROLE_SELECTED);
    }

    pub fn clear_selection(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().selected.clear();
        self.as_mut().rust_mut().anchor = None;
        self.changed(ROLE_SELECTED);
    }

    /// One role changed on some rows. The view only rereads the rows it has
    /// built, so naming the whole range costs what is on screen.
    fn changed(mut self: Pin<&mut Self>, role: i32) {
        if role == ROLE_SELECTED {
            let count = self.rust().selected.len() as i32;
            self.as_mut().set_selected_count(count);
        }
        let last = self.rust().len() as i32 - 1;
        if last < 0 {
            return;
        }
        let parent = QModelIndex::default();
        let (top, bottom) = (self.index(0, 0, &parent), self.index(last, 0, &parent));
        let mut roles = QVector::<i32>::default();
        roles.append(role);
        self.as_mut().data_changed(&top, &bottom, &roles);
    }

    /// Asks for a reload once this turn of the event loop is over, however
    /// many inputs changed in it.
    fn schedule(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().asked = self.rust().asked.wrapping_add(1);
        if self.rust().pending {
            return;
        }
        self.as_mut().rust_mut().pending = true;
        let _ = self.qt_thread().queue(|model| model.reload());
    }

    fn home_arrived(self: Pin<&mut Self>, asked: u64, rows: chilpa_api::HomeRows) {
        if asked == self.rust().asked {
            if let chilpa_api::HomeRows::Tracks(tracks) = rows {
                self.show("mostPlayed".into(), Rows::Loaded(tracks.into_iter().map(Entry::Track).collect()), None, Vec::new());
            }
        }
    }

    fn reload(mut self: Pin<&mut Self>) {
        let mut queue_keys = Vec::new();
        self.as_mut().rust_mut().pending = false;
        let (id, limit) = (self.rust().album_id, self.rust().limit.max(1) as usize);
        let kind = self.rust().kind.to_string();
        let identity = (kind.clone(), id, self.key().to_string());
        if kind == "source" && *self.stable_items() && *self.count() > 0 && self.rust().loaded.as_ref() == Some(&identity) {
            if let Rows::Loaded(rows) = &self.rust().rows {
                let mut rows = rows.clone();
                app::with(|api| api.refresh_source_track_artwork(&mut rows));
                let duration = rows.iter().filter_map(|row| match row { Entry::Track(track) => Some(track.duration_ms), _ => None }).sum();
                return self.show(kind, Rows::Loaded(rows), Some(duration), Vec::new());
            }
        }
        if kind == "mostPlayed" {
            let asked = self.rust().asked.wrapping_add(1);
            self.as_mut().rust_mut().asked = asked;
            let (thread, revision) = (self.qt_thread(), self.rust().filter_rev);
            let cached = app::with(|api| api.home_section(&kind, limit, revision, move |rows| {
                let _ = thread.queue(move |model| model.home_arrived(asked, rows));
            }));
            if let Some(rows) = cached { self.home_arrived(asked, rows); }
            return;
        }
        if kind == "tracks" {
            app::with(|api| api.prepare_track_libraries());
            // Found on the worker; what is shown stays until it is back.
            let (query, extras) = (self.rust().key.to_string(), browse::read(|s| s.filter.clone()));
            let asked = self.rust().asked + 1;
            self.as_mut().rust_mut().asked = asked;
            let thread = self.qt_thread();
            worker::submit(move |reader| reader.tracks_progressive(&query, &extras, |found| {
                let _ = thread.queue(move |mut model| {
                    if model.rust().asked == asked {
                        let ms = found.snapshot.totals.duration_ms;
                        (model.as_mut().rust_mut().group, model.as_mut().rust_mut().letters, model.as_mut().rust_mut().sections) = (found.group, found.letters, found.sections);
                        model.show("tracks".into(), Rows::Snapshot { ids: Rc::new(found.snapshot), pages: Rc::default() }, Some(ms), Vec::new());
                    }
                });
            }));
            return;
        }
        if kind == "album" && id != 0 {
            let asked = self.rust().asked.wrapping_add(1);
            self.as_mut().rust_mut().asked = asked;
            // For a record, `key` is what is typed to find its rows (`entries_matching`).
            let typed = self.rust().key.to_string();
            // A source playlist's page shows its rows in the order chosen for it.
            let (order, ascending) = (self.rust().order.to_string(), self.rust().ascending);
            if let Some(page) = chilpa_qt_kit::album_pages::get(self.rust().album_page, id) {
                self.show(kind, Rows::Loaded(chilpa_api::ordered_entries(chilpa_api::entries_matching(page.entries.clone(), &typed), &order, ascending)), Some(page.detail.duration_ms), Vec::new());
            } else if id > 0 {
                let thread = self.qt_thread();
                worker::submit_album(move |reader| {
                    let page = reader.album_page(id, true);
                    let _ = thread.queue(move |model| {
                        if model.rust().asked == asked && model.rust().album_id == id && model.rust().kind.to_string() == "album" {
                            if let Some(page) = page { model.show("album".into(), Rows::Loaded(chilpa_api::entries_matching(page.entries, &typed)), Some(page.detail.duration_ms), Vec::new()); }
                        }
                    });
                });
            } else if let Some(page) = app::with(|api| api.cached_album_page(id)) {
                self.show(kind, Rows::Loaded(chilpa_api::ordered_entries(chilpa_api::entries_matching(page.entries, &typed), &order, ascending)), Some(page.detail.duration_ms), Vec::new());
            }
            return;
        }
        // The rows, and how long they last when the list says so.
        let (rows, length) = match kind.as_str() {
            "source" => {
                let mut entries = app::with(|api| api.source_track_entries(&self.rust().key.to_string()));
                if self.rust().limit > 0 { entries.truncate(limit); }
                let duration = entries.iter().filter_map(|e| match e { Entry::Track(t) => Some(t.duration_ms), _ => None }).sum();
                (Rows::Loaded(entries), Some(duration))
            }
            "artist" | "artistTop" => {
                let name = self.rust().key.to_string();
                let top = kind == "artistTop";
                let (order, count) = if top { (TrackOrder::Plays, 10) } else { (TrackOrder::Artist, 100_000) };
                let mut tracks = app::with(|api| if top { api.cached_artist_top_tracks(&name) } else { api.artist_tracks(&name, order, count) });
                if self.rust().limit > 0 { tracks.truncate(limit); }
                let ms = tracks.iter().map(|t| t.duration_ms).sum::<u64>();
                (Rows::Loaded(tracks.into_iter().map(Entry::Track).collect()), (!top).then_some(ms))
            }
            "folder" => {
                let tracks = chilpa_qt_folders::bridge::folder_view::tracks();
                let ms = tracks.iter().map(|t| t.duration_ms).sum::<u64>();
                (Rows::Loaded(tracks.iter().cloned().map(Entry::Track).collect()), Some(ms))
            }
            "playlist" => {
                let order = self.rust().order.to_string();
                let ascending = self.rust().ascending;
                // What is typed above the list finds its rows, as on a record's page.
                let rows = chilpa_api::entries_matching(app::with(|api| api.playlist_rows_sorted(id, &order, ascending)).into_iter().map(Entry::Track).collect(), &self.rust().key.to_string());
                let ms = rows.iter().map(|e| if let Entry::Track(t) = e { t.duration_ms } else { 0 }).sum::<u64>();
                (Rows::Loaded(rows), Some(ms))
            }
            "openMedia" => {
                let (rows, keys) = app::with(|api| api.media_rows(&self.rust().key.to_string()));
                queue_keys = keys;
                (Rows::Loaded(rows), None)
            }
            "queue" | "listenlist" | "history" => {
                let query = self.rust().key.to_string();
                let (entries, keys) = app::with(|api| if kind == "history" { api.listen_history(&query) } else { api.listen_rows(id as u64, &query, kind == "listenlist") });
                queue_keys = keys;
                let ms = entries.iter().filter_map(|e| if let Entry::Track(t) = e { Some(t.duration_ms) } else { None }).sum();
                (Rows::Loaded(entries), Some(ms))
            }
            "library" => {
                let (snapshot, pages) = browse::read(|s| (s.snapshot.clone(), s.track_pages.clone()));
                let ms = snapshot.totals.duration_ms;
                (Rows::Snapshot { ids: snapshot, pages }, Some(ms))
            }
            _ => (Rows::default(), None),
        };
        self.show(kind, rows, length, queue_keys);
    }

    fn show(mut self: Pin<&mut Self>, kind: String, rows: Rows, length: Option<u64>, keys: Vec<String>) {
        let id = self.rust().album_id;
        // I preserve selection when refreshing the same list.
        let identity = (kind.clone(), id, self.rust().key.to_string());
        let same = self.rust().loaded.as_ref() == Some(&identity);
        if same {
            if let (Rows::Snapshot { ids: old, pages: old_pages }, Rows::Snapshot { ids: new, pages: new_pages }) = (&self.rust().rows, &rows) {
                if old.ids == new.ids {
                    let changed = !Rc::ptr_eq(old_pages, new_pages);
                    self.as_mut().rust_mut().rows = rows;
                    if changed && *self.count() > 0 {
                        let (first, last) = (self.index(0, 0, &QModelIndex::default()), self.index(*self.count() - 1, 0, &QModelIndex::default()));
                        self.as_mut().data_changed(&first, &last, &QVector::<i32>::default());
                    }
                    if let Some(ms) = length { let count = *self.count() as usize; self.as_mut().set_summary(QString::from(&summary(count, ms))); }
                    return;
                }
            }
        }
        if same && self.rust().queue_keys == keys && matches!((&self.rust().rows, &rows), (Rows::Loaded(a), Rows::Loaded(b)) if a == b) { return; }
        let plan = if same && self.rust().len() > 0 { match (&self.rust().rows, &rows) {
            (Rows::Loaded(old), Rows::Loaded(new)) => list_changes::tracks(old, new, &self.rust().queue_keys, &keys),
            _ => None,
        } } else { None };
        if let Some(plan) = plan {
            let Rows::Loaded(new) = &rows else { unreachable!() };
            let parent = QModelIndex::default();
            for step in plan.steps {
                // SAFETY: each valid edit brackets its storage mutation.
                unsafe { match step {
                    Step::Remove(at) => self.as_mut().begin_remove_rows(&parent, at as i32, at as i32),
                    Step::Insert(at) => self.as_mut().begin_insert_rows(&parent, at as i32, at as i32),
                    Step::Move { from, .. } => { self.as_mut().begin_move_rows(&parent, from as i32, from as i32, &parent, step.destination() as i32); }
                } }
                { let mut this = self.as_mut().rust_mut();
                    if let Rows::Loaded(old) = &mut this.rows { step.apply(old, new); }
                    if !keys.is_empty() || !this.queue_keys.is_empty() { step.apply(&mut this.queue_keys, &keys); }
                    this.anchor = step.remap(this.anchor);
                    this.queue_numbers = list_changes::numbers(&this.queue_keys, kind == "openMedia");
                }
                unsafe { match step {
                    Step::Remove(_) => self.as_mut().end_remove_rows(), Step::Insert(_) => self.as_mut().end_insert_rows(), Step::Move { .. } => self.as_mut().end_move_rows(),
                } }
            }
            self.as_mut().rust_mut().rows = rows;
            for row in plan.changed { let at = self.index(row as i32, 0, &parent); self.as_mut().data_changed(&at, &at, &QVector::<i32>::default()); }
        } else {
            // SAFETY: the reset is begun and ended around the swap, as Qt asks.
            unsafe { self.as_mut().begin_reset_model() };
            {
                let mut this = self.as_mut().rust_mut();
                if same { if let (Rows::Loaded(old), Rows::Loaded(new)) = (&this.rows, &rows) {
                    this.anchor = list_changes::track_anchor(old, new, &this.queue_keys, &keys, this.anchor);
                } }
                this.rows = rows;
                this.queue_numbers = list_changes::numbers(&keys, kind == "openMedia");
                this.queue_keys = keys;
                this.by_track_number = kind == "album";
                if !same { this.selected.clear(); this.anchor = None; }
                this.loaded = Some(identity);
            }
            unsafe { self.as_mut().end_reset_model() };
        }
        { let mut this = self.as_mut().rust_mut();
            if let Rows::Loaded(entries) = &this.rows {
                let there: HashSet<_> = entries.iter().filter_map(|e| if let Entry::Track(t) = e { Some(t.id) } else { None }).collect();
                this.selected.retain(|id| there.contains(id));
                if !this.queue_keys.is_empty() && this.anchor.is_none() { this.selected.clear(); }
            }
        }
        let chosen = self.rust().selected.len() as i32;
        self.as_mut().set_selected_count(chosen);
        let tracks = match &self.rust().rows {
            Rows::Loaded(_) => self.rust().loaded_tracks().len(),
            Rows::Snapshot { ids, .. } => ids.ids.len(),
        };
        self.as_mut().set_count(tracks as i32);
        let text = length.map(|ms| summary(tracks, ms)).unwrap_or_default();
        self.as_mut().set_summary(QString::from(&text));
        self.as_mut().set_ready(true);
    }
}

impl cxx_qt::Initialize for qobject::TrackListModel {
    fn initialize(mut self: Pin<&mut Self>) {
        self.as_mut().on_kind_changed(|m| m.schedule()).release();
        self.as_mut().on_album_id_changed(|m| m.schedule()).release();
        self.as_mut().on_album_page_changed(|m| m.schedule()).release();
        self.as_mut().on_key_changed(|m| m.schedule()).release();
        self.as_mut().on_order_changed(|m| m.schedule()).release();
        self.as_mut().on_ascending_changed(|m| m.schedule()).release();
        self.as_mut().on_limit_changed(|m| m.schedule()).release();
        self.as_mut().on_filter_rev_changed(|m| m.schedule()).release();
        self.as_mut().on_stable_items_changed(|mut m| { m.as_mut().rust_mut().loaded = None; m.schedule(); }).release();
        self.as_mut().on_snapshot_rev_changed(|mut m| {
            if *m.stable_items() { m.as_mut().rust_mut().loaded = None; m.schedule(); }
        }).release();
        self.as_mut()
            .on_playing_path_changed(|mut m| {
                let now = m.playing_path().to_string();
                m.as_mut().rust_mut().playing = now;
                m.changed(ROLE_PLAYING);
            })
            .release();
    }
}
