//! A server's playlist as the window shows and changes it, in a library in
//! memory. No thread is started: a change that is sent stops at the door
//! of the writer that is not there, which is enough to see that nothing
//! was written here.

use super::*;

const ENTRY: &str = "remote:jellyfin/4f1c9a0b2d3e4f50/t1";
const OTHER: &str = "remote:jellyfin/0a0b0c0d0e0f1011/t9";

fn server(lib: &chilpa_library::Library, instance: &str, label: &str, entry: &str) -> (i64, i64) {
    let source = lib
        .add_server(&chilpa_library::NewServer {
            provider: "jellyfin".into(),
            instance: instance.into(),
            label: label.into(),
            icon: "jellyfin".into(),
            address: "http://jf.lan:8096".into(),
            product: "Jellyfin 10.10.7".into(),
            libraries: "[]".into(),
            details: "{}".into(),
            ..Default::default()
        })
        .unwrap();
    let generation = lib.begin_server_sync(source).unwrap();
    let track = chilpa_library::ServerTrack {
        external_id: entry.into(),
        title: "Song".into(),
        artist: "Band".into(),
        album: "Record".into(),
        album_artist: "Band".into(),
        duration_ms: 60_000,
        ..Default::default()
    };
    lib.save_server_tracks(source, generation, &[track]).unwrap();
    lib.finish_server_sync(source, generation).unwrap();
    lib.set_source_reachable(source, true).unwrap();
    let track: i64 = lib.connection().query_row("SELECT id FROM tracks WHERE external_id = ?1", [entry], |r| r.get(0)).unwrap();
    (source, track)
}

/// A server with one track and one playlist of its own that holds it, and
/// another server with a track of its own: the facade, the playlist's id,
/// its source key, its track and the other server's track.
fn with_a_server_playlist() -> (Chilpa, i64, String, i64, i64) {
    let lib = chilpa_library::Library::in_memory().unwrap();
    let (source, track) = server(&lib, "4f1c9a0b2d3e4f50", "Living room", ENTRY);
    let (_, other) = server(&lib, "0a0b0c0d0e0f1011", "Attic", OTHER);
    let road = chilpa_library::ServerPlaylist { external_id: "pl-1".into(), name: "Road Trip".into(), description: String::new(), writable: true, entries: vec![ENTRY.into()] };
    let id = lib.mirror_server_playlist(source, &road).unwrap().id;
    let mut api = Chilpa::new();
    api.library = Some(lib);
    (api, id, source_key("jellyfin", "4f1c9a0b2d3e4f50"), track, other)
}

#[test]
fn a_server_s_playlist_is_listed_under_the_server_and_says_what_it_is() {
    let (mut api, id, key, _, _) = with_a_server_playlist();
    let playlist = api.cached_playlists().into_iter().find(|p| p.id == id).unwrap();
    assert_eq!((playlist.source.as_str(), playlist.provider.as_str()), (key.as_str(), "Living room"), "the server's source and its name");
    assert!(playlist.writable && !playlist.mixed, "changed only through the server");
    let page: serde_json::Value = serde_json::from_str(&api.server_playlist_json(id)).unwrap();
    assert_eq!((page["server"].as_str(), page["icon"].as_str(), page["writable"].as_bool(), page["pending"].as_bool()), (Some("Living room"), Some("jellyfin"), Some(true), Some(false)));
    assert_eq!(page["describable"].as_bool(), Some(false), "this kind of server takes a name and no description");
    let mine = api.create_playlist("Mine").unwrap();
    assert_eq!(api.server_playlist_json(mine), "{}", "one of ours is no server's");
    assert!(api.managed_playlist_sources().contains(&key), "the manager filters by it");
    let servers: serde_json::Value = serde_json::from_str(&api.playlist_servers_json()).unwrap();
    assert_eq!((servers[key.as_str()]["name"].as_str(), servers[key.as_str()]["icon"].as_str()), (Some("Living room"), Some("jellyfin")));
}

#[test]
fn a_server_s_playlist_opens_without_a_plugin() {
    let (mut api, id, key, _, _) = with_a_server_playlist();
    let playlist = api.playlist(id).unwrap();
    assert_eq!(api.open_source_playlist(&playlist.source, &playlist.external_id), 0);
    assert!(api.now_playing().error.is_empty(), "no plugin was asked: {}", api.now_playing().error);
    assert!(api.is_server_source(&key) && !api.is_server_source("") && !api.is_server_source("app.example.music"));
}

#[test]
fn an_edit_of_a_server_s_playlist_goes_to_the_server_and_nothing_changes_here_before_it_answers() {
    let (mut api, id, _, track, _) = with_a_server_playlist();
    // No writer runs here: what reaches the server's door is refused there.
    assert!(api.add_track_ids_to_playlist(id, &[track]).rejected || api.playlist(id).unwrap().track_count == 1);
    assert!(!api.remove_from_playlist(id, 0));
    assert!(!api.move_in_playlist(id, 0, 0));
    assert_eq!(api.remove_tracks_from_playlist(id, &[track]), 0);
    api.rename_playlist(id, "Night");
    api.delete_playlist(id);
    let kept = api.playlist(id).expect("not deleted here before the server says so");
    assert_eq!((kept.name.as_str(), kept.track_count), ("Road Trip", 1), "it holds what the server says");
    assert!(!api.now_playing().error.is_empty(), "the window is told why");
}

#[test]
fn the_edit_dialog_sends_name_and_description_to_the_server_and_files_it_here() {
    let (mut api, id, _, _, _) = with_a_server_playlist();
    let folder = api.create_playlist_folder("Servers", "accent").unwrap();
    // Only the folder changed: it is the library's own, and nothing goes out.
    assert!(api.update_playlist(id, "Road Trip", "", None, &[], Some(folder)));
    assert_eq!(api.playlist(id).unwrap().folder_id, Some(folder));
    assert!(api.now_playing().error.is_empty(), "{}", api.now_playing().error);
    // A new name goes to the server's door, which is closed here: nothing
    // changes in the library before the server answers.
    assert!(!api.update_playlist(id, "Night drive", "For the night", None, &[], Some(folder)));
    let kept = api.playlist(id).unwrap();
    assert_eq!((kept.name.as_str(), kept.description.as_str()), ("Road Trip", ""));
    assert!(!api.now_playing().error.is_empty(), "the window is told why");
}

#[test]
fn a_server_s_playlist_takes_only_that_server_s_tracks() {
    let (mut api, id, _, track, other) = with_a_server_playlist();
    let targets = api.playlist_targets(&[other], "Road");
    assert!(targets.iter().all(|t| t.playlist.id != id || !t.accepts), "another server's track does not go in it");
    // With the network off, no server's playlist takes tracks at all.
    let online = !api.offline();
    let targets = api.playlist_targets(&[track], "Road");
    let road = targets.iter().find(|t| t.playlist.id == id);
    assert!(road.is_some() || !online, "online, it takes its own server's track");
    if let Some(road) = road {
        assert!(road.accepts && road.same_source, "its own server's track belongs in it");
    }
    let sources: Vec<serde_json::Value> = api.server_playlist_sources(&[track]);
    let accepts: Vec<(String, bool)> = sources.iter().map(|s| (s["name"].as_str().unwrap().to_string(), s["accepts"].as_bool().unwrap())).collect();
    assert!(!accepts.iter().any(|(name, accepts)| name == "Attic" && *accepts), "a new playlist on another server does not take it: {accepts:?}");
}

#[test]
fn a_server_that_makes_no_empty_playlist_is_not_offered_for_one() {
    let (mut api, _, _, _, _) = with_a_server_playlist();
    let lib = api.library().unwrap();
    let new = chilpa_library::NewServer {
        provider: "plex".into(),
        instance: "77aa0b1c2d3e4f50".into(),
        label: "MacMini Plex".into(),
        identity: "f00dfeed".into(),
        details: "{}".into(),
        ..Default::default()
    };
    let plex = lib.add_server(&new).unwrap();
    lib.set_source_reachable(plex, true).unwrap();
    let key = source_key("plex", "77aa0b1c2d3e4f50");
    let sources = api.server_playlist_sources(&[]);
    let row = sources.iter().find(|s| s["id"] == key.as_str()).unwrap();
    assert_eq!((row["accepts"].as_bool(), row["why"].as_str()), (Some(false), Some(needs_tracks().as_str())), "greyed, with the reason");
    let jellyfin = sources.iter().find(|s| s["name"] == "Living room").unwrap();
    assert!(jellyfin["why"].is_null(), "another kind makes an empty one");
    // Asked anyway, nothing goes to its door, and the window is told why.
    assert_eq!(api.create_server_playlist(&key, "Iron Maiden 80s", "The Iron Maiden records of the eighties", &[], "r1"), Some(false));
    let told = chilpa_i18n::t_args("{} makes a playlist only with tracks in it: select them, then Add to playlist.", &["MacMini Plex"]);
    assert_eq!(api.now_playing().error, told);
}

#[test]
fn a_server_in_its_first_sync_takes_no_change_of_its_playlists_until_it_ends() {
    let (mut api, id, key, track, _) = with_a_server_playlist();
    let source = api.playlist(id).unwrap().source_id;
    // Its sync is running, as Settings shows it, and none has finished yet.
    api.library().unwrap().connection().execute("UPDATE servers SET synced_at = NULL WHERE source_id = ?1", [source]).unwrap();
    let keys = std::sync::Arc::new(super::super::keys::Keys::derived(std::env::temp_dir().join("chilpa-first-sync-keys")));
    let mut syncer = super::super::sync::Syncer::start(keys, std::sync::Arc::new(std::sync::Mutex::new(())));
    syncer.running.insert(source, 250);
    api.servers.syncer = Some(syncer);
    let told = chilpa_i18n::t_args("{} is still syncing its library for the first time. Try again when it finishes.", &["Living room"]);
    api.rename_playlist(id, "Night");
    assert_eq!(api.now_playing().error, told, "a rename is not sent");
    api.now.error.clear();
    assert!(api.add_to_server_playlist(id, &[track], true).is_some_and(|added| added.rejected));
    assert_eq!(api.now_playing().error, told, "an add is not sent");
    assert_eq!(api.create_server_playlist(&key, "Night drive", "", &[track], "r1"), Some(false));
    assert_eq!(api.now_playing().error, told, "a new one is not made");
    // Nothing offers it meanwhile, and its page says why.
    let sources = api.server_playlist_sources(&[track]);
    let row = sources.iter().find(|s| s["id"] == key.as_str()).unwrap();
    assert_eq!((row["accepts"].as_bool(), row["why"].as_str()), (Some(false), Some(chilpa_i18n::t("Still syncing its library for the first time").as_str())));
    assert!(api.playlist_targets(&[track], "").iter().all(|t| t.playlist.id != id), "not in the picker");
    let page: serde_json::Value = serde_json::from_str(&api.server_playlist_json(id)).unwrap();
    assert_eq!(page["held"].as_str(), Some(told.as_str()));
    // Once that sync ends, a change goes to the server's door again.
    api.servers.syncer.as_mut().unwrap().running.remove(&source);
    api.rename_playlist(id, "Night");
    assert_ne!(api.now_playing().error, told);
    let page: serde_json::Value = serde_json::from_str(&api.server_playlist_json(id)).unwrap();
    assert_eq!(page["held"].as_str(), Some(""));
}

#[test]
fn a_disabled_server_s_playlists_are_hidden_and_kept() {
    let (mut api, id, _, track, _) = with_a_server_playlist();
    let source = api.playlist(id).unwrap().source_id;
    assert!(api.library().unwrap().set_server_enabled(source, false).unwrap());
    assert!(api.cached_playlists().iter().all(|p| p.id != id), "not in the lists");
    assert!(api.managed_playlists("", "all", "", "name", true, None).iter().all(|row| row.id != id), "not in the manager");
    assert!(api.playlist_targets(&[track], "").iter().all(|t| t.playlist.id != id), "not in the picker");
    assert!(api.library().unwrap().playlists().unwrap().iter().any(|p| p.id == id), "and still in the library");
    assert!(api.library().unwrap().set_server_enabled(source, true).unwrap());
    assert!(api.cached_playlists().iter().any(|p| p.id == id), "back with its server");
}

#[test]
fn removing_the_server_says_its_playlists_leave_with_it() {
    let (mut api, _, _, _, _) = with_a_server_playlist();
    let server = api.servers()[0].id;
    let said = api.server_removal(server);
    assert!(said.contains("1 playlist"), "{said}");
}

#[test]
fn a_move_and_a_dropped_group_are_one_new_order() {
    assert_eq!(moved(4, 3, 0), Some(vec![3, 0, 1, 2]));
    assert_eq!(moved(4, 1, 1), None, "nowhere to go");
    assert_eq!(moved(4, 4, 0), None);
    assert_eq!(moved_group(5, &[1, 4], 2), Some(vec![0, 2, 1, 4, 3]), "one from above and one from below meet at the target");
    assert_eq!(moved_group(4, &[2, 3], 2), None, "dropped on itself, nothing moves");
}
