//! The playlist answers are synthetic: shaped as Plex Media Server answers
//! `/playlists` and `/playlists/{ratingKey}/items` in JSON, with the lab
//! library's own ratingKeys. None was saved from a server yet, and the
//! writes follow python-plexapi, not a capture.

use super::*;
use crate::model::Client;
use crate::net::testing::Canned;

const AT: &str = "http://192.168.0.69:32400";

fn lab() -> (Server, Credential) {
    let client = Client { version: "0.1.0".into(), platform: "Linux".into(), device: "desk".into(), id: "9e2b7c1d4a6f8e30".into() };
    let mut server = crate::plex::server(AT, client, vec!["1".into()], Vec::new());
    server.details.machine = "f00dfeed".into();
    (server, Credential::Plex { account: "acct".into(), server: "srv".into() })
}

fn header<'a>(request: &'a Request, name: &str) -> Option<&'a str> {
    request.headers.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
}

fn road(server: &Server, key: &Credential) -> PlaylistTracks {
    playlist_tracks(&Canned::new().file("plex/plex-playlist-items.json"), server, key, "30").unwrap()
}

#[test]
fn its_audio_playlists_and_their_entries_come_in_their_order() {
    let (server, key) = lab();
    let net = Canned::new().file("plex/plex-playlists.json").file("plex/plex-playlist-items.json");
    let listing = playlists(&net, &server, &key).unwrap();
    assert!(listing.whole);
    let named: Vec<(&str, Option<bool>)> = listing.found.iter().map(|p| (p.name.as_str(), p.editable)).collect();
    assert_eq!(named, [("Road Trip", Some(true)), ("All Lab Music", Some(false))], "a smart one is read, not changed; one of videos never");
    assert_eq!(listing.found[0].description, "Songs for the drive");
    let read = playlist_tracks(&net, &server, &key, &listing.found[0].id).unwrap();
    assert_eq!(read.tracks(), ["17", "8", "17", "12", "7"]);
    assert_eq!((read.skipped(), read.whole), (1, true), "the clip keeps its place");
    assert_eq!(read.entries.iter().map(|e| e.entry.as_str()).collect::<Vec<_>>(), ["901", "902", "903", "904", "905", "906"], "each entry its own id");
    let asked = net.asked();
    assert_eq!(asked[0].url, format!("{AT}/playlists?playlistType=audio"));
    assert_eq!(asked[1].url, format!("{AT}/playlists/30/items"));
    assert_eq!(header(&asked[1], "X-Plex-Token"), Some("srv"), "the server's token");
    assert_eq!(header(&asked[1], "X-Plex-Container-Start"), Some("0"));
}

#[test]
fn a_long_playlist_without_a_total_is_read_until_a_short_page() {
    let (server, key) = lab();
    let items: Vec<String> = (0..250).map(|n| format!(r#"{{"ratingKey":"{n}","type":"track","playlistItemID":{}}}"#, 1000 + n)).collect();
    let full = format!(r#"{{"MediaContainer":{{"size":250,"Metadata":[{}]}}}}"#, items.join(","));
    let net = Canned::new().answer(200, full.as_bytes()).answer(200, br#"{"MediaContainer":{"size":1,"Metadata":[{"ratingKey":"250","type":"track","playlistItemID":1250}]}}"#);
    let read = playlist_tracks(&net, &server, &key, "30").unwrap();
    assert_eq!((read.entries.len(), read.whole), (251, true));
    assert_eq!(header(&net.asked()[1], "X-Plex-Container-Start"), Some("250"));
    let net = Canned::new().answer(404, b"").answer(403, b"");
    assert!(matches!(playlist_tracks(&net, &server, &key, "gone"), Err(Failure::Gone(_))));
    assert!(matches!(change_playlist(&net, &server, &key, "30", &read, &Change::Delete), Err(Failure::Forbidden(_))));
}

#[test]
fn tracks_go_in_by_the_server_s_uri_and_entries_come_out_by_their_own_id() {
    let (server, key) = lab();
    let created = br#"{"MediaContainer":{"size":1,"Metadata":[{"ratingKey":"44","type":"playlist","title":"Night drive","smart":false}]}}"#;
    let net = Canned::new().answer(200, created).answer(200, b"").answer(200, b"").answer(200, b"").answer(200, b"").answer(200, b"");
    let none = PlaylistTracks::default();
    let made = change_playlist(&net, &server, &key, "", &none, &Change::Create { name: "Night drive".into(), tracks: vec!["17".into(), "8".into()] }).unwrap();
    assert_eq!(made.as_deref(), Some("44"));
    change_playlist(&net, &server, &key, "30", &none, &Change::Edit { name: Some("Road trip".into()), description: Some("For the drive".into()) }).unwrap();
    change_playlist(&net, &server, &key, "30", &none, &Change::Add { tracks: vec!["12".into()] }).unwrap();
    let read = road(&server, &key);
    change_playlist(&net, &server, &key, "30", &read, &Change::Remove { at: vec![3, 0] }).unwrap();
    change_playlist(&net, &server, &key, "30", &none, &Change::Delete).unwrap();
    let asked = net.asked();
    let uri = "server%3A%2F%2Ff00dfeed%2Fcom.plexapp.plugins.library%2Flibrary%2Fmetadata%2F";
    assert_eq!((asked[0].method, asked[0].url.clone()), ("POST", format!("{AT}/playlists?type=audio&title=Night%20drive&smart=0&uri={uri}17%2C8")));
    assert_eq!((asked[1].method, asked[1].url.clone()), ("PUT", format!("{AT}/playlists/30?title=Road%20trip&summary=For%20the%20drive")));
    assert_eq!((asked[2].method, asked[2].url.clone()), ("PUT", format!("{AT}/playlists/30/items?uri={uri}12")));
    assert_eq!((asked[3].method, asked[3].url.clone()), ("DELETE", format!("{AT}/playlists/30/items/904")));
    assert_eq!((asked[4].method, asked[4].url.clone()), ("DELETE", format!("{AT}/playlists/30/items/901")));
    assert_eq!((asked[5].method, asked[5].url.clone()), ("DELETE", format!("{AT}/playlists/30")));
    assert!(asked.iter().all(|r| header(r, "X-Plex-Token") == Some("srv")));
    let describable: Vec<bool> = crate::model::Kind::ALL.iter().map(|&kind| crate::playlist_describable(kind)).collect();
    assert_eq!(describable, [true, false, true], "Subsonic and Plex take a description; Jellyfin only a name");
}

#[test]
fn a_new_order_moves_each_entry_after_the_one_before_it() {
    let (server, key) = lab();
    let read = road(&server, &key);
    let net = Canned::new().answer(200, b"").answer(200, b"");
    // The last to the top, and the clip after the first track.
    change_playlist(&net, &server, &key, "30", &read, &Change::Reorder { order: vec![5, 0, 2, 1, 3, 4] }).unwrap();
    let asked: Vec<String> = net.asked().iter().map(|r| r.url.clone()).collect();
    assert_eq!(asked, [format!("{AT}/playlists/30/items/906/move"), format!("{AT}/playlists/30/items/903/move?after=901")]);
    let net = Canned::new();
    assert!(change_playlist(&net, &server, &key, "30", &read, &Change::Reorder { order: vec![0, 0, 1, 2, 3, 4] }).is_err());
    assert!(net.asked().is_empty(), "an order that is not one goes nowhere");
}
