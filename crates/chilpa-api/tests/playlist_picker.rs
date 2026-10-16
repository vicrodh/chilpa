//! The picker contract against a real, isolated database.
use chilpa_api::Chilpa;
use chilpa_library::Library;
use rusqlite::params;

#[test]
fn search_membership_add_edit_and_reopen_preserve_the_selected_tracks() {
    let dir = std::env::temp_dir().join(format!("chilpa-playlist-picker-{}", std::process::id()));
    std::env::set_var("XDG_DATA_HOME", &dir);
    std::env::set_var("XDG_CONFIG_HOME", dir.join("config"));
    let lib = Library::open(&Library::default_path()).unwrap();
    let local = lib.source_id("local", "default").unwrap();
    let remote = lib.source_id("test-source", "home").unwrap();
    lib.connection().execute("INSERT INTO albums(id, title, album_artist) VALUES (1, 'Album', 'Artist')", []).unwrap();
    for (id, source, title) in [(1, local, "Song"), (2, local, "Song (Remastered)"), (3, local, "Different song"), (4, remote, "Remote song")] {
        lib.connection().execute("INSERT INTO tracks(id, source_id, external_id, title, artist, album_id, duration_ms, added_at)
            VALUES (?1, ?2, ?3, ?4, 'Artist', 1, 1000, 1)", params![id, source, format!("/track-{id}.flac"), title]).unwrap();
    }
    let read_only = lib.create_playlist(remote, "Source playlist", "remote-id").unwrap();
    let mut api = Chilpa::new();
    assert!(api.create_playlist_with("  ", "", None, &[], None).is_none());
    let folder = api.create_playlist_folder("Music", "").unwrap();
    let playlist = api.create_playlist_with("Evening music", "A description", Some("cover.png"), &["night".into()], Some(folder)).unwrap();
    assert!(api.playlist_targets(&[1], "unmatched").is_empty());
    let targets = api.playlist_targets(&[1], " MUSIC evening ");
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].playlist.id, playlist);
    assert!(targets[0].accepts);
    assert_eq!((targets[0].exact, targets[0].similar), (0, 0));

    assert_eq!(api.add_track_ids_to_playlist(playlist, &[1]).added, 1);
    let targets = api.playlist_targets(&[1, 2, 3], "evening");
    assert_eq!((targets[0].exact, targets[0].similar), (1, 1));
    let added = api.add_track_ids_to_playlist(playlist, &[1, 2, 3]);
    assert_eq!((added.added, added.exact, added.similar, added.rejected), (2, 1, 1, false));
    assert_eq!(api.add_track_ids_to_playlist(playlist, &[1, 2, 3]).added, 0);
    assert_eq!(api.playlist(playlist).unwrap().track_count, 3);
    assert!(api.add_track_ids_to_playlist(playlist, &[4, 999]).rejected, "a stale selection is not silently half-added");
    assert!(api.add_track_ids_to_playlist(read_only, &[1]).rejected);
    assert!(api.add_track_ids_to_playlist(read_only, &[4]).rejected, "source writes need a declared capability");
    assert!(api.add_track_ids_to_playlist(999, &[1]).rejected);
    assert_eq!(api.playlist(playlist).unwrap().track_count, 3);
    let ours = api.playlist_targets(&[1], "evening");
    assert_eq!((ours[0].same_source, ours[0].icon.as_str()), (true, "chilpa"), "the library's tracks belong in ours");
    assert!(!api.playlist_targets(&[4], "evening")[0].same_source, "a source's track does not");
    assert!(api.playlist_targets(&[4], "source")[0].same_source, "it belongs in its source's own list");
    assert!(api.playlist_targets(&[1, 4], "evening")[0].same_source, "a mix of sources belongs in ours");
    assert_eq!(api.add_track_ids_again(playlist, &[1]).added, 1, "asked for, a repeat goes in");
    assert_eq!(api.playlist(playlist).unwrap().track_count, 4);
    assert!(api.add_track_ids_to_playlist(playlist, &[4]).added == 1, "local playlists accept mixed sources");
    assert!(api.update_playlist(playlist, "New name", "New description", Some("new.png"), &["quiet".into()], Some(folder)));
    drop(api);
    let mut reopened = Chilpa::new();
    let saved = reopened.playlist(playlist).unwrap();
    assert_eq!(saved.name, "New name");
    assert_eq!(saved.description, "New description");
    assert_eq!(saved.artwork_path, "new.png");
    assert_eq!(saved.folder_id, Some(folder));
    assert_eq!(saved.tags, ["quiet"]);
    assert_eq!(lib.playlist_entries(playlist).unwrap().iter().map(|entry| entry.track_id.unwrap()).collect::<Vec<_>>(), [1, 2, 3, 1, 4]);
    assert!(!reopened.playlist_targets(&[1], "source")[0].accepts);
    drop(reopened);
    drop(lib);
    std::fs::remove_dir_all(dir).unwrap();
}
