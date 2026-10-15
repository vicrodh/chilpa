use chilpa_library::{Filter, Library, TrackOrder};

fn seed(lib: &Library) {
    lib.connection().execute_batch("INSERT INTO sources(id, provider, instance) VALUES
        (1, 'local', 'default'), (2, 'server', 'fixture');
        INSERT INTO albums(id, title, album_artist, year) VALUES
        (1, 'First', 'One', 2000), (2, 'Second', 'Two', 2001), (3, 'Third', 'Three', 2002);
        INSERT INTO tracks(id, source_id, external_id, title, artist, album_id, added_at, duration_ms,
            bit_depth, sample_rate_hz, label) VALUES
        (1, 1, '/unicode/歌/one.DSF', 'One', 'One', 1, 1, 100, 1, 2822400, 'Label A'),
        (2, 1, '/two.flac', 'Two', 'Two', 2, 1, 200, 24, 96000, 'Label B'),
        (3, 2, '/three.dff', 'Three', 'Three', 3, 1, 300, 1, 2822400, 'Label C'),
        (4, 1, '/four.mp3', 'Four', 'One', 1, 1, 400, NULL, 44100, 'Label A');
        INSERT INTO genres(id, name) VALUES (1, 'Jazz'), (2, 'Rock');
        INSERT INTO track_genres(track_id, genre_id) VALUES (1, 1), (2, 2), (3, 2), (4, 1);
        INSERT INTO composers(id, name) VALUES (1, 'Writer A'), (2, 'Writer B');
        INSERT INTO track_composers(track_id, composer_id) VALUES (1, 1), (2, 2), (3, 2);").unwrap();
}

#[test]
fn eligibility_preserves_every_cascade_and_resets_after_the_read() {
    let lib = Library::in_memory().unwrap();
    seed(&lib);
    for mut filter in [Filter::default(),
        Filter { qualities: vec!["dsd".into()], ..Default::default() },
        Filter { excluded_sources: vec!["server:fixture".into()], formats: vec!["dsd".into()], ..Default::default() },
        Filter { qualities: vec!["hires".into(), "lossy".into()], ..Default::default() }] {
        filter.artists = vec!["One".into()];
        filter.queries[6] = "First".into();
        let expected = (lib.facet_genres(&filter).unwrap(), lib.facet_labels(&filter).unwrap(),
            lib.facet_composers(&filter).unwrap(), lib.facet_ratings(&filter).unwrap(),
            lib.facet_artists(&filter).unwrap(), lib.facet_albums(&filter).unwrap(),
            lib.grouped_snapshot(&filter, TrackOrder::Album).unwrap());
        let revision = lib.read_revision().unwrap();
        lib.with_browse_candidates(&filter, || {
            assert_eq!(expected.0, lib.facet_genres(&filter).unwrap());
            assert_eq!(expected.1, lib.facet_labels(&filter).unwrap());
            assert_eq!(expected.2, lib.facet_composers(&filter).unwrap());
            assert_eq!(expected.3, lib.facet_ratings(&filter).unwrap());
            assert_eq!(expected.4, lib.facet_artists(&filter).unwrap());
            assert_eq!(expected.5, lib.facet_albums(&filter).unwrap());
            assert_eq!(expected.6, lib.grouped_snapshot(&filter, TrackOrder::Album).unwrap());
        }).unwrap();
        assert_eq!(lib.read_revision().unwrap(), revision, "eligibility is not a library change");
        assert_eq!(lib.browse_ids(&Filter::default(), TrackOrder::Album).unwrap().ids.len(), 4);
    }
}

#[test]
fn a_read_only_main_database_can_build_temporary_eligibility() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("library.db");
    let writer = Library::open(&path).unwrap();
    seed(&writer);
    let reader = Library::open_read_only(&path).unwrap();
    let filter = Filter { qualities: vec!["dsd".into()], ..Default::default() };
    let found = reader.with_browse_candidates(&filter,
        || reader.browse_ids(&filter, TrackOrder::Album).unwrap()).unwrap();
    assert_eq!(found.ids, [1, 3]);
    assert_eq!(reader.facet_genres(&filter).unwrap().len(), 2, "the final album search cannot narrow this column");
}

#[test]
fn suffix_matching_keeps_the_previous_format_and_quality_predicates() {
    let lib = Library::in_memory().unwrap();
    seed(&lib);
    for (format, expected) in [("dsd", vec![1, 3]), ("flac", vec![2]), ("mp3", vec![4]), ("unknown", vec![1, 2, 3, 4])] {
        let filter = Filter { formats: vec![format.into()], ..Default::default() };
        let mut ids = lib.browse_ids(&filter, TrackOrder::Album).unwrap().ids;
        ids.sort();
        assert_eq!(ids, expected, "{format}");
    }
    let filter = Filter { qualities: vec!["dsd".into()], ..Default::default() };
    assert_eq!(lib.browse_ids(&filter, TrackOrder::Album).unwrap().ids, [1, 3]);
}
