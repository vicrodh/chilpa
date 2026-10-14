use chilpa_library::{AlbumSort, Filter, Library, TrackOrder};
use chilpa_library::store::versions::album_group;
use rusqlite::params;

fn album(lib: &Library, id: i64, title: &str, artist: &str) {
    lib.connection().execute("INSERT INTO albums(id, title, album_artist, folder) VALUES (?1, ?2, ?3, ?4)", params![id, title, artist, id.to_string()]).unwrap();
}

fn track(lib: &Library, id: i64, album: i64, source: i64, path: &str, bits: i64, rate: i64) {
    lib.connection().execute("INSERT INTO tracks(id, source_id, external_id, title, artist, album_id, duration_ms, added_at, bit_depth, sample_rate_hz, track_number)
        VALUES (?1, ?2, ?3, 'Track', 'Artist', ?4, 1000, 1, ?5, ?6, 1)", params![id, source, path, album, bits, rate]).unwrap();
}

#[test]
fn edition_annotations_group_but_other_records_and_artists_do_not() {
    for title in ["1971 Led Zeppelin IV", "Led Zeppelin IV [1994 Remaster]", "IV (Deluxe Edition)", "IV - Remastered"] {
        assert_eq!(album_group(title, "Led Zeppelin"), album_group("IV", "Led Zeppelin"), "{title}");
    }
    assert_ne!(album_group("IV", "Led Zeppelin"), album_group("III", "Led Zeppelin"));
    assert_ne!(album_group("Greatest Hits", "Queen"), album_group("Greatest Hits", "ABBA"));
    assert_ne!(album_group("Live (London 1971)", "Band"), album_group("Live (London 1972)", "Band"));
    assert_ne!(album_group("1984", "Band"), album_group("1985", "Band"));
}

#[test]
fn physical_formats_group_without_merging_their_tracks() {
    for (title, annotated, extension) in [("Powerslave", "Powerslave (DSF)", "dsf"),
        ("The Book of Souls", "The Book of Souls (3LP Vinyl)", "flac")] {
        let lib = Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        album(&lib, 1, title, "Iron Maiden");
        album(&lib, 2, annotated, "Iron Maiden");
        track(&lib, 1, 1, source, "/first/1.flac", 16, 44100);
        track(&lib, 2, 2, source, &format!("/second/1.{extension}"), 24, 96000);
        assert_eq!(lib.grouped_album_ids(&[1, 2]).unwrap().len(), 1);
        assert_eq!(lib.version_catalog().unwrap().choices(1).len(), 2);
        assert_eq!(lib.album_tracks(1).unwrap()[0].id, 1);
        assert_eq!(lib.album_tracks(2).unwrap()[0].id, 2);
        assert!(lib.select_album_version(1, 2).unwrap());
        assert_eq!(lib.grouped_album_ids(&[1, 2]).unwrap(), [2]);
        lib.set_group_album_versions(false).unwrap();
        assert_eq!(lib.grouped_album_ids(&[1, 2]).unwrap(), [1, 2]);
    }
    assert_ne!(album_group("Record (Live Vinyl)", "Band"), album_group("Record", "Band"));
    assert_ne!(album_group("Record (Part II)", "Band"), album_group("Record", "Band"));
}

#[test]
fn quality_then_tracks_then_remaster_and_remembered_choice() {
    let lib = Library::in_memory().unwrap();
    let source = lib.source_id("local", "default").unwrap();
    for (id, title) in [(1, "Coda"), (2, "Coda [Remastered]"), (3, "Coda (Deluxe Edition)"), (4, "Coda [24 Bit Remaster]")] {
        album(&lib, id, title, "Led Zeppelin");
        track(&lib, id, id, source, &format!("/v{id}/1.flac"), if id == 4 { 24 } else { 16 }, if id == 4 { 96000 } else { 44100 });
    }
    track(&lib, 30, 3, source, "/v3/2.flac", 16, 44100);
    assert_eq!(lib.grouped_album_ids(&[1, 2, 3, 4]).unwrap(), vec![4]);
    assert_eq!(lib.grouped_album_ids(&[1, 2, 3]).unwrap(), vec![3]);
    assert_eq!(lib.grouped_album_ids(&[1, 2]).unwrap(), vec![2]);
    assert!(lib.select_album_version(4, 1).unwrap());
    assert_eq!(lib.selected_album(4), 1);
    assert_eq!(lib.grouped_album_ids(&[1, 2, 3, 4]).unwrap(), vec![1]);
    assert_eq!(lib.grouped_album_ids(&[2, 3, 4]).unwrap(), vec![4], "a filtered preference cannot escape its filter");
    lib.set_group_album_versions(false).unwrap();
    assert_eq!(lib.grouped_album_ids(&[1, 2, 3, 4]).unwrap(), vec![1, 2, 3, 4]);
    assert_eq!(lib.selected_album(4), 4);
    lib.set_group_album_versions(true).unwrap();
    assert_eq!(lib.selected_album(4), 1);
}

#[test]
fn the_best_of_a_record_is_its_best_edition_that_plays() {
    let lib = Library::in_memory().unwrap();
    let source = lib.source_id("local", "default").unwrap();
    for (id, title) in [(1, "Coda"), (2, "Coda [24 Bit Remaster]")] {
        album(&lib, id, title, "Led Zeppelin");
        track(&lib, id, id, source, &format!("/v{id}/1.flac"), if id == 2 { 24 } else { 16 }, if id == 2 { 96000 } else { 44100 });
    }
    assert!(lib.select_album_version(2, 1).unwrap());
    let catalog = lib.version_catalog().unwrap();
    assert_eq!(catalog.best_of("Coda (Deluxe Edition)", "Led Zeppelin"), Some(2), "the best edition, not the remembered one");
    assert_eq!(catalog.best_of("Coda", "Queen"), None);
    lib.connection().execute("UPDATE tracks SET reachable = 0 WHERE album_id = 2", []).unwrap();
    assert_eq!(lib.version_catalog().unwrap().best_of("Coda", "Led Zeppelin"), Some(1), "one that cannot play goes last");
}

#[test]
fn header_search_all_means_only_matching_albums_and_tracks() {
    let lib = Library::in_memory().unwrap();
    let source = lib.source_id("local", "default").unwrap();
    for (id, title) in [(1, "Load"), (2, "Reload"), (3, "Overload"), (4, "Coda")] {
        album(&lib, id, title, "Artist");
        track(&lib, id, id, source, &format!("/v{id}/1.flac"), 16, 44100);
    }
    let mut filter = Filter::default();
    filter.queries[6] = "load".into();
    let found = lib.grouped_snapshot(&filter, TrackOrder::Album).unwrap();
    assert_eq!(found.totals.albums, 3);
    assert_eq!(found.totals.tracks, 3);
    assert!(!found.ids.contains(&4));
    filter.choose(6, &[]);
    assert_eq!(lib.grouped_snapshot(&filter, TrackOrder::Album).unwrap().totals.albums, 3);
    filter.queries[6] = "nothing matches".into();
    assert_eq!(lib.grouped_snapshot(&filter, TrackOrder::Album).unwrap().totals.tracks, 0);
    filter.queries[6].clear();
    assert_eq!(lib.grouped_snapshot(&filter, TrackOrder::Album).unwrap().totals.albums, 4);
}

#[test]
fn mixed_legacy_album_is_split_without_changing_track_ids_or_history() {
    let lib = Library::in_memory().unwrap();
    let source = lib.source_id("local", "default").unwrap();
    album(&lib, 1, "A Hard Day's Night", "The Beatles");
    track(&lib, 1, 1, source, "/music/44/CD1/01.flac", 24, 44100);
    track(&lib, 2, 1, source, "/music/44/CD2/01.flac", 24, 44100);
    track(&lib, 3, 1, source, "/music/96/01.flac", 24, 96000);
    lib.set_album_favorite(1, true).unwrap();
    lib.log_play(Some(3), None, 1, 1000, 1000, false, None, None, true).unwrap();
    lib.separate_local_editions().unwrap();
    let ids = lib.album_ids(AlbumSort::Title).unwrap();
    assert_eq!(ids.len(), 2);
    assert_eq!(lib.album_tracks(1).unwrap().iter().map(|t| t.id).collect::<Vec<_>>(), vec![1, 2]);
    let best = lib.selected_album(1);
    assert_ne!(best, 1);
    assert_eq!(lib.album_tracks(best).unwrap()[0].id, 3);
    assert_eq!(lib.album_play_count(best).unwrap(), 1);
    assert!(lib.favorite_albums().unwrap().contains(&best));
    lib.separate_local_editions().unwrap();
    assert_eq!(lib.album_ids(AlbumSort::Title).unwrap(), ids);
    assert!(lib.check().unwrap().is_empty());
}

#[test]
fn grouped_results_respect_source_exclusions_and_never_interleave_versions() {
    let lib = Library::in_memory().unwrap();
    let local = lib.source_id("local", "default").unwrap();
    let server = lib.source_id("server", "home").unwrap();
    album(&lib, 1, "Coda", "Led Zeppelin");
    album(&lib, 2, "Coda (Remastered)", "Led Zeppelin");
    track(&lib, 1, 1, local, "/coda/1.flac", 16, 44100);
    track(&lib, 2, 2, server, "opaque-release-track", 24, 96000);
    assert_eq!(lib.grouped_snapshot(&Filter::default(), TrackOrder::Album).unwrap().ids, vec![2]);
    let mut filter = Filter { excluded_sources: vec!["server:home".into()], ..Filter::default() };
    assert_eq!(lib.grouped_snapshot(&filter, TrackOrder::Album).unwrap().ids, vec![1]);
    filter.excluded_sources.clear();
    filter.albums = vec![1];
    assert_eq!(lib.grouped_snapshot(&filter, TrackOrder::Album).unwrap().ids, vec![2]);
}

#[test]
fn sibling_disc_folders_are_one_complete_edition() {
    assert_eq!(chilpa_library::store::versions::local_location("/music/İstanbul CD 1/01.flac"), "/music/İstanbul CD 1");
    let lib = Library::in_memory().unwrap();
    let source = lib.source_id("local", "default").unwrap();
    album(&lib, 1, "Past Masters", "The Beatles");
    album(&lib, 2, "Past Masters", "The Beatles");
    track(&lib, 1, 1, source, "/Beatles/Past Masters/CD 1/01.flac", 24, 44100);
    track(&lib, 2, 2, source, "/Beatles/Past Masters/CD 2/19.flac", 24, 44100);
    lib.separate_local_editions().unwrap();
    assert_eq!(lib.album_ids(AlbumSort::Title).unwrap(), vec![1]);
    assert_eq!(lib.album_tracks(1).unwrap().len(), 2);
}

#[test]
fn selection_survives_connections_and_an_unavailable_version_falls_back() {
    let dir = std::env::temp_dir().join(format!("chilpa-version-selection-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("library.db");
    let lib = Library::open(&path).unwrap();
    let source = lib.source_id("local", "default").unwrap();
    album(&lib, 1, "Coda", "Led Zeppelin");
    album(&lib, 2, "Coda [Remaster]", "Led Zeppelin");
    track(&lib, 1, 1, source, "/one/1.flac", 16, 44100);
    track(&lib, 2, 2, source, "/two/1.flac", 24, 96000);
    let reader = Library::open(&path).unwrap();
    lib.set_group_artist_versions(false).unwrap();
    assert!(!reader.group_artist_versions(), "the artist preference is visible across connections");
    assert_eq!(reader.selected_album(1), 2);
    lib.select_album_version(2, 1).unwrap();
    assert_eq!(reader.selected_album(2), 1, "an existing reader notices the committed choice");
    drop(reader);
    drop(lib);
    let lib = Library::open(&path).unwrap();
    assert!(!lib.group_artist_versions(), "the artist preference survives reopening");
    assert_eq!(lib.selected_album(2), 1);
    lib.connection().execute("UPDATE tracks SET present = 0 WHERE id = 1", []).unwrap();
    assert_eq!(lib.selected_album(2), 2);
    lib.connection().execute("UPDATE tracks SET present = 1 WHERE id = 1", []).unwrap();
    assert_eq!(lib.selected_album(2), 1, "availability does not overwrite the preference");
    drop(lib);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
#[ignore = "migrates an explicitly supplied disposable database copy"]
fn inspect_a_disposable_library_copy() {
    let path = std::env::var("CHILPA_TEST_LIBRARY_COPY").expect("a disposable copy path");
    let lib = Library::open(std::path::Path::new(&path)).unwrap();
    assert!(lib.check().unwrap().is_empty());
    let catalog = lib.version_catalog().unwrap();
    for title in ["A.T. EVA01 Reference CD", "EVANGELION 2.0 YOU CAN (NOT) ADVANCE", "EVANGELION WIND SYMPHONY", "Refrain of Evangelion"] {
        let ids = lib.album_ids_matching(AlbumSort::Title, title).unwrap();
        for (album, _) in lib.albums_by_ids(&ids).unwrap() {
            eprintln!("compilation: {}: {} tracks, {}", album.title, album.track_count, album.album_artist);
        }
    }
    for artist in ["Claire", "Megumi Hayashibara", "Amano & Kosei", "林原めぐみ"] {
        let mut filter = Filter::default();
        filter.queries[5] = artist.into();
        let started = std::time::Instant::now();
        let snapshot = lib.grouped_snapshot(&filter, TrackOrder::Album).unwrap();
        eprintln!("member {artist}: {} albums, {} tracks, {:?}", snapshot.album_ids.len(), snapshot.ids.len(), started.elapsed());
        assert!(!snapshot.ids.is_empty());
    }
    let started = std::time::Instant::now();
    let artists = lib.facet_artists(&Filter::default()).unwrap();
    eprintln!("artist facets: {} entries, {:?}", artists.len(), started.elapsed());
    for artist in ["Led Zeppelin", "The Beatles"] {
        let albums = lib.albums_filtered(AlbumSort::Title, Some(artist), 1000, 0).unwrap();
        let ids: Vec<i64> = albums.iter().map(|(a, _)| a.id).collect();
        let groups = catalog.collapse(&ids);
        eprintln!("{artist}: {} editions, {} groups", ids.len(), groups.len());
        for id in groups {
            let v = &catalog.versions[&id];
            eprintln!("  {}: {} tracks, {:?}/{:?}, {} versions", v.title, v.tracks, v.bits, v.rate, catalog.choices(id).len());
        }
    }
    for query in ["coda", "load", "a hard day's night"] {
        let mut filter = Filter::default();
        filter.queries[6] = query.into();
        let snapshot = lib.grouped_snapshot(&filter, TrackOrder::Album).unwrap();
        eprintln!("{query}: {:?}", snapshot.totals);
        assert_eq!(snapshot.totals.albums, if query == "load" { 3 } else { 1 });
        if query == "a hard day's night" { assert_eq!(snapshot.totals.tracks, 13); }
    }
}
