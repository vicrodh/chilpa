use super::*;
use std::cell::Cell;

fn reader() -> Reader {
    let lib = chilpa_library::Library::in_memory().unwrap();
    lib.connection().execute_batch("INSERT INTO sources(id, provider, instance) VALUES (1, 'local', 'default');
        INSERT INTO albums(id, title, album_artist) VALUES (1, 'First', 'Artist'), (2, 'Second', 'Other');
        INSERT INTO tracks(id, source_id, external_id, title, album_id, added_at, bit_depth, sample_rate_hz)
        VALUES (1, 1, '/first.dsf', 'First', 1, 1, 1, 2822400), (2, 1, '/second.flac', 'Second', 2, 1, 24, 96000);").unwrap();
    Reader { lib, catalog: None, refresh_cache: Default::default() }
}

#[test]
fn repeated_filters_reuse_results_and_a_real_write_invalidates_them() {
    let reader = reader();
    let filter = Filter::default();
    let first = reader.refresh(&filter, TrackOrder::Album, || true).unwrap();
    let dsd = Filter { qualities: vec!["dsd".into()], ..Default::default() };
    let narrowed = reader.refresh(&dsd, TrackOrder::Album, || true).unwrap();
    assert_eq!(narrowed.1.ids, [1]);
    let checks = Cell::new(0);
    let cached = reader.refresh(&filter, TrackOrder::Album, || { checks.set(checks.get() + 1); true }).unwrap();
    assert_eq!(checks.get(), 1, "a cached refresh does no eligibility or facet work");
    assert_eq!(first, cached);
    reader.refresh_cache.borrow_mut().entries.front_mut().unwrap().2.ratings[0].label = "Previous language".into();
    let relabeled = reader.refresh(&filter, TrackOrder::Album, || true).unwrap();
    assert_eq!(relabeled.0.ratings[0].label, named_stars(relabeled.0.ratings[0].clone()).label,
        "cached star labels follow the current language");
    reader.lib.connection().execute("UPDATE tracks SET present = 0 WHERE id = 2", []).unwrap();
    assert_eq!(reader.refresh(&filter, TrackOrder::Album, || true).unwrap().1.ids, [1]);
}

#[test]
fn superseded_refreshes_stop_before_work_and_between_facet_reads() {
    let reader = reader();
    let before = reader.lib.connection().total_changes();
    assert!(reader.refresh(&Filter::default(), TrackOrder::Album, || false).is_none());
    assert_eq!(reader.lib.connection().total_changes(), before);
    let checks = Cell::new(0);
    assert!(reader.refresh(&Filter::default(), TrackOrder::Album, || {
        checks.set(checks.get() + 1);
        checks.get() < 4
    }).is_none());
    assert_eq!(checks.get(), 4, "I stop at the first canceled checkpoint");
    assert!(reader.refresh_cache.borrow().entries.is_empty(), "a canceled partial result is never cached");
    assert_eq!(reader.refresh(&Filter::default(), TrackOrder::Album, || true).unwrap().1.ids, [1, 2]);
}

#[test]
fn transaction_local_results_never_replace_the_committed_cache() {
    let reader = reader();
    let filter = Filter::default();
    assert_eq!(reader.refresh(&filter, TrackOrder::Album, || true).unwrap().1.ids, [1, 2]);
    reader.lib.connection().execute_batch("BEGIN; UPDATE tracks SET present = 0 WHERE id = 2;").unwrap();
    assert_eq!(reader.refresh(&filter, TrackOrder::Album, || true).unwrap().1.ids, [1]);
    reader.lib.connection().execute_batch("ROLLBACK;").unwrap();
    assert_eq!(reader.refresh(&filter, TrackOrder::Album, || true).unwrap().1.ids, [1, 2]);
}

#[test]
fn an_existing_reader_invalidates_cached_filters_after_an_external_commit() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("library.db");
    let writer = chilpa_library::Library::open(&path).unwrap();
    let source = writer.source_id("local", "default").unwrap();
    let album = writer.source_edition(source, "fixture", "Record", "Artist", None).unwrap();
    writer.connection().execute("INSERT INTO tracks(source_id, external_id, title, album_id, added_at)
        VALUES (?1, '/one.flac', 'One', ?2, 1)", rusqlite::params![source, album]).unwrap();
    let reader = Reader::open(&path).unwrap();
    let filter = Filter::default();
    assert_eq!(reader.refresh(&filter, TrackOrder::Album, || true).unwrap().1.ids.len(), 1);
    writer.connection().execute("UPDATE tracks SET present = 0", []).unwrap();
    assert!(reader.refresh(&filter, TrackOrder::Album, || true).unwrap().1.ids.is_empty());
}
