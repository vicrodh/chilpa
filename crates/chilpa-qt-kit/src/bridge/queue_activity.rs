//! I report background queue work without polling or serializing its rows.
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
        #[qproperty(bool, busy)]
        /// Play on a list with a place, waiting for its answer, as JSON.
        #[qproperty(QString, offer)]
        type QueueActivity = super::QueueActivityRust;
        #[qinvokable]
        fn refresh(self: Pin<&mut QueueActivity>);
        /// "continue", "restart", or anything else to play nothing.
        #[qinvokable]
        fn answer(self: Pin<&mut QueueActivity>, answer: &QString);
        #[qinvokable]
        fn collection_menu(self: &QueueActivity) -> QString;
    }
    impl cxx_qt::Threading for QueueActivity {}
    impl cxx_qt::Initialize for QueueActivity {}
}
use std::pin::Pin;
use cxx_qt::Threading as _;
static THREAD: std::sync::OnceLock<cxx_qt::CxxQtThread<qobject::QueueActivity>> = std::sync::OnceLock::new();
static BUSY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// I publish background activity from the existing transport heartbeat.
pub fn publish(busy: bool) {
    if let Some(thread) = THREAD.get() {
        if BUSY.swap(busy, std::sync::atomic::Ordering::Relaxed) != busy {
            let _ = thread.queue(|activity| activity.refresh());
        }
    }
}

fn refresh() {
    if let Some(thread) = THREAD.get() {
        let _ = thread.queue(|activity| activity.refresh());
    }
}

// I retain queue insertions across navigation, after their originating model is gone.
pub fn post(job: impl FnOnce() + Send + 'static) {
    if let Some(thread) = THREAD.get() {
        let _ = thread.queue(move |activity| { job(); activity.refresh(); });
    }
}

pub fn albums(ids: Vec<i64>, action: String, shuffled: bool, label: String) {
    let request = crate::app::with(|api| api.prepare_album_queue(ids, action, shuffled, label));
    refresh();
    if let Some(request) = request {
        crate::worker::submit(move |reader| {
            let paths = reader.album_paths(&request.ids);
            post(move || crate::app::with(|api| api.finish_album_queue(request, &paths)));
        });
    }
}

/// A confirmed bulk action works from the model's captured ids and outlives the page.
pub fn collection(ids: Vec<i64>, albums: bool, action: chilpa_api::CollectionQueueAction) {
    let ids = if albums { action.album_selection(ids) } else { ids };
    if ids.is_empty() { return; }
    let (origin, work) = crate::app::with(|api| (api.browse_origin(), api.begin_queue_work()));
    refresh();
    let pending = if albums { ids.iter().copied().filter(|id| *id < 0).collect() } else { Vec::new() };
    let resolve = move |remote| {
        crate::worker::submit(move |reader| {
            let paths = if albums { reader.collection_album_paths(&ids, remote, &action) }
                else { reader.collection_track_paths(&ids, &action) };
            post(move || {
                crate::app::with(|api| api.with_queue_origin(origin, |api| action.apply(api, &paths)));
                drop(work);
            });
        });
    };
    if albums {
        // The catalog can complete inline under the facade borrow. Submit to
        // the reader on the next GUI turn, once that borrow has been released.
        crate::app::with(|api| api.read_collection_album_paths(pending, move |remote| post(move || resolve(remote))));
    } else {
        resolve(Default::default());
    }
}

pub enum QueuePaths { Known(Vec<String>), Ids(Vec<i64>) }
pub fn paths(paths: QueuePaths, then: impl FnOnce(&mut chilpa_api::Chilpa, &[String]) + Send + 'static) {
    let (origin, work) = crate::app::with(|api| (api.browse_origin(), api.begin_queue_work()));
    refresh();
    let apply = move |paths: Vec<String>| post(move || {
        crate::app::with(|api| api.with_queue_origin(origin, |api| then(api, &paths)));
        drop(work);
    });
    match paths {
        QueuePaths::Known(paths) => apply(paths),
        QueuePaths::Ids(ids) => crate::worker::submit(move |reader| apply(reader.paths(&ids))),
    }
}

unsafe extern "C" { fn chilpa_queue_cursor(busy: bool); }
#[derive(Default)]
pub struct QueueActivityRust { busy: bool, offer: cxx_qt_lib::QString }
impl qobject::QueueActivity {
    pub fn collection_menu(&self) -> cxx_qt_lib::QString {
        cxx_qt_lib::QString::from(crate::app::with(|api| api.collection_queue_menu()).to_string())
    }

    pub fn answer(mut self: Pin<&mut Self>, answer: &cxx_qt_lib::QString) {
        crate::app::with(|api| api.answer_resume(&answer.to_string()));
        self.as_mut().refresh();
    }

    pub fn refresh(mut self: Pin<&mut Self>) {
        let offer = cxx_qt_lib::QString::from(crate::app::with(|api| api.resume_offer_json()));
        if *self.offer() != offer { self.as_mut().set_offer(offer); }
        let busy = crate::app::with(|api| api.queue_busy());
        BUSY.store(busy, std::sync::atomic::Ordering::Relaxed);
        if *self.busy() == busy { return; }
        // SAFETY: I change the application's cursor only on the GUI thread.
        unsafe { chilpa_queue_cursor(busy); }
        self.as_mut().set_busy(busy);
    }
}
impl cxx_qt::Initialize for qobject::QueueActivity {
    fn initialize(mut self: Pin<&mut Self>) { let _ = THREAD.set(self.qt_thread()); self.as_mut().refresh(); }
}
