//! Changes asked of a Jellyfin server's playlist against synthetic answers
//! shaped as 10.10.7's (204 for a change, the items read back), into a
//! library in memory. The server's own list is simulated where the order
//! matters: a move takes the entry and puts it at its new index.

use super::*;
use chilpa_servers::{Reply, Request};
use std::collections::VecDeque;

const USER: &str = "501512e000854c8d832d2f1d2a26a0a0";
const ROAD: &str = "b3a91c5e0d7f4e2a9c6b1f8e2d4a7c30";
const FIRST_SONG_1: &str = "d29818a3701ef1b2d7fb8c90e2c0f2b2";
const COMP_TRACK_1: &str = "5d0a04cb4d421afbbd017f0be4168548";
const COMP_TRACK_2: &str = "7a12ca93c59e64ce0a53286039943fd7";
const ELSEWHERE: &str = "ab93228537aa8104f4f440f2d4bdc42e";
const VIDEO: &str = "9e1f2a3b4c5d4e6f8a7b6c5d4e3f2a1b";

/// Answers with what it was loaded with, in order, and writes down what it
/// was asked: the method and the address.
#[derive(Default)]
struct Answers {
    replies: Mutex<VecDeque<Result<(u16, String), String>>>,
    asked: Mutex<Vec<(String, String)>>,
}

impl Answers {
    fn with(self, status: u16, body: &str) -> Self {
        self.replies.lock().unwrap().push_back(Ok((status, body.to_string())));
        self
    }
    fn silence(self) -> Self {
        self.replies.lock().unwrap().push_back(Err("the server could not be reached".into()));
        self
    }
    fn asked(&self) -> Vec<(String, String)> {
        self.asked.lock().unwrap().clone()
    }
}

impl Net for Answers {
    fn send(&self, request: &Request) -> Result<Reply, String> {
        self.asked.lock().unwrap().push((request.method.to_string(), request.url.clone()));
        let (status, body) = self.replies.lock().unwrap().pop_front().unwrap_or_else(|| Err("nothing was loaded".into()))?;
        Ok(Reply { status, headers: Default::default(), body: body.into_bytes() })
    }
}

/// A playlist's items as 10.10.7 gives them: each entry known by its item.
fn items(entries: &[&str]) -> String {
    let items: Vec<serde_json::Value> = entries.iter().map(|id| serde_json::json!({"Id": id, "PlaylistItemId": id, "Type": if *id == VIDEO { "MusicVideo" } else { "Audio" }})).collect();
    serde_json::json!({"Items": items, "TotalRecordCount": entries.len(), "StartIndex": 0}).to_string()
}

struct Lab {
    lib: chilpa_library::Library,
    keys: Keys,
    stored: chilpa_library::StoredServer,
    root: std::path::PathBuf,
}

impl Drop for Lab {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

impl Lab {
    /// A Jellyfin server whose catalogue in the library holds three tracks,
    /// and a playlist of it, "Road Trip", that holds `road`.
    fn new(road: &[&str]) -> (Lab, i64) {
        let lib = chilpa_library::Library::in_memory().unwrap();
        let instance = super::super::keys::mint();
        let root = std::env::temp_dir().join(format!("chilpa-writes-{instance}"));
        let keys = Keys::derived(root.clone());
        assert!(keys.put(&instance, &chilpa_servers::Credential::Token { token: "t".into() }));
        let new = chilpa_library::NewServer {
            provider: "jellyfin".into(),
            instance,
            label: "Living room".into(),
            address: "http://jf.lan:8096".into(),
            libraries: "[]".into(),
            details: format!(r#"{{"user_id":"{USER}"}}"#),
            ..Default::default()
        };
        let source = lib.add_server(&new).unwrap();
        let stored = lib.server(source).unwrap().unwrap();
        let generation = lib.begin_server_sync(source).unwrap();
        let tracks: Vec<chilpa_library::ServerTrack> = [FIRST_SONG_1, COMP_TRACK_1, COMP_TRACK_2]
            .iter()
            .map(|id| chilpa_library::ServerTrack {
                external_id: net::entry(&stored, id),
                title: id[..4].to_string(),
                artist: "Band".into(),
                album: "One".into(),
                duration_ms: 1_000,
                ..Default::default()
            })
            .collect();
        lib.save_server_tracks(source, generation, &tracks).unwrap();
        lib.finish_server_sync(source, generation).unwrap();
        let lab = Lab { lib, keys, stored, root };
        let read =
            chilpa_servers::playlist_tracks(&Answers::default().with(200, &items(road)), &net::server(&lab.stored).unwrap(), &chilpa_servers::Credential::Token { token: "t".into() }, ROAD).unwrap();
        let playlist = Playlist { id: ROAD.into(), name: "Road Trip".into(), description: String::new(), editable: Some(true) };
        let id = mirror(&lab.lib, &lab.stored, &playlist, true, &read).unwrap().id;
        (lab, id)
    }

    fn apply(&self, net: &Answers, playlist: i64, change: Change) -> Written {
        apply(&self.lib, net, &self.keys, &Mutex::new(()), &Write { source: self.stored.source_id, playlist, request: "r1".into(), change })
    }

    fn entry(&self, id: &str) -> String {
        net::entry(&self.stored, id)
    }

    /// The tracks of a playlist in the library, by their ids on the server.
    fn holds(&self, playlist: i64) -> Vec<String> {
        self.lib.playlist_entries(playlist).unwrap().into_iter().map(|e| chilpa_models::RemoteEntry::parse(&e.external_id).unwrap().external_id).collect()
    }
}

#[test]
fn a_track_added_goes_to_the_server_and_the_playlist_is_what_the_server_reads_back() {
    let (lab, road) = Lab::new(&[FIRST_SONG_1, COMP_TRACK_2]);
    let net = Answers::default().with(204, "").with(200, &items(&[FIRST_SONG_1, COMP_TRACK_2, COMP_TRACK_1]));
    let written = lab.apply(&net, road, Change::Add { tracks: vec![lab.entry(COMP_TRACK_1)] });
    assert_eq!((written.why.as_str(), written.playlist, written.request.as_str(), written.missing), ("", road, "r1", false));
    assert_eq!(lab.holds(road), [FIRST_SONG_1, COMP_TRACK_2, COMP_TRACK_1]);
    let asked = net.asked();
    assert_eq!(asked[0], ("POST".to_string(), format!("http://jf.lan:8096/Playlists/{ROAD}/Items?ids={COMP_TRACK_1}&userId={USER}")));
    assert!(asked[1].1.starts_with(&format!("http://jf.lan:8096/Playlists/{ROAD}/Items?")), "read back: {:?}", asked[1]);
}

#[test]
fn a_change_by_row_goes_out_only_while_the_server_has_the_rows_shown() {
    let (lab, road) = Lab::new(&[FIRST_SONG_1, COMP_TRACK_2, COMP_TRACK_1]);
    // Somebody took Comp Track 2 out on the server meanwhile.
    let net = Answers::default().with(200, &items(&[FIRST_SONG_1, COMP_TRACK_1]));
    let written = lab.apply(&net, road, Change::Remove { rows: vec![2] });
    assert!(written.why.contains("changed on Living room"), "{}", written.why);
    assert_eq!(net.asked().len(), 1, "it was read, and nothing was sent");
    assert_eq!(lab.holds(road), [FIRST_SONG_1, COMP_TRACK_1], "the library shows what the server has now");

    let net = Answers::default().with(200, &items(&[FIRST_SONG_1, COMP_TRACK_1])).with(204, "").with(200, &items(&[FIRST_SONG_1]));
    assert_eq!(lab.apply(&net, road, Change::Remove { rows: vec![1] }).why, "");
    assert_eq!(net.asked()[1], ("DELETE".to_string(), format!("http://jf.lan:8096/Playlists/{ROAD}/Items?entryIds={COMP_TRACK_1}")));
    assert_eq!(lab.holds(road), [FIRST_SONG_1]);
}

#[test]
fn rows_move_around_what_the_library_does_not_show() {
    // The server holds a track of a library that was not chosen and a
    // video; the library shows the other three.
    let server = [FIRST_SONG_1, ELSEWHERE, COMP_TRACK_2, VIDEO, COMP_TRACK_1];
    let (lab, road) = Lab::new(&server);
    assert_eq!(lab.holds(road), [FIRST_SONG_1, COMP_TRACK_2, COMP_TRACK_1]);
    // Comp Track 1 to the top: the library's rows in their new order.
    let wanted = [COMP_TRACK_1, ELSEWHERE, FIRST_SONG_1, VIDEO, COMP_TRACK_2];
    // Three moves: two entries of the five stay where they are.
    let net = Answers::default().with(200, &items(&server)).with(204, "").with(204, "").with(204, "").with(200, &items(&wanted));
    let written = lab.apply(&net, road, Change::Reorder { rows: vec![2, 0, 1] });
    assert_eq!(written.why, "");
    let moves: Vec<(String, String)> = net.asked().into_iter().filter(|(_, url)| url.contains("/Move/")).collect();
    assert_eq!(moves.len(), 3);
    // The moves, done on the server's list as the server does them.
    let mut list: Vec<String> = server.iter().map(|s| s.to_string()).collect();
    for (method, url) in &moves {
        assert_eq!(method, "POST");
        let tail = url.split(&format!("/Playlists/{ROAD}/Items/")).nth(1).unwrap();
        let (entry, to) = tail.split_once("/Move/").unwrap();
        let from = list.iter().position(|e| e == entry).unwrap();
        let moved = list.remove(from);
        list.insert(to.parse().unwrap(), moved);
    }
    assert_eq!(list, wanted, "what the library does not show keeps its place");
    assert_eq!(lab.holds(road), [COMP_TRACK_1, FIRST_SONG_1, COMP_TRACK_2]);
}

#[test]
fn a_playlist_the_server_no_longer_has_leaves_here_too() {
    let (lab, road) = Lab::new(&[FIRST_SONG_1]);
    let net = Answers::default().with(404, "Playlist not found");
    let written = lab.apply(&net, road, Change::Edit { name: Some("Night".into()), description: None });
    assert!(written.why.contains("no longer has this playlist"), "{}", written.why);
    assert_eq!(written.playlist, 0);
    assert!(lab.lib.playlists().unwrap().is_empty(), "gone there, gone here");
}

#[test]
fn a_change_the_account_may_not_make_is_said_and_the_playlist_is_read_only_after() {
    let (lab, road) = Lab::new(&[FIRST_SONG_1]);
    let net = Answers::default().with(403, "").with(200, &items(&[FIRST_SONG_1]));
    let written = lab.apply(&net, road, Change::Edit { name: Some("Night".into()), description: None });
    assert!(written.why.starts_with("Living room did not take the change"), "{}", written.why);
    let kept = lab.lib.playlists().unwrap().into_iter().find(|p| p.id == road).unwrap();
    assert_eq!((kept.name.as_str(), kept.writable), ("Road Trip", false), "its name as the server has it, and no longer changeable here");
}

#[test]
fn deleting_takes_the_playlist_out_there_and_here_and_creating_reads_the_new_one_in() {
    let (lab, road) = Lab::new(&[FIRST_SONG_1]);
    let net = Answers::default().with(204, "");
    let written = lab.apply(&net, road, Change::Delete);
    assert_eq!((written.why.as_str(), written.playlist), ("", 0));
    assert_eq!(net.asked()[0], ("DELETE".to_string(), format!("http://jf.lan:8096/Items/{ROAD}")));
    assert!(lab.lib.playlists().unwrap().is_empty());

    let made = "0a1b2c3d4e5f60718293a4b5c6d7e8f9";
    let net = Answers::default().with(200, &format!(r#"{{"Id":"{made}"}}"#)).with(200, &items(&[COMP_TRACK_2, FIRST_SONG_1]));
    let written = lab.apply(&net, 0, Change::Create { name: "Night drive".into(), tracks: vec![lab.entry(COMP_TRACK_2), lab.entry(FIRST_SONG_1)] });
    assert_eq!(written.why, "");
    let created = lab.lib.playlists().unwrap().into_iter().find(|p| p.id == written.playlist).unwrap();
    assert_eq!((created.name.as_str(), created.external_id.as_str(), created.writable, created.server), ("Night drive", made, true, true));
    assert_eq!(lab.holds(created.id), [COMP_TRACK_2, FIRST_SONG_1]);
}

#[test]
fn a_playlist_that_names_a_track_the_library_lacks_asks_for_a_sync_after_a_change() {
    let (lab, road) = Lab::new(&[FIRST_SONG_1]);
    // Read back, it holds a track of a library that was not chosen.
    let net = Answers::default().with(204, "").with(200, &items(&[FIRST_SONG_1, ELSEWHERE, COMP_TRACK_1]));
    let written = lab.apply(&net, road, Change::Add { tracks: vec![lab.entry(COMP_TRACK_1)] });
    assert_eq!((written.why.as_str(), written.missing), ("", true));
    assert_eq!(lab.holds(road), [FIRST_SONG_1, COMP_TRACK_1]);
    let net = Answers::default().with(204, "").with(200, &items(&[FIRST_SONG_1, COMP_TRACK_1, COMP_TRACK_2]));
    assert!(!lab.apply(&net, road, Change::Add { tracks: vec![lab.entry(COMP_TRACK_2)] }).missing, "all of it in the library: nothing to ask for");
}

#[test]
fn a_server_that_does_not_answer_changes_nothing_and_a_foreign_track_goes_nowhere() {
    let (lab, road) = Lab::new(&[FIRST_SONG_1]);
    let net = Answers::default().silence();
    let written = lab.apply(&net, road, Change::Add { tracks: vec![lab.entry(COMP_TRACK_1)] });
    assert!(written.why.contains("could not be reached"), "{}", written.why);
    assert_eq!(net.asked().len(), 1, "nothing read back from a server that is not there");
    assert_eq!(lab.holds(road), [FIRST_SONG_1]);

    let net = Answers::default();
    let foreign = chilpa_models::RemoteEntry::new(chilpa_models::SourceId::new("jellyfin", "another"), COMP_TRACK_1).to_entry();
    let written = lab.apply(&net, road, Change::Add { tracks: vec![foreign] });
    assert!(!written.why.is_empty() && net.asked().is_empty(), "another server's track is not sent: {}", written.why);
}
