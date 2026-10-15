use super::*;
use std::cell::Cell;

fn reader() -> Reader {
    let lib = chilpa_library::Library::in_memory().unwrap();
    lib.connection().execute_batch("INSERT INTO sources(id, provider, instance) VALUES (1, 'local', 'default');
        INSERT INTO albums(id, title, album_artist, year) VALUES (1, 'First', 'Artist', 2000), (2, 'Second', 'Other', 2001);
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

#[test]
fn different_filters_and_orders_reuse_only_committed_source_and_year_facets() {
    let reader = reader();
    let all = Filter::default();
    reader.refresh(&all, TrackOrder::Album, || true).unwrap();
    // I mark these entries so I can distinguish reuse from another SQL read.
    {
        let mut cache = reader.refresh_cache.borrow_mut();
        let facets = &mut cache.entries.front_mut().unwrap().2;
        facets.sources[0].label = "Cached source".into();
        facets.years[0].label = "Cached year".into();
    }
    let flac = Filter { formats: vec!["flac".into()], ..Default::default() };
    let narrowed = reader.refresh(&flac, TrackOrder::Title, || true).unwrap();
    assert_eq!(narrowed.1.ids, [2]);
    assert_eq!(narrowed.0.sources[0].label, "Cached source");
    assert_eq!(narrowed.0.years[0].label, "Cached year");
    assert_eq!(narrowed.0.sources[0].albums, 2, "source counts remain independent of the selected format");
    assert_ne!(reader.facets(&flac).sources[0].label, "Cached source", "the public facet read never takes refresh cache entries");
    assert_ne!(reader.facets(&flac).years[0].label, "Cached year");
}

#[test]
fn own_metadata_writes_invalidate_shared_source_and_year_facets() {
    let reader = reader();
    let all = Filter::default();
    reader.refresh(&all, TrackOrder::Album, || true).unwrap();
    let flac = Filter { formats: vec!["flac".into()], ..Default::default() };
    reader.lib.connection().execute_batch("UPDATE sources SET label = 'Updated source';
        UPDATE albums SET year = 2002 WHERE id = 2;").unwrap();
    let changed = reader.refresh(&flac, TrackOrder::Album, || true).unwrap();
    assert_eq!(changed.0.sources[0].label, "Updated source");
    assert_eq!(changed.0.years.iter().map(|year| year.key.as_str()).collect::<Vec<_>>(), ["2002", "2000"]);
    reader.lib.connection().execute("UPDATE sources SET enabled = 0", []).unwrap();
    let disabled = reader.refresh(&flac, TrackOrder::Title, || true).unwrap();
    assert!(disabled.0.sources.is_empty());
    assert!(disabled.0.years.is_empty());
    assert!(disabled.1.ids.is_empty());
}

#[test]
fn a_transaction_and_its_rollback_never_share_uncommitted_invariant_facets() {
    let reader = reader();
    let all = Filter::default();
    let committed = reader.refresh(&all, TrackOrder::Album, || true).unwrap().0;
    reader.lib.connection().execute_batch("BEGIN; UPDATE sources SET label = 'Uncommitted source';
        UPDATE albums SET year = 2099 WHERE id = 2;").unwrap();
    let flac = Filter { formats: vec!["flac".into()], ..Default::default() };
    let inside = reader.refresh(&flac, TrackOrder::Title, || true).unwrap().0;
    assert_eq!(inside.sources[0].label, "Uncommitted source");
    assert_eq!(inside.years[0].key, "2099");
    assert!(reader.refresh_cache.borrow().entries.is_empty());
    reader.lib.connection().execute_batch("ROLLBACK;").unwrap();
    let restored = reader.refresh(&flac, TrackOrder::Album, || true).unwrap().0;
    assert_eq!(restored.sources, committed.sources);
    assert_eq!(restored.years, committed.years);
}

#[test]
fn an_external_commit_refreshes_invariant_facets_but_an_external_rollback_does_not() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("library.db");
    let writer = chilpa_library::Library::open(&path).unwrap();
    let source = writer.source_id("local", "default").unwrap();
    let album = writer.source_edition(source, "fixture", "Record", "Artist", Some(2000)).unwrap();
    writer.connection().execute("INSERT INTO tracks(source_id, external_id, title, album_id, added_at)
        VALUES (?1, '/one.flac', 'One', ?2, 1)", rusqlite::params![source, album]).unwrap();
    let reader = Reader::open(&path).unwrap();
    let all = Filter::default();
    let committed = reader.refresh(&all, TrackOrder::Album, || true).unwrap().0;
    writer.connection().execute_batch("BEGIN; UPDATE sources SET label = 'Hidden source'; UPDATE albums SET year = 2099;").unwrap();
    let flac = Filter { formats: vec!["flac".into()], ..Default::default() };
    let while_writing = reader.refresh(&flac, TrackOrder::Title, || true).unwrap().0;
    assert_eq!(while_writing.sources, committed.sources);
    assert_eq!(while_writing.years, committed.years);
    writer.connection().execute_batch("ROLLBACK;").unwrap();
    let checks = Cell::new(0);
    reader.refresh(&flac, TrackOrder::Title, || { checks.set(checks.get() + 1); true }).unwrap();
    assert_eq!(checks.get(), 1, "another connection's rollback keeps the committed revision reusable");
    writer.connection().execute_batch("UPDATE sources SET label = 'Committed source'; UPDATE albums SET year = 2005;").unwrap();
    let changed = reader.refresh(&flac, TrackOrder::Album, || true).unwrap().0;
    assert_eq!(changed.sources[0].label, "Committed source");
    assert_eq!(changed.years[0].key, "2005");
    writer.connection().execute("UPDATE sources SET enabled = 0", []).unwrap();
    let disabled = reader.refresh(&flac, TrackOrder::Title, || true).unwrap().0;
    assert!(disabled.sources.is_empty());
    assert!(disabled.years.is_empty());
    writer.connection().execute("UPDATE sources SET enabled = 1", []).unwrap();
    reader.refresh(&all, TrackOrder::Artist, || true).unwrap();
    writer.connection().execute("UPDATE tracks SET present = 0", []).unwrap();
    let absent = reader.refresh(&flac, TrackOrder::Artist, || true).unwrap().0;
    assert!(absent.sources.is_empty());
    assert!(absent.years.is_empty());
}
