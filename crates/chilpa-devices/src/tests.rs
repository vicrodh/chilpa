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
fn changed_source_after_preview_never_reaches_a_published_path() {
    let mut rig = Rig::new();
    let input = rig.input("a", b"original");
    let plan = prepare(&rig.state, &mut rig.device, &[input.clone()], &[], false, &Cancellation::default()).unwrap();
    std::fs::write(&input.source, b"modified").unwrap();
    assert!(synchronize(&mut rig.state, &mut rig.device, &plan, plan.id(), false, &Cancellation::default(), &mut |_| ()).is_err());
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
