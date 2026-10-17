//! Bounded disposable artwork and an asynchronous view of cache usage.
//! Library-referenced covers stay pinned until their library rows change.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

const DEFAULT_REMOTE_MB: u64 = 200;
// A library with thousands of records needs its scaled covers to survive
// ordinary scrolling. A 128 MiB ceiling evicted the warm grid cache.
const DERIVATIVE_BYTES: u64 = 512 * 1024 * 1024;
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
        let usage = scan_usage(&root, &chilpa_library::Library::default_path());
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
            trim_remote(&root, &chilpa_library::Library::default_path(), REMOTE_MB.load(Ordering::Relaxed) * 1024 * 1024);
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
                trim_remote(&root, &chilpa_library::Library::default_path(), 0);
            } else {
                prune(&derivative_dirs(&root), 0, &Default::default());
            }
        }
        request_scan();
    }).is_ok()
}

/// I read pins without running library migrations or whole-library release assembly.
fn pinned_artwork(database: &Path, remote: &Path) -> Option<std::collections::HashSet<PathBuf>> {
    chilpa_library::Library::open_read_only(database).ok()?.protected_cache_artwork(remote).ok()
}

fn scan_usage(root: &Path, database: &Path) -> Usage {
    let mut usage = Usage { ready: true, ..Usage::default() };
    let pinned = pinned_artwork(database, &root.join("artwork").join("remote")).unwrap_or_default();
    visit(root, &mut |path, bytes, _| {
        let relative = path.strip_prefix(root).unwrap_or(path);
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
    usage
}

fn trim_remote(root: &Path, database: &Path, limit: u64) {
    let remote = root.join("artwork").join("remote");
    // An unavailable or older library cannot supply its pins. I leave its
    // remote files alone until the primary connection initializes it.
    if let Some(pinned) = pinned_artwork(database, &remote) { prune(&[remote], limit, &pinned); }
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
    fn usage_and_trimming_read_pins_without_assembling_local_releases() {
        let dir = tempfile::tempdir().unwrap();
        let database = dir.path().join("library.db");
        let cache = dir.path().join("cache");
        let remote = cache.join("artwork").join("remote");
        std::fs::create_dir_all(&remote).unwrap();
        let pinned = remote.join("pinned.jpg");
        let disposable = remote.join("disposable.jpg");
        std::fs::write(&pinned, [1_u8; 10]).unwrap();
        std::fs::write(&disposable, [2_u8; 10]).unwrap();
        let library = chilpa_library::Library::open(&database).unwrap();
        let source = library.source_id("local", "default").unwrap();
        library.connection().execute("INSERT INTO albums(id, title, album_artist) VALUES (1, 'Record', 'Artist')", []).unwrap();
        library.connection().execute("INSERT INTO tracks(id, source_id, external_id, title, artist, album_credit, album_id, added_at)
            VALUES (1, ?1, ?2, 'Song', 'Artist', 'Artist', 1, 1)",
            rusqlite::params![source, dir.path().join("music").join("song.flac").display().to_string()]).unwrap();
        library.set_remote_artwork("fixture", "record", Some(pinned.to_str().unwrap())).unwrap();
        drop(library);
        let release_part = || {
            chilpa_library::Library::open_read_only(&database).unwrap().connection()
                .query_row("SELECT release_part FROM tracks WHERE id = 1", [], |row| row.get::<_, String>(0)).unwrap()
        };
        assert_eq!(release_part(), "", "the fixture needs primary library maintenance");
        let usage = scan_usage(&cache, &database);
        assert_eq!((usage.remote.files, usage.remote.bytes, usage.pinned.files, usage.pinned.bytes), (2, 20, 1, 10));
        assert_eq!(release_part(), "", "a usage scan must not assemble releases");
        trim_remote(&cache, &database, 0);
        assert!(pinned.exists());
        assert!(!disposable.exists());
        assert_eq!(release_part(), "", "cache trimming must not assemble releases");
        drop(chilpa_library::Library::open(&database).unwrap());
        assert!(!release_part().is_empty(), "the primary opener still assembles releases");
    }

    #[test]
    fn unavailable_or_older_libraries_never_authorize_remote_cache_deletion() {
        let dir = tempfile::tempdir().unwrap();
        let database = dir.path().join("library.db");
        let cache = dir.path().join("cache");
        let remote = cache.join("artwork").join("remote");
        std::fs::create_dir_all(&remote).unwrap();
        let art = remote.join("unknown.jpg");
        std::fs::write(&art, [1_u8; 10]).unwrap();
        trim_remote(&cache, &database, 0);
        assert!(art.exists());
        assert!(!database.exists(), "cache work must not initialize the primary library");
        let older = rusqlite::Connection::open(&database).unwrap();
        older.pragma_update(None, "user_version", 1).unwrap();
        drop(older);
        trim_remote(&cache, &database, 0);
        assert!(art.exists());
        let version: i64 = rusqlite::Connection::open(&database).unwrap()
            .query_row("PRAGMA user_version", [], |row| row.get(0)).unwrap();
        assert_eq!(version, 1, "cache work must not migrate an older library");
    }

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
