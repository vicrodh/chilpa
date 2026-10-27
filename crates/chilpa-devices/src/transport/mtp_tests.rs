use super::*;
use mtp_rs::{register_virtual_device, unregister_virtual_device, VirtualDeviceConfig, VirtualStorageConfig};
use mtp_rs::transport::virtual_device::registry::{rekey_virtual_object, rescan_virtual_device};

struct Fixture { directory:tempfile::TempDir, location:u64, serial:String }
impl Fixture {
    fn new(partial:bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(directory.path().join("Sync/Music/Artist/Album")).unwrap();
        std::fs::write(directory.path().join("Sync/Music/Artist/Album/a.flac"), b"first").unwrap();
        let serial = format!("fixture-{}", directory.path().display());
        let config = VirtualDeviceConfig {
            serial:serial.clone(), watch_backing_dirs:false, event_poll_interval:Duration::ZERO,
            supports_partial_object:partial, supports_partial_object_64:partial,
            storages:vec![VirtualStorageConfig { description:"Card".into(), capacity:1024*1024*1024,
                backing_dir:directory.path().to_path_buf(),read_only:false }],
            ..Default::default()
        };
        let info = register_virtual_device(&config);
        Self { directory, location:info.location_id, serial }
    }
    fn open(&self) -> Mtp {
        let volume = volumes(self.location).unwrap().remove(0).id;
        Mtp::open(self.location, volume, RelativePath::new("Sync").unwrap()).unwrap()
    }
    fn file(&self) -> std::path::PathBuf { self.directory.path().join("Sync/Music/Artist/Album/a.flac") }
}
impl Drop for Fixture { fn drop(&mut self) { unregister_virtual_device(self.location); } }
fn song() -> RelativePath { RelativePath::new("Music/Artist/Album/a.flac").unwrap() }

#[test]
fn cached_ancestors_do_not_hide_same_size_media_edits() {
    for partial in [true,false] {
        let fixture = Fixture::new(partial);
        let mut device = fixture.open();
        let first = device.fingerprint(&song(), &Cancellation::default()).unwrap();
        assert!(!device.folders.borrow().is_empty());
        std::fs::write(fixture.file(), b"other").unwrap();
        rescan_virtual_device(&fixture.serial).unwrap();
        let second = device.fingerprint(&song(), &Cancellation::default()).unwrap();
        assert_ne!(first, second);
        assert_eq!(second.unwrap().size, 5);
    }
}

#[test]
fn rekeyed_folder_handles_are_resolved_again_within_the_same_session() {
    let fixture = Fixture::new(true);
    let mut device = fixture.open();
    assert!(device.stat(&song()).unwrap().is_some());
    rekey_virtual_object(&fixture.serial, std::path::Path::new("Sync/Music/Artist")).unwrap();
    assert!(device.stat(&song()).unwrap().is_some());
}

#[test]
fn moving_a_cached_folder_never_recreates_the_original_path() {
    let fixture = Fixture::new(true);
    let mut device = fixture.open();
    assert!(device.stat(&song()).unwrap().is_some());
    let old = fixture.directory.path().join("Sync/Music/Artist");
    let moved = fixture.directory.path().join("Sync/Music/Moved");
    std::fs::rename(&old, &moved).unwrap();
    rescan_virtual_device(&fixture.serial).unwrap();
    assert!(!matches!(device.stat(&song()), Ok(Some(_))));
    assert!(!old.exists());
    assert_eq!(std::fs::read(moved.join("Album/a.flac")).unwrap(),b"first");
}

#[test]
fn a_cached_folder_is_revalidated_when_it_is_the_requested_path() {
    let fixture = Fixture::new(true);
    let device = fixture.open();
    let path = RelativePath::new("Sync/Music/Artist/Album").unwrap();
    assert!(device.locate(&path).unwrap().is_some());
    let original = fixture.directory.path().join(path.as_str());
    std::fs::rename(&original, original.with_file_name("Moved album")).unwrap();
    rescan_virtual_device(&fixture.serial).unwrap();
    assert!(!matches!(device.locate(&path), Ok(Some(_))));
    assert!(!original.exists());
}

#[test]
fn folder_references_stay_bounded_across_many_albums() {
    let fixture = Fixture::new(true);
    for index in 0..FOLDER_CACHE_LIMIT {
        let folder = fixture.directory.path().join(format!("Sync/Music/Artist/Album-{index}"));
        std::fs::create_dir(&folder).unwrap();
        std::fs::write(folder.join("a.flac"),b"file").unwrap();
    }
    let mut device = fixture.open();
    for index in 0..FOLDER_CACHE_LIMIT {
        let path = RelativePath::new(format!("Music/Artist/Album-{index}/a.flac")).unwrap();
        assert!(device.stat(&path).unwrap().is_some());
        assert!(device.folders.borrow().len() <= FOLDER_CACHE_LIMIT);
    }
}

#[test]
fn review_lists_each_folder_once_and_releases_its_snapshot() {
    let fixture = Fixture::new(true);
    for index in 0..64 {
        std::fs::write(fixture.file().with_file_name(format!("track-{index}.flac")), b"audio").unwrap();
    }
    let mut device = fixture.open();
    device.listing_calls.set(0);
    device.begin_review();
    let started = std::time::Instant::now();
    for index in 0..64 {
        let path = RelativePath::new(format!("Music/Artist/Album/track-{index}.flac")).unwrap();
        assert_eq!(device.stat(&path).unwrap().unwrap().size, 5);
    }
    let review_calls = device.listing_calls.get();
    let review_time = started.elapsed();
    assert_eq!(review_calls, 5);
    device.end_review();
    assert!(device.review.borrow().is_none());
    assert!(device.folders.borrow().is_empty());
    device.listing_calls.set(0);
    let started = std::time::Instant::now();
    for index in 0..64 {
        let path = RelativePath::new(format!("Music/Artist/Album/track-{index}.flac")).unwrap();
        assert_eq!(device.stat(&path).unwrap().unwrap().size, 5);
    }
    eprintln!("MTP review fixture: tracks=64 review_listings={review_calls} fresh_listings={} review_us={} fresh_us={}",
        device.listing_calls.get(), review_time.as_micros(), started.elapsed().as_micros());
    assert_eq!(device.listing_calls.get(), 68);
}

#[test]
fn review_absence_is_not_reused_by_upload_or_removal() {
    let fixture = Fixture::new(true);
    let mut device = fixture.open();
    let path = RelativePath::new("Music/Artist/Album/new.flac").unwrap();
    device.begin_review();
    assert!(device.stat(&path).unwrap().is_none());
    let actual = fixture.file().with_file_name("new.flac");
    std::fs::write(&actual, b"foreign").unwrap();
    rescan_virtual_device(&fixture.serial).unwrap();
    assert!(device.write_new(&path, &mut b"ours".as_slice(), 4, &Cancellation::default(), &mut |_| ()).is_err());
    assert_eq!(std::fs::read(&actual).unwrap(), b"foreign");
    assert!(device.review.borrow().is_none());
    device.begin_review();
    assert!(device.stat(&song()).unwrap().is_some());
    device.end_review();
    std::fs::write(fixture.file(), b"other").unwrap();
    rescan_virtual_device(&fixture.serial).unwrap();
    let mut bytes = Vec::new();
    device.read(&song(), &mut bytes, &Cancellation::default()).unwrap();
    assert_eq!(bytes, b"other");
}

#[test]
fn discovery_matches_the_card_marker_without_picking_the_internal_volume() {
    let directory = tempfile::tempdir().unwrap();
    let internal = directory.path().join("internal");
    let card = directory.path().join("card");
    std::fs::create_dir(&internal).unwrap();
    std::fs::create_dir_all(card.join("Chilpa/.chilpa-sync")).unwrap();
    let target = "c".repeat(32);
    std::fs::write(card.join("Chilpa/.chilpa-sync/identity.json"), format!(r#"{{"version":1,"id":"{target}","name":"Card"}}"#)).unwrap();
    let config = VirtualDeviceConfig {
        serial:format!("discovery-{}", directory.path().display()), watch_backing_dirs:false, event_poll_interval:Duration::ZERO,
        storages:vec![VirtualStorageConfig { description:"Internal".into(), capacity:1024*1024, backing_dir:internal.clone(), read_only:false },
            VirtualStorageConfig { description:"MicroSD".into(), capacity:1024*1024, backing_dir:card.clone(), read_only:false }],
        ..Default::default()
    };
    let device = register_virtual_device(&config);
    let result = describe_identities(device.location_id, &[RelativePath::new("Chilpa").unwrap()], &Cancellation::default());
    unregister_virtual_device(device.location_id);
    let (description, identities) = result.unwrap();
    assert_eq!(description.volumes.len(), 2);
    assert_eq!(identities, vec![(description.volumes[1].id, "Chilpa".into(), target)]);
    assert!(!internal.join("Chilpa").exists());
    assert!(!card.join("Chilpa/.chilpa-sync/lock").exists());
}

#[test]
fn cancellation_stops_review_enumeration_and_a_new_job_can_reopen_the_snapshot() {
    let fixture = Fixture::new(true);
    let mut device = fixture.open();
    let cancel = Cancellation::default();
    device.set_cancellation(&cancel);
    device.begin_review();
    cancel.cancel();
    let before = device.listing_calls.get();
    assert!(matches!(device.stat(&song()), Err(crate::Error::Cancelled)));
    assert_eq!(device.listing_calls.get(), before);
    device.end_review();
    device.set_cancellation(&Cancellation::default());
    device.begin_review();
    assert!(device.stat(&song()).unwrap().is_some());
    device.end_review();
}
