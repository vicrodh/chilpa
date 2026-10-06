//! The query worker shared by the window's models. Queries and connection
//! recovery live in the API; a model only queues its result back to Qt.

use std::cell::RefCell;

thread_local! {
    static WORKER: RefCell<Option<chilpa_api::QueryWorker>> = const { RefCell::new(None) };
}

/// Queues a query, retrying failed startup on the next submission.
pub fn submit(job: impl FnOnce(&chilpa_api::Reader) + Send + 'static) {
    WORKER.with(|slot| {
        let mut worker = slot.borrow_mut();
        if worker.is_none() { *worker = crate::app::with(|api| api.query_worker()); }
        if let Some(running) = worker.as_ref() {
            if !running.submit(job) { *worker = None; }
        }
    });
}
