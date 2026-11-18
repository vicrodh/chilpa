//! The registry file, read and written off the caller's thread. One writer,
//! one version waiting at most; a newer one replaces it.
use super::document::{self, Stored, Unusable, MAX_BYTES};
use std::{io::{Read, Write}, path::{Path, PathBuf}, sync::{atomic::{AtomicU8, Ordering}, Arc, Condvar, Mutex}};

pub(super) const SAVED: u8 = 1;
pub(super) const FAILED: u8 = 2;
pub(super) const TOO_LARGE: u8 = 3;

/// A missing file is an empty registry; anything else unreadable is kept.
pub(super) fn load(path: &Path) -> Result<Vec<Stored>, Unusable> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err(Unusable::Unreadable),
    };
    let mut bytes = Vec::new();
    file.take(MAX_BYTES as u64 + 1).read_to_end(&mut bytes).map_err(|_| Unusable::Unreadable)?;
    document::parse(&bytes)
}

/// Where a damaged file goes when a new list replaces it. One name: a
/// second damaged file takes the place of the first.
pub fn set_aside_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".invalid");
    path.with_file_name(name)
}

fn write(path: &Path, stored: &[Stored], set_aside: bool) -> u8 {
    let Some(bytes) = document::render(stored) else { return TOO_LARGE };
    let Some(directory) = path.parent() else { return FAILED };
    let done = (|| {
        std::fs::create_dir_all(directory)?;
        if set_aside && path.exists() { std::fs::rename(path, set_aside_path(path))?; }
        let mut temp = tempfile::NamedTempFile::new_in(directory)?;
        temp.write_all(&bytes)?;
        temp.as_file().sync_all()?;
        temp.persist(path).map_err(|e| e.error)?;
        Ok::<_, std::io::Error>(())
    })();
    if done.is_ok() { SAVED } else { FAILED }
}

#[derive(Default)]
struct Slot { waiting: Option<(Vec<Stored>, bool)>, writing: bool }
pub(super) struct Writer { path: PathBuf, slot: Arc<(Mutex<Slot>, Condvar)>, outcome: Arc<AtomicU8> }
impl Writer {
    pub(super) fn new(path: PathBuf) -> Self { Self { path, slot: Default::default(), outcome: Default::default() } }
    /// Leaves the list to be written. The thread exists only while there is
    /// something to write.
    pub(super) fn save(&self, stored: Vec<Stored>, set_aside: bool) {
        let mut slot = self.slot.0.lock().unwrap_or_else(|e| e.into_inner());
        let aside = set_aside || slot.waiting.as_ref().is_some_and(|w| w.1);
        slot.waiting = Some((stored, aside));
        if slot.writing { return; }
        slot.writing = true;
        let (path, shared, outcome) = (self.path.clone(), self.slot.clone(), self.outcome.clone());
        let spawned = std::thread::Builder::new().name("chilpa-endpoint-store".into()).spawn(move || loop {
            let next = {
                let mut slot = shared.0.lock().unwrap_or_else(|e| e.into_inner());
                let next = slot.waiting.take();
                if next.is_none() { slot.writing = false; shared.1.notify_all(); }
                next
            };
            let Some((stored, aside)) = next else { break };
            outcome.store(write(&path, &stored, aside), Ordering::Release);
        });
        if spawned.is_err() { slot.writing = false; slot.waiting = None; self.outcome.store(FAILED, Ordering::Release); }
    }
    /// The result of the last finished write, read once.
    pub(super) fn outcome(&self) -> u8 { self.outcome.swap(0, Ordering::AcqRel) }
}
impl Drop for Writer {
    /// A list left to be written is on disk before the process goes on.
    fn drop(&mut self) {
        let mut slot = self.slot.0.lock().unwrap_or_else(|e| e.into_inner());
        while slot.writing { slot = self.slot.1.wait(slot).unwrap_or_else(|e| e.into_inner()); }
    }
}
