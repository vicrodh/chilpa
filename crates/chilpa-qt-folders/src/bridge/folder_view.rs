//! The Folders screen's subfolders and tracks, read on the worker and tagged
//! so a late answer for a folder already left behind is dropped.

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
        #[qproperty(QString, path)]
        #[qproperty(QString, query)]
        #[qproperty(bool, content)]
        #[qproperty(i32, rev)]
        #[qproperty(i32, content_rev)]
        #[qproperty(bool, loading)]
        #[qproperty(bool, in_library)]
        #[qproperty(i32, folders)]
        #[qproperty(i32, tracks)]
        /// The folder tree as a list instead of one folder's children.
        #[qproperty(bool, tree)]
        /// The discs of a box set side by side shown as the box.
        #[qproperty(bool, grouped)]
        type FolderModel = super::FolderModelRust;
        #[qinvokable]
        fn play_all(self: Pin<&mut FolderModel>);
        /// Opens or closes a branch of the tree; with no path, closes all.
        #[qinvokable]
        fn turn(self: Pin<&mut FolderModel>, path: &QString);
        /// Opens the branches that lead to a folder, so it is in sight.
        #[qinvokable]
        fn reveal(self: Pin<&mut FolderModel>, path: &QString);
        /// Plays a folder and what is under it ("play"), or queues it
        /// "next", "later" or at the end ("queue").
        #[qinvokable]
        fn act(self: Pin<&mut FolderModel>, path: &QString, action: &QString);
    }

    unsafe extern "RustQt" {
        #[inherit]
        #[cxx_name = "beginResetModel"]
        unsafe fn begin_reset_model(self: Pin<&mut FolderModel>);
        #[inherit]
        #[cxx_name = "endResetModel"]
        unsafe fn end_reset_model(self: Pin<&mut FolderModel>);
    }

    unsafe extern "RustQt" {
        #[qinvokable]
        #[cxx_override]
        fn data(self: &FolderModel, index: &QModelIndex, role: i32) -> QVariant;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &FolderModel) -> QHash_i32_QByteArray;
        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &FolderModel, parent: &QModelIndex) -> i32;
    }

    impl cxx_qt::Threading for FolderModel {}
    impl cxx_qt::Initialize for FolderModel {}
}

use chilpa_qt_kit::{app, worker};
use chilpa_api::{FolderItem, FolderView, TrackRow};
use cxx_qt::{CxxQtType as _, Threading as _};
use cxx_qt_lib::{QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QUrl, QVariant};
use std::cell::RefCell;
use std::pin::Pin;
use std::rc::Rc;

const ROLES: &[&str] = &["name", "path", "count", "art", "folders", "album", "depth", "open", "file"];
const USER_ROLE: i32 = 256;

thread_local! {
    static TRACKS: RefCell<Rc<Vec<TrackRow>>> = RefCell::new(Rc::new(Vec::new()));
    /// The folders of the one on screen that are records, in its order.
    static ALBUMS: RefCell<Vec<i64>> = const { RefCell::new(Vec::new()) };
    /// The branches of the tree that are open, for as long as the window lasts.
    static OPEN: RefCell<std::collections::HashSet<String>> = RefCell::new(Default::default());
}

/// The tracks of the folder on screen, for the track list.
pub fn tracks() -> Rc<Vec<TrackRow>> {
    TRACKS.with(|t| t.borrow().clone())
}

/// The folders of the current one that are records.
pub fn album_ids() -> Vec<i64> {
    ALBUMS.with(|a| a.borrow().clone())
}

#[derive(Default)]
pub struct FolderModelRust {
    path: QString,
    query: QString,
    content: bool,
    rev: i32,
    content_rev: i32,
    loading: bool,
    in_library: bool,
    folders: i32,
    tracks: i32,
    tree: bool,
    grouped: bool,
    items: Vec<FolderItem>,
    generation: u64,
    pending: bool,
}

impl qobject::FolderModel {
    pub fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let Some(f) = self.rust().items.get(index.row().max(0) as usize) else { return QVariant::default() };
        match role - USER_ROLE {
            0 => QVariant::from(&QString::from(&f.name)),
            1 => QVariant::from(&QString::from(&f.path)),
            2 => QVariant::from(&(f.tracks as i32)),
            3 => {
                let url = if f.artwork_path.is_empty() {
                    QUrl::default()
                } else {
                    QUrl::from_local_file(&QString::from(&f.artwork_path))
                };
                QVariant::from(&url)
            }
            4 => QVariant::from(&(f.folders as i32)),
            5 => QVariant::from(&f.album_id),
            6 => QVariant::from(&(f.depth as i32)),
            7 => QVariant::from(&f.open),
            8 => QVariant::from(&f.file),
            _ => QVariant::default(),
        }
    }

    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        chilpa_qt_kit::format::roles(USER_ROLE, ROLES)
    }

    pub fn row_count(&self, _parent: &QModelIndex) -> i32 {
        self.rust().items.len() as i32
    }

    pub fn turn(self: Pin<&mut Self>, path: &QString) {
        let path = path.to_string();
        OPEN.with(|open| {
            let mut open = open.borrow_mut();
            if path.is_empty() {
                open.clear();
            } else if !open.remove(&path) {
                open.insert(path);
            }
        });
        self.schedule();
    }

    pub fn reveal(self: Pin<&mut Self>, path: &QString) {
        let path = path.to_string();
        let added = OPEN.with(|open| {
            let mut open = open.borrow_mut();
            std::path::Path::new(&path).ancestors().skip(1).filter(|a| a.parent().is_some()).fold(false, |any, a| open.insert(a.display().to_string()) || any)
        });
        if added {
            self.schedule();
        }
    }

    pub fn play_all(self: Pin<&mut Self>) {
        let dir = self.rust().path.clone();
        self.act(&dir, &QString::from("play"));
    }

    pub fn act(self: Pin<&mut Self>, path: &QString, action: &QString) {
        let (dir, action) = (path.to_string(), action.to_string());
        let thread = self.qt_thread();
        worker::submit(move |reader| {
            let files = reader.folder_files(&dir);
            let label = std::path::Path::new(&dir).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            // Played on the window's thread: the player belongs to it.
            let _ = thread.queue(move |_| {
                app::with(|api| match action.as_str() {
                    "next" => api.play_next_here(&files),
                    "later" => { api.play_later_here(&files, &label); }
                    "queue" => api.enqueue_here(&files),
                    _ => api.play_from(&files, 0),
                })
            });
        });
    }

    /// Defers until every QML binding for this turn has arrived.
    fn schedule(mut self: Pin<&mut Self>) {
        if self.rust().pending {
            return;
        }
        self.as_mut().rust_mut().pending = true;
        let _ = self.qt_thread().queue(|model| model.load());
    }

    fn load(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().pending = false;
        let generation = self.rust().generation + 1;
        self.as_mut().rust_mut().generation = generation;
        self.as_mut().set_loading(true);
        let dir = self.rust().path.to_string();
        let query = self.rust().query.to_string();
        let thread = self.qt_thread();
        let tree = self.rust().tree.then(|| OPEN.with(|open| open.borrow().clone()));
        let grouped = self.rust().grouped;
        worker::submit(move |reader| {
            let view = match &tree {
                Some(open) => reader.folder_tree(open, &query),
                None if query.trim().is_empty() => reader.folder_with(&dir, grouped),
                None => reader.folder_matches(&query),
            };
            let _ = thread.queue(move |model| model.arrive(generation, view));
        });
    }

    fn arrive(mut self: Pin<&mut Self>, generation: u64, view: FolderView) {
        if generation != self.rust().generation {
            return;
        }
        let (folders, tracks) = (view.folders.len() as i32, view.total as i32);
        if self.rust().content {
            TRACKS.with(|t| *t.borrow_mut() = Rc::new(view.tracks));
            ALBUMS.with(|a| *a.borrow_mut() = view.folders.iter().filter(|f| f.album_id > 0).map(|f| f.album_id).collect());
        }
        // SAFETY: the reset is begun and ended around the swap, as Qt asks.
        unsafe { self.as_mut().begin_reset_model() };
        self.as_mut().rust_mut().items = view.folders;
        unsafe { self.as_mut().end_reset_model() };
        self.as_mut().set_in_library(view.in_library);
        self.as_mut().set_folders(folders);
        self.as_mut().set_tracks(tracks);
        self.as_mut().set_loading(false);
        let next = self.rev().wrapping_add(1);
        self.as_mut().set_rev(next);
    }
}

impl cxx_qt::Initialize for qobject::FolderModel {
    fn initialize(mut self: Pin<&mut Self>) {
        self.as_mut().on_path_changed(|m| m.schedule()).release();
        self.as_mut().on_query_changed(|m| m.schedule()).release();
        self.as_mut().on_content_rev_changed(|m| m.schedule()).release();
        self.as_mut().on_tree_changed(|m| m.schedule()).release();
        self.as_mut().on_grouped_changed(|m| m.schedule()).release();
        self.schedule();
    }
}
