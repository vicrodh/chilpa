//! Bounded disposable artwork and an asynchronous view of cache usage.
//! Library-referenced covers stay pinned until their library rows change.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

const DEFAULT_REMOTE_MB: u64 = 200;
const DERIVATIVE_BYTES: u64 = 128 * 1024 * 1024;
const TRIM_AFTER_BYTES: u64 = 16 * 1024 * 1024;
static REMOTE_MB: AtomicU64 = AtomicU64::new(DEFAULT_REMOTE_MB);
static STORED: AtomicU64 = AtomicU64::new(0);
static WORKING: AtomicBool = AtomicBool::new(false);
static SCANNING: AtomicBool = AtomicBool::new(false);
static TRIM_AGAIN: AtomicBool = AtomicBool::new(false);
static SCAN_AGAIN: AtomicBool = AtomicBool::new(false);
static MAINTENANCE: Mutex<()> = Mutex::new(());
static USAGE: OnceLock<Mutex<Usage>> = OnceLock::new();

#[derive(Clone, Copy, Default)]
pub(crate) struct Count {
    pub files: u64,
    pub bytes: u64,
}

#[derive(Clone, Default)]
pub(crate) struct Usage {
    pub ready: bool,
    pub remote: Count,
    pub pinned: Count,
    pub embedded: Count,
    pub derivatives: Count,
    pub other: Count,
}

impl Usage {
    pub fn total(&self) -> u64 {
        self.remote.bytes + self.embedded.bytes + self.derivatives.bytes + self.other.bytes
    }
}

pub(crate) fn start(saved_mb: Option<&str>) {
    REMOTE_MB.store(parse_mb(saved_mb).unwrap_or(DEFAULT_REMOTE_MB), Ordering::Relaxed);
    trim_later();
    request_scan();
}

fn parse_mb(value: Option<&str>) -> Option<u64> {
    let value = value?.parse::<u64>().ok()?;
    [100, 200, 500, 1024].contains(&value).then_some(value)
}

pub(crate) fn set_limit(value: &str) -> bool {
    let Some(mb) = parse_mb(Some(value)) else { return false };
    REMOTE_MB.store(mb, Ordering::Relaxed);
    trim_later();
    true
}

pub(crate) fn limit_mb() -> u64 { REMOTE_MB.load(Ordering::Relaxed) }

pub(crate) fn note_stored(bytes: u64) {
    if STORED.fetch_add(bytes, Ordering::Relaxed).saturating_add(bytes) >= TRIM_AFTER_BYTES {
        STORED.store(0, Ordering::Relaxed);
        trim_later();
    }
}

pub(crate) fn usage() -> Usage {
    USAGE.get_or_init(|| Mutex::new(Usage::default())).lock().map(|state| state.clone()).unwrap_or_default()
}

pub(crate) fn request_scan() {
    if SCANNING.swap(true, Ordering::AcqRel) {
        SCAN_AGAIN.store(true, Ordering::Release);
        return
    }
    let spawned = std::thread::Builder::new().name("chilpa-cache-usage".into()).spawn(|| {
        let root = chilpa_dirs::cache();
        let mut usage = Usage { ready: true, ..Usage::default() };
        let remote = root.join("artwork").join("remote");
        let pinned = chilpa_core::open_library().ok()
            .and_then(|library| library.protected_cache_artwork(&remote).ok()).unwrap_or_default();
        visit(&root, &mut |path, bytes, _| {
            let relative = path.strip_prefix(&root).unwrap_or(path);
            let mut parts = relative.components();
            let first = parts.next().map(|part| part.as_os_str().to_string_lossy());
            let category = match first.as_deref() {
                Some("artwork") if parts.next().is_some_and(|part| part.as_os_str() == "remote") => &mut usage.remote,
                Some("artwork") => &mut usage.embedded,
                Some("covers" | "contained-covers" | "atmosphere") => &mut usage.derivatives,
                _ => &mut usage.other,
            };
            category.files += 1;
            category.bytes += bytes;
            if pinned.contains(path) {
                usage.pinned.files += 1;
                usage.pinned.bytes += bytes;
            }
        });
        if let Ok(mut state) = USAGE.get_or_init(|| Mutex::new(Usage::default())).lock() {
            *state = usage;
        }
        SCANNING.store(false, Ordering::Release);
        if SCAN_AGAIN.swap(false, Ordering::AcqRel) { request_scan(); }
    });
    if spawned.is_err() { SCANNING.store(false, Ordering::Release); }
}

pub(crate) fn trim_later() {
    if WORKING.swap(true, Ordering::AcqRel) {
        TRIM_AGAIN.store(true, Ordering::Release);
        return
    }
    let spawned = std::thread::Builder::new().name("chilpa-cache-trim".into()).spawn(|| {
        if let Ok(_guard) = MAINTENANCE.lock() {
            let root = chilpa_dirs::cache();
            if let Ok(library) = chilpa_core::open_library() {
                let remote = root.join("artwork").join("remote");
                if let Ok(pinned) = library.protected_cache_artwork(&remote) {
                    prune(&[remote], REMOTE_MB.load(Ordering::Relaxed) * 1024 * 1024, &pinned);
                }
            }
            prune(&derivative_dirs(&root), DERIVATIVE_BYTES, &Default::default());
        }
        WORKING.store(false, Ordering::Release);
        request_scan();
        if TRIM_AGAIN.swap(false, Ordering::AcqRel) { trim_later(); }
    });
    if spawned.is_err() { WORKING.store(false, Ordering::Release); }
}

pub(crate) fn clear(kind: &str) -> bool {
    if !matches!(kind, "remote" | "derivatives") { return false }
    let kind = kind.to_owned();
    std::thread::Builder::new().name("chilpa-cache-clear".into()).spawn(move || {
        if let Ok(_guard) = MAINTENANCE.lock() {
            let root = chilpa_dirs::cache();
            if kind == "remote" {
                let remote = root.join("artwork").join("remote");
                if let Ok(library) = chilpa_core::open_library() {
                    if let Ok(pinned) = library.protected_cache_artwork(&remote) {
                        prune(&[remote], 0, &pinned);
                    }
                }
            } else {
                prune(&derivative_dirs(&root), 0, &Default::default());
            }
        }
        request_scan();
    }).is_ok()
}

fn derivative_dirs(root: &Path) -> [PathBuf; 3] {
    [root.join("covers"), root.join("contained-covers"), root.join("atmosphere")]
}

fn visit(root: &Path, take: &mut impl FnMut(&Path, u64, SystemTime)) {
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = entry.metadata() else { continue };
            if entry.file_type().is_ok_and(|kind| kind.is_symlink()) { continue }
            if meta.is_dir() { pending.push(path) }
            else if meta.is_file() { take(&path, meta.len(), meta.accessed().or_else(|_| meta.modified()).unwrap_or(UNIX_EPOCH)) }
        }
    }
}

fn prune(roots: &[PathBuf], limit: u64, pinned: &std::collections::HashSet<PathBuf>) -> u64 {
    let mut files = Vec::new();
    let mut total = 0_u64;
    for root in roots {
        visit(root, &mut |path, bytes, modified| {
            if path.extension().is_some_and(|ext| matches!(ext.to_str(), Some("jpg" | "png" | "webp"))) {
                total = total.saturating_add(bytes);
                if !pinned.contains(path) { files.push((modified, bytes, path.to_path_buf())); }
            }
        });
    }
    if total <= limit { return 0 }
    files.sort_unstable_by_key(|(modified, _, _)| *modified);
    let mut freed = 0;
    for (_, bytes, path) in files {
        if total <= limit { break }
        if std::fs::remove_file(path).is_ok() {
            total = total.saturating_sub(bytes);
            freed += bytes;
        }
    }
    freed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pruning_keeps_pinned_files_and_removes_oldest_recoverable_files() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("old.jpg");
        let pinned = dir.path().join("pinned.jpg");
        let newest = dir.path().join("newest.jpg");
        std::fs::write(&old, [1_u8; 10]).unwrap();
        std::fs::write(&pinned, [2_u8; 10]).unwrap();
        std::fs::write(&newest, [3_u8; 10]).unwrap();
        let _ = std::fs::File::options().write(true).open(&old).unwrap().set_times(std::fs::FileTimes::new().set_accessed(UNIX_EPOCH));
        let mut keep = std::collections::HashSet::new();
        keep.insert(pinned.clone());
        assert_eq!(prune(&[dir.path().to_path_buf()], 20, &keep), 10);
        assert!(!old.exists());
        assert!(pinned.exists());
        assert!(newest.exists());
    }
}
