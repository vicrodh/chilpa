use chilpa_library::Library;
use std::sync::Arc;

fn seed(lib: &Library) {
    lib.connection().execute_batch("INSERT INTO sources(id, provider, instance) VALUES (1, 'local', 'default');
        INSERT INTO albums(id, title, album_artist, folder) VALUES (1, 'Record', 'Artist', '/record');
        INSERT INTO tracks(id, source_id, external_id, title, album_id, added_at, duration_ms, bit_depth, sample_rate_hz)
        VALUES (1, 1, '/record/one.flac', 'One', 1, 1, 1000, 16, 44100);").unwrap();
}

fn irrelevant_writes(lib: &Library) {
    lib.connection().execute_batch("INSERT INTO source_progress(source, external_id, title, position_ms, updated_at)
        VALUES ('fixture', 'episode', 'Episode', 100, 1)
        ON CONFLICT(source, external_id) DO UPDATE SET position_ms = position_ms + 1;
        INSERT INTO album_artwork(album_id, path) VALUES (1, '/cover.png')
        ON CONFLICT(album_id) DO UPDATE SET path = '/another.png';
        UPDATE tracks SET title = 'Renamed track' WHERE id = 1;
        UPDATE albums SET artwork_path = '/cover.png' WHERE id = 1;
        INSERT INTO audio_features(track_id, analyzer, version, bpm, analyzed_at)
        VALUES (1, 'fixture', 1, 120, 1)
        ON CONFLICT(track_id, analyzer) DO UPDATE SET bpm = 121;").unwrap();
    lib.rate(chilpa_library::store::ratings::RatingTarget::Album(1), 4).unwrap();
    lib.log_play(Some(1), None, 1, 1000, 1000, false, None, None, true).unwrap();
}

#[test]
fn unrelated_writes_reuse_the_catalog_on_the_writer_and_an_existing_reader() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("library.db");
    let writer = Library::open(&path).unwrap();
    seed(&writer);
    let reader = Library::open_read_only(&path).unwrap();
    let own = writer.version_catalog().unwrap();
    let remote = reader.version_catalog().unwrap();
    irrelevant_writes(&writer);
    assert!(Arc::ptr_eq(&own, &writer.version_catalog().unwrap()));
    assert!(Arc::ptr_eq(&remote, &reader.version_catalog().unwrap()));
    writer.connection().execute("UPDATE tracks SET reachable = 0 WHERE id = 1", []).unwrap();
    let own_changed = writer.version_catalog().unwrap();
    let remote_changed = reader.version_catalog().unwrap();
    assert!(!Arc::ptr_eq(&own, &own_changed));
    assert!(!Arc::ptr_eq(&remote, &remote_changed));
    assert!(!remote_changed.versions[&1].reachable);
}

#[test]
fn every_catalog_input_invalidates_without_rebuilding_for_a_title_noop() {
    let lib = Library::in_memory().unwrap();
    seed(&lib);
    let mut previous = lib.version_catalog().unwrap();
    lib.connection().execute("UPDATE albums SET title = title WHERE id = 1", []).unwrap();
    assert!(Arc::ptr_eq(&previous, &lib.version_catalog().unwrap()));
    for sql in ["UPDATE albums SET title = 'Changed' WHERE id = 1",
        "UPDATE albums SET album_artist = 'Changed artist' WHERE id = 1",
        "UPDATE albums SET folder = '/other' WHERE id = 1",
        "UPDATE tracks SET bit_depth = 24, sample_rate_hz = 96000 WHERE id = 1",
        "UPDATE tracks SET duration_ms = 2000 WHERE id = 1",
        "UPDATE tracks SET codec = 'flac', external_id = '/other/one.flac' WHERE id = 1",
        "UPDATE sources SET label = 'Renamed', icon = 'server' WHERE id = 1",
        "UPDATE sources SET enabled = 0 WHERE id = 1",
        "UPDATE sources SET enabled = 1 WHERE id = 1",
        "UPDATE tracks SET present = 0 WHERE id = 1",
        "UPDATE tracks SET present = 1 WHERE id = 1"] {
        lib.connection().execute(sql, []).unwrap();
        let next = lib.version_catalog().unwrap();
        assert!(!Arc::ptr_eq(&previous, &next), "{sql}");
        previous = next;
    }
    assert!(lib.select_album_version(1, 1).unwrap());
    assert!(!Arc::ptr_eq(&previous, &lib.version_catalog().unwrap()));
}

#[test]
fn a_rolled_back_revision_cannot_alias_a_different_committed_catalog() {
    let lib = Library::in_memory().unwrap();
    seed(&lib);
    let committed = lib.version_catalog().unwrap();
    lib.connection().execute_batch("BEGIN; UPDATE albums SET title = 'Rolled back' WHERE id = 1;").unwrap();
    let uncommitted = lib.version_catalog().unwrap();
    assert_eq!(uncommitted.versions[&1].title, "Rolled back");
    lib.connection().execute_batch("ROLLBACK; UPDATE albums SET title = 'Committed' WHERE id = 1;").unwrap();
    let next = lib.version_catalog().unwrap();
    assert_eq!(next.versions[&1].title, "Committed");
    assert!(!Arc::ptr_eq(&committed, &next));
    assert!(!Arc::ptr_eq(&uncommitted, &next));
}
