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
        #[qproperty(bool, loading)]
        #[qproperty(bool, in_library)]
        #[qproperty(i32, folders)]
        #[qproperty(i32, tracks)]
        #[qproperty(bool, has_cd)]
        type FolderModel = super::FolderModelRust;
        #[qinvokable]
        fn play_all(self: Pin<&mut FolderModel>);
        #[qinvokable]
        fn pick(self: Pin<&mut FolderModel>);
        #[qinvokable]
        fn play_cd(self: Pin<&mut FolderModel>);
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

use crate::{app, registry, worker};
use chilpa_api::{FolderItem, FolderView, TrackRow};
use cxx_qt::{CxxQtType as _, Threading as _};
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QUrl, QVariant};
use std::cell::RefCell;
use std::pin::Pin;
use std::rc::Rc;

const ROLES: &[&str] = &["name", "path", "count", "art"];
const USER_ROLE: i32 = 256;

thread_local! {
    static TRACKS: RefCell<Rc<Vec<TrackRow>>> = RefCell::new(Rc::new(Vec::new()));
}

/// The tracks of the folder on screen, for the track list.
pub fn tracks() -> Rc<Vec<TrackRow>> {
    TRACKS.with(|t| t.borrow().clone())
}

/// The records in the current folder, once and in first-track order.
pub fn album_ids() -> Vec<i64> {
    let mut seen = std::collections::HashSet::new();
    tracks().iter().filter_map(|track| (track.album_id > 0 && seen.insert(track.album_id)).then_some(track.album_id)).collect()
}

#[derive(Default)]
pub struct FolderModelRust {
    path: QString,
    query: QString,
    content: bool,
    rev: i32,
    loading: bool,
    in_library: bool,
    folders: i32,
    tracks: i32,
    has_cd: bool,
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
            _ => QVariant::default(),
        }
    }

    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut roles = QHash::<QHashPair_i32_QByteArray>::default();
        for (i, name) in ROLES.iter().enumerate() {
            roles.insert(USER_ROLE + i as i32, QByteArray::from(*name));
        }
        roles
    }

    pub fn row_count(&self, _parent: &QModelIndex) -> i32 {
        self.rust().items.len() as i32
    }

    pub fn play_all(self: Pin<&mut Self>) {
        let dir = self.rust().path.to_string();
        let thread = self.qt_thread();
        worker::submit(move |reader| {
            let files = reader.folder_files(&dir);
            // Played on the window's thread: the player belongs to it.
            let _ = thread.queue(move |_| app::with(|api| api.play_from(&files, 0)));
        });
    }

    pub fn pick(self: Pin<&mut Self>) {
        let title = app::t("Open a folder");
        std::thread::spawn(move || {
            let picked = pollster::block_on(rfd::AsyncFileDialog::new().set_title(&title).pick_folder());
            if let Some(folder) = picked {
                let path = folder.path().to_string_lossy().into_owned();
                registry::navigation(move |nav| nav.open_folder(&QString::from(&path)));
            }
        });
    }

    pub fn play_cd(self: Pin<&mut Self>) {
        app::with(|api| {
            let (device, bit_perfect) = api.chosen_output();
            api.play_cd("", &device, bit_perfect);
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
        worker::submit(move |reader| {
            let view = if query.trim().is_empty() { reader.folder(&dir) } else { reader.folder_matches(&query) };
            let _ = thread.queue(move |model| model.arrive(generation, view));
        });
    }

    fn arrive(mut self: Pin<&mut Self>, generation: u64, view: FolderView) {
        if generation != self.rust().generation {
            return;
        }
        let (folders, tracks) = (view.folders.len() as i32, view.tracks.len() as i32);
        if self.rust().content {
            TRACKS.with(|t| *t.borrow_mut() = Rc::new(view.tracks));
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
        let has_cd = app::with(|api| !api.cd_drives().is_empty());
        self.as_mut().set_has_cd(has_cd);
        self.as_mut().on_path_changed(|m| m.schedule()).release();
        self.as_mut().on_query_changed(|m| m.schedule()).release();
        self.schedule();
    }
}
