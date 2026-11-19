use super::*;
use chilpa_library::schema::{self, VERSION};
use rusqlite::Connection;
use std::path::Path;
use std::time::{Duration, Instant};

pub(crate) fn older(path: &Path, version: usize) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let db = Connection::open(path).unwrap();
    schema::apply_pragmas(&db).unwrap();
    schema::stop_at(&db, version);
    db.execute_batch("INSERT INTO sources(id,provider,instance) VALUES(1,'local','default');
        INSERT INTO tracks(id,source_id,external_id,title,added_at) VALUES(1,1,'/fixture.flac','Song',1);").unwrap();
}

pub(crate) fn version_of(path: &Path) -> i64 {
    Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap()
        .query_row("PRAGMA user_version", [], |row| row.get(0)).unwrap()
}

/// Asks for the cover's view until the upgrade is over, as the cover does.
pub(crate) fn wait_for_the_upgrade(api: &mut crate::Chilpa) -> serde_json::Value {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let view: serde_json::Value = serde_json::from_str(&api.library_upgrade_view()).unwrap();
        if view["active"] != true || view["failed"] == true { return view; }
        assert!(Instant::now() < deadline, "the upgrade did not end: {view}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn the_cover_says_each_part_of_the_upgrade() {
    let at = |phase, step, steps, version, fraction| said(Some(&Progress { phase, step, steps, version, fraction }));
    assert_eq!(said(None), ("Updating the library…".to_string(), -1.0), "before it says anything, how far it is cannot be told");
    assert_eq!(at(Phase::WaitingForOther, 0, 0, 0, 0.0), ("Another Chilpa is updating this library. Waiting for it…".to_string(), -1.0));
    assert_eq!(at(Phase::BackingUp, 0, 3, 54, 0.0).0, "Saving a copy of the library first…");
    assert_eq!(at(Phase::Step, 2, 3, 56, 0.4), ("Step 2 of 3: changing how the library is stored".to_string(), 0.4));
    assert_eq!(at(Phase::Finishing, 3, 3, VERSION, 0.8), ("Checking the result…".to_string(), 0.8));
}

#[test]
fn a_failure_says_where_the_copy_from_before_is_only_when_there_is_one() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("library.db");
    older(&path, 54);
    let mut upgrade = LibraryUpgrade::detect(path.clone());
    upgrade.fail("disk I/O error".into());
    let view = upgrade.view();
    assert_eq!((view["active"].as_bool(), view["failed"].as_bool()), (Some(true), Some(true)));
    assert_eq!(view["title"], "The library could not be updated");
    assert_eq!(view["message"], "disk I/O error");
    assert_eq!(view["detail"], "Nothing was lost: the library is as it was before this step.");
    // A copy taken by this upgrade, which then failed at a later step.
    Connection::open(&path).unwrap().execute_batch("CREATE TABLE local_release_members(planted INTEGER);").unwrap();
    assert!(schema::upgrade::run(&path, &mut |_| {}, &Stop::default()).is_err());
    upgrade.fail("table local_release_members already exists".into());
    let backup = schema::backup_path(&path).display().to_string();
    assert_eq!(upgrade.view()["detail"], format!("Nothing was lost: the library is as it was before this step, and a copy from before the update is at {backup}."));
    drop(upgrade);
    assert_eq!(version_of(&path), 55, "nothing more ran");
}

#[test]
fn a_current_library_costs_one_look_and_shows_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("library.db");
    let upgrade = LibraryUpgrade::detect(path.clone());
    assert!(!upgrade.holds());
    assert!(!path.exists(), "looking does not create it");
    assert_eq!(upgrade.view(), serde_json::json!({ "active": false }));
    drop(chilpa_library::Library::open(&path).unwrap());
    assert!(!LibraryUpgrade::detect(path).holds());
}

#[test]
fn closing_during_an_upgrade_leaves_a_complete_version_and_a_true_marker() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("library.db");
    older(&path, 1);
    let mut upgrade = LibraryUpgrade::detect(path.clone());
    assert!(upgrade.holds());
    upgrade.start();
    let deadline = Instant::now() + Duration::from_secs(30);
    while !upgrade.heard.lock().unwrap().is_some_and(|progress| progress.phase == Phase::Step) {
        assert!(Instant::now() < deadline, "the upgrade never reached a step");
        std::thread::sleep(Duration::from_millis(1));
    }
    let closing = Instant::now();
    drop(upgrade);
    assert!(closing.elapsed() < Duration::from_secs(3), "closing does not wait for the whole upgrade");
    let reached = version_of(&path);
    let db = Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(db.query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0)).unwrap(), "ok");
    assert_eq!(db.query_row("SELECT title FROM tracks WHERE id = 1", [], |row| row.get::<_, String>(0)).unwrap(), "Song");
    drop(db);
    if reached < VERSION {
        assert_eq!(schema::upgrade::unfinished_copy(&path), Some((1, VERSION)), "the copy from before stays marked as this upgrade's");
    } else {
        assert!(!schema::upgrade::marker_path(&path).exists(), "a finished upgrade leaves no marker");
    }
    // The next start finishes what this one left.
    let mut again = LibraryUpgrade::detect(path.clone());
    if reached < VERSION {
        assert!(again.holds());
        again.start();
        let deadline = Instant::now() + Duration::from_secs(60);
        while !again.advance() {
            assert!(again.outcome.is_some(), "it failed: {}", again.view());
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    assert!(!again.holds());
    assert_eq!(version_of(&path), VERSION);
    assert_eq!(version_of(&schema::backup_path(&path)), 1, "the copy is still the one from before the upgrade");
    assert!(!schema::upgrade::marker_path(&path).exists());
}

#[test]
fn an_older_library_is_upgraded_before_anything_reads_it_and_then_everything_starts() {
    const CHILD: &str = "CHILPA_TEST_LIBRARY_UPGRADE_START";
    if std::env::var(CHILD).as_deref() != Ok("1") {
        let profile = tempfile::tempdir().unwrap();
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "library_upgrade::tests::an_older_library_is_upgraded_before_anything_reads_it_and_then_everything_starts", "--nocapture"])
            .env(CHILD, "1").envs(crate::test_support::profile_environment(profile.path())).output().unwrap();
        assert!(result.status.success(), "{}\n{}", String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
        return;
    }
    let path = chilpa_library::Library::default_path();
    older(&path, 50);
    let mut api = crate::Chilpa::new();
    assert!(api.library().is_none(), "nothing reads it before the upgrade");
    assert!(api.now.error.is_empty(), "and that is not an error: {}", api.now.error);
    assert_eq!(version_of(&path), 50, "building the facade leaves it as it was");
    assert!(!schema::backup_path(&path).exists());
    api.start_services();
    assert!(api.servers.keys.is_none(), "what reads the library waits for it");
    api.poll();
    let view = wait_for_the_upgrade(&mut api);
    assert_eq!(view, serde_json::json!({ "active": false }));
    assert_eq!(version_of(&path), VERSION);
    assert_eq!(version_of(&schema::backup_path(&path)), 50);
    assert!(api.servers.keys.is_some(), "and starts once it is done");
    assert!(api.library_changed(), "the window reads the library again");
    assert!(api.library().is_some());
}
