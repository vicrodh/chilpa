use super::*;
use crate::store::PlaylistEntry;
use crate::store::ratings::RatingTarget;
use std::collections::HashSet;

fn new_server(instance: &str, identity: &str) -> NewServer {
    NewServer {
        provider: "example-server".into(),
        instance: instance.into(),
        label: format!("Server {instance}"),
        icon: "server".into(),
        address: format!("http://{instance}.lan:4533"),
        account: "me".into(),
        identity: identity.into(),
        product: "Example 1.0".into(),
        libraries: "[]".into(),
        details: "{}".into(),
    }
}

fn track(id: &str, album: &str) -> ServerTrack {
    ServerTrack {
        external_id: id.into(),
        title: format!("Song {id}"),
        artist: "Band".into(),
        album: album.into(),
        album_artist: "Band".into(),
        album_key: format!("al-{album}"),
        cover: Some(format!("cover-{album}")),
        track_number: Some(1),
        disc_number: Some(1),
        year: Some(2001),
        genres: vec!["Rock".into(), "Pop".into()],
        duration_ms: 180_000,
        codec: Some("flac".into()),
        bit_depth: Some(24),
        sample_rate_hz: Some(96_000),
        channels: Some(2),
        revision: Some(format!("r1-{id}")),
        ..ServerTrack::default()
    }
}

fn synced(lib: &Library, source: i64, tracks: &[ServerTrack]) -> usize {
    let generation = lib.begin_server_sync(source).unwrap();
    lib.save_server_tracks(source, generation, tracks).unwrap();
    lib.finish_server_sync(source, generation).unwrap()
}

#[test]
fn disabling_a_server_hides_its_library_entries_but_keeps_playlist_entries() {
    let lib = Library::in_memory().unwrap();
    let server = lib.add_server(&new_server("aaaa", "server-id-1")).unwrap();
    synced(&lib, server, &[track("one", "One")]);
    let track_id: i64 = lib.db.query_row("SELECT id FROM tracks WHERE source_id = ?1", [server], |r| r.get(0)).unwrap();
    let playlist = lib.create_playlist(lib.source_id("local", "default").unwrap(), "Saved", "saved").unwrap();
    lib.set_playlist_entries(playlist, &[PlaylistEntry {
        track_id: Some(track_id), source_id: Some(server), external_id: "one".into(),
    }]).unwrap();

    assert_eq!(lib.album_ids(crate::AlbumSort::Title).unwrap().len(), 1);
    assert_eq!(lib.browse_ids(&crate::Filter::default(), crate::TrackOrder::default()).unwrap().ids, [track_id]);
    assert_eq!(lib.artists_by_role(crate::ArtistRole::Any).unwrap().len(), 1);
    assert!(lib.set_server_enabled(server, false).unwrap());
    assert!(lib.album_ids(crate::AlbumSort::Title).unwrap().is_empty());
    assert!(lib.browse_ids(&crate::Filter::default(), crate::TrackOrder::default()).unwrap().ids.is_empty());
    assert!(lib.artists_by_role(crate::ArtistRole::Any).unwrap().is_empty());
    assert!(lib.find("song", 10).unwrap().tracks.is_empty());
    assert_eq!(lib.playlist_entries(playlist).unwrap().len(), 1);
    assert!(!lib.tracks_by_ids(&[track_id]).unwrap()[0].available);

    assert!(lib.set_server_enabled(server, true).unwrap());
    assert_eq!(lib.album_ids(crate::AlbumSort::Title).unwrap().len(), 1);
    lib.remove_source(server).unwrap();
    assert!(lib.playlist_entries(playlist).unwrap().is_empty());
}

#[test]
fn editing_a_server_keeps_its_source_and_playlist_references() {
    let lib = Library::in_memory().unwrap();
    let server = lib.add_server(&new_server("aaaa", "server-id-1")).unwrap();
    synced(&lib, server, &[track("one", "One")]);
    let track_id: i64 = lib.db.query_row("SELECT id FROM tracks WHERE source_id = ?1", [server], |r| r.get(0)).unwrap();
    let playlist = lib.create_playlist(lib.source_id("local", "default").unwrap(), "Saved", "saved").unwrap();
    lib.set_playlist_entries(playlist, &[PlaylistEntry {
        track_id: Some(track_id), source_id: Some(server), external_id: "one".into(),
    }]).unwrap();

    let mut changed = new_server("aaaa", "server-id-1");
    changed.label = "Renamed".into();
    changed.address = "https://music.example.test".into();
    changed.account = "another-user".into();
    lib.edit_server(server, &changed).unwrap();
    let stored = lib.server(server).unwrap().unwrap();
    assert_eq!((&stored.label, &stored.address, &stored.account), (&changed.label, &changed.address, &changed.account));
    assert_eq!(lib.playlist_entries(playlist).unwrap()[0].track_id, Some(track_id));
    assert!(!lib.tracks_by_ids(&[track_id]).unwrap()[0].available);
}

#[test]
fn a_server_is_a_source_of_the_library_with_its_own_settings() {
    let lib = Library::in_memory().unwrap();
    let source = lib.add_server(&new_server("aaaa", "server-id-1")).unwrap();

    assert_eq!(lib.source_kind(source).unwrap().as_deref(), Some("server"));
    let server = lib.server(source).unwrap().unwrap();
    assert_eq!((server.provider.as_str(), server.instance.as_str()), ("example-server", "aaaa"));
    assert_eq!(server.address, "http://aaaa.lan:4533");
    assert!(server.reachable);
    assert_eq!(server.synced_at, None, "it has never finished a sync");
    assert_eq!(lib.server_with_identity("example-server", "server-id-1").unwrap(), Some(source));
    assert_eq!(lib.server_with_identity("example-server", "").unwrap(), None, "no identity matches nothing");
    assert_eq!(lib.server_with_identity("other-server", "server-id-1").unwrap(), None);
}

#[test]
fn a_sync_mirrors_tracks_records_genres_and_covers_into_the_library() {
    let lib = Library::in_memory().unwrap();
    let source = lib.add_server(&new_server("aaaa", "")).unwrap();

    let gone = synced(&lib, source, &[track("1", "One"), track("2", "One"), track("3", "Two")]);

    assert_eq!(gone, 0);
    let server = lib.server(source).unwrap().unwrap();
    assert_eq!((server.tracks, server.albums), (3, 2));
    assert!(server.synced_at.is_some());
    assert_eq!(lib.track_count(false).unwrap(), 3, "they are in the library like any file");
    let albums = lib.album_ids(crate::AlbumSort::Title).unwrap();
    assert_eq!(albums.len(), 2, "and their records are in the grid");
    let key: String = lib.db.query_row("SELECT version_key FROM albums WHERE title = 'One'", [], |r| r.get(0)).unwrap();
    assert_eq!(key, format!("{source}:al-One"), "a record is the server's own, scoped to this server");
    let id: i64 = lib.db.query_row("SELECT id FROM tracks WHERE external_id = '1'", [], |r| r.get(0)).unwrap();
    assert_eq!(lib.track_genres(id).unwrap(), vec!["Pop".to_string(), "Rock".to_string()]);
    assert_eq!(lib.pending_server_covers(source, 10).unwrap().len(), 2, "one cover per record, to fetch");
}

#[test]
fn a_finished_sync_takes_out_what_the_server_no_longer_has_and_a_cut_one_takes_nothing() {
    let lib = Library::in_memory().unwrap();
    let source = lib.add_server(&new_server("aaaa", "")).unwrap();
    synced(&lib, source, &[track("1", "One"), track("2", "One"), track("3", "Two")]);

    // Cut off after one page: nothing is taken out.
    let generation = lib.begin_server_sync(source).unwrap();
    lib.keep_server_tracks(source, generation, &["1".into()]).unwrap();
    assert_eq!(lib.track_count(false).unwrap(), 3);

    // A whole one that no longer sees track 3.
    let generation = lib.begin_server_sync(source).unwrap();
    let revisions = lib.server_revisions(source).unwrap();
    assert_eq!(revisions.get("1").cloned().flatten().as_deref(), Some("r1-1"), "what a sync compares against");
    lib.keep_server_tracks(source, generation, &["1".into(), "2".into()]).unwrap();
    let gone = lib.finish_server_sync(source, generation).unwrap();

    assert_eq!(gone, 1);
    assert_eq!(lib.track_count(false).unwrap(), 2, "not listed");
    assert_eq!(lib.track_count(true).unwrap(), 3, "but kept, for its ratings and history");
    assert_eq!(lib.album_ids(crate::AlbumSort::Title).unwrap().len(), 1, "its record with nothing left leaves the grid");

    // And back when the server has it again.
    synced(&lib, source, &[track("1", "One"), track("2", "One"), track("3", "Two")]);
    assert_eq!(lib.track_count(false).unwrap(), 3);
}

#[test]
fn two_servers_with_the_same_ids_share_nothing() {
    let lib = Library::in_memory().unwrap();
    let a = lib.add_server(&new_server("aaaa", "")).unwrap();
    let b = lib.add_server(&new_server("bbbb", "")).unwrap();
    synced(&lib, a, &[track("1", "One")]);
    synced(&lib, b, &[track("1", "One")]);

    let tracks: i64 = lib.db.query_row("SELECT count(*) FROM tracks WHERE external_id = '1'", [], |r| r.get(0)).unwrap();
    let albums: i64 = lib.db.query_row("SELECT count(*) FROM albums WHERE title = 'One'", [], |r| r.get(0)).unwrap();
    assert_eq!((tracks, albums), (2, 2), "two copies of one record are two versions");
    lib.set_source_reachable(a, false).unwrap();
    let down: i64 = lib.db.query_row("SELECT count(*) FROM tracks WHERE reachable = 0", [], |r| r.get(0)).unwrap();
    assert_eq!(down, 1, "one server going away touches only its own");
}

#[test]
fn a_server_track_without_a_record_id_is_grouped_by_title_and_artist() {
    let lib = Library::in_memory().unwrap();
    let source = lib.add_server(&new_server("aaaa", "")).unwrap();
    let mut one = track("1", "Loose");
    let mut two = track("2", "Loose");
    one.album_key.clear();
    two.album_key.clear();
    synced(&lib, source, &[one, two]);
    assert_eq!(lib.server(source).unwrap().unwrap().albums, 1);
}

/// A local track, a local playlist holding it and two of the server's, my
/// stars and my history on all of them.
fn a_library_with_a_server_mixed_in() -> (Library, i64, i64) {
    let lib = Library::in_memory().unwrap();
    let local = lib.source_id("local", "default").unwrap();
    lib.db.execute("INSERT INTO albums (title, album_artist) VALUES ('Mine', 'Me')", []).unwrap();
    let mine_album = lib.db.last_insert_rowid();
    lib.db.execute(
        "INSERT INTO tracks (source_id, external_id, title, album_id, duration_ms, added_at) VALUES (?1, '/m/a.flac', 'Mine', ?2, 1000, 0)",
        params![local, mine_album],
    ).unwrap();
    let mine = lib.db.last_insert_rowid();

    let server = lib.add_server(&new_server("aaaa", "")).unwrap();
    synced(&lib, server, &[track("1", "One"), track("2", "One"), track("3", "Two")]);
    let id = |external: &str| -> i64 {
        lib.db.query_row("SELECT id FROM tracks WHERE source_id = ?1 AND external_id = ?2", params![server, external], |r| r.get(0)).unwrap()
    };
    let (one, two) = (id("1"), id("2"));
    // A genre only the server's tracks have, and one shared with mine.
    lib.db.execute("INSERT INTO genres (name) VALUES ('Server Only'), ('Shared')", []).unwrap();
    lib.db.execute(
        "INSERT INTO track_genres (track_id, genre_id) SELECT ?1, id FROM genres WHERE name IN ('Server Only', 'Shared')",
        params![one],
    ).unwrap();
    lib.db.execute("INSERT INTO track_genres (track_id, genre_id) SELECT ?1, id FROM genres WHERE name = 'Shared'", params![mine]).unwrap();
    let their_album: i64 = lib.db.query_row("SELECT album_id FROM tracks WHERE id = ?1", params![one], |r| r.get(0)).unwrap();

    let playlist = lib.create_playlist(local, "Mixed", "").unwrap();
    lib.set_playlist_entries(playlist, &[
        PlaylistEntry { track_id: Some(one), source_id: Some(server), external_id: "1".into() },
        PlaylistEntry { track_id: Some(mine), source_id: Some(local), external_id: "/m/a.flac".into() },
        PlaylistEntry { track_id: Some(two), source_id: Some(server), external_id: "2".into() },
    ]).unwrap();
    lib.rate(RatingTarget::Track(one), 4).unwrap();
    lib.rate(RatingTarget::Track(mine), 5).unwrap();
    lib.rate(RatingTarget::Album(their_album), 3).unwrap();
    lib.log_play(Some(one), Some(server), 100, 180_000, 180_000, false, None, None, false).unwrap();
    lib.log_play(Some(mine), Some(local), 200, 1000, 1000, false, None, None, false).unwrap();
    // A listen logged before its track was cached names only the server.
    lib.log_play(None, Some(server), 300, 180_000, 180_000, false, None, None, false).unwrap();
    (lib, server, playlist)
}

#[test]
fn removing_a_server_says_first_what_it_takes() {
    let (lib, server, _) = a_library_with_a_server_mixed_in();
    let removal = lib.removal(server).unwrap();
    assert_eq!(removal, Removal {
        tracks: 3,
        albums: 2,
        ratings: 2,
        plays: 2,
        playlists: 0,
        playlist_entries: 2,
        playlists_touched: 1,
    });
}

#[test]
fn removing_a_server_leaves_no_row_that_points_at_it_and_keeps_what_is_mine() {
    let (lib, server, playlist) = a_library_with_a_server_mixed_in();
    let their_tracks: HashSet<i64> = {
        let mut stmt = lib.db.prepare("SELECT id FROM tracks WHERE source_id = ?1").unwrap();
        stmt.query_map(params![server], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
    };
    let their_albums: HashSet<i64> = {
        let mut stmt = lib.db.prepare("SELECT DISTINCT album_id FROM tracks WHERE source_id = ?1").unwrap();
        stmt.query_map(params![server], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
    };

    lib.remove_source(server).unwrap();

    // Every column of every table that can name a source, a track or a
    // record, checked against what was removed.
    let tables: Vec<String> = {
        let mut stmt = lib.db.prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name NOT LIKE 'track_search%'").unwrap();
        stmt.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
    };
    for table in tables {
        let columns: Vec<String> = {
            let mut stmt = lib.db.prepare(&format!("PRAGMA table_info({table})")).unwrap();
            stmt.query_map([], |r| r.get(1)).unwrap().map(Result::unwrap).collect()
        };
        for column in columns {
            let (ids, what): (Vec<i64>, &str) = match column.as_str() {
                "source_id" => (vec![server], "source"),
                "track_id" => (their_tracks.iter().copied().collect(), "track"),
                "album_id" => (their_albums.iter().copied().collect(), "record"),
                "id" if table == "sources" => (vec![server], "source"),
                "id" if table == "tracks" => (their_tracks.iter().copied().collect(), "track"),
                "id" if table == "albums" => (their_albums.iter().copied().collect(), "record"),
                _ => continue,
            };
            for id in ids {
                let n: i64 = lib.db.query_row(&format!("SELECT count(*) FROM {table} WHERE {column} = ?1"), params![id], |r| r.get(0)).unwrap();
                assert_eq!(n, 0, "{table}.{column} still names the removed {what} {id}");
            }
        }
    }

    // What is mine stays, and my playlist closes up around what left.
    assert_eq!(lib.track_count(true).unwrap(), 1);
    let entries = lib.playlist_entries(playlist).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].external_id, "/m/a.flac");
    let positions: Vec<i64> = {
        let mut stmt = lib.db.prepare("SELECT position FROM playlist_tracks WHERE playlist_id = ?1").unwrap();
        stmt.query_map(params![playlist], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
    };
    assert_eq!(positions, vec![0], "no hole where the server's tracks were");
    let (plays, stars): (i64, i64) = lib.db.query_row(
        "SELECT (SELECT plays FROM library_totals), (SELECT stars FROM track_ratings)", [], |r| Ok((r.get(0)?, r.get(1)?)),
    ).unwrap();
    assert_eq!((plays, stars), (1, 5), "the totals count only what is left, and my stars on my file stay");
    assert!(lib.check().unwrap().is_empty(), "no row points at nothing");
    let genres: Vec<String> = {
        let mut stmt = lib.db.prepare("SELECT name FROM genres ORDER BY name").unwrap();
        stmt.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
    };
    assert_eq!(genres, vec!["Shared".to_string()], "a genre only the server had leaves; one I also have stays");
}

#[test]
fn a_service_s_playlist_copy_never_enters_the_library() {
    let lib = Library::in_memory().unwrap();
    let service = lib.source_id("example-source", "default").unwrap();
    let playlist = lib.sync_source_playlist(service, "pl-1", "Theirs", "", None).unwrap();
    lib.sync_source_playlist_tracks(service, playlist, &[crate::store::PlaylistTrackMetadata {
        external_id: "x1".into(),
        title: "Streamed".into(),
        artist: "Someone".into(),
        album: "Their Record".into(),
        ..Default::default()
    }]).unwrap();

    assert_eq!(lib.track_count(false).unwrap(), 0, "not in the library");
    assert!(lib.album_ids(crate::AlbumSort::Title).unwrap().is_empty(), "nor its record in the grid");
    assert!(lib.search("Streamed", 10).unwrap().is_empty(), "nor in the library's search");
    assert_eq!(lib.playlist_entries(playlist).unwrap().len(), 1, "its playlist still holds it");
}

#[test]
fn a_record_whose_tracks_cannot_play_is_listed_and_marked_and_its_edition_is_not_chosen() {
    let lib = Library::in_memory().unwrap();
    let local = lib.source_id("local", "default").unwrap();
    let mine = lib.local_edition(local, "One", "Band", "/m/One", Some(2001)).unwrap();
    lib.db.execute(
        "INSERT INTO tracks (source_id, external_id, title, album_id, bit_depth, sample_rate_hz, duration_ms, added_at)
         VALUES (?1, '/m/One/01.flac', 'Song', ?2, 16, 44100, 1000, 0)",
        params![local, mine],
    ).unwrap();
    let server = lib.add_server(&new_server("aaaa", "")).unwrap();
    synced(&lib, server, &[track("1", "One")]);
    let theirs: i64 = lib.db.query_row("SELECT album_id FROM tracks WHERE source_id = ?1", params![server], |r| r.get(0)).unwrap();

    // Both can play: the better edition leads, and I can choose mine.
    assert_eq!(lib.version_catalog().unwrap().selected(mine), theirs, "24/96 over 16/44.1");
    assert!(lib.select_album_version(mine, mine).unwrap());
    assert_eq!(lib.version_catalog().unwrap().selected(theirs), mine, "my choice");

    // My folder goes away: my choice waits, and the one that plays is shown.
    lib.db.execute("UPDATE tracks SET reachable = 0 WHERE source_id = ?1", params![local]).unwrap();
    let catalog = lib.version_catalog().unwrap();
    assert!(!catalog.versions[&mine].reachable);
    assert_eq!(catalog.selected(mine), theirs);
    assert_eq!(lib.unavailable_albums(&[mine, theirs]).unwrap(), HashSet::from([mine]));
    let rows = lib.tracks_by_ids(&[1]).unwrap();
    assert!(!rows[0].available, "its track is dimmed wherever it is listed");
    assert_eq!(lib.track_count(false).unwrap(), 2, "and still listed");

    // The server goes away too: nothing can play, so my choice stands.
    lib.set_source_reachable(server, false).unwrap();
    assert_eq!(lib.version_catalog().unwrap().selected(theirs), mine);
    assert_eq!(lib.unavailable_albums(&[mine, theirs]).unwrap(), HashSet::from([mine, theirs]));
    assert_eq!(lib.unreachable_sources_of(theirs).unwrap(), vec!["Server aaaa".to_string()]);
    assert!(lib.disabled_servers_of(theirs).unwrap().is_empty());
    lib.set_server_enabled(server, false).unwrap();
    assert_eq!(lib.disabled_servers_of(theirs).unwrap(), vec!["Server aaaa".to_string()]);
}

#[test]
fn a_server_edition_is_ranked_by_its_codec_not_by_an_id_without_extension() {
    let lib = Library::in_memory().unwrap();
    let server = lib.add_server(&new_server("aaaa", "")).unwrap();
    let mut lossy = track("x", "Lossy");
    lossy.codec = Some("mp3".into());
    lossy.bit_depth = None;
    lossy.sample_rate_hz = Some(44_100);
    let mut lossless = track("y", "Lossy");
    lossless.album_key = "al-other-edition".into();
    lossless.bit_depth = Some(16);
    lossless.sample_rate_hz = Some(44_100);
    synced(&lib, server, &[lossy, lossless]);
    let flac: i64 = lib.db.query_row("SELECT album_id FROM tracks WHERE external_id = 'y'", [], |r| r.get(0)).unwrap();
    let mp3: i64 = lib.db.query_row("SELECT album_id FROM tracks WHERE external_id = 'x'", [], |r| r.get(0)).unwrap();
    assert_eq!(lib.version_catalog().unwrap().selected(mp3), flac);
}

#[test]
fn a_queue_entry_leads_back_to_its_track_and_says_whether_it_can_play() {
    let lib = Library::in_memory().unwrap();
    let server = lib.add_server(&new_server("aaaa", "")).unwrap();
    let mut one = track("remote:example-server/aaaa/1", "One");
    one.cover = None;
    synced(&lib, server, &[one]);
    let entry = "remote:example-server/aaaa/1".to_string();

    assert_eq!(lib.find_source("example-server", "aaaa").unwrap(), Some(server));
    assert_eq!(lib.find_source("example-server", "zzzz").unwrap(), None);
    let (id, source) = lib.track_by_entry(&entry).unwrap().unwrap();
    assert_eq!(source, server);
    assert_eq!(lib.server_track(server, &entry).unwrap(), Some((Some("flac".into()), Some(format!("r1-{entry}")))));
    let card = lib.server_track_card(&entry).unwrap().unwrap();
    assert_eq!((card.0.as_str(), card.2.as_str()), ("Song remote:example-server/aaaa/1", "One"));

    let entries = vec![entry.clone(), "/not/in/the/library.flac".to_string()];
    assert!(lib.unplayable_entries(&entries).unwrap().is_empty());
    lib.set_source_reachable(server, false).unwrap();
    assert_eq!(lib.unplayable_entries(&entries).unwrap(), HashSet::from([entry]), "what the library does not know is not refused");
    let _ = id;
}

#[test]
fn the_artists_a_server_gives_apart_are_kept_and_go_with_the_server() {
    let lib = Library::in_memory().unwrap();
    let source = lib.add_server(&new_server("aaaa", "server-id-1")).unwrap();
    let joined = ServerTrack { artist: "One, Two".into(), artists: vec!["One".into(), "Two".into()], ..track("t1", "Record") };
    synced(&lib, source, &[joined, track("t2", "Record")]);

    assert_eq!(lib.credit_names("One, Two").unwrap(), ["One", "Two"]);
    let filed: Vec<String> = {
        let mut stmt = lib.connection().prepare("SELECT DISTINCT artist FROM album_members ORDER BY artist").unwrap();
        let rows = stmt.query_map([], |r| r.get(0)).unwrap();
        rows.collect::<rusqlite::Result<_>>().unwrap()
    };
    assert_eq!(filed, ["Band", "One", "Two"], "nobody is filed under the joined credit");

    lib.remove_source(source).unwrap();
    let left: i64 = lib.connection().query_row("SELECT count(*) FROM credit_parts", [], |r| r.get(0)).unwrap();
    assert_eq!(left, 0);
}
