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
