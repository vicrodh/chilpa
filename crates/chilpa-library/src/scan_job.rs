//! Scanning without freezing the interface.
//!
//! The scan opens its own connection on its own thread: a SQLite connection
//! is not shared across threads, and the interface has to keep reading
//! while writing happens. That is what WAL is for (see
//! [`crate::schema::apply_pragmas`]).
//!
//! What is reported is what really happened: files seen, read and skipped.
//! A bar that advances without knowing how many files there are is lying,
//! so here we count first and advance afterwards.

use crate::scan::ScanReport;
use crate::store::Library;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::thread::JoinHandle;

/// How the scan is going.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ScanProgress {
    /// How many files there are to look at is now known.
    Counted { total: usize },
    /// Van `done` de `total`.
    Advanced { done: usize, total: usize },
    /// It finished, with its report.
    Done(ScanReport),
    /// It could not be done.
    Failed(String),
}

/// A scan running in the background.
pub struct ScanJob {
    events: Receiver<ScanProgress>,
    thread: Option<JoinHandle<()>>,
}

impl ScanJob {
    /// Starts scanning a folder into the database at `db_path`.
    pub fn start(db_path: PathBuf, root: PathBuf, artwork_cache: Option<PathBuf>) -> Self {
        Self::start_all(db_path, vec![root], artwork_cache)
    }

    /// The same with several folders, one after another.
    ///
    /// The count is taken over all of them first, so the bar advances from
    /// start to finish instead of restarting on every folder.
    pub fn start_all(
        db_path: PathBuf,
        roots: Vec<PathBuf>,
        artwork_cache: Option<PathBuf>,
    ) -> Self {
        let (tx, rx) = channel();
        let thread = std::thread::Builder::new()
            .name("chilpa-scan".into())
            .spawn(move || {
                let lib = match Library::open(&db_path) {
                    Ok(l) => l,
                    Err(e) => {
                        let _ = tx.send(ScanProgress::Failed(e.to_string()));
                        return;
                    }
                };
                // Count first: without a total, a progress bar invents one.
                let total: usize = roots.iter().map(|r| crate::scan::count_audio_files(r)).sum();
                let _ = tx.send(ScanProgress::Counted { total });

                let mut accumulated = ScanReport {
                    root: roots.first().cloned().unwrap_or_default(),
                    unreachable: true,
                    ..ScanReport::default()
                };
                // What the earlier folders already account for, so progress does
                // not start over on each one.
                let mut previous = 0usize;
                for root in &roots {
                    let warn = |done: usize| {
                        let _ = tx.send(ScanProgress::Advanced { done: previous + done, total });
                    };
                    match crate::scan::scan_root_reporting(
                        &lib,
                        root,
                        artwork_cache.as_deref(),
                        &warn,
                    ) {
                        Ok(r) => {
                            previous += r.seen;
                            accumulated.seen += r.seen;
                            accumulated.read += r.read;
                            accumulated.unchanged += r.unchanged;
                            accumulated.missing += r.missing;
                            // As long as one folder could be read, the scan was worth
                            // something.
                            accumulated.unreachable &= r.unreachable;
                        }
                        Err(e) => {
                            let _ = tx.send(ScanProgress::Failed(e.to_string()));
                            return;
                        }
                    }
                }
                if roots.is_empty() {
                    accumulated.unreachable = false;
                }
                let _ = tx.send(ScanProgress::Done(accumulated));
            })
            .expect("el hilo de escaneo arranca");
        Self { events: rx, thread: Some(thread) }
    }

    /// Whatever has happened, without blocking.
    pub fn poll(&self) -> Vec<ScanProgress> {
        let mut out = Vec::new();
        loop {
            match self.events.try_recv() {
                Ok(e) => out.push(e),
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            }
        }
        out
    }

    /// True when the thread has finished.
    pub fn finished(&self) -> bool {
        self.thread.as_ref().is_some_and(|t| t.is_finished())
    }
}

impl Drop for ScanJob {
    fn drop(&mut self) {
        // The scan is not interrupted halfway: leaving the database with files
        // half-recorded is worse than waiting.
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Counts a folder's audio files, without touching the database.
pub fn count(root: &Path) -> usize {
    crate::scan::count_audio_files(root)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let dir = std::env::temp_dir().join("chilpa-scanjob-progreso");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("disco")).unwrap();
        for n in 0..4 {
            std::fs::write(dir.join("disco").join(format!("{n:02}.flac")), b"no es audio").unwrap();
        }
        std::fs::write(dir.join("notas.txt"), b"x").unwrap();

        let db = dir.join("lib.db");
        let job = ScanJob::start(db, dir.clone(), None);
        let events = to_the_end(&job);

        // The total arrives first: a bar without one invents its progress.
        match events.first() {
            Some(ScanProgress::Counted { total }) => {
                assert_eq!(*total, 4, "se cuentan los de audio, no el .txt")
            }
            other => panic!("lo primero es la cuenta: {other:?}"),
        }
        let mut seen = 0;
        for e in &events {
            if let ScanProgress::Advanced { done, total } = e {
                assert_eq!(*total, 4, "el total no se mueve a media cuenta");
                assert!(*done >= seen, "el avance no retrocede");
                seen = *done;
            }
        }
        match events.last() {
            Some(ScanProgress::Done(r)) => assert_eq!(r.seen, 4),
            other => panic!("debía terminar: {other:?}"),
        }
        drop(job);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn several_folders_count_together_and_progress_does_not_restart() {
        let dir = std::env::temp_dir().join("chilpa-scanjob-varias");
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
            Some(ScanProgress::Counted { total }) => assert_eq!(*total, 5, "las dos juntas"),
            other => panic!("lo primero es la cuenta: {other:?}"),
        }
        let mut seen = 0;
        for e in &events {
            if let ScanProgress::Advanced { done, .. } = e {
                assert!(*done >= seen, "el avance no reinicia entre carpetas");
                seen = *done;
            }
        }
        match events.last() {
            Some(ScanProgress::Done(r)) => {
                assert_eq!(r.seen, 5, "se vieron las cinco");
                assert!(!r.unreachable);
            }
            other => panic!("debía terminar: {other:?}"),
        }
        drop(job);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn if_one_folder_is_missing_the_rest_are_scanned_anyway() {
        let dir = std::env::temp_dir().join("chilpa-scanjob-falta");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("una.flac"), b"x").unwrap();

        let job = ScanJob::start_all(
            dir.join("lib.db"),
            vec![PathBuf::from("/no/existe"), dir.clone()],
            None,
        );
        let events = to_the_end(&job);
        match events.last() {
            Some(ScanProgress::Done(r)) => {
                assert_eq!(r.seen, 1);
                assert!(!r.unreachable, "una sí se pudo leer");
            }
            other => panic!("debía terminar: {other:?}"),
        }
        drop(job);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_root_that_does_not_exist_finishes_with_its_report() {
        let db = std::env::temp_dir().join("chilpa-scanjob-test.db");
        let _ = std::fs::remove_file(&db);
        let job = ScanJob::start(db.clone(), PathBuf::from("/no/existe"), None);
        let limit = std::time::Instant::now() + std::time::Duration::from_secs(5);
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
            Some(ScanProgress::Done(r)) => assert!(r.unreachable, "se marcó inalcanzable"),
            other => panic!("debía terminar con un reporte: {other:?}"),
        }
        drop(job);
        let _ = std::fs::remove_file(db);
    }
}
