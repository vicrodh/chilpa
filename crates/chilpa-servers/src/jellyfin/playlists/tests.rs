//! The playlist answers are synthetic: shaped as 10.10.7 answers them
//! (`PlaylistsController.GetPlaylistItems`, where `PlaylistItemId` is the
//! linked item's id), with the lab library's own track ids. None was saved
//! from a server yet. The writes are answered as 10.10.7 answers them:
//! 204 with nothing, and 200 with the new id on creating.

use super::*;
use crate::model::Client;
use crate::net::testing::Canned;
use crate::playlist::Entry;
use serde_json::json;

const AT: &str = "http://jf.lan:8096";
const USER: &str = "501512e000854c8d832d2f1d2a26a0a0";
const ROAD: &str = "b3a91c5e0d7f4e2a9c6b1f8e2d4a7c30";
const FIRST_SONG_1: &str = "d29818a3701ef1b2d7fb8c90e2c0f2b2";
const COMP_TRACK_1: &str = "5d0a04cb4d421afbbd017f0be4168548";
const COMP_TRACK_2: &str = "7a12ca93c59e64ce0a53286039943fd7";
const HI_RES_D2_T1: &str = "3499eeafb8280bb73ce1967d4c581621";

fn lab() -> (Server, Credential) {
    let client = Client { version: "0.1.0".into(), platform: "Linux".into(), device: "my desk".into(), id: "4f1c9a0b2d3e4f50".into() };
    (crate::jellyfin::server(AT, client, Vec::new(), USER), Credential::Token { token: "tok en".into() })
}

fn header(request: &Request) -> String {
    request.headers.iter().find(|(k, _)| k == "Authorization").map(|(_, v)| v.clone()).unwrap_or_default()
}

fn sent(request: &Request) -> Value {
    serde_json::from_slice(&request.body.as_ref().expect("a body").1).unwrap()
}

/// Entries read as a playlist without repeats would be.
fn read(ids: &[&str]) -> PlaylistTracks {
    PlaylistTracks { entries: ids.iter().map(|id| Entry { id: id.to_string(), entry: id.to_string(), track: true }).collect(), whole: true }
}

#[test]
fn its_playlists_are_listed_by_name_without_the_ones_of_videos() {
    let (server, key) = lab();
    let net = Canned::new().file("jellyfin/jellyfin-playlists.json");
    let listing = playlists(&net, &server, &key).unwrap();
    assert!(listing.whole, "three of three");
    let names: Vec<&str> = listing.found.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["Empty for now", "Road Trip"], "the playlist of videos is left out, the empty one stays");
    assert_eq!((listing.found[1].description.as_str(), listing.found[0].description.as_str()), ("Songs for the drive", ""));
    assert!(listing.found.iter().all(|p| p.editable.is_none()), "the listing does not say who may change one");
    let asked = &net.asked()[0];
    assert!(asked.url.starts_with(&format!("{AT}/Items?userId={USER}&IncludeItemTypes=Playlist&Recursive=true")), "{}", asked.url);
    assert_eq!(asked.method, "GET");
    assert!(header(asked).ends_with(r#"Token="tok%20en""#));
}

#[test]
fn a_playlist_s_entries_keep_its_order_its_repeats_and_the_place_of_a_video() {
    let (server, key) = lab();
    let net = Canned::new().file("jellyfin/jellyfin-playlist-items.json");
    let read = playlist_tracks(&net, &server, &key, ROAD).unwrap();
    assert_eq!(read.tracks(), [FIRST_SONG_1, COMP_TRACK_2, FIRST_SONG_1, HI_RES_D2_T1, COMP_TRACK_1]);
    assert_eq!((read.entries.len(), read.skipped(), read.whole), (6, 1, true), "the music video keeps its place");
    assert!(!read.entries[2].track);
    assert_eq!(read.entries[3].entry, FIRST_SONG_1, "an entry is known by its item's id");
    let asked = &net.asked()[0].url;
    assert!(asked.starts_with(&format!("{AT}/Playlists/{ROAD}/Items?userId={USER}&StartIndex=0&Limit=250")), "{asked}");
}

#[test]
fn an_empty_playlist_is_whole_and_one_cut_short_is_not() {
    let (server, key) = lab();
    let net = Canned::new().file("jellyfin/jellyfin-playlist-empty.json");
    let empty = playlist_tracks(&net, &server, &key, "0f6e2d9a7b3c4e15a8d2c6f9e1b47a52").unwrap();
    assert!(empty.entries.is_empty() && empty.whole, "empty on the server, empty here");

    let mut body: Value = serde_json::from_slice(&std::fs::read(format!("{}/tests/answers/jellyfin/jellyfin-playlist-items.json", env!("CARGO_MANIFEST_DIR"))).unwrap()).unwrap();
    body["TotalRecordCount"] = json!(260);
    let net = Canned::new().answer(200, body.to_string().as_bytes()).answer(200, br#"{"Items":[],"TotalRecordCount":260,"StartIndex":6}"#);
    let cut = playlist_tracks(&net, &server, &key, ROAD).unwrap();
    assert!(!cut.whole, "six of 260, then nothing");
    assert!(net.asked()[1].url.contains("StartIndex=6"), "the next page starts where the first ended");

    let net = Canned::new().answer(200, br#"{"Items":[{"Id":"x","Type":"Audio"}]}"#);
    assert!(!playlist_tracks(&net, &server, &key, "b3").unwrap().whole, "no total: the end cannot be told from a cut");
}

#[test]
fn a_playlist_gone_or_not_its_to_read_is_told_from_a_session_refused() {
    let (server, key) = lab();
    let net = Canned::new().answer(404, b"Playlist not found").answer(403, b"").answer(401, b"");
    assert!(matches!(playlist_tracks(&net, &server, &key, "gone"), Err(Failure::Gone(_))));
    assert!(matches!(playlist_tracks(&net, &server, &key, "theirs"), Err(Failure::Forbidden(_))), "not shared with this user");
    assert!(matches!(playlists(&net, &server, &key), Err(Failure::Refused(_))), "the session itself");
}

#[test]
fn who_may_change_a_playlist_is_asked_of_the_server() {
    let (server, key) = lab();
    let net = Canned::new().answer(200, format!(r#"{{"UserId":"{USER}","CanEdit":true}}"#).as_bytes()).answer(200, br#"{"UserId":"x","CanEdit":false}"#).answer(404, b"User permissions not found");
    assert!(playlist_editable(&net, &server, &key, ROAD), "the owner, or shared to edit");
    assert!(!playlist_editable(&net, &server, &key, ROAD), "shared to listen");
    assert!(!playlist_editable(&net, &server, &key, ROAD), "public, and not shared");
    assert_eq!(net.asked()[0].url, format!("{AT}/Playlists/{ROAD}/Users/{USER}"));
}

#[test]
fn creating_renaming_and_deleting_are_asked_as_the_server_names_them() {
    let (server, key) = lab();
    let net = Canned::new().answer(200, br#"{"Id":"0a1b2c3d4e5f60718293a4b5c6d7e8f9"}"#).answer(204, b"").answer(204, b"");
    let none = PlaylistTracks::default();
    let made = change_playlist(&net, &server, &key, "", &none, &Change::Create { name: "Night drive".into(), tracks: vec![FIRST_SONG_1.into(), COMP_TRACK_1.into()] }).unwrap();
    assert_eq!(made.as_deref(), Some("0a1b2c3d4e5f60718293a4b5c6d7e8f9"));
    change_playlist(&net, &server, &key, ROAD, &none, &Change::Edit { name: Some("Road trip".into()), description: None }).unwrap();
    change_playlist(&net, &server, &key, ROAD, &none, &Change::Delete).unwrap();
    let asked = net.asked();
    assert_eq!((asked[0].method, asked[0].url.as_str()), ("POST", format!("{AT}/Playlists").as_str()));
    assert_eq!(sent(&asked[0]), json!({"Name": "Night drive", "Ids": [FIRST_SONG_1, COMP_TRACK_1], "UserId": USER, "MediaType": "Audio", "IsPublic": false}));
    assert_eq!((asked[1].method, asked[1].url.as_str()), ("POST", format!("{AT}/Playlists/{ROAD}").as_str()));
    assert_eq!(sent(&asked[1]), json!({"Name": "Road trip"}));
    assert_eq!((asked[2].method, asked[2].url.as_str()), ("DELETE", format!("{AT}/Items/{ROAD}").as_str()));
    assert!(asked.iter().all(|r| header(r).ends_with(r#"Token="tok%20en""#)));

    let net = Canned::new().answer(401, b"Unauthorized access");
    assert!(matches!(change_playlist(&net, &server, &key, ROAD, &none, &Change::Delete), Err(Failure::Forbidden(_))), "shared to edit, and not the owner's to delete");
}

#[test]
fn tracks_go_in_a_hundred_at_a_time_and_entries_come_out_by_their_id() {
    let (server, key) = lab();
    let many: Vec<String> = (0..150).map(|n| format!("{n:032x}")).collect();
    let net = Canned::new().answer(204, b"").answer(204, b"").answer(204, b"");
    change_playlist(&net, &server, &key, ROAD, &PlaylistTracks::default(), &Change::Add { tracks: many }).unwrap();
    let as_read = read(&[FIRST_SONG_1, COMP_TRACK_2, HI_RES_D2_T1]);
    change_playlist(&net, &server, &key, ROAD, &as_read, &Change::Remove { at: vec![2, 0] }).unwrap();
    let asked = net.asked();
    assert_eq!(asked.len(), 3);
    assert!(asked[0].url.starts_with(&format!("{AT}/Playlists/{ROAD}/Items?ids=")) && asked[0].url.ends_with(&format!("&userId={USER}")), "{}", asked[0].url);
    assert_eq!(asked[0].url.matches(',').count(), 99, "a hundred in the first");
    assert_eq!(asked[1].url.matches(',').count(), 49, "the other fifty in the second");
    assert_eq!((asked[2].method, asked[2].url.as_str()), ("DELETE", format!("{AT}/Playlists/{ROAD}/Items?entryIds={HI_RES_D2_T1},{FIRST_SONG_1}").as_str()));

    let net = Canned::new();
    assert!(change_playlist(&net, &server, &key, ROAD, &as_read, &Change::Remove { at: vec![3] }).is_err(), "a place it does not have");
    assert!(net.asked().is_empty(), "and nothing went out");
}

#[test]
fn a_new_order_is_one_move_for_each_entry_that_moved() {
    let (server, key) = lab();
    let as_read = read(&[FIRST_SONG_1, COMP_TRACK_2, HI_RES_D2_T1, COMP_TRACK_1]);
    let net = Canned::new().answer(204, b"");
    change_playlist(&net, &server, &key, ROAD, &as_read, &Change::Reorder { order: vec![3, 0, 1, 2] }).unwrap();
    let asked = net.asked();
    assert_eq!(asked.len(), 1, "the last one to the top");
    assert_eq!((asked[0].method, asked[0].url.as_str()), ("POST", format!("{AT}/Playlists/{ROAD}/Items/{COMP_TRACK_1}/Move/0").as_str()));

    // The lab's "Road Trip" holds a track twice, and a move takes the first.
    let net = Canned::new().file("jellyfin/jellyfin-playlist-items.json");
    let repeated = playlist_tracks(&net, &server, &key, ROAD).unwrap();
    let net = Canned::new();
    let refused = change_playlist(&net, &server, &key, ROAD, &repeated, &Change::Reorder { order: vec![5, 0, 1, 2, 3, 4] });
    assert!(matches!(refused, Err(Failure::Unexpected(_))), "{refused:?}");
    assert!(net.asked().is_empty(), "refused before anything went out");
    let cut = PlaylistTracks { whole: false, ..as_read };
    assert!(change_playlist(&net, &server, &key, ROAD, &cut, &Change::Reorder { order: vec![3, 0, 1, 2] }).is_err(), "not read whole");
    assert!(net.asked().is_empty());
}

#[test]
fn one_copy_of_a_repeated_track_is_not_taken_out_alone() {
    let (server, key) = lab();
    let net = Canned::new().file("jellyfin/jellyfin-playlist-items.json");
    let repeated = playlist_tracks(&net, &server, &key, ROAD).unwrap();
    let net = Canned::new();
    // First Song 1 is at 0 and at 3; its entry id takes out both.
    assert!(matches!(change_playlist(&net, &server, &key, ROAD, &repeated, &Change::Remove { at: vec![3] }), Err(Failure::Unexpected(_))));
    assert!(net.asked().is_empty(), "refused before anything went out");
    let net = Canned::new().answer(204, b"");
    change_playlist(&net, &server, &key, ROAD, &repeated, &Change::Remove { at: vec![0, 3] }).unwrap();
    assert_eq!(net.asked()[0].url, format!("{AT}/Playlists/{ROAD}/Items?entryIds={FIRST_SONG_1}"), "both copies, as one entry id");
}

#[test]
fn a_description_is_not_sent_to_a_jellyfin_server() {
    let (server, key) = lab();
    let net = Canned::new();
    let none = PlaylistTracks::default();
    let asked = Change::Edit { name: Some("Road trip".into()), description: Some("For the drive".into()) };
    assert!(matches!(change_playlist(&net, &server, &key, ROAD, &none, &asked), Err(Failure::Unexpected(_))));
    assert!(net.asked().is_empty(), "not even the name: the change goes whole or not at all");
    change_playlist(&net, &server, &key, ROAD, &none, &Change::Edit { name: None, description: None }).unwrap();
    assert!(net.asked().is_empty(), "nothing to say, nothing sent");
}

#[test]
fn a_change_refused_for_the_playlist_says_so() {
    let (server, key) = lab();
    let net = Canned::new().answer(403, b"").answer(404, b"Playlist not found");
    let none = PlaylistTracks::default();
    assert!(matches!(change_playlist(&net, &server, &key, ROAD, &none, &Change::Edit { name: Some("x".into()), description: None }), Err(Failure::Forbidden(_))));
    assert!(matches!(change_playlist(&net, &server, &key, ROAD, &none, &Change::Add { tracks: vec!["a".into()] }), Err(Failure::Gone(_))));
}
