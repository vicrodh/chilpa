//! The Folders and library routes use every track of an image.

#[path = "support/sacd.rs"]
mod sacd;

use chilpa_api::Chilpa;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn scan(api: &mut Chilpa, root: &Path) {
    api.start_scan(root);
    let limit = Instant::now() + Duration::from_secs(120);
    while api.scan_status().running {
        assert!(Instant::now() < limit, "the background scan did not finish");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!api.scan_status().summary.unreachable);
    assert_eq!(api.scan_status().summary.unreadable, 0);
}

fn check(api: &mut Chilpa, root: &Path, images: &[PathBuf]) -> usize {
    let reader = api.reader().unwrap();
    let mut expected = Vec::new();
    for image in images {
        let reference = chilpa_core::sacd_tracks::disc_images().tracks(image).unwrap();
        assert!(!reference.is_empty());
        let direct = reader.folder(&image.to_string_lossy());
        assert_eq!(direct.tracks.len(), reference.len());
        let folder = reader.folder(&image.parent().unwrap().to_string_lossy());
        assert_eq!(folder.tracks.len(), reference.len());
        for (row, track) in direct.tracks.iter().zip(&reference) {
            assert!(row.available, "{}", row.path);
            assert_eq!(row.path, track.path);
            assert_eq!(row.title, track.info.tags.title.clone().unwrap());
            assert_eq!(row.number, track.info.tags.track_number.unwrap());
            assert_eq!(row.duration_ms, track.info.duration.as_millis() as u64);
            assert_eq!(row.quality.tier, chilpa_api::Tier::Dsd);
        }
        assert_eq!(reader.folder_files(&image.to_string_lossy()), reference.iter().map(|t| t.path.clone()).collect::<Vec<_>>());
        expected.extend(reference.into_iter().map(|t| t.path));
    }
    drop(reader);
    scan(api, root);
    for image in images {
        let reader = api.reader().unwrap();
        let direct = reader.folder(&image.to_string_lossy());
        assert!(direct.in_library);
        assert!(direct.tracks.iter().all(|t| t.id > 0 && t.available));
        let parent = reader.folder(&image.parent().unwrap().to_string_lossy());
        assert_eq!(parent.tracks, direct.tracks);
        assert_eq!(parent.total, direct.tracks.len());
        assert_eq!(reader.folder_files(&image.parent().unwrap().to_string_lossy()), direct.tracks.iter().map(|t| t.path.clone()).collect::<Vec<_>>());
        let album = direct.tracks[0].album_id;
        let album_tracks = api.album_tracks(album);
        assert!(direct.tracks.iter().all(|row| album_tracks.iter().any(|saved| saved.path == row.path)));
        eprintln!("SACD: {} tracks | {}", direct.tracks.len(), image.display());
    }
    let reader = api.reader().unwrap();
    assert_eq!(reader.folder(&root.to_string_lossy()).total, expected.len());
    assert_eq!(reader.folder_files(&root.to_string_lossy()).len(), expected.len());
    assert_eq!(reader.folder_matches("disc").folders.iter().filter(|f| Path::new(&f.path).starts_with(root)).count(), images.len());
    drop(reader);
    scan(api, root);
    assert_eq!(api.scan_status().summary.unchanged, expected.len());
    assert_eq!(api.reader().unwrap().folder_files(&root.to_string_lossy()).len(), expected.len());
    expected.len()
}

#[test]
fn opening_and_scanning_images_keep_the_complete_tracklist() {
    let profile = tempfile::tempdir().unwrap();
    for (key, sub) in [("XDG_DATA_HOME", "data"), ("XDG_CONFIG_HOME", "config"), ("XDG_CACHE_HOME", "cache")] {
        std::env::set_var(key, profile.path().join(sub));
    }
    let root = profile.path().join("music");
    let folder = root.join("disc #1 á");
    std::fs::create_dir_all(&folder).unwrap();
    let image = folder.join("Album #1.ISO");
    sacd::image(&image);
    drop(chilpa_library::Library::open(&chilpa_library::Library::default_path()).unwrap());
    let mut api = Chilpa::new();
    check(&mut api, &root, &[image]);

    // Optional read-only smoke of owned images. Only links and the test
    // database are written; the original music is never changed.
    #[cfg(unix)]
    if let Some(real) = std::env::var_os("CHILPA_TEST_SACD_ROOT") {
        let root = profile.path().join("real");
        let mut pending = vec![PathBuf::from(real)];
        let mut images = Vec::new();
        while let Some(dir) = pending.pop() {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() { pending.push(path); }
                else if chilpa_core::sacd_tracks::is_image(&path) && chilpa_core::sacd_tracks::disc_images().tracks(&path).is_some() {
                    let folder = root.join(format!("disc {:03}", images.len()));
                    std::fs::create_dir_all(&folder).unwrap();
                    let link = folder.join(path.file_name().unwrap());
                    std::os::unix::fs::symlink(&path, &link).unwrap();
                    images.push(link);
                }
            }
        }
        assert!(!images.is_empty(), "the real-image smoke must read at least one SACD");
        // The first synthetic root remains in the library; matching this
        // batch is checked by its own root instead of all library folders.
        let tracks = check(&mut api, &root, &images);
        if let Some(report) = std::env::var_os("CHILPA_TEST_SACD_REPORT") {
            std::fs::write(report, format!("images={} tracks={} opened=ok scanned=ok rescanned=ok\n", images.len(), tracks)).unwrap();
        }
    }
}
