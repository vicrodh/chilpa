use super::*;
use crate::store::{NewServer, ServerTrack};

fn new_server(instance: &str) -> NewServer {
    NewServer {
        provider: "example-server".into(),
        instance: instance.into(),
        label: format!("Server {instance}"),
        icon: "server".into(),
        address: format!("http://{instance}.lan:4533"),
        libraries: "[]".into(),
        details: "{}".into(),
        ..NewServer::default()
    }
}

/// A server whose catalogue holds these tracks, by their entries.
fn synced(lib: &Library, instance: &str, entries: &[&str]) -> i64 {
    let source = lib.add_server(&new_server(instance)).unwrap();
    let generation = lib.begin_server_sync(source).unwrap();
    let tracks: Vec<ServerTrack> = entries
        .iter()
        .map(|e| ServerTrack {
            external_id: e.to_string(),
            title: format!("Song {e}"),
            artist: "Band".into(),
            album: "One".into(),
            album_artist: "Band".into(),
            duration_ms: 1_000,
            ..ServerTrack::default()
        })
        .collect();
    lib.save_server_tracks(source, generation, &tracks).unwrap();
    lib.finish_server_sync(source, generation).unwrap();
    source
}

fn listed(id: &str, name: &str, writable: bool, entries: &[&str]) -> ServerPlaylist {
    ServerPlaylist { external_id: id.into(), name: name.into(), description: String::new(), writable, entries: entries.iter().map(|e| e.to_string()).collect() }
}

#[test]
fn a_server_s_playlist_keeps_its_order_and_repeats_and_counts_what_the_library_lacks() {
    let lib = Library::in_memory().unwrap();
    let source = synced(&lib, "aaaa", &["1", "2", "3"]);
    let mirrored = lib.mirror_server_playlist(source, &listed("pl-1", "Road", true, &["3", "1", "9", "3"])).unwrap();
    assert_eq!(mirrored.left_out, 1, "track 9 is not in the library");
    assert_eq!((mirrored.kept.as_slice(), mirrored.changed), (&[0, 1, 3][..], true), "rows are the entries at 0, 1 and 3; new rows are a change");
    let stored = lib.playlists().unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!((stored[0].id, stored[0].server, stored[0].writable, stored[0].left_out), (mirrored.id, true, true, 1), "the server's, changeable there, one left out");
    assert_eq!((stored[0].external_id.as_str(), stored[0].name.as_str(), stored[0].track_count), ("pl-1", "Road", 3));
    let order: Vec<String> = lib.playlist_entries(mirrored.id).unwrap().into_iter().map(|e| e.external_id).collect();
    assert_eq!(order, ["3", "1", "3"], "the server's order, the repeat kept");
    assert!(lib.playlist_entries(mirrored.id).unwrap().iter().all(|e| e.track_id.is_some() && e.source_id == Some(source)));
    assert_eq!(lib.server_playlists_left_out(source).unwrap(), 1);

    let local = lib.create_playlist(lib.source_id("local", "default").unwrap(), "Mine", "").unwrap();
    let mine = lib.playlists().unwrap().into_iter().find(|p| p.id == local).unwrap();
    assert!(!mine.server && mine.left_out == 0, "one of ours is not a server's");
}

#[test]
fn reading_the_same_playlist_again_writes_nothing_and_keeps_what_was_done_here() {
    let lib = Library::in_memory().unwrap();
    let source = synced(&lib, "aaaa", &["1", "2"]);
    let first = listed("pl-1", "Road", true, &["1", "2"]);
    let id = lib.mirror_server_playlist(source, &first).unwrap().id;
    let folder = lib.create_playlist_folder("Servers", "accent").unwrap();
    lib.move_playlist_to_folder(id, Some(folder)).unwrap();
    lib.record_playlist_play(id).unwrap();
    lib.db.execute("UPDATE playlists SET updated_at = 5 WHERE id = ?1", [id]).unwrap();

    let again = lib.mirror_server_playlist(source, &first).unwrap();
    assert_eq!((again.id, again.changed), (id, false), "the same rows are no change");
    let again = &lib.playlists().unwrap()[0];
    assert_eq!((again.folder_id, again.play_count, again.updated_at), (Some(folder), 1, 5), "nothing was written");

    let renamed = ServerPlaylist { name: "Road trip".into(), description: "For the drive".into(), writable: false, entries: vec!["2".into()], ..first };
    lib.mirror_server_playlist(source, &renamed).unwrap();
    let changed = &lib.playlists().unwrap()[0];
    assert_eq!((changed.id, changed.name.as_str(), changed.description.as_str(), changed.track_count), (id, "Road trip", "For the drive", 1));
    assert_eq!(changed.folder_id, Some(folder), "its folder here stays");
    assert!(!changed.writable, "the server took it from the account");
    assert!(changed.updated_at > 5);
}

#[test]
fn a_playlist_the_server_no_longer_lists_leaves_and_nothing_else_does() {
    let lib = Library::in_memory().unwrap();
    let source = synced(&lib, "aaaa", &["1"]);
    let other = lib.add_server(&new_server("bbbb")).unwrap();
    lib.mirror_server_playlist(source, &listed("pl-1", "Kept", true, &["1"])).unwrap();
    lib.mirror_server_playlist(source, &listed("pl-2", "Deleted there", true, &["1"])).unwrap();
    lib.mirror_server_playlist(other, &listed("pl-2", "Another server's", true, &[])).unwrap();
    lib.create_playlist(lib.source_id("local", "default").unwrap(), "Mine", "").unwrap();
    assert_eq!(lib.removal(source).unwrap().playlists, 2);

    assert_eq!(lib.prune_server_playlists(source, &["pl-1".to_string()]).unwrap(), 1);
    let mut names: Vec<String> = lib.playlists().unwrap().into_iter().map(|p| p.name).collect();
    names.sort();
    assert_eq!(names, ["Another server's", "Kept", "Mine"]);
    assert_eq!(lib.track_count(false).unwrap(), 1, "a playlist is a view: its tracks stay");

    lib.remove_source(source).unwrap();
    assert!(lib.playlists().unwrap().iter().all(|p| p.name != "Kept"), "removing the server takes its playlists");
}
