//! The query worker shared by the window's models. Queries and connection
//! recovery live in the API; a model only queues its result back to Qt.

use std::cell::RefCell;

thread_local! {
    static WORKER: RefCell<Option<chilpa_api::QueryWorker>> = const { RefCell::new(None) };
    static ALBUM_WORKER: RefCell<Option<chilpa_api::QueryWorker>> = const { RefCell::new(None) };
}

/// Queues a query, retrying failed startup on the next submission.
pub fn submit(job: impl FnOnce(&chilpa_api::Reader) + Send + 'static) {
    submit_to(&WORKER, job);
}

/// A page receives a terminal result when initialization or reader startup fails.
pub fn submit_checked(job: impl FnOnce(Result<&chilpa_api::Reader, &str>) + Send + 'static) {
    WORKER.with(|slot| {
        let mut worker = slot.borrow_mut();
        if worker.is_none() { *worker = crate::app::with(|api| api.query_worker()); }
        if let Some(running) = worker.as_ref() {
            if !running.submit_checked(job) { *worker = None; }
        } else { job(Err("could not initialize the library reader")); }
    });
}

/// Delivers either a read value or a reader-startup error to a live Qt owner.
/// The owner checks its generation before accepting either terminal result.
pub fn query<T: cxx_qt::Threading + 'static, R: Send + 'static>(
    thread: cxx_qt::CxxQtThread<T>,
    read: impl FnOnce(&chilpa_api::Reader) -> R + Send + 'static,
    arrived: impl FnOnce(std::pin::Pin<&mut T>, R) + Send + 'static,
    failed: impl FnOnce(std::pin::Pin<&mut T>, String) + Send + 'static,
) {
    submit_checked(move |reader| {
        let result = reader.map(read).map_err(str::to_owned);
        let _ = thread.queue(move |model| match result {
            Ok(value) => arrived(model, value),
            Err(error) => failed(model, error),
        });
    });
}

/// Album navigation has its own reader, so an old search cannot delay it.
pub fn submit_album(job: impl FnOnce(&chilpa_api::Reader) + Send + 'static) {
    let queued = std::time::Instant::now();
    let timed = std::env::var_os("CHILPA_ALBUM_TIMINGS").is_some();
    submit_to(&ALBUM_WORKER, move |reader| {
        let started = std::time::Instant::now();
        job(reader);
        if timed { eprintln!("album-worker wait={}ms read={}ms", started.duration_since(queued).as_millis(), started.elapsed().as_millis()); }
    });
}

fn submit_to(slot: &'static std::thread::LocalKey<RefCell<Option<chilpa_api::QueryWorker>>>, job: impl FnOnce(&chilpa_api::Reader) + Send + 'static) {
    slot.with(|slot| {
        let mut worker = slot.borrow_mut();
        if worker.is_none() { *worker = crate::app::with(|api| api.query_worker()); }
        if let Some(running) = worker.as_ref() {
            if !running.submit(job) { *worker = None; }
        }
    });
}
