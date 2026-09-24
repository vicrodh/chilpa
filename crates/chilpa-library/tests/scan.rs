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
    std::fs::create_dir_all(dir.join("disc 1")).ok()?;
    for i in 1..=tracks {
        let target = dir.join("disc 1").join(format!("{i:02}-track.flac"));
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

    // Counted in the table and not through the view that joins it: the view
    // showed one album while `albums` held one row per track, because the
    // unique constraint let every null mbid through and the lookup after the
    // insert always picked the lowest id.
    let rows: i64 = lib
        .connection()
        .query_row("SELECT count(*) FROM albums", [], |r| r.get(0))
        .unwrap();
    assert_eq!(rows, 1, "one album, not one per track");

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

    std::fs::remove_file(dir.join("disc 1").join("02-track.flac")).unwrap();
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
    let cover = dir.join("disc 1").join("cover.jpg");
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
        .track_by_path(source, &dir.join("disc 1").join("01-track.flac"))
        .unwrap()
        .expect("the track is there");
    assert_eq!(lib.track_genres(id).unwrap(), vec!["Rock".to_string()]);

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_track_that_moves_keeps_its_history() {
    // The thing every player keyed by path gets wrong. Someone reorganises
    // their music, and the play counts, the ratings and the analysis are
    // left behind under the old name while a brand new empty track appears
    // under the new one.
    let Some(dir) = album("moved", 2) else {
        eprintln!("no ffmpeg: skipped");
        return;
    };
    let lib = Library::in_memory().unwrap();
    scan_root(&lib, &dir).unwrap();

    let source = lib.source_id("local", "default").unwrap();
    let before = dir.join("disc 1").join("01-track.flac");
    let track = lib.track_by_path(source, &before).unwrap().expect("it was scanned");

    // A history worth losing: plays, and what the analysis found.
    lib.log_play(Some(track), Some(source), 100, 1_000, 1_000, false, None, None, true).unwrap();
    lib.log_play(Some(track), Some(source), 200, 1_000, 1_000, false, None, None, true).unwrap();
    lib.save_features(track, "chilpa", 2, Some(128.0), Some(0.7), None, Some(-9.0)).unwrap();
    assert_eq!(lib.top_tracks(5).unwrap().len(), 1);

    // Reorganised: another folder, another name. Same file.
    let elsewhere = dir.join("another disc");
    std::fs::create_dir_all(&elsewhere).unwrap();
    let after = elsewhere.join("la misma.flac");
    std::fs::rename(&before, &after).unwrap();

    let report = scan_root(&lib, &dir).unwrap();

    assert_eq!(report.missing, 0, "nothing went missing: it moved");
    assert_eq!(lib.track_count(true).unwrap(), 2, "no new row was made");
    assert_eq!(
        lib.track_by_path(source, &after).unwrap(),
        Some(track),
        "it is the same track under the new name"
    );
    assert!(lib.track_by_path(source, &before).unwrap().is_none(), "and not under the old one");

    let top = lib.top_tracks(5).unwrap();
    assert_eq!(top.len(), 1, "the history is still one track's");
    assert_eq!(top[0].count, 2, "with both its plays");
    assert!(
        lib.track_features(track, "chilpa").unwrap().is_some(),
        "and what the analysis found"
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_copy_is_a_new_track_and_not_a_move() {
    // The same inode, the same size, the same mtime -- and the old name
    // still resolves. That is a hardlink or a copy, and it is two files.
    let Some(dir) = album("copied", 1) else {
        eprintln!("no ffmpeg: skipped");
        return;
    };
    let lib = Library::in_memory().unwrap();
    scan_root(&lib, &dir).unwrap();
    assert_eq!(lib.track_count(true).unwrap(), 1);

    let original = dir.join("disc 1").join("01-track.flac");
    let link = dir.join("disc 1").join("02-link.flac");
    std::fs::hard_link(&original, &link).unwrap();

    scan_root(&lib, &dir).unwrap();
    assert_eq!(lib.track_count(true).unwrap(), 2, "two names, two tracks");

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_scan_into_a_library_on_disk_works_the_same() {
    // Every other test here uses a library in memory. That is not the same
    // thing: a library on disk has a write-ahead log, a busy timeout and a
    // file another process can hold. The bug this test exists for -- a
    // rescan failing on a track with no album -- was invisible in memory and
    // came out the first time five thousand files went into a real file.
    let Some(dir) = album("on-disk", 3) else {
        eprintln!("no ffmpeg: skipped");
        return;
    };
    // A loose file, in no album: what "Open an audio file" leaves behind.
    let loose = dir.join("loose.flac");
    std::fs::copy(dir.join("disc 1").join("01-track.flac"), &loose).unwrap();
    let stripped = std::process::Command::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-i"])
        .arg(&loose)
        .args(["-map_metadata", "-1", "-c:a", "flac"])
        .arg(dir.join("bare.flac"))
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !stripped {
        eprintln!("ffmpeg could not strip the tags: skipped");
        let _ = std::fs::remove_dir_all(dir);
        return;
    }
    std::fs::remove_file(&loose).unwrap();

    let db = dir.join("library.db");
    let lib = Library::open(&db).unwrap();
    let first = scan_root(&lib, &dir).unwrap();
    assert_eq!(first.read, 4, "three in the album and one on its own");

    // Without this the test passes without ever reaching the bug.
    let loose_tracks: i64 = lib
        .connection()
        .query_row("SELECT count(*) FROM tracks WHERE album_id IS NULL", [], |r| r.get(0))
        .unwrap();
    assert_eq!(loose_tracks, 1, "one track really has no album");

    // The rescan is the one that used to fail.
    let again = scan_root(&lib, &dir).unwrap();
    assert_eq!(again.read, 0, "nothing changed");
    assert_eq!(again.unchanged, 4);
    assert_eq!(again.missing, 0);

    assert!(lib.check().unwrap().is_empty(), "and nothing points at nothing");
    assert_eq!(lib.track_count(false).unwrap(), 4);

    drop(lib);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_library_can_be_read_while_it_is_being_scanned() {
    // The interface reads on its own connection while the scan writes on
    // another. WAL is what makes that work, and a batch is what keeps the
    // wait short; this is the test that says both are still true.
    let Some(dir) = album("while-scanning", 40) else {
        eprintln!("no ffmpeg: skipped");
        return;
    };
    let db = dir.join("library.db");
    let writer = Library::open(&db).unwrap();
    let reader = Library::open(&db).unwrap();

    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = stop.clone();
    let reads = std::thread::spawn(move || {
        let mut done = 0usize;
        let mut slowest = std::time::Duration::ZERO;
        while !flag.load(std::sync::atomic::Ordering::Relaxed) {
            let start = std::time::Instant::now();
            // What the window asks for while a scan runs.
            reader.albums_sorted(chilpa_library::AlbumSort::Artist, 50, 0).expect("the grid");
            reader.stats().expect("the stats");
            reader.search("Track", 20).expect("the search");
            slowest = slowest.max(start.elapsed());
            done += 1;
        }
        (done, slowest)
    });

    scan_root(&writer, &dir).unwrap();
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    let (done, slowest) = reads.join().unwrap();

    assert!(done > 0, "the reader never got a turn");
    assert!(
        slowest < std::time::Duration::from_secs(1),
        "a read waited {slowest:?} on the scan; the point of WAL and of batching is that it \
         does not"
    );
    assert_eq!(writer.track_count(false).unwrap(), 40);

    let _ = std::fs::remove_dir_all(dir);
}
