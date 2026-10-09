//! The seek bar's waveform as the facade runs it: a pass over the track
//! in a thread of its own when it starts, the finished document kept on
//! disk by what identifies the track, and the columns the view asks for.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub use chilpa_core::seek_waveform::{Snapshot, BINS};
use chilpa_core::seek_waveform::{analyze_file, columns};
use chilpa_audio_feed::{Accumulator, Feed};

mod chilpa_audio_feed {
    pub use chilpa_core::seek_waveform::{Accumulator, Feed};
}

/// Whether the player bar stands tall with the waveform.
pub const OPEN: &str = "playback.waveform";

/// The finished documents, one file of 512 bytes per track.
struct Store {
    dir: PathBuf,
}

impl Store {
    fn new() -> Self {
        Self { dir: chilpa_dirs::cache().join("waveforms") }
    }

    /// What identifies a track's content: a file by its size, its time and
    /// the hash of its head, so a moved file keeps its document; a source's
    /// track by its source and id.
    fn key(path: &str) -> Option<String> {
        if let Some((instance, id)) = crate::source_opener::of_plugin(path) {
            return Some(format!("source-{:016x}", fnv(format!("{instance}:{id}").as_bytes())));
        }
        // A track of an SACD image: the image's content and the track's
        // number.
        if let Some((image, number)) = chilpa_core::sacd_tracks::parse(Path::new(path)) {
            return Self::key(&image.to_string_lossy()).map(|k| format!("{k}-t{number}"));
        }
        let file = Path::new(path);
        let meta = std::fs::metadata(file).ok()?;
        if !meta.is_file() {
            return None;
        }
        let modified = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
        let mut head = vec![0u8; 64 * 1024];
        let read = std::fs::File::open(file).ok().and_then(|mut f| std::io::Read::read(&mut f, &mut head).ok())?;
        head.truncate(read);
        Some(format!("local-{:016x}-{}-{}", fnv(&head), meta.len(), modified))
    }

    /// The two swings, highs then lows; a file of another size is from an
    /// earlier shape of the document and is read again.
    fn get(&self, key: &str) -> Option<([u8; BINS], [u8; BINS])> {
        let bytes = std::fs::read(self.dir.join(key)).ok()?;
        if bytes.len() != BINS * 2 {
            return None;
        }
        let (highs, lows) = bytes.split_at(BINS);
        Some((highs.try_into().ok()?, lows.try_into().ok()?))
    }

    fn set(&self, key: &str, highs: &[u8; BINS], lows: &[u8; BINS]) {
        if std::fs::create_dir_all(&self.dir).is_err() {
            return;
        }
        let mut bytes = Vec::with_capacity(BINS * 2);
        bytes.extend_from_slice(highs);
        bytes.extend_from_slice(lows);
        let _ = std::fs::write(self.dir.join(key), bytes);
    }
}

/// FNV-1a, 64 bits: a compact fingerprint without a new dependency.
fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325u64, |hash, byte| (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3))
}

/// Whether a track can be read apart from playing it: a file on disk, or
/// a track of an SACD image on disk.
fn readable_apart(path: &Path) -> bool {
    path.is_file() || chilpa_core::sacd_tracks::parse(path).is_some()
}

struct Job {
    stop: Arc<AtomicBool>,
}

impl Drop for Job {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

pub struct SeekWaveform {
    store: Arc<Store>,
    /// What the player hands over as it decodes: the document of a track
    /// that cannot be read apart from playing it.
    live: Arc<Live>,
    /// Counts the tracks followed, so a revision never repeats across two
    /// documents and a reader comparing revisions always sees the change.
    epoch: u64,
    /// Whose document this is: the path that started it.
    path: String,
    shared: Arc<Mutex<Snapshot>>,
    job: Option<Job>,
}

impl Default for SeekWaveform {
    fn default() -> Self {
        let (store, shared) = (Arc::new(Store::new()), Arc::new(Mutex::new(Snapshot::default())));
        let live = Arc::new(Live { store: Arc::clone(&store), shared: Arc::clone(&shared), growing: Mutex::new(None) });
        Self { store, live, epoch: 0, path: String::new(), shared, job: None }
    }
}

impl SeekWaveform {
    /// What plays now, every poll: a new track starts its pass, unless its
    /// document is on disk already; nothing playing drops the document.
    pub fn follow(&mut self, path: &str) {
        if path == self.path {
            return;
        }
        self.job = None;
        self.epoch += 1;
        self.path = path.to_owned();
        *self.shared.lock().unwrap_or_else(|e| e.into_inner()) = Snapshot::default();
        if path.is_empty() {
            return;
        }
        let key = Store::key(path);
        if let Some((highs, lows)) = key.as_deref().and_then(|k| self.store.get(k)) {
            *self.shared.lock().unwrap_or_else(|e| e.into_inner()) =
                Snapshot { revision: 1, highs, lows, analyzed: BINS as u16, complete: true };
            return;
        }
        if !readable_apart(Path::new(path)) {
            // A stream is read as it plays, by the player's feed, and kept
            // for the next time (the owner, 2026-10-09).
            return;
        }
        let stop = Arc::new(AtomicBool::new(false));
        let (file, shared, store, flag) = (PathBuf::from(path), Arc::clone(&self.shared), Arc::clone(&self.store), Arc::clone(&stop));
        let spawned = std::thread::Builder::new().name("seek-waveform".into()).spawn(move || {
            let outcome = analyze_file(&file, &flag, &mut |snapshot| {
                if snapshot.complete {
                    if let Some(k) = key.as_deref() {
                        store.set(k, &snapshot.highs, &snapshot.lows);
                    }
                }
                if !flag.load(Ordering::Relaxed) {
                    *shared.lock().unwrap_or_else(|e| e.into_inner()) = snapshot.clone();
                }
            });
            if let Err(why) = outcome {
                chilpa_core::trace_playback("waveform", "pass_failed", format_args!("{why}"));
            }
        });
        if spawned.is_ok() {
            self.job = Some(Job { stop });
        }
    }

    /// Where the player hands its frames.
    pub fn feed(&self) -> Arc<dyn Feed> {
        Arc::clone(&self.live) as Arc<dyn Feed>
    }

    pub fn snapshot(&self) -> Snapshot {
        let mut snapshot = self.shared.lock().unwrap_or_else(|e| e.into_inner()).clone();
        snapshot.revision |= self.epoch << 32;
        snapshot
    }

    /// The document reduced to `count` columns for the view, the upward
    /// swings and the downward ones.
    pub fn columns(&self, count: usize) -> (Vec<f32>, Vec<f32>) {
        let snapshot = self.snapshot();
        if snapshot.analyzed == 0 {
            return (Vec::new(), Vec::new());
        }
        columns(&snapshot.highs, &snapshot.lows, count)
    }
}

/// The document of what plays, gathered from the frames the player
/// decodes, for a track with no file to read apart from playing it. A
/// local file is read whole by its own pass instead, and a document kept
/// on disk is not gathered again.
struct Live {
    store: Arc<Store>,
    shared: Arc<Mutex<Snapshot>>,
    growing: Mutex<Option<Growing>>,
}

struct Growing {
    key: Option<String>,
    accumulator: Accumulator,
    revision: u64,
    last: std::time::Instant,
}

/// How often a growing document is handed to the view.
const PUBLISH_EVERY: std::time::Duration = std::time::Duration::from_millis(100);

impl Live {
    fn publish(&self, growing: &mut Growing, force: bool) {
        if !force && growing.last.elapsed() < PUBLISH_EVERY {
            return;
        }
        growing.revision += 1;
        growing.last = std::time::Instant::now();
        let snapshot = growing.accumulator.snapshot(growing.revision);
        if snapshot.complete {
            if let Some(key) = growing.key.as_deref() {
                self.store.set(key, &snapshot.highs, &snapshot.lows);
            }
        }
        *self.shared.lock().unwrap_or_else(|e| e.into_inner()) = snapshot;
    }

    fn with_growing(&self, run: impl FnOnce(&Self, &mut Growing)) {
        let mut slot = self.growing.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(growing) = slot.as_mut() {
            if !growing.accumulator.complete() {
                run(self, growing);
            }
        }
    }
}

impl Feed for Live {
    fn begin(&self, path: &Path, channels: u16, total_frames: Option<u64>) {
        let mut slot = self.growing.lock().unwrap_or_else(|e| e.into_inner());
        *slot = None;
        let Some(total) = total_frames else { return };
        if readable_apart(path) {
            return;
        }
        let key = Store::key(&path.to_string_lossy());
        if key.as_deref().is_some_and(|k| self.store.get(k).is_some()) {
            return;
        }
        *slot = Some(Growing { key, accumulator: Accumulator::new(channels, total), revision: 0, last: std::time::Instant::now() });
    }

    fn feed_float(&self, start_frame: u64, samples: &[f32]) {
        self.with_growing(|live, growing| {
            growing.accumulator.feed_float(start_frame, samples);
            live.publish(growing, growing.accumulator.complete());
        });
    }

    fn feed_int(&self, start_frame: u64, samples: &[i32], bits: u8) {
        self.with_growing(|live, growing| {
            growing.accumulator.feed_int(start_frame, samples, bits);
            live.publish(growing, growing.accumulator.complete());
        });
    }

    fn end(&self) {
        let mut slot = self.growing.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(mut growing) = slot.take() {
            if !growing.accumulator.complete() {
                self.publish(&mut growing, true);
            }
        }
    }
}

impl crate::Chilpa {
    /// Every poll: nothing playing parks the visualizer, and the waveform
    /// follows what plays, paused or not.
    pub(crate) fn follow_playing_documents(&mut self) {
        self.visualizer.set_paused(!self.now.playing || self.now.paused);
        let path = if self.now.playing { self.now.path.clone() } else { String::new() };
        self.seek_waveform.follow(&path);
    }

    pub fn seek_waveform_open(&self) -> bool {
        self.preference(OPEN).as_deref() == Some("true")
    }

    pub fn remember_seek_waveform_open(&mut self, open: bool) {
        self.remember_preference(OPEN, &open.to_string());
    }

    pub fn seek_waveform_snapshot(&self) -> Snapshot {
        self.seek_waveform.snapshot()
    }

    pub fn seek_waveform_columns(&self, count: usize) -> (Vec<f32>, Vec<f32>) {
        self.seek_waveform.columns(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_follows_the_content_and_not_the_path() {
        let dir = std::env::temp_dir().join(format!("chilpa-waveform-key-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let a = dir.join("a.wav");
        let b = dir.join("b.wav");
        std::fs::write(&a, b"RIFF same bytes").unwrap();
        std::fs::copy(&a, &b).unwrap();
        let ka = Store::key(a.to_str().unwrap()).unwrap();
        let kb = Store::key(b.to_str().unwrap()).unwrap();
        assert_eq!(&ka[..ka.rfind('-').unwrap()], &kb[..kb.rfind('-').unwrap()], "same head and size: {ka} {kb}");
        assert!(ka.starts_with("local-"));
        assert!(Store::key(dir.join("none.wav").to_str().unwrap()).is_none());
    }

    #[test]
    fn a_document_on_disk_comes_back_whole() {
        let dir = std::env::temp_dir().join(format!("chilpa-waveform-store-{}", std::process::id()));
        let store = Store { dir };
        let mut highs = [0u8; BINS];
        let mut lows = [0u8; BINS];
        highs[7] = 200;
        lows[9] = 90;
        store.set("local-test", &highs, &lows);
        assert_eq!(store.get("local-test"), Some((highs, lows)));
        assert_eq!(store.get("local-missing"), None);
    }

    #[test]
    fn nothing_playing_leaves_no_document_and_no_columns() {
        let mut waveform = SeekWaveform::default();
        waveform.follow("");
        assert_eq!(waveform.snapshot().analyzed, 0);
        assert!(waveform.columns(64).0.is_empty());
    }
}
