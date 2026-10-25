//! The library's folders and the scan, as Settings > Library folders shows
//! them.
//!
//! This object holds the actions and the scan's state; the list itself is a
//! `FolderListModel`, which rereads the folders kept here when `rev` moves.
//! A singleton handed whole to a view as its model fails in compiled QML:
//! cxx-qt's singletons are registered at run time, and the compiler takes
//! the bare name for a type.
//!
//! I publish scan state from the existing player tick, only by revision.
//! The facade owns automatic schedules and the single scan worker.

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
        /// The folder being scanned; empty while every folder is.
        #[qproperty(QString, scanning_path)]
        /// From 0 to 1 while a scan runs.
        #[qproperty(f64, progress)]
        /// How far the scan is, or what the last one found.
        #[qproperty(QString, status)]
        #[qproperty(QString, phase)]
        #[qproperty(bool, can_resume)]
        #[qproperty(i32, done)]
        #[qproperty(i32, total)]
        #[qproperty(i32, read_count)]
        #[qproperty(i32, same_count)]
        #[qproperty(i32, missing_count)]
        #[qproperty(i32, unreadable_count)]
        #[qproperty(bool, unreachable)]
        type LibraryFolders = super::FoldersRust;

        /// A folder that was not in the library joined it and its scan began.
        #[qsignal]
        fn added(self: Pin<&mut LibraryFolders>, path: &QString);

        /// Analyzes what `path` holds once the scan running now is done.
        #[qinvokable]
        fn analyze_after_scan(self: &LibraryFolders, path: &QString);

        /// Whether there is an analysis to offer: none runs or waits.
        #[qinvokable]
        fn offers_analysis(self: &LibraryFolders) -> bool;

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
        #[qinvokable]
        fn stop_scan(self: Pin<&mut LibraryFolders>);
        #[qinvokable]
        fn resume_scan(self: Pin<&mut LibraryFolders>);
        #[qinvokable]
        fn dismiss_status(self: Pin<&mut LibraryFolders>);
        #[qinvokable]
        fn choose_policy(self: Pin<&mut LibraryFolders>, path: &QString, policy: &QString);

        /// Stops scanning a folder. Its files are not touched, and adding it
        /// back finds its tracks with their history.
        #[qinvokable]
        fn forget(self: Pin<&mut LibraryFolders>, path: &QString);
    }

    impl cxx_qt::Threading for LibraryFolders {}
    impl cxx_qt::Initialize for LibraryFolders {}
}

use chilpa_api::paths;
use chilpa_qt_kit::app;
use chilpa_api::LibraryFolder;
use cxx_qt::{CxxQtType as _, Threading as _};
use cxx_qt_lib::QString;
use std::cell::{Cell, RefCell};
use std::pin::Pin;

thread_local! {
    static FOLDERS: RefCell<Vec<LibraryFolder>> = const { RefCell::new(Vec::new()) };
}

/// The folders as last read, for the list models.
pub fn read<R>(f: impl FnOnce(&[LibraryFolder]) -> R) -> R {
    FOLDERS.with(|l| f(&l.borrow()))
}

#[derive(Default)]
pub struct FoldersRust {
    rev: i32,
    count: i32,
    scanning: bool,
    scanning_path: QString,
    progress: f64,
    status: QString,
    phase: QString,
    can_resume: bool,
    done: i32,
    total: i32,
    read_count: i32,
    same_count: i32,
    missing_count: i32,
    unreadable_count: i32,
    unreachable: bool,
    seen_revision: u64,
}

impl qobject::LibraryFolders {
    pub fn add(mut self: Pin<&mut Self>, path: &QString) {
        let folder = paths::path_of(path.to_string().trim());
        if folder.is_empty() {
            return;
        }
        let bare = folder.trim_end_matches('/').to_owned();
        let known = read(|folders| folders.iter().any(|f| f.path.trim_end_matches('/') == bare));
        let started = self.as_mut().scan(Some(folder));
        if !known && started {
            self.as_mut().added(&QString::from(&bare));
        }
    }

    pub fn analyze_after_scan(&self, path: &QString) {
        app::with(|api| api.analyze_after_scan(&path.to_string()));
    }

    pub fn offers_analysis(&self) -> bool {
        app::with(|api| { let status = api.analysis_status(); !status.running && status.waiting.is_none() })
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
            let revision = *self.rev();
            self.as_mut().poll();
            if *self.rev() == revision { self.as_mut().refresh(); }
            app::with(|api| api.mark_library_changed());
        } else { self.as_mut().poll(); }
    }

    fn scan(mut self: Pin<&mut Self>, folder: Option<String>) -> bool {
        let started = app::with(|api| match &folder {
            Some(folder) => api.start_scan(std::path::Path::new(folder)),
            None => api.rescan(),
        });
        self.as_mut().poll();
        started
    }

    pub fn stop_scan(mut self: Pin<&mut Self>) {
        app::with(|api| api.stop_scan());
        self.as_mut().poll();
    }

    pub fn resume_scan(mut self: Pin<&mut Self>) {
        app::with(|api| api.resume_scan());
        self.as_mut().poll();
    }

    pub fn dismiss_status(mut self: Pin<&mut Self>) {
        app::with(|api| api.dismiss_scan_status());
        self.as_mut().poll();
    }

    pub fn choose_policy(mut self: Pin<&mut Self>, path: &QString, policy: &QString) {
        let changed = app::with(|api| {
            let before = api.scan_revision();
            api.set_folder_scan_policy(&path.to_string(), &policy.to_string()) && api.scan_revision() != before
        });
        let revision = *self.rev();
        self.as_mut().poll();
        if changed && *self.rev() == revision { self.as_mut().refresh(); }
    }

    fn poll(mut self: Pin<&mut Self>) {
        let state = app::with(|api| api.scan_status().clone());
        if state.revision == self.rust().seen_revision { return; }
        self.as_mut().rust_mut().seen_revision = state.revision;
        let refresh = !state.running || !*self.scanning();
        let count = |value: usize| value.min(i32::MAX as usize) as i32;
        self.as_mut().set_scanning(state.running);
        self.as_mut().set_scanning_path(QString::from(&state.root));
        self.as_mut().set_phase(QString::from(state.phase.key()));
        self.as_mut().set_can_resume(state.can_resume);
        self.as_mut().set_progress(if state.total > 0 { (state.done as f64 / state.total as f64).min(1.0) } else { 0.0 });
        self.as_mut().set_status(QString::from(&state.error));
        self.as_mut().set_done(count(state.done));
        self.as_mut().set_total(count(state.total));
        self.as_mut().set_read_count(count(state.summary.read));
        self.as_mut().set_same_count(count(state.summary.unchanged));
        self.as_mut().set_missing_count(count(state.summary.missing));
        self.as_mut().set_unreadable_count(count(state.summary.unreadable));
        self.as_mut().set_unreachable(state.summary.unreachable);
        if refresh { self.refresh(); }
    }

    fn refresh(mut self: Pin<&mut Self>) {
        let folders = app::with(|api| api.library_folders());
        if read(|current| current == folders.as_slice()) { return; }
        let n = folders.len() as i32;
        FOLDERS.with(|l| *l.borrow_mut() = folders);
        self.as_mut().set_count(n);
        let next = self.rev().wrapping_add(1);
        self.as_mut().set_rev(next);
    }
}

impl cxx_qt::Initialize for qobject::LibraryFolders {
    fn initialize(mut self: Pin<&mut Self>) {
        let thread = self.qt_thread();
        THREAD.with(|slot| *slot.borrow_mut() = Some(thread));
        self.as_mut().rust_mut().seen_revision = u64::MAX;
        self.as_mut().poll();
    }
}

thread_local! {
    /// The folders' thread, for whoever has to ask them something.
    static THREAD: RefCell<Option<cxx_qt::CxxQtThread<qobject::LibraryFolders>>> = const { RefCell::new(None) };
}

/// Queues `f` on the library's folders, from another object: the settings
/// page asking for a rescan. Dropped if they have not started, which only
/// happens before the window is up.
pub fn ask(f: impl FnOnce(Pin<&mut qobject::LibraryFolders>) + Send + 'static) {
    THREAD.with(|slot| {
        if let Some(thread) = slot.borrow().as_ref() {
            let _ = thread.queue(f);
        }
    });
}

/// I queue Qt work only for a scan revision or a changed library snapshot.
pub fn sync(library_changed: bool) {
    thread_local! { static LAST: Cell<u64> = const { Cell::new(u64::MAX) }; }
    let revision = app::with(|api| api.scan_revision());
    if LAST.with(|last| last.replace(revision) != revision) || library_changed {
        ask(move |mut folders| {
            let revision = *folders.rev();
            folders.as_mut().poll();
            if library_changed && *folders.rev() == revision { folders.refresh(); }
        });
    }
}
