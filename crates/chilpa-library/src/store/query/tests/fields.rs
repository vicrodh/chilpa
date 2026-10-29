use super::*;

#[test]
fn new_fields_parse_print_and_keep_their_shapes() {
    for text in ["plays:0", "p:0,1,10", "plays:1..10", "plays:1..", "plays:..10", "-plays:0",
        "ty:ALBUM", "type:single,ep", "-type:compilation", "type:live", "type:soundtrack",
        "q:CD", "quality:lossy,hires,dsd", "-quality:cd", "(p:10.. | ty:live) -q:lossy"] {
        let tree = parse(text).unwrap();
        let written = tree.to_string();
        assert_eq!(parse(&written).unwrap(), tree, "{text} -> {written}");
    }
    assert_eq!(parse("p:0 ty:EP q:CD").unwrap().to_string(), "plays:0 type:ep quality:cd");
    for text in ["plays:-1", "plays:1.5", "plays:abc", "plays:..", "plays:-1..3", "plays:3..1", "plays:0,-2", "plays:9223372036854775808"] {
        assert!(parse(text).is_err(), "{text}");
    }
    for text in ["type:other", "type:none", "type:album,bogus", "type:album..ep", "type:..ep"] {
        let error = parse(text).unwrap_err();
        for kind in crate::release_kind::NAMES { assert!(error.what.contains(kind), "{error}"); }
    }
    for text in ["quality:other", "quality:none", "quality:cd,bogus", "quality:cd..dsd", "quality:..cd"] {
        let error = parse(text).unwrap_err();
        for tier in ["lossy", "cd", "hires", "dsd"] { assert!(error.what.contains(tier), "{error}"); }
    }
}

#[test]
fn plays_counts_the_same_listens_as_track_rows_and_keeps_zeroes() {
    let lib = library();
    let source = lib.source_id("local", "default").unwrap();
    for (id, count) in [(1, 1), (2, 2), (3, 10), (4, 0)] {
        for n in 0..=count {
            lib.connection().execute("INSERT INTO play_events(track_id, source_id, started_at, played_ms, duration_ms)
                VALUES (?1, ?2, ?3, ?4, 1000)", params![id, source, n, if n == 0 { 100 } else { 1000 }]).unwrap();
        }
    }
    assert_eq!(lib.tracks_by_ids(&[1, 2, 3, 4, 5, 6]).unwrap().iter().map(|t| t.plays).collect::<Vec<_>>(), [1, 2, 10, 0, 0, 0]);
    for (query, ids) in [("plays:0", vec![4, 5, 6]), ("plays:1", vec![1]), ("plays:0,10", vec![3, 4, 5, 6]),
        ("plays:1..2", vec![1, 2]), ("plays:1..", vec![1, 2, 3]), ("plays:10..", vec![3]),
        ("plays:..1", vec![1, 4, 5, 6]), ("-plays:0", vec![1, 2, 3]), ("-plays:1..", vec![4, 5, 6])] {
        assert_eq!(found(&lib, query), ids, "{query}");
    }
    let filter = crate::Filter { query: Some(parse("plays:2").unwrap()), ..Default::default() };
    assert_eq!(lib.browse_ids(&filter, crate::TrackOrder::Title).unwrap().album_ids, [1], "one matching track keeps its record");
}

#[test]
fn plays_uses_an_uncorrelated_indexed_subquery() {
    let lib = library();
    for text in ["plays:0", "plays:1,10", "plays:1..10", "plays:1..", "plays:..10", "-plays:0"] {
        let (sql, args) = parse(text).unwrap().compile(0);
        let mut stmt = lib.connection().prepare(&format!("EXPLAIN QUERY PLAN SELECT t.id FROM tracks t WHERE {sql}")).unwrap();
        let plan = stmt.query_map(rusqlite::params_from_iter(args.iter()), |r| r.get::<_, String>(3)).unwrap()
            .map(Result::unwrap).collect::<Vec<_>>().join("\n");
        assert!(plan.contains("SEARCH track_rollup USING COVERING INDEX track_rollup_plays"), "{text}: {plan}");
        assert!(!plan.contains("CORRELATED") && !plan.contains("play_events"), "{text}: {plan}");
    }
}

#[test]
fn release_types_filter_whole_records_by_title_artist_and_size() {
    let lib = library();
    for (id, title, artist) in [(1, "Record (Live at Home)", "Band"), (2, "Record [EP]", "Band"),
        (3, "Hits", "Various Artists"), (4, "Film (Original Soundtrack)", "Band")] {
        lib.connection().execute("UPDATE albums SET title = ?2, album_artist = ?3 WHERE id = ?1", params![id, title, artist]).unwrap();
    }
    for (query, ids) in [("type:live", vec![1, 2]), ("type:ep,compilation", vec![3, 4, 5]),
        ("type:soundtrack", vec![6]), ("-type:compilation", vec![1, 2, 3, 6]), ("type:album", vec![])] {
        assert_eq!(found(&lib, query), ids, "{query}");
    }
}

#[test]
fn quality_uses_the_best_present_track_and_preserves_unknowns() {
    let lib = library();
    for (id, path, codec, bits, rate) in [(1, "/1.mp3", None, Some(32), Some(96_000)), (2, "/2.flac", None, Some(16), Some(48_000)),
        (3, "/3.flac", None, Some(24), Some(44_100)), (4, "remote:test/instance/4", Some("dsd"), None, None),
        (5, "/5.flac", None, Some(16), Some(44_100)), (6, "/6.unknown", None, None, None)] {
        lib.connection().execute("UPDATE tracks SET external_id = ?2, codec = ?3, bit_depth = ?4, sample_rate_hz = ?5 WHERE id = ?1",
            params![id, path, codec, bits, rate]).unwrap();
    }
    for (query, ids) in [("quality:cd", vec![1, 2]), ("quality:hires", vec![3]), ("quality:dsd", vec![4, 5]),
        ("quality:lossy", vec![]), ("quality:cd,hires", vec![1, 2, 3]), ("-quality:dsd", vec![1, 2, 3, 6])] {
        assert_eq!(found(&lib, query), ids, "{query}");
    }
    lib.connection().execute("UPDATE tracks SET present = 0 WHERE id = 4", []).unwrap();
    assert_eq!(found(&lib, "quality:cd"), [1, 2, 4, 5], "an absent better file does not change the bucket");
    assert!(found(&lib, "quality:dsd").is_empty());
}

#[test]
fn bucket_values_and_play_bounds_are_bound_parameters() {
    // Quality binds each rank once for records and once for tracks without an album.
    for (query, count) in [("plays:0", 1), ("plays:1,10", 2), ("plays:1..10", 2), ("plays:..5", 1),
        ("-plays:10..", 1), ("type:album", 1), ("-type:single,ep", 2), ("quality:cd", 2), ("-quality:hires,dsd", 4)] {
        let (sql, args) = parse(query).unwrap().compile(0);
        assert_eq!(args.len(), count, "{query}: {sql}");
        assert_eq!(sql.matches('?').count(), count, "{query}: {sql}");
    }
    let hostile = "album'); DROP TABLE tracks; --";
    let (sql, args) = Node::Is(Field::Type, Values::Any(vec![hostile.into()])).compile(0);
    assert!(!sql.contains(hostile));
    assert_eq!(args.len(), 1);
    let (sql, args) = Node::Is(Field::Quality, Values::Any(vec!["cd".into(), hostile.into()])).compile(0);
    assert!(!sql.contains(hostile));
    assert_eq!(args.len(), 2, "only the known quality rank is bound in both branches");
    assert_eq!(sql.matches('?').count(), 2);
}

#[test]
fn bucket_functions_are_available_on_every_library_connection() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.db");
    let lib = Library::open(&path).unwrap();
    for lib in [lib, Library::open_existing(&path).unwrap(), Library::open_read_only(&path).unwrap()] {
        for text in ["plays:0", "type:single", "quality:cd"] { assert!(found(&lib, text).is_empty()); }
    }
}
