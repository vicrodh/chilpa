//! Scanning for real: incremental, and deleting nothing it cannot see.

use chilpa_library::{scan_root, Library};
use std::path::PathBuf;
use std::process::Command;

fn have_ffmpeg() -> bool {
    Command::new("ffmpeg").arg("-version").output().map(|o| o.status.success()).unwrap_or(false)
}

/// A test album on disk.
fn album(name: &str, tracks: usize) -> Option<PathBuf> {
    if !have_ffmpeg() {
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

/// A record of one track whose file says it came out on `date`.
fn dated(name: &str, date: &str) -> Option<PathBuf> {
    if !have_ffmpeg() {
        return None;
    }
    let dir = std::env::temp_dir().join(format!("chilpa-scan-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).ok()?;
    let ok = Command::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-f", "lavfi", "-i", "sine=frequency=440:duration=1", "-c:a", "flac"])
        .args(["-metadata", "title=Song", "-metadata", "artist=Artist", "-metadata", &format!("album={name}")])
        .args(["-metadata", &format!("date={date}")])
        .arg(dir.join("01-song.flac"))
        .output()
        .ok()?
        .status
        .success();
    ok.then_some(dir)
}

#[test]
fn a_record_keeps_the_whole_date_its_file_says_and_otherwise_its_year() {
    let (Some(whole), Some(year)) = (dated("whole-date", "1981-04-26"), dated("year-only", "1999")) else {
        eprintln!("no ffmpeg: skipped");
        return;
    };
    let lib = Library::in_memory().unwrap();
    scan_root(&lib, &whole).unwrap();
    scan_root(&lib, &year).unwrap();
    let albums = lib.albums(10, 0).unwrap();
    let of = |title: &str| albums.iter().find(|a| a.title == title).map(|a| lib.album_release_date(a.id).unwrap());
    assert_eq!(of("whole-date"), Some(Some("1981-04-26".into())));
    assert_eq!(of("year-only"), Some(Some("1999".into())), "a year alone is all there is");

    // An edited year leaves the old date unshown instead of contradicting it.
    let id = albums.iter().find(|a| a.title == "whole-date").unwrap().id;
    lib.connection().execute("UPDATE albums SET year = 1982 WHERE id = ?1", [id]).unwrap();
    assert_eq!(lib.album_release_date(id).unwrap(), Some("1982".into()));

    let _ = std::fs::remove_dir_all(whole);
    let _ = std::fs::remove_dir_all(year);
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
fn rereading_a_known_part_retains_album_marks_and_version_choice() {
    let Some(dir) = album("retained-release", 2) else { return };
    let lib = Library::in_memory().unwrap();
    scan_root(&lib, &dir).unwrap();
    let id = lib.album_ids(chilpa_library::AlbumSort::Title).unwrap()[0];
    lib.set_album_favorite(id, true).unwrap();
    lib.select_album_version(id, id).unwrap();
    lib.connection().execute("UPDATE tracks SET release_evidence_read = 0", []).unwrap();
    assert_eq!(scan_root(&lib, &dir.join("disc 1")).unwrap().read, 2);
    assert_eq!(lib.album_ids(chilpa_library::AlbumSort::Title).unwrap(), [id]);
    assert!(lib.favorite_albums().unwrap().contains(&id));
    assert_eq!(lib.connection().query_row("SELECT album_id FROM album_preferences", [], |r| r.get::<_, i64>(0)).unwrap(), id);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn tagged_editions_in_separate_folders_stay_separate_after_rescanning() {
    if !have_ffmpeg() { return; }
    let dir = std::env::temp_dir().join(format!("chilpa-scan-editions-{}", std::process::id()));
    for edition in ["Original", "Remaster"] {
        std::fs::create_dir_all(dir.join(edition)).unwrap();
        assert!(tagged_file(&dir.join(edition).join("01.flac"), "Song", "Artist", "Album", Some("Artist")));
    }
    let lib = Library::in_memory().unwrap();
    assert_eq!(scan_root(&lib, &dir).unwrap().read, 2);
    let albums = lib.albums(10, 0).unwrap();
    assert_eq!(albums.len(), 2, "equal album-artist tags do not merge physical editions");
    assert!(albums.iter().all(|album| album.track_count == 1));
    let ids: Vec<_> = albums.iter().map(|album| album.id).collect();
    let chosen = lib.grouped_album_ids(&ids).unwrap();
    assert_eq!(chosen.len(), 1);
    assert!(lib.album_tracks(chosen[0]).unwrap()[0].external_id.contains("Remaster"));
    assert_eq!(scan_root(&lib, &dir).unwrap().unchanged, 2);
    assert_eq!(lib.albums(10, 0).unwrap(), albums);
    std::fs::remove_dir_all(dir).unwrap();
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
    assert_eq!(lib.track_count(false).unwrap(), 2, "still listed (ADR-017)");
    assert_eq!(unreachable(&lib), 2, "and none of them can be played");

    // And when it comes back, they play again.
    let dir2 = album("unmounted", 2).unwrap();
    let wrap = scan_root(&lib, &dir2).unwrap();
    assert_eq!(wrap.read, 2);
    assert_eq!(lib.track_count(false).unwrap(), 2);
    assert_eq!(unreachable(&lib), 0);
    let _ = std::fs::remove_dir_all(dir2);
}

#[test]
fn a_share_that_goes_away_while_it_is_read_is_away_not_emptied() {
    use chilpa_library::scan::{scan_root_with, ContainedTrack, Containers};
    use std::path::Path;
    // Read first, it takes the share away: its mount point stays, empty.
    struct Unmount { album: PathBuf, parked: PathBuf }
    impl Containers for Unmount {
        fn may_hold(&self, path: &Path) -> bool { path.extension().is_some_and(|extension| extension == "disc") }
        fn tracks(&self, _: &Path) -> Option<Vec<ContainedTrack>> {
            let _ = std::fs::rename(&self.album, &self.parked);
            None
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let (share, parked) = (dir.path().join("share"), dir.path().join("parked"));
    let album = share.join("album");
    std::fs::create_dir_all(&album).unwrap();
    for n in 0..3 { std::fs::write(album.join(format!("{n}.flac")), b"not audio").unwrap(); }
    let lib = Library::in_memory().unwrap();
    scan_root(&lib, &share).unwrap();
    assert_eq!(lib.track_count(false).unwrap(), 3);

    // Named so it is read before the tracks.
    std::fs::write(album.join("0.disc"), b"x").unwrap();
    let report = scan_root_with(&lib, &share, None, &|_| {}, &Unmount { album: album.clone(), parked }).unwrap();

    assert!(report.unreachable, "the share stopped answering");
    assert_eq!(report.missing, 0, "the files it could not read are not taken for deleted");
    assert_eq!(lib.track_count(false).unwrap(), 3, "still listed (ADR-017)");
    let scanned: i64 = lib.connection().query_row("SELECT count(*) FROM scanned_files", [], |r| r.get(0)).unwrap();
    assert_eq!(scanned, 3, "what was scanned of them is kept");
    assert_eq!(unreachable(&lib), 3, "and none of them can be played");
}

#[test]
fn different_album_artist_tags_in_one_release_do_not_split_a_compilation() {
    if !have_ffmpeg() { return; }
    let dir = std::env::temp_dir().join(format!("chilpa-scan-compilation-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (file, artist, credit) in [("01.flac", "歌手 A", "Singer A"), ("02.flac", "歌手 B", "Singer B")] {
        assert!(tagged_file(&dir.join(file), "Song", artist, "Reference CD", Some(credit)));
    }
    let lib = Library::in_memory().unwrap();
    assert_eq!(scan_root(&lib, &dir).unwrap().read, 2);
    let albums = lib.albums(100, 0).unwrap();
    assert_eq!(albums.len(), 1);
    assert_eq!(albums[0].album_artist, "Various Artists");
    assert_eq!(albums[0].track_count, 2);
    assert_eq!(lib.album_ids_matching(chilpa_library::AlbumSort::Title, "Singer B").unwrap(), [albums[0].id]);
    assert_eq!(scan_root(&lib, &dir).unwrap().unchanged, 2);
    assert_eq!(lib.albums(100, 0).unwrap(), albums);
    std::fs::remove_dir_all(dir).unwrap();
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

#[cfg(unix)]
#[test]
fn a_link_that_points_nowhere_is_reported_and_is_not_left_present() {
    // A music folder held 20,298 links into a layout that no longer
    // existed. The scan counted every one as seen, recorded none, and said
    // nothing; and a link that broke after it was recorded stayed present.
    let Some(dir) = album("broken-link", 1) else {
        eprintln!("no ffmpeg: skipped");
        return;
    };
    let real = dir.join("disc 1").join("01-track.flac");
    std::os::unix::fs::symlink(&real, dir.join("link.flac")).unwrap();
    let lib = Library::in_memory().unwrap();
    let first = scan_root(&lib, &dir).unwrap();
    assert_eq!(first.read, 2, "the file and the link to it");

    // The file goes somewhere the scan does not look, and the link breaks.
    std::fs::rename(&real, dir.join("moved-away.bin")).unwrap();
    let again = scan_root(&lib, &dir).unwrap();
    assert_eq!(again.unreadable, 1, "the broken link is reported");
    assert_eq!(lib.track_count(false).unwrap(), 0, "and neither of the two is left present");
    assert_eq!(lib.track_count(true).unwrap(), 2, "though both rows survive");

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

/// A file that holds two tracks, standing in for a disc image: `.box`, its
/// tracks `box:<file>#1` and `#2`. A box holds tracks only if it says "box";
/// anything else in a `.box` holds none. It counts how often it is opened.
#[derive(Default)]
struct Boxes {
    opened: std::sync::atomic::AtomicUsize,
}

impl chilpa_library::scan::Containers for Boxes {
    fn may_hold(&self, path: &std::path::Path) -> bool {
        path.extension().is_some_and(|e| e == "box")
    }

    fn tracks(&self, path: &std::path::Path) -> Option<Vec<chilpa_library::scan::ContainedTrack>> {
        self.opened.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if std::fs::read(path).ok()? != b"box" {
            return None;
        }
        Some(
            (1..=2u16)
                .map(|n| chilpa_library::scan::ContainedTrack {
                    path: format!("box:{}#{n}", path.display()),
                    info: chilpa_tags::FileInfo {
                        tags: chilpa_tags::Tags {
                            title: Some(format!("Boxed {n}")),
                            artists: vec!["Boxer".into()],
                            album: Some("The Box".into()),
                            track_number: Some(n),
                            ..chilpa_tags::Tags::default()
                        },
                        duration: std::time::Duration::from_secs(60),
                        sample_rate_hz: Some(2_822_400),
                        bit_depth: Some(1),
                        channels: Some(2),
                    },
                })
                .collect(),
        )
    }
}

fn scan_boxes(lib: &Library, dir: &std::path::Path, boxes: &Boxes) -> chilpa_library::ScanReport {
    chilpa_library::scan::scan_root_with(lib, dir, None, &|_| {}, boxes).unwrap()
}

#[test]
fn a_file_that_holds_tracks_goes_in_as_its_tracks() {
    let Some(dir) = album("boxes", 1) else {
        eprintln!("no ffmpeg: skipped");
        return;
    };
    std::fs::write(dir.join("album.box"), b"box").unwrap();
    let lib = Library::in_memory().unwrap();
    let report = scan_boxes(&lib, &dir, &Boxes::default());
    assert_eq!((report.seen, report.read), (3, 3), "the FLAC and the box's two tracks");
    let boxed = lib.search("Boxed", 10).unwrap();
    let mut titles: Vec<&str> = boxed.iter().map(|t| t.title.as_str()).collect();
    titles.sort();
    assert_eq!(titles, ["Boxed 1", "Boxed 2"]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_file_that_holds_tracks_is_not_opened_again_when_it_did_not_change() {
    // Reading a disc image's tables costs about 0.4 s over the network, and
    // a rescan that finds nothing changed is the one that runs at startup.
    let Some(dir) = album("boxes-again", 1) else {
        eprintln!("no ffmpeg: skipped");
        return;
    };
    std::fs::write(dir.join("album.box"), b"box").unwrap();
    let lib = Library::in_memory().unwrap();
    let boxes = Boxes::default();
    scan_boxes(&lib, &dir, &boxes);
    let again = scan_boxes(&lib, &dir, &boxes);
    assert_eq!(boxes.opened.load(std::sync::atomic::Ordering::Relaxed), 1, "opened on the first scan only");
    assert_eq!((again.read, again.unchanged, again.missing), (0, 3, 0));
    assert_eq!(lib.track_count(false).unwrap(), 3, "and all three still present");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_file_that_holds_tracks_and_goes_away_leaves_them_missing() {
    let Some(dir) = album("boxes-gone", 1) else {
        eprintln!("no ffmpeg: skipped");
        return;
    };
    std::fs::write(dir.join("album.box"), b"box").unwrap();
    let lib = Library::in_memory().unwrap();
    let boxes = Boxes::default();
    scan_boxes(&lib, &dir, &boxes);
    std::fs::remove_file(dir.join("album.box")).unwrap();
    let again = scan_boxes(&lib, &dir, &boxes);
    assert_eq!(again.missing, 2);
    assert_eq!(lib.track_count(false).unwrap(), 1, "the FLAC is left present");
    assert_eq!(lib.track_count(true).unwrap(), 3, "the box's rows survive, missing");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_file_that_holds_nothing_is_not_audio() {
    // A DVD image beside the music has the extension and none of the tracks.
    let Some(dir) = album("boxes-empty", 1) else {
        eprintln!("no ffmpeg: skipped");
        return;
    };
    std::fs::write(dir.join("film.box"), b"not a box").unwrap();
    let lib = Library::in_memory().unwrap();
    let report = scan_boxes(&lib, &dir, &Boxes::default());
    assert_eq!((report.seen, report.read, report.unreadable), (1, 1, 0));
    let _ = std::fs::remove_dir_all(dir);
}

/// A tagged file on disk: title, artist, album and, if given, album artist.
fn tagged_file(target: &std::path::Path, title: &str, artist: &str, album: &str, album_artist: Option<&str>) -> bool {
    let _ = std::fs::create_dir_all(target.parent().unwrap());
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-y", "-loglevel", "error", "-f", "lavfi", "-i", "sine=frequency=440:duration=1", "-c:a", "flac"])
        .args(["-metadata", &format!("title={title}")])
        .args(["-metadata", &format!("artist={artist}")])
        .args(["-metadata", &format!("album={album}")]);
    if let Some(name) = album_artist {
        cmd.args(["-metadata", &format!("album_artist={name}")]);
    }
    cmd.arg(target).output().map(|o| o.status.success()).unwrap_or(false)
}

#[test]
fn a_compilation_without_an_album_artist_is_one_record() {
    if !have_ffmpeg() {
        eprintln!("no ffmpeg: skipped");
        return;
    }
    let dir = std::env::temp_dir().join("chilpa-scan-compilation");
    let _ = std::fs::remove_dir_all(&dir);
    // Every song by someone else, over two discs, and no album artist: the
    // scan made one record per artist of this.
    let compilation = dir.join("OP&ED Collection");
    assert!(tagged_file(&compilation.join("CD1/01.flac"), "Asterisk", "ORANGE RANGE", "OP&ED Collection", None));
    assert!(tagged_file(&compilation.join("CD1/02.flac"), "Life", "YUI", "OP&ED Collection", None));
    assert!(tagged_file(&compilation.join("CD2/01.flac"), "Ref:rain", "Aimer", "OP&ED Collection", None));
    // One artist and no album artist: a record by that artist.
    let single = dir.join("Solo");
    assert!(tagged_file(&single.join("01.flac"), "One", "Mora Vale", "Solo", None));
    assert!(tagged_file(&single.join("02.flac"), "Two", "Mora Vale", "Solo", None));
    // An album artist named: that is who it is by, whatever the tracks say.
    let tagged = dir.join("Tagged");
    assert!(tagged_file(&tagged.join("01.flac"), "A", "Band feat. Guest", "Tagged", Some("Band")));
    assert!(tagged_file(&tagged.join("02.flac"), "B", "Band", "Tagged", Some("Band")));

    let lib = Library::in_memory().unwrap();
    scan_root(&lib, &dir).unwrap();
    let mut albums: Vec<(String, String, usize)> =
        lib.albums(10, 0).unwrap().into_iter().map(|a| (a.title, a.album_artist, a.track_count)).collect();
    albums.sort();
    assert_eq!(
        albums,
        [
            ("OP&ED Collection".to_string(), "Various Artists".to_string(), 3),
            ("Solo".to_string(), "Mora Vale".to_string(), 2),
            ("Tagged".to_string(), "Band".to_string(), 2),
        ]
    );
    // A rescan that rereads nothing leaves it as it is.
    scan_root(&lib, &dir).unwrap();
    assert_eq!(lib.albums(10, 0).unwrap().len(), 3);
    let _ = std::fs::remove_dir_all(dir);
}

fn unreachable(lib: &Library) -> i64 {
    lib.connection().query_row("SELECT count(*) FROM tracks WHERE present = 1 AND reachable = 0", [], |r| r.get(0)).unwrap()
}

#[test]
fn an_empty_mount_point_is_a_folder_that_is_away_not_one_that_was_emptied() {
    // The mount point exists and nothing is mounted on it. Counting every
    // file as deleted emptied the library each time the NAS was away.
    let Some(dir) = album("empty-mount", 2) else {
        eprintln!("no ffmpeg: skipped");
        return;
    };
    let lib = Library::in_memory().unwrap();
    scan_root(&lib, &dir).unwrap();

    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() { std::fs::remove_dir_all(path).unwrap() } else { std::fs::remove_file(path).unwrap() }
    }
    let report = scan_root(&lib, &dir).unwrap();

    assert!(report.unreachable);
    assert_eq!(report.missing, 0, "nothing was taken for deleted");
    assert_eq!(lib.track_count(false).unwrap(), 2);
    assert_eq!(unreachable(&lib), 2);
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[test]
fn a_subfolder_that_cannot_be_opened_is_away_not_emptied() {
    use std::os::unix::fs::PermissionsExt;
    let Some(dir) = album("blocked-subfolder", 2) else {
        eprintln!("no ffmpeg: skipped");
        return;
    };
    // One track outside it, so the folder itself answers.
    std::fs::copy(dir.join("disc 1").join("01-track.flac"), dir.join("loose.flac")).unwrap();
    let lib = Library::in_memory().unwrap();
    scan_root(&lib, &dir).unwrap();
    assert_eq!(lib.track_count(false).unwrap(), 3);

    let sub = dir.join("disc 1");
    std::fs::set_permissions(&sub, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::read_dir(&sub).is_ok() {
        // Run as root: permissions do not stop it.
        std::fs::set_permissions(&sub, std::fs::Permissions::from_mode(0o755)).unwrap();
        eprintln!("permissions do not apply here: skipped");
        return;
    }
    let report = scan_root(&lib, &dir).unwrap();
    std::fs::set_permissions(&sub, std::fs::Permissions::from_mode(0o755)).unwrap();

    assert!(!report.unreachable, "the folder answers");
    assert_eq!(report.missing, 0, "nothing was taken for deleted");
    assert_eq!(lib.track_count(false).unwrap(), 3, "all still listed");
    assert_eq!(unreachable(&lib), 2, "the two under it cannot be played");

    // Readable again: the next scan gives them back.
    scan_root(&lib, &dir).unwrap();
    assert_eq!(unreachable(&lib), 0);
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[test]
fn the_tracks_inside_a_file_under_a_subfolder_that_cannot_be_opened_are_away_not_emptied() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let sub = dir.path().join("disc 1");
    std::fs::create_dir(&sub).unwrap();
    std::fs::write(sub.join("album.box"), b"box").unwrap();
    let lib = Library::in_memory().unwrap();
    scan_boxes(&lib, dir.path(), &Boxes::default());
    assert_eq!(lib.track_count(false).unwrap(), 2);

    std::fs::set_permissions(&sub, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::read_dir(&sub).is_ok() {
        std::fs::set_permissions(&sub, std::fs::Permissions::from_mode(0o755)).unwrap();
        eprintln!("permissions do not apply here: skipped");
        return;
    }
    let report = scan_boxes(&lib, dir.path(), &Boxes::default());
    std::fs::set_permissions(&sub, std::fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!(report.missing, 0, "a track's own path is not where its file is");
    assert_eq!(lib.track_count(false).unwrap(), 2, "both still listed");
    assert_eq!(unreachable(&lib), 2, "and neither can be played");
    let scanned: i64 = lib.connection().query_row("SELECT count(*) FROM scanned_files", [], |r| r.get(0)).unwrap();
    assert_eq!(scanned, 2, "what was scanned of them is kept");
}

#[cfg(unix)]
#[test]
fn files_that_can_be_listed_and_not_looked_at_are_away_not_emptied() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let album = dir.path().join("album");
    std::fs::create_dir(&album).unwrap();
    for n in 0..2 { std::fs::write(album.join(format!("{n}.flac")), b"not audio").unwrap(); }
    let lib = Library::in_memory().unwrap();
    scan_root(&lib, dir.path()).unwrap();
    assert_eq!(lib.track_count(false).unwrap(), 2);

    // Readable and not searchable: the names list, the files cannot be looked at.
    std::fs::set_permissions(&album, std::fs::Permissions::from_mode(0o644)).unwrap();
    if std::fs::metadata(album.join("0.flac")).is_ok() {
        std::fs::set_permissions(&album, std::fs::Permissions::from_mode(0o755)).unwrap();
        eprintln!("permissions do not apply here: skipped");
        return;
    }
    let report = scan_root(&lib, dir.path()).unwrap();
    std::fs::set_permissions(&album, std::fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!((report.unreadable, report.missing), (2, 0), "not looked at is not gone");
    assert_eq!(lib.track_count(false).unwrap(), 2, "both still listed");
    assert_eq!(unreachable(&lib), 2, "and neither can be played");
    scan_root(&lib, dir.path()).unwrap();
    assert_eq!(unreachable(&lib), 0, "searchable again, they play again");
}

#[cfg(unix)]
#[test]
fn a_linked_folder_whose_place_stops_answering_is_away_until_the_link_goes() {
    let dir = tempfile::tempdir().unwrap();
    let (root, elsewhere) = (dir.path().join("music"), dir.path().join("elsewhere"));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&elsewhere).unwrap();
    std::fs::write(elsewhere.join("far.flac"), b"not audio").unwrap();
    std::os::unix::fs::symlink(&elsewhere, root.join("linked")).unwrap();
    let lib = Library::in_memory().unwrap();
    scan_root(&lib, &root).unwrap();
    assert_eq!(lib.track_count(false).unwrap(), 1);

    std::fs::rename(&elsewhere, dir.path().join("parked")).unwrap();
    let report = scan_root(&lib, &root).unwrap();
    assert_eq!(report.missing, 0, "a link that does not resolve says nothing about what was under it");
    assert_eq!((lib.track_count(false).unwrap(), unreachable(&lib)), (1, 1));

    // Without the link, what was under it is gone.
    std::fs::write(root.join("kept.flac"), b"not audio").unwrap();
    std::fs::remove_file(root.join("linked")).unwrap();
    let report = scan_root(&lib, &root).unwrap();
    assert_eq!(report.missing, 1);
}

/// One track of a set, in the folder of its disc.
fn disc_track(dir: &std::path::Path, folder: &str, disc: u32) -> bool {
    std::fs::create_dir_all(dir.join(folder)).is_ok()
        && Command::new("ffmpeg")
            .args(["-y", "-loglevel", "error", "-f", "lavfi", "-i", "sine=frequency=440:duration=1", "-c:a", "flac"])
            .args(["-metadata", "title=Track", "-metadata", "artist=Artist", "-metadata", "album=Set"])
            .args(["-metadata", &format!("disc={disc}"), "-metadata", "track=1"])
            .arg(dir.join(folder).join("01.flac"))
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
}

#[test]
fn a_disc_with_its_own_cover_says_so_though_it_was_read_before() {
    let dir = std::env::temp_dir().join("chilpa-scan-disc-covers");
    let _ = std::fs::remove_dir_all(&dir);
    if !have_ffmpeg() || !disc_track(&dir, "CD1", 1) || !disc_track(&dir, "CD2", 2) || !disc_track(&dir, "CD3", 3) {
        eprintln!("no ffmpeg: skipped");
        return;
    }
    // Two discs carry a cover of their own; the third has none.
    let covers = [dir.join("CD1/cover.jpg"), dir.join("CD2/cover.jpg")];
    std::fs::write(&covers[0], b"not a real jpeg, but it exists").unwrap();
    std::fs::write(&covers[1], b"another one, of another size").unwrap();
    let lib = Library::in_memory().unwrap();
    let cache = std::env::temp_dir().join("chilpa-scan-cache");
    chilpa_library::scan_root_with_cache(&lib, &dir, Some(&cache)).unwrap();
    let album = lib.albums(10, 0).unwrap();
    assert_eq!(album.len(), 1, "the three discs are one record");
    let said = |lib: &Library| -> Vec<(u16, String)> {
        lib.album_discs(album[0].id).unwrap().into_iter().map(|disc| (disc.number, disc.artwork)).collect()
    };
    let own = vec![(1, covers[0].display().to_string()), (2, covers[1].display().to_string()), (3, String::new())];
    assert_eq!(said(&lib), own);

    // As a library read before each disc's cover was kept: every track
    // holds its record's, which is the first disc's.
    lib.connection().execute("UPDATE tracks SET release_artwork = ?1", [covers[0].display().to_string()]).unwrap();
    let report = chilpa_library::scan_root_with_cache(&lib, &dir, Some(&cache)).unwrap();
    assert_eq!(report.read, 0, "no file changed, so none is read again");
    let again = said(&lib);
    assert_eq!(again[..2], own[..2], "each disc with a cover of its own holds it again");
    assert_eq!(again[2].1, covers[0].display().to_string(), "a disc with none keeps what it had");

    // A cover taken out of a file is the disc's own for all anybody knows.
    let embedded = cache.join("embedded-of-disc-2.jpg").display().to_string();
    lib.connection().execute("UPDATE tracks SET release_artwork = ?1 WHERE disc_number = 2", [&embedded]).unwrap();
    chilpa_library::scan_root_with_cache(&lib, &dir, Some(&cache)).unwrap();
    assert_eq!(said(&lib)[1].1, embedded);

    // A set that copies one cover into every disc has one cover: its discs
    // say the same one.
    std::fs::copy(&covers[0], dir.join("CD3/cover.jpg")).unwrap();
    lib.connection().execute("UPDATE tracks SET release_artwork = ?1 WHERE disc_number = 2", [covers[1].display().to_string()]).unwrap();
    chilpa_library::scan_root_with_cache(&lib, &dir, Some(&cache)).unwrap();
    let copied = said(&lib);
    assert_eq!(copied[2].1, covers[0].display().to_string(), "the third disc's copy is the first disc's cover");
    assert_eq!(copied[1].1, covers[1].display().to_string());
    chilpa_library::scan_root_with_cache(&lib, &dir, Some(&cache)).unwrap();
    assert_eq!(said(&lib), copied, "and it stays so");
    let _ = std::fs::remove_dir_all(dir);
}

/// A disc whose cover is inside its file, as a box set's discs often are:
/// a picture of `side` pixels, so each disc's is of another size.
fn disc_track_with_cover(dir: &std::path::Path, folder: &str, disc: u32, side: u32) -> bool {
    let picture = dir.join(format!("{folder}.png"));
    std::fs::create_dir_all(dir.join(folder)).is_ok()
        && Command::new("ffmpeg")
            .args(["-y", "-loglevel", "error", "-f", "lavfi", "-i", &format!("color=c=red:s={side}x{side}"), "-frames:v", "1"])
            .arg(&picture)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        && Command::new("ffmpeg")
            .args(["-y", "-loglevel", "error", "-f", "lavfi", "-i", "sine=frequency=440:duration=1", "-i"])
            .arg(&picture)
            .args(["-map", "0", "-map", "1", "-c:a", "flac", "-c:v", "png", "-disposition:v", "attached_pic"])
            .args(["-metadata", "title=Track", "-metadata", "artist=Artist", "-metadata", "album=Box"])
            .args(["-metadata", &format!("disc={disc}"), "-metadata", "track=1"])
            .arg(dir.join(folder).join("01.flac"))
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        && std::fs::remove_file(&picture).is_ok()
}

#[test]
fn discs_whose_covers_are_inside_their_files_each_show_their_own() {
    let dir = std::env::temp_dir().join("chilpa-scan-embedded-disc-covers");
    let _ = std::fs::remove_dir_all(&dir);
    if !have_ffmpeg() || !disc_track_with_cover(&dir, "Disc 01", 1, 16) || !disc_track_with_cover(&dir, "Disc 02", 2, 24) || !disc_track_with_cover(&dir, "Disc 03", 3, 32) {
        eprintln!("no ffmpeg: skipped");
        return;
    }
    let lib = Library::in_memory().unwrap();
    let cache = std::env::temp_dir().join("chilpa-scan-embedded-cache");
    let _ = std::fs::remove_dir_all(&cache);
    chilpa_library::scan_root_with_cache(&lib, &dir, Some(&cache)).unwrap();
    let album = lib.albums(10, 0).unwrap();
    assert_eq!(album.len(), 1, "the three discs are one record");
    let covers = |lib: &Library| -> Vec<String> { lib.album_discs(album[0].id).unwrap().into_iter().map(|disc| disc.artwork).collect() };
    let own = covers(&lib);
    assert!(own.iter().all(|cover| !cover.is_empty()), "{own:?}");
    assert_eq!(own.iter().collect::<std::collections::BTreeSet<_>>().len(), 3, "each disc its own: {own:?}");

    // As a library read before: every track holds its record's cover, the
    // first disc's. Nothing changed on disk, and each disc gets its own back.
    lib.connection().execute("UPDATE tracks SET release_artwork = (SELECT artwork_path FROM albums WHERE id = tracks.album_id)", []).unwrap();
    let report = chilpa_library::scan_root_with_cache(&lib, &dir, Some(&cache)).unwrap();
    assert_eq!(report.read, 0, "no file changed, so none is read again as a track");
    assert_eq!(covers(&lib), own);
    let _ = std::fs::remove_dir_all(dir);
}

/// A FLAC file with no tags at all: what ffmpeg writes with its own
/// metadata turned off.
fn untagged_file(target: &std::path::Path) -> bool {
    let _ = std::fs::create_dir_all(target.parent().unwrap());
    Command::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-f", "lavfi", "-i", "sine=frequency=440:duration=1"])
        .args(["-map_metadata", "-1", "-fflags", "+bitexact", "-c:a", "flac"])
        .arg(target)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
fn untagged_files_take_their_record_from_their_folders() {
    // 23 files of a record with tags nobody could read came in under their
    // names and with no record: on no album screen.
    if !have_ffmpeg() {
        eprintln!("no ffmpeg: skipped");
        return;
    }
    let root = std::env::temp_dir().join("chilpa-scan-from-path");
    let _ = std::fs::remove_dir_all(&root);
    let record = root.join("G").join("George Harrison").join("1970 - All Things Must Pass");
    for name in ["01 - I'd Have You Anytime.flac", "02 - My Sweet Lord.flac"] {
        assert!(untagged_file(&record.join(name)));
    }
    let lib = Library::in_memory().unwrap();
    scan_root(&lib, &root).unwrap();
    let albums = lib.albums(10, 0).unwrap();
    assert_eq!(albums.len(), 1, "one record: {:?}", albums.iter().map(|a| &a.title).collect::<Vec<_>>());
    assert_eq!(albums[0].title, "All Things Must Pass");
    assert_eq!(albums[0].album_artist, "George Harrison");
    assert_eq!(albums[0].year, Some(1970));
    let tracks = lib.album_tracks(albums[0].id).unwrap();
    let named: Vec<(Option<u16>, &str, &str)> = tracks.iter().map(|t| (t.track_number, t.title.as_str(), t.artist.as_str())).collect();
    assert_eq!(named, [(Some(1), "I'd Have You Anytime", "George Harrison"), (Some(2), "My Sweet Lord", "George Harrison")]);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn an_untagged_file_joins_the_record_its_tagged_neighbours_share() {
    // Four bonus tracks of a box with no tags, in the folder of the sixteen
    // that have them: they belong to that record, whichever is read first.
    if !have_ffmpeg() {
        eprintln!("no ffmpeg: skipped");
        return;
    }
    let root = std::env::temp_dir().join("chilpa-scan-neighbours");
    let _ = std::fs::remove_dir_all(&root);
    let record = root.join("C").join("The Cure").join("(1992) Wish [FLAC 16-44]");
    assert!(untagged_file(&record.join("01. The Cure - Wendy Time (Demo).flac")), "read before the tagged one");
    assert!(tagged_file(&record.join("02. Open.flac"), "Open", "The Cure", "Wish (Deluxe Edition)", Some("The Cure")));
    assert!(untagged_file(&record.join("03. The Cure - High (Higher Mix).flac")), "read after it");
    let lib = Library::in_memory().unwrap();
    scan_root(&lib, &root).unwrap();
    let albums = lib.albums(10, 0).unwrap();
    assert_eq!(albums.iter().map(|a| a.title.as_str()).collect::<Vec<_>>(), ["Wish (Deluxe Edition)"], "one record, the tagged one's");
    let titles: Vec<String> = lib.album_tracks(albums[0].id).unwrap().into_iter().map(|t| t.title).collect();
    assert_eq!(titles, ["Wendy Time (Demo)", "Open", "High (Higher Mix)"]);
    assert_eq!(albums[0].year, Some(1992), "the tags gave no year; the folder does");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_track_that_came_in_without_a_record_is_read_again() {
    // What earlier scans left without a record is not "unchanged": the next
    // scan reads it again and gives it one.
    if !have_ffmpeg() {
        eprintln!("no ffmpeg: skipped");
        return;
    }
    let root = std::env::temp_dir().join("chilpa-scan-orphan");
    let _ = std::fs::remove_dir_all(&root);
    let record = root.join("Q").join("Queen").join("1975 - A Night at the Opera");
    assert!(untagged_file(&record.join("01 - Death on Two Legs.flac")));
    assert!(untagged_file(&record.join("02 - Lazing on a Sunday Afternoon.flac")));
    let lib = Library::in_memory().unwrap();
    scan_root(&lib, &root).unwrap();
    // As an earlier version left them.
    lib.connection().execute("UPDATE tracks SET album_id = NULL WHERE external_id LIKE '%01 - Death%'", []).unwrap();
    let again = scan_root(&lib, &root).unwrap();
    assert_eq!((again.read, again.unchanged), (1, 1), "the orphan is read, the other is not");
    let albums = lib.albums(10, 0).unwrap();
    assert_eq!(albums.len(), 1);
    assert_eq!(lib.album_tracks(albums[0].id).unwrap().len(), 2, "both on the record again");
    let _ = std::fs::remove_dir_all(root);
}
