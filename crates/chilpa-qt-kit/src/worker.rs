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
