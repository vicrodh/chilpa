//! The library's folders and the scan, as Settings > Library folders shows
//! them.
//!
//! This object holds the actions and the scan's state; the list itself is a
//! `FolderListModel`, which rereads the folders kept here when `rev` moves.
//! A singleton handed whole to a view as its model fails in compiled QML:
//! cxx-qt's singletons are registered at run time, and the compiler takes
//! the bare name for a type.
//!
//! Adding a folder scans it in the background. While a scan runs, a thread
//! queues a poll four times a second; the page shows how far it is, and
//! when it ends the folders are read again and the Library is told its
//! content changed, which reloads every screen that shows it.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        /// Bumped when the folders were read again.
        #[qproperty(i32, rev)]
        #[qproperty(i32, count)]
        #[qproperty(bool, scanning)]
        /// From 0 to 1 while a scan runs.
        #[qproperty(f64, progress)]
        /// How far the scan is, or what the last one found.
        #[qproperty(QString, status)]
        type LibraryFolders = super::FoldersRust;

        /// Adds a folder, by path or by a dialog's file URL, and scans it.
        #[qinvokable]
        fn add(self: Pin<&mut LibraryFolders>, path: &QString);

        /// Opens the system's folder picker and adds what is chosen.
        #[qinvokable]
        fn browse(self: Pin<&mut LibraryFolders>);

        /// Scans one folder again.
        #[qinvokable]
        fn rescan(self: Pin<&mut LibraryFolders>, path: &QString);

        /// Scans every folder again.
        #[qinvokable]
        fn rescan_all(self: Pin<&mut LibraryFolders>);

        /// Stops scanning a folder. Its files are not touched, and adding it
        /// back finds its tracks with their history.
        #[qinvokable]
        fn forget(self: Pin<&mut LibraryFolders>, path: &QString);
    }

    impl cxx_qt::Threading for LibraryFolders {}
    impl cxx_qt::Initialize for LibraryFolders {}
}

use crate::{app, paths, registry};
use chilpa_api::LibraryFolder;
use cxx_qt::{CxxQtType as _, Threading as _};
use cxx_qt_lib::QString;
use std::cell::RefCell;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

thread_local! {
    static FOLDERS: RefCell<Vec<LibraryFolder>> = const { RefCell::new(Vec::new()) };
}

/// The folders as last read, for the list models.
pub fn read<R>(f: impl FnOnce(&[LibraryFolder]) -> R) -> R {
    FOLDERS.with(|l| f(&l.borrow()))
}
const POLL: std::time::Duration = std::time::Duration::from_millis(250);

#[derive(Default)]
pub struct FoldersRust {
    rev: i32,
    count: i32,
    scanning: bool,
    progress: f64,
    status: QString,
    /// Cleared by the poll that sees the scan end, which stops its thread.
    polling: Option<Arc<AtomicBool>>,
}

impl qobject::LibraryFolders {
    pub fn add(self: Pin<&mut Self>, path: &QString) {
        let folder = paths::path_of(path.to_string().trim());
        if folder.is_empty() {
            return;
        }
        self.scan(Some(folder));
    }

    pub fn browse(self: Pin<&mut Self>) {
        // The picker is the desktop's, and it blocks until it is answered, so
        // it runs on a thread of its own and hands the folder back queued.
        let title = app::t("Add a folder to the library");
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let picked = pollster::block_on(rfd::AsyncFileDialog::new().set_title(&title).pick_folder());
            if let Some(folder) = picked {
                let path = folder.path().to_string_lossy().into_owned();
                let _ = thread.queue(move |folders| folders.add(&QString::from(&path)));
            }
        });
    }

    pub fn rescan(self: Pin<&mut Self>, path: &QString) {
        self.scan(Some(path.to_string()));
    }

    pub fn rescan_all(self: Pin<&mut Self>) {
        self.scan(None);
    }

    pub fn forget(mut self: Pin<&mut Self>, path: &QString) {
        let path = path.to_string();
        if app::with(|api| api.forget_folder(&path)) {
            self.as_mut().refresh();
            registry::library(|library| library.content_changed());
        }
    }

    /// Starts a scan of one folder, or of all of them, and polls it.
    fn scan(mut self: Pin<&mut Self>, folder: Option<String>) {
        let started = app::with(|api| {
            match &folder {
                Some(f) => api.start_scan(std::path::Path::new(f)),
                None => api.rescan(),
            }
            api.scan_status().running
        });
        if !started {
            let why = app::with(|api| api.now_playing().error.clone());
            self.as_mut().set_status(QString::from(&why));
            return;
        }
        self.as_mut().set_scanning(true);
        self.as_mut().set_progress(0.0);
        let active = Arc::new(AtomicBool::new(true));
        self.as_mut().rust_mut().polling = Some(active.clone());
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            while active.load(Ordering::Relaxed) {
                std::thread::sleep(POLL);
                if thread.queue(|folders| folders.poll()).is_err() {
                    break;
                }
            }
        });
    }

    fn poll(mut self: Pin<&mut Self>) {
        let state = app::with(|api| api.scan_status().clone());
        if state.running {
            let progress = if state.total > 0 { state.done as f64 / state.total as f64 } else { 0.0 };
            let (done, total) = (state.done.to_string(), state.total.to_string());
            let line = app::with(|api| api.t_args("Reading {} of {} files", &[&done, &total]));
            self.as_mut().set_progress(progress);
            self.as_mut().set_status(QString::from(&line));
            return;
        }
        if let Some(active) = self.as_mut().rust_mut().polling.take() {
            active.store(false, Ordering::Relaxed);
        }
        let s = &state.summary;
        let (read, same, missing) = (s.read.to_string(), s.unchanged.to_string(), s.missing.to_string());
        let line = app::with(|api| api.t_args("{} read, {} unchanged, {} missing", &[&read, &same, &missing]));
        self.as_mut().set_status(QString::from(&line));
        self.as_mut().set_progress(1.0);
        self.as_mut().set_scanning(false);
        self.as_mut().refresh();
        registry::library(|library| library.content_changed());
    }

    fn refresh(mut self: Pin<&mut Self>) {
        let folders = app::with(|api| api.library_folders());
        let n = folders.len() as i32;
        FOLDERS.with(|l| *l.borrow_mut() = folders);
        self.as_mut().set_count(n);
        let next = self.rev().wrapping_add(1);
        self.as_mut().set_rev(next);
    }
}

impl cxx_qt::Initialize for qobject::LibraryFolders {
    fn initialize(mut self: Pin<&mut Self>) {
        registry::register_folders(self.qt_thread());
        self.as_mut().refresh();
    }
}
