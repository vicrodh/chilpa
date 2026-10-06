//! The metadata editor as QML sees it (frontend ADR-0004).
//!
//! The form lives in the facade. Every invokable here is one call into it,
//! and `document` is what the facade says, as JSON. While a record is open
//! a thread queues a poll four times a second, which is how what the
//! worker finishes reaches the window.

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
        /// Moves with every change: `document(rev)` is read again.
        #[qproperty(i32, rev)]
        #[qproperty(bool, open)]
        /// Opening, saving, or waiting for something it asked for.
        #[qproperty(bool, busy)]
        type MetadataEditor = super::MetadataEditorRust;

        /// Zero as the track opens the record on its first.
        #[qinvokable] fn open_album(self: Pin<&mut MetadataEditor>, album_id: i64, focus_track: i64) -> bool;
        #[qinvokable] fn close(self: Pin<&mut MetadataEditor>);
        /// The whole view, as JSON.
        #[qinvokable] fn document(self: &MetadataEditor, rev: i32) -> QString;
        #[qinvokable] fn set_album(self: Pin<&mut MetadataEditor>, name: &QString, value: &QString) -> bool;
        #[qinvokable] fn set_track(self: Pin<&mut MetadataEditor>, track_id: i64, name: &QString, value: &QString) -> bool;
        #[qinvokable] fn revert(self: Pin<&mut MetadataEditor>);
        /// "sidecar" | "files"
        #[qinvokable] fn set_mode(self: Pin<&mut MetadataEditor>, mode: &QString) -> bool;
        /// "2.4" | "2.3"
        #[qinvokable] fn set_id3(self: Pin<&mut MetadataEditor>, version: &QString) -> bool;
        #[qinvokable] fn set_secondary_layers(self: Pin<&mut MetadataEditor>, on: bool);
        /// "musicbrainz" | "discogs"
        #[qinvokable] fn set_provider(self: Pin<&mut MetadataEditor>, provider: &QString) -> bool;
        #[qinvokable] fn search(self: Pin<&mut MetadataEditor>) -> bool;
        #[qinvokable] fn load(self: Pin<&mut MetadataEditor>, hit: i32) -> bool;
        /// How many tracks found their pair.
        #[qinvokable] fn apply(self: Pin<&mut MetadataEditor>) -> i32;
        #[qinvokable] fn set_artwork_provider(self: Pin<&mut MetadataEditor>, provider: &QString) -> bool;
        #[qinvokable] fn search_artwork(self: Pin<&mut MetadataEditor>) -> bool;
        #[qinvokable] fn choose_artwork(self: Pin<&mut MetadataEditor>, candidate: i32) -> bool;
        /// Opens the system's file picker and takes what is chosen.
        #[qinvokable] fn artwork_from_file(self: Pin<&mut MetadataEditor>);
        #[qinvokable] fn clear_artwork(self: Pin<&mut MetadataEditor>);
        #[qinvokable] fn save(self: Pin<&mut MetadataEditor>) -> bool;
    }

    impl cxx_qt::Threading for MetadataEditor {}
    impl cxx_qt::Initialize for MetadataEditor {}
}

use chilpa_qt_kit::app;
use cxx_qt::{CxxQtType as _, Threading as _};
use cxx_qt_lib::QString;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const POLL: std::time::Duration = std::time::Duration::from_millis(250);

#[derive(Default)]
pub struct MetadataEditorRust {
    rev: i32,
    open: bool,
    busy: bool,
    saving: bool,
    /// Cleared by the poll that sees the editor closed, which stops its thread.
    polling: Option<Arc<AtomicBool>>,
}

impl qobject::MetadataEditor {
    pub fn open_album(self: Pin<&mut Self>, album_id: i64, focus_track: i64) -> bool { self.then(|api| api.metadata_open(album_id, focus_track)) }
    pub fn close(self: Pin<&mut Self>) { self.then(|api| api.metadata_close()) }
    pub fn document(&self, _rev: i32) -> QString { QString::from(&app::with(|api| serde_json::to_string(api.metadata_view()).unwrap_or_else(|_| "{}".into()))) }
    pub fn set_album(self: Pin<&mut Self>, name: &QString, value: &QString) -> bool { let (name, value) = (name.to_string(), value.to_string()); self.then(|api| api.metadata_set_album(&name, &value)) }
    pub fn set_track(self: Pin<&mut Self>, track_id: i64, name: &QString, value: &QString) -> bool { let (name, value) = (name.to_string(), value.to_string()); self.then(|api| api.metadata_set_track(track_id, &name, &value)) }
    pub fn revert(self: Pin<&mut Self>) { self.then(|api| api.metadata_revert()) }
    pub fn set_mode(self: Pin<&mut Self>, mode: &QString) -> bool { let mode = mode.to_string(); self.then(|api| api.metadata_set_mode(&mode)) }
    pub fn set_id3(self: Pin<&mut Self>, version: &QString) -> bool { let version = version.to_string(); self.then(|api| api.metadata_set_id3(&version)) }
    pub fn set_secondary_layers(self: Pin<&mut Self>, on: bool) { self.then(|api| api.metadata_set_secondary_layers(on)) }
    pub fn set_provider(self: Pin<&mut Self>, provider: &QString) -> bool { let provider = provider.to_string(); self.then(|api| api.metadata_set_provider(&provider)) }
    pub fn search(self: Pin<&mut Self>) -> bool { self.then(|api| api.metadata_search()) }
    pub fn load(self: Pin<&mut Self>, hit: i32) -> bool { self.then(|api| api.metadata_load(hit.max(0) as usize)) }
    pub fn apply(self: Pin<&mut Self>) -> i32 { self.then(|api| api.metadata_apply() as i32) }
    pub fn set_artwork_provider(self: Pin<&mut Self>, provider: &QString) -> bool { let provider = provider.to_string(); self.then(|api| api.metadata_set_artwork_provider(&provider)) }
    pub fn search_artwork(self: Pin<&mut Self>) -> bool { self.then(|api| api.metadata_search_artwork()) }
    pub fn choose_artwork(self: Pin<&mut Self>, candidate: i32) -> bool { self.then(|api| api.metadata_choose_artwork(candidate.max(0) as usize)) }
    pub fn clear_artwork(self: Pin<&mut Self>) { self.then(|api| api.metadata_clear_artwork()) }
    pub fn save(self: Pin<&mut Self>) -> bool { self.then(|api| api.metadata_save()) }

    pub fn artwork_from_file(self: Pin<&mut Self>) {
        // The picker is the desktop's and blocks until it is answered, so it
        // runs on a thread of its own and hands the file back queued.
        let title = app::t("Choose a cover");
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let picked = pollster::block_on(rfd::AsyncFileDialog::new().set_title(&title).add_filter("JPEG, PNG", &["jpg", "jpeg", "png"]).pick_file());
            if let Some(file) = picked {
                let path = file.path().to_path_buf();
                let _ = thread.queue(move |editor| {
                    editor.then(|api| api.metadata_artwork_from_file(&path));
                });
            }
        });
    }

    /// Asks the facade for something and then shows what it says.
    fn then<R>(mut self: Pin<&mut Self>, ask: impl FnOnce(&mut chilpa_api::Chilpa) -> R) -> R {
        let answer = app::with(ask);
        self.as_mut().sync();
        answer
    }

    fn poll(self: Pin<&mut Self>) {
        self.then(|api| {
            api.metadata_poll();
        });
    }

    fn sync(mut self: Pin<&mut Self>) {
        let (rev, state, waiting) = app::with(|api| {
            let view = api.metadata_view();
            (view.rev as i32, view.state, view.lookup.searching || view.lookup.loading || view.artwork.searching || view.artwork.loading)
        });
        let (open, saving) = (state != "closed", state == "saving");
        let was_saving = std::mem::replace(&mut self.as_mut().rust_mut().saving, saving);
        self.as_mut().set_open(open);
        self.as_mut().set_busy(state == "opening" || saving || waiting);
        self.as_mut().set_rev(rev);
        if was_saving && !saving {
            // What a save left changes what every list shows: the facade
            // says so to whoever draws them.
            app::with(|api| api.mark_library_changed());
        }
        if !open {
            if let Some(active) = self.as_mut().rust_mut().polling.take() {
                active.store(false, Ordering::Relaxed);
            }
        } else if self.rust().polling.is_none() {
            let active = Arc::new(AtomicBool::new(true));
            self.as_mut().rust_mut().polling = Some(active.clone());
            let thread = self.qt_thread();
            std::thread::spawn(move || {
                while active.load(Ordering::Relaxed) {
                    std::thread::sleep(POLL);
                    if thread.queue(|editor| editor.poll()).is_err() {
                        break;
                    }
                }
            });
        }
    }
}

impl cxx_qt::Initialize for qobject::MetadataEditor {
    fn initialize(self: Pin<&mut Self>) {
        let thread = self.qt_thread();
        THREAD.with(|slot| *slot.borrow_mut() = Some(thread));
    }
}

thread_local! {
    /// The editor's thread, for whoever has to ask it something.
    static THREAD: std::cell::RefCell<Option<cxx_qt::CxxQtThread<qobject::MetadataEditor>>> = const { std::cell::RefCell::new(None) };
}

/// Queues `f` on the editor, from another object: a list of tracks opening
/// it on one of them. Dropped if the editor has not started, which only
/// happens before the window is up.
pub fn ask(f: impl FnOnce(Pin<&mut qobject::MetadataEditor>) + Send + 'static) {
    THREAD.with(|slot| {
        if let Some(thread) = slot.borrow().as_ref() {
            let _ = thread.queue(f);
        }
    });
}
