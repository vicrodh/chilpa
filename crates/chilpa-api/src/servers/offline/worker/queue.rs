//! The one offline worker's queue. Every selection a menu or the page sends
//! waits here in order, so starting a second one never refuses or replaces
//! the first. Each item carries its library track id: the rows read how
//! their own track goes from here, wherever the listener is.
//!
//! Bounded: at most `MAX_TRACKS` items and `MAX_SELECTION_BYTES` of their
//! text wait at once, the session keeps at most `MAX_TRACKS` failures and
//! the last `MAX_JOBS` selections' counts.

use super::*;
use crate::servers::session::Scope;
use std::collections::{HashSet, VecDeque};

const MAX_JOBS: usize = 64;

pub(in crate::servers::offline) enum Work {
    Fetch { track: Track, fresh: bool },
    Remove { copy: Copy },
    /// The startup check of every registered file (`recovery`).
    Recover,
}

impl Work {
    fn bytes(&self) -> usize {
        match self {
            Work::Fetch { track, .. } => {
                track.entry.len() + track.title.len() + track.revision.as_ref().map_or(0, String::len)
                    + track.codec.as_ref().map_or(0, String::len) + std::mem::size_of::<Track>()
            }
            Work::Remove { copy } => copy.retained_bytes(),
            Work::Recover => 0,
        }
    }
}

pub(in crate::servers::offline) struct Item {
    /// The library's track, or 0 when the work is not a track's.
    pub(super) id: i64,
    pub(super) work: Work,
    pub(super) job: Arc<Job>,
}

/// What one selection captured when it was chosen.
pub(in crate::servers::offline) struct Job {
    pub(in crate::servers::offline) seq: u64,
    pub(in crate::servers::offline) scopes: HashMap<i64, Option<Scope>>,
    pub(in crate::servers::offline) collection: String,
    pub(in crate::servers::offline) root: PathBuf,
    pub(in crate::servers::offline) limit: Limit,
}

impl Job {
    pub(super) fn scope(&self, source: i64) -> Option<&Scope> {
        self.scopes.get(&source).and_then(Option::as_ref)
    }
}

/// How one selection goes: the line under its page's header reads it.
#[derive(Clone, Debug, Default, PartialEq)]
pub(in crate::servers::offline) struct Count {
    pub(in crate::servers::offline) seq: u64,
    pub(in crate::servers::offline) kind: String,
    pub(in crate::servers::offline) ids: Vec<i64>,
    pub(in crate::servers::offline) total: usize,
    pub(in crate::servers::offline) settled: usize,
}

/// The item the worker is on.
pub(in crate::servers::offline) struct Current {
    pub(in crate::servers::offline) id: i64,
    pub(in crate::servers::offline) seq: u64,
    pub(in crate::servers::offline) phase: &'static str,
    pub(in crate::servers::offline) bytes: u64,
    /// The file's size when it is known (a re-download), else 0.
    pub(in crate::servers::offline) total: u64,
    pub(in crate::servers::offline) cancel: Arc<AtomicBool>,
}

#[derive(Default)]
pub(in crate::servers::offline) struct Tracker {
    queue: VecDeque<Item>,
    queued_bytes: usize,
    /// A worker thread is running; set and cleared under this lock.
    pub(super) alive: bool,
    pub(in crate::servers::offline) current: Option<Current>,
    /// Why a track failed this session, by library track id.
    pub(in crate::servers::offline) failures: HashMap<i64, String>,
    pub(in crate::servers::offline) jobs: VecDeque<Count>,
    pub(super) progress: Progress,
    seq: u64,
    /// Moves when what waits or what failed changes, not with each byte.
    pub(in crate::servers::offline) revision: u64,
}

impl Tracker {
    pub(in crate::servers::offline) fn queued(&self) -> HashSet<i64> {
        self.queue.iter().map(|item| item.id).filter(|id| *id > 0).collect()
    }

    pub(in crate::servers::offline) fn busy(&self) -> bool {
        self.alive || !self.queue.is_empty()
    }

    fn settle(&mut self, seq: u64) {
        if let Some(count) = self.jobs.iter_mut().find(|count| count.seq == seq) {
            count.settled = (count.settled + 1).min(count.total);
        }
    }

    /// Waits `work` behind what is already queued. False when it does not
    /// fit; nothing is queued then.
    pub(in crate::servers::offline) fn push(&mut self, kind: &str, ids: Vec<i64>, job: impl FnOnce(u64) -> Job, work: Vec<(i64, Work)>) -> bool {
        let bytes: usize = work.iter().map(|(_, work)| work.bytes()).sum();
        if work.is_empty() || self.queue.len() + work.len() > MAX_TRACKS || self.queued_bytes + bytes > MAX_SELECTION_BYTES {
            return false;
        }
        self.seq += 1;
        let job = Arc::new(job(self.seq));
        if !self.busy() {
            self.progress = Progress { running: true, ..Default::default() };
        }
        self.progress.running = true;
        self.progress.total += work.len();
        if !matches!(work[0].1, Work::Recover) {
            if self.jobs.len() >= MAX_JOBS {
                self.jobs.pop_front();
            }
            self.jobs.push_back(Count { seq: self.seq, kind: kind.into(), ids, total: work.len(), settled: 0 });
        }
        for (id, work) in work {
            self.failures.remove(&id);
            self.queue.push_back(Item { id, work, job: job.clone() });
        }
        self.queued_bytes += bytes;
        self.revision += 1;
        true
    }

    /// Marks `ids` failed with `error`, for a selection refused whole.
    pub(in crate::servers::offline) fn refuse(&mut self, ids: &[i64], error: &str) {
        for id in ids {
            if self.failures.len() < MAX_TRACKS || self.failures.contains_key(id) {
                self.failures.insert(*id, error.chars().take(512).collect());
            }
        }
        self.revision += 1;
    }

    /// Drops the waiting items `chosen` keeps and cancels the current one
    /// if it keeps it too. A cancelled item leaves no failure behind.
    pub(in crate::servers::offline) fn cancel(&mut self, chosen: impl Fn(i64) -> bool) {
        let (kept, dropped): (VecDeque<Item>, VecDeque<Item>) = std::mem::take(&mut self.queue).into_iter().partition(|item| !chosen(item.id));
        self.queue = kept;
        for item in dropped {
            self.queued_bytes = self.queued_bytes.saturating_sub(item.work.bytes());
            self.progress.total = self.progress.total.saturating_sub(1);
            self.settle(item.job.seq);
        }
        if let Some(current) = self.current.as_ref().filter(|current| chosen(current.id)) {
            current.cancel.store(true, Ordering::Release);
        }
        self.revision += 1;
    }

    pub(in crate::servers::offline) fn cancel_all(&mut self) {
        let stopped = !self.queue.is_empty() || self.current.is_some();
        self.cancel(|_| true);
        if stopped {
            self.progress.cancelled = true;
        }
    }

    /// The next item, made current. None ends the worker: `alive` falls in
    /// the same lock that a new selection checks.
    fn next(&mut self) -> Option<(Item, Arc<AtomicBool>)> {
        self.current = None;
        let Some(item) = self.queue.pop_front() else {
            self.alive = false;
            self.progress.running = false;
            self.revision += 1;
            return None;
        };
        self.queued_bytes = self.queued_bytes.saturating_sub(item.work.bytes());
        let cancel = Arc::new(AtomicBool::new(false));
        let (phase, title) = match &item.work {
            Work::Fetch { track, .. } => ("downloading", track.title.chars().take(512).collect()),
            Work::Remove { .. } => ("removing", String::new()),
            Work::Recover => ("checking", String::new()),
        };
        self.current = Some(Current { id: item.id, seq: item.job.seq, phase, bytes: 0, total: 0, cancel: cancel.clone() });
        self.progress.phase = phase;
        self.progress.removing = matches!(item.work, Work::Remove { .. });
        self.progress.title = title;
        self.progress.bytes = 0;
        self.revision += 1;
        Some((item, cancel))
    }

    fn finish(&mut self, item: &Item, result: Result<(), String>, cancelled: bool) {
        if matches!(item.work, Work::Recover) {
            if let Err(error) = result {
                self.progress.error = error;
            }
        } else {
            self.settle(item.job.seq);
            match result {
                Ok(()) => self.progress.done += 1,
                Err(_) if cancelled => self.progress.total = self.progress.total.saturating_sub(1),
                Err(error) => {
                    self.progress.failed += 1;
                    self.progress.error = error.clone();
                    if item.id > 0 {
                        self.refuse(&[item.id], &error);
                    }
                }
            }
        }
        self.current = None;
        self.revision += 1;
    }
}

pub(in crate::servers::offline) fn lock(tracker: &Mutex<Tracker>) -> std::sync::MutexGuard<'_, Tracker> {
    tracker.lock().unwrap_or_else(|error| error.into_inner())
}

impl Chilpa {
    /// Queues `work` and starts the worker when none runs. The answer is
    /// the page's status, or the reason nothing was queued.
    pub(in crate::servers::offline) fn offline_enqueue(&mut self, kind: &str, ids: Vec<i64>, job: impl FnOnce(u64) -> Job, work: Vec<(i64, Work)>) -> String {
        self.poll_server_offline();
        let chosen: Vec<i64> = work.iter().map(|(id, _)| *id).filter(|id| *id > 0).collect();
        let Some(keys) = self.servers.keys.clone() else {
            let error = chilpa_i18n::t("servers are not started");
            lock(&self.servers.offline.tracker).refuse(&chosen, &error);
            return json!({"error": error}).to_string();
        };
        let state = &mut self.servers.offline;
        let start = {
            let mut tracker = lock(&state.tracker);
            if !tracker.push(kind, ids, job, work) {
                let error = chilpa_i18n::t("Choose between 1 and 10,000 server tracks for an offline download.");
                tracker.refuse(&chosen, &error);
                return json!({"error": error}).to_string();
            }
            let start = !tracker.alive;
            tracker.alive = true;
            start
        };
        if start {
            if let Some(task) = state.task.take() {
                let _ = task.join();
            }
            let (tracker, changed, path) = (state.tracker.clone(), state.changed.clone(), Library::default_path());
            let task = std::thread::Builder::new().name("chilpa-offline-download".into()).spawn(move || run(&path, &keys, &tracker, &changed));
            match task {
                Ok(task) => state.task = Some(task),
                Err(_) => {
                    let error = chilpa_i18n::t("The download worker could not be started.");
                    let mut tracker = lock(&state.tracker);
                    tracker.cancel_all();
                    tracker.alive = false;
                    tracker.progress.running = false;
                    tracker.refuse(&chosen, &error);
                    return json!({"error": error}).to_string();
                }
            }
        }
        self.server_offline_status()
    }
}

/// The worker: one item after another until the queue is empty.
pub(super) fn run(path: &Path, keys: &super::super::super::keys::Keys, tracker: &Mutex<Tracker>, changed: &AtomicBool) {
    let lib = Library::open_existing(path).map_err(|_| chilpa_i18n::t("the library could not be opened"));
    loop {
        // The lock is let go before the item runs: its progress takes it.
        let next = lock(tracker).next();
        let Some((item, cancel)) = next else { return };
        let result = lib.as_ref().map_err(Clone::clone).and_then(|lib| work(lib, keys, tracker, &item, &cancel, changed));
        let cancelled = cancel.load(Ordering::Acquire);
        let result = match result {
            Err(_) if !cancelled && matches!(item.work, Work::Fetch { .. }) && chilpa_core::network::current().offline => Err(chilpa_core::network::blocked()),
            other => other,
        };
        if result.is_ok() && !matches!(item.work, Work::Recover) {
            changed.store(true, Ordering::Release);
        }
        lock(tracker).finish(&item, result, cancelled);
    }
}

fn work(lib: &Library, keys: &super::super::super::keys::Keys, tracker: &Mutex<Tracker>, item: &Item, cancel: &AtomicBool, changed: &AtomicBool) -> Result<(), String> {
    match &item.work {
        Work::Fetch { track, fresh } => {
            if chilpa_core::network::current().offline {
                return Err(chilpa_core::network::blocked());
            }
            let scope = item.job.scope(track.source).ok_or_else(super::super::super::session::changed)?;
            obtain(lib, &item.job.root, keys, track, scope, &item.job.collection, &item.job.limit, cancel, *fresh, |update| {
                let mut tracker = lock(tracker);
                let Some(current) = tracker.current.as_mut() else { return };
                match update {
                    Update::Bytes(bytes) => {
                        current.bytes = bytes;
                        current.phase = "downloading";
                    }
                    Update::Verifying => current.phase = "verifying",
                    Update::Size(total) => current.total = total,
                }
                let (phase, bytes) = (current.phase, current.bytes);
                tracker.progress.phase = phase;
                tracker.progress.bytes = bytes;
            })
        }
        Work::Remove { copy } => removal::remove(lib, copy, item.job.scope(copy.source)),
        Work::Recover => recovery::check_all(lib, cancel, |result| {
            let mut tracker = lock(tracker);
            match result {
                Ok(true) => {
                    tracker.progress.done += 1;
                    changed.store(true, Ordering::Release);
                }
                Ok(false) => tracker.progress.done += 1,
                Err(error) => {
                    tracker.progress.failed += 1;
                    tracker.progress.error = error;
                }
            }
        }),
    }
}

#[cfg(test)]
mod tests;
