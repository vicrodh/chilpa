//! The rows of an album grid (frontend ADR-0002, `AlbumGrid`).
//!
//! A grid recycles rows of cards, which also lets a quick view open between
//! rows. `columns` comes from the width; zero lays out nothing.
//!
//! A shelf ("recentlyPlayed", "recentlyAdded") is read whole. The whole
//! library ("all") is a snapshot of ids taken on the worker and read a page
//! of 128 albums at a time by id, as the track list does.

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
        /// "recentlyPlayed", "recentlyAdded", "all", "library" (the records
        /// the Library screen's filter leaves), "artist" (one artist's
        /// records, by `key`), "sameArtist" and "related" (an album id),
        /// "appearsOn" (the compilations they are on)
        /// "folder" (the records directly in its current directory) or
        /// "artists" (every artist, as round cards).
        #[qproperty(QString, kind)]
        #[qproperty(QString, key)]
        /// For "all": "artist", "title", "year", "added", "played" or
        /// "last_played".
        #[qproperty(QString, order)]
        #[qproperty(i32, limit)]
        #[qproperty(i32, columns)]
        /// Zero lays out every row; a shelf asks for one.
        #[qproperty(i32, max_rows)]
        #[qproperty(i32, albums)]
        #[qproperty(i32, rev)]
        /// Bound to `Library.contentRev`: the library changed. A grid of
        /// kind "library" binds it to `Library.rev`: the filter moved.
        #[qproperty(i32, content_rev)]
        /// For "artists" and "all": only what holds every word of it -- the
        /// name, or the record's title or artist.
        #[qproperty(QString, filter)]
        /// The card whose quick view is open, or -1: one per grid.
        #[qproperty(i32, expanded)]
        type AlbumGridModel = super::AlbumGridRust;
        #[qinvokable]
        fn title(self: &AlbumGridModel, index: i32, rev: i32) -> QString;
        #[qinvokable]
        fn subtitle(self: &AlbumGridModel, index: i32, rev: i32) -> QString;
        #[qinvokable]
        fn year(self: &AlbumGridModel, index: i32, rev: i32) -> QString;
        #[qinvokable]
        fn art(self: &AlbumGridModel, index: i32, rev: i32) -> QUrl;
        #[qinvokable]
        fn album_id(self: &AlbumGridModel, index: i32, rev: i32) -> i64;
        #[qinvokable]
        fn prepare_open(self: &AlbumGridModel, index: i32);

        /// Selects on a click: 1 (Ctrl) flips one record, 2 (Shift) takes
        /// the records from the last one clicked (ADR-0007).
        #[qinvokable]
        fn select(self: Pin<&mut AlbumGridModel>, index: i32, modifiers: i32);
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
use crate::{app, registry, worker};
use chilpa_api::{Album, AlbumSort};
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use cxx_qt::{CxxQtType as _, Threading as _};
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QUrl, QVariant};
use std::pin::Pin;

const USER_ROLE: i32 = 256;
const PAGE: usize = 128;
const PAGES_KEPT: usize = 24;
#[derive(Default)]
struct Pages {
    map: HashMap<usize, Rc<Vec<Album>>>,
    order: VecDeque<usize>,
}
enum Albums {
    Loaded(Vec<Album>),
    Snapshot { ids: Vec<i64>, pages: RefCell<Pages> },
}
impl Default for Albums {
    fn default() -> Self {
        Albums::Loaded(Vec::new())
    }
}
impl Albums {
    fn len(&self) -> usize {
        match self {
            Albums::Loaded(v) => v.len(),
            Albums::Snapshot { ids, .. } => ids.len(),
        }
    }
    /// Runs `f` with the album at `index`, reading its page if it is not
    /// cached. By id: any page costs what the first one costs.
    fn with<R>(&self, index: usize, f: impl FnOnce(&Album) -> R) -> Option<R> {
        match self {
            Albums::Loaded(v) => v.get(index).map(f),
            Albums::Snapshot { ids, pages } => {
                if index >= ids.len() {
                    return None;
                }
                let page = index / PAGE;
                let rows = {
                    let mut cache = pages.borrow_mut();
                    match cache.map.get(&page) {
                        Some(r) => r.clone(),
                        None => {
                            let end = ((page + 1) * PAGE).min(ids.len());
                            let read = Rc::new(app::with(|api| api.albums_by_ids(&ids[page * PAGE..end])));
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
                rows.get(index % PAGE).map(f)
            }
        }
    }
}
#[derive(Default)]
pub struct AlbumGridRust {
    kind: QString,
    key: QString,
    order: QString,
    limit: i32,
    columns: i32,
    max_rows: i32,
    albums: i32,
    rev: i32,
    content_rev: i32,
    filter: QString,
    expanded: i32,
    anchor: Option<i32>,
    /// Every artist, for "artists", kept so the filter needs no query.
    artists: Vec<chilpa_api::Artist>,
    list: Albums,
    rows: usize,
    pending: bool,
    /// Tags the worker's answers, so one for an old order is dropped.
    generation: u64,
}

impl AlbumGridRust {
    fn get<R>(&self, index: i32, f: impl FnOnce(&Album) -> R) -> Option<R> {
        self.list.with(usize::try_from(index).ok()?, f)
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
    pub fn prepare_open(&self, index: i32) {
        if !matches!(self.rust().kind.to_string().as_str(), "artist" | "appearsOn") { return; }
        if let Some(id) = self.rust().get(index, |album| album.id) {
            app::with(|api| {
                if !api.group_artist_versions() { api.select_album_version(id, id); }
            });
        }
    }

    pub fn title(&self, index: i32, _rev: i32) -> QString {
        self.rust().get(index, |a| QString::from(&a.title)).unwrap_or_default()
    }

    pub fn subtitle(&self, index: i32, _rev: i32) -> QString {
        self.rust().get(index, |a| QString::from(&a.artist)).unwrap_or_default()
    }

    pub fn year(&self, index: i32, _rev: i32) -> QString {
        self.rust().get(index, |a| a.year.map(|year| QString::from(&year.to_string())).unwrap_or_default()).unwrap_or_default()
    }

    pub fn art(&self, index: i32, _rev: i32) -> QUrl {
        self.rust()
            .get(index, |a| {
                if a.artwork_path.is_empty() {
                    QUrl::default()
                } else {
                    QUrl::from_local_file(&QString::from(&a.artwork_path))
                }
            })
            .unwrap_or_default()
    }

    pub fn album_id(&self, index: i32, _rev: i32) -> i64 {
        self.rust().get(index, |a| a.id).unwrap_or(0)
    }

    pub fn select(mut self: Pin<&mut Self>, index: i32, modifiers: i32) {
        let from = match self.rust().anchor {
            Some(a) if modifiers & 2 != 0 => a,
            _ => index,
        };
        let ids: Vec<i64> =
            (from.min(index)..=from.max(index)).filter_map(|i| self.rust().get(i, |a| a.id)).collect();
        if modifiers & 2 == 0 {
            self.as_mut().rust_mut().anchor = Some(index);
        }
        let toggle = modifiers & 2 == 0;
        registry::album_actions(move |actions| actions.select(ids, toggle));
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
            "all" => {
                // Sorting the whole library does not fit in a frame: the
                // worker takes the snapshot and the rows follow.
                let generation = self.rust().generation + 1;
                self.as_mut().rust_mut().generation = generation;
                let order = AlbumSort::from_name(&self.rust().order.to_string());
                let text = self.rust().filter.to_string();
                let thread = self.qt_thread();
                worker::submit(move |reader| {
                    let ids = reader.album_ids_matching(order, &text);
                    let _ = thread.queue(move |model| model.arrive(generation, ids));
                });
                return;
            }
            "artist" => {
                let name = self.rust().key.to_string();
                app::with(|api| api.artist_release_cards(&name, false))
            }
            "appearsOn" => {
                let name = self.rust().key.to_string();
                app::with(|api| api.artist_release_cards(&name, true))
            }
            "sameArtist" => {
                let id = self.rust().key.to_string().parse().unwrap_or_default();
                app::with(|api| api.same_artist_albums(id, limit))
            }
            "related" => app::with(|api| api.related_albums(self.rust().key.to_string().parse().unwrap_or_default(), limit)),
            "artists" => {
                // An artist is drawn as a card too: its name where a record's
                // title goes, how many records it has under it.
                let generation = self.rust().generation + 1;
                self.as_mut().rust_mut().generation = generation;
                let thread = self.qt_thread();
                worker::submit(move |reader| {
                    let artists = reader.artists();
                    let _ = thread.queue(move |model| model.arrive_artists(generation, artists));
                });
                return;
            }
            "library" => {
                // Unlike the facet, this includes the album choice itself
                // and follows the result's requested order.
                let ids = crate::browse::read(|s| s.snapshot.album_ids.clone());
                return self.show(Albums::Snapshot { ids, pages: RefCell::default() });
            }
            "folder" => {
                let ids = crate::bridge::folder_view::album_ids();
                return self.show(Albums::Snapshot { ids, pages: RefCell::default() });
            }
            _ => Vec::new(),
        };
        self.show(Albums::Loaded(list));
    }

    fn arrive_artists(mut self: Pin<&mut Self>, generation: u64, artists: Vec<chilpa_api::Artist>) {
        if generation != self.rust().generation {
            return;
        }
        self.as_mut().rust_mut().artists = artists;
        self.show_artists();
    }

    /// The artists the filter leaves, as cards.
    fn show_artists(self: Pin<&mut Self>) {
        let words: Vec<String> = self.rust().filter.to_string().to_lowercase().split_whitespace().map(String::from).collect();
        let cards = self
            .rust()
            .artists
            .iter()
            .filter(|a| {
                let name = a.name.to_lowercase();
                words.iter().all(|w| name.contains(w.as_str()))
            })
            .cloned()
            .map(|a| {
                let n = a.album_count.to_string();
                Album {
                    id: 0,
                    artist: app::with(|api| api.tf("{} record", "{} records", a.album_count as i64, &[&n])),
                    title: a.name,
                    year: None,
                    track_count: a.track_count,
                    artwork_path: a.artwork_path,
                    play_count: 0,
                }
            })
            .collect();
        self.show(Albums::Loaded(cards));
    }

    fn arrive(self: Pin<&mut Self>, generation: u64, ids: Vec<i64>) {
        if generation == self.rust().generation {
            self.show(Albums::Snapshot { ids, pages: RefCell::default() });
        }
    }

    fn show(mut self: Pin<&mut Self>, list: Albums) {
        let opened = self.rust().get(self.rust().expanded, |a| a.id)
            .map(|id| app::with(|api| api.selected_album_version(id)));
        let expanded = match (&list, opened) {
            (Albums::Loaded(cards), Some(id)) => cards.iter().position(|a| a.id == id),
            (Albums::Snapshot { ids, .. }, Some(id)) => ids.iter().position(|a| *a == id),
            _ => None,
        }.map(|at| at as i32).unwrap_or(-1);
        let count = list.len() as i32;
        self.as_mut().rust_mut().list = list;
        self.as_mut().rust_mut().anchor = None;
        self.as_mut().set_expanded(expanded);
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
        self.as_mut().set_expanded(-1);
        self.as_mut().on_kind_changed(|m| m.schedule()).release();
        self.as_mut().on_key_changed(|m| m.schedule()).release();
        self.as_mut().on_order_changed(|m| m.schedule()).release();
        self.as_mut().on_limit_changed(|m| m.schedule()).release();
        self.as_mut().on_content_rev_changed(|m| m.schedule()).release();
        self.as_mut()
            .on_filter_changed(|m| match m.kind().to_string().as_str() {
                "artists" => m.show_artists(),
                "all" => m.schedule(),
                _ => {}
            })
            .release();
        self.as_mut().on_columns_changed(|m| m.split_rows()).release();
        self.as_mut().on_max_rows_changed(|m| m.split_rows()).release();
    }
}
