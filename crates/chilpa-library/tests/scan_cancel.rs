//! Cancellation keeps complete records and never prunes an incomplete walk.
use chilpa_library::{Library, scan::{self, NoContainers, ScanControl}};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

fn wave(path: &Path) {
    let mut bytes = b"RIFF".to_vec();
    bytes.extend(44u32.to_le_bytes());
    bytes.extend(b"WAVEfmt ");
    bytes.extend(16u32.to_le_bytes());
    bytes.extend(1u16.to_le_bytes());
    bytes.extend(1u16.to_le_bytes());
    bytes.extend(8_000u32.to_le_bytes());
    bytes.extend(16_000u32.to_le_bytes());
    bytes.extend(2u16.to_le_bytes());
    bytes.extend(16u16.to_le_bytes());
    bytes.extend(b"data");
    bytes.extend(8u32.to_le_bytes());
    bytes.extend([0; 8]);
    std::fs::write(path, bytes).unwrap();
}

#[test]
fn interruption_commits_complete_files_without_pruning_and_resumes_incrementally() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let old = root.join("old.wav");
    wave(&old);
    let library = Library::in_memory().unwrap();
    scan::scan_root(&library, root).unwrap();
    library.connection().execute("UPDATE roots SET last_scan = 42", []).unwrap();
    std::fs::remove_file(&old).unwrap();
    for index in 0..260 { wave(&root.join(format!("{index:03}.wav"))); }
    let control = ScanControl::default();
    let finalized = AtomicBool::new(false);
    let report = scan::scan_controlled(&library, root, None,
        &|done| { if done >= 225 { control.stop(); } }, &NoContainers, &control,
        &|| finalized.store(true, Ordering::Relaxed)).unwrap();
    assert!(report.interrupted);
    assert!((225..=226).contains(&report.read));
    assert!(!finalized.load(Ordering::Relaxed));
    assert_eq!(report.missing, 0);
    assert_eq!(library.roots().unwrap()[0].last_scan, Some(42));
    let present: bool = library.connection().query_row("SELECT present FROM tracks WHERE external_id = ?1", [old.to_string_lossy().as_ref()], |row| row.get(0)).unwrap();
    assert!(present, "a file outside the partial walk is never marked missing");
    let resumed = scan::scan_root(&library, root).unwrap();
    assert!(!resumed.interrupted);
    assert_eq!(resumed.seen, 260);
    assert_eq!(resumed.unchanged, report.read, "committed file records are reused");
    assert_eq!(resumed.read + resumed.unchanged, 260);
    assert_eq!(resumed.missing, 1, "only a complete walk reconciles missing files");
}

#[test]
fn cancellation_before_discovery_does_not_touch_the_database() {
    let directory = tempfile::tempdir().unwrap();
    wave(&directory.path().join("track.wav"));
    let library = Library::in_memory().unwrap();
    let control = ScanControl::default();
    control.stop();
    assert_eq!(scan::count_controlled(directory.path(), &NoContainers, &control), None);
    let report = scan::scan_controlled(&library, directory.path(), None, &|_| {}, &NoContainers, &control, &|| panic!("no finalization")).unwrap();
    assert!(report.interrupted);
    assert!(library.roots().unwrap().is_empty());
}

/// What a scan said about its files and its commits.
#[derive(Default)]
struct Commits { listed: std::cell::Cell<usize>, done: std::cell::RefCell<Vec<(usize, u64)>> }
impl scan::ScanWatch for Commits {
    fn listed(&self, files: usize) { self.listed.set(files); }
    fn committed(&self, done: usize, written: u64) { self.done.borrow_mut().push((done, written)); }
}
impl Commits {
    fn done(&self) -> Vec<usize> { self.done.borrow().iter().map(|(done, _)| *done).collect() }
}

#[test]
fn the_first_batch_is_committed_at_twenty_five_files_and_then_every_two_hundred() {
    let directory = tempfile::tempdir().unwrap();
    for index in 0..450 { wave(&directory.path().join(format!("{index:03}.wav"))); }
    let library = Library::in_memory().unwrap();
    let commits = Commits::default();
    let report = scan::scan_watched(&library, directory.path(), None, &NoContainers, &ScanControl::default(), &commits).unwrap();
    assert_eq!(commits.listed.get(), 450);
    assert_eq!(report.read, 450);
    assert_eq!(commits.done(), [25, 200, 400, 450], "the first records are written early, the rest by the batch, the last batch too");
}

#[test]
fn a_root_smaller_than_the_first_batch_still_says_its_records_are_written() {
    let directory = tempfile::tempdir().unwrap();
    for index in 0..24 { wave(&directory.path().join(format!("{index:02}.wav"))); }
    let library = Library::in_memory().unwrap();
    let commits = Commits::default();
    scan::scan_watched(&library, directory.path(), None, &NoContainers, &ScanControl::default(), &commits).unwrap();
    let said = commits.done.borrow().clone();
    assert!(matches!(said[..], [(24, written)] if written > 0), "{said:?}");

    let again = Commits::default();
    scan::scan_watched(&library, directory.path(), None, &NoContainers, &ScanControl::default(), &again).unwrap();
    assert_eq!(*again.done.borrow(), [(24, 0)], "found as it was, it wrote nothing");
}

#[test]
fn availability_and_what_reconciliation_takes_away_are_said_as_written() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("music");
    std::fs::create_dir(&root).unwrap();
    for index in 0..3 { wave(&root.join(format!("{index}.wav"))); }
    let library = Library::in_memory().unwrap();
    scan::scan_root(&library, &root).unwrap();
    let scan = || {
        let commits = Commits::default();
        scan::scan_watched(&library, &root, None, &NoContainers, &ScanControl::default(), &commits).unwrap();
        commits.done.take()
    };

    // Back with every file as it was: only its availability changed.
    library.set_root_reachable(library.roots().unwrap()[0].id, false).unwrap();
    let back = scan();
    assert!(matches!(back[..], [(3, written)] if written > 0), "{back:?}");

    // One file gone: reading writes nothing, reconciliation does.
    std::fs::remove_file(root.join("0.wav")).unwrap();
    let gone = scan();
    assert!(matches!(gone[..], [(2, 0), (2, written)] if written > 0), "{gone:?}");

    // The folder stops answering.
    std::fs::rename(&root, directory.path().join("parked")).unwrap();
    let away = scan();
    assert!(matches!(away[..], [(0, written)] if written > 0), "{away:?}");
}

#[test]
fn a_share_away_only_while_it_is_walked_is_not_taken_for_emptied() {
    // Answers when asked, is away while walked, back before it is asked again.
    struct Bounce { album: std::path::PathBuf, parked: std::path::PathBuf }
    impl scan::ScanWatch for Bounce {
        fn walking(&self) { std::fs::rename(&self.album, &self.parked).unwrap(); }
        fn listed(&self, _: usize) { std::fs::rename(&self.parked, &self.album).unwrap(); }
    }
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("share");
    let bounce = Bounce { album: root.join("album"), parked: directory.path().join("parked") };
    std::fs::create_dir_all(&bounce.album).unwrap();
    for index in 0..3 { wave(&bounce.album.join(format!("{index}.wav"))); }
    let library = Library::in_memory().unwrap();
    scan::scan_root(&library, &root).unwrap();

    let report = scan::scan_watched(&library, &root, None, &NoContainers, &ScanControl::default(), &bounce).unwrap();
    if bounce.parked.exists() { std::fs::rename(&bounce.parked, &bounce.album).unwrap(); }
    assert!(report.unreachable, "an empty walk of a root that holds files is not believed");
    assert_eq!(report.missing, 0);
    let scanned: i64 = library.connection().query_row("SELECT count(*) FROM scanned_files", [], |row| row.get(0)).unwrap();
    assert_eq!((library.track_count(false).unwrap(), scanned), (3, 3), "the tracks and what was scanned of them are kept");
    let again = scan::scan_root(&library, &root).unwrap();
    assert_eq!((again.unreachable, again.unchanged), (false, 3));
}

#[test]
fn unchanged_checks_are_quiet_but_reachability_and_renames_are_reported() {
    let directory = tempfile::tempdir().unwrap();
    let first = directory.path().join("first.wav");
    wave(&first);
    let library = Library::in_memory().unwrap();
    scan::scan_root(&library, directory.path()).unwrap();
    let root = library.roots().unwrap()[0].id;
    let same = scan::scan_root(&library, directory.path()).unwrap();
    assert_eq!((same.read, same.missing), (0, 0));
    assert!(!same.changed, "an equivalent check needs no library refresh");
    library.set_root_reachable(root, false).unwrap();
    let back = scan::scan_root(&library, directory.path()).unwrap();
    assert!(back.changed);
    assert_eq!((back.read, back.missing), (0, 0));
    std::fs::rename(&first, directory.path().join("second.wav")).unwrap();
    let moved = scan::scan_root(&library, directory.path()).unwrap();
    assert_eq!((moved.read, moved.missing), (0, 0));
    assert!(moved.changed, "a retained track moved even though tags did not change");
}
