//! The playlist answers are synthetic: shaped as the OpenSubsonic examples
//! and Navidrome 0.64.2 write them, with the lab library's own song ids.
//! None was saved from a server yet.

use super::*;
use crate::model::Client;
use crate::net::testing::Canned;

const AT: &str = "http://music.lan:4533";
const ROAD: &str = "5tL2pRqWm9XbKc3VdNf8Gh";
const OK: &[u8] = br#"{"subsonic-response":{"status":"ok","version":"1.16.1"}}"#;

fn lab(extensions: &[&str]) -> (Server, Credential) {
    let client = Client { version: "0.0.0".into(), platform: "Linux".into(), device: "desk".into(), id: "c0ffee".into() };
    let extensions = extensions.iter().map(|e| e.to_string()).collect();
    (crate::subsonic::server(AT, client, Vec::new(), extensions), Credential::Password { user: "lab".into(), password: "lab-password".into() })
}

/// Every value of one parameter, in order.
fn values(query: &str, name: &str) -> Vec<String> {
    query.split(['?', '&']).filter_map(|pair| pair.strip_prefix(&format!("{name}="))).map(str::to_string).collect()
}

fn read_road(server: &Server, key: &Credential) -> PlaylistTracks {
    playlist_tracks(&Canned::new().file("navidrome/subsonic-playlist.json"), server, key, ROAD).unwrap()
}

#[test]
fn its_playlists_say_whose_they_are_and_their_songs_keep_their_order() {
    let (server, key) = lab(&[]);
    let net = Canned::new().file("navidrome/subsonic-playlists.json").file("navidrome/subsonic-playlists.json");
    let listing = playlists(&net, &server, &key, "lab").unwrap();
    assert!(listing.whole);
    let named: Vec<(&str, &str, Option<bool>)> = listing.found.iter().map(|p| (p.name.as_str(), p.description.as_str(), p.editable)).collect();
    assert_eq!(named, [("Road Trip", "Songs for the drive", Some(true)), ("Empty for now", "", Some(true))]);
    let theirs = playlists(&net, &server, &key, "someone").unwrap();
    assert!(theirs.found.iter().all(|p| p.editable == Some(false)), "another user's are only read");

    let read = read_road(&server, &key);
    assert_eq!(read.tracks(), ["2FeDGrsepscsZy3Cv5BEmt", "4ga7TPjhl6cKaxXCe1uVPV", "2FeDGrsepscsZy3Cv5BEmt", "2DSqoTsUI6vwEEmqv8z9dk", "3SdaeYx2CkAdT7V60T0NVi"]);
    assert_eq!((read.entries.len(), read.skipped(), read.whole), (6, 1, true), "the video keeps its place");
    let asked = net.asked();
    assert!(asked[0].url.starts_with(&format!("{AT}/rest/getPlaylists.view?")), "{}", asked[0].url);
    assert!(asked.iter().all(|r| r.method == "GET"));
}

#[test]
fn a_playlist_the_server_no_longer_has_or_will_not_let_change_says_so() {
    let (server, key) = lab(&[]);
    let gone = br#"{"subsonic-response":{"status":"failed","version":"1.16.1","error":{"code":70,"message":"Playlist not found"}}}"#;
    let theirs = br#"{"subsonic-response":{"status":"failed","version":"1.16.1","error":{"code":50,"message":"not the owner"}}}"#;
    let net = Canned::new().answer(200, gone).answer(200, theirs).answer(200, OK);
    assert!(matches!(playlist_tracks(&net, &server, &key, "gone"), Err(Failure::Gone(_))));
    assert!(matches!(change_playlist(&net, &server, &key, ROAD, &PlaylistTracks::default(), &Change::Delete), Err(Failure::Forbidden(_))));
    assert!(playlist_tracks(&net, &server, &key, "x").is_err(), "an answer without the playlist is no empty playlist");
    let net = Canned::new().answer(200, br#"{"subsonic-response":{"status":"ok","version":"1.16.1","playlists":{}}}"#);
    let none = playlists(&net, &server, &key, "lab").unwrap();
    assert!(none.found.is_empty() && none.whole, "no playlists at all");
}

#[test]
fn every_change_is_asked_by_its_endpoint_in_the_address() {
    let (server, key) = lab(&[]);
    let read = read_road(&server, &key);
    let created = br#"{"subsonic-response":{"status":"ok","version":"1.16.1","playlist":{"id":"9zNewOne","name":"Night drive","entry":[]}}}"#;
    let net = Canned::new().answer(200, created).answer(200, OK).answer(200, OK).answer(200, OK).answer(200, OK).answer(200, OK);
    let made = change_playlist(&net, &server, &key, "", &PlaylistTracks::default(), &Change::Create { name: "Night drive".into(), tracks: vec!["a".into(), "b".into()] }).unwrap();
    assert_eq!(made.as_deref(), Some("9zNewOne"));
    change_playlist(&net, &server, &key, ROAD, &read, &Change::Edit { name: Some("Road trip".into()), description: Some("For the drive".into()) }).unwrap();
    change_playlist(&net, &server, &key, ROAD, &read, &Change::Add { tracks: vec!["c".into()] }).unwrap();
    change_playlist(&net, &server, &key, ROAD, &read, &Change::Remove { at: vec![2, 0] }).unwrap();
    change_playlist(&net, &server, &key, ROAD, &read, &Change::Reorder { order: vec![5, 0, 1, 2, 3, 4] }).unwrap();
    change_playlist(&net, &server, &key, ROAD, &read, &Change::Delete).unwrap();
    let asked = net.asked();
    assert!(asked.iter().all(|r| r.method == "GET"), "the server takes no form: everything in the address");
    assert!(asked[0].url.starts_with(&format!("{AT}/rest/createPlaylist.view?")));
    assert_eq!((values(&asked[0].url, "name"), values(&asked[0].url, "songId")), (vec!["Night%20drive".to_string()], vec!["a".to_string(), "b".to_string()]));
    assert_eq!((values(&asked[1].url, "playlistId"), values(&asked[1].url, "name")), (vec![ROAD.to_string()], vec!["Road%20trip".to_string()]));
    assert_eq!(values(&asked[1].url, "comment"), ["For%20the%20drive"], "the description is its comment");
    assert_eq!(values(&asked[2].url, "songIdToAdd"), ["c"]);
    assert_eq!(values(&asked[3].url, "songIndexToRemove"), ["2", "0"], "the places, in one request");
    assert!(asked[4].url.starts_with(&format!("{AT}/rest/createPlaylist.view?")), "a new order is the whole list written again");
    assert_eq!(values(&asked[4].url, "playlistId"), [ROAD]);
    assert_eq!(
        values(&asked[4].url, "songId"),
        ["3SdaeYx2CkAdT7V60T0NVi", "2FeDGrsepscsZy3Cv5BEmt", "4ga7TPjhl6cKaxXCe1uVPV", "6hViDeOcLiPaB2nM4qWx7Z", "2FeDGrsepscsZy3Cv5BEmt", "2DSqoTsUI6vwEEmqv8z9dk"],
        "the video too, in its place"
    );
    assert!(asked[5].url.starts_with(&format!("{AT}/rest/deletePlaylist.view?")) && values(&asked[5].url, "id") == [ROAD]);
}

#[test]
fn a_long_list_goes_in_a_form_when_the_server_takes_one_and_is_refused_otherwise() {
    let many: Vec<String> = (0..400).map(|n| format!("song-{n:016}")).collect();
    let (plain, key) = lab(&[]);
    let net = Canned::new().answer(200, OK).answer(200, OK).answer(200, OK).answer(200, OK);
    change_playlist(&net, &plain, &key, ROAD, &PlaylistTracks::default(), &Change::Add { tracks: many.clone() }).unwrap();
    assert_eq!(net.asked().len(), 4, "a hundred at a time in the address");
    let net = Canned::new();
    let refused = change_playlist(&net, &plain, &key, "", &PlaylistTracks::default(), &Change::Create { name: "Big".into(), tracks: many.clone() });
    assert!(matches!(refused, Err(Failure::Unexpected(_))), "{refused:?}");
    assert!(net.asked().is_empty(), "refused before it went out");

    let (forms, key) = lab(&["formPost"]);
    let net = Canned::new().answer(200, OK);
    change_playlist(&net, &forms, &key, ROAD, &PlaylistTracks::default(), &Change::Add { tracks: many }).unwrap();
    let asked = net.asked();
    assert_eq!((asked.len(), asked[0].method), (1, "POST"), "all at once in the body");
    let (kind, body) = asked[0].body.clone().unwrap();
    assert_eq!(kind, "application/x-www-form-urlencoded");
    let body = String::from_utf8(body).unwrap();
    assert!(body.starts_with(&format!("playlistId={ROAD}&songIdToAdd=")), "{}", &body[..60]);
    assert_eq!(values(&format!("?{body}"), "songIdToAdd").len(), 400);
    assert!(!asked[0].url.contains("songIdToAdd"), "nothing of the list in the address");
}
