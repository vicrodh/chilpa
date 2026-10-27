use chilpa_sync::*;
use crate::transport::{Entry, Mounted, Transport};
use std::io::{Read, Write};

struct Rig { root: tempfile::TempDir, source: tempfile::TempDir, profile: tempfile::TempDir, state: State, device: Mounted }
impl Rig {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let source = tempfile::tempdir().unwrap();
        let profile = tempfile::tempdir().unwrap();
        let mut state = State::open(&profile.path().join("sync.db")).unwrap();
        let mut device = Mounted::open(root.path()).unwrap();
        pair(&mut state, &mut device, "Test card", &Cancellation::default()).unwrap();
        Self { root, source, profile, state, device }
    }
    fn input(&self, id: &str, bytes: &[u8]) -> Input {
        let source = self.source.path().join(format!("{id}.flac"));
        std::fs::write(&source, bytes).unwrap();
        Input { id:id.into(), source, artist:"Artist: Name".into(), album:"Album?".into(), title:id.into(), track_number:1 }
    }
    fn sync(&mut self, inputs: &[Input], playlists: &[Playlist], remove: bool) -> Plan {
        let cancel = Cancellation::default();
        let plan = prepare(&self.state, &mut self.device, inputs, playlists, remove, &cancel).unwrap();
        synchronize(&mut self.state, &mut self.device, &plan, plan.id(), remove, &cancel, &mut |_| ()).unwrap();
        plan
    }
}
fn playlist(tracks: &[&str]) -> Playlist {
    Playlist { id:"list-1".into(), name:"My music".into(), tracks:tracks.iter().map(|s| (*s).into()).collect() }
}
fn copies(plan: &Plan) -> Vec<RelativePath> {
    plan.actions().iter().filter_map(|a| match a { Action::Copy { path, .. } => Some(path.clone()), _ => None }).collect()
}

#[test]
fn incremental_sync_preserves_bytes_playlist_order_and_duplicates() {
    let mut rig = Rig::new();
    let a = rig.input("a", b"first file");
    let b = rig.input("b", b"second file");
    let lists = [playlist(&["b", "a", "b"])];
    let plan = rig.sync(&[a.clone(), b.clone()], &lists, false);
    assert_eq!(rig.state.managed_count().unwrap(), 3);
    let files = copies(&plan);
    let list = files.iter().find(|p| p.as_str().ends_with("m3u8")).unwrap();
    let text = std::fs::read_to_string(rig.root.path().join(list.as_str())).unwrap();
    let entries: Vec<_> = text.lines().skip(1).collect();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0], entries[2]);
    assert_ne!(entries[0], entries[1]);
    for entry in &entries { assert!(rig.root.path().join("Playlists").join(entry).is_file()); }
    assert_eq!(std::fs::read(rig.root.path().join(entries[1].trim_start_matches("../"))).unwrap(), b"first file");
    let revision = rig.state.revision().unwrap();
    let next = rig.sync(&[a, b], &lists, false);
    assert!(next.actions().is_empty());
    assert_eq!(next.unchanged_files(), 3);
    assert_eq!(rig.state.revision().unwrap(), revision);
}

#[test]
fn retaining_previous_selections_cannot_make_the_manifest_unreadable() {
    let mut rig = Rig::new();
    let mut first = rig.input("first", b"first file");
    first.title = "A".repeat(MAX_METADATA_BYTES / 2 + 1024);
    let previous = rig.sync(&[first.clone()], &[], false);
    let mut second = rig.input("second", b"second file");
    second.title = "B".repeat(MAX_METADATA_BYTES / 2 + 1024);
    let revision = rig.state.revision().unwrap();
    let error = prepare(&rig.state, &mut rig.device, &[second.clone()], &[], false, &Cancellation::default()).err().unwrap();
    assert!(matches!(error, Error::Invalid(_)));
    assert_eq!(rig.state.managed_count().unwrap(), 1);
    assert_eq!(rig.state.revision().unwrap(), revision);
    assert!(!rig.state.interrupted().unwrap());
    assert!(rig.root.path().join(copies(&previous)[0].as_str()).is_file());
    let unchanged = prepare(&rig.state, &mut rig.device, &[first], &[], false, &Cancellation::default()).unwrap();
    assert_eq!(unchanged.unchanged_files(), 1);
    let replacement = rig.sync(&[second], &[], true);
    assert_eq!(replacement.deletion_count(), 1);
    assert_eq!(rig.state.managed_count().unwrap(), 1);
    assert!(!rig.root.path().join(copies(&previous)[0].as_str()).exists());
}

#[test]
fn missing_sources_and_unresolved_playlist_members_never_produce_a_removal_plan() {
    let mut rig = Rig::new();
    let input = rig.input("a", b"audio");
    let previous = rig.sync(&[input.clone()], &[], false);
    std::fs::remove_file(&input.source).unwrap();
    assert!(prepare(&rig.state, &mut rig.device, &[input], &[], true, &Cancellation::default()).is_err());
    assert!(prepare(&rig.state, &mut rig.device, &[], &[playlist(&["missing"])], true, &Cancellation::default()).is_err());
    assert!(rig.root.path().join(copies(&previous)[0].as_str()).is_file());
}

#[test]
fn removals_require_the_exact_preview_and_separate_approval_and_leave_foreign_files() {
    let mut rig = Rig::new();
    let a = rig.input("a", b"audio");
    rig.sync(&[a], &[], false);
    std::fs::write(rig.root.path().join("precious.flac"), b"mine").unwrap();
    let keep = prepare(&rig.state, &mut rig.device, &[], &[], false, &Cancellation::default()).unwrap();
    assert!(keep.actions().is_empty());
    let remove = prepare(&rig.state, &mut rig.device, &[], &[], true, &Cancellation::default()).unwrap();
    assert_eq!(remove.deletion_count(), 1);
    assert!(synchronize(&mut rig.state, &mut rig.device, &remove, "another-preview", true, &Cancellation::default(), &mut |_| ()).is_err());
    assert!(synchronize(&mut rig.state, &mut rig.device, &remove, remove.id(), false, &Cancellation::default(), &mut |_| ()).is_err());
    synchronize(&mut rig.state, &mut rig.device, &remove, remove.id(), true, &Cancellation::default(), &mut |_| ()).unwrap();
    assert_eq!(rig.state.managed_count().unwrap(), 0);
    assert_eq!(std::fs::read(rig.root.path().join("precious.flac")).unwrap(), b"mine");
}

#[test]
fn same_size_device_edits_and_unowned_collisions_are_conflicts() {
    let mut rig = Rig::new();
    let a = rig.input("a", b"original");
    let plan = rig.sync(&[a.clone()], &[], false);
    std::fs::write(rig.root.path().join(copies(&plan)[0].as_str()), b"modified").unwrap();
    assert!(matches!(prepare(&rig.state, &mut rig.device, &[a], &[], true, &Cancellation::default()), Err(Error::Conflict(_))));
    assert!(matches!(prepare(&rig.state, &mut rig.device, &[], &[], true, &Cancellation::default()), Err(Error::Conflict(_))));
    let mut clean = Rig::new();
    let b = clean.input("b", b"other");
    let plan = prepare(&clean.state, &mut clean.device, &[b.clone()], &[], false, &Cancellation::default()).unwrap();
    let path = clean.root.path().join(copies(&plan)[0].as_str());
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"other").unwrap();
    assert!(prepare(&clean.state, &mut clean.device, &[b], &[], false, &Cancellation::default()).is_err());
}

#[test]
fn optional_copy_verification_rejects_changed_bytes_before_publication() {
    let mut rig = Rig::new();
    let input = rig.input("a", b"original");
    let plan = prepare(&rig.state, &mut rig.device, &[input.clone()], &[], false, &Cancellation::default()).unwrap();
    std::fs::write(&input.source, b"modified").unwrap();
    assert!(synchronize_with_verification(&mut rig.state, &mut rig.device, &plan, plan.id(), false, true, &Cancellation::default(), &mut |_| ()).is_err());
    assert!(!rig.root.path().join(copies(&plan)[0].as_str()).exists());
    assert_eq!(rig.state.managed_count().unwrap(), 0);
    assert!(rig.state.interrupted().unwrap());
    recover(&mut rig.state, &mut rig.device, &Cancellation::default()).unwrap();
    assert!(!rig.state.interrupted().unwrap());
}

#[test]
fn replacing_the_storage_or_reusing_a_stale_plan_fails() {
    let mut rig = Rig::new();
    let input = rig.input("a", b"audio");
    let plan = prepare(&rig.state, &mut rig.device, &[input.clone()], &[], false, &Cancellation::default()).unwrap();
    let other_root = tempfile::tempdir().unwrap();
    let mut other = Mounted::open(other_root.path()).unwrap();
    assert!(synchronize(&mut rig.state, &mut other, &plan, plan.id(), false, &Cancellation::default(), &mut |_| ()).is_err());
    rig.sync(&[input], &[], false);
    assert!(synchronize(&mut rig.state, &mut rig.device, &plan, plan.id(), false, &Cancellation::default(), &mut |_| ()).is_err());
    assert!(Mounted::open(rig.root.path()).is_err());
    assert!(State::open(&rig.profile.path().join("sync.db")).is_err());
}

#[test]
fn path_validation_survives_serialization_and_rename_never_replaces() {
    for text in ["../a", "/a", "a//b", "a/./b", "C:/a", "a\\b", "CON.flac", "a. ", "a\n.flac"] {
        assert!(RelativePath::new(text).is_err(), "{text:?}");
        assert!(serde_json::from_str::<RelativePath>(&serde_json::to_string(text).unwrap()).is_err());
    }
    assert_eq!(component("A:B?"), "A_B_");
    assert_eq!(component("con.flac"), "_con.flac");
    let mut rig = Rig::new();
    std::fs::write(rig.root.path().join("a"), b"a").unwrap();
    std::fs::write(rig.root.path().join("b"), b"b").unwrap();
    assert!(rig.device.rename_new(&RelativePath::new("a").unwrap(), &RelativePath::new("b").unwrap()).is_err());
    assert_eq!(std::fs::read(rig.root.path().join("b")).unwrap(), b"b");
}

#[cfg(unix)]
#[test]
fn device_symlinks_cannot_escape_the_selected_root() {
    let mut rig = Rig::new();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), rig.root.path().join("Music")).unwrap();
    let input = rig.input("a", b"audio");
    assert!(prepare(&rig.state, &mut rig.device, &[input], &[], false, &Cancellation::default()).is_err());
    assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
}

struct Faults { inner: Mounted, fail_at: usize, calls: usize, free: Option<u64>, partial: bool }
impl Faults {
    fn after(&mut self) -> Result<()> {
        self.calls += 1;
        if self.calls == self.fail_at { Err(std::io::Error::other("injected disconnect after mutation").into()) } else { Ok(()) }
    }
}
impl Transport for Faults {
    fn available_bytes(&mut self) -> Result<u64> { match self.free { Some(free) => Ok(free), None => self.inner.available_bytes() } }
    fn stat(&mut self, p: &RelativePath) -> Result<Option<Entry>> { self.inner.stat(p) }
    fn create_dir_all(&mut self, p: &RelativePath) -> Result<()> { self.inner.create_dir_all(p)?; self.after() }
    fn read(&mut self, p: &RelativePath, out: &mut dyn Write, c: &Cancellation) -> Result<()> { self.inner.read(p,out,c) }
    fn write_new(&mut self, p: &RelativePath, input: &mut (dyn Read + Send), n: u64, c: &Cancellation, f: &mut (dyn FnMut(u64) + Send)) -> Result<()> {
        if self.partial {
            self.inner.write_new(p, &mut input.take(n/2), n/2, c, f)?;
            return Err(std::io::Error::other("injected partial upload").into());
        }
        self.inner.write_new(p,input,n,c,f)?; self.after()
    }
    fn rename_new(&mut self, a: &RelativePath, b: &RelativePath) -> Result<()> { self.inner.rename_new(a,b)?; self.after() }
    fn remove_file(&mut self, p: &RelativePath) -> Result<()> { self.inner.remove_file(p)?; self.after() }
}

#[test]
fn insufficient_space_and_cancellation_leave_the_device_untouched() {
    let rig = Rig::new();
    let input = rig.input("a", b"audio");
    let mut faults = Faults { inner:rig.device, fail_at:usize::MAX, calls:0, free:Some(SPACE_RESERVE), partial:false };
    assert!(matches!(prepare(&rig.state, &mut faults, &[input.clone()], &[], false, &Cancellation::default()), Err(Error::Space { .. })));
    assert_eq!(faults.calls, 0);
    let cancel = Cancellation::default(); cancel.cancel();
    assert!(matches!(prepare(&rig.state, &mut faults, &[input], &[], false, &cancel), Err(Error::Cancelled)));
    assert_eq!(faults.calls, 0);
}

#[test]
fn recovery_handles_every_disconnect_boundary_and_preserves_valid_playlist_references() {
    for failure in 1..=18 {
        let mut rig = Rig::new();
        let a = rig.input("a", b"old audio");
        rig.sync(&[a.clone()], &[playlist(&["a"])], false);
        let b = rig.input("b", b"new audio");
        let plan = prepare(&rig.state, &mut rig.device, &[a.clone(), b.clone()], &[playlist(&["b","a","b"])], false, &Cancellation::default()).unwrap();
        let Rig { root, source:_source, profile, mut state, device } = rig;
        let mut faulty = Faults { inner:device, fail_at:failure, calls:0, free:None, partial:false };
        let _ = synchronize(&mut state, &mut faulty, &plan, plan.id(), false, &Cancellation::default(), &mut |_| ());
        drop(state); drop(faulty);
        let mut state = State::open(&profile.path().join("sync.db")).unwrap();
        let mut device = Mounted::open(root.path()).unwrap();
        recover(&mut state, &mut device, &Cancellation::default()).unwrap();
        assert!(!state.interrupted().unwrap(), "boundary {failure}");
        for entry in std::fs::read_dir(root.path().join("Playlists")).unwrap() {
            let path = entry.unwrap().path();
            let text = std::fs::read_to_string(&path).unwrap();
            for entry in text.lines().skip(1) { assert!(path.parent().unwrap().join(entry).is_file(), "boundary {failure}: {entry}"); }
        }
        assert!(root.path().join("Music").is_dir());
    }
}

#[test]
fn partial_upload_recovery_discards_only_the_journaled_temporary_file() {
    let rig = Rig::new();
    let input = rig.input("a", b"audio bytes to interrupt");
    let mut state = rig.state;
    let mut faults = Faults { inner:rig.device, fail_at:usize::MAX, calls:0, free:None, partial:true };
    let plan = prepare(&state, &mut faults, &[input], &[], false, &Cancellation::default()).unwrap();
    assert!(synchronize(&mut state, &mut faults, &plan, plan.id(), false, &Cancellation::default(), &mut |_| ()).is_err());
    assert_eq!(recover(&mut state, &mut faults, &Cancellation::default()).unwrap(), 1);
    assert_eq!(state.managed_count().unwrap(), 0);
    assert!(!rig.root.path().join(copies(&plan)[0].as_str()).exists());
}

#[test]
fn interrupted_removals_never_leave_a_surviving_playlist_with_missing_media() {
    for failure in 1..=12 {
        let mut rig = Rig::new();
        let input = rig.input("a", b"old audio");
        rig.sync(&[input], &[playlist(&["a"])], false);
        let plan = prepare(&rig.state, &mut rig.device, &[], &[], true, &Cancellation::default()).unwrap();
        assert!(matches!(&plan.actions()[0], Action::Remove { path, .. } if path.as_str().starts_with("Playlists/")));
        let mut faults = Faults { inner:rig.device, fail_at:failure, calls:0, free:None, partial:false };
        let _ = synchronize(&mut rig.state, &mut faults, &plan, plan.id(), true, &Cancellation::default(), &mut |_| ());
        // I inspect the real intermediate state before reconciliation, as well as its recovered state.
        for entry in std::fs::read_dir(rig.root.path().join("Playlists")).unwrap() {
            let path = entry.unwrap().path();
            for line in std::fs::read_to_string(&path).unwrap().lines().skip(1) {
                assert!(path.parent().unwrap().join(line).exists(), "before recovery at boundary {failure}");
            }
        }
        recover(&mut rig.state, &mut faults, &Cancellation::default()).unwrap();
        for entry in std::fs::read_dir(rig.root.path().join("Playlists")).unwrap() {
            let path = entry.unwrap().path();
            for line in std::fs::read_to_string(&path).unwrap().lines().skip(1) {
                assert!(path.parent().unwrap().join(line).exists(), "after recovery at boundary {failure}");
            }
        }
    }
}

#[test]
fn an_approved_removal_can_free_a_full_card_without_an_upload_reserve() {
    let mut rig = Rig::new();
    let input = rig.input("a", b"audio");
    rig.sync(&[input], &[], false);
    let mut device = Faults { inner:rig.device, fail_at:usize::MAX, calls:0, free:Some(0), partial:false };
    let plan = prepare(&rig.state, &mut device, &[], &[], true, &Cancellation::default()).unwrap();
    assert_eq!(plan.required_bytes(), 0);
    assert_eq!(plan.deletion_count(), 1);
    synchronize(&mut rig.state, &mut device, &plan, plan.id(), true, &Cancellation::default(), &mut |_| ()).unwrap();
    assert_eq!(rig.state.managed_count().unwrap(), 0);
}

struct ReviewMeter<'a> { inner: &'a mut dyn Transport, media_bytes: u64, reads: usize, stats: usize, reviewing: bool }
impl Transport for ReviewMeter<'_> {
    fn begin_review(&mut self) { self.reviewing = true; self.inner.begin_review(); }
    fn end_review(&mut self) { self.reviewing = false; self.inner.end_review(); }
    fn available_bytes(&mut self) -> Result<u64> { self.inner.available_bytes() }
    fn stat(&mut self, path: &RelativePath) -> Result<Option<Entry>> { self.stats += 1; self.inner.stat(path) }
    fn read(&mut self, path: &RelativePath, out: &mut dyn Write, cancel: &Cancellation) -> Result<()> {
        struct Counter<'a> { out: &'a mut dyn Write, count: &'a mut u64 }
        impl Write for Counter<'_> {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                let written = self.out.write(bytes)?;
                *self.count += written as u64;
                Ok(written)
            }
            fn flush(&mut self) -> std::io::Result<()> { self.out.flush() }
        }
        self.reads += 1;
        if path.as_str().starts_with("Music/") || path.as_str().starts_with("Playlists/") {
            self.inner.read(path, &mut Counter { out, count:&mut self.media_bytes }, cancel)
        } else { self.inner.read(path, out, cancel) }
    }
    fn create_dir_all(&mut self, p: &RelativePath) -> Result<()> { self.inner.create_dir_all(p) }
    fn write_new(&mut self, p: &RelativePath, input: &mut (dyn Read + Send), size: u64, c: &Cancellation, f: &mut (dyn FnMut(u64) + Send)) -> Result<()> {
        self.inner.write_new(p,input,size,c,f)
    }
    fn rename_new(&mut self, a: &RelativePath, b: &RelativePath) -> Result<()> { self.inner.rename_new(a,b) }
    fn remove_file(&mut self, p: &RelativePath) -> Result<()> { self.inner.remove_file(p) }
}

#[test]
fn metadata_review_reads_no_device_media_and_reuses_unchanged_local_hashes() {
    let mut rig = Rig::new();
    let inputs: Vec<_> = (0..12).map(|i| rig.input(&format!("track-{i}"), &vec![i as u8; 1024*1024])).collect();
    rig.sync(&inputs, &[], false);
    let mut meter = ReviewMeter { inner:&mut rig.device, media_bytes:0, reads:0, stats:0, reviewing:false };
    let started = std::time::Instant::now();
    let full = prepare(&rig.state, &mut meter, &inputs, &[], false, &Cancellation::default()).unwrap();
    let full_time = started.elapsed();
    assert_eq!(meter.media_bytes, 12*1024*1024);
    assert_eq!(full.source_bytes_hashed(), 12*1024*1024);
    meter.media_bytes = 0;
    meter.reads = 0;
    meter.stats = 0;
    let started = std::time::Instant::now();
    let review = prepare_review(&rig.state, &mut meter, &inputs, &[], false, &Cancellation::default()).unwrap();
    assert!(review.is_metadata_review());
    assert_eq!(review.unchanged_files(), 12);
    assert!(review.actions().is_empty());
    assert_eq!(meter.media_bytes, 0);
    assert_eq!(meter.reads, 1); // Only the bounded identity marker is read.
    assert_eq!(meter.stats, 13);
    assert!(!meter.reviewing);
    #[cfg(unix)] assert_eq!(review.source_bytes_hashed(), 0);
    eprintln!("Review fixture: tracks=12 media_bytes=12582912 full_device_bytes=12582912 review_device_bytes={} full_source_bytes={} review_source_bytes={} review_stats={} full_us={} review_us={}",
        meter.media_bytes, full.source_bytes_hashed(), review.source_bytes_hashed(), meter.stats,
        full_time.as_micros(), started.elapsed().as_micros());
}

#[test]
fn review_caches_do_not_hide_source_edits_or_atomic_replacements() {
    let mut rig = Rig::new();
    let input = rig.input("track", b"original");
    rig.sync(&[input.clone()], &[], false);
    let modified = std::fs::metadata(&input.source).unwrap().modified().unwrap();
    std::fs::write(&input.source, b"modified").unwrap();
    std::fs::File::options().write(true).open(&input.source).unwrap().set_modified(modified).unwrap();
    let changed = prepare_review(&rig.state, &mut rig.device, &[input.clone()], &[], false, &Cancellation::default()).unwrap();
    assert_eq!(changed.source_bytes_hashed(), 8);
    assert_eq!(copies(&changed).len(), 1);
    let replacement = input.source.with_extension("replacement");
    std::fs::write(&replacement, b"replaced").unwrap();
    std::fs::File::options().write(true).open(&replacement).unwrap().set_modified(modified).unwrap();
    std::fs::remove_file(&input.source).unwrap();
    std::fs::rename(replacement, &input.source).unwrap();
    let replaced = prepare_review(&rig.state, &mut rig.device, &[input], &[], false, &Cancellation::default()).unwrap();
    assert_eq!(replaced.source_bytes_hashed(), 8);
    assert_ne!(copies(&changed), copies(&replaced));
}

#[test]
fn approved_removals_do_not_read_audio_even_when_copy_verification_is_enabled() {
    let mut rig = Rig::new();
    let input = rig.input("track", b"original");
    let original = rig.sync(&[input.clone()], &[], false);
    let path = rig.root.path().join(copies(&original)[0].as_str());
    std::fs::write(&path, b"modified").unwrap();
    let present = prepare_review(&rig.state, &mut rig.device, &[input], &[], false, &Cancellation::default()).unwrap();
    assert_eq!(present.unchanged_files(), 1);
    assert!(present.is_metadata_review());
    let removal = prepare_review(&rig.state, &mut rig.device, &[], &[], true, &Cancellation::default()).unwrap();
    assert_eq!(removal.deletion_count(), 1);
    let mut meter = ReviewMeter { inner:&mut rig.device, media_bytes:0, reads:0, stats:0, reviewing:false };
    let mut events = Vec::new();
    synchronize_with_verification(&mut rig.state, &mut meter, &removal, removal.id(), true, true, &Cancellation::default(), &mut |event| events.push(event)).unwrap();
    assert_eq!(meter.media_bytes, 0, "deletion must not download managed audio");
    assert!(events.iter().any(|event| event.completed_files == 0 && !event.current_path.is_empty()));
    assert!(events.iter().all(|event| !event.verifying));
    assert!(!rig.state.interrupted().unwrap());
    assert!(!path.exists());
}

#[test]
fn failed_or_cancelled_reviews_release_snapshots_and_do_not_publish_partial_caches() {
    let mut rig = Rig::new();
    let input = rig.input("track", b"original");
    let mut missing = input.clone();
    missing.id = "missing".into();
    missing.source = input.source.with_extension("absent");
    let mut meter = ReviewMeter { inner:&mut rig.device, media_bytes:0, reads:0, stats:0, reviewing:false };
    assert!(prepare_review(&rig.state, &mut meter, &[input.clone(),missing], &[], false, &Cancellation::default()).is_err());
    assert!(!meter.reviewing);
    let cancelled = Cancellation::default(); cancelled.cancel();
    assert!(matches!(prepare_review(&rig.state, &mut meter, &[input.clone()], &[], false, &cancelled), Err(Error::Cancelled)));
    assert!(!meter.reviewing);
    let plan = prepare_review(&rig.state, &mut meter, &[input.clone()], &[], false, &Cancellation::default()).unwrap();
    assert_eq!(plan.source_bytes_hashed(), 8);
    let mut other = Rig::new();
    let plan = prepare_review(&other.state, &mut other.device, &[input], &[], false, &Cancellation::default()).unwrap();
    assert_eq!(plan.source_bytes_hashed(), 8);
    assert!(prepare_review(&rig.state, &mut other.device, &[], &[], false, &Cancellation::default()).is_err());
}

#[test]
fn metadata_review_rejects_foreign_and_resized_files_before_writing() {
    let mut rig = Rig::new();
    let a = rig.input("a", b"audio");
    let b = rig.input("b", b"music");
    let initial = rig.sync(&[a.clone()], &[playlist(&["a"])], false);
    let playlist_path = copies(&initial).into_iter().find(|p| p.as_str().starts_with("Playlists/")).unwrap();
    let actual = rig.root.path().join(playlist_path.as_str());
    let review = prepare_review(&rig.state, &mut rig.device, &[a.clone(), b.clone()], &[playlist(&["b","a"])], false, &Cancellation::default()).unwrap();
    let original = std::fs::read(&actual).unwrap();
    let new_path = copies(&review).into_iter().find(|p| p.as_str().starts_with("Music/")).unwrap();
    let foreign = rig.root.path().join(new_path.as_str());
    std::fs::create_dir_all(foreign.parent().unwrap()).unwrap();
    std::fs::write(&foreign, b"music").unwrap();
    assert!(synchronize(&mut rig.state, &mut rig.device, &review, review.id(), false, &Cancellation::default(), &mut |_| ()).is_err());
    assert!(!rig.state.interrupted().unwrap());
    assert_eq!(std::fs::read(&actual).unwrap(), original);
    assert!(prepare_review(&rig.state, &mut rig.device, &[a.clone(),b], &[], false, &Cancellation::default()).is_err());
    std::fs::write(&actual, b"shorter").unwrap();
    assert!(prepare_review(&rig.state, &mut rig.device, &[a], &[playlist(&["a"])], false, &Cancellation::default()).is_err());
}

#[test]
fn copies_default_to_no_readback_and_verify_once_when_requested() {
    for verify in [false, true] {
        let mut rig = Rig::new();
        let input = rig.input("track", &vec![7; 1024 * 1024]);
        let mut meter = ReviewMeter { inner:&mut rig.device, media_bytes:0, reads:0, stats:0, reviewing:false };
        let plan = prepare_review(&rig.state, &mut meter, &[input], &[], false, &Cancellation::default()).unwrap();
        assert_eq!(meter.media_bytes, 0);
        let mut events = Vec::new();
        if verify {
            synchronize_with_verification(&mut rig.state, &mut meter, &plan, plan.id(), false, true, &Cancellation::default(), &mut |event| events.push(event)).unwrap();
        } else {
            synchronize(&mut rig.state, &mut meter, &plan, plan.id(), false, &Cancellation::default(), &mut |event| events.push(event)).unwrap();
        }
        assert_eq!(meter.media_bytes, if verify {1024 * 1024} else {0});
        assert_eq!(events.iter().any(|event| event.verifying && event.transferred_bytes > 0), verify);
        assert_eq!(rig.state.managed_count().unwrap(), 1);
        assert!(!rig.state.interrupted().unwrap());
    }
}

#[cfg(unix)]
#[test]
fn local_review_cache_survives_reopening_only_its_own_profile() {
    let mut rig = Rig::new();
    let input = rig.input("track", b"original");
    let first = prepare_review(&rig.state, &mut rig.device, &[input.clone()], &[], false, &Cancellation::default()).unwrap();
    assert_eq!(first.source_bytes_hashed(), 8);
    let Rig { root:_root, source:_source, profile, state, mut device } = rig;
    drop(state);
    let reopened = State::open(&profile.path().join("sync.db")).unwrap();
    let next = prepare_review(&reopened, &mut device, &[input], &[], false, &Cancellation::default()).unwrap();
    assert_eq!(next.source_bytes_hashed(), 0);
    assert_eq!(copies(&first), copies(&next));
}
