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
    assert_eq!(at(Phase::Step, 3, 3, 57, 0.6).0, "Step 3 of 3: giving every track a number that is never used again");
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

#[test]
fn a_failure_after_a_committed_step_offers_the_copy_and_puts_it_back() {
    const CHILD: &str = "CHILPA_TEST_LIBRARY_UPGRADE_RESTORE";
    if std::env::var(CHILD).as_deref() != Ok("1") {
        let profile = tempfile::tempdir().unwrap();
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "library_upgrade::tests::a_failure_after_a_committed_step_offers_the_copy_and_puts_it_back", "--nocapture"])
            .env(CHILD, "1").envs(crate::test_support::profile_environment(profile.path())).output().unwrap();
        assert!(result.status.success(), "{}\n{}", String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
        return;
    }
    let path = chilpa_library::Library::default_path();
    older(&path, 54);
    Connection::open(&path).unwrap().execute_batch("CREATE TABLE local_release_members(planted INTEGER);").unwrap();
    let mut api = crate::Chilpa::new();
    api.start_services();
    let view = wait_for_the_upgrade(&mut api);
    assert_eq!((view["failed"].as_bool(), view["canRestore"].as_bool()), (Some(true), Some(true)), "{view}");
    assert!(api.servers.keys.is_none(), "nothing that reads the library started");
    assert!(api.window_request().is_none());
    api.restore_library_copy();
    assert_eq!(wait_for_a_request(&mut api), crate::WindowRequest::Quit, "and Chilpa quits once the copy is back");
    assert_eq!(version_of(&path), 54, "the library is the copy from before");
    assert!(schema::upgrade::failed_path(&path).exists());
}

#[test]
fn a_failure_before_any_step_committed_does_not_offer_the_copy() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("library.db");
    older(&path, 54);
    let mut upgrade = LibraryUpgrade::detect(path.clone());
    upgrade.fail("disk I/O error".into());
    assert_eq!(upgrade.view()["canRestore"], false);
}

/// Runs `test` again in a child process with a profile of its own, marked
/// by `child`. True inside the child, where the test goes on.
pub(crate) fn in_own_profile(test: &str, child: &str) -> bool {
    if std::env::var(child).as_deref() == Ok("1") { return true; }
    let profile = tempfile::tempdir().unwrap();
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test, "--nocapture"])
        .env(child, "1").envs(crate::test_support::profile_environment(profile.path())).output().unwrap();
    assert!(result.status.success(), "{}\n{}", String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
    false
}

/// A library whose upgrade from 54 committed 55 and failed at 56, with its
/// copy from before at 54.
pub(crate) fn failed_at_56(path: &Path) {
    older(path, 54);
    // Step 56 creates this table.
    Connection::open(path).unwrap().execute_batch("CREATE TABLE local_release_members(planted INTEGER);").unwrap();
    assert!(schema::upgrade::run(path, &mut |_| {}, &Stop::default()).is_err());
}

/// Asks for the cover's view, which moves the upgrade on, until the window
/// is asked for something.
fn wait_for_a_request(api: &mut crate::Chilpa) -> crate::WindowRequest {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        api.library_upgrade_view();
        if let Some(request) = api.window_request() { return request; }
        assert!(Instant::now() < deadline, "the window was never asked for anything");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn a_library_missing_beside_its_marker_is_offered_its_copy_and_never_created() {
    if !in_own_profile("library_upgrade::tests::a_library_missing_beside_its_marker_is_offered_its_copy_and_never_created", "CHILPA_TEST_INTERRUPTED_RESTORE") { return; }
    let path = chilpa_library::Library::default_path();
    failed_at_56(&path);
    // A restore cut off between moving the library aside and putting the copy in.
    std::fs::rename(&path, schema::upgrade::failed_path(&path)).unwrap();
    let mut api = crate::Chilpa::new();
    assert!(api.library().is_none());
    assert!(!path.exists(), "nothing creates an empty library in its place");
    api.start_services();
    let view = wait_for_the_upgrade(&mut api);
    assert_eq!((view["failed"].as_bool(), view["canRestore"].as_bool(), view["canRetry"].as_bool()), (Some(true), Some(true), Some(false)), "{view}");
    assert_eq!(view["title"], "The library could not be updated");
    assert_eq!(view["message"], "Putting the copy back did not finish.");
    assert_eq!(view["detail"], format!("The copy from before the update is at {}.", schema::backup_path(&path).display()));
    api.retry_library_upgrade();
    assert!(!path.exists(), "trying the update again would create an empty library");
    assert!(api.servers.keys.is_none());
    api.restore_library_copy();
    assert_eq!(wait_for_a_request(&mut api), crate::WindowRequest::Quit);
    assert_eq!(version_of(&path), 54, "the copy is back in its place");
}

#[test]
fn a_restore_that_fails_says_the_library_was_not_changed() {
    if !in_own_profile("library_upgrade::tests::a_restore_that_fails_says_the_library_was_not_changed", "CHILPA_TEST_RESTORE_FAILS") { return; }
    let path = chilpa_library::Library::default_path();
    failed_at_56(&path);
    let mut api = crate::Chilpa::new();
    api.start_services();
    let view = wait_for_the_upgrade(&mut api);
    assert_eq!(view["canRestore"], true, "{view}");
    // The upgrade lock cannot be taken: the restore stops before it starts.
    let lock = path.with_file_name("library.db.upgrade-lock");
    let _ = std::fs::remove_file(&lock);
    std::fs::create_dir(&lock).unwrap();
    api.restore_library_copy();
    let view = wait_for_the_upgrade(&mut api);
    assert_eq!(view["failed"], true, "{view}");
    assert!(view["message"].as_str().unwrap().starts_with("The copy could not be put back: "), "{view}");
    assert_eq!(view["detail"], format!("The library was not changed, and the copy from before the update is still at {}.", schema::backup_path(&path).display()));
    assert_eq!(view["canRetry"], true);
    assert!(api.window_request().is_none());
    assert_eq!(version_of(&path), 55, "the library is where it was");
}

#[test]
fn a_marker_left_beside_a_current_library_is_removed_when_it_is_found() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("library.db");
    drop(chilpa_library::Library::open(&path).unwrap());
    std::fs::write(schema::upgrade::marker_path(&path), "54 57").unwrap();
    let upgrade = LibraryUpgrade::detect(path.clone());
    assert!(!upgrade.holds());
    assert!(!schema::upgrade::marker_path(&path).exists(), "a stale marker is not left to be taken for an unfinished upgrade");
}

#[test]
fn nothing_that_reads_the_library_starts_before_its_upgrade_ends() {
    if !in_own_profile("library_upgrade::tests::nothing_that_reads_the_library_starts_before_its_upgrade_ends", "CHILPA_TEST_NOTHING_STARTS") { return; }
    older(&chilpa_library::Library::default_path(), 50);
    let mut api = crate::Chilpa::new();
    api.start_services();
    assert!(api.library_upgrading());
    // What the window asks for by itself while the splash covers it.
    assert!(api.plugin_notifications(0).is_none(), "the plugin centre waits");
    api.plugin_updates_command("refresh", "{}");
    api.start_plugins();
    api.start_scrobbling();
    api.start_servers();
    api.start_media_controls();
    assert!(!api.plugins_starting() && api.plugins.is_none(), "no plugin starts");
    assert!(api.plugin_center.is_none());
    assert!(api.scrobbler.is_none());
    assert!(api.servers.keys.is_none());
    #[cfg(target_os = "linux")]
    assert!(!api.media_controls_started());
    wait_for_the_upgrade(&mut api);
    assert!(api.plugins_starting() || api.plugins.is_some(), "they start once it ends");
    assert!(api.scrobbler.is_some() && api.servers.keys.is_some());
}

fn onboarding_open(api: &crate::Chilpa) -> bool {
    let (_, text) = api.onboarding_doc(0).expect("a document");
    serde_json::from_str::<serde_json::Value>(&text).unwrap()["open"] == true
}

#[test]
fn nothing_opens_over_the_splash_while_the_library_is_upgraded() {
    if !in_own_profile("library_upgrade::tests::nothing_opens_over_the_splash_while_the_library_is_upgraded", "CHILPA_TEST_NOTHING_OPENS") { return; }
    older(&chilpa_library::Library::default_path(), 50);
    let mut api = crate::Chilpa::new();
    api.start_services();
    api.onboarding_start();
    assert!(!onboarding_open(&api), "the Welcome would open over the splash");
    api.appimage_updates_check(true);
    assert!(api.appimage_updates.is_none(), "the start's update check waits for the library");
    wait_for_the_upgrade(&mut api);
    assert!(api.appimage_updates.is_some(), "and runs once it is current");
    // The Welcome of a new library waits for the plugins, which start now.
    let deadline = Instant::now() + Duration::from_secs(30);
    while !onboarding_open(&api) {
        assert!(Instant::now() < deadline, "the Welcome never opened after the upgrade");
        api.poll();
        api.onboarding_start();
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn putting_the_copy_back_runs_behind_a_line_that_says_so() {
    if !in_own_profile("library_upgrade::tests::putting_the_copy_back_runs_behind_a_line_that_says_so", "CHILPA_TEST_RESTORE_BEHIND") { return; }
    let path = chilpa_library::Library::default_path();
    failed_at_56(&path);
    let mut api = crate::Chilpa::new();
    api.start_services();
    assert_eq!(wait_for_the_upgrade(&mut api)["canRestore"], true);
    api.restore_library_copy();
    assert!(api.window_request().is_none(), "Chilpa quits once the copy is back, not before");
    let view = api.upgrade.view();
    assert_eq!((view["active"].as_bool(), view["failed"].as_bool()), (Some(true), Some(false)), "{view}");
    assert_eq!(view["text"], "Putting the copy back…");
    assert_eq!(view["progress"], -1.0, "how long a copy takes cannot be told");
    assert_eq!(wait_for_a_request(&mut api), crate::WindowRequest::Quit);
    assert_eq!(version_of(&path), 54);
}

#[test]
fn a_library_swapped_for_an_older_one_while_running_is_said() {
    if !in_own_profile("library_upgrade::tests::a_library_swapped_for_an_older_one_while_running_is_said", "CHILPA_TEST_SWAPPED_LIBRARY") { return; }
    let path = chilpa_library::Library::default_path();
    drop(chilpa_library::Library::open(&path).unwrap());
    let mut api = crate::Chilpa::new();
    assert!(!api.library_upgrading());
    // Replaced by an older file after the start, before it was first read.
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
    }
    older(&path, 50);
    assert!(api.library().is_none());
    assert!(api.now.error.contains("has to be updated first"), "the window says why the library is empty: {:?}", api.now.error);
}

#[test]
fn a_part_whose_length_cannot_be_told_sweeps() {
    let at = |phase| said(Some(&Progress { phase, step: 0, steps: 3, version: 54, fraction: 0.25 })).1;
    assert_eq!(at(Phase::BackingUp), -1.0, "the copy says nothing of how far it is until it ends");
    assert_eq!(at(Phase::WaitingForOther), -1.0);
    assert_eq!(at(Phase::Step), 0.25);
}

#[test]
fn a_failure_after_the_last_step_says_the_library_was_updated() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("library.db");
    drop(chilpa_library::Library::open(&path).unwrap());
    let mut upgrade = LibraryUpgrade::detect(path.clone());
    // What the open after the steps, or removing the marker, would report.
    upgrade.fail("database is locked".into());
    let view = upgrade.view();
    assert_eq!(view["title"], "The library was updated, but the check after it failed", "{view}");
    assert!(!view["detail"].as_str().unwrap().contains("Nothing was lost"), "it is not as it was before: {view}");
    assert_eq!((view["canRestore"].as_bool(), view["canRetry"].as_bool()), (Some(false), Some(true)));
}

#[test]
fn an_upgrade_thread_that_ends_without_an_answer_says_so() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("library.db");
    older(&path, 54);
    let mut upgrade = LibraryUpgrade::detect(path.clone());
    let (tell, outcome) = mpsc::channel::<Result<(), String>>();
    drop(tell);
    upgrade.outcome = Some(outcome);
    upgrade.state = State::Running;
    assert!(!upgrade.advance());
    assert_eq!(upgrade.view()["message"], "The library update stopped unexpectedly.");
}

#[test]
fn quitting_during_an_upgrade_does_not_ask() {
    if !in_own_profile("library_upgrade::tests::quitting_during_an_upgrade_does_not_ask", "CHILPA_TEST_QUIT_UNASKED") { return; }
    older(&chilpa_library::Library::default_path(), 50);
    let mut api = crate::Chilpa::new();
    api.start_services();
    // The question says playback stops and the window closes; what stops is
    // the upgrade, and it goes on at the next start.
    assert!(!api.confirms_quit());
    wait_for_the_upgrade(&mut api);
    assert!(api.confirms_quit(), "afterwards quitting asks as it did");
}

#[test]
fn files_asked_for_during_an_upgrade_open_once_it_ends() {
    if !in_own_profile("library_upgrade::tests::files_asked_for_during_an_upgrade_open_once_it_ends", "CHILPA_TEST_FILES_WAIT") { return; }
    older(&chilpa_library::Library::default_path(), 50);
    let mut api = crate::Chilpa::new();
    api.start_services();
    // A second launch, or this one's command line, handed over a file.
    api.desktop.opening.push("/music/asked.flac".into());
    api.desktop.requests.push_back(crate::WindowRequest::Open);
    assert!(api.window_request().is_none(), "nothing is opened behind the splash");
    assert!(api.take_open_request().is_none());
    wait_for_the_upgrade(&mut api);
    assert_eq!(api.window_request(), Some(crate::WindowRequest::Open));
    assert_eq!(api.take_open_request().as_deref(), Some("/music/asked.flac"));
}
