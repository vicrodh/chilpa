//! A bounded asynchronous checklist owned by the Tracks sheet popup.
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
        #[qproperty(QString, key)]
        #[qproperty(i32, filter_rev)]
        #[qproperty(QString, column_result)]
        #[qproperty(bool, column_busy)]
        type TrackColumnChoices = super::TrackColumnChoicesRust;
        #[qinvokable]
        fn request_column(self: Pin<&mut TrackColumnChoices>, field: &QString, search: &QString, cursor: &QString);
        #[qinvokable]
        fn close_column(self: Pin<&mut TrackColumnChoices>);
    }
    impl cxx_qt::Threading for TrackColumnChoices {}
    impl cxx_qt::Initialize for TrackColumnChoices {}
}

use crate::{app, worker};
use cxx_qt::{CxxQtType as _, Threading as _};
use cxx_qt_lib::QString;
use std::pin::Pin;
use std::sync::{Arc, atomic::{AtomicU64, Ordering}};

/// A dropped queue job completes too, including reader startup failure.
struct Completion(Option<Box<dyn FnOnce() + Send>>);
impl Completion { fn disarm(&mut self) { self.0 = None; } }
impl Drop for Completion {
    fn drop(&mut self) { if let Some(failed) = self.0.take() { failed(); } }
}

#[derive(Default)]
pub struct TrackColumnChoicesRust {
    key: QString,
    filter_rev: i32,
    column_result: QString,
    column_busy: bool,
    column_running: bool,
    column_generation: Arc<AtomicU64>,
    /// At most one pending request replaces typing that arrived during a query.
    column_pending: Option<(String, String, String)>,
}

impl qobject::TrackColumnChoices {
    pub fn request_column(mut self: Pin<&mut Self>, field: &QString, search: &QString, cursor: &QString) {
        self.rust().column_generation.fetch_add(1, Ordering::Relaxed);
        self.as_mut().rust_mut().column_pending = Some((field.to_string(), search.to_string(), cursor.to_string()));
        self.as_mut().set_column_result(QString::default());
        self.as_mut().set_column_busy(true);
        self.column_start();
    }

    pub fn close_column(mut self: Pin<&mut Self>) {
        self.rust().column_generation.fetch_add(1, Ordering::Relaxed);
        self.as_mut().rust_mut().column_pending = None;
        self.as_mut().set_column_busy(false);
        self.as_mut().set_column_result(QString::default());
    }

    fn column_start(mut self: Pin<&mut Self>) {
        if self.rust().column_running { return; }
        let Some((field, search, cursor)) = self.as_mut().rust_mut().column_pending.take() else { return; };
        self.as_mut().rust_mut().column_running = true;
        let query = self.key().to_string();
        let extras = app::with(|api| api.track_sheet_extras(&query, &field, chilpa_api::Filter::default()));
        let ticket = self.rust().column_generation.clone();
        let generation = ticket.load(Ordering::Relaxed);
        let revision = *self.filter_rev();
        let thread = self.qt_thread();
        let failed_thread = self.qt_thread();
        let failure = chilpa_api::Reader::track_column_failure(&field, &search, &cursor).to_string();
        let mut completion = Completion(Some(Box::new(move || {
            let _ = failed_thread.queue(move |mut model| {
                model.as_mut().rust_mut().column_running = false;
                if model.rust().column_generation.load(Ordering::Relaxed) == generation {
                    model.as_mut().set_column_result(QString::from(&failure));
                    model.as_mut().set_column_busy(false);
                }
                model.column_start();
            });
        })));
        worker::submit(move |reader| {
            if ticket.load(Ordering::Relaxed) != generation { return; }
            let result = reader.track_column(&query, &extras, &field, &search, &cursor).to_string();
            completion.disarm();
            let _ = thread.queue(move |mut model| {
                model.as_mut().rust_mut().column_running = false;
                if model.rust().column_generation.load(Ordering::Relaxed) == generation && model.key().to_string() == query && *model.filter_rev() == revision {
                    model.as_mut().set_column_result(QString::from(&result));
                    model.as_mut().set_column_busy(false);
                }
                model.column_start();
            });
        });
    }

}

impl cxx_qt::Initialize for qobject::TrackColumnChoices {
    fn initialize(mut self: Pin<&mut Self>) {
        self.as_mut().on_key_changed(|model| model.close_column()).release();
        self.as_mut().on_filter_rev_changed(|model| model.close_column()).release();
    }
}

impl Drop for TrackColumnChoicesRust {
    fn drop(&mut self) { self.column_generation.fetch_add(1, Ordering::Relaxed); }
}
