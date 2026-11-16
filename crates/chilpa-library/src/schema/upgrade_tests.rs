use super::*;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn fixture(path: &Path) {
    let db = Connection::open(path).unwrap();
    apply_pragmas(&db).unwrap();
    stop_at(&db, VERSION as usize - 1);
    db.execute_batch("INSERT INTO sources(id,provider,instance) VALUES(1,'local','default');
        INSERT INTO albums(id,title,album_artist) VALUES(1,'Record','Artist');
        INSERT INTO tracks(id,source_id,external_id,title,album_id,added_at) VALUES(1,1,'/fixture','Song',1,1);").unwrap();
}

fn verify(path: &Path) {
    let current = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let copied = Connection::open_with_flags(backup_path(path), rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(current.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0)).unwrap(), VERSION);
    assert_eq!(copied.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0)).unwrap(), VERSION - 1);
    for db in [current, copied] {
        assert_eq!(db.query_row("SELECT title FROM tracks WHERE id=1", [], |row| row.get::<_, String>(0)).unwrap(), "Song");
        assert_eq!(db.query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0)).unwrap(), "ok");
    }
}

#[test]
fn concurrent_library_opens_keep_one_pre_upgrade_backup() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("library.db");
    fixture(&path);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let workers: Vec<_> = (0..2).map(|_| {
        let (path, barrier) = (path.clone(), barrier.clone());
        std::thread::spawn(move || {
            barrier.wait();
            crate::Library::open(&path).unwrap();
        })
    }).collect();
    for worker in workers { worker.join().unwrap(); }
    verify(&path);
}

#[test]
fn child_opens_current_library() {
    let Some(path) = std::env::var_os("CHILPA_TEST_UPGRADE_LIBRARY") else { return };
    let ready = std::env::var_os("CHILPA_TEST_UPGRADE_READY").unwrap();
    std::fs::write(ready, b"ready").unwrap();
    crate::Library::open(Path::new(&path)).unwrap();
}

struct Children(Vec<Child>);
impl Drop for Children {
    fn drop(&mut self) {
        for child in &mut self.0 { let _ = child.kill(); let _ = child.wait(); }
    }
}

#[test]
fn separate_processes_wait_and_reread_schema_before_publishing_a_backup() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("library.db");
    fixture(&path);
    let held = upgrade::lock(&path).unwrap();
    let mut children = Children(Vec::new());
    let mut signals = Vec::new();
    for at in 0..2 {
        let ready = directory.path().join(format!("child-{at}.ready"));
        children.0.push(Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "schema::upgrade_tests::child_opens_current_library", "--test-threads=1"])
            .env("CHILPA_TEST_UPGRADE_LIBRARY", &path)
            .env("CHILPA_TEST_UPGRADE_READY", &ready)
            .stdin(Stdio::null()).stdout(Stdio::null()).spawn().unwrap());
        signals.push(ready);
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    while signals.iter().any(|path| !path.exists()) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(signals.iter().all(|path| path.exists()), "both fixture processes must start within the deadline");
    std::thread::sleep(Duration::from_millis(60));
    assert!(children.0.iter_mut().all(|child| child.try_wait().unwrap().is_none()), "the advisory lock holds both openers");
    // Complete the upgrade while both openers are waiting. Their initial
    // version reads must not cause a version-56 copy to replace this backup.
    let db = Connection::open(&path).unwrap();
    apply_pragmas(&db).unwrap();
    upgrade::backup(&db, &path).unwrap();
    migrate_locked(&db).unwrap();
    drop(db);
    let before = std::fs::read(backup_path(&path)).unwrap();
    drop(held);
    let deadline = Instant::now() + Duration::from_secs(10);
    for child in &mut children.0 {
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() { break status; }
            assert!(Instant::now() < deadline, "fixture opener exceeded its deadline");
            std::thread::sleep(Duration::from_millis(10));
        };
        assert!(status.success(), "fixture opener must finish successfully");
    }
    assert_eq!(std::fs::read(backup_path(&path)).unwrap(), before);
    verify(&path);
    assert!(!std::fs::read_dir(directory.path()).unwrap().any(|entry| entry.unwrap().file_name().to_string_lossy().starts_with(".library-upgrade-")));
}

#[test]
fn current_schema_readers_do_not_create_an_upgrade_lock() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("library.db");
    let db = Connection::open(&path).unwrap();
    apply_pragmas(&db).unwrap();
    stop_at(&db, VERSION as usize);
    drop(db);
    drop(crate::Library::open_read_only(&path).unwrap());
    drop(crate::Library::open(&path).unwrap());
    assert!(!directory.path().join("library.db.upgrade-lock").exists());
}

#[test]
fn backup_context_retains_extended_io_failure_codes() {
    let original = rusqlite::Error::SqliteFailure(rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_IOERR_FSYNC), Some("disk I/O error".into()));
    let error = upgrade::backup_error(Path::new("fixture.db.before-upgrade"), original);
    assert_eq!(error.sqlite_error().unwrap().extended_code, rusqlite::ffi::SQLITE_IOERR_FSYNC);
    assert!(error.to_string().contains("disk I/O error"));
    assert!(!error.to_string().contains("Make room"));
}

#[test]
fn an_existing_backup_is_replaced_by_a_complete_pre_upgrade_wal_snapshot() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("listener's library.db");
    fixture(&path);
    let destination = backup_path(&path);
    let previous = Connection::open(&destination).unwrap();
    previous.execute_batch("CREATE TABLE previous(value INTEGER); INSERT INTO previous VALUES(83); PRAGMA user_version=54;").unwrap();
    drop(previous);
    let old_bytes = std::fs::read(&destination).unwrap();
    let writer = Connection::open(&path).unwrap();
    apply_pragmas(&writer).unwrap();
    writer.execute_batch("INSERT INTO tracks(id,source_id,external_id,title,album_id,added_at) VALUES(2,1,'/second','Recent song',1,2);").unwrap();
    let wal = directory.path().join("listener's library.db-wal");
    assert!(std::fs::metadata(&wal).unwrap().len() > 0, "the snapshot includes committed WAL data");
    drop(crate::Library::open(&path).unwrap());
    assert_ne!(std::fs::read(&destination).unwrap(), old_bytes);
    let copied = Connection::open_with_flags(&destination, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(copied.query_row("SELECT title FROM tracks WHERE id=2", [], |row| row.get::<_, String>(0)).unwrap(), "Recent song");
    drop(copied);
    drop(writer);
    verify(&path);
    assert!(!std::fs::read_dir(directory.path()).unwrap().any(|entry| entry.unwrap().file_name().to_string_lossy().starts_with(".library-upgrade-")));
}

#[test]
fn a_backup_failure_keeps_the_previous_backup_and_cleans_private_staging() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("library.db");
    fixture(&path);
    let destination = backup_path(&path);
    let previous = Connection::open(&destination).unwrap();
    previous.execute_batch("CREATE TABLE previous(value INTEGER); INSERT INTO previous VALUES(91); PRAGMA user_version=54;").unwrap();
    drop(previous);
    let old_bytes = std::fs::read(&destination).unwrap();
    let db = Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA query_only=ON").unwrap();
    let error = upgrade::backup(&db, &path).unwrap_err();
    assert_eq!(error.sqlite_error_code(), Some(rusqlite::ErrorCode::ReadOnly));
    assert_eq!(std::fs::read(&destination).unwrap(), old_bytes);
    assert_eq!(db.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0)).unwrap(), VERSION - 1);
    assert!(!std::fs::read_dir(directory.path()).unwrap().any(|entry| entry.unwrap().file_name().to_string_lossy().starts_with(".library-upgrade-")));
}

#[cfg(unix)]
#[test]
fn a_published_backup_keeps_private_temporary_file_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("library.db");
    fixture(&path);
    let template = tempfile::NamedTempFile::new_in(directory.path()).unwrap();
    let expected = template.as_file().metadata().unwrap().permissions().mode() & 0o777;
    assert_eq!(expected & 0o077, 0, "temporary files exclude group and other access");
    drop(template);
    drop(crate::Library::open(&path).unwrap());
    let actual = std::fs::metadata(backup_path(&path)).unwrap().permissions().mode() & 0o777;
    assert_eq!(actual, expected, "publication preserves the earlier private backup permissions");
    verify(&path);
}
