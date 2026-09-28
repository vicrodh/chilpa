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
        #[qproperty(QString, key)]
        /// Playlist presentation order: custom, title, artist, album,
        /// duration or plays.
        #[qproperty(QString, order)]
        #[qproperty(bool, ascending)]
        #[qproperty(bool, playlist_editable)]
        #[qproperty(i32, limit)]
        #[qproperty(i32, filter_rev)]
        #[qproperty(QString, playing_path)]
        #[qproperty(i32, count)]
        #[qproperty(QString, summary)]
        #[qproperty(i32, selected_count)]
        type TrackListModel = super::TrackListRust;

        #[qinvokable]
        fn play(self: Pin<&mut TrackListModel>, row: i32);
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
        #[qinvokable]
        fn clear_selection(self: Pin<&mut TrackListModel>);
    }

    unsafe extern "RustQt" {
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut TrackListModel>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut TrackListModel>);
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
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QUrl, QVariant, QVector};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::pin::Pin;
use std::rc::Rc;

/// The roles, in order from `Qt::UserRole`.
const ROLES: &[&str] = &["rowKind", "number", "title", "artist", "album", "duration", "plays", "quality", "tier", "playing", "selected", "headerLabel", "headerMeta", "trackId", "albumId", "art"];
const USER_ROLE: i32 = 256;
const ROLE_PLAYING: i32 = USER_ROLE + 9;
const ROLE_SELECTED: i32 = USER_ROLE + 10;

/// Rows read at a time from a snapshot, and how many pages stay cached.
const PAGE: usize = 128;
const PAGES_KEPT: usize = 32;

#[derive(Default)]
struct Pages {
    map: HashMap<usize, Rc<Vec<TrackRow>>>,
    order: VecDeque<usize>,
}

enum Rows {
    Loaded(Vec<Entry>),
    Snapshot { ids: Rc<chilpa_api::Snapshot>, pages: RefCell<Pages> },
}

impl Default for Rows {
    fn default() -> Self {
        Rows::Loaded(Vec::new())
    }
}

#[derive(Default)]
pub struct TrackListRust {
    kind: QString,
    album_id: i64,
    key: QString,
    order: QString,
    ascending: bool,
    playlist_editable: bool,
    limit: i32,
    filter_rev: i32,
    playing_path: QString,
    count: i32,
    summary: QString,
    selected_count: i32,
    rows: Rows,
    playing: String,
    /// A record numbers its rows by track number; any other list, by
    /// position.
    by_track_number: bool,
    selected: HashSet<i64>,
    anchor: Option<usize>,
    /// Which list the rows are: kind, id and key.
    loaded: Option<(String, i64, String)>,
    pending: bool,
}

/// A row, either loaded or read from the snapshot's page.
enum At<'a> {
    Header(&'a str, &'a str),
    Track(Rc<Vec<TrackRow>>, usize),
    Loaded(&'a TrackRow),
}

impl TrackListRust {
    fn len(&self) -> usize {
        match &self.rows {
            Rows::Loaded(v) => v.len(),
            Rows::Snapshot { ids, .. } => ids.ids.len(),
        }
    }

    fn at(&self, row: usize) -> Option<At<'_>> {
        match &self.rows {
            Rows::Loaded(v) => match v.get(row)? {
                Entry::Header { label, meta } => Some(At::Header(label, meta)),
                Entry::Track(t) => Some(At::Loaded(t)),
            },
            Rows::Snapshot { ids, pages } => {
                let page = row / PAGE;
                let rows = {
                    let mut cache = pages.borrow_mut();
                    match cache.map.get(&page) {
                        Some(r) => r.clone(),
                        None => {
                            let end = ((page + 1) * PAGE).min(ids.ids.len());
                            let wanted = &ids.ids[page * PAGE..end];
                            let read = Rc::new(app::with(|api| api.track_rows(wanted)));
                            cache.map.insert(page, read.clone());
                            cache.order.push_back(page);
                            if cache.order.len() > PAGES_KEPT {
                                if let Some(old) = cache.order.pop_front() {
                                    cache.map.remove(&old);
                                }
                            }
                            read
                        }
                    }
                };
                (row % PAGE < rows.len()).then(|| At::Track(rows, row % PAGE))
            }
        }
    }

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

    fn track_id(&self, row: usize) -> Option<i64> {
        match self.at(row)? {
            At::Track(page, i) => Some(page[i].id),
            At::Loaded(t) => Some(t.id),
            At::Header(..) => None,
        }
    }

    /// Tracks belonging to a loaded group header, up to the next header.
    /// The boundary is label-agnostic, so discs, acts and scenes share it.
    fn group_paths(&self, row: usize) -> Vec<String> {
        let Rows::Loaded(entries) = &self.rows else {
            return Vec::new();
        };
        if !matches!(entries.get(row), Some(Entry::Header { .. })) {
            return Vec::new();
        }
        entries[row + 1..].iter().take_while(|entry| !matches!(entry, Entry::Header { .. })).filter_map(|entry| match entry {
            Entry::Track(track) => Some(track.path.clone()),
            Entry::Header { .. } => None,
        }).collect()
    }
}

fn role_value(list: &TrackListRust, row: usize, role: i32) -> QVariant {
    let Some(at) = list.at(row) else {
        return QVariant::default();
    };
    let track = match &at {
        At::Header(label, meta) => {
            return match role - USER_ROLE {
                0 => QVariant::from(&1i32),
                11 => QVariant::from(&QString::from(*label)),
                12 => QVariant::from(&QString::from(*meta)),
                15 => QVariant::from(&QUrl::default()),
                _ => QVariant::default(),
            };
        }
        At::Track(page, i) => &page[*i],
        At::Loaded(t) => *t,
    };
    let text = |s: &str| QVariant::from(&QString::from(s));
    match role - USER_ROLE {
        0 => QVariant::from(&0i32),
        1 => {
            let n = if list.by_track_number { i32::from(track.number) } else { row as i32 + 1 };
            QVariant::from(&n)
        }
        2 => text(&track.title),
        3 => text(&track.artist),
        4 => text(&track.album),
        5 => text(&format::clock(track.duration_ms)),
        6 => QVariant::from(&(track.plays as i32)),
        7 => text(&track.quality.short()),
        8 => QVariant::from(&track.quality.tier.rank()),
        9 => QVariant::from(&(!list.playing.is_empty() && track.path == list.playing)),
        10 => QVariant::from(&list.selected.contains(&track.id)),
        13 => QVariant::from(&track.id),
        14 => QVariant::from(&track.album_id),
        15 => {
            let url = if track.artwork_path.is_empty() { QUrl::default() } else { QUrl::from_local_file(&QString::from(&track.artwork_path)) };
            QVariant::from(&url)
        }
        _ => QVariant::default(),
    }
}

/// Adds files to the end of the queue, on the output in use.
fn enqueue(paths: &[String]) {
    app::with(|api| api.enqueue_here(paths));
}

/// Puts files right after what is playing, on the output in use.
fn next(paths: &[String]) {
    app::with(|api| api.play_next_here(paths));
}

fn summary(tracks: usize, ms: u64) -> String {
    let n = tracks.to_string();
    let songs = app::with(|api| api.tf("{} song", "{} songs", tracks as i64, &[&n]));
    format!("{songs} · {}", format::span(ms))
}

impl qobject::TrackListModel {
    pub fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        role_value(self.rust(), index.row().max(0) as usize, role)
    }

    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut roles = QHash::<QHashPair_i32_QByteArray>::default();
        for (i, name) in ROLES.iter().enumerate() {
            roles.insert(USER_ROLE + i as i32, QByteArray::from(*name));
        }
        roles
    }

    pub fn row_count(&self, _parent: &QModelIndex) -> i32 {
        self.rust().len() as i32
    }

    pub fn play(self: Pin<&mut Self>, row: i32) {
        let row = row.max(0) as usize;
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
                worker::submit(move |reader| {
                    let paths = reader.paths(&ids);
                    let _ = thread.queue(move |_| app::with(|api| api.play_from(&paths, row)));
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
        self.with_all_paths(move |paths| {
            app::with(|api| {
                api.play_later_here(paths, &label);
            })
        });
    }

    pub fn group_action(self: Pin<&mut Self>, row: i32, action: &QString, label: &QString) {
        let paths = self.rust().group_paths(row.max(0) as usize);
        if paths.is_empty() {
            return;
        }
        app::with(|api| match action.to_string().as_str() {
            "play" => api.play_from(&paths, 0),
            "queue" => api.enqueue_here(&paths),
            "next" => api.play_next_here(&paths),
            "later" => { api.play_later_here(&paths, &label.to_string()); }
            _ => {}
        });
    }

    fn with_all_paths(&self, then: impl FnOnce(&[String]) + Send + 'static) {
        match &self.rust().rows {
            Rows::Loaded(_) => {
                let paths: Vec<String> = self.rust().loaded_tracks().iter().map(|t| t.path.clone()).collect();
                then(&paths);
            }
            Rows::Snapshot { ids, .. } => {
                let ids = ids.ids.clone();
                let thread = self.qt_thread();
                worker::submit(move |reader| {
                    let paths = reader.paths(&ids);
                    let _ = thread.queue(move |_| then(&paths));
                });
            }
        }
    }

    pub fn play_all(self: Pin<&mut Self>, shuffled: bool) {
        app::with(|api| api.set_shuffle(shuffled));
        self.play(0);
    }

    pub fn select(mut self: Pin<&mut Self>, row: i32, modifiers: i32) {
        let row = row.max(0) as usize;
        let (selected, anchor) = chilpa_api::select_tracks(&self.rust().selected, self.rust().anchor, row, modifiers, |at| self.rust().track_id(at));
        let mut this = self.as_mut().rust_mut();
        this.selected = selected;
        this.anchor = anchor;
        self.changed(ROLE_SELECTED);
    }

    pub fn queue(self: Pin<&mut Self>, row: i32) {
        if !self.in_selection(row) {
            if let Some(track) = self.rust().track(row.max(0) as usize) {
                enqueue(&[track.path]);
            }
            return;
        }
        self.with_selection(enqueue);
    }

    pub fn play_chosen(self: Pin<&mut Self>, row: i32) {
        if !self.in_selection(row) {
            self.play(row);
            return;
        }
        self.with_selection(|paths| app::with(|api| api.play_from(paths, 0)));
    }

    pub fn play_next(self: Pin<&mut Self>, row: i32) {
        if !self.in_selection(row) {
            if let Some(track) = self.rust().track(row.max(0) as usize) {
                next(&[track.path]);
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
        let later = move |paths: &[String]| {
            app::with(|api| api.play_later_here(paths, &label));
        };
        if !self.in_selection(row) {
            if let Some(track) = self.rust().track(row.max(0) as usize) {
                later(&[track.path]);
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
        if self.rust().kind.to_string() != "playlist" || row < 0 {
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
        if self.rust().kind.to_string() != "playlist" || self.rust().order.to_string() != "custom" || from < 0 || to < 0 || from == to {
            return;
        }
        let playlist = self.rust().album_id;
        if app::with(|api| api.move_in_playlist(playlist, from as usize, to as usize)) {
            registry::playlists(|lists| lists.changed());
        }
    }

    pub fn selected_row(&self) -> i32 {
        let this = self.rust();
        if this.selected.len() != 1 {
            return -1;
        }
        (0..this.len()).find(|&r| this.track_id(r).is_some_and(|id| this.selected.contains(&id))).map_or(-1, |r| r as i32)
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
        let this = self.rust();
        let editable = this.kind.to_string() == "playlist" && this.playlist_editable;
        let context = chilpa_api::TrackMenuContext {
            count: if row < 0 { 0 } else { self.chosen_track_ids(row).len() },
            queued: this.kind.to_string() == "queue",
            editable,
            move_up: editable && this.order.to_string() == "custom" && row > 0,
            move_down: editable && this.order.to_string() == "custom" && row < this.len() as i32 - 1,
            album: self.can_go(row, &QString::from("album")),
            artist: self.can_go(row, &QString::from("artist")),
            folder: self.can_go(row, &QString::from("folder")),
            metadata: self.can_go(row, &QString::from("metadata")),
        };
        QString::from(serde_json::to_string(&chilpa_api::track_menu(context)).unwrap_or_default())
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
    fn with_selection(&self, then: impl FnOnce(&[String]) + Send + 'static) {
        let this = self.rust();
        match &this.rows {
            Rows::Loaded(_) => {
                let paths: Vec<String> = this.loaded_tracks().iter().filter(|t| this.selected.contains(&t.id)).map(|t| t.path.clone()).collect();
                then(&paths);
            }
            Rows::Snapshot { ids, .. } => {
                let chosen: Vec<i64> = ids.ids.iter().copied().filter(|id| this.selected.contains(id)).collect();
                let thread = self.qt_thread();
                worker::submit(move |reader| {
                    let paths = reader.paths(&chosen);
                    let _ = thread.queue(move |_| then(&paths));
                });
            }
        }
    }

    pub fn can_go(&self, row: i32, target: &QString) -> bool {
        let Some(track) = self.rust().track(row.max(0) as usize) else {
            return false;
        };
        let kind = self.rust().kind.to_string();
        match target.to_string().as_str() {
            "album" => track.album_id > 0 && kind != "album",
            "artist" => {
                let own = matches!(kind.as_str(), "artist" | "artistTop") && track.artist.eq_ignore_ascii_case(&self.rust().key.to_string());
                !track.artist.is_empty() && !own
            }
            "folder" => kind != "folder",
            "metadata" => track.album_id > 0,
            _ => false,
        }
    }

    pub fn go(self: Pin<&mut Self>, row: i32, target: &QString) {
        let Some(track) = self.rust().track(row.max(0) as usize) else {
            return;
        };
        let target = target.to_string();
        if target == "metadata" {
            // The editor opens on this track before its screen shows.
            let (album, id) = (track.album_id, track.id);
            registry::metadata(move |editor| {
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
        registry::navigation(move |mut nav| match target.as_str() {
            "album" => nav.as_mut().go(&QString::from("album"), track.album_id),
            "metadata" => nav.as_mut().go(&QString::from("metadata"), track.album_id),
            "artist" => nav.as_mut().open_artist(&QString::from(&track.artist)),
            "folder" => {
                let dir = std::path::Path::new(&track.path).parent().map(|d| d.display().to_string());
                if let Some(dir) = dir {
                    nav.as_mut().open_folder(&QString::from(&dir));
                }
            }
            _ => {}
        });
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
        if self.rust().pending {
            return;
        }
        self.as_mut().rust_mut().pending = true;
        let _ = self.qt_thread().queue(|model| model.reload());
    }

    fn reload(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().pending = false;
        let (id, limit) = (self.rust().album_id, self.rust().limit.max(1) as usize);
        let kind = self.rust().kind.to_string();
        // The rows, and how long they last when the list says so.
        let (rows, length) = match kind.as_str() {
            "album" if id > 0 => {
                let entries = app::with(|api| api.album_track_entries(id));
                let ms = entries.iter().filter_map(|entry| match entry { Entry::Track(track) => Some(track.duration_ms), Entry::Header { .. } => None }).sum::<u64>();
                (Rows::Loaded(entries), Some(ms))
            }
            "mostPlayed" => {
                let tracks = app::with(|api| api.most_played_rows(limit));
                (Rows::Loaded(tracks.into_iter().map(Entry::Track).collect()), None)
            }
            "artist" | "artistTop" => {
                let name = self.rust().key.to_string();
                let top = kind == "artistTop";
                let (order, count) = if top { (TrackOrder::Plays, 10) } else { (TrackOrder::Artist, 100_000) };
                let mut tracks = app::with(|api| api.artist_tracks(&name, order, count));
                // What never played is not "most played".
                if top {
                    tracks.retain(|t| t.plays > 0);
                }
                let ms = tracks.iter().map(|t| t.duration_ms).sum::<u64>();
                (Rows::Loaded(tracks.into_iter().map(Entry::Track).collect()), (!top).then_some(ms))
            }
            "folder" => {
                let tracks = super::folder_view::tracks();
                let ms = tracks.iter().map(|t| t.duration_ms).sum::<u64>();
                (Rows::Loaded(tracks.iter().cloned().map(Entry::Track).collect()), Some(ms))
            }
            "playlist" => {
                let order = self.rust().order.to_string();
                let ascending = self.rust().ascending;
                let tracks = app::with(|api| api.playlist_rows_sorted(id, &order, ascending));
                let ms = tracks.iter().map(|t| t.duration_ms).sum::<u64>();
                (Rows::Loaded(tracks.into_iter().map(Entry::Track).collect()), Some(ms))
            }
            "queue" => {
                let tracks = app::with(|api| api.queue_rows());
                let ms = tracks.iter().map(|t| t.duration_ms).sum::<u64>();
                (Rows::Loaded(tracks.into_iter().map(Entry::Track).collect()), Some(ms))
            }
            "library" => {
                let snapshot = browse::read(|s| s.snapshot.clone());
                let ms = snapshot.totals.duration_ms;
                (Rows::Snapshot { ids: snapshot, pages: RefCell::default() }, Some(ms))
            }
            _ => (Rows::default(), None),
        };
        // The same list read again -- a track moved in a playlist, the filter
        // narrowed -- keeps what is selected and still there; another list
        // starts with nothing selected.
        let identity = (kind.clone(), id, self.rust().key.to_string());
        let same = self.rust().loaded.as_ref() == Some(&identity);
        // SAFETY: the reset is begun and ended around the swap, as Qt asks.
        unsafe { self.as_mut().begin_reset_model() };
        {
            let mut this = self.as_mut().rust_mut();
            this.rows = rows;
            this.by_track_number = kind == "album";
            if same {
                let there: HashSet<i64> = this.loaded_tracks().iter().map(|t| t.id).collect();
                if matches!(this.rows, Rows::Loaded(_)) {
                    this.selected.retain(|id| there.contains(id));
                }
            } else {
                this.selected.clear();
                this.anchor = None;
            }
            this.loaded = Some(identity);
        }
        unsafe { self.as_mut().end_reset_model() };
        let chosen = self.rust().selected.len() as i32;
        self.as_mut().set_selected_count(chosen);
        let tracks = match &self.rust().rows {
            Rows::Loaded(_) => self.rust().loaded_tracks().len(),
            Rows::Snapshot { ids, .. } => ids.ids.len(),
        };
        self.as_mut().set_count(tracks as i32);
        let text = length.map(|ms| summary(tracks, ms)).unwrap_or_default();
        self.as_mut().set_summary(QString::from(&text));
    }
}

impl cxx_qt::Initialize for qobject::TrackListModel {
    fn initialize(mut self: Pin<&mut Self>) {
        self.as_mut().on_kind_changed(|m| m.schedule()).release();
        self.as_mut().on_album_id_changed(|m| m.schedule()).release();
        self.as_mut().on_key_changed(|m| m.schedule()).release();
        self.as_mut().on_order_changed(|m| m.schedule()).release();
        self.as_mut().on_ascending_changed(|m| m.schedule()).release();
        self.as_mut().on_limit_changed(|m| m.schedule()).release();
        self.as_mut().on_filter_rev_changed(|m| m.schedule()).release();
        self.as_mut()
            .on_playing_path_changed(|mut m| {
                let now = m.playing_path().to_string();
                m.as_mut().rust_mut().playing = now;
                m.changed(ROLE_PLAYING);
            })
            .release();
    }
}
