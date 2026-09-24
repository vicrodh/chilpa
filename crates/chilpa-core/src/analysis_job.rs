//! Analysing the library without getting in anyone's way.
//!
//! The analysis reads files and writes numbers. It does not touch the output
//! path, it does not share the interface's connection and it runs on its own
//! thread, like the scan.
//!
//! Two precautions that make it cheap:
//!
//! 1. **The whole track is not read.** It seeks to a quarter of the way in
//!    and takes a minute. A long intro or a fade out says nothing about the
//!    pulse, and reading sixty minutes off disk per album would turn this
//!    into something nobody lets finish.
//! 2. **It is asked for in batches.** The pending list comes out of the
//!    database a little at a time; a library of a hundred thousand tracks
//!    does not fit in memory and has no reason to.

use chilpa_library::Library;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::sync::Arc;
use std::thread::JoinHandle;

/// What this analyser is called in the database. It is the one from
/// `chilpa-analysis`, repeated here so the facade need not know that crate.
pub const ANALYZER: &str = chilpa_analysis::NAME;

/// How much audio is looked at per track.
const SECONDS_EXAMINED: u64 = 60;

/// How many tracks are asked for per batch.
const BATCH: usize = 200;

/// How the analysis is going.
#[derive(Clone, PartialEq, Debug)]
pub enum AnalysisProgress {
    /// How many are left is now known.
    Counted { total: usize },
    /// `done` of `total` are through, and this was the last one looked at.
    Advanced {
        done: usize,
        total: usize,
        title: String,
        bpm: Option<f32>,
        key: Option<String>,
    },
    Done { analyzed: usize, without_pulse: usize, failed: usize },
    Failed(String),
}

/// An analysis running in the background.
pub struct AnalysisJob {
    events: Receiver<AnalysisProgress>,
    cancel: Arc<std::sync::atomic::AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl AnalysisJob {
    /// Starts the analysis of whatever is missing in the database at `db_path`.
    pub fn start(db_path: PathBuf) -> Self {
        let (tx, rx) = channel();
        let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        let thread = std::thread::Builder::new()
            .name("chilpa-analysis".into())
            .spawn(move || {
                let lib = match Library::open(&db_path) {
                    Ok(l) => l,
                    Err(e) => {
                        let _ = tx.send(AnalysisProgress::Failed(e.to_string()));
                        return;
                    }
                };
                let analyzer = chilpa_analysis::NAME;
                let version = chilpa_analysis::VERSION;

                let total = match lib.tracks_pending_analysis(analyzer, version, usize::MAX) {
                    Ok(v) => v.len(),
                    Err(e) => {
                        let _ = tx.send(AnalysisProgress::Failed(e.to_string()));
                        return;
                    }
                };
                let _ = tx.send(AnalysisProgress::Counted { total });

                let (mut made, mut no_pulse, mut failed) = (0usize, 0usize, 0usize);
                loop {
                    if flag.load(std::sync::atomic::Ordering::Relaxed) {
                        break;
                    }
                    let batch = match lib.tracks_pending_analysis(analyzer, version, BATCH) {
                        Ok(v) if v.is_empty() => break,
                        Ok(v) => v,
                        Err(e) => {
                            let _ = tx.send(AnalysisProgress::Failed(e.to_string()));
                            return;
                        }
                    };
                    for track in batch {
                        if flag.load(std::sync::atomic::Ordering::Relaxed) {
                            break;
                        }
                        let features = analyze_file(Path::new(&track.external_id));
                        // Even with no pulse found, the row is saved: otherwise
                        // the track would come out pending forever and the
                        // analysis would never end.
                        let saved = lib.save_features(
                            track.id,
                            analyzer,
                            version,
                            features.as_ref().and_then(|f| f.bpm),
                            features.as_ref().and_then(|f| f.tempo_confidence),
                            features
                                .as_ref()
                                .and_then(|f| f.key.as_ref())
                                .map(|k| format!("{} · {}", k.name, k.camelot)),
                            features.as_ref().and_then(|f| f.loudness_lufs),
                        );
                        match (&features, &saved) {
                            (_, Err(e)) => {
                                let _ = tx.send(AnalysisProgress::Failed(e.to_string()));
                                return;
                            }
                            (None, _) => failed += 1,
                            (Some(f), _) if f.bpm.is_none() => no_pulse += 1,
                            _ => {}
                        }
                        made += 1;
                        let _ = tx.send(AnalysisProgress::Advanced {
                            done: made,
                            total,
                            title: track.title.clone(),
                            bpm: features.as_ref().and_then(|f| f.bpm),
                            key: features
                                .as_ref()
                                .and_then(|f| f.key.as_ref())
                                .map(|k| k.camelot.clone()),
                        });
                    }
                }
                let _ = tx.send(AnalysisProgress::Done {
                    analyzed: made,
                    without_pulse: no_pulse,
                    failed: failed,
                });
            })
            .expect("the analysis thread starts");
        Self { events: rx, cancel, thread: Some(thread) }
    }

    /// Whatever has happened, without blocking.
    pub fn poll(&self) -> Vec<AnalysisProgress> {
        let mut out = Vec::new();
        loop {
            match self.events.try_recv() {
                Ok(e) => out.push(e),
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            }
        }
        out
    }

    /// Asks it to stop. The track in progress is finished.
    pub fn cancel(&self) {
        self.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn finished(&self) -> bool {
        self.thread.as_ref().is_some_and(|t| t.is_finished())
    }
}

impl Drop for AnalysisJob {
    fn drop(&mut self) {
        self.cancel();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Decodes a piece of the track and analyses it.
///
/// `None` when the file could not be opened or read. Finding no pulse is not
/// a failure: it is an answer.
pub fn analyze_file(path: &Path) -> Option<chilpa_analysis::Analysis> {
    let mut decoded = chilpa_decode::Decoded::open(path).ok()?;
    let format = decoded.stream_format();
    let channels = format.channels.max(1) as usize;
    let rate = format.sample_rate_hz;
    if rate == 0 {
        return None;
    }

    // It starts a quarter of the way in: intros say nothing about the
    // pulse, and the start of an album is usually the least representative.
    if let Some(total) = decoded.frame_count() {
        let startup = total / 4;
        if startup > u64::from(rate) {
            let _ = decoded.seek_to_frame(startup);
        }
    }

    let want = SECONDS_EXAMINED * u64::from(rate);
    let mut mono: Vec<f32> = Vec::with_capacity(want as usize / 4);
    // Loudness is measured on the fly, with the signal as it comes: mixing
    // it to mono or dropping its rate would change the number.
    let mut meter = chilpa_analysis::LoudnessMeter::new(rate, format.channels.max(1));
    let mut read_frames = 0u64;
    let mut interleaved: Vec<f32> = Vec::new();
    while read_frames < want {
        let block = match decoded.next_block() {
            Ok(Some(b)) => b,
            Ok(None) | Err(_) => break,
        };
        interleaved.clear();
        match block {
            chilpa_decode::Block::Int { data, bits } => {
                let scale = 1.0f32 / (1i64 << (bits.max(1) - 1)) as f32;
                interleaved.extend(data.iter().map(|m| *m as f32 * scale));
            }
            chilpa_decode::Block::Float(data) => interleaved.extend(data),
        }
        if interleaved.is_empty() {
            continue;
        }
        if let Some(m) = meter.as_mut() {
            m.push(&interleaved);
        }
        for frame in interleaved.chunks_exact(channels) {
            mono.push(frame.iter().sum::<f32>() / channels as f32);
        }
        read_frames += (interleaved.len() / channels) as u64;
    }
    if mono.is_empty() {
        return None;
    }
    let mut features = chilpa_analysis::analyze(&mono, rate);
    features.loudness_lufs = meter.and_then(|m| m.integrated()).map(|l| l as f32);
    Some(features)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wait(job: &AnalysisJob) -> Vec<AnalysisProgress> {
        let limit = std::time::Instant::now() + std::time::Duration::from_secs(20);
        let mut events = Vec::new();
        while std::time::Instant::now() < limit {
            events.extend(job.poll());
            if events
                .iter()
                .any(|e| matches!(e, AnalysisProgress::Done { .. } | AnalysisProgress::Failed(_)))
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        events
    }

    #[test]
    fn with_nothing_to_analyze_it_finishes_at_once() {
        let dir = std::env::temp_dir().join("chilpa-analysis-empty");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let job = AnalysisJob::start(dir.join("lib.db"));
        let events = wait(&job);
        match events.first() {
            Some(AnalysisProgress::Counted { total }) => assert_eq!(*total, 0),
            other => panic!("the count comes first: {other:?}"),
        }
        assert!(matches!(
            events.last(),
            Some(AnalysisProgress::Done { analyzed: 0, .. })
        ));
        drop(job);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_file_that_cannot_be_opened_does_not_bring_the_analysis_down() {
        assert!(analyze_file(Path::new("/does/not/exist.flac")).is_none());
    }
}
