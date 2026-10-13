//! Album grid rows, paged snapshots and shelf models (frontend ADR-0002).
//! Zero columns lays out nothing; library pages hold 128 records each.

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
        include!("cxx-qt-lib/qvector.h");
        type QVector_i32 = cxx_qt_lib::QVector<i32>;
    }
    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[base = QAbstractListModel]
        #[qml_element]
        /// "recentlyPlayed", "recentlyPlayedEverywhere" (with the sources'),
        /// "recentlyAdded", "all", "library" (the records
        /// the Library screen's filter leaves), "artist" (one artist's
        /// records, by `key`), "sameArtist" and "related" (an album id),
        /// "appearsOn" (the compilations they are on)
        /// "folder" (the records directly in its current directory) or
        /// "artists" (the artists, as round cards; `key` is "all" for
        /// everyone who plays, and the album artists otherwise).
        #[qproperty(QString, kind)]
        #[qproperty(QString, key)]
        /// For "all": "artist", "title", "year", "added", "played" or
        /// "last_played".
        #[qproperty(QString, order)]
        /// "letter", "artist" or nothing: how the records of "all" are grouped.
        #[qproperty(QString, group)]
        #[qproperty(i32, limit)]
        #[qproperty(i32, columns)]
        /// Zero lays out every row; a shelf asks for one.
        #[qproperty(i32, max_rows)]
        #[qproperty(i32, albums)]
        #[qproperty(bool, ready)]
        #[qproperty(i32, rev)]
        /// Bound to `Library.contentRev`: the library changed. A grid of
        /// kind "library" binds it to `Library.rev`: the filter moved.
        #[qproperty(i32, content_rev)]
        /// For "artists" and "all": only what holds every word of it -- the
        /// name, or the record's title or artist.
        #[qproperty(QString, filter)]
        /// The card whose quick view is open, or -1: one per grid.
        #[qproperty(i32, expanded)]
        /// The first card of what was just added to the end, or -1: a list
        /// that grew keeps its cards and only takes the new ones in.
        #[qproperty(i32, fresh)]
        /// The library's records beside a source's: they wear the player's badge.
        #[qproperty(bool, mixed)]
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
        #[inherit]
        #[cxx_name = "beginInsertRows"]
        unsafe fn begin_insert_rows(self: Pin<&mut AlbumGridModel>, parent: &QModelIndex, first: i32, last: i32);
        #[inherit]
        #[cxx_name = "endInsertRows"]
        unsafe fn end_insert_rows(self: Pin<&mut AlbumGridModel>);
        #[inherit]
        #[qsignal]
        #[cxx_name = "dataChanged"]
        fn data_changed(self: Pin<&mut AlbumGridModel>, top_left: &QModelIndex, bottom_right: &QModelIndex, roles: &QVector_i32);
        #[inherit]
        fn index(self: &AlbumGridModel, row: i32, column: i32, parent: &QModelIndex) -> QModelIndex;
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
use chilpa_api::Album;
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use cxx_qt::{CxxQtType as _, Threading as _};
use cxx_qt_lib::{QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QUrl, QVariant, QVector};
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
    fn mixed(&self) -> bool {
        match self {
            Albums::Loaded(v) => chilpa_api::mixes_sources(v.iter().map(|a| a.id)),
            Albums::Snapshot { ids, .. } => chilpa_api::mixes_sources(ids.iter().copied()),
        }
    }
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
    group: QString,
    limit: i32,
    columns: i32,
    max_rows: i32,
    albums: i32,
    ready: bool,
    rev: i32,
    content_rev: i32,
    filter: QString,
    expanded: i32,
    fresh: i32,
    mixed: bool,
    anchor: Option<i32>,
    /// The artists of "artists", kept so the filter needs no query.
    artists: Vec<chilpa_api::Artist>,
    list: Albums,
    /// Where each group starts, and the rows that leaves at this width.
    groups: Vec<(usize, String)>,
    rows: Vec<chilpa_api::GridRow>,
    pending: bool,
    generation: u64,
}

impl AlbumGridRust {
    fn get<R>(&self, index: i32, f: impl FnOnce(&Album) -> R) -> Option<R> {
        self.list.with(usize::try_from(index).ok()?, f)
    }

    fn laid_out(&self) -> Vec<chilpa_api::GridRow> {
        chilpa_api::grid_rows(self.list.len(), self.columns.max(0) as usize, self.max_rows.max(0) as usize, &self.groups)
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
        let Some(row) = self.rust().rows.get(index.row().max(0) as usize) else { return QVariant::default() };
        match role - USER_ROLE {
            0 => QVariant::from(&(row.first as i32)),
            1 => QVariant::from(&(row.span as i32)),
            2 => QVariant::from(&QString::from(&row.label)),
            _ => QVariant::default(),
        }
    }

    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        crate::format::roles(USER_ROLE, &["first", "span", "group"])
    }

    pub fn row_count(&self, _parent: &QModelIndex) -> i32 {
        self.rust().rows.len() as i32
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
            "recentlyPlayedEverywhere" => app::with(|api| api.recently_played_everywhere(limit)),
            "recentlyAdded" => app::with(|api| api.recently_added_albums(limit)),
            "all" => {
                // Sorting the whole library does not fit in a frame: the
                // worker takes the snapshot and the rows follow.
                let generation = self.rust().generation + 1;
                self.as_mut().rust_mut().generation = generation;
                let (order, group, text) = (self.rust().order.to_string(), self.rust().group.to_string(), self.rust().filter.to_string());
                let extras = crate::browse::read(|s| s.filter.clone());
                let remote = app::with(|api| api.integrated_albums(&text, &extras));
                let thread = self.qt_thread();
                worker::submit(move |reader| {
                    let found = reader.albums_view_with_sources(&order, &group, &text, &extras, remote);
                    let _ = thread.queue(move |model| model.arrive(generation, found));
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
            // The artist's newest record, for the page's "Latest release".
            "artistLatest" => {
                let name = self.rust().key.to_string();
                app::with(|api| api.artist_release_cards(&name, false)).into_iter().take(1).collect()
            }
            "sameArtist" => {
                let id = self.rust().key.to_string().parse().unwrap_or_default();
                app::with(|api| api.same_artist_albums(id, limit))
            }
            "related" => app::with(|api| api.related_albums(self.rust().key.to_string().parse().unwrap_or_default(), limit)),
            "source" => {
                let (mut cards, mut groups) = app::with(|api| api.source_shelf(&self.rust().key.to_string(), &self.rust().order.to_string(), &self.rust().group.to_string(), &self.rust().filter.to_string()));
                if self.rust().limit > 0 {
                    cards.truncate(limit);
                    groups.retain(|(at, _)| *at < cards.len());
                }
                self.as_mut().rust_mut().groups = groups; cards
            }
            "artists" => {
                // An artist is drawn as a card too: its name where a record's
                // title goes, how many records it has under it.
                let generation = self.rust().generation + 1;
                self.as_mut().rust_mut().generation = generation;
                let role = self.rust().key.to_string();
                let thread = self.qt_thread();
                worker::submit(move |reader| {
                    let artists = reader.artists(&role);
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
                let ids = chilpa_qt_folders::bridge::folder_view::album_ids();
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
        let cards = chilpa_api::artist_cards(&self.rust().artists, &self.rust().filter.to_string());
        self.show(Albums::Loaded(cards));
    }

    fn arrive(mut self: Pin<&mut Self>, generation: u64, found: chilpa_api::AlbumsFound) {
        if generation == self.rust().generation {
            self.as_mut().rust_mut().groups = found.groups;
            self.show(Albums::Snapshot { ids: found.ids, pages: RefCell::default() });
        }
    }

    fn show(mut self: Pin<&mut Self>, list: Albums) {
        // I append a shelf's next page without moving its existing rows.
        if let (Albums::Loaded(old), Albums::Loaded(new)) = (&self.rust().list, &list) {
            if new.len() > old.len() && self.rust().groups.is_empty() && old.iter().zip(new).all(|(a, b)| a.id == b.id) {
                let from = old.len();
                return self.grow(list, from);
            }
        }
        self.as_mut().set_fresh(-1);
        let opened = self.rust().get(self.rust().expanded, |a| a.id)
            .map(|id| app::with(|api| api.selected_album_version(id)));
        let expanded = match (&list, opened) {
            (Albums::Loaded(cards), Some(id)) => cards.iter().position(|a| a.id == id),
            (Albums::Snapshot { ids, .. }, Some(id)) => ids.iter().position(|a| *a == id),
            _ => None,
        }.map(|at| at as i32).unwrap_or(-1);
        let count = list.len() as i32;
        self.as_mut().set_mixed(list.mixed());
        self.as_mut().rust_mut().list = list;
        self.as_mut().rust_mut().anchor = None;
        self.as_mut().set_expanded(expanded);
        self.as_mut().split_rows();
        self.as_mut().set_albums(count);
        let next = self.rev().wrapping_add(1);
        self.as_mut().set_rev(next);
        self.as_mut().set_ready(true);
    }

    /// Appends records without rebuilding the rows already shown.
    fn grow(mut self: Pin<&mut Self>, list: Albums, from: usize) {
        let count = list.len() as i32;
        self.as_mut().set_mixed(list.mixed());
        self.as_mut().rust_mut().list = list;
        let rows = self.rust().laid_out();
        let before = self.rust().rows.len();
        let filled = before > 0 && rows.get(before - 1).map(|row| row.span) != self.rust().rows.get(before - 1).map(|row| row.span);
        let parent = QModelIndex::default();
        if rows.len() > before {
            // SAFETY: the insertion is begun and ended around the swap, as Qt asks.
            unsafe { self.as_mut().begin_insert_rows(&parent, before as i32, rows.len() as i32 - 1) };
            self.as_mut().rust_mut().rows = rows;
            unsafe { self.as_mut().end_insert_rows() };
        } else {
            self.as_mut().rust_mut().rows = rows;
        }
        if filled {
            let last = self.index(before as i32 - 1, 0, &parent);
            self.as_mut().data_changed(&last, &last, &QVector::<i32>::default());
        }
        self.as_mut().set_albums(count);
        self.as_mut().set_fresh(from as i32);
        let next = self.rev().wrapping_add(1);
        self.as_mut().set_rev(next);
        self.as_mut().set_ready(true);
    }

    /// Reflows cached albums without querying their data again.
    fn split_rows(mut self: Pin<&mut Self>) {
        let rows = self.rust().laid_out();
        // A refreshed snapshot or a width within the same column count
        // changes card bindings through rev, without tearing down the grid.
        if rows == self.rust().rows { return; }
        // SAFETY: the reset is begun and ended around the swap, as Qt asks.
        unsafe { self.as_mut().begin_reset_model() };
        self.as_mut().rust_mut().rows = rows;
        unsafe { self.as_mut().end_reset_model() };
    }
}

impl cxx_qt::Initialize for qobject::AlbumGridModel {
    fn initialize(mut self: Pin<&mut Self>) {
        self.as_mut().set_expanded(-1);
        self.as_mut().set_fresh(-1);
        self.as_mut().on_kind_changed(|m| m.schedule()).release();
        self.as_mut().on_key_changed(|m| m.schedule()).release();
        self.as_mut().on_order_changed(|m| m.schedule()).release();
        self.as_mut().on_group_changed(|m| m.schedule()).release();
        self.as_mut().on_limit_changed(|m| m.schedule()).release();
        self.as_mut().on_content_rev_changed(|m| m.schedule()).release();
        self.as_mut()
            .on_filter_changed(|m| match m.kind().to_string().as_str() {
                "artists" => m.show_artists(),
                "all" | "source" => m.schedule(),
                _ => {}
            })
            .release();
        self.as_mut().on_columns_changed(|m| m.split_rows()).release();
        self.as_mut().on_max_rows_changed(|m| m.split_rows()).release();
    }
}
