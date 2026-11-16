//! Favorites against answers in the shapes the lab servers gave
//! (CHD-0060 §4). `subsonic-starred2.json` is written by hand in the shape
//! Navidrome 0.64.2 answers `getStarred2` with, for ids of the saved
//! catalogue; the Jellyfin listings follow `jellyfin-items.json`.

use super::*;
use crate::model::Client;
use crate::net::testing::Canned;

fn client() -> Client {
    Client { version: "0.0.0".into(), platform: "Linux".into(), device: "desk".into(), id: "c0ffee".into() }
}

fn subsonic() -> (Server, Credential) {
    (subsonic::server("http://music.lan:4533", client(), Vec::new(), Vec::new()), Credential::Password { user: "lab".into(), password: "lab-password".into() })
}

fn jellyfin() -> (Server, Credential) {
    (jellyfin::server("http://jf.lan:8096", client(), Vec::new(), "user-1"), Credential::Token { token: "tok".into() })
}

fn query(url: &str, name: &str) -> Option<String> {
    url.split('?').nth(1)?.split('&').find_map(|pair| pair.strip_prefix(&format!("{name}=")).map(str::to_string))
}

fn later() -> Instant {
    Instant::now() + Duration::from_secs(60)
}

fn ok() -> &'static [u8] {
    br#"{"subsonic-response":{"status":"ok","version":"1.16.1"}}"#
}

#[test]
fn only_the_kinds_that_keep_favorites_say_so() {
    assert!(Kind::Subsonic.keeps_favorites());
    assert!(Kind::Jellyfin.keeps_favorites());
    assert!(!Kind::Plex.keeps_favorites(), "its like is the five-star rating");
    assert!(!Kind::Dlna.keeps_favorites());
}

#[test]
fn a_subsonic_server_lists_its_starred_songs_and_records_at_once() {
    let (server, key) = subsonic();
    let net = Canned::new().file("navidrome/subsonic-starred2.json");
    let found = favorites(&net, &server, &key, 10, later()).unwrap();
    assert_eq!(found.tracks, ["47gLTJvR49ADB1Pa0vPhB5".to_string()].into());
    assert_eq!(found.albums, ["2NGalRARWUlolzE5BntvyN".to_string()].into());
    assert!(found.whole);
    assert!(net.asked()[0].url.contains("/rest/getStarred2.view?"));
    let mut chosen = server.clone();
    chosen.libraries = vec!["1".into(), "2".into()];
    let net = Canned::new().file("navidrome/subsonic-starred2.json").file("navidrome/subsonic-starred2.json");
    favorites(&net, &chosen, &key, 10, later()).unwrap();
    let asked: Vec<Option<String>> = net.asked().iter().map(|r| query(&r.url, "musicFolderId")).collect();
    assert_eq!(asked, [Some("1".to_string()), Some("2".to_string())], "one listing per chosen folder");
}

#[test]
fn subsonic_stars_a_song_by_id_and_a_record_by_album_id() {
    let (server, key) = subsonic();
    let gone = br#"{"subsonic-response":{"status":"failed","version":"1.16.1","error":{"code":70,"message":"not found"}}}"#;
    let net = Canned::new().answer(200, ok()).answer(200, ok()).answer(200, gone);
    favor(&net, &server, &key, Rated::Track, "47gLTJvR49ADB1Pa0vPhB5", true).unwrap();
    favor(&net, &server, &key, Rated::Album, "2NGalRARWUlolzE5BntvyN", false).unwrap();
    assert!(matches!(favor(&net, &server, &key, Rated::Track, "x", true), Err(Failure::Gone(_))));
    let asked = net.asked();
    assert!(asked[0].url.contains("/rest/star.view?"));
    assert_eq!(query(&asked[0].url, "id").as_deref(), Some("47gLTJvR49ADB1Pa0vPhB5"));
    assert!(asked[1].url.contains("/rest/unstar.view?"));
    assert_eq!(query(&asked[1].url, "albumId").as_deref(), Some("2NGalRARWUlolzE5BntvyN"));
    assert!(asked.iter().all(|r| r.method == "GET" && r.timeout <= Duration::from_secs(15)));
}

#[test]
fn a_starred_song_reads_back_as_a_favorite() {
    let (server, key) = subsonic();
    let starred = br#"{"subsonic-response":{"status":"ok","version":"1.16.1","song":{"id":"a","title":"A","starred":"2026-11-11T12:00:00Z"}}}"#;
    let plain = br#"{"subsonic-response":{"status":"ok","version":"1.16.1","song":{"id":"a","title":"A"}}}"#;
    let net = Canned::new().answer(200, starred).answer(200, plain);
    assert!(is_favorite(&net, &server, &key, Rated::Track, "a").unwrap());
    assert!(!is_favorite(&net, &server, &key, Rated::Track, "a").unwrap());
}

#[test]
fn a_jellyfin_server_lists_its_favorite_tracks_then_records_page_by_page() {
    let (server, key) = jellyfin();
    let net = Canned::new()
        .answer(200, br#"{"Items":[{"Id":"t1"},{"Id":"t2"}],"TotalRecordCount":3,"StartIndex":0}"#)
        .answer(200, br#"{"Items":[{"Id":"t3"}],"TotalRecordCount":3,"StartIndex":2}"#)
        .answer(200, br#"{"Items":[{"Id":"a1"}],"TotalRecordCount":1,"StartIndex":0}"#);
    let found = favorites(&net, &server, &key, 10, later()).unwrap();
    assert_eq!(found.tracks, ["t1", "t2", "t3"].map(String::from).into());
    assert_eq!(found.albums, ["a1".to_string()].into());
    assert!(found.whole);
    let asked = net.asked();
    assert!(asked[0].url.contains("IncludeItemTypes=Audio") && asked[0].url.contains("IsFavorite=true") && asked[0].url.contains("userId=user-1"));
    assert_eq!(query(&asked[1].url, "StartIndex").as_deref(), Some("2"));
    assert!(asked[2].url.contains("IncludeItemTypes=MusicAlbum"));
    let cut = favorites(&Canned::new().answer(200, br#"{"Items":[{"Id":"t1"}],"TotalRecordCount":5}"#), &server, &key, 1, later()).unwrap();
    assert!(!cut.whole, "the budget ran out before every favorite was listed");
}

#[test]
fn jellyfin_marks_with_post_and_unmarks_with_delete() {
    let (server, key) = jellyfin();
    let net = Canned::new().answer(200, br#"{"IsFavorite":true}"#).answer(200, br#"{"IsFavorite":false}"#).answer(404, b"");
    favor(&net, &server, &key, Rated::Track, "t1", true).unwrap();
    favor(&net, &server, &key, Rated::Album, "a1", false).unwrap();
    assert!(matches!(favor(&net, &server, &key, Rated::Track, "gone", true), Err(Failure::Gone(_))));
    let asked = net.asked();
    assert_eq!((asked[0].method, asked[0].url.as_str()), ("POST", "http://jf.lan:8096/UserFavoriteItems/t1?userId=user-1"));
    assert_eq!((asked[1].method, asked[1].url.as_str()), ("DELETE", "http://jf.lan:8096/UserFavoriteItems/a1?userId=user-1"));
    assert!(asked[0].headers.iter().any(|(k, v)| k == "Authorization" && v.contains("Token=\"tok\"")));
}

#[test]
fn a_kind_without_favorites_is_never_asked() {
    let server = crate::plex::server("http://192.168.0.69:32400", client(), Vec::new(), Vec::new());
    let key = Credential::Plex { account: "a".into(), server: "s".into() };
    let net = Canned::new();
    assert!(favor(&net, &server, &key, Rated::Track, "1", true).is_err());
    assert_eq!(favorites(&net, &server, &key, 10, later()).unwrap(), Favorites { whole: true, ..Favorites::default() });
    assert!(net.asked().is_empty());
}
