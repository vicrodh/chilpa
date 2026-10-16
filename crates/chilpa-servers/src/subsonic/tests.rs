//! Against Navidrome 0.64.2's own answers, saved on 2026-09-28 from a lab
//! server with a synthetic library (CHD-0060 §4).

use super::*;
use crate::net::testing::Canned;

const AT: &str = "http://music.lan:4533";

fn client() -> Client {
    Client { version: "0.0.0".into(), platform: "Linux".into(), device: "desk".into(), id: "c0ffee".into() }
}

fn lab() -> (Server, Credential) {
    (server(AT, client(), Vec::new(), Vec::new()), Credential::Password { user: "lab".into(), password: "lab-password".into() })
}

fn query(url: &str, name: &str) -> Option<String> {
    url.split('?').nth(1)?.split('&').find_map(|pair| pair.strip_prefix(&format!("{name}=")).map(str::to_string))
}

#[test]
fn a_probe_reads_what_the_server_is_before_anyone_signs_in() {
    let net = Canned::new().file("navidrome/subsonic-ping-anonymous.json").file("navidrome/subsonic-extensions.json");
    let found = probe(&net, AT).unwrap();
    assert_eq!(found.product, "Navidrome 0.64.2");
    assert!(!found.api_key, "Navidrome 0.64.2 takes no API key");
    assert!(found.extensions.contains(&"songLyrics".to_string()));
    assert!(net.asked().iter().all(|r| query(&r.url, "u").is_none()), "nothing of mine went out");
}

#[test]
fn a_password_goes_out_only_as_a_salted_token_that_changes_every_time() {
    let net = Canned::new().file("navidrome/subsonic-ping.json").file("navidrome/subsonic-extensions.json");
    let signed = sign_in(&net, AT, "lab", "lab-password", false).unwrap();
    assert_eq!(signed.account, "lab");
    assert_eq!(signed.identity, "http://music.lan:4533|lab");
    assert_eq!(signed.product, "Navidrome 0.64.2");
    let asked = net.asked();
    for request in &asked {
        assert!(!request.url.contains("lab-password"), "{}", request.url);
        let (salt, token) = (query(&request.url, "s").unwrap(), query(&request.url, "t").unwrap());
        assert_eq!(token, format!("{:x}", Md5::digest(format!("lab-password{salt}").as_bytes())));
    }
    assert_ne!(query(&asked[0].url, "s"), query(&asked[1].url, "s"), "a fresh salt per request");
}

#[test]
fn a_wrong_password_is_refused_as_such() {
    let net = Canned::new().file("navidrome/subsonic-ping-wrong.json");
    assert!(matches!(sign_in(&net, AT, "lab", "nope", false), Err(Failure::Refused(_))));
}

#[test]
fn an_api_key_asks_the_server_whose_it_is() {
    let net = Canned::new()
        .answer(200, br#"{"subsonic-response":{"status":"ok","version":"1.16.1","type":"lms","serverVersion":"3.81.0","openSubsonic":true}}"#)
        .answer(200, br#"{"subsonic-response":{"status":"ok","version":"1.16.1","openSubsonic":true,"tokenInfo":{"username":"Me"}}}"#)
        .answer(200, br#"{"subsonic-response":{"status":"ok","version":"1.16.1","openSubsonic":true,"openSubsonicExtensions":[{"name":"apiKeyAuthentication","versions":[1]}]}}"#);
    let signed = sign_in(&net, AT, "", "k3y", true).unwrap();
    assert_eq!((signed.account.as_str(), signed.product.as_str()), ("Me", "Lms 3.81.0"));
    let first = &net.asked()[0].url;
    assert_eq!(query(first, "apiKey").as_deref(), Some("k3y"));
    assert!(query(first, "u").is_none(), "with a key the user must not be sent (error 43)");
}

#[test]
fn the_catalogue_is_read_record_by_record_with_what_each_song_carries() {
    let (server, key) = lab();
    let net = Canned::new()
        .file("navidrome/subsonic-scan.json")
        .file("navidrome/subsonic-albums.json")
        .file("navidrome/subsonic-album-0.json")
        .file("navidrome/subsonic-album-1.json")
        .file("navidrome/subsonic-album-2.json")
        .file("navidrome/subsonic-album-3.json");
    let page = page(&net, &server, &key, None).unwrap();

    assert_eq!(page.tracks.len(), 12);
    assert_eq!(page.next, None, "four records are one page");
    let first = page.tracks.iter().find(|t| t.title == "First Song 1").unwrap();
    assert_eq!((first.artist.as_str(), first.album.as_str(), first.album_artist.as_str()), ("Lab Artist", "Lab Album One", "Lab Artist"));
    assert_eq!((first.codec.as_deref(), first.bits, first.rate, first.channels), (Some("flac"), Some(16), Some(44_100), Some(2)));
    assert_eq!(first.genres, vec!["Rock".to_string(), "Pop".to_string()]);
    assert_eq!(first.isrc.as_deref(), Some("USLAB0100001"));
    assert_eq!(first.recording_mbid.as_deref(), Some("22222221-2222-4222-8222-222222222222"));
    assert_eq!((first.number, first.disc, first.year, first.duration_ms), (Some(1), Some(1), Some(2001), 12_000));
    assert!(first.cover.as_deref().unwrap().starts_with("al-"), "the record's cover, not the disc's");
    assert!(first.complete, "Subsonic says the quality in the listing");

    let compiled = page.tracks.iter().find(|t| t.title == "Comp Track 1").unwrap();
    assert_eq!((compiled.artist.as_str(), compiled.album_artist.as_str()), ("Singer Uno", "Various Artists"));
    assert_eq!(compiled.codec.as_deref(), Some("alac"), "an m4a with a bit depth");
    let hires = page.tracks.iter().find(|t| t.title == "Hi-Res D2 T1").unwrap();
    assert_eq!((hires.bits, hires.rate, hires.disc), (Some(24), Some(96_000), Some(2)));
    let lossy = page.tracks.iter().find(|t| t.title == "Canción ñ 日本").unwrap();
    assert_eq!((lossy.codec.as_deref(), lossy.bits), (Some("opus"), None), "no bit depth: lossy");
    assert_eq!(page.tracks.iter().find(|t| t.title == "Segunda").unwrap().codec.as_deref(), Some("vorbis"));
}

#[test]
fn a_server_reading_its_own_library_is_waited_for() {
    let (server, key) = lab();
    let net = Canned::new().answer(200, br#"{"subsonic-response":{"status":"ok","version":"1.16.1","scanStatus":{"scanning":true,"count":10}}}"#);
    assert!(matches!(page(&net, &server, &key, None), Err(Failure::Busy(_))));
}

#[test]
fn chosen_folders_are_walked_one_after_another() {
    let (mut server, key) = lab();
    server.libraries = vec!["1".into(), "7".into()];
    let net = Canned::new().file("navidrome/subsonic-scan.json").file("navidrome/subsonic-albums-end.json");
    let first = page(&net, &server, &key, None).unwrap();
    assert!(net.asked()[1].url.contains("musicFolderId=1"));
    let net = Canned::new().file("navidrome/subsonic-albums-end.json");
    let second = page(&net, &server, &key, first.next.as_deref()).unwrap();
    assert!(net.asked()[0].url.contains("musicFolderId=7"));
    assert_eq!(second.next, None);
}

#[test]
fn an_error_in_json_or_in_xml_is_read_as_what_it_says() {
    let (server, key) = lab();
    let net = Canned::new().file("navidrome/subsonic-album-missing.json");
    assert!(matches!(libraries(&net, &server, &key), Err(Failure::Unexpected(_))));
    let net = Canned::new().answer(200, br#"<?xml version="1.0"?><subsonic-response status="failed"><error code="40" message="Wrong"/></subsonic-response>"#);
    assert!(matches!(ping(&net, &server, &key), Err(Failure::Refused(_))));
    let net = Canned::new().answer(200, b"<html>not a subsonic server</html>");
    assert!(matches!(ping(&net, &server, &key), Err(Failure::Unexpected(_))));
    let net = Canned::new().silence();
    assert!(ping(&net, &server, &key).unwrap_err().is_unreachable());
    let mut moved = crate::net::Reply { status: 308, ..Default::default() };
    moved.headers.insert("location".into(), "https://music.example.org/".into());
    struct Once(crate::net::Reply);
    impl Net for Once {
        fn send(&self, _: &Request) -> Result<crate::net::Reply, String> { Ok(self.0.clone()) }
    }
    assert_eq!(ping(&Once(moved), &server, &key), Err(Failure::Moved("https://music.example.org/".into())));
}

#[test]
fn the_folders_are_its_libraries() {
    let (server, key) = lab();
    let net = Canned::new().file("navidrome/subsonic-folders.json");
    assert_eq!(libraries(&net, &server, &key).unwrap(), vec![Library { id: "1".into(), name: "Music Library".into() }]);
}

#[test]
fn a_stream_asks_for_the_original_and_names_its_extension() {
    let (server, key) = lab();
    let stream = stream(&server, &key, "2FeDG rs/x", Some("alac"));
    assert!(stream.url.starts_with("http://music.lan:4533/rest/stream.view?"));
    assert_eq!(query(&stream.url, "format").as_deref(), Some("raw"));
    assert_eq!(query(&stream.url, "id").as_deref(), Some("2FeDG%20rs%2Fx"));
    assert_eq!(stream.extension.as_deref(), Some("m4a"));
    assert!(!format!("{stream:?}").contains("t="), "its debug form keeps the credential out");
    let cover = cover(&server, &key, "al-1_2", 1200);
    assert_eq!(query(&cover.url, "size").as_deref(), Some("1200"));
}

#[test]
fn a_numeric_id_from_funkwhale_is_an_id_like_any_other() {
    let song = serde_json::json!({"id": 2059, "title": "Numbered", "albumId": 31, "duration": 60, "suffix": "mp3"});
    let t = track(&song, &serde_json::json!({})).unwrap();
    assert_eq!((t.id.as_str(), t.album_id.as_str(), t.codec.as_deref()), ("2059", "31", Some("mp3")));
}
