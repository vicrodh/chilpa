use super::*;
use chilpa_library::Library;

fn seed(lib: &Library) -> (i64, i64) {
    let local = lib.source_id("local", "default").unwrap();
    let server = lib.source_id("server", "home").unwrap();
    let first = lib.source_edition(local, "box", "Box", "Artist", Some(2000)).unwrap();
    let second = lib.source_edition(server, "box", "Box [Remastered]", "Artist", Some(2000)).unwrap();
    for (album, source, bits, rate) in [(first, local, 16, 44100), (second, server, 24, 96000)] {
        for (number, disc) in [(1, 1), (2, 1), (1, 2)] {
            lib.connection().execute("INSERT INTO tracks(source_id, external_id, title, artist, album_id,
                duration_ms, added_at, bit_depth, sample_rate_hz, codec, track_number, disc_number)
                VALUES (?1, ?2, 'Track', 'Artist', ?3, 1000, 1, ?4, ?5, 'flac', ?6, ?7)",
                rusqlite::params![source, format!("/music/{album}/CD{disc}/{number}.flac"), album, bits, rate, number, disc]).unwrap();
        }
    }
    (first, second)
}

#[test]
fn page_matches_the_existing_header_versions_and_disc_rows() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.db");
    let lib = Library::open(&path).unwrap();
    let (first, second) = seed(&lib);
    let mut api = Chilpa::new();
    api.library = Some(lib);
    let expected = api.album_detail(second).unwrap();
    let heading = api.album_heading(&expected, &crate::format::span(expected.duration_ms));
    let entries = api.album_track_entries(second);
    let versions = api.album_versions(second);
    let artist_information = api.album_artist_information(second);
    let reader = Reader::open(&path).unwrap();
    let page = reader.album_page(first, false).unwrap();
    assert_eq!(page.detail, expected);
    assert_eq!(page.heading.kind, heading.kind);
    assert_eq!(page.heading.meta, heading.meta);
    assert_eq!(page.heading.discs, heading.discs);
    assert_eq!(page.heading.info, heading.info);
    assert_eq!(page.heading.summary, heading.summary);
    assert_eq!(page.entries, entries);
    assert_eq!(page.artist_information, artist_information);
    assert_eq!(serde_json::to_value(page.versions).unwrap(), serde_json::to_value(versions).unwrap());
    assert_eq!(page.entries.iter().filter(|entry| matches!(entry, TrackEntry::Header(_))).count(), 2);
    assert_eq!(reader.album_page(first, true).unwrap().detail.album.id, first, "a filtered grid keeps the clicked edition");
}

#[test]
fn a_read_only_page_invalidates_version_preferences_and_track_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.db");
    let lib = Library::open(&path).unwrap();
    let (first, second) = seed(&lib);
    let reader = Reader::open(&path).unwrap();
    assert_eq!(reader.album_page(first, false).unwrap().detail.album.id, second);
    let cached = reader.lib.version_catalog().unwrap();
    assert!(std::sync::Arc::ptr_eq(&cached, &reader.lib.version_catalog().unwrap()), "read-only page snapshots keep the version catalog warm");
    let in_snapshot = reader.lib.read_snapshot(|| reader.lib.version_catalog().unwrap()).unwrap();
    assert!(std::sync::Arc::ptr_eq(&cached, &in_snapshot), "a committed read-only snapshot reuses the same version catalog");
    assert!(reader.select_album_version(first, first));
    lib.connection().execute("UPDATE tracks SET duration_ms = 2000, title = 'Edited' WHERE album_id = ?1", [first]).unwrap();
    let page = reader.album_page(second, false).unwrap();
    assert_eq!(page.detail.album.id, first);
    assert_eq!(page.detail.duration_ms, 6000);
    assert!(page.entries.iter().filter_map(|entry| match entry { TrackEntry::Track(row) => Some(row), _ => None }).all(|row| row.title == "Edited"));
    assert!(!std::sync::Arc::ptr_eq(&cached, &reader.lib.version_catalog().unwrap()));
    lib.set_group_album_versions(false).unwrap();
    assert_eq!(reader.album_page(second, false).unwrap().detail.album.id, second);
    assert!(reader.album_page(i64::MAX, false).is_none());
}

#[test]
fn a_writer_cannot_mix_the_selection_header_and_rows_in_one_page() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.db");
    let lib = Library::open(&path).unwrap();
    let (first, second) = seed(&lib);
    let reader = Reader::open(&path).unwrap();
    reader.lib.read_snapshot(|| {
        assert_eq!(reader.lib.selected_album(first), second);
        lib.select_album_version(first, first).unwrap();
        lib.connection().execute("UPDATE tracks SET duration_ms = 9000 WHERE album_id = ?1", [second]).unwrap();
        assert_eq!(reader.lib.selected_album(first), second, "preference reads stay on the page's committed snapshot");
        let rows = reader.lib.browse_tracks(&crate::Filter { album: Some(second), ..Default::default() }, crate::TrackOrder::Disc, 100).unwrap();
        assert_eq!(rows.iter().map(|row| row.duration_ms).sum::<u64>(), 3000);
    }).unwrap();
    let page = reader.album_page(first, false).unwrap();
    assert_eq!(page.detail.album.id, first);
    assert_eq!(reader.album_page(second, true).unwrap().detail.duration_ms, 27000);
}

#[test]
fn page_artist_links_preserve_compilation_credit_parts_and_metadata_updates() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("library.db");
    let library = Library::open(&path).unwrap();
    let (album, _) = seed(&library);
    library.connection().execute("UPDATE albums SET album_artist = 'Various Artists' WHERE id = ?1", [album]).unwrap();
    library.connection().execute("UPDATE tracks SET artist = 'Player & <Band>, Guest' WHERE album_id = ?1", [album]).unwrap();
    library.state_credit("Player & <Band>, Guest", &["Player & <Band>".into(), "Guest".into()]).unwrap();
    let reader = Reader::open(&path).unwrap();
    let mut api = Chilpa::new();
    api.library = Some(library);
    let page = reader.album_page(album, true).unwrap();
    assert_eq!(page.artist_information, api.album_artist_information(album));
    let document: serde_json::Value = serde_json::from_str(&page.artist_information).unwrap();
    assert_eq!(document["artist_links"].as_array().unwrap().len(), 3, "I preserve the combined tag fallback alongside its stated people");
    assert!(document["artist_html"].as_str().unwrap().contains("&amp; &lt;Band&gt;"));
    api.library.as_ref().unwrap().connection().execute("UPDATE tracks SET artist = 'Replacement' WHERE album_id = ?1", [album]).unwrap();
    assert_eq!(reader.album_page(album, true).unwrap().artist_information, api.album_artist_information(album));
}
