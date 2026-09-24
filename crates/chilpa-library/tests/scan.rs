//! Scanning for real: incremental, and deleting nothing it cannot see.

use chilpa_library::{scan_root, Library};
use std::path::PathBuf;
use std::process::Command;

fn hay_ffmpeg() -> bool {
    Command::new("ffmpeg").arg("-version").output().map(|o| o.status.success()).unwrap_or(false)
}

/// A test album on disk.
fn album(name: &str, tracks: usize) -> Option<PathBuf> {
    if !hay_ffmpeg() {
        return None;
    }
    let mut dir = std::env::temp_dir();
    dir.push(format!("chilpa-scan-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("disco 1")).ok()?;
    for i in 1..=tracks {
        let target = dir.join("disco 1").join(format!("{i:02}-track.flac"));
        let ok = Command::new("ffmpeg")
            .args(["-y", "-loglevel", "error", "-f", "lavfi", "-i", "sine=frequency=440:duration=1", "-c:a", "flac"])
            .args(["-metadata", &format!("title=Track {i}")])
            .args(["-metadata", "artist=Artist"])
            .args(["-metadata", "album=Disco"])
            .args(["-metadata", &format!("track={i}")])
            .args(["-metadata", "genre=Rock"])
            .arg(&target)
            .output()
            .ok()?
            .status
            .success();
        if !ok {
            return None;
        }
    }
    // Something that is not audio, to check it does not get in.
    std::fs::write(dir.join("notes.txt"), b"hi").ok()?;
    Some(dir)
}

#[test]
fn a_scan_reads_the_tags_and_builds_the_album() {
    let Some(dir) = album("basic", 3) else {
        eprintln!("no ffmpeg: skipped");
        return;
    };
    let lib = Library::in_memory().unwrap();
    let report = scan_root(&lib, &dir).unwrap();

    assert_eq!(report.seen, 3, "the .txt does not count");
    assert_eq!(report.read, 3);
    assert_eq!(lib.track_count(false).unwrap(), 3);

    let albums = lib.albums(10, 0).unwrap();
    assert_eq!(albums.len(), 1);
    assert_eq!(albums[0].title, "Disco");
    assert_eq!(albums[0].album_artist, "Artist");
    assert_eq!(albums[0].track_count, 3);

    let tracks = lib.album_tracks(albums[0].id).unwrap();
    assert_eq!(tracks.len(), 3);
    assert_eq!(tracks[0].title, "Track 1", "they come in the disc's order");
    assert_eq!(tracks[2].title, "Track 3");
    assert!(tracks[0].duration_ms > 800, "it lasted {} ms", tracks[0].duration_ms);

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_second_scan_does_not_reread_what_did_not_change() {
    let Some(dir) = album("incremental", 4) else {
        eprintln!("no ffmpeg: skipped");
        return;
    };
    let lib = Library::in_memory().unwrap();
    scan_root(&lib, &dir).unwrap();
    let second = scan_root(&lib, &dir).unwrap();

    assert_eq!(second.read, 0, "nothing changed, nothing was reread");
    assert_eq!(second.unchanged, 4);
    assert_eq!(lib.track_count(false).unwrap(), 4, "and the library stayed the same");

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_root_that_disappears_marks_its_tracks_but_does_not_delete_them() {
    // The audit's high finding: an unmounted mountpoint looked empty and
    // the prune deleted all its tracks.
    let Some(dir) = album("unmounted", 2) else {
        eprintln!("no ffmpeg: skipped");
        return;
    };
    let lib = Library::in_memory().unwrap();
    scan_root(&lib, &dir).unwrap();
    assert_eq!(lib.track_count(false).unwrap(), 2);

    std::fs::remove_dir_all(&dir).unwrap();
    let report = scan_root(&lib, &dir).unwrap();

    assert!(report.unreachable, "the root could not be read");
    assert_eq!(report.missing, 0, "nothing was pruned");
    assert_eq!(lib.track_count(true).unwrap(), 2, "the rows are still there");
    assert_eq!(lib.track_count(false).unwrap(), 0, "marked as missing");

    // And when it comes back, they count again.
    let dir2 = album("unmounted", 2).unwrap();
    let wrap = scan_root(&lib, &dir2).unwrap();
    assert_eq!(wrap.read, 2);
    assert_eq!(lib.track_count(false).unwrap(), 2);
    let _ = std::fs::remove_dir_all(dir2);
}

#[test]
fn a_deleted_file_is_marked_missing_and_keeps_its_history() {
    let Some(dir) = album("deleted", 3) else {
        eprintln!("no ffmpeg: skipped");
        return;
    };
    let lib = Library::in_memory().unwrap();
    scan_root(&lib, &dir).unwrap();

    std::fs::remove_file(dir.join("disco 1").join("02-track.flac")).unwrap();
    let report = scan_root(&lib, &dir).unwrap();

    assert_eq!(report.missing, 1);
    assert_eq!(lib.track_count(false).unwrap(), 2, "two are left present");
    assert_eq!(lib.track_count(true).unwrap(), 3, "the deleted one's row survives");

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn search_finds_by_title_artist_and_album() {
    let Some(dir) = album("search", 2) else {
        eprintln!("no ffmpeg: skipped");
        return;
    };
    let lib = Library::in_memory().unwrap();
    scan_root(&lib, &dir).unwrap();

    assert_eq!(lib.search("Track 1", 10).unwrap().len(), 1);
    assert_eq!(lib.search("Artist", 10).unwrap().len(), 2);
    assert_eq!(lib.search("Disco", 10).unwrap().len(), 2);
    assert!(lib.search("nothing like this", 10).unwrap().is_empty());

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_folder_cover_is_used_when_the_file_carries_none() {
    let Some(dir) = album("cover-folder", 2) else {
        eprintln!("no ffmpeg: skipped");
        return;
    };
    // The files the test generates carry no embedded cover.
    let cover = dir.join("disco 1").join("cover.jpg");
    std::fs::write(&cover, b"not a real jpeg, but it exists").unwrap();

    let lib = Library::in_memory().unwrap();
    let cache = std::env::temp_dir().join("chilpa-scan-cache");
    chilpa_library::scan_root_with_cache(&lib, &dir, Some(&cache)).unwrap();

    let albums = lib.albums(10, 0).unwrap();
    assert_eq!(albums.len(), 1);
    assert_eq!(
        albums[0].artwork_path.as_deref(),
        Some(cover.display().to_string().as_str()),
        "the folder's cover was taken"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_genres_in_the_file_reach_the_library() {
    let Some(dir) = album("genres", 2) else {
        eprintln!("no ffmpeg: skipped");
        return;
    };
    let lib = Library::in_memory().unwrap();
    scan_root(&lib, &dir).unwrap();

    let genres = lib.genres().unwrap();
    assert_eq!(genres, vec![("Rock".to_string(), 2)], "one genre with its two tracks");

    // And every track carries it.
    let source = lib.source_id("local", "default").unwrap();
    let id = lib
        .track_by_path(source, &dir.join("disco 1").join("01-track.flac"))
        .unwrap()
        .expect("the track is there");
    assert_eq!(lib.track_genres(id).unwrap(), vec!["Rock".to_string()]);

    let _ = std::fs::remove_dir_all(dir);
}
