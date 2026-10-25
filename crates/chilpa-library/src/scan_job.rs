//! One background scan, with cooperative cancellation and bounded progress.

use crate::scan::{Containers, NoContainers, ScanControl, ScanReport};
use crate::store::Library;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use std::thread::JoinHandle;

pub const MAX_ROOTS: usize = 256;

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ScanProgress {
    Counted { total: usize },
    Advanced { done: usize, total: usize },
    Phase { root: PathBuf, finalizing: bool },
    Done(ScanReport),
    Interrupted(ScanReport),
    Failed(String),
}

// I retain one count, one progress update, one phase and one terminal event.
// A hidden or busy frontend never builds a queue proportional to file count.
#[derive(Default)]
struct Mailbox { events: Mutex<VecDeque<ScanProgress>>, terminal: AtomicBool }
impl Mailbox {
    fn send(&self, event: ScanProgress) {
        let mut events = self.events.lock().unwrap_or_else(|error| error.into_inner());
        events.retain(|old| std::mem::discriminant(old) != std::mem::discriminant(&event));
        let terminal = matches!(event, ScanProgress::Done(_) | ScanProgress::Interrupted(_) | ScanProgress::Failed(_));
        events.push_back(event);
        if terminal { self.terminal.store(true, Ordering::Release); }
    }
    fn poll(&self) -> Vec<ScanProgress> {
        self.events.lock().unwrap_or_else(|error| error.into_inner()).drain(..).collect()
    }
}

pub struct ScanJob {
    events: Arc<Mailbox>,
    control: ScanControl,
    thread: Option<JoinHandle<()>>,
}

impl ScanJob {
    pub fn start(db_path: PathBuf, root: PathBuf, artwork_cache: Option<PathBuf>) -> Self {
        Self::start_all(db_path, vec![root], artwork_cache)
    }

    pub fn start_all(db_path: PathBuf, roots: Vec<PathBuf>, artwork_cache: Option<PathBuf>) -> Self {
        Self::start_with(db_path, roots, artwork_cache, Arc::new(NoContainers))
    }

    pub fn start_with(db_path: PathBuf, roots: Vec<PathBuf>, artwork_cache: Option<PathBuf>, containers: Arc<dyn Containers + Send + Sync>) -> Self {
        Self::try_start_with(db_path, roots, artwork_cache, containers).expect("the scan thread starts")
    }

    pub fn try_start_with(db_path: PathBuf, roots: Vec<PathBuf>, artwork_cache: Option<PathBuf>, containers: Arc<dyn Containers + Send + Sync>) -> std::io::Result<Self> {
        if roots.len() > MAX_ROOTS { return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "a scan supports at most 256 roots")); }
        let events = Arc::new(Mailbox::default());
        let worker_events = events.clone();
        let control = ScanControl::default();
        let worker_control = control.clone();
        let thread = std::thread::Builder::new().name("chilpa-scan".into()).spawn(move || {
            run(db_path, roots, artwork_cache, containers, worker_control, worker_events);
        })?;
        Ok(Self { events, control, thread: Some(thread) })
    }

    pub fn poll(&self) -> Vec<ScanProgress> {
        let mut events = self.events.poll();
        if self.finished() && self.events.terminal.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_ok() {
            events.push(ScanProgress::Failed(chilpa_i18n::t("The scan worker stopped unexpectedly.")));
        }
        events
    }
    pub fn stop(&self) { self.control.stop(); }
    pub fn finished(&self) -> bool { self.thread.as_ref().is_some_and(|thread| thread.is_finished()) }
}

fn run(db_path: PathBuf, roots: Vec<PathBuf>, artwork_cache: Option<PathBuf>, containers: Arc<dyn Containers + Send + Sync>, control: ScanControl, events: Arc<Mailbox>) {
    let mut accumulated = ScanReport { root: roots.first().cloned().unwrap_or_default(), unreachable: !roots.is_empty(), ..ScanReport::default() };
    let lib = match Library::open(&db_path) {
        Ok(lib) => lib,
        Err(error) => { events.send(ScanProgress::Failed(error.to_string())); return; }
    };
    // I retain at most 256 counters, released with this worker.
    let mut counts = Vec::with_capacity(roots.len());
    for root in &roots {
        let Some(count) = crate::scan::count_controlled(root, &*containers, &control) else {
            accumulated.interrupted = true;
            events.send(ScanProgress::Interrupted(accumulated));
            return;
        };
        counts.push(count);
    }
    let total = counts.iter().sum();
    events.send(ScanProgress::Counted { total });
    let mut previous = 0;
    for (root, count) in roots.iter().zip(counts) {
        events.send(ScanProgress::Phase { root: root.clone(), finalizing: false });
        let progress = |done| events.send(ScanProgress::Advanced { done: previous + done, total });
        let finalizing = || events.send(ScanProgress::Phase { root: root.clone(), finalizing: true });
        match crate::scan::scan_controlled(&lib, root, artwork_cache.as_deref(), &progress, &*containers, &control, &finalizing) {
            Ok(report) => {
                previous += count;
                accumulated.seen += report.seen;
                accumulated.read += report.read;
                accumulated.unchanged += report.unchanged;
                accumulated.missing += report.missing;
                accumulated.unreadable += report.unreadable;
                accumulated.sidecars_unreadable += report.sidecars_unreadable;
                accumulated.unreachable &= report.unreachable;
                accumulated.changed |= report.changed;
                if report.interrupted {
                    accumulated.interrupted = true;
                    events.send(ScanProgress::Interrupted(accumulated));
                    return;
                }
            }
            Err(error) => { events.send(ScanProgress::Failed(error.to_string())); return; }
        }
    }
    events.send(ScanProgress::Done(accumulated));
}

impl Drop for ScanJob {
    fn drop(&mut self) {
        self.control.stop();
        if let Some(thread) = self.thread.take() { let _ = thread.join(); }
    }
}

pub fn count(root: &Path) -> usize { crate::scan::count_audio_files(root) }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panicked_worker_reports_failure_once_instead_of_running_forever() {
        struct Panics;
        impl Containers for Panics {
            fn may_hold(&self, path: &Path) -> bool { path.extension().is_some_and(|extension| extension == "disc") }
            fn tracks(&self, _: &Path) -> Option<Vec<crate::scan::ContainedTrack>> { panic!("injected container failure") }
        }
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("music");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("album.disc"), "fixture").unwrap();
        let job = ScanJob::start_with(directory.path().join("library.db"), vec![root], None, Arc::new(Panics));
        let events = to_the_end(&job);
        assert!(matches!(events.last(), Some(ScanProgress::Failed(_))));
        assert!(job.finished());
        assert!(job.poll().is_empty(), "the terminal failure is emitted once");
    }

    #[test]
    fn progress_mailbox_stays_bounded_when_nobody_polls() {
        let mailbox = Mailbox::default();
        mailbox.send(ScanProgress::Counted { total: 100_000 });
        for done in 0..100_000 {
            mailbox.send(ScanProgress::Advanced { done, total: 100_000 });
            mailbox.send(ScanProgress::Phase { root: PathBuf::from("music"), finalizing: false });
        }
        mailbox.send(ScanProgress::Done(ScanReport::default()));
        let events = mailbox.poll();
        assert_eq!(events.len(), 4);
        assert!(matches!(events[0], ScanProgress::Counted { .. }));
        assert!(matches!(events[1], ScanProgress::Advanced { done: 99_999, .. }));
        assert!(matches!(events[3], ScanProgress::Done(_)));
        assert!(mailbox.poll().is_empty());
    }

    /// Waits for the scan to finish and returns everything it reported.
    fn to_the_end(job: &ScanJob) -> Vec<ScanProgress> {
        let limit = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut events = Vec::new();
        while std::time::Instant::now() < limit {
            events.extend(job.poll());
            if events.iter().any(|e| matches!(e, ScanProgress::Done(_) | ScanProgress::Failed(_)))
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        events
    }

    #[test]
    fn counting_comes_before_progress_and_the_total_does_not_move() {
        let dir = std::env::temp_dir().join("chilpa-scanjob-progress");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("album")).unwrap();
        for n in 0..4 {
            std::fs::write(dir.join("album").join(format!("{n:02}.flac")), b"not audio").unwrap();
        }
        std::fs::write(dir.join("notes.txt"), b"x").unwrap();

        let db = dir.join("lib.db");
        let job = ScanJob::start(db, dir.clone(), None);
        let events = to_the_end(&job);

        // The total arrives first: a bar without one invents its progress.
        match events.first() {
            Some(ScanProgress::Counted { total }) => {
                assert_eq!(*total, 4, "the audio ones are counted, not the .txt")
            }
            other => panic!("the count comes first: {other:?}"),
        }
        let mut seen = 0;
        for e in &events {
            if let ScanProgress::Advanced { done, total } = e {
                assert_eq!(*total, 4, "the total does not move mid count");
                assert!(*done >= seen, "progress does not go backwards");
                seen = *done;
            }
        }
        match events.last() {
            Some(ScanProgress::Done(r)) => assert_eq!(r.seen, 4),
            other => panic!("it should have finished: {other:?}"),
        }
        drop(job);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn several_folders_count_together_and_progress_does_not_restart() {
        let dir = std::env::temp_dir().join("chilpa-scanjob-several");
        let _ = std::fs::remove_dir_all(&dir);
        let (a, b) = (dir.join("a"), dir.join("b"));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        for n in 0..3 {
            std::fs::write(a.join(format!("{n}.flac")), b"x").unwrap();
        }
        for n in 0..2 {
            std::fs::write(b.join(format!("{n}.flac")), b"x").unwrap();
        }

        let job = ScanJob::start_all(dir.join("lib.db"), vec![a, b], None);
        let events = to_the_end(&job);

        match events.first() {
            Some(ScanProgress::Counted { total }) => assert_eq!(*total, 5, "both together"),
            other => panic!("the count comes first: {other:?}"),
        }
        let mut seen = 0;
        for e in &events {
            if let ScanProgress::Advanced { done, .. } = e {
                assert!(*done >= seen, "progress does not restart between folders");
                seen = *done;
            }
        }
        match events.last() {
            Some(ScanProgress::Done(r)) => {
                assert_eq!(r.seen, 5, "all five were seen");
                assert!(!r.unreachable);
            }
            other => panic!("it should have finished: {other:?}"),
        }
        drop(job);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn if_one_folder_is_missing_the_rest_are_scanned_anyway() {
        let dir = std::env::temp_dir().join("chilpa-scanjob-missing");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("one.flac"), b"x").unwrap();

        let job = ScanJob::start_all(
            dir.join("lib.db"),
            vec![PathBuf::from("/does/not/exist"), dir.clone()],
            None,
        );
        let events = to_the_end(&job);
        match events.last() {
            Some(ScanProgress::Done(r)) => {
                assert_eq!(r.seen, 1);
                assert!(!r.unreachable, "one could be read");
            }
            other => panic!("it should have finished: {other:?}"),
        }
        drop(job);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_root_that_does_not_exist_finishes_with_its_report() {
        let db = std::env::temp_dir().join("chilpa-scanjob-test.db");
        let _ = std::fs::remove_file(&db);
        let job = ScanJob::start(db.clone(), PathBuf::from("/does/not/exist"), None);
        let limit = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let mut events = Vec::new();
        while std::time::Instant::now() < limit {
            events.extend(job.poll());
            if events.iter().any(|e| matches!(e, ScanProgress::Done(_) | ScanProgress::Failed(_)))
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        match events.last() {
            Some(ScanProgress::Done(r)) => assert!(r.unreachable, "it was marked unreachable"),
            other => panic!("it should have finished with a report: {other:?}"),
        }
        drop(job);
        let _ = std::fs::remove_file(db);
    }
}
