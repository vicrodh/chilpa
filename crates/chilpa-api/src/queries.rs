//! Serial queries on a separate connection, with retryable reader startup.

use crate::{Chilpa, Reader};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Sender};

type Job = Box<dyn FnOnce(Result<&Reader, &str>) + Send>;

/// The window's query thread. Opening the reader belongs to the thread,
/// and a failed open drops only that job, never the rest of the session.
pub struct QueryWorker {
    jobs: Sender<Job>,
}

impl QueryWorker {
    fn start(path: PathBuf, catalog: std::sync::Arc<crate::source_catalog::Catalog>) -> std::io::Result<Self> {
        let (jobs, inbox) = channel::<Job>();
        std::thread::Builder::new().name("chilpa-queries".into()).spawn(move || {
            let mut reader = None;
            for job in inbox {
                if reader.is_none() {
                    match Reader::open(&path) {
                        Ok(mut opened) => {
                            opened.catalog = Some(catalog.clone());
                            reader = Some(opened);
                        }
                        Err(e) => {
                            eprintln!("chilpa-queries: {e}; will retry on the next job");
                            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| job(Err(&e))));
                            continue;
                        }
                    }
                }
                if let Some(reader) = &reader {
                    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| job(Ok(reader)))).is_err() {
                        eprintln!("chilpa-queries: a query panicked; continuing with the next job");
                    }
                }
            }
        })?;
        Ok(Self { jobs })
    }

    /// False if the thread stopped; the caller can create a new worker.
    pub fn submit(&self, job: impl FnOnce(&Reader) + Send + 'static) -> bool {
        self.submit_checked(move |result| if let Ok(reader) = result { job(reader); })
    }

    /// Completes even when opening the read-only connection fails, so a page
    /// can show a retry action instead of retaining an indefinite spinner.
    pub fn submit_checked(&self, job: impl FnOnce(Result<&Reader, &str>) + Send + 'static) -> bool {
        if let Err(error) = self.jobs.send(Box::new(job)) {
            eprintln!("chilpa-queries: query thread stopped; will restart on the next job");
            error.0(Err("query thread stopped"));
            return false;
        }
        true
    }
}

impl Chilpa {
    /// Initializes the main library before starting its read-only worker.
    /// Neither a failed initialization nor a failed spawn is cached.
    pub fn query_worker(&mut self) -> Option<QueryWorker> {
        if self.library().is_none() {
            // An upgrade under way is the startup cover's to say, not a fault.
            if !self.library_upgrading() {
                eprintln!("chilpa-queries: {}; will retry on the next job", self.now.error);
            }
            return None;
        }
        QueryWorker::start(chilpa_library::Library::default_path(), self.catalog.clone())
            .map_err(|e| eprintln!("chilpa-queries: could not start query thread: {e}; will retry on the next job")).ok()
    }
}

#[cfg(test)]
mod tests;
