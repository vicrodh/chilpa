//! One upgrade owns the backup, the steps and what is said about them. The
//! process-local gate and OS advisory lock cover the same canonical library
//! path. The zero-byte lock file stays in place; closing its handle releases
//! ownership, including on process exit. Readers and current-schema opens
//! never acquire the file lock. One private staging directory lives beside
//! the destination until publication.
use super::{backup_path, Connection, Path, VERSION};
use std::fs::{File, OpenOptions, TryLockError};
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// What an upgrade is doing right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Another process holds this library's upgrade lock.
    WaitingForOther,
    /// The copy from before the upgrade is being written.
    BackingUp,
    /// A step is changing the file.
    Step,
    /// The steps are done and the library opens once, so its repairs run
    /// before anything else reads it.
    Finishing,
}

/// How far an upgrade is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Progress {
    pub phase: Phase,
    /// The step under way, counted from 1 among the steps this upgrade
    /// applies, and how many it applies.
    pub step: usize,
    pub steps: usize,
    /// The version the step under way leads to.
    pub version: i64,
    /// Done units over all units, from 0 to 1. The copy, each step and the
    /// finishing open are one unit each.
    pub fraction: f64,
}

/// A library file older than this code. Nothing may read it before [`run`]
/// brings it up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pending {
    pub from: i64,
    pub to: i64,
}

/// Asks an upgrade under way to stop. It stops between parts, and a
/// statement already running is interrupted; either way the transaction of
/// the step under way rolls back and the library stays at the last version
/// it completed.
#[derive(Default)]
pub struct Stop {
    asked: AtomicBool,
    running: Mutex<Option<rusqlite::InterruptHandle>>,
}

impl Stop {
    pub fn request(&self) {
        self.asked.store(true, Ordering::SeqCst);
        if let Some(handle) = self.running.lock().unwrap_or_else(|error| error.into_inner()).as_ref() {
            handle.interrupt();
        }
    }

    fn watch(&self, db: &Connection) {
        *self.running.lock().unwrap_or_else(|error| error.into_inner()) = Some(db.get_interrupt_handle());
    }

    pub(super) fn check(&self) -> rusqlite::Result<()> {
        if !self.asked.load(Ordering::SeqCst) { return Ok(()); }
        Err(rusqlite::Error::SqliteFailure(rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_INTERRUPT), None))
    }
}

/// Whether the library at `library` has to be upgraded: one read-only open
/// and its version. A missing file, a current one and a newer one are not
/// pending; neither is a file that cannot be read, which its opener reports.
pub fn pending(library: &Path) -> Option<Pending> {
    if !library.is_file() { return None; }
    let db = Connection::open_with_flags(library, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    db.busy_timeout(Duration::from_secs(5)).ok()?;
    let from: i64 = db.query_row("PRAGMA user_version", [], |row| row.get(0)).ok()?;
    (from > 0 && from < VERSION).then_some(Pending { from, to: VERSION })
}

/// Brings the library at `library` up to [`VERSION`]: the copy first, then
/// each step in its own transaction, then one open so the library's repairs
/// run before anything else reads it. `observer` hears each part as it
/// starts, and the last thing it hears is `Finishing` at 1. Another process
/// upgrading the same file is waited for, however long it takes, unless
/// `stop` is requested.
pub fn run(library: &Path, observer: &mut dyn FnMut(&Progress), stop: &Stop) -> rusqlite::Result<()> {
    if let Some(dir) = library.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let steps = {
        let db = Connection::open(library)?;
        stop.watch(&db);
        crate::store::query::register(&db)?;
        super::apply_pragmas(&db)?;
        apply(&db, library, observer, None, stop)?
    };
    stop.check()?;
    drop(crate::Library::open(library)?);
    observer(&Progress { phase: Phase::Finishing, step: steps, steps, version: VERSION, fraction: 1.0 });
    Ok(())
}

/// The steps this connection's file still needs, under both locks, telling
/// `observer`. `patience` bounds the wait for another process; `None` waits
/// for as long as it takes. Returns how many steps it applied.
pub(super) fn apply(db: &Connection, library: &Path, observer: &mut dyn FnMut(&Progress), patience: Option<Duration>, stop: &Stop) -> rusqlite::Result<usize> {
    if version(db)? >= VERSION { super::migrate(db)?; return Ok(0); }
    let gate = super::migration_gate(db);
    let _process = gate.as_ref().map(|gate| gate.lock().unwrap_or_else(|error| error.into_inner()));
    if version(db)? >= VERSION { super::migrate_locked(db)?; return Ok(0); }
    let on_disk = db.path().is_some_and(|path| !path.is_empty());
    let _file = if on_disk { Some(lock_reporting(library, observer, patience, stop)?) } else { None };
    // Another process may have completed the upgrade while this one waited.
    let from = version(db)?;
    if from >= VERSION {
        super::migrate_locked(db)?;
        if on_disk { unmark(library)?; }
        return Ok(0);
    }
    let copy = on_disk && from > 0 && !copy_at_hand(library, from);
    if copy { clear_staging(library); }
    let steps = (VERSION - from) as usize;
    let total = usize::from(copy) + steps + 1;
    let mut tell = |phase, step, version, done: usize| {
        observer(&Progress { phase, step, steps, version, fraction: done as f64 / total as f64 });
    };
    if copy {
        tell(Phase::BackingUp, 0, from, 0);
        stop.check()?;
        backup(db, library)?;
        mark(library, from)?;
    }
    let before = usize::from(copy);
    super::steps_locked(db, &mut |target| {
        let step = (target - from) as usize;
        tell(Phase::Step, step, target, before + step - 1);
        stop.check()
    })?;
    if on_disk {
        // The log grew by about what the steps rewrote; it goes back to
        // nothing before anything else opens the file.
        let _ = db.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()));
        unmark(library)?;
    }
    tell(Phase::Finishing, steps, VERSION, total - 1);
    Ok(steps)
}

fn version(db: &Connection) -> rusqlite::Result<i64> {
    db.query_row("PRAGMA user_version", [], |row| row.get(0))
}

pub(super) fn lock(library: &Path) -> rusqlite::Result<File> {
    lock_reporting(library, &mut |_| {}, Some(Duration::from_secs(5)), &Stop::default())
}

fn lock_reporting(library: &Path, observer: &mut dyn FnMut(&Progress), patience: Option<Duration>, stop: &Stop) -> rusqlite::Result<File> {
    let path = std::fs::canonicalize(library).map_err(|error| io_error(library, error))?;
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".upgrade-lock");
    let path = path.with_file_name(name);
    let file = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(&path)
        .map_err(|error| io_error(&path, error))?;
    let deadline = patience.map(|patience| Instant::now() + patience);
    let mut told = false;
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(TryLockError::WouldBlock) if deadline.is_none_or(|deadline| Instant::now() < deadline) => {
                if !told {
                    observer(&Progress { phase: Phase::WaitingForOther, step: 0, steps: 0, version: 0, fraction: 0.0 });
                    told = true;
                }
                stop.check()?;
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(TryLockError::WouldBlock) => return Err(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY),
                Some(chilpa_i18n::t("Another Chilpa process is updating this library. Try opening it again shortly.")))),
            Err(TryLockError::Error(error)) => return Err(io_error(&path, error)),
        }
    }
}

/// Written beside the library once its copy is published, holding the
/// version the copy was taken from and the one the upgrade leads to, and
/// removed when the upgrade completes. A later attempt of that same upgrade
/// that finds it, and a copy at that version, keeps that copy: replacing it
/// would put a half-upgraded library where the one from before the upgrade
/// was. One left behind by an upgrade that did complete (the process died
/// between its last step and the removal) names a target the library has
/// already reached, and is ignored.
pub fn marker_path(library: &Path) -> PathBuf {
    let mut name = library.file_name().unwrap_or_default().to_os_string();
    name.push(".upgrade-pending");
    library.with_file_name(name)
}

/// The version of the copy an unfinished upgrade took, and the version that
/// upgrade leads to, when the copy beside the library is still that one.
pub fn unfinished_copy(library: &Path) -> Option<(i64, i64)> {
    let marked = std::fs::read_to_string(marker_path(library)).ok()?;
    let mut numbers = marked.split_whitespace().map(str::parse::<i64>);
    let (Some(Ok(copied)), Some(Ok(target))) = (numbers.next(), numbers.next()) else { return None };
    let copy = Connection::open_with_flags(backup_path(library), rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    let version: i64 = copy.query_row("PRAGMA user_version", [], |row| row.get(0)).ok()?;
    (version == copied).then_some((copied, target))
}

fn copy_at_hand(library: &Path, from: i64) -> bool {
    unfinished_copy(library).is_some_and(|(copied, target)| copied <= from && from < target)
}

fn mark(library: &Path, version: i64) -> rusqlite::Result<()> {
    let path = marker_path(library);
    let mut file = File::create(&path).map_err(|error| io_error(&path, error))?;
    file.write_all(format!("{version} {VERSION}").as_bytes()).and_then(|()| file.sync_all())
        .map_err(|error| io_error(&path, error))
}

fn unmark(library: &Path) -> rusqlite::Result<()> {
    let path = marker_path(library);
    match std::fs::remove_file(&path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(io_error(&path, error)),
        _ => Ok(()),
    }
}

/// A copy killed halfway leaves its private folder behind; nothing else can
/// be writing one while this process holds the lock.
fn clear_staging(library: &Path) {
    let destination = backup_path(library);
    let parent = destination.parent().filter(|path| !path.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
    let Ok(entries) = std::fs::read_dir(parent) else { return };
    for entry in entries.flatten() {
        if entry.file_name().to_string_lossy().starts_with(STAGING) && entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

const STAGING: &str = ".library-upgrade-";

fn context(path: &Path, error: &str) -> String {
    chilpa_i18n::t_args("The library update stopped while preparing its backup at {}: {}", &[&path.display().to_string(), error])
}

fn io_error(path: &Path, error: std::io::Error) -> rusqlite::Error {
    rusqlite::Error::SqliteFailure(rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_IOERR), Some(context(path, &error.to_string())))
}

pub(super) fn backup_error(destination: &Path, error: rusqlite::Error) -> rusqlite::Error {
    let message = context(destination, &error.to_string());
    let code = match error {
        rusqlite::Error::SqliteFailure(code, _) => code,
        _ => rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_ERROR),
    };
    rusqlite::Error::SqliteFailure(code, Some(message))
}

pub(super) fn backup(db: &Connection, library: &Path) -> rusqlite::Result<()> {
    let actual: i64 = db.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if actual == 0 || actual >= VERSION { return Ok(()); }
    let destination = backup_path(library);
    let parent = destination.parent().filter(|path| !path.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
    let staging = tempfile::Builder::new().prefix(STAGING).tempdir_in(parent)
        .map_err(|error| io_error(&destination, error))?;
    // Some platform SQLite builds reject even an existing empty output file.
    // Reserve a private sibling directory and let SQLite create its child.
    let reserved = tempfile::NamedTempFile::new_in(staging.path())
        .map_err(|error| io_error(&destination, error))?;
    let permissions = reserved.as_file().metadata().map_err(|error| io_error(&destination, error))?.permissions();
    let temporary = reserved.path().to_owned();
    reserved.close().map_err(|error| io_error(&destination, error))?;
    db.execute("VACUUM INTO ?1", [&temporary.to_string_lossy().as_ref()])
        .map_err(|error| backup_error(&destination, error))?;
    let copied = OpenOptions::new().read(true).write(true).open(&temporary)
        .map_err(|error| io_error(&destination, error))?;
    // Restore the private temporary-file permissions before publication.
    copied.set_permissions(permissions).map_err(|error| io_error(&destination, error))?;
    copied.sync_all().map_err(|error| io_error(&destination, error))?;
    drop(copied);
    std::fs::rename(&temporary, &destination).map_err(|error| io_error(&destination, error))?;
    Ok(())
}
