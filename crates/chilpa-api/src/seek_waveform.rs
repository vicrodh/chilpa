//! The seek bar's waveform as the facade runs it: a pass over the track
//! in a thread of its own when it starts, the finished document kept on
//! disk by what identifies the track, and the columns the view asks for.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub use chilpa_core::seek_waveform::{Snapshot, BINS};
use chilpa_core::seek_waveform::{analyze_file, columns};

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

    fn get(&self, key: &str) -> Option<[u8; BINS]> {
        let bytes = std::fs::read(self.dir.join(key)).ok()?;
        bytes.try_into().ok()
    }

    fn set(&self, key: &str, bins: &[u8; BINS]) {
        if std::fs::create_dir_all(&self.dir).is_err() {
            return;
        }
        let _ = std::fs::write(self.dir.join(key), bins);
    }
}

/// FNV-1a, 64 bits: a compact fingerprint without a new dependency.
fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325u64, |hash, byte| (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3))
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
        Self { store: Arc::new(Store::new()), epoch: 0, path: String::new(), shared: Arc::new(Mutex::new(Snapshot::default())), job: None }
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
        if let Some(bins) = key.as_deref().and_then(|k| self.store.get(k)) {
            *self.shared.lock().unwrap_or_else(|e| e.into_inner()) =
                Snapshot { revision: 1, bins, analyzed: BINS as u16, complete: true };
            return;
        }
        if !Path::new(path).is_file() {
            // A stream has no second reader yet: the owner decides how it
            // gets one (contract, Q1).
            return;
        }
        let stop = Arc::new(AtomicBool::new(false));
        let (file, shared, store, flag) = (PathBuf::from(path), Arc::clone(&self.shared), Arc::clone(&self.store), Arc::clone(&stop));
        let spawned = std::thread::Builder::new().name("seek-waveform".into()).spawn(move || {
            let outcome = analyze_file(&file, &flag, &mut |snapshot| {
                if snapshot.complete {
                    if let Some(k) = key.as_deref() {
                        store.set(k, &snapshot.bins);
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

    pub fn snapshot(&self) -> Snapshot {
        let mut snapshot = self.shared.lock().unwrap_or_else(|e| e.into_inner()).clone();
        snapshot.revision |= self.epoch << 32;
        snapshot
    }

    /// The document reduced to `count` columns for the view.
    pub fn columns(&self, count: usize) -> Vec<f32> {
        let snapshot = self.snapshot();
        if snapshot.analyzed == 0 {
            return Vec::new();
        }
        columns(&snapshot.bins, count)
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

    pub fn seek_waveform_columns(&self, count: usize) -> Vec<f32> {
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
        let mut bins = [0u8; BINS];
        bins[7] = 200;
        store.set("local-test", &bins);
        assert_eq!(store.get("local-test"), Some(bins));
        assert_eq!(store.get("local-missing"), None);
    }

    #[test]
    fn nothing_playing_leaves_no_document_and_no_columns() {
        let mut waveform = SeekWaveform::default();
        waveform.follow("");
        assert_eq!(waveform.snapshot().analyzed, 0);
        assert!(waveform.columns(64).is_empty());
    }
}
