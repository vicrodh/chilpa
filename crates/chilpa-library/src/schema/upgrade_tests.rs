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
fn concurrent_upgrades_keep_one_pre_upgrade_backup() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("library.db");
    fixture(&path);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let workers: Vec<_> = (0..2).map(|_| {
        let (path, barrier) = (path.clone(), barrier.clone());
        std::thread::spawn(move || {
            barrier.wait();
            upgrade::run(&path, &mut |_| {}, &upgrade::Stop::default()).unwrap();
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
    upgrade::run(Path::new(&path), &mut |_| {}, &upgrade::Stop::default()).unwrap();
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
    upgrade::run(&path, &mut |_| {}, &upgrade::Stop::default()).unwrap();
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
    upgrade::run(&path, &mut |_| {}, &upgrade::Stop::default()).unwrap();
    let actual = std::fs::metadata(backup_path(&path)).unwrap().permissions().mode() & 0o777;
    assert_eq!(actual, expected, "publication preserves the earlier private backup permissions");
    verify(&path);
}

fn at(path: &Path, version: usize) {
    let db = Connection::open(path).unwrap();
    apply_pragmas(&db).unwrap();
    stop_at(&db, version);
    db.execute_batch("INSERT INTO sources(id,provider,instance) VALUES(1,'local','default');
        INSERT INTO tracks(id,source_id,external_id,title,added_at) VALUES(1,1,'/fixture','Song',1);").unwrap();
}

fn version_of(path: &Path) -> i64 {
    Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap()
        .query_row("PRAGMA user_version", [], |row| row.get(0)).unwrap()
}

fn heard(path: &Path, stop: &upgrade::Stop, mut also: impl FnMut(&upgrade::Progress)) -> (rusqlite::Result<()>, Vec<upgrade::Progress>) {
    let mut seen = Vec::new();
    let result = upgrade::run(path, &mut |progress| { also(progress); seen.push(*progress); }, stop);
    (result, seen)
}

fn staging_left(directory: &Path) -> bool {
    std::fs::read_dir(directory).unwrap().any(|entry| entry.unwrap().file_name().to_string_lossy().starts_with(".library-upgrade-"))
}

#[test]
fn the_version_is_the_number_of_steps() {
    assert_eq!(STEPS.len() as i64, VERSION);
}

#[test]
fn pending_names_only_an_older_library_and_writes_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("library.db");
    assert_eq!(upgrade::pending(&path), None, "no file");
    assert!(!path.exists(), "asking does not create it");
    at(&path, 40);
    assert_eq!(upgrade::pending(&path), Some(upgrade::Pending { from: 40, to: VERSION }));
    let current = directory.path().join("current.db");
    drop(crate::Library::open(&current).unwrap());
    assert_eq!(upgrade::pending(&current), None, "current");
    let newer = directory.path().join("newer.db");
    Connection::open(&newer).unwrap().pragma_update(None, "user_version", VERSION + 1).unwrap();
    assert_eq!(upgrade::pending(&newer), None, "newer is its opener's error to say");
    assert_eq!(version_of(&path), 40);
    assert!(!directory.path().join("library.db.upgrade-lock").exists());
    assert!(!backup_path(&path).exists());
}

#[test]
fn the_observer_hears_every_step_in_order_and_ends_at_one() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("library.db");
    at(&path, 50);
    let (result, seen) = heard(&path, &upgrade::Stop::default(), |_| {});
    result.unwrap();
    assert_eq!(seen[0].phase, upgrade::Phase::BackingUp);
    // A step in parts is heard once a part.
    let mut steps: Vec<_> = seen.iter().filter(|p| p.phase == upgrade::Phase::Step).map(|p| (p.version, p.step, p.steps)).collect();
    steps.dedup();
    assert_eq!(steps.iter().map(|p| p.0).collect::<Vec<_>>(), (51..=VERSION).collect::<Vec<_>>(), "every step, in order");
    assert_eq!(steps.iter().map(|p| p.1).collect::<Vec<_>>(), (1..=steps.len()).collect::<Vec<_>>());
    assert!(steps.iter().all(|p| p.2 == steps.len()));
    assert!(seen.windows(2).all(|pair| pair[0].fraction <= pair[1].fraction), "the bar never goes back: {seen:?}");
    let last = seen.last().unwrap();
    assert_eq!((last.phase, last.fraction), (upgrade::Phase::Finishing, 1.0));
    assert_eq!(version_of(&path), VERSION);
    assert_eq!(version_of(&backup_path(&path)), 50);
    assert!(!upgrade::marker_path(&path).exists(), "a completed upgrade leaves no marker");
}

#[test]
fn a_current_library_is_only_opened() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("library.db");
    drop(crate::Library::open(&path).unwrap());
    // Creating it took the lock; running over it must not.
    std::fs::remove_file(directory.path().join("library.db.upgrade-lock")).unwrap();
    let (result, seen) = heard(&path, &upgrade::Stop::default(), |_| {});
    result.unwrap();
    assert_eq!(seen.iter().map(|p| (p.phase, p.fraction)).collect::<Vec<_>>(), [(upgrade::Phase::Finishing, 1.0)]);
    assert!(!backup_path(&path).exists());
    assert!(!directory.path().join("library.db.upgrade-lock").exists());
}

#[test]
fn a_failed_step_keeps_the_copy_from_before_the_upgrade_for_the_next_attempt() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("library.db");
    at(&path, 54);
    // Step 56 creates this table: the upgrade commits 55 and stops at 56.
    Connection::open(&path).unwrap().execute_batch("CREATE TABLE local_release_members(planted INTEGER);").unwrap();
    let (result, _) = heard(&path, &upgrade::Stop::default(), |_| {});
    assert!(result.unwrap_err().to_string().contains("local_release_members"));
    assert_eq!(version_of(&path), 55, "the step before the failure is kept");
    assert_eq!(version_of(&backup_path(&path)), 54);
    assert_eq!(upgrade::unfinished_copy(&path), Some((54, VERSION)));
    let copy = std::fs::read(backup_path(&path)).unwrap();
    Connection::open(&path).unwrap().execute_batch("DROP TABLE local_release_members;").unwrap();
    let (result, seen) = heard(&path, &upgrade::Stop::default(), |_| {});
    result.unwrap();
    assert!(seen.iter().all(|p| p.phase != upgrade::Phase::BackingUp), "the copy at hand is not taken again");
    assert_eq!(std::fs::read(backup_path(&path)).unwrap(), copy, "the copy is still the one from before the upgrade");
    assert_eq!(version_of(&path), VERSION);
    assert!(!upgrade::marker_path(&path).exists());
}

#[test]
fn a_marker_left_by_a_completed_upgrade_is_ignored() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("library.db");
    at(&path, 54);
    at(&backup_path(&path), 53);
    // An upgrade from 53 to 54 died after its last step and before it removed its marker.
    std::fs::write(upgrade::marker_path(&path), "53 54").unwrap();
    assert_eq!(upgrade::unfinished_copy(&path), Some((53, 54)));
    let (result, seen) = heard(&path, &upgrade::Stop::default(), |_| {});
    result.unwrap();
    assert_eq!(seen[0].phase, upgrade::Phase::BackingUp, "that copy is not from this upgrade");
    assert_eq!(version_of(&backup_path(&path)), 54);
    assert!(!upgrade::marker_path(&path).exists());
}

#[test]
fn leftover_staging_folders_are_removed_before_the_copy() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("library.db");
    at(&path, 54);
    let leftover = directory.path().join(".library-upgrade-killed");
    std::fs::create_dir(&leftover).unwrap();
    std::fs::write(leftover.join("half"), b"a copy killed halfway").unwrap();
    heard(&path, &upgrade::Stop::default(), |_| {}).0.unwrap();
    assert!(!staging_left(directory.path()));
}

#[test]
fn a_held_upgrade_lock_is_waited_for_past_five_seconds_and_said() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("library.db");
    at(&path, 54);
    let held = upgrade::lock(&path).unwrap();
    let (said, phases) = std::sync::mpsc::channel();
    let worker = {
        let path = path.clone();
        std::thread::spawn(move || upgrade::run(&path, &mut |progress| { let _ = said.send(progress.phase); }, &upgrade::Stop::default()))
    };
    assert_eq!(phases.recv_timeout(Duration::from_secs(5)).unwrap(), upgrade::Phase::WaitingForOther);
    // The bounded wait of an opener gives up after five seconds; this one does not.
    std::thread::sleep(Duration::from_millis(5300));
    assert!(!worker.is_finished(), "still waiting");
    assert_eq!(version_of(&path), 54);
    drop(held);
    worker.join().unwrap().unwrap();
    assert_eq!(version_of(&path), VERSION);
}

#[test]
fn a_stop_between_steps_keeps_the_last_complete_version_and_the_copy() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("library.db");
    at(&path, 52);
    let stop = upgrade::Stop::default();
    let (result, _) = heard(&path, &stop, |progress| if progress.phase == upgrade::Phase::Step && progress.version == 54 { stop.request() });
    assert_eq!(result.unwrap_err().sqlite_error_code(), Some(rusqlite::ErrorCode::OperationInterrupted));
    assert_eq!(version_of(&path), 53, "the step that was asked to stop never began");
    let db = Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(db.query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0)).unwrap(), "ok");
    drop(db);
    assert_eq!(upgrade::unfinished_copy(&path), Some((52, VERSION)), "the marker still names the copy from before");
    let copy = std::fs::read(backup_path(&path)).unwrap();
    heard(&path, &upgrade::Stop::default(), |_| {}).0.unwrap();
    assert_eq!(std::fs::read(backup_path(&path)).unwrap(), copy);
    assert_eq!(version_of(&path), VERSION);
    assert!(!upgrade::marker_path(&path).exists());
}

#[test]
fn a_stop_before_the_copy_changes_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("library.db");
    at(&path, 54);
    at(&backup_path(&path), 53);
    let previous = std::fs::read(backup_path(&path)).unwrap();
    let stop = upgrade::Stop::default();
    let (result, _) = heard(&path, &stop, |progress| if progress.phase == upgrade::Phase::BackingUp { stop.request() });
    assert_eq!(result.unwrap_err().sqlite_error_code(), Some(rusqlite::ErrorCode::OperationInterrupted));
    assert_eq!(version_of(&path), 54);
    assert_eq!(std::fs::read(backup_path(&path)).unwrap(), previous);
    assert!(!upgrade::marker_path(&path).exists());
    assert!(!staging_left(directory.path()));
}

fn listing(directory: &Path) -> Vec<String> {
    // The log and shared memory of a connection come and go with it.
    let mut names: Vec<String> = std::fs::read_dir(directory).unwrap().map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| !name.ends_with("-wal") && !name.ends_with("-shm")).collect();
    names.sort();
    names
}

#[test]
fn every_opener_refuses_an_older_library_and_leaves_it_alone() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("library.db");
    at(&path, 54);
    let (files, bytes) = (listing(directory.path()), std::fs::read(&path).unwrap());
    let refused = crate::Library::open(&path).err().expect("an opener never upgrades");
    assert_eq!(refused.sqlite_error_code(), Some(rusqlite::ErrorCode::TypeMismatch));
    assert!(refused.to_string().contains("has to be updated first"), "{refused}");
    assert!(crate::Library::open_existing(&path).is_err());
    assert!(crate::Library::open_read_only(&path).is_err());
    let scan = crate::ScanJob::try_start_with(path.clone(), vec![directory.path().join("music")], None, std::sync::Arc::new(crate::scan::NoContainers)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !scan.poll().iter().any(|event| matches!(event, crate::ScanProgress::Failed(_))) {
        assert!(Instant::now() < deadline, "a scan of an older library fails instead of upgrading it");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(version_of(&path), 54);
    assert_eq!(listing(directory.path()), files, "no copy, marker or lock file");
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}
