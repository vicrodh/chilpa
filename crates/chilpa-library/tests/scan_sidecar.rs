//! Scanning folders that carry a sidecar.

use chilpa_library::scan::{scan_root_with, ContainedTrack, Containers, NoContainers};
use chilpa_library::sidecar::{stage, store, Home, Override, Sidecar};
use chilpa_library::{scan_root, Library};
use std::path::{Path, PathBuf};
use std::process::Command;

fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("chilpa-scan-sidecar-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A tagged FLAC. False without ffmpeg.
fn tagged(target: &Path, title: &str, track: u32) -> bool {
    let _ = std::fs::create_dir_all(target.parent().unwrap());
    Command::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-f", "lavfi", "-i", "sine=frequency=440:duration=1", "-c:a", "flac"])
        .args(["-metadata", &format!("title={title}")])
        .args(["-metadata", "artist=File artist"])
        .args(["-metadata", "album_artist=File artist"])
        .args(["-metadata", "album=File album"])
        .args(["-metadata", "genre=Rock"])
        .args(["-metadata", "date=2019-03-15"])
        .args(["-metadata", &format!("track={track}")])
        .arg(target)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// A record of two files in one folder. `None` without ffmpeg.
fn record(name: &str) -> Option<PathBuf> {
    let dir = folder(name);
    (tagged(&dir.join("Record/01.flac"), "One", 1) && tagged(&dir.join("Record/02.flac"), "Two", 2)).then_some(dir)
}

fn home_of(dir: &Path) -> (Home, String) {
    let file = dir.join("Record/01.flac").to_string_lossy().into_owned();
    (Home::of(&file, None), file)
}

/// Title, artist and track number of every track, by path.
fn rows(lib: &Library) -> Vec<(String, String, String, Option<i64>, Option<i64>)> {
    let mut stmt = lib
        .connection()
        .prepare(
            "SELECT t.external_id, t.title, t.artist, t.track_number, t.disc_number
             FROM tracks t ORDER BY t.external_id",
        )
        .unwrap();
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))).unwrap();
    rows.map(|r| r.unwrap()).collect()
}

fn albums(lib: &Library) -> Vec<(String, String, Option<i64>, Option<String>)> {
    let mut stmt = lib
        .connection()
        .prepare(
            "SELECT a.title, a.album_artist, a.year, a.artwork_path FROM albums a
             WHERE EXISTS (SELECT 1 FROM tracks t WHERE t.album_id = a.id) ORDER BY a.title",
        )
        .unwrap();
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).unwrap();
    rows.map(|r| r.unwrap()).collect()
}

fn overriding() -> Sidecar {
    let mut s = Sidecar::new(0);
    s.album.title = Override::Value("Sidecar album".into());
    s.album.album_artist = Override::Value("Sidecar artist".into());
    s.album.genres = Override::Value(vec!["Blues".into(), "Soul".into()]);
    let one = s.track_mut("01.flac");
    one.title = Override::Value("Sidecar one".into());
    one.artist_credit = Override::Value("Someone feat. Someone else".into());
    one.track_number = Override::Value(9);
    one.disc_number = Override::Value(2);
    s
}

#[test]
fn a_sidecar_changes_what_the_scan_writes_down() {
    let Some(dir) = record("changes") else {
        eprintln!("no ffmpeg: skipped");
        return;
    };
    let (home, _) = home_of(&dir);
    store(&home, &overriding(), 1).unwrap();

    let lib = Library::in_memory().unwrap();
    let report = scan_root(&lib, &dir).unwrap();
    assert_eq!((report.seen, report.read), (2, 2), "a sidecar is not a track");

    let tracks = rows(&lib);
    assert_eq!(tracks[0].1, "Sidecar one");
    assert_eq!(tracks[0].2, "Someone feat. Someone else");
    assert_eq!((tracks[0].3, tracks[0].4), (Some(9), Some(2)));
    assert_eq!(tracks[1].1, "Two", "the other track keeps its own title");
    assert_eq!(tracks[1].2, "File artist");

    let records = albums(&lib);
    assert_eq!(records.len(), 1, "and both are on the record the sidecar names");
    assert_eq!((records[0].0.as_str(), records[0].1.as_str()), ("Sidecar album", "Sidecar artist"));
    assert_eq!(records[0].2, Some(2019), "the year nobody overrode is the file's");

    let id: i64 = lib.connection().query_row("SELECT id FROM tracks ORDER BY external_id LIMIT 1", [], |r| r.get(0)).unwrap();
    assert_eq!(lib.track_genres(id).unwrap(), ["Blues", "Soul"]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_second_scan_with_the_same_sidecar_rereads_nothing() {
    let Some(dir) = record("same") else {
        eprintln!("no ffmpeg: skipped");
        return;
    };
    let (home, _) = home_of(&dir);
    store(&home, &overriding(), 1).unwrap();
    let lib = Library::in_memory().unwrap();
    scan_root(&lib, &dir).unwrap();
    let before = rows(&lib);

    let again = scan_root(&lib, &dir).unwrap();
    assert_eq!((again.read, again.unchanged), (0, 2));
    assert_eq!(rows(&lib), before);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_changed_sidecar_rereads_its_folder_and_no_other() {
    let Some(dir) = record("changed") else {
        eprintln!("no ffmpeg: skipped");
        return;
    };
    assert!(tagged(&dir.join("Other/01.flac"), "Elsewhere", 1));
    let lib = Library::in_memory().unwrap();
    scan_root(&lib, &dir).unwrap();

    let (home, _) = home_of(&dir);
    store(&home, &overriding(), 1).unwrap();
    let report = scan_root(&lib, &dir).unwrap();
    assert_eq!((report.read, report.unchanged), (2, 1), "the two beside the sidecar, and not the third");
    assert_eq!(rows(&lib).iter().filter(|r| r.1 == "Sidecar one").count(), 1);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_removed_sidecar_gives_the_files_their_own_word_back() {
    let Some(dir) = record("removed") else {
        eprintln!("no ffmpeg: skipped");
        return;
    };
    let (home, _) = home_of(&dir);
    store(&home, &overriding(), 1).unwrap();
    let lib = Library::in_memory().unwrap();
    scan_root(&lib, &dir).unwrap();

    std::fs::remove_file(home.sidecar()).unwrap();
    let report = scan_root(&lib, &dir).unwrap();
    assert_eq!(report.read, 2);
    let tracks = rows(&lib);
    assert_eq!(tracks[0].1, "One");
    assert_eq!((tracks[0].3, tracks[0].4), (Some(1), None));
    assert_eq!(albums(&lib)[0].0, "File album");

    let kept: i64 = lib.connection().query_row("SELECT count(*) FROM sidecars", [], |r| r.get(0)).unwrap();
    assert_eq!(kept, 0, "and the library forgets a sidecar that is gone");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_sidecar_cover_replaces_the_album_s_cover() {
    let Some(dir) = record("cover") else {
        eprintln!("no ffmpeg: skipped");
        return;
    };
    std::fs::write(dir.join("Record/cover.jpg"), b"the folder's").unwrap();
    let lib = Library::in_memory().unwrap();
    let cache = dir.join("cache");
    chilpa_library::scan_root_with_cache(&lib, &dir, Some(&cache)).unwrap();
    assert!(albums(&lib)[0].3.as_deref().unwrap().ends_with("cover.jpg"));

    let (home, _) = home_of(&dir);
    stage(&home, Sidecar::new(0), Some((b"the chosen one", "jpg")), 1).unwrap().land().unwrap();
    chilpa_library::scan_root_with_cache(&lib, &dir, Some(&cache)).unwrap();
    assert_eq!(albums(&lib)[0].3.as_deref(), Some(home.cover("jpg").to_string_lossy().as_ref()));
    assert_eq!(std::fs::read(dir.join("Record/cover.jpg")).unwrap(), b"the folder's", "and the folder's own is as it was");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_sidecar_that_cannot_be_read_is_counted_and_skipped() {
    let Some(dir) = record("junk") else {
        eprintln!("no ffmpeg: skipped");
        return;
    };
    std::fs::write(dir.join("Record/.chilpa.json"), b"{ not json").unwrap();
    let lib = Library::in_memory().unwrap();
    let report = scan_root(&lib, &dir).unwrap();
    assert_eq!(report.sidecars_unreadable, 1);
    assert_eq!(report.read, 2);
    assert_eq!(rows(&lib)[0].1, "One");

    let again = scan_root(&lib, &dir).unwrap();
    assert_eq!(again.read, 0, "and it is not a reason to read the folder every time");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_qbz_sidecar_is_honored_when_there_is_no_other() {
    let Some(dir) = record("qbz") else {
        eprintln!("no ffmpeg: skipped");
        return;
    };
    // The path QBZ knew the file by is from before the folder was renamed.
    let theirs = r#"{"version":1,"updatedAt":1,
        "album":{"albumTitle":"From QBZ","albumArtist":null,"year":1999,"genre":null,"catalogNumber":null},
        "tracks":[{"filePath":"/somewhere/else/entirely/01.flac","cueStartSecs":null,"title":"Theirs","discNumber":null,"trackNumber":null}]}"#;
    std::fs::write(dir.join("Record/.qbz.json"), theirs).unwrap();
    let lib = Library::in_memory().unwrap();
    scan_root(&lib, &dir).unwrap();
    assert_eq!(rows(&lib)[0].1, "Theirs");
    assert_eq!(albums(&lib)[0].0, "From QBZ");
    assert_eq!(albums(&lib)[0].2, Some(1999));
    assert_eq!(std::fs::read_to_string(dir.join("Record/.qbz.json")).unwrap(), theirs);
    let _ = std::fs::remove_dir_all(dir);
}

/// A file that holds two tracks, standing in for a disc image.
struct Boxes;

impl Containers for Boxes {
    fn may_hold(&self, path: &Path) -> bool {
        path.extension().is_some_and(|e| e == "box")
    }

    fn tracks(&self, path: &Path) -> Option<Vec<ContainedTrack>> {
        Some(
            (1..=2u16)
                .map(|n| ContainedTrack {
                    path: format!("box:{}#{n}", path.display()),
                    info: chilpa_tags::FileInfo {
                        tags: chilpa_tags::Tags {
                            title: Some(format!("Boxed {n}")),
                            artists: vec!["Boxer".into()],
                            album: Some("The Box".into()),
                            album_artist: Some("Boxer".into()),
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

#[test]
fn a_disc_s_sidecar_reaches_the_tracks_inside_it() {
    let dir = folder("disc");
    let disc = dir.join("album.box");
    std::fs::write(&disc, b"box").unwrap();
    let held = format!("box:{}#2", disc.display());
    let home = Home::of(&held, Some(&disc.to_string_lossy()));
    assert_eq!(home.key(&held), "#2");
    let mut s = Sidecar::new(0);
    s.album.title = Override::Value("The Box, corrected".into());
    s.track_mut("#2").title = Override::Value("Second, corrected".into());
    store(&home, &s, 1).unwrap();

    let lib = Library::in_memory().unwrap();
    let report = scan_root_with(&lib, &dir, None, &|_| {}, &Boxes).unwrap();
    assert_eq!((report.seen, report.read), (2, 2));
    let tracks = rows(&lib);
    assert_eq!(tracks[0].1, "Boxed 1");
    assert_eq!(tracks[1].1, "Second, corrected");
    assert_eq!(albums(&lib)[0].0, "The Box, corrected");

    let again = scan_root_with(&lib, &dir, None, &|_| {}, &Boxes).unwrap();
    assert_eq!((again.read, again.unchanged), (0, 2));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_sidecar_written_outside_a_scan_is_not_news_to_the_next_one() {
    let Some(dir) = record("noted") else {
        eprintln!("no ffmpeg: skipped");
        return;
    };
    let lib = Library::in_memory().unwrap();
    scan_root(&lib, &dir).unwrap();
    let (home, file) = home_of(&dir);
    store(&home, &overriding(), 1).unwrap();
    lib.note_home(&home, &file, 2).unwrap();

    let report = scan_root_with(&lib, &dir, None, &|_| {}, &NoContainers).unwrap();
    assert_eq!(report.read, 0, "whoever wrote it said so");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_scan_after_an_edit_changes_nothing() {
    use chilpa_library::{AlbumEdit, Put, TrackEdit};

    let Some(dir) = record("invariant") else {
        eprintln!("no ffmpeg: skipped");
        return;
    };
    let lib = Library::in_memory().unwrap();
    scan_root(&lib, &dir).unwrap();
    let album: i64 = lib.connection().query_row("SELECT id FROM albums LIMIT 1", [], |r| r.get(0)).unwrap();
    let tracks = lib.edit_tracks(album).unwrap();

    // What an editor does: the sidecar first, then the library.
    let (home, file) = home_of(&dir);
    store(&home, &overriding(), 1).unwrap();
    lib.note_home(&home, &file, 1).unwrap();
    let mut first = TrackEdit::none(tracks[0].id);
    first.title = Put::Set("Sidecar one".into());
    first.artist = Put::Set("Someone feat. Someone else".into());
    first.track_number = Put::Set(Some(9));
    first.disc_number = Put::Set(Some(2));
    first.genres = Put::Set(vec!["Blues".into(), "Soul".into()]);
    let mut second = TrackEdit::none(tracks[1].id);
    second.genres = Put::Set(vec!["Blues".into(), "Soul".into()]);
    lib.apply_edit(&AlbumEdit {
        album_id: album,
        title: "Sidecar album".into(),
        album_artist: "Sidecar artist".into(),
        year: Put::Keep,
        label: Put::Keep,
        artwork: Put::Keep,
        tracks: vec![first, second],
    })
    .unwrap();
    let edited = (rows(&lib), albums(&lib));

    // The files are made to look changed, so the scan reads every one again.
    lib.connection().execute("UPDATE scanned_files SET mtime = 0", []).unwrap();
    let report = scan_root(&lib, &dir).unwrap();
    assert_eq!(report.read, 2);
    assert_eq!((rows(&lib), albums(&lib)), edited, "the scan and the edit say the same");
    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(unix)]
#[test]
fn a_sidecar_that_cannot_be_read_this_time_is_asked_again_next_time() {
    use std::os::unix::fs::PermissionsExt;

    let Some(dir) = record("transient") else {
        eprintln!("no ffmpeg: skipped");
        return;
    };
    let (home, _) = home_of(&dir);
    store(&home, &overriding(), 1).unwrap();
    let lib = Library::in_memory().unwrap();
    scan_root(&lib, &dir).unwrap();
    let with = rows(&lib);

    // Changed, and then out of reach for one scan: what the library has from
    // it stays, and nothing is written down about it.
    store(&home, &overriding(), 2).unwrap();
    std::fs::set_permissions(home.sidecar(), std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::read(home.sidecar()).is_ok() {
        // Running as a user no permission stops.
        eprintln!("the sidecar is readable anyway: skipped");
        let _ = std::fs::remove_dir_all(dir);
        return;
    }
    let report = scan_root(&lib, &dir).unwrap();
    assert_eq!(report.read, 0, "the files are not read again without their sidecar");
    assert_eq!(rows(&lib), with);

    std::fs::set_permissions(home.sidecar(), std::fs::Permissions::from_mode(0o644)).unwrap();
    let report = scan_root(&lib, &dir).unwrap();
    assert_eq!(report.read, 2, "the next scan asks again");
    assert_eq!(rows(&lib), with);
    let _ = std::fs::remove_dir_all(dir);
}
