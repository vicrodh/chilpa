//! Step 57: the tracks table rebuilt so an id is never given twice.
use super::*;
use rusqlite::types::Value;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// A library at 56 with gaps in its ids, a deleted newest track and a row in
/// every table that names a track.
const FILLED: &str = "
    INSERT INTO sources(id, provider, instance) VALUES (1, 'local', 'default');
    INSERT INTO roots(id, source_id, path) VALUES (1, 1, '/music');
    INSERT INTO albums(id, title, album_artist) VALUES (1, 'Record', 'Artist');
    INSERT INTO tracks(id, source_id, external_id, title, artist, album_id, added_at, album_credit, label, work) VALUES
        (1, 1, '/music/1.flac', 'Opening', 'Artist', 1, 1, 'Artist', 'Label', NULL),
        (2, 1, '/music/2.flac', 'Second Song', 'Artist', 1, 2, 'Artist', NULL, 'Suite'),
        (5, 1, '/music/5.flac', 'Fifth', 'Guest', 1, 3, 'Artist', NULL, NULL),
        (9, 1, '/music/9.flac', 'Ninth', 'Artist', NULL, 4, '', NULL, NULL),
        (12, 1, '/music/12.flac', 'Gone', 'Artist', 1, 5, 'Artist', NULL, NULL);
    DELETE FROM tracks WHERE id = 12;
    INSERT INTO scanned_files(root_id, path, mtime, size, track_id, scanned_at) VALUES (1, '/music/1.flac', 7, 8, 1, 9);
    INSERT INTO play_events(track_id, source_id, started_at, played_ms, duration_ms) VALUES (2, 1, 100, 1000, 2000);
    INSERT INTO audio_features(track_id, analyzer, version, bpm, analyzed_at) VALUES (5, 'chilpa', 1, 120.0, 0);
    INSERT INTO playlists(id, source_id, name, created_at, updated_at) VALUES (1, 1, 'List', 0, 0);
    INSERT INTO playlist_tracks(playlist_id, position, track_id, external_id) VALUES (1, 0, 9, '/music/9.flac');
    INSERT INTO genres(id, name) VALUES (1, 'Rock');
    INSERT INTO track_genres(track_id, genre_id) VALUES (1, 1);
    INSERT INTO composers(id, name) VALUES (1, 'Someone');
    INSERT INTO track_composers(track_id, composer_id) VALUES (2, 1);
    INSERT INTO track_favorites(track_id, added_at) VALUES (5, 0);
    INSERT INTO track_ratings(track_id, stars, revision, updated_at) VALUES (9, 4, 1, 0);
    INSERT INTO mood_features(track_id, analyzer, version, mood_happy, mood_sad, mood_relaxed, mood_aggressive,
        mood_party, danceability, embedding, analyzed_at) VALUES (1, 'mood', 1, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, x'00', 0);
    INSERT INTO analysis_runs(id, analyzer, version, started_at, total, previous) VALUES (1, 'chilpa', 1, 0, 1, 0);
    INSERT INTO analysis_results(run_id, track_id, state, analyzed_at) VALUES (1, 2, 'done', 0);
    INSERT INTO local_release_members(track_id, group_key, title, artist, evidence) VALUES (5, 'g', 'Record', 'Artist', '{}');
";

fn filled() -> Connection {
    let db = Connection::open_in_memory().unwrap();
    apply_pragmas(&db).unwrap();
    stop_at(&db, 56);
    db.execute_batch(FILLED).unwrap();
    db
}

fn on_disk(path: &Path) {
    let db = Connection::open(path).unwrap();
    apply_pragmas(&db).unwrap();
    stop_at(&db, 56);
    db.execute_batch(FILLED).unwrap();
}

/// Every row of a table, as values, in one order whatever the table's key.
fn dump(db: &Connection, table: &str) -> Vec<Vec<Value>> {
    let mut statement = db.prepare(&format!("SELECT * FROM \"{table}\"")).unwrap();
    let width = statement.column_count();
    let mut rows: Vec<Vec<Value>> = statement.query_map([], |row| (0..width).map(|i| row.get::<_, Value>(i)).collect()).unwrap().map(Result::unwrap).collect();
    rows.sort_by_key(|row| format!("{row:?}"));
    rows
}

/// The tables that hold rows: everything but SQLite's own and the search
/// index's shadow tables.
fn tables(db: &Connection) -> Vec<String> {
    db.prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name NOT LIKE 'track_search_%' ORDER BY name").unwrap()
        .query_map([], |row| row.get(0)).unwrap().map(Result::unwrap).collect()
}

fn everything(db: &Connection) -> Vec<(String, Vec<Vec<Value>>)> {
    tables(db).into_iter().map(|table| { let rows = dump(db, &table); (table, rows) }).collect()
}

fn schema_of(db: &Connection) -> Vec<(String, String, String, Option<String>)> {
    db.prepare("SELECT type, name, tbl_name, sql FROM sqlite_master ORDER BY type, name").unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))).unwrap().map(Result::unwrap).collect()
}

fn version(db: &Connection) -> i64 {
    db.query_row("PRAGMA user_version", [], |row| row.get(0)).unwrap()
}

fn insert(db: &Connection, external: &str) -> i64 {
    db.execute("INSERT INTO tracks(source_id, external_id, title, added_at) VALUES (1, ?1, 'New', 0)", [external]).unwrap();
    db.last_insert_rowid()
}

#[test]
fn every_id_row_and_column_is_kept() {
    let db = filled();
    let before = everything(&db);
    let columns: Vec<String> = db.prepare("PRAGMA table_info(tracks)").unwrap().query_map([], |row| row.get(1)).unwrap().map(Result::unwrap).collect();
    migrate(&db).unwrap();
    assert_eq!(version(&db), 57);
    let after_columns: Vec<String> = db.prepare("PRAGMA table_info(tracks)").unwrap().query_map([], |row| row.get(1)).unwrap().map(Result::unwrap).collect();
    assert_eq!(after_columns, columns);
    assert_eq!(columns.len(), 33);
    let ids: Vec<i64> = db.prepare("SELECT id FROM tracks ORDER BY id").unwrap().query_map([], |row| row.get(0)).unwrap().map(Result::unwrap).collect();
    assert_eq!(ids, [1, 2, 5, 9], "the gaps stay gaps");
    let after = everything(&db);
    for ((table, rows), (_, kept)) in before.iter().zip(&after) {
        assert_eq!(rows, kept, "{table} keeps every row");
    }
}

#[test]
fn only_the_tracks_table_changes_in_the_schema() {
    let db = filled();
    let before = schema_of(&db);
    migrate(&db).unwrap();
    let after = schema_of(&db);
    let others = |schema: &[(String, String, String, Option<String>)]| schema.iter().filter(|row| !(row.0 == "table" && row.1 == "tracks")).cloned().collect::<Vec<_>>();
    assert_eq!(others(&after), others(&before), "every index and trigger, of tracks and of the rest, is as it was");
    let table: String = db.query_row("SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'tracks'", [], |row| row.get(0)).unwrap();
    assert!(table.contains("AUTOINCREMENT"), "{table}");
}

#[test]
fn every_row_elsewhere_still_finds_its_track() {
    let db = filled();
    migrate(&db).unwrap();
    assert_eq!(db.prepare("PRAGMA foreign_key_check").unwrap().query_map([], |_| Ok(())).unwrap().count(), 0);
    for (table, track) in [("scanned_files", 1), ("play_events", 2), ("audio_features", 5), ("playlist_tracks", 9), ("track_genres", 1),
        ("track_composers", 2), ("track_favorites", 5), ("track_ratings", 9), ("mood_features", 1), ("analysis_results", 2), ("local_release_members", 5)] {
        let found: i64 = db.query_row(&format!("SELECT count(*) FROM {table} c JOIN tracks t ON t.id = c.track_id WHERE t.id = ?1"), [track], |row| row.get(0)).unwrap();
        assert_eq!(found, 1, "{table} still points at track {track}");
    }
    let favorite: String = db.query_row("SELECT t.title FROM track_favorites f JOIN tracks t ON t.id = f.track_id", [], |row| row.get(0)).unwrap();
    assert_eq!(favorite, "Fifth");
}

#[test]
fn a_deleted_newest_id_is_never_given_again() {
    let db = filled();
    migrate(&db).unwrap();
    // An id freed before the upgrade is past knowing; from here on none is.
    assert!(insert(&db, "/music/13.flac") > 9);
    let newest = insert(&db, "/music/new.flac");
    db.execute("DELETE FROM tracks WHERE id = ?1", [newest]).unwrap();
    assert!(insert(&db, "/music/after.flac") > newest, "a freed id is never handed out again");
}

#[test]
fn a_scan_a_server_sync_and_a_source_list_never_reuse_an_id() {
    use crate::store::{NewServer, PlaylistTrackMetadata, ServerTrack};
    let directory = tempfile::tempdir().unwrap();
    let wave = |name: &str| {
        let mut bytes = b"RIFF".to_vec();
        bytes.extend(44u32.to_le_bytes());
        bytes.extend(b"WAVEfmt ");
        bytes.extend(16u32.to_le_bytes());
        bytes.extend(1u16.to_le_bytes());
        bytes.extend(1u16.to_le_bytes());
        bytes.extend(8_000u32.to_le_bytes());
        bytes.extend(16_000u32.to_le_bytes());
        bytes.extend(2u16.to_le_bytes());
        bytes.extend(16u16.to_le_bytes());
        bytes.extend(b"data");
        bytes.extend(8u32.to_le_bytes());
        bytes.extend([0; 8]);
        std::fs::write(directory.path().join(name), bytes).unwrap();
    };
    let lib = crate::Library::in_memory().unwrap();
    let newest = |lib: &crate::Library| lib.connection().query_row("SELECT max(id) FROM tracks", [], |row| row.get::<_, i64>(0)).unwrap();
    let forget = |lib: &crate::Library, id: i64| lib.connection().execute_batch(&format!("DELETE FROM scanned_files WHERE track_id = {id}; DELETE FROM tracks WHERE id = {id};")).unwrap();
    wave("a.wav");
    wave("b.wav");
    crate::scan::scan_root(&lib, directory.path()).unwrap();
    let scanned = newest(&lib);
    forget(&lib, scanned);
    wave("c.wav");
    crate::scan::scan_root(&lib, directory.path()).unwrap();
    assert!(newest(&lib) > scanned, "a scan takes a new id");

    let server = lib.add_server(&NewServer { provider: "example-server".into(), instance: "a".into(), label: "A".into(), icon: "server".into(),
        address: "http://a.lan".into(), libraries: "[]".into(), details: "{}".into(), ..NewServer::default() }).unwrap();
    let sync = |entries: &[&str]| {
        let generation = lib.begin_server_sync(server).unwrap();
        let tracks: Vec<_> = entries.iter().map(|entry| ServerTrack { external_id: entry.to_string(), title: entry.to_string(), artist: "Band".into(),
            album: "One".into(), album_artist: "Band".into(), duration_ms: 1_000, ..ServerTrack::default() }).collect();
        lib.save_server_tracks(server, generation, &tracks).unwrap();
        lib.finish_server_sync(server, generation).unwrap();
    };
    sync(&["one"]);
    let synced = newest(&lib);
    forget(&lib, synced);
    sync(&["two"]);
    assert!(newest(&lib) > synced, "a sync takes a new id");

    let source = lib.source_id("guest", "default").unwrap();
    let listed = |external: &str| PlaylistTrackMetadata { external_id: external.into(), title: external.into(), ..PlaylistTrackMetadata::default() };
    lib.cache_playlist_tracks(source, &[listed("x")]).unwrap();
    let cached = newest(&lib);
    forget(&lib, cached);
    lib.cache_playlist_tracks(source, &[listed("y")]).unwrap();
    assert!(newest(&lib) > cached, "a source's list takes a new id");
}

#[test]
fn the_next_id_passes_ids_that_rows_elsewhere_still_name() {
    let db = filled();
    // A play of a track that is gone, from when nothing checked: kept as it
    // is, and its id never goes to a new track that would inherit it.
    db.execute_batch("PRAGMA foreign_keys = OFF;
        INSERT INTO play_events(track_id, source_id, started_at, played_ms) VALUES (100, 1, 0, 1);
        PRAGMA foreign_keys = ON;").unwrap();
    let broken = |db: &Connection| db.prepare("PRAGMA foreign_key_check").unwrap().query_map([], |_| Ok(())).unwrap().count();
    let before = broken(&db);
    assert!(before > 0);
    migrate(&db).unwrap();
    assert_eq!(version(&db), 57);
    assert_eq!(broken(&db), before, "what was already broken is kept, not fixed and not refused");
    assert!(insert(&db, "/music/new.flac") > 100);
}

#[test]
fn the_search_finds_old_and_new_tracks_by_the_same_rowid() {
    let db = filled();
    migrate(&db).unwrap();
    let found = |word: &str| db.query_row("SELECT rowid FROM track_search WHERE track_search MATCH ?1", [word], |row| row.get::<_, i64>(0)).unwrap();
    assert_eq!(found("Second"), 2);
    let added = insert(&db, "/music/new.flac");
    db.execute("UPDATE tracks SET title = 'Brandnew' WHERE id = ?1", [added]).unwrap();
    assert_eq!(found("Brandnew"), added);
}

#[test]
fn an_insert_that_finds_its_track_keeps_its_id_and_spends_a_number() {
    let db = filled();
    migrate(&db).unwrap();
    let again = || {
        db.execute("INSERT INTO tracks(source_id, external_id, title, added_at) VALUES (1, '/music/1.flac', 'Again', 0)
            ON CONFLICT(source_id, external_id) DO UPDATE SET title = excluded.title", []).unwrap();
        db.query_row("SELECT id FROM tracks WHERE external_id = '/music/1.flac'", [], |row| row.get::<_, i64>(0)).unwrap()
    };
    let sequence = || db.query_row("SELECT seq FROM sqlite_sequence WHERE name = 'tracks'", [], |row| row.get::<_, i64>(0)).unwrap();
    let before = sequence();
    assert_eq!(again(), 1);
    assert_eq!(again(), 1, "the track keeps its id");
    assert!(sequence() > before, "each insert that finds its track spends a number: ids grow sparse, by design");
}

#[test]
fn a_failure_after_the_drop_leaves_fifty_six_as_it_was() {
    let db = filled();
    let (schema, rows) = (schema_of(&db), everything(&db));
    rebuild::TROUBLE.with(|cell| cell.set(Some("SELECT missing FROM a_table_that_is_not_there;")));
    let failed = migrate(&db);
    rebuild::TROUBLE.with(|cell| cell.set(None));
    let error = failed.unwrap_err().to_string();
    assert!(error.contains("a_table_that_is_not_there"), "{error}");
    assert_eq!(version(&db), 56);
    assert_eq!(schema_of(&db), schema, "no table, index or trigger was lost");
    assert_eq!(everything(&db), rows);
    assert_eq!(db.query_row("PRAGMA foreign_keys", [], |row| row.get::<_, i64>(0)).unwrap(), 1, "the connection's settings are back");
    migrate(&db).unwrap();
    assert_eq!(version(&db), 57);
}

#[test]
fn a_change_the_rebuild_did_not_mean_rolls_it_back() {
    let db = filled();
    let rows = everything(&db);
    rebuild::TROUBLE.with(|cell| cell.set(Some("DELETE FROM tracks WHERE id = 2;")));
    let failed = migrate(&db);
    rebuild::TROUBLE.with(|cell| cell.set(None));
    assert!(failed.is_err(), "a row lost on the way is refused");
    assert_eq!(version(&db), 56);
    assert_eq!(everything(&db), rows);
}

#[test]
fn the_cover_hears_each_part_of_the_rebuild() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("library.db");
    on_disk(&path);
    let mut heard = Vec::new();
    upgrade::run(&path, &mut |progress| heard.push(*progress), &upgrade::Stop::default()).unwrap();
    let parts: Vec<f64> = heard.iter().filter(|p| p.phase == upgrade::Phase::Step && p.version == 57).map(|p| p.fraction).collect();
    assert_eq!(parts.len(), rebuild::PARTS);
    assert!(parts.windows(2).all(|pair| pair[0] < pair[1]), "{parts:?}");
    assert!(heard.windows(2).all(|pair| pair[0].fraction <= pair[1].fraction));
}

#[test]
fn child_rebuilds_until_killed() {
    let Some(path) = std::env::var_os("CHILPA_TEST_REBUILD_LIBRARY") else { return };
    upgrade::run(Path::new(&path), &mut |_| {}, &upgrade::Stop::default()).unwrap();
}

#[test]
fn a_kill_between_the_copy_and_the_commit_leaves_fifty_six() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("library.db");
    on_disk(&path);
    let rows = everything(&Connection::open(&path).unwrap());
    let ready = directory.path().join("copied");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "schema::track_ids_tests::child_rebuilds_until_killed", "--test-threads=1"])
        .env("CHILPA_TEST_REBUILD_LIBRARY", &path).env("CHILPA_TEST_REBUILD_PAUSE", &ready)
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    while !ready.exists() {
        assert!(Instant::now() < deadline, "the child never copied the table");
        assert!(child.try_wait().unwrap().is_none(), "the child ended before the copy");
        std::thread::sleep(Duration::from_millis(10));
    }
    child.kill().unwrap();
    child.wait().unwrap();
    let db = Connection::open(&path).unwrap();
    assert_eq!(version(&db), 56, "what was not committed is gone");
    assert_eq!(db.query_row("SELECT count(*) FROM sqlite_master WHERE name = 'tracks_new'", [], |row| row.get::<_, i64>(0)).unwrap(), 0);
    assert_eq!(everything(&db), rows);
    drop(db);
    upgrade::run(&path, &mut |_| {}, &upgrade::Stop::default()).unwrap();
    let db = Connection::open(&path).unwrap();
    assert_eq!(version(&db), 57);
    assert_eq!(db.query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0)).unwrap(), "ok");
}
