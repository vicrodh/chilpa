//! The thread for what does not fit in a frame.
//!
//! Sorting fifty thousand tracks or counting every genre takes around a
//! hundred milliseconds; on the window's thread that is six dropped frames
//! each time a filter changes. Those queries run here instead, on one
//! thread with its own read-only connection, one job at a time. A job
//! computes and then queues its result back onto the window's thread
//! through the QObject that asked, so nothing here touches Qt.

use std::sync::mpsc::{channel, Sender};
use std::sync::OnceLock;

type Job = Box<dyn FnOnce(&chilpa_api::Reader) + Send>;

static JOBS: OnceLock<Option<Sender<Job>>> = OnceLock::new();

/// Runs `job` on the worker. It is dropped when the library cannot be
/// opened; the window's own connection has already said why.
pub fn submit(job: impl FnOnce(&chilpa_api::Reader) + Send + 'static) {
    let jobs = JOBS.get_or_init(|| {
        let reader = crate::app::with(|api| api.reader())?;
        let (tx, rx) = channel::<Job>();
        std::thread::Builder::new()
            .name("chilpa-queries".into())
            .spawn(move || {
                for job in rx {
                    job(&reader);
                }
            })
            .ok()?;
        Some(tx)
    });
    if let Some(tx) = jobs {
        let _ = tx.send(Box::new(job));
    }
}
