use chilpa_library::{Library, AlbumSort, Filter, TrackOrder};
use rusqlite::params;

fn library() -> Library {
    let lib = Library::in_memory().unwrap();
    assert_eq!(lib.source_id("local", "default").unwrap(), 1);
    lib
}

fn part(lib: &Library, id: i64, path: &str, disc: Option<u16>, total: Option<u16>) {
    lib.connection().execute("INSERT INTO albums(id, title, album_artist, folder) VALUES (?1, 'Set', 'Band', ?2)", params![id, path]).unwrap();
    lib.connection().execute("INSERT INTO tracks(id, source_id, external_id, title, artist, album_credit,
        album_title, album_id, disc_number, disc_total, track_number, added_at)
        VALUES (?1, 1, ?2, 'Track', 'Band', 'Band', 'Set', ?1, ?3, ?4, 1, 1)", params![id, path, disc, total]).unwrap();
}

fn ids(lib: &Library) -> Vec<i64> { lib.album_ids(AlbumSort::Title).unwrap() }

#[test]
fn tag_evidence_assembles_named_parts_and_preserves_every_reference() {
    let lib = library();
    part(&lib, 10, "/box/second/01.flac", Some(2), Some(2));
    part(&lib, 20, "/box/first/01.flac", Some(1), Some(2));
    lib.connection().execute_batch("UPDATE albums SET year = 1999, artwork_path = 'second.jpg' WHERE id = 10;
        UPDATE albums SET year = 1980, artwork_path = 'first.jpg' WHERE id = 20;
        INSERT INTO album_preferences VALUES ('saved', 20);
        INSERT INTO albums(id, title, album_artist) VALUES (30, 'Empty', 'Band');").unwrap();
    lib.set_album_favorite(20, true).unwrap();
    lib.log_play(Some(20), None, 1, 1000, 1000, false, None, None, true).unwrap();
    let playlist = lib.create_playlist(1, "Mix", "mix").unwrap();
    let entries = lib.playlist_entries_for_tracks(&[10, 20]).unwrap();
    lib.append_to_playlist(playlist, &entries).unwrap();
    lib.assemble_local_releases().unwrap();
    assert_eq!(ids(&lib), [10]);
    assert_eq!(lib.album_tracks(10).unwrap().iter().map(|t| t.id).collect::<Vec<_>>(), [20, 10]);
    assert_eq!(lib.playlist_entries(playlist).unwrap(), entries);
    assert_eq!(lib.album_play_count(10).unwrap(), 1);
    assert!(lib.favorite_albums().unwrap().contains(&10));
    assert_eq!(lib.connection().query_row("SELECT album_id FROM album_preferences WHERE group_key = 'saved'", [], |r| r.get::<_, i64>(0)).unwrap(), 10);
    assert_eq!(lib.connection().query_row("SELECT year, artwork_path FROM albums", [], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))).unwrap(), (1980, "first.jpg".into()));
    assert_eq!(lib.version_catalog().unwrap().choices(10)[0].tracks, 2);
    assert_eq!(lib.presented_artists(100, 0).unwrap()[0].album_count, 1);
    let changes = lib.connection().total_changes();
    lib.assemble_local_releases().unwrap();
    assert_eq!(lib.connection().total_changes(), changes, "the second pass makes no writes");
    assert!(lib.check().unwrap().is_empty());
}

#[test]
fn folders_supply_only_confirmed_disc_evidence() {
    for (one, two) in [("CD 1 - Mono", "CD 2 - Stereo"), ("CD 1 Mono", "CD 2 Stereo"), ("Disc 1 1975-1980", "Disc 2 1980-1986"), ("CD 01", "CD 02")] {
        let lib = library();
        part(&lib, 1, &format!("/box/{one}/01.flac"), None, None);
        part(&lib, 2, &format!("/box/{two}/01.flac"), None, None);
        lib.assemble_local_releases().unwrap();
        assert_eq!(ids(&lib), [1], "{one}");
        let rows = lib.browse_tracks(&Filter::default(), TrackOrder::Disc, 10).unwrap();
        assert_eq!(rows.iter().map(|t| t.disc_number).collect::<Vec<_>>(), [Some(1), Some(2)]);
    }
    let lib = library();
    part(&lib, 1, "/box/CD 1 Mono/01.flac", None, None);
    part(&lib, 2, "/box/ordinary/01.flac", Some(2), None);
    lib.assemble_local_releases().unwrap();
    assert_eq!(ids(&lib).len(), 2, "a weak name without confirmation supplies nothing");
    lib.connection().execute("UPDATE tracks SET disc_number = 1 WHERE id = 1", []).unwrap();
    lib.assemble_local_releases().unwrap();
    assert_eq!(ids(&lib).len(), 1, "matching tags confirm a weak name");
}

#[test]
fn incompatible_or_missing_evidence_never_joins_parts() {
    for (one, two, d1, d2, n1, n2) in [
        ("/box/a/1.flac", "/box/b/1.flac", Some(1), Some(1), None, None),
        ("/box/a/1.flac", "/box/b/1.flac", Some(1), Some(2), Some(2), Some(3)),
        ("/box/a/1.flac", "/box/b/1.flac", Some(1), Some(3), Some(2), Some(2)),
        ("/box/a/1.flac", "/other/b/1.flac", Some(1), Some(2), None, None),
        ("/box/a/1.flac", "/box/b/1.flac", None, None, None, None),
        ("/box/Name CD1/1.flac", "/box/Name CD2/1.flac", None, None, None, None),
    ] {
        let lib = library();
        part(&lib, 1, one, d1, n1);
        part(&lib, 2, two, d2, n2);
        lib.assemble_local_releases().unwrap();
        assert_eq!(ids(&lib).len(), 2, "{one}, {two}, {d1:?}/{n1:?}, {d2:?}/{n2:?}");
    }
}

#[test]
fn exact_album_credit_and_source_are_required_not_presentation_keys() {
    for sql in ["UPDATE tracks SET album_title = 'Set (Anniversary Edition)' WHERE id = 2",
        "UPDATE tracks SET album_credit = 'Other' WHERE id = 2",
        "INSERT INTO sources(id, provider, instance) VALUES (2, 'local', 'other'); UPDATE tracks SET source_id = 2 WHERE id = 2"] {
        let lib = library();
        part(&lib, 1, "/box/a/1.flac", Some(1), None);
        part(&lib, 2, "/box/b/1.flac", Some(2), None);
        lib.connection().execute_batch(sql).unwrap();
        lib.assemble_local_releases().unwrap();
        assert_eq!(ids(&lib).len(), 2);
    }
    let lib = library();
    part(&lib, 1, "/box/a/1.flac", Some(1), None);
    part(&lib, 2, "/box/b/1.flac", Some(2), None);
    lib.connection().execute_batch("UPDATE tracks SET album_title = ' SET ', album_credit = ' band ' WHERE id = 2").unwrap();
    lib.assemble_local_releases().unwrap();
    assert_eq!(ids(&lib).len(), 1);
}

#[test]
fn sacd_set_tags_and_confirmed_folders_keep_the_raw_toc() {
    for toc in [true, false] {
        let lib = library();
        part(&lib, 1, "sacd:/box/Disc 1/one.iso#1", Some(1), Some(if toc { 2 } else { 1 }));
        part(&lib, 2, "sacd:/box/Disc 2/two.iso#1", Some(if toc { 2 } else { 1 }), Some(if toc { 2 } else { 1 }));
        lib.assemble_local_releases().unwrap();
        assert_eq!(ids(&lib), [1]);
        assert_eq!(lib.connection().query_row("SELECT disc_number FROM tracks WHERE id = 2", [], |r| r.get::<_, i64>(0)).unwrap(), if toc { 2 } else { 1 });
        assert_eq!(lib.browse_tracks(&Filter::default(), TrackOrder::Disc, 10).unwrap()[1].disc_number, Some(2));
    }
}

#[test]
fn partial_refresh_and_an_overlapping_edition_do_not_interleave() {
    let lib = library();
    part(&lib, 1, "/set/one/1.flac", Some(1), Some(2));
    part(&lib, 2, "/set/two/1.flac", Some(2), Some(2));
    lib.assemble_local_releases().unwrap();
    lib.connection().execute_batch("INSERT INTO albums(id, title, album_artist, folder) VALUES (9, 'Set', 'Band', '/set/two');
        UPDATE tracks SET album_id = 9 WHERE id = 2").unwrap();
    lib.assemble_local_releases().unwrap();
    assert_eq!(ids(&lib), [1]);
    part(&lib, 3, "/edition/disc/1.flac", Some(1), Some(2));
    lib.assemble_local_releases().unwrap();
    assert_eq!(ids(&lib).len(), 2);
    assert_eq!(lib.album_tracks(1).unwrap().len(), 2);
    assert_eq!(lib.album_tracks(3).unwrap().len(), 1);
}

#[test]
fn compilation_parts_keep_contributors_and_do_not_become_versions() {
    let lib = library();
    part(&lib, 1, "/set/CD1/1.flac", None, None);
    part(&lib, 2, "/set/CD2/1.flac", None, None);
    lib.connection().execute_batch("UPDATE tracks SET album_credit = 'Various Artists';
        UPDATE tracks SET artist = 'Singer A' WHERE id = 1;
        UPDATE tracks SET artist = 'Singer B' WHERE id = 2;").unwrap();
    lib.assemble_local_releases().unwrap();
    lib.set_group_album_versions(false).unwrap();
    assert_eq!(ids(&lib), [1]);
    for artist in ["Singer A", "Singer B"] {
        let filter = Filter { artist: Some(artist.into()), ..Filter::default() };
        assert_eq!(lib.grouped_snapshot(&filter, TrackOrder::Disc).unwrap().ids.len(), 2);
        assert_eq!(lib.find(artist, 20).unwrap().albums, [1]);
    }
}

#[test]
fn a_guest_on_one_disc_does_not_split_the_other_discs() {
    let lib = library();
    part(&lib, 1, "/set/Disc 1/1.flac", Some(1), None);
    part(&lib, 2, "/set/Disc 2/1.flac", Some(2), None);
    lib.connection().execute_batch("INSERT INTO tracks(id, source_id, external_id, title, artist, album_credit,
        album_title, album_id, disc_number, added_at) VALUES
        (3, 1, '/set/Disc 1/2.flac', 'Song', 'Guest', 'Guest', 'Set', 1, 1, 1);").unwrap();
    lib.assemble_local_releases().unwrap();
    assert_eq!(ids(&lib), [1]);
    assert_eq!(lib.album(1).unwrap().unwrap().0.album_artist, "Various Artists");
    assert_eq!(lib.find("Guest", 10).unwrap().albums, [1]);
}

#[test]
fn metadata_variants_with_overlapping_discs_in_one_folder_are_not_interleaved() {
    let lib = library();
    part(&lib, 1, "/edition/01.flac", Some(1), None);
    part(&lib, 2, "/edition/01 (remastered).flac", Some(1), None);
    lib.connection().execute("UPDATE tracks SET album_title = 'Set ' WHERE id = 2", []).unwrap();
    lib.assemble_local_releases().unwrap();
    assert_eq!(ids(&lib).len(), 2);
}

#[test]
fn an_ambiguous_new_disc_cannot_be_assigned_to_an_existing_set_by_guessing() {
    let lib = library();
    part(&lib, 1, "/box/one/1.flac", Some(1), Some(2));
    part(&lib, 2, "/box/two/1.flac", Some(2), Some(2));
    lib.assemble_local_releases().unwrap();
    part(&lib, 3, "/box/another/1.flac", Some(1), Some(2));
    lib.assemble_local_releases().unwrap();
    assert_eq!(ids(&lib).len(), 3, "no unique assignment remains in this container");
    assert!(ids(&lib).iter().all(|id| lib.album_tracks(*id).unwrap().len() == 1));
    let changes = lib.connection().total_changes();
    lib.assemble_local_releases().unwrap();
    assert_eq!(changes, lib.connection().total_changes());
}

#[test]
fn every_present_track_needs_evidence_and_missing_art_uses_the_next_disc() {
    let lib = library();
    part(&lib, 1, "/box/one/1.flac", Some(1), Some(2));
    part(&lib, 2, "/box/two/1.flac", Some(2), Some(2));
    lib.connection().execute_batch("INSERT INTO tracks(id, source_id, external_id, title, artist, album_credit,
        album_title, album_id, added_at) VALUES (3, 1, '/box/one/2.flac', 'Song', 'Band', 'Band', 'Set', 1, 1);
        UPDATE albums SET artwork_path = 'second.jpg', year = 1982 WHERE id = 2;").unwrap();
    lib.assemble_local_releases().unwrap();
    assert_eq!(ids(&lib).len(), 2);
    lib.connection().execute("UPDATE tracks SET present = 0 WHERE id = 3", []).unwrap();
    lib.assemble_local_releases().unwrap();
    assert_eq!(ids(&lib), [1]);
    assert_eq!(lib.connection().query_row("SELECT year, artwork_path FROM albums", [], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))).unwrap(), (1982, "second.jpg".into()));
}

#[test]
#[ignore = "opens only an explicitly supplied disposable database copy"]
fn audit_disposable_copy() {
    let path = std::env::var("CHILPA_TEST_LIBRARY_COPY").unwrap();
    let lib = Library::open(std::path::Path::new(&path)).unwrap();
    lib.assemble_local_releases().unwrap();
    let changes = lib.connection().total_changes();
    let started = std::time::Instant::now();
    lib.assemble_local_releases().unwrap();
    assert_eq!(changes, lib.connection().total_changes());
    eprintln!("unchanged assembly: {:?}", started.elapsed());
    assert!(lib.check().unwrap().is_empty());
    eprintln!("present releases: {}", ids(&lib).len());
}
