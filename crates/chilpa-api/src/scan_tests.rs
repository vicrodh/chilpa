use super::*;
use crate::ScanPolicy;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

fn isolated(name: &str) -> bool {
    const CHILD: &str = "CHILPA_SCAN_TEST";
    if std::env::var(CHILD).as_deref() == Ok(name) { return true; }
    let profile = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"]).env(CHILD, name)
        .envs(chilpa_dirs::profile_environment(profile.path())).output().unwrap();
    assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed; 0 failed"));
    false
}

fn wave(path: &Path) {
    let mut bytes = b"RIFF".to_vec();
    bytes.extend(44u32.to_le_bytes()); bytes.extend(b"WAVEfmt "); bytes.extend(16u32.to_le_bytes());
    bytes.extend(1u16.to_le_bytes()); bytes.extend(1u16.to_le_bytes()); bytes.extend(8_000u32.to_le_bytes());
    bytes.extend(16_000u32.to_le_bytes()); bytes.extend(2u16.to_le_bytes()); bytes.extend(16u16.to_le_bytes());
    bytes.extend(b"data"); bytes.extend(8u32.to_le_bytes()); bytes.extend([0; 8]);
    std::fs::write(path, bytes).unwrap();
}
fn finish(api: &mut Chilpa) {
    let limit = Instant::now() + Duration::from_secs(20);
    while api.scan_status().running {
        assert!(Instant::now() < limit, "scan did not finish");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn periodic_and_startup_policies_are_persistent_incremental_and_idle_between_checks() {
    if !isolated("scan_actions::tests::periodic_and_startup_policies_are_persistent_incremental_and_idle_between_checks") { return; }
    let music = tempfile::tempdir().unwrap();
    wave(&music.path().join("track.wav"));
    let path = music.path().to_string_lossy().into_owned();
    let mut api = Chilpa::new();
    assert!(api.start_scan(music.path()));
    finish(&mut api);
    assert_eq!(api.scan_status.phase, ScanPhase::Complete);
    assert_eq!(api.scan_status.summary.read, 1);
    assert!(api.set_folder_scan_policy(&path, "12h"));
    let saved_revision = api.scan_revision();
    assert!(api.set_folder_scan_policy(&path, "12h"));
    assert_eq!(api.scan_revision(), saved_revision, "the same policy publishes nothing again");
    api.advance_scan();
    assert!(api.scan.is_none(), "a fresh scan is not due");
    let revision = api.scan_revision();
    let deadline = api.scan_runtime.next_check;
    let changes = api.library().unwrap().connection().total_changes();
    for _ in 0..100 { api.advance_scan(); }
    assert_eq!(api.scan_revision(), revision);
    assert_eq!(api.scan_runtime.next_check, deadline);
    assert_eq!(api.library().unwrap().connection().total_changes(), changes, "idle checks never scan or rewrite library rows");
    api.scan_runtime.last_attempts.insert(path.clone(), scan_state::now() - 43_200);
    api.scan_runtime.next_check = Instant::now();
    api.advance_scan();
    assert!(api.scan.is_some());
    let called = AtomicBool::new(false);
    assert!(!api.start_scan_using(vec![music.path().into()], |_| { called.store(true, Ordering::Relaxed); unreachable!() }));
    assert!(!called.load(Ordering::Relaxed), "only one worker can start");
    finish(&mut api);
    assert_eq!(api.scan_status.summary.read, 0);
    assert_eq!(api.scan_status.summary.unchanged, 1);
    assert!(scan_state::intent(chilpa_core::settings::read_value(INTENT).unwrap().as_deref()).unwrap().is_empty());
    assert!(api.set_folder_scan_policy(&path, "startup"));
    api.advance_scan();
    assert!(api.scan.is_none(), "startup means the next launch");
    drop(api);
    let mut reopened = Chilpa::new();
    reopened.advance_scan();
    assert!(reopened.scan.is_some());
    finish(&mut reopened);
    assert_eq!(reopened.folder_scan_policy(&path), ScanPolicy::Startup);
    reopened.scan_runtime.next_check = Instant::now();
    reopened.advance_scan();
    assert!(reopened.scan.is_none(), "a startup policy runs only once per launch");
    assert!(reopened.forget_folder(&path));
    assert!(!reopened.scan_runtime.policies.contains_key(&path), "forgetting cannot schedule the folder back into the library");
}

#[test]
fn an_unconfirmed_scan_is_restored_without_surprising_manual_restart() {
    if !isolated("scan_actions::tests::an_unconfirmed_scan_is_restored_without_surprising_manual_restart") { return; }
    let music = tempfile::tempdir().unwrap();
    wave(&music.path().join("track.wav"));
    let mut api = Chilpa::new();
    assert!(api.start_scan(music.path()));
    finish(&mut api);
    drop(api);
    let roots = vec![music.path().to_path_buf()];
    chilpa_core::settings::update_value(INTENT, |_| Ok((scan_state::validate_roots(&roots)?, ()))).unwrap();
    let mut api = Chilpa::new();
    assert_eq!(api.scan_status().phase, ScanPhase::Interrupted);
    assert!(api.scan_status().can_resume);
    for _ in 0..10 { api.advance_scan(); }
    assert!(api.scan.is_none());
    assert!(!api.start_scan_using(roots.clone(), |_| Err(std::io::Error::other("injected resume failure"))));
    assert_eq!(api.scan_runtime.intent, roots);
    assert_eq!(scan_state::intent(chilpa_core::settings::read_value(INTENT).unwrap().as_deref()).unwrap(), roots);
    assert!(api.scan_status.can_resume);
    assert!(api.resume_scan());
    finish(&mut api);
    assert_eq!(api.scan_status.phase, ScanPhase::Complete);
    assert_eq!(api.scan_status.summary.unchanged, 1);
    assert_eq!(api.scan_status.summary.read, 0);
    assert!(!api.scan_status.can_resume);
}

#[test]
fn storage_and_spawn_failures_do_not_start_or_leave_a_false_active_scan() {
    if !isolated("scan_actions::tests::storage_and_spawn_failures_do_not_start_or_leave_a_false_active_scan") { return; }
    let music = tempfile::tempdir().unwrap();
    let mut api = Chilpa::new();
    api.ensure_scan_runtime();
    let file = chilpa_core::settings::path();
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    let saved = std::fs::read(&file).ok();
    if file.exists() { std::fs::remove_file(&file).unwrap(); }
    std::fs::create_dir(&file).unwrap();
    let called = AtomicBool::new(false);
    assert!(!api.start_scan_using(vec![music.path().into()], |_| { called.store(true, Ordering::Relaxed); unreachable!() }));
    assert!(!called.load(Ordering::Relaxed));
    assert!(api.scan.is_none());
    assert_eq!(api.scan_status.phase, ScanPhase::Failed);
    std::fs::remove_dir(&file).unwrap();
    if let Some(saved) = saved { std::fs::write(&file, saved).unwrap(); }
    assert!(!api.start_scan_using(vec![music.path().into()], |_| Err(std::io::Error::other("injected thread failure"))));
    assert!(api.scan_runtime.intent.is_empty());
    assert!(!api.scan_status.running);
    assert!(!api.scan_status.can_resume);
    assert!(scan_state::intent(chilpa_core::settings::read_value(INTENT).unwrap().as_deref()).unwrap().is_empty());
    assert!(!api.start_scan_using(vec![music.path().into(); scan_state::MAX_ROOTS + 1], |_| panic!("over-limit job started")));
    let bad = music.path().join("broken.db");
    std::fs::write(&bad, "not a database").unwrap();
    assert!(api.start_scan_using(vec![music.path().into()], |roots| chilpa_library::ScanJob::try_start_with(
        bad, roots, None, std::sync::Arc::new(chilpa_library::scan::NoContainers))));
    finish(&mut api);
    assert_eq!(api.scan_status.phase, ScanPhase::Failed, "a worker error is never a successful empty summary");
    assert!(!api.scan_status.error.is_empty());
    assert!(api.scan_status.can_resume);
}

#[test]
fn dismissing_interrupted_manual_scan_releases_due_automatic_checks() {
    if !isolated("scan_actions::tests::dismissing_interrupted_manual_scan_releases_due_automatic_checks") { return; }
    let manual = tempfile::tempdir().unwrap();
    let automatic = tempfile::tempdir().unwrap();
    wave(&manual.path().join("manual.wav"));
    wave(&automatic.path().join("automatic.wav"));
    let mut api = Chilpa::new();
    for root in [manual.path(), automatic.path()] {
        assert!(api.start_scan(root));
        finish(&mut api);
    }
    let path = automatic.path().to_string_lossy().into_owned();
    assert!(api.set_folder_scan_policy(&path, "12h"));
    drop(api);
    let pending = vec![manual.path().to_path_buf()];
    chilpa_core::settings::update_value(INTENT, |_| Ok((scan_state::validate_roots(&pending)?, ()))).unwrap();
    let mut api = Chilpa::new();
    assert_eq!(api.scan_status().phase, ScanPhase::Interrupted);
    api.scan_runtime.last_attempts.insert(path, scan_state::now() - 43_200);
    api.scan_runtime.next_check = Instant::now();
    for _ in 0..10 { api.advance_scan(); }
    assert!(api.scan.is_none(), "an interrupted manual scan pauses automatic checks");
    assert!(!api.start_scan(automatic.path()), "another manual request cannot overwrite pending intent");
    assert_eq!(api.scan_runtime.intent, pending);
    assert_eq!(scan_state::intent(chilpa_core::settings::read_value(INTENT).unwrap().as_deref()).unwrap(), pending);
    api.dismiss_scan_status();
    assert!(!api.scan_status.can_resume);
    assert!(api.scan_runtime.intent.is_empty());
    api.advance_scan();
    assert!(api.scan.is_some(), "dismiss releases a due policy without resuming the manual scan");
    assert_eq!(api.scan_runtime.intent, vec![automatic.path().to_path_buf()]);
    finish(&mut api);
    assert_eq!(api.scan_status.summary.unchanged, 1);
    assert_eq!(api.library_folders().len(), 2, "dismiss keeps the registered folders");
    assert!(api.start_scan(manual.path()));
    finish(&mut api);
    assert_eq!(api.scan_status.summary.unchanged, 1, "dismiss preserves committed tracks");
}

#[test]
fn the_first_batch_of_a_running_scan_reaches_the_screens_before_it_ends() {
    if !isolated("scan_actions::tests::the_first_batch_of_a_running_scan_reaches_the_screens_before_it_ends") { return; }
    let music = tempfile::tempdir().unwrap();
    for index in 0..2_000 { wave(&music.path().join(format!("{index:04}.wav"))); }
    let mut api = Chilpa::new();
    assert!(api.start_scan(music.path()));
    assert!(api.library_changed(), "a folder that joined the library is a change on its own");
    let mut shown_while_running = false;
    let mut found_was_published = false;
    let limit = Instant::now() + Duration::from_secs(60);
    while api.scan_status().running {
        assert!(Instant::now() < limit, "scan did not finish");
        found_was_published |= api.scan_status.found > 0;
        shown_while_running |= api.library_changed();
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(shown_while_running, "the records of the first batch are shown while the rest is still being read");
    assert!(found_was_published, "the walk says how many files it has found");
    assert_eq!(api.scan_status.summary.read, 2_000);
}

#[test]
fn a_rescan_that_finds_nothing_changed_asks_no_screen_to_read_the_library_again() {
    if !isolated("scan_actions::tests::a_rescan_that_finds_nothing_changed_asks_no_screen_to_read_the_library_again") { return; }
    let music = tempfile::tempdir().unwrap();
    for index in 0..600 { wave(&music.path().join(format!("{index:04}.wav"))); }
    let mut api = Chilpa::new();
    assert!(api.start_scan(music.path()));
    finish(&mut api);
    assert_eq!(api.scan_status.summary.read, 600);
    assert!(api.library_changed());
    assert!(api.rescan());
    finish(&mut api);
    assert_eq!(api.scan_status.summary.unchanged, 600);
    assert!(!api.library_changed(), "no batch wrote a row, so no screen reads the library again");
}

#[test]
fn folder_projection_uses_background_reachability_without_filesystem_probes() {
    if !isolated("scan_actions::tests::folder_projection_uses_background_reachability_without_filesystem_probes") { return; }
    let directory = tempfile::tempdir().unwrap();
    let missing = directory.path().join("not-mounted");
    let mut api = Chilpa::new();
    let library = api.library().unwrap();
    let source = library.source_id("local", "default").unwrap();
    let root = library.root_id(source, &missing).unwrap();
    library.set_root_reachable(root, true).unwrap();
    assert!(api.library_folders()[0].reachable, "projection consumes the background snapshot even when the path is absent");
    api.library().unwrap().set_root_reachable(root, false).unwrap();
    assert!(!api.library_folders()[0].reachable, "a background availability update reaches the projection");
}
