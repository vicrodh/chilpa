use chilpa_library::{Filter, Library, TrackOrder};
use rusqlite::ErrorCode;
use std::path::PathBuf;

fn path(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("chilpa-library-reader-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("library.db")
}

#[test]
fn a_reader_skips_maintenance_and_observes_later_commits() {
    let path = path("commits");
    let lib = Library::open(&path).unwrap();
    let source = lib.source_id("local", "default").unwrap();
    lib.connection().execute_batch("INSERT INTO albums(id, title, album_artist, folder)
        VALUES (1, 'Record', 'Artist', '/music');
        INSERT INTO genres(id, name) VALUES (1, 'Rock; Ambient');").unwrap();
    lib.connection().execute("INSERT INTO tracks(id, source_id, external_id, title, album_id, added_at)
        VALUES (1, ?1, '/music/01.flac', 'Committed title', 1, 1)", [source]).unwrap();
    lib.connection().execute("INSERT INTO track_genres VALUES (1, 1)", []).unwrap();

    let tx = lib.connection().unchecked_transaction().unwrap();
    tx.execute("UPDATE tracks SET title = 'Later title' WHERE id = 1", []).unwrap();
    let reader = Library::open_read_only(&path).unwrap();
    assert_eq!(reader.grouped_snapshot(&Filter::default(), TrackOrder::Album).unwrap().totals.tracks, 1);
    assert_eq!(reader.tracks_by_ids(&[1]).unwrap()[0].title, "Committed title");
    tx.commit().unwrap();
    assert_eq!(reader.tracks_by_ids(&[1]).unwrap()[0].title, "Later title");
    assert_eq!(reader.track_genres(1).unwrap(), ["Rock; Ambient"], "opening a reader must not normalize genres");
    let release: String = reader.connection().query_row("SELECT release_part FROM tracks WHERE id = 1", [], |r| r.get(0)).unwrap();
    assert!(release.is_empty(), "opening a reader must not assemble releases");
    assert_eq!(reader.connection().total_changes(), 0);
    let error = reader.connection().execute("UPDATE tracks SET present = 0", []).unwrap_err();
    assert_eq!(error.sqlite_error_code(), Some(ErrorCode::ReadOnly));
}

#[test]
fn a_reader_never_creates_or_migrates_a_database() {
    let path = path("schema");
    assert!(Library::open_read_only(&path).is_err());
    assert!(!path.exists(), "a reader must not create an empty library");
    let lib = Library::open(&path).unwrap();
    for version in [chilpa_library::schema::VERSION - 1, chilpa_library::schema::VERSION + 1] {
        lib.connection().pragma_update(None, "user_version", version).unwrap();
        let error = Library::open_read_only(&path).err().expect("a reader requires the current schema");
        assert!(error.to_string().contains(&format!("found {version}")));
        assert_eq!(lib.connection().query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0)).unwrap(), version);
    }
}

#[test]
#[ignore = "requires CHILPA_TEST_LIBRARY; works only on a fresh copy of that file"]
fn a_copied_library_remains_readable_during_a_writer() {
    let original = std::env::var_os("CHILPA_TEST_LIBRARY").expect("set CHILPA_TEST_LIBRARY to a database backup");
    let path = path("copy");
    std::fs::copy(original, &path).unwrap();
    let lib = Library::open(&path).unwrap();
    let expected = lib.grouped_snapshot(&Filter::default(), TrackOrder::Album).unwrap();
    assert!(expected.totals.tracks > 0);
    let tx = lib.connection().unchecked_transaction().unwrap();
    tx.execute("UPDATE tracks SET present = present WHERE id = (SELECT min(id) FROM tracks)", []).unwrap();
    let start = std::time::Instant::now();
    let error = Library::open(&path).err().expect("maintenance must hit the read-to-write upgrade");
    assert_eq!(error.sqlite_error_code(), Some(ErrorCode::DatabaseBusy));
    eprintln!("maintenance open: {error} after {:?}", start.elapsed());
    let reader = Library::open_read_only(&path).unwrap();
    let snapshot = reader.grouped_snapshot(&Filter::default(), TrackOrder::Album).unwrap();
    assert_eq!(snapshot.ids, expected.ids);
    assert_eq!(snapshot.album_ids, expected.album_ids);
    assert_eq!(snapshot.totals, expected.totals);
    eprintln!("read-only snapshot during writer: {} albums, {} tracks", snapshot.totals.albums, snapshot.totals.tracks);
    tx.rollback().unwrap();
}
