//! One background scan, with cooperative cancellation and bounded progress.
//!
//! Each root is listed right before it is read, never ahead: a listing
//! taken while a share was away is empty, and read after the share came
//! back it took every track under it for deleted. So the total grows root
//! by root. The files found and the total count every root listed so far,
//! and the files read every root before the current one, so "Reading N of
//! M" never goes back and M grows when the next root has been listed.
//!
//! Memory (ADR-018): the worker holds the listing of one root at a time --
//! a path per audio file under it and one per folder it could not read --
//! and, while reading, the set of the paths that could be read, which the
//! reconciliation needs, of the same length. Both are bounded by that
//! root's file count (at about 100 bytes a path, some 10 MB each for
//! 100,000 files) and released when the root's scan returns, before the
//! next root is listed. The progress mailbox holds one event of each kind.
//! The facade runs one worker at a time.

use crate::scan::{Containers, NoContainers, ScanControl, ScanReport, ScanWatch};
use crate::store::Library;
use std::cell::Cell;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use std::thread::JoinHandle;

pub const MAX_ROOTS: usize = 256;

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ScanProgress {
    /// How many files the walk has found so far, in this root and the ones
    /// listed before it.
    Discovered { found: usize },
    /// The files of every root listed so far. It grows as each root is
    /// listed, right before it is read.
    Counted { total: usize },
    Advanced { done: usize, total: usize },
    /// What the scan of the first `done` files wrote is committed and can be
    /// shown. `written` counts the rows the scan has changed so far across
    /// roots -- availability, records and reconciliation: when it has not
    /// grown, nothing a screen shows has changed.
    Committed { done: usize, written: u64 },
    Phase { root: PathBuf, stage: ScanStage },
    Done(ScanReport),
    Interrupted(ScanReport),
    Failed(String),
}

/// What the scan is doing with a root.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ScanStage {
    Listing,
    Reading,
    Finalizing,
}

// I retain one of each kind of event -- found, count, progress, commit,
// phase -- and one terminal event. A hidden or busy frontend never builds a
// queue proportional to file count.
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

/// The worker's connection. The one that started the scan already opened
/// the library, so its upgrade and maintenance are done; opening the full
/// way again read every local track before the first file was looked at,
/// 230 ms on thirty thousand and growing with the library, not the folder.
/// A library that is not there yet, or not at this version, is opened the
/// full way, which is what creates it.
fn open(db_path: &Path) -> rusqlite::Result<Library> {
    Library::open_existing(db_path).or_else(|_| Library::open(db_path))
}

fn run(db_path: PathBuf, roots: Vec<PathBuf>, artwork_cache: Option<PathBuf>, containers: Arc<dyn Containers + Send + Sync>, control: ScanControl, events: Arc<Mailbox>) {
    let mut accumulated = ScanReport { root: roots.first().cloned().unwrap_or_default(), unreachable: !roots.is_empty(), ..ScanReport::default() };
    let lib = match open(&db_path) {
        Ok(lib) => lib,
        Err(error) => { events.send(ScanProgress::Failed(error.to_string())); return; }
    };
    // The files of the roots already read, which were listed and read
    // whole, and the rows reading them wrote.
    let (mut before, mut written) = (0, 0);
    for root in &roots {
        events.send(ScanProgress::Phase { root: root.clone(), stage: ScanStage::Listing });
        let watch = RootWatch { events: &events, root, before, written, files: Cell::new(0), wrote: Cell::new(0) };
        match crate::scan::scan_watched(&lib, root, artwork_cache.as_deref(), &*containers, &control, &watch) {
            Ok(report) => {
                before += watch.files.get();
                written += watch.wrote.get();
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

/// One root's scan, told in the job's terms: its counts run on from the
/// roots before it.
struct RootWatch<'a> {
    events: &'a Mailbox,
    root: &'a Path,
    /// The files of the roots already read, and the rows they wrote.
    before: usize,
    written: u64,
    /// This root's files, once it is listed, and the rows it has written.
    files: Cell<usize>,
    wrote: Cell<u64>,
}

impl ScanWatch for RootWatch<'_> {
    fn found(&self, so_far: usize) {
        self.events.send(ScanProgress::Discovered { found: self.before + so_far });
    }
    fn listed(&self, files: usize) {
        self.files.set(files);
        self.events.send(ScanProgress::Counted { total: self.before + files });
        self.events.send(ScanProgress::Phase { root: self.root.to_path_buf(), stage: ScanStage::Reading });
    }
    fn advanced(&self, done: usize) {
        self.events.send(ScanProgress::Advanced { done: self.before + done, total: self.before + self.files.get() });
    }
    fn committed(&self, done: usize, written: u64) {
        self.wrote.set(written);
        self.events.send(ScanProgress::Committed { done: self.before + done, written: self.written + written });
    }
    fn finalizing(&self) {
        self.events.send(ScanProgress::Phase { root: self.root.to_path_buf(), stage: ScanStage::Finalizing });
    }
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
        for found in 0..100_000 { mailbox.send(ScanProgress::Discovered { found }); }
        mailbox.send(ScanProgress::Counted { total: 100_000 });
        for done in 0..100_000 {
            mailbox.send(ScanProgress::Advanced { done, total: 100_000 });
            mailbox.send(ScanProgress::Committed { done, written: 0 });
            mailbox.send(ScanProgress::Phase { root: PathBuf::from("music"), stage: ScanStage::Reading });
        }
        mailbox.send(ScanProgress::Done(ScanReport::default()));
        let events = mailbox.poll();
        assert_eq!(events.len(), 6);
        assert!(matches!(events[0], ScanProgress::Discovered { found: 99_999 }));
        assert!(matches!(events[1], ScanProgress::Counted { .. }));
        assert!(matches!(events[2], ScanProgress::Advanced { done: 99_999, .. }));
        assert!(matches!(events[3], ScanProgress::Committed { done: 99_999, .. }));
        assert!(matches!(events[5], ScanProgress::Done(_)));
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
            std::thread::sleep(std::time::Duration::from_millis(1));
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

        // The total arrives before any progress: a bar without one invents
        // its progress.
        let counted = events.iter().position(|e| matches!(e, ScanProgress::Counted { .. }));
        let advanced = events.iter().position(|e| matches!(e, ScanProgress::Advanced { .. }));
        match counted.map(|at| &events[at]) {
            Some(ScanProgress::Counted { total }) => {
                assert_eq!(*total, 4, "the audio ones are counted, not the .txt")
            }
            other => panic!("the count comes before progress: {other:?}"),
        }
        assert!(counted < advanced, "{events:?}");
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
    fn a_long_walk_says_what_it_found_and_the_first_batch_is_committed_early() {
        let dir = tempfile::tempdir().unwrap();
        for n in 0..520 {
            std::fs::write(dir.path().join(format!("{n:03}.flac")), b"not audio").unwrap();
        }
        let job = ScanJob::start(dir.path().join("lib.db"), dir.path().to_path_buf(), None);
        let events = to_the_end(&job);
        let found = events.iter().position(|e| matches!(e, ScanProgress::Discovered { found: 500 }));
        let counted = events.iter().position(|e| matches!(e, ScanProgress::Counted { total: 520 }));
        assert!(found.is_some() && found < counted, "the walk is watched before the total is known: {events:?}");
        // The commit notice is coalesced, so a poll sees the latest; one
        // arrives while the scan still runs, and never says more than a
        // batch was written before the first was.
        let first_commit = events.iter().find_map(|e| match e { ScanProgress::Committed { done, .. } => Some(*done), _ => None });
        assert!(first_commit.is_some_and(|done| done <= 200), "{events:?}");
        let committed = events.iter().position(|e| matches!(e, ScanProgress::Committed { .. }));
        let done = events.iter().position(|e| matches!(e, ScanProgress::Done(_)));
        assert!(committed < done);
        let lib = Library::open(&dir.path().join("lib.db")).unwrap();
        assert_eq!(lib.track_count(false).unwrap(), 520);
    }

    #[test]
    fn a_commit_says_whether_reading_wrote_anything() {
        let dir = tempfile::tempdir().unwrap();
        for n in 0..450 {
            std::fs::write(dir.path().join(format!("{n:03}.flac")), b"not audio").unwrap();
        }
        let written = |events: &[ScanProgress]| -> Vec<u64> {
            events.iter().filter_map(|e| match e { ScanProgress::Committed { written, .. } => Some(*written), _ => None }).collect()
        };
        let first = to_the_end(&ScanJob::start(dir.path().join("lib.db"), dir.path().to_path_buf(), None));
        assert!(written(&first).last().is_some_and(|rows| *rows > 0), "{first:?}");
        let again = to_the_end(&ScanJob::start(dir.path().join("lib.db"), dir.path().to_path_buf(), None));
        let rows = written(&again);
        assert!(!rows.is_empty() && rows.iter().all(|rows| *rows == 0), "a rescan that finds every file as it was writes nothing: {again:?}");
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

        // Each folder is listed right before it is read, so the total grows
        // folder by folder; it never shrinks and progress never restarts.
        let (mut seen, mut most) = (0, 0);
        for e in &events {
            match e {
                ScanProgress::Counted { total } => {
                    assert!(*total >= most, "the total does not shrink: {events:?}");
                    most = *total;
                }
                ScanProgress::Advanced { done, total } => {
                    assert!(*done >= seen, "progress does not restart between folders");
                    assert!(*done <= *total && *total <= 5, "{events:?}");
                    seen = *done;
                }
                _ => {}
            }
        }
        assert_eq!(most, 5, "both together in the end");
        // Neither folder reaches a first batch; each still says its last
        // batch is written, and the latest notice is the second's.
        let last_commit = events.iter().rev().find_map(|e| match e { ScanProgress::Committed { done, written } => Some((*done, *written)), _ => None });
        assert!(last_commit.is_some_and(|(done, written)| done == 5 && written > 0), "{events:?}");
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

    /// A share that is away when the scan starts and back by its turn: an
    /// empty listing taken while it was away took every track for deleted.
    #[test]
    fn a_folder_away_when_the_scan_starts_and_back_by_its_turn_loses_nothing() {
        struct Remount { parked: PathBuf, back: PathBuf }
        impl Containers for Remount {
            fn may_hold(&self, path: &Path) -> bool { path.extension().is_some_and(|extension| extension == "disc") }
            // Read while the other folder is scanned, before the share's turn.
            fn tracks(&self, _: &Path) -> Option<Vec<crate::scan::ContainedTrack>> {
                let _ = std::fs::rename(&self.parked, &self.back);
                None
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let (share, other, parked) = (dir.path().join("share"), dir.path().join("other"), dir.path().join("parked"));
        std::fs::create_dir_all(share.join("album")).unwrap();
        for n in 0..3 { std::fs::write(share.join("album").join(format!("{n}.flac")), b"not audio").unwrap(); }
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(other.join("one.flac"), b"not audio").unwrap();
        std::fs::write(other.join("remount.disc"), b"x").unwrap();
        let db = dir.path().join("lib.db");
        crate::scan::scan_root(&Library::open(&db).unwrap(), &share).unwrap();
        // Away: the mount point is there and empty.
        std::fs::rename(share.join("album"), &parked).unwrap();

        let back = share.join("album");
        let job = ScanJob::start_with(db.clone(), vec![other, share.clone()], None, Arc::new(Remount { parked, back }));
        let report = match to_the_end(&job).last() {
            Some(ScanProgress::Done(report)) => report.clone(),
            other => panic!("it should have finished: {other:?}"),
        };
        drop(job);
        assert_eq!(report.missing, 0, "nothing under the share was taken for deleted");
        let lib = Library::open_existing(&db).unwrap();
        let under = format!("{}/%", share.display());
        let present: i64 = lib.db.query_row("SELECT count(*) FROM tracks WHERE present = 1 AND external_id LIKE ?1", [&under], |r| r.get(0)).unwrap();
        let scanned: i64 = lib.db.query_row("SELECT count(*) FROM scanned_files WHERE path LIKE ?1", [&under], |r| r.get(0)).unwrap();
        assert_eq!((present, scanned), (3, 3), "the share's tracks and what was scanned of them are kept");
    }

    #[test]
    fn the_worker_opens_a_library_the_facade_already_has_without_its_maintenance() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("lib.db");
        std::fs::write(dir.path().join("one.flac"), b"x").unwrap();
        drop(Library::open(&db).unwrap());
        let before = std::fs::metadata(&db).unwrap().modified().unwrap();
        let lib = open(&db).unwrap();
        // The light open neither migrates nor rewrites anything.
        assert_eq!(std::fs::metadata(&db).unwrap().modified().unwrap(), before);
        drop(lib);
        let job = ScanJob::start(db.clone(), dir.path().to_path_buf(), None);
        match to_the_end(&job).last() {
            Some(ScanProgress::Done(report)) => assert_eq!(report.seen, 1),
            other => panic!("it should have finished: {other:?}"),
        }
    }
}
