//! Default genre images, fetched from the website when a card or a header is
//! shown and kept in the remote artwork cache, whose size limit and eviction
//! apply to them. The bundled index gives each file's digest: bytes that do not
//! match are not kept. Two workers start on the first request and wait on a
//! queue of at most 32 jobs; each shown image owns a cancellation handle and
//! nothing polls. A failed file is not asked again for ten minutes (at most
//! one entry per published file).
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Condvar, Mutex, OnceLock, atomic::{AtomicBool, Ordering}};
use std::time::{Duration, Instant};

/// Files under it never change; a new set goes to a new version folder.
pub(crate) const BASE: &str = "https://chilpa.app/genre-art/v1/";
const RETRY_AFTER: Duration = Duration::from_secs(10 * 60);
const QUEUE: usize = 32;

pub struct GenreArtRequest(Arc<AtomicBool>);
impl Drop for GenreArtRequest { fn drop(&mut self) { self.0.store(false, Ordering::Release); } }

struct Job { address: String, digest: &'static str, alive: Arc<AtomicBool>, done: Box<dyn FnOnce(String) + Send> }

fn failures() -> &'static Mutex<HashMap<String, Instant>> {
    static FAILED: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
    FAILED.get_or_init(Default::default)
}

fn resolve(job: &Job) -> String {
    if let Some(path) = crate::pictures::kept(&job.address) { return path; }
    if chilpa_core::network::current().offline { return String::new(); }
    let mut failed = failures().lock().unwrap_or_else(|error| error.into_inner());
    if failed.get(&job.address).is_some_and(|at| at.elapsed() < RETRY_AFTER) { return String::new(); }
    drop(failed);
    match crate::pictures::keep_digest(&job.address, job.digest, Duration::from_secs(30)) {
        Some(path) => path,
        None => {
            failed = failures().lock().unwrap_or_else(|error| error.into_inner());
            if !chilpa_core::network::current().offline { failed.insert(job.address.clone(), Instant::now()); }
            String::new()
        }
    }
}

struct Queue { jobs: Mutex<VecDeque<Job>>, ready: Condvar }
fn queue() -> &'static Arc<Queue> {
    static JOBS: OnceLock<Arc<Queue>> = OnceLock::new();
    JOBS.get_or_init(|| {
        let queue = Arc::new(Queue { jobs:Mutex::new(VecDeque::new()), ready:Condvar::new() });
        for index in 0..2 {
            let queue = queue.clone();
            let _ = std::thread::Builder::new().name(format!("chilpa-genre-art-{index}")).spawn(move || loop {
                let job = {
                    let mut jobs = queue.jobs.lock().unwrap_or_else(|error| error.into_inner());
                    while jobs.is_empty() { jobs = queue.ready.wait(jobs).unwrap_or_else(|error| error.into_inner()); }
                    jobs.pop_front().unwrap()
                };
                if !job.alive.load(Ordering::Acquire) { continue; }
                let path = resolve(&job);
                if job.alive.load(Ordering::Acquire) { (job.done)(path); }
            });
        }
        queue
    })
}

fn push(queue: &Queue, job: Job) -> bool {
    let mut jobs = queue.jobs.lock().unwrap_or_else(|error| error.into_inner());
    jobs.retain(|job| job.alive.load(Ordering::Acquire));
    if jobs.len() == QUEUE { return false; }
    jobs.push_back(job);
    queue.ready.notify_one();
    true
}

/// The card (or the header) of the genre whose image key is `key`, as a file
/// on disk: `done` gets its path, or an empty one when there is none to show.
/// Dropping the handle cancels the request.
pub fn request_genre_art(key: &str, header: bool, done: impl FnOnce(String) + Send + 'static) -> Option<GenreArtRequest> {
    let digest = crate::discovery::genre_art_digest(key, header)?;
    let address = format!("{BASE}{key}/{}.webp", if header { "header" } else { "card" });
    let alive = Arc::new(AtomicBool::new(true));
    let job = Job { address, digest, alive:alive.clone(), done:Box::new(done) };
    if !push(queue(), job) { return None; }
    Some(GenreArtRequest(alive))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_indexed_keys_are_asked_and_the_queue_keeps_room_for_the_new_viewport() {
        assert!(request_genre_art("../rock", false, |_| ()).is_none());
        assert!(request_genre_art("not-a-genre", true, |_| ()).is_none());
        let queue = Queue { jobs:Mutex::new(VecDeque::new()), ready:Condvar::new() };
        let job = || {
            let alive = Arc::new(AtomicBool::new(true));
            (Job { address:format!("{BASE}rock/card.webp"), digest:"", alive:alive.clone(), done:Box::new(|_| panic!("no worker in this test")) }, GenreArtRequest(alive))
        };
        let mut handles = Vec::new();
        for _ in 0..QUEUE { let (job, handle) = job(); assert!(push(&queue, job)); handles.push(handle); }
        let (overflow, _) = job();
        assert!(!push(&queue, overflow));
        drop(handles);
        let (next, _handle) = job();
        assert!(push(&queue, next));
        assert_eq!(queue.jobs.lock().unwrap().len(), 1);
    }
}
