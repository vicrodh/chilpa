use super::*;
use query::*;

fn fixture() -> Library {
    let lib = Library::in_memory().unwrap();
    lib.db
        .execute_batch(
            "INSERT INTO sources(id, provider, instance) VALUES
        (1, 'local', 'default'), (2, 'server', 'home');
        INSERT INTO albums(id, title, album_artist) VALUES (1, 'Compilation', 'Various Artists');
        INSERT INTO tracks(id, source_id, external_id, title, artist, album_id, added_at) VALUES
        (1, 1, 'one', 'One', 'Singer A', 1, 0), (2, 2, 'one', 'Two', 'Singer B', 1, 0);
        INSERT INTO genres(id, name) VALUES (1, 'Rock'), (2, 'Blues');
        INSERT INTO track_genres VALUES (1, 1), (1, 2), (2, 1);",
        )
        .unwrap();
    lib
}

fn event(
    lib: &Library,
    track: i64,
    started_at: i64,
    offset: Option<i32>,
    heard: u64,
) -> ListenEvent {
    let mut context = lib.listen_context(track).unwrap();
    context.utc_offset_seconds = offset;
    ListenEvent {
        context,
        started_at,
        played_ms: heard,
        duration_ms: 1000,
        skipped: heard < 500,
        sample_rate_hz: Some(44100),
        bit_depth: Some(16),
        bit_perfect: true,
    }
}

#[test]
fn genres_periods_and_sources_do_not_multiply_the_summary() {
    let lib = fixture();
    lib.record_listen(&event(&lib, 1, 3600, Some(-7200), 1000))
        .unwrap();
    lib.record_listen(&event(&lib, 2, 7200, Some(0), 100))
        .unwrap();
    let filter = ListenFilter::default();
    let summary = lib.listening_summary(&filter).unwrap();
    assert_eq!(
        (
            summary.events,
            summary.plays,
            summary.listened_ms,
            summary.skips
        ),
        (2, 1, 1100, 1)
    );
    assert_eq!(
        summary.distinct_tracks, 2,
        "same external id in different sources"
    );
    let genres = lib
        .listening_buckets(
            &filter,
            ListenDimension::Genre,
            ListenOrder::ListenedTime,
            10,
        )
        .unwrap();
    assert_eq!(genres.len(), 2);
    assert_eq!(genres[0].key.as_deref(), Some("Rock"));
    assert_eq!(genres[0].metrics.listened_ms, 1100);
    assert_eq!(genres[1].metrics.listened_ms, 1000);
    let period = ListenFilter {
        from: Some(3600),
        until: Some(7200),
        ..Default::default()
    };
    assert_eq!(lib.listening_summary(&period).unwrap().events, 1);
    let source = ListenFilter {
        source_id: Some(2),
        ..Default::default()
    };
    assert_eq!(lib.listening_summary(&source).unwrap().plays, 0);
    let genre = ListenFilter {
        genre: Some("blues".into()),
        ..Default::default()
    };
    assert_eq!(lib.listening_summary(&genre).unwrap().events, 1);
    let artists = lib
        .listening_buckets(&filter, ListenDimension::Artist, ListenOrder::Plays, 10)
        .unwrap();
    assert_eq!(
        artists[0].label, "Singer A",
        "I count the performer, not Various Artists"
    );
}

#[test]
fn time_buckets_use_captured_offsets_and_leave_missing_offsets_unknown() {
    let lib = fixture();
    lib.record_listen(&event(&lib, 1, 3600, Some(-7200), 1000))
        .unwrap();
    lib.record_listen(&event(&lib, 1, 7200, None, 1000))
        .unwrap();
    let buckets = |filter: &ListenFilter, dim| {
        lib.listening_buckets(filter, dim, ListenOrder::Key, 100)
            .unwrap()
    };
    let local = ListenFilter::default();
    let hours = buckets(&local, ListenDimension::Hour);
    assert_eq!(hours[0].key, None);
    assert_eq!(hours[1].key.as_deref(), Some("23"));
    let days = buckets(&local, ListenDimension::Day);
    assert_eq!(days[1].key.as_deref(), Some("1969-12-31"));
    let weekdays = buckets(&local, ListenDimension::Weekday);
    assert_eq!(weekdays[1].key.as_deref(), Some("3"));
    let utc = ListenFilter {
        time: TimeBasis::Utc,
        ..Default::default()
    };
    assert_eq!(
        buckets(&utc, ListenDimension::Hour)
            .iter()
            .map(|b| b.key.as_deref())
            .collect::<Vec<_>>(),
        [Some("01"), Some("02")]
    );
    let cross = buckets(&local, ListenDimension::GenreHour);
    assert_eq!(cross.len(), 4);
    assert!(cross
        .iter()
        .any(|b| b.key.as_deref() == Some("Rock") && b.hour == Some(23)));
    assert_eq!(lib.listening_summary(&local).unwrap().unknown_local_time, 1);
}

#[test]
fn snapshot_survives_metadata_edits_and_missing_tracks() {
    let lib = fixture();
    let captured = event(&lib, 1, 1, Some(0), 1000);
    lib.db
        .execute_batch(
            "UPDATE tracks SET artist = 'Changed', external_id = 'moved', present = 0 WHERE id = 1;
        DELETE FROM track_genres WHERE track_id = 1;",
        )
        .unwrap();
    lib.record_listen(&captured).unwrap();
    let f = ListenFilter::default();
    assert_eq!(
        lib.listening_buckets(&f, ListenDimension::Genre, ListenOrder::Key, 10)
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        lib.listening_buckets(&f, ListenDimension::Artist, ListenOrder::Key, 10)
            .unwrap()[0]
            .label,
        "Singer A"
    );
    assert_eq!(
        lib.stats().unwrap().listened_ms,
        lib.listening_summary(&f).unwrap().listened_ms
    );
    lib.record_listen(&event(&lib, 1, 2, Some(0), 1000)).unwrap();
    assert_eq!(lib.listening_summary(&f).unwrap().distinct_tracks, 1);
}

#[test]
fn invalid_evidence_is_atomic_and_unknown_streams_are_not_local() {
    let lib = fixture();
    let mut bad = event(&lib, 1, 1, Some(0), 1000);
    bad.context.source_id = Some(2);
    assert!(lib.record_listen(&bad).is_err());
    bad.context.source_id = Some(1);
    bad.played_ms = u64::MAX;
    assert!(lib.record_listen(&bad).is_err());
    assert_eq!(lib.stats().unwrap().plays, 0);
    let unknown = ListenEvent {
        context: ListenContext::default(),
        played_ms: 1000,
        ..bad
    };
    lib.record_listen(&unknown).unwrap();
    let f = ListenFilter::default();
    let sources = lib
        .listening_buckets(&f, ListenDimension::Source, ListenOrder::Key, 10)
        .unwrap();
    assert_eq!(sources[0].key, None);
    assert_eq!(sources[0].metrics.distinct_tracks, 0);
    let bad = ListenFilter {
        from: Some(2),
        until: Some(1),
        ..Default::default()
    };
    assert!(lib.listening_summary(&bad).is_err());
    assert!(lib.check().unwrap().is_empty());
}

#[test]
fn migration_preserves_totals_and_marks_reconstructed_metadata() {
    let db = rusqlite::Connection::open_in_memory().unwrap();
    crate::schema::apply_pragmas(&db).unwrap();
    crate::schema::stop_at(&db, 25);
    db.execute_batch("INSERT INTO sources(id, provider, instance) VALUES (1, 'local', 'default');
        INSERT INTO tracks(id, source_id, external_id, title, added_at) VALUES (1, 1, 'one', 'One', 0);
        INSERT INTO genres VALUES (1, 'Rock'); INSERT INTO track_genres VALUES (1, 1);
        INSERT INTO play_events(track_id, source_id, started_at, played_ms, duration_ms) VALUES (1, 1, 100, 1000, 1000);").unwrap();
    crate::schema::migrate(&db).unwrap();
    crate::schema::migrate(&db).unwrap();
    let lib = Library {
        db,
        versions: Default::default(),
        related: Default::default(),
    };
    let f = ListenFilter::default();
    let summary = lib.listening_summary(&f).unwrap();
    assert_eq!(
        (
            summary.plays,
            summary.listened_ms,
            summary.backfilled,
            summary.unknown_local_time
        ),
        (1, 1000, 1, 1)
    );
    assert_eq!(
        lib.listening_buckets(&f, ListenDimension::Genre, ListenOrder::Key, 10)
            .unwrap()[0]
            .label,
        "Rock"
    );
    assert_eq!(lib.stats().unwrap().listened_ms, summary.listened_ms);
}

#[test]
fn empty_queries_and_every_dimension_have_defined_results() {
    let lib = fixture();
    let f = ListenFilter::default();
    assert_eq!(lib.listening_summary(&f).unwrap(), ListenMetrics::default());
    for dimension in [
        ListenDimension::Track,
        ListenDimension::Album,
        ListenDimension::Artist,
        ListenDimension::Source,
        ListenDimension::Genre,
        ListenDimension::Hour,
        ListenDimension::Weekday,
        ListenDimension::Day,
        ListenDimension::GenreHour,
    ] {
        assert!(lib
            .listening_buckets(&f, dimension, ListenOrder::Plays, 10)
            .unwrap()
            .is_empty());
    }
}

#[test]
fn source_adapters_can_log_stable_keys_before_tracks_are_cached() {
    let lib = fixture();
    for source in [1, 2] {
        let mut e = event(&lib, 1, 1000, Some(1800), 1000);
        e.context = ListenContext {
            source_id: Some(source),
            external_id: "opaque-track".into(),
            album_external_id: "opaque-album".into(),
            title: "Remote song".into(),
            album: "Remote album".into(),
            genres: vec!["Rock; Blues".into(), "rock".into()],
            ..Default::default()
        };
        lib.record_listen(&e).unwrap();
    }
    let f = ListenFilter::default();
    assert_eq!(lib.listening_summary(&f).unwrap().distinct_tracks, 2);
    assert_eq!(
        lib.listening_buckets(&f, ListenDimension::Album, ListenOrder::Key, 10)
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        lib.listening_buckets(&f, ListenDimension::Genre, ListenOrder::Key, 10)
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn a_listen_waits_for_another_writer_instead_of_failing() {
    let dir = std::env::temp_dir().join(format!("chilpa-listen-wait-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("library.db");
    let lib = Library::open(&path).unwrap();
    lib.db
        .execute_batch(
            "INSERT INTO sources(id, provider, instance) VALUES (7, 'server', 'home');
        INSERT INTO tracks(id, source_id, external_id, title, added_at) VALUES (1, 7, 'one', 'One', 0);",
        )
        .unwrap();
    let listen = event(&lib, 1, 100, None, 1000);
    // Another connection is in the middle of a write, as a scan or the
    // watch over the servers is every so often.
    let other = rusqlite::Connection::open(&path).unwrap();
    other.execute_batch("BEGIN IMMEDIATE; UPDATE tracks SET title = 'Other' WHERE id = 1;").unwrap();
    let writer = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(300));
        other.execute_batch("COMMIT").unwrap();
    });
    let saved = lib.record_listen(&listen);
    writer.join().unwrap();
    assert!(saved.is_ok(), "the listen was refused instead of waiting: {saved:?}");
    assert_eq!(lib.stats().unwrap().plays, 1);
    drop(lib);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_history_goes_newest_first_a_page_at_a_time_and_says_its_total() {
    let lib = fixture();
    for (at, track) in [(3600, 1), (7200, 2), (10800, 1)] {
        lib.record_listen(&event(&lib, track, at, Some(0), 1000)).unwrap();
    }
    let filter = ListenFilter::default();
    let (first, total) = lib.listening_history(&filter, "", 0, 2).unwrap();
    assert_eq!(total, 3);
    assert_eq!(first.iter().map(|e| e.started_at).collect::<Vec<_>>(), [10800, 7200]);
    assert_eq!(first[0].title, "One");
    assert_eq!(first[0].local_time, "1970-01-01 03:00", "where it was heard");
    let (rest, _) = lib.listening_history(&filter, "", 2, 2).unwrap();
    assert_eq!(rest.iter().map(|e| e.started_at).collect::<Vec<_>>(), [3600]);
    let late = ListenFilter { from: Some(7200), ..ListenFilter::default() };
    assert_eq!(lib.listening_history(&late, "", 0, 10).unwrap().1, 2);
}

#[test]
fn a_period_counts_its_artists_and_records_and_its_decades() {
    let lib = fixture();
    lib.db.execute("UPDATE albums SET year = 1977 WHERE id = 1", []).unwrap();
    lib.record_listen(&event(&lib, 1, 3600, Some(0), 1000)).unwrap();
    lib.record_listen(&event(&lib, 2, 7200, Some(0), 1000)).unwrap();
    assert_eq!(lib.listening_counts(&ListenFilter::default()).unwrap(), (2, 1));
    let decades = lib.listening_buckets(&ListenFilter::default(), ListenDimension::Decade, ListenOrder::Plays, 10).unwrap();
    assert_eq!(decades.len(), 1);
    assert_eq!(decades[0].key.as_deref(), Some("1970"));
    assert_eq!(decades[0].metrics.plays, 2);
}

#[test]
fn local_days_end_today_and_start_at_a_local_midnight() {
    let lib = fixture();
    let days = lib.local_days(6, 7).unwrap();
    assert_eq!(days.len(), 7);
    assert!(days.windows(2).all(|w| w[0] < w[1]), "oldest first");
    let today: String = lib.db.query_row("SELECT date('now', 'localtime')", [], |r| r.get(0)).unwrap();
    assert_eq!(days[6], today);
    let start = lib.local_day_start(0).unwrap();
    let now: i64 = lib.db.query_row("SELECT unixepoch('now')", [], |r| r.get(0)).unwrap();
    assert!(start <= now && now - start < 86_400 + 3_600);
}

#[test]
fn the_history_finds_every_word_in_a_track_its_artist_or_its_record() {
    let lib = fixture();
    for (at, track) in [(3600, 1), (7200, 2), (10800, 1)] {
        lib.record_listen(&event(&lib, track, at, Some(0), 1000)).unwrap();
    }
    let filter = ListenFilter::default();
    let (found, total) = lib.listening_history(&filter, "singer b", 0, 10).unwrap();
    assert_eq!((total, found.len()), (1, 1));
    assert_eq!(found[0].title, "Two");
    assert_eq!(lib.listening_history(&filter, "one compilation", 0, 10).unwrap().1, 2, "words across the track and its record");
    assert_eq!(lib.listening_history(&filter, "nothing", 0, 10).unwrap().1, 0);
    let late = ListenFilter { from: Some(7200), ..ListenFilter::default() };
    assert_eq!(lib.listening_history(&late, "one", 0, 10).unwrap().1, 1, "the search keeps the period's bounds");
}

#[test]
fn a_local_date_starts_at_its_own_midnight() {
    let lib = fixture();
    let today: String = lib.db.query_row("SELECT date('now', 'localtime')", [], |r| r.get(0)).unwrap();
    assert_eq!(lib.local_date_start(&today).unwrap(), lib.local_day_start(0).unwrap());
}
