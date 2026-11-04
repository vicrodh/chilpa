//! Removing a server away from the window, against a library on disk that
//! another writer holds.

use super::*;
use std::cell::Cell;

fn busy() -> rusqlite::Error {
    rusqlite::Error::SqliteFailure(rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY), None)
}

#[test]
fn a_busy_library_is_tried_again_and_one_that_stays_busy_is_said() {
    let short = [Duration::from_millis(5); 3];
    let tries = Cell::new(0);
    let got = retried("a test write", &short, chilpa_library::is_busy, || {
        tries.set(tries.get() + 1);
        if tries.get() < 3 { Err(busy()) } else { Ok(7) }
    });
    assert_eq!((got.ok(), tries.get()), (Some(7), 3), "busy twice, then written");

    let tries = Cell::new(0);
    let got: rusqlite::Result<()> = retried("a test write", &short, chilpa_library::is_busy, || {
        tries.set(tries.get() + 1);
        Err(busy())
    });
    assert!(got.is_err());
    assert_eq!(tries.get(), short.len() + 1, "every wait, and no more");
    let error = got.unwrap_err();
    let refusal = Refusal::library("Lab could not be removed".into(), &error, chilpa_library::is_busy(&error));
    assert!(refusal.advice.contains("busy") && !refusal.detail.is_empty(), "{refusal:?}");

    let tries = Cell::new(0);
    let got: rusqlite::Result<()> = retried("a test write", &short, chilpa_library::is_busy, || {
        tries.set(tries.get() + 1);
        Err(rusqlite::Error::InvalidQuery)
    });
    assert!(got.is_err() && tries.get() == 1, "anything but busy is not tried again");
}

/// A library on disk with a server of `tracks` tracks, one of them in a
/// playlist of mine.
fn library_with_a_server(tracks: usize) -> (std::path::PathBuf, i64) {
    let path = std::env::temp_dir().join(format!("chilpa-remove-{}.db", super::super::keys::mint()));
    let lib = chilpa_library::Library::open(&path).unwrap();
    let source = lib
        .add_server(&chilpa_library::NewServer {
            provider: "subsonic".into(),
            instance: super::super::keys::mint(),
            label: "Lab".into(),
            address: "http://lab.lan".into(),
            libraries: "[]".into(),
            details: "{}".into(),
            ..Default::default()
        })
        .unwrap();
    let generation = lib.begin_server_sync(source).unwrap();
    let songs: Vec<chilpa_library::ServerTrack> = (0..tracks)
        .map(|n| chilpa_library::ServerTrack {
            external_id: format!("remote:subsonic/lab/{n}"),
            title: format!("Song {n}"),
            artist: "Lab Artist".into(),
            album: format!("Record {}", n / 10),
            album_artist: "Lab Artist".into(),
            genres: vec!["Lab".into()],
            duration_ms: 1_000,
            ..Default::default()
        })
        .collect();
    lib.save_server_tracks(source, generation, &songs).unwrap();
    let first: i64 = lib.connection().query_row("SELECT id FROM tracks WHERE source_id = ?1 ORDER BY id LIMIT 1", [source], |r| r.get(0)).unwrap();
    let local = lib.source_id("local", "default").unwrap();
    let mine = lib.create_playlist(local, "Mine", "").unwrap();
    let entries = lib.playlist_entries_for_tracks(&[first]).unwrap();
    assert_eq!(lib.append_to_playlist(mine, &entries).unwrap(), 1);
    (path, source)
}

#[test]
fn a_server_is_taken_out_whole_while_another_writer_holds_the_library_a_while() {
    let (path, source) = library_with_a_server(1_234);
    // Another writer, as a sync is: it holds the library for a second.
    let held = path.clone();
    let writer = std::thread::spawn(move || {
        let other = chilpa_library::Library::open_existing(&held).unwrap();
        other.connection().execute_batch("BEGIN IMMEDIATE").unwrap();
        std::thread::sleep(Duration::from_secs(1));
        other.connection().execute_batch("COMMIT").unwrap();
    });
    std::thread::sleep(Duration::from_millis(100));
    take(&path, None, source, "lab", "Lab", &WAITS).expect("taken out once the other writer let go");
    writer.join().unwrap();

    let lib = chilpa_library::Library::open(&path).unwrap();
    let left: i64 = lib.connection().query_row("SELECT count(*) FROM tracks WHERE source_id = ?1", [source], |r| r.get(0)).unwrap();
    assert_eq!(left, 0, "no track of it is left");
    assert!(lib.server(source).unwrap().is_none(), "nor the server");
    let records: i64 = lib.connection().query_row("SELECT count(*) FROM albums", [], |r| r.get(0)).unwrap();
    assert_eq!(records, 0, "nor a record only it had");
    let genres: i64 = lib.connection().query_row("SELECT count(*) FROM genres WHERE name = 'Lab'", [], |r| r.get(0)).unwrap();
    assert_eq!(genres, 0, "nor a genre only it had");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_server_s_tracks_go_a_part_at_a_time() {
    let (path, source) = library_with_a_server(1_234);
    let lib = chilpa_library::Library::open_existing(&path).unwrap();
    assert_eq!(lib.remove_source_tracks(source, PART).unwrap(), PART);
    assert_eq!(lib.remove_source_tracks(source, PART).unwrap(), PART);
    assert_eq!(lib.remove_source_tracks(source, PART).unwrap(), 1_234 - 2 * PART - 1, "the one in a playlist waits for the rest");
    assert_eq!(lib.remove_source_tracks(source, PART).unwrap(), 0);
    lib.remove_source(source).unwrap();
    let left: i64 = lib.connection().query_row("SELECT count(*) FROM tracks WHERE source_id = ?1", [source], |r| r.get(0)).unwrap();
    assert_eq!(left, 0);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_server_being_removed_says_so_in_its_row_and_its_setting() {
    let lib = chilpa_library::Library::in_memory().unwrap();
    let instance = super::super::keys::mint();
    let source = lib
        .add_server(&chilpa_library::NewServer {
            provider: "subsonic".into(),
            instance: instance.clone(),
            label: "Lab".into(),
            address: "http://lab.lan".into(),
            libraries: "[]".into(),
            details: "{}".into(),
            ..Default::default()
        })
        .unwrap();
    let mut api = Chilpa::new();
    api.library = Some(lib);
    let (_ends, done) = std::sync::mpsc::channel();
    api.servers.removing = Some(Removing { id: source, label: "Lab".into(), instance, stage: Stage::Taking(done), asked: true });

    let row = api.servers().into_iter().find(|row| row.id == source).unwrap();
    assert_eq!(row.state, chilpa_i18n::t("Being removed…"));
    let notes = crate::settings_rows::RowNotes { report: None, said: None, plugin_link: None };
    let key = format!("sources.server/{source}");
    let setting = api.server_rows(&notes).into_iter().find(|row| row.key == key).unwrap();
    assert_eq!(setting.action, chilpa_i18n::t("Removing…"), "not Syncing, whatever else it was doing");
}
