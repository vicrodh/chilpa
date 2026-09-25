//! The rows of every track list in the window (frontend ADR-0006).
//!
//! One model for every list, configured by `kind`: a record ("album"), the
//! most played ("mostPlayed"), the queue from what plays on ("queue"), or
//! what the Library screen's filter leaves ("library"). A record with several discs carries its disc headers as
//! rows of their own, not as separate lists.
//!
//! A small list is read whole. The library is a snapshot of ids, and its
//! rows are read by id a page at a time as they scroll into view, with a
//! bounded cache: page 400 costs what page 1 costs, and a hundred thousand
//! tracks never sit in memory as rows.
//!
//! Inputs (`kind`, `albumId`, `filterRev`) can arrive one after the other
//! while a screen is built; they are merged into one reload per turn of the
//! event loop. The playing track and the selection change rows in place,
//! with `dataChanged` on one role, never with a reset.

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
        /// "album", "mostPlayed" or "library".
        #[qproperty(QString, kind)]
        #[qproperty(i64, album_id)]
        /// The artist, for "artist" (every track) and "artistTop" (the most
        /// played ten).
        #[qproperty(QString, key)]
        /// How many rows "mostPlayed" brings.
        #[qproperty(i32, limit)]
        /// Bound to `Library.rev`: the library's filter moved.
        #[qproperty(i32, filter_rev)]
        /// Bound to `Player.path`: which row is playing.
        #[qproperty(QString, playing_path)]
        /// Tracks, headers excluded.
        #[qproperty(i32, count)]
        /// "58 songs · 4 h 18 min".
        #[qproperty(QString, summary)]
        type TrackListModel = super::TrackListRust;

        /// Plays the list from that row: the list becomes what plays next.
        #[qinvokable]
        fn play(self: Pin<&mut TrackListModel>, row: i32);

        /// Plays the whole list from the top, shuffled or not.
        #[qinvokable]
        fn play_all(self: Pin<&mut TrackListModel>, shuffled: bool);

        /// Adds the whole list to the end of the queue.
        #[qinvokable]
        fn queue_all(self: Pin<&mut TrackListModel>);

        /// Selects a row. `modifiers`: 1 toggles it (Ctrl or Cmd), 2 extends
        /// from the anchor (Shift).
        #[qinvokable]
        fn select(self: Pin<&mut TrackListModel>, row: i32, modifiers: i32);

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
        fn data_changed(
            self: Pin<&mut TrackListModel>,
            top_left: &QModelIndex,
            bottom_right: &QModelIndex,
            roles: &QVector_i32,
        );
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

use crate::{app, browse, format, worker};
use chilpa_api::{TrackOrder, TrackRow};
use cxx_qt::{CxxQtType as _, Threading as _};
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant, QVector};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::pin::Pin;
use std::rc::Rc;

/// The roles, in order from `Qt::UserRole`.
const ROLES: &[&str] = &[
    "rowKind", "number", "title", "artist", "album", "duration", "plays", "quality", "tier",
    "playing", "selected", "headerLabel", "headerMeta", "trackId", "albumId",
];
const USER_ROLE: i32 = 256;
const ROLE_PLAYING: i32 = USER_ROLE + 9;
const ROLE_SELECTED: i32 = USER_ROLE + 10;

/// Rows read at a time from a snapshot, and how many pages stay cached.
const PAGE: usize = 128;
const PAGES_KEPT: usize = 32;

enum Entry {
    Header { label: String, meta: String },
    Track(TrackRow),
}

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
    limit: i32,
    filter_rev: i32,
    playing_path: QString,
    count: i32,
    summary: QString,
    rows: Rows,
    playing: String,
    /// A record numbers its rows by track number; any other list, by
    /// position.
    by_track_number: bool,
    selected: HashSet<i64>,
    anchor: Option<usize>,
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
            Rows::Loaded(v) => v
                .iter()
                .filter_map(|e| if let Entry::Track(t) = e { Some(t) } else { None })
                .collect(),
            Rows::Snapshot { .. } => Vec::new(),
        }
    }

    fn track_id(&self, row: usize) -> Option<i64> {
        match self.at(row)? {
            At::Track(page, i) => Some(page[i].id),
            At::Loaded(t) => Some(t.id),
            At::Header(..) => None,
        }
    }
}

fn role_value(list: &TrackListRust, row: usize, role: i32) -> QVariant {
    let Some(at) = list.at(row) else { return QVariant::default() };
    let track = match &at {
        At::Header(label, meta) => {
            return match role - USER_ROLE {
                0 => QVariant::from(&1i32),
                11 => QVariant::from(&QString::from(*label)),
                12 => QVariant::from(&QString::from(*meta)),
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
        _ => QVariant::default(),
    }
}

/// A record's tracks, with a header before each disc when there is more
/// than one.
fn with_discs(rows: Vec<TrackRow>) -> Vec<Entry> {
    let mut discs: Vec<u16> = rows.iter().map(|t| t.disc.max(1)).collect();
    discs.dedup();
    if discs.len() < 2 {
        return rows.into_iter().map(Entry::Track).collect();
    }
    let mut out = Vec::with_capacity(rows.len() + discs.len());
    let mut current = None;
    for (i, t) in rows.iter().enumerate() {
        let disc = t.disc.max(1);
        if current != Some(disc) {
            current = Some(disc);
            let songs = rows[i..].iter().take_while(|r| r.disc.max(1) == disc).count();
            let n = songs.to_string();
            out.push(Entry::Header {
                label: app::with(|api| api.t_args("Disc {}", &[&disc.to_string()])),
                meta: app::with(|api| api.tf("{} song", "{} songs", songs as i64, &[&n])),
            });
        }
        out.push(Entry::Track(t.clone()));
    }
    out
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
                let start = entries[..row.min(entries.len())]
                    .iter()
                    .filter(|e| matches!(e, Entry::Track(_)))
                    .count();
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
        match &self.rust().rows {
            Rows::Loaded(_) => {
                let paths: Vec<String> = self.rust().loaded_tracks().iter().map(|t| t.path.clone()).collect();
                app::with(|api| {
                    let (device, bit_perfect) = api.chosen_output();
                    api.enqueue(&paths, &device, bit_perfect);
                });
            }
            Rows::Snapshot { ids, .. } => {
                let ids = ids.ids.clone();
                let thread = self.qt_thread();
                worker::submit(move |reader| {
                    let paths = reader.paths(&ids);
                    let _ = thread.queue(move |_| {
                        app::with(|api| {
                            let (device, bit_perfect) = api.chosen_output();
                            api.enqueue(&paths, &device, bit_perfect);
                        })
                    });
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
        let Some(id) = self.rust().track_id(row) else { return };
        let anchor = self.rust().anchor;
        if modifiers & 2 != 0 && anchor.is_some() {
            let from = anchor.unwrap_or(row);
            let (lo, hi) = (from.min(row), from.max(row));
            let ids: Vec<i64> = (lo..=hi).filter_map(|r| self.rust().track_id(r)).collect();
            let mut this = self.as_mut().rust_mut();
            this.selected = ids.into_iter().collect();
        } else if modifiers & 1 != 0 {
            let mut this = self.as_mut().rust_mut();
            if !this.selected.remove(&id) {
                this.selected.insert(id);
            }
            this.anchor = Some(row);
        } else {
            let mut this = self.as_mut().rust_mut();
            this.selected = [id].into_iter().collect();
            this.anchor = Some(row);
        }
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
                let tracks = app::with(|api| api.album_track_rows(id));
                let ms = tracks.iter().map(|t| t.duration_ms).sum::<u64>();
                (Rows::Loaded(with_discs(tracks)), Some(ms))
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
        // SAFETY: the reset is begun and ended around the swap, as Qt asks.
        unsafe { self.as_mut().begin_reset_model() };
        {
            let mut this = self.as_mut().rust_mut();
            this.rows = rows;
            this.by_track_number = kind == "album";
            this.selected.clear();
            this.anchor = None;
        }
        unsafe { self.as_mut().end_reset_model() };
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
