use super::*;
use chilpa_library::Library;

fn seed(lib: &Library) -> (i64, i64) {
    let source = lib.source_id("local", "default").unwrap();
    let mut ids = Vec::new();
    // I keep the shared genre rare enough to pass the production relevance floor.
    for index in 0..48 {
        let artist = if index < 2 { "Artist".into() } else { format!("Band {index}") };
        let album = lib.source_edition(source, &format!("edition:{index}"), &format!("Record {index}"), &artist, Some(2000 + index)).unwrap();
        lib.connection().execute("INSERT INTO tracks(source_id, external_id, title, artist, album_id, duration_ms, added_at, bit_depth, sample_rate_hz)
            VALUES (?1, ?2, 'Song', ?3, ?4, 1000, 1, 16, 44100)", rusqlite::params![source, format!("/music/{album}.flac"), artist, album]).unwrap();
        let track = lib.connection().last_insert_rowid();
        let genre = if matches!(index, 0 | 1 | 2 | 3) { "Chamber".into() } else { format!("Genre {index}") };
        lib.set_track_genres(track, &[genre]).unwrap();
        ids.push(album);
    }
    lib.update_statistics().unwrap();
    (ids[0], ids[1])
}

#[test]
fn worker_shelves_match_local_order_grouping_and_related_metadata() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("library.db");
    let library = Library::open(&path).unwrap();
    let (target, other) = seed(&library);
    let mut api = crate::Chilpa::new();
    api.library = Some(library);
    let mut reader = Reader::open(&path).unwrap();
    reader.catalog = Some(api.catalog.clone());
    assert_eq!(reader.album_shelf("sameArtist", target, 8).iter().map(|card| card.id).collect::<Vec<_>>(), [other]);
    assert!(!reader.album_shelf("related", target, 8).is_empty(), "the fixture must exercise ranked recommendations");
    for grouped in [true, false] {
        api.library.as_ref().unwrap().set_group_album_versions(grouped).unwrap();
        for limit in [1, 2, 8] {
            assert_eq!(reader.album_shelf("sameArtist", target, limit), api.same_artist_albums(target, limit));
            assert_eq!(reader.album_shelf("related", target, limit), api.related_albums(target, limit));
        }
    }
    let track = api.library.as_ref().unwrap().connection().query_row("SELECT id FROM tracks WHERE album_id = ?1", [target], |row| row.get(0)).unwrap();
    api.library.as_ref().unwrap().set_track_genres(track, &["Changed".into()]).unwrap();
    assert_eq!(reader.album_shelf("related", target, 8), api.related_albums(target, 8), "another connection's metadata invalidates ranked shelves");
    assert!(reader.album_shelf("sameArtist", i64::MAX, 8).is_empty());
}

#[test]
fn source_shelves_stay_interleaved_bounded_and_exclude_the_open_record() {
    let album = |id| Album { id, title: format!("Record {id}"), artist: "Artist".into(), year: None, track_count: 1, artwork_path: String::new(), play_count: 0, explicit: None };
    let groups = vec![vec![album(7), album(4), album(2)], vec![album(-3)], vec![album(-5)]];
    assert_eq!(merge_shelves(7, groups.clone(), 8).iter().map(|card| card.id).collect::<Vec<_>>(), [2, -3, -5, 4]);
    assert_eq!(merge_shelves(7, groups.clone(), 3).iter().map(|card| card.id).collect::<Vec<_>>(), [2, -3, -5]);
    assert!(merge_shelves(7, groups, 0).is_empty());
}
