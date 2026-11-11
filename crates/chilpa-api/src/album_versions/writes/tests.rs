//! Grouping choices against a library on disk that another writer holds.

use super::*;
use std::time::{Duration, Instant};

/// Runs the test in a child with a profile of its own: true in the child.
fn isolated(name: &str) -> bool {
    const CHILD: &str = "CHILPA_GROUPING_TEST";
    if std::env::var(CHILD).as_deref() == Ok(name) {
        return true;
    }
    let profile = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env(CHILD, name)
        .envs(crate::test_support::profile_environment(profile.path()))
        .output()
        .unwrap();
    assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed; 0 failed"));
    false
}

/// Polls until every choice asked was kept or refused.
fn settle(api: &mut Chilpa) {
    let limit = Instant::now() + Duration::from_secs(30);
    while !api.grouping_settled() {
        assert!(Instant::now() < limit, "a grouping choice was never kept nor refused");
        api.poll_grouping();
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_grouping_choice_does_not_wait_for_another_writer_and_is_kept_after_it() {
    if !isolated("album_versions::writes::tests::a_grouping_choice_does_not_wait_for_another_writer_and_is_kept_after_it") { return; }
    let mut api = Chilpa::new();
    assert!(api.library().is_some());
    let before = api.group_album_versions();
    let _ = api.library_changed();

    // Another writer holds the library, as a sync's page did.
    let (held, holding) = std::sync::mpsc::channel();
    let holder = std::thread::spawn(move || {
        let other = chilpa_library::Library::open_existing(&chilpa_library::Library::default_path()).unwrap();
        other.connection().execute_batch("BEGIN IMMEDIATE").unwrap();
        held.send(()).unwrap();
        std::thread::sleep(Duration::from_millis(1_500));
        other.connection().execute_batch("COMMIT").unwrap();
    });
    holding.recv().unwrap();

    let asked = Instant::now();
    assert!(api.set_group_album_versions(!before));
    assert!(api.set_group_album_versions(before));
    assert!(api.set_group_album_versions(!before));
    assert!(asked.elapsed() < Duration::from_millis(300), "the window waited {:?} for another writer", asked.elapsed());
    assert_eq!(api.group_album_versions(), !before, "the newest choice shows at once");
    assert!(!api.library_changed(), "nothing is read again before it is kept");

    holder.join().unwrap();
    settle(&mut api);
    assert!(api.now.error.is_empty(), "{}", api.now.error);
    assert!(api.library_changed(), "kept, the window reads the library again");
    let library = chilpa_library::Library::open_existing(&chilpa_library::Library::default_path()).unwrap();
    assert_eq!(library.group_album_versions(), !before, "kept in the order asked");
    assert_eq!(api.group_album_versions(), !before);
}

#[test]
fn a_grouping_choice_that_cannot_be_kept_is_said_and_shows_as_the_library_has_it() {
    if !isolated("album_versions::writes::tests::a_grouping_choice_that_cannot_be_kept_is_said_and_shows_as_the_library_has_it") { return; }
    let mut api = Chilpa::new();
    assert!(api.library().is_some());
    let before = api.group_artist_versions();
    let _ = api.library_changed();
    let other = chilpa_library::Library::open_existing(&chilpa_library::Library::default_path()).unwrap();
    other.connection().execute_batch(
        "CREATE TRIGGER refuse_grouping BEFORE UPDATE ON album_presentation BEGIN SELECT RAISE(ABORT, 'refused here'); END;",
    ).unwrap();

    assert!(api.set_group_artist_versions(!before));
    assert_eq!(api.group_artist_versions(), !before);
    settle(&mut api);
    assert!(api.now.error.contains("refused here"), "said: {:?}", api.now.error);
    assert_eq!(api.group_artist_versions(), before, "the switch shows the library as it is");
    assert!(api.library_changed(), "the window reads it again, and its switch with it");
}
