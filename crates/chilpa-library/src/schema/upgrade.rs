//! One upgrade owns both the backup and migration. The process-local gate and
//! OS advisory lock cover the same canonical library path. The zero-byte lock
//! file stays in place; closing its handle releases ownership, including on
//! process exit. Readers and current-schema opens never acquire the file lock.
//! One private staging directory lives beside the destination until publication.
use super::{backup_path, Connection, Path, VERSION};
use std::fs::{File, OpenOptions, TryLockError};
use std::time::{Duration, Instant};

pub(super) fn run(db: &Connection, library: &Path) -> rusqlite::Result<()> {
    let actual: i64 = db.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if actual >= VERSION { return super::migrate(db); }
    let gate = super::migration_gate(db);
    let _process = gate.as_ref().map(|gate| gate.lock().unwrap_or_else(|error| error.into_inner()));
    let actual: i64 = db.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if actual >= VERSION { return super::migrate_locked(db); }
    let _file = if db.path().is_some_and(|path| !path.is_empty()) {
        Some(lock(library)?)
    } else { None };
    // Another process may have completed the upgrade while this one waited.
    // Both helpers reread the version while the advisory lock is held.
    backup(db, library)?;
    super::migrate_locked(db)
}

pub(super) fn lock(library: &Path) -> rusqlite::Result<File> {
    let path = std::fs::canonicalize(library).map_err(|error| io_error(library, error))?;
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".upgrade-lock");
    let path = path.with_file_name(name);
    let file = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(&path)
        .map_err(|error| io_error(&path, error))?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(TryLockError::WouldBlock) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            Err(TryLockError::WouldBlock) => return Err(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY),
                Some(chilpa_i18n::t("Another Chilpa process is updating this library. Try opening it again shortly.")))),
            Err(TryLockError::Error(error)) => return Err(io_error(&path, error)),
        }
    }
}

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
    let staging = tempfile::Builder::new().prefix(".library-upgrade-").tempdir_in(parent)
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
