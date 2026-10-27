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
        /// Home shelves, "all", filtered "library", "artist"/"appearsOn" by
        /// name, "sameArtist"/"related" by album id, or "folder". "artists"
        /// draws round cards; key "all" includes contributing performers.
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
        fn explicit_content(self: &AlbumGridModel, index: i32, rev: i32) -> bool;
        #[qinvokable]
        fn subtitle(self: &AlbumGridModel, index: i32, rev: i32) -> QString;
        #[qinvokable]
        fn year(self: &AlbumGridModel, index: i32, rev: i32) -> QString;
        #[qinvokable]
        fn art(self: &AlbumGridModel, index: i32, rev: i32) -> QUrl;
        #[qinvokable]
        fn album_id(self: &AlbumGridModel, index: i32, rev: i32) -> i64;
        /// An artist's card of "artists": its name and the sources that hold its records.
        #[qinvokable]
        fn artist(self: &AlbumGridModel, index: i32, rev: i32) -> QString;
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
        #[cxx_name = "beginRemoveRows"]
        unsafe fn begin_remove_rows(self: Pin<&mut AlbumGridModel>, parent: &QModelIndex, first: i32, last: i32);
        #[inherit]
        #[cxx_name = "endRemoveRows"]
        unsafe fn end_remove_rows(self: Pin<&mut AlbumGridModel>);
        #[inherit]
        #[cxx_name = "beginMoveRows"]
        unsafe fn begin_move_rows(self: Pin<&mut AlbumGridModel>, parent: &QModelIndex, first: i32, last: i32, destination: &QModelIndex, child: i32) -> bool;
        #[inherit]
        #[cxx_name = "endMoveRows"]
        unsafe fn end_move_rows(self: Pin<&mut AlbumGridModel>);
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
use std::rc::Rc;
use cxx_qt::{CxxQtType as _, Threading as _};
use cxx_qt_lib::{QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QUrl, QVariant, QVector};
use std::pin::Pin;
use chilpa_api::list_changes::{self, Step};

const USER_ROLE: i32 = 256;
use chilpa_qt_kit::album_rows::Albums;
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
    source_ready: bool,
    rev: i32,
    content_rev: i32,
    filter: QString,
    expanded: i32,
    fresh: i32,
    mixed: bool,
    anchor: Option<i32>,
    /// The artists of "artists", kept so the filter needs no query.
    artists: Vec<chilpa_api::ListedArtist>,
    /// The card of each artist the filter leaves, in the order of `list`.
    artist_cards: Vec<String>,
    list: Albums,
    /// Where each group starts, and the rows that leaves at this width.
    groups: Vec<(usize, String)>,
    rows: Vec<chilpa_api::GridRow>,
    pending: bool,
    generation: u64,
    loaded: Option<(String, String, String, String, String)>,
}

impl AlbumGridRust {
    fn get<R>(&self, index: i32, f: impl FnOnce(&Album) -> R) -> Option<R> {
        self.list.with(usize::try_from(index).ok()?, f)
    }

    fn laid_out(&self) -> Vec<chilpa_api::GridRow> {
        list_changes::album_rows(self.list.len(), self.columns.max(0) as usize, self.max_rows.max(0) as usize, &self.groups)
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
    pub fn explicit_content(&self, index: i32, _rev: i32) -> bool {
        self.rust().get(index, |album| album.explicit == Some(true)).unwrap_or(false)
    }
    pub fn subtitle(&self, index: i32, _rev: i32) -> QString {
        self.rust().get(index, |a| QString::from(&a.artist)).unwrap_or_default()
    }
    pub fn year(&self, index: i32, _rev: i32) -> QString {
        self.rust().get(index, |a| a.year.map(|year| QString::from(&year.to_string())).unwrap_or_default()).unwrap_or_default()
    }
    pub fn art(&self, index: i32, _rev: i32) -> QUrl {
        self.rust().get(index, |album| crate::format::art_url(&album.artwork_path)).unwrap_or_default()
    }
    pub fn artist(&self, index: i32, _rev: i32) -> QString {
        self.rust().artist_cards.get(index.max(0) as usize).map(QString::from).unwrap_or_default()
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
            3 => QVariant::from(&self.rust().rev),
            _ => QVariant::default(),
        }
    }

    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        crate::format::roles(USER_ROLE, &["first", "span", "group", "revision"])
    }

    pub fn row_count(&self, _parent: &QModelIndex) -> i32 {
        self.rust().rows.len() as i32
    }

    fn schedule(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().generation = self.rust().generation.wrapping_add(1);
        if self.rust().pending {
            return;
        }
        self.as_mut().rust_mut().pending = true;
        let _ = self.qt_thread().queue(|model| model.reload());
    }

    fn reload(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().pending = false;
        let limit = self.rust().limit.max(1) as usize;
        let kind = self.rust().kind.to_string();
        if matches!(kind.as_str(), "sameArtist" | "related") {
            let generation = self.rust().generation.wrapping_add(1);
            self.as_mut().rust_mut().generation = generation;
            let (thread, id) = (self.qt_thread(), self.rust().key.to_string().parse().unwrap_or_default());
            worker::submit(move |reader| {
                let albums = reader.album_shelf(&kind, id, limit);
                let _ = thread.queue(move |model| {
                    if model.rust().generation == generation { model.show(Albums::Loaded(albums)); }
                });
            });
            return;
        }
        if matches!(kind.as_str(), "recentlyPlayed" | "recentlyPlayedEverywhere" | "recentlyAdded") {
            let generation = self.rust().generation.wrapping_add(1);
            self.as_mut().rust_mut().generation = generation;
            let (thread, revision) = (self.qt_thread(), self.rust().content_rev);
            let cached = app::with(|api| api.home_section(&kind, limit, revision, move |rows| {
                let _ = thread.queue(move |model| model.home_arrived(generation, rows));
            }));
            if let Some(rows) = cached { self.home_arrived(generation, rows); }
            return;
        }
        let list = match self.rust().kind.to_string().as_str() {
            "all" => {
                // Sorting the whole library does not fit in a frame: the
                // worker takes the snapshot and the rows follow.
                let generation = self.rust().generation + 1;
                self.as_mut().rust_mut().generation = generation;
                let (order, group, text) = (self.rust().order.to_string(), self.rust().group.to_string(), self.rust().filter.to_string());
                let extras = crate::browse::read(|s| s.filter.clone());
                let thread = self.qt_thread();
                worker::submit(move |reader| {
                    let found = reader.albums_view(&order, &group, &text, &extras);
                    let _ = thread.queue(move |model| model.arrive(generation, found));
                });
                return;
            }
            "artist" | "appearsOn" | "artistLatest" => {
                let name = self.rust().key.to_string();
                let mut records = app::with(|api| api.cached_artist_releases(&name, kind == "appearsOn"));
                if kind == "artistLatest" { records.truncate(1); }
                records
            }
            "source" => {
                let snapshot = app::with(|api| api.source_shelf_snapshot(&self.rust().key.to_string(), &self.rust().order.to_string(), &self.rust().group.to_string(), &self.rust().filter.to_string()));
                let ready = snapshot.ready();
                self.as_mut().rust_mut().source_ready = ready;
                if !ready { self.as_mut().set_ready(false); }
                let (mut cards, mut groups) = (snapshot.albums, snapshot.groups);
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
                let extras = crate::browse::read(|s| s.filter.clone());
                let thread = self.qt_thread();
                worker::submit(move |reader| {
                    let artists = reader.listed_artists(&role, &extras);
                    let _ = thread.queue(move |model| model.arrive_artists(generation, artists));
                });
                return;
            }
            "library" => {
                // Unlike the facet, this includes the album choice itself
                // and follows the result's requested order.
                let (ids, pages, order) = crate::browse::read(|s| (s.snapshot.album_ids.clone(), s.album_pages.clone(), s.order));
                let group = self.rust().group.to_string();
                if group.is_empty() { self.as_mut().rust_mut().groups.clear(); return self.show(Albums::Snapshot { ids, pages }); }
                let (generation, thread) = (self.rust().generation, self.qt_thread());
                worker::submit(move |reader| {
                    let found = reader.library_album_groups(ids, &group, order);
                    let _ = thread.queue(move |model| model.arrive(generation, found));
                });
                return;
            }
            "folder" => {
                let ids = chilpa_qt_folders::bridge::folder_view::album_ids();
                return self.show(Albums::Snapshot { ids, pages: Rc::default() });
            }
            _ => Vec::new(),
        };
        self.show(Albums::Loaded(list));
    }

    fn home_arrived(self: Pin<&mut Self>, generation: u64, rows: chilpa_api::HomeRows) {
        if generation == self.rust().generation {
            if let chilpa_api::HomeRows::Albums(rows) = rows { self.show(Albums::Loaded(rows)); }
        }
    }

    fn arrive_artists(mut self: Pin<&mut Self>, generation: u64, artists: Vec<chilpa_api::ListedArtist>) {
        if generation != self.rust().generation {
            return;
        }
        self.as_mut().rust_mut().artists = artists;
        self.show_artists();
    }

    /// The artists the filter leaves, as cards.
    fn show_artists(mut self: Pin<&mut Self>) {
        let (cards, shown) = chilpa_api::artist_cards(&self.rust().artists, &self.rust().filter.to_string());
        self.as_mut().rust_mut().artist_cards = shown;
        self.show(Albums::Loaded(cards));
    }

    fn arrive(mut self: Pin<&mut Self>, generation: u64, found: chilpa_api::AlbumsFound) {
        if generation == self.rust().generation {
            self.as_mut().rust_mut().groups = found.groups;
            self.show(Albums::Snapshot { ids: found.ids, pages: Rc::default() });
        }
    }

    fn show(mut self: Pin<&mut Self>, mut list: Albums) {
        let identity = (self.kind().to_string(), self.key().to_string(), self.order().to_string(), self.group().to_string(), self.filter().to_string());
        let same = self.rust().loaded.as_ref() == Some(&identity);
        let rows = list_changes::album_rows(list.len(), self.rust().columns.max(0) as usize, self.rust().max_rows.max(0) as usize, &self.rust().groups);
        let next = self.rev().wrapping_add(1);
        self.as_mut().set_rev(next);
        if same && rows == self.rust().rows && matches!((&self.rust().list, &list), (Albums::Loaded(a), Albums::Loaded(b)) if a == b) { let ready = self.rust().kind.to_string() != "source" || self.rust().source_ready; self.as_mut().set_ready(ready); return; }
        let plan = if same && !self.rust().rows.is_empty() { match (&self.rust().list, &list) {
            (Albums::Loaded(old), Albums::Loaded(new)) => list_changes::albums(old, new, &self.rust().rows, &rows),
            _ => None,
        } } else { None };
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
        let anchor = self.rust().anchor.and_then(|at| self.rust().get(at, |a| a.id));
        self.as_mut().rust_mut().anchor = anchor.and_then(|id| list.position(id)).map(|at| at as i32);
        let offset = list.stage(&mut self.as_mut().rust_mut().list, plan.as_ref());
        self.as_mut().rust_mut().list = list;
        self.as_mut().set_expanded(expanded);
        if let Some(plan) = plan {
            let parent = QModelIndex::default();
            let staged: Vec<_> = rows.iter().cloned().map(|mut row| { row.first += offset; row }).collect();
            for &step in &plan.steps {
                // SAFETY: each planned edit brackets its storage mutation.
                unsafe { match step {
                    Step::Remove(at) => self.as_mut().begin_remove_rows(&parent, at as i32, at as i32),
                    Step::Insert(at) => self.as_mut().begin_insert_rows(&parent, at as i32, at as i32),
                    Step::Move { from, .. } => { self.as_mut().begin_move_rows(&parent, from as i32, from as i32, &parent, step.destination() as i32); }
                } }
                step.apply(&mut self.as_mut().rust_mut().rows, &staged);
                unsafe { match step {
                    Step::Remove(_) => self.as_mut().end_remove_rows(), Step::Insert(_) => self.as_mut().end_insert_rows(), Step::Move { .. } => self.as_mut().end_move_rows(),
                } }
            }
            self.as_mut().rust_mut().list.finish(offset);
            self.as_mut().rust_mut().rows = rows;
            for row in plan.published() { let at = self.index(row as i32, 0, &parent); self.as_mut().data_changed(&at, &at, &QVector::<i32>::default()); }
        } else if same && rows == self.rust().rows && matches!(self.rust().list, Albums::Snapshot { .. }) {
            if !rows.is_empty() {
                let (first, last) = (self.index(0, 0, &QModelIndex::default()), self.index(rows.len() as i32 - 1, 0, &QModelIndex::default()));
                self.as_mut().data_changed(&first, &last, &QVector::<i32>::default());
            }
        } else {
            // SAFETY: a new context or a large edit replaces all rows at once.
            unsafe { self.as_mut().begin_reset_model() };
            self.as_mut().rust_mut().rows = rows;
            unsafe { self.as_mut().end_reset_model() };
        }
        self.as_mut().rust_mut().loaded = Some(identity);
        self.as_mut().set_albums(count);
        let ready = self.rust().kind.to_string() != "source" || self.rust().source_ready;
        self.as_mut().set_ready(ready);
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
        self.as_mut().on_kind_changed(|mut m| { m.as_mut().set_ready(false); m.schedule(); }).release();
        self.as_mut().on_key_changed(|mut m| { m.as_mut().set_ready(false); m.schedule(); }).release();
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
