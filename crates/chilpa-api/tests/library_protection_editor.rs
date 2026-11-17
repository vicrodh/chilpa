//! Protect my library, through the metadata editor, end to end.
//!
//! One test on purpose: it points the library, the settings and the cache
//! at a temporary directory, and those belong to the whole process.

use chilpa_api::Chilpa;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

fn tagged(target: &Path, title: &str, track: u32, album_artist: bool) -> bool {
    let _ = std::fs::create_dir_all(target.parent().unwrap());
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-y", "-loglevel", "error", "-f", "lavfi", "-i", "sine=frequency=440:duration=1", "-c:a", "flac"])
        .args(["-metadata", &format!("title={title}")])
        .args(["-metadata", "artist=Someone"])
        .args(["-metadata", "album=A record"])
        .args(["-metadata", "genre=Rock"])
        .args(["-metadata", "date=1990-05-01"])
        .args(["-metadata", &format!("track={track}")]);
    if album_artist {
        cmd.args(["-metadata", "album_artist=Someone"]);
    }
    cmd.arg(target).output().map(|o| o.status.success()).unwrap_or(false)
}

/// Every file under a folder that is not one of the editor's own, with
/// its bytes.
fn audio(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in std::fs::read_dir(next).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|e| e == "flac") {
                out.push((path.clone(), std::fs::read(&path).unwrap()));
            }
        }
    }
    out.sort();
    out
}


fn until(chilpa: &mut Chilpa, state: &str) {
    let limit = Instant::now() + Duration::from_secs(20);
    while Instant::now() < limit {
        if chilpa.metadata_poll().state == state {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("the editor never got to \"{state}\": {:?}", chilpa.metadata_view());
}


fn album_titled(chilpa: &mut Chilpa, title: &str) -> i64 {
    chilpa.albums_sorted(chilpa_api::AlbumSort::Title, 100, 0).into_iter().find(|a| a.title == title).map(|a| a.id).unwrap_or(0)
}

#[test]
fn a_protected_library_keeps_every_file_and_allows_only_permitted_sidecars() {
    let dir = std::env::temp_dir().join("chilpa-api-library-protection");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("XDG_DATA_HOME", dir.join("data"));
    std::env::set_var("XDG_CONFIG_HOME", dir.join("config"));
    std::env::set_var("XDG_CACHE_HOME", dir.join("cache"));
    let music = dir.join("music");
    if !tagged(&music.join("A record/01.flac"), "One", 1, true) {
        eprintln!("no ffmpeg: skipped");
        return;
    }
    let mut chilpa = Chilpa::new();
    chilpa.scan(&music);
    let album = album_titled(&mut chilpa, "A record");
    assert_ne!(album, 0);
    let before = audio(&music);
    assert!(chilpa.adjust("library.protect", "true"));

    // Protected, no sidecars: nothing can be saved.
    assert!(chilpa.metadata_open(album, 0));
    until(&mut chilpa, "ready");
    let save = chilpa.metadata_view().save.clone();
    assert!(save.protected && !save.sidecar_allowed);
    assert!(!chilpa.metadata_set_mode("files"), "the files are protected");
    assert!(!chilpa.metadata_set_mode("sidecar"), "sidecars are not allowed");
    assert!(chilpa.metadata_set_album("title", "Changed"));
    assert!(!chilpa.metadata_save());
    assert!(chilpa.metadata_view().error.contains("Protect my library"), "{}", chilpa.metadata_view().error);
    assert!(!music.join("A record/.chilpa.json").exists());
    chilpa.metadata_close();

    // Sidecars allowed: the edit lands beside the music, the audio does not change.
    assert!(chilpa.adjust("library.protect.sidecars", "true"));
    assert!(chilpa.metadata_open(album, 0));
    until(&mut chilpa, "ready");
    assert_eq!(chilpa.metadata_view().save.mode, "sidecar");
    assert!(!chilpa.metadata_set_mode("files"));
    assert!(chilpa.metadata_set_album("title", "Changed"));
    assert!(chilpa.metadata_save());
    until(&mut chilpa, "closed");
    assert!(chilpa.metadata_view().error.is_empty(), "{}", chilpa.metadata_view().error);
    assert!(music.join("A record/.chilpa.json").is_file());
    assert_eq!(audio(&music), before, "not one byte of audio changed");

    // Off again: the files can be chosen.
    assert!(chilpa.adjust("library.protect", "false"));
    let album = album_titled(&mut chilpa, "Changed");
    assert!(chilpa.metadata_open(album, 0));
    until(&mut chilpa, "ready");
    assert!(!chilpa.metadata_view().save.protected);
    assert!(chilpa.metadata_set_mode("files"));
}
