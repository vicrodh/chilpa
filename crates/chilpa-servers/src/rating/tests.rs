//! Ratings against the saved answers of the lab servers (CHD-0060 §4).
//! `plex-tracks-rated.json` and `subsonic-album-0-rated.json` are the saved
//! listings with `userRating` added where each server writes it;
//! `plex-albums-rated.json` is a records listing written by hand in the
//! shape of the saved ones.

use super::*;
use crate::model::Client;
use crate::net::testing::Canned;

fn client() -> Client {
    Client { version: "0.0.0".into(), platform: "Linux".into(), device: "desk".into(), id: "c0ffee".into() }
}

fn plex() -> (Server, Credential) {
    (plex::server("http://192.168.0.69:32400", client(), vec!["1".into()], Vec::new()), Credential::Plex { account: "acct".into(), server: "srv".into() })
}

fn subsonic() -> (Server, Credential) {
    (subsonic::server("http://music.lan:4533", client(), Vec::new(), Vec::new()), Credential::Password { user: "lab".into(), password: "lab-password".into() })
}

fn query(url: &str, name: &str) -> Option<String> {
    url.split('?').nth(1)?.split('&').find_map(|pair| pair.strip_prefix(&format!("{name}=")).map(str::to_string))
}

fn later() -> Instant {
    Instant::now() + Duration::from_secs(60)
}

#[test]
fn only_the_kinds_that_keep_ratings_have_a_scale() {
    assert_eq!(Kind::Subsonic.rating_scale(), Some(Scale { min: 1.0, max: 5.0, step: 1.0 }));
    assert_eq!(Kind::Plex.rating_scale(), Some(Scale { min: 1.0, max: 10.0, step: 1.0 }));
    assert_eq!(Kind::Jellyfin.rating_scale(), None, "its Rating number has no scale said");
    assert_eq!(Kind::Dlna.rating_scale(), None);
}

#[test]
fn a_plex_page_carries_each_track_s_rating_and_unrated_is_zero() {
    let (server, key) = plex();
    let page = crate::page(&Canned::new().file("plex/plex-tracks-rated.json"), &server, &key, Some(r#"{"sections":["1"],"section":0,"offset":0}"#)).unwrap();
    assert_eq!(page.ratings.len(), page.tracks.len(), "one per track listed");
    let of = |id: &str| page.ratings.iter().find(|r| r.id == id).map(|r| (r.rated, r.value));
    assert_eq!(of("17"), Some((Rated::Track, 7.0)), "three and a half stars as Plex writes it");
    assert_eq!(of("18"), Some((Rated::Track, 10.0)));
    assert_eq!(of("19"), Some((Rated::Track, 0.0)), "no userRating is unrated");
    let unrated = crate::page(&Canned::new().file("plex/plex-tracks.json"), &server, &key, Some(r#"{"sections":["1"],"section":0,"offset":0}"#)).unwrap();
    assert!(unrated.ratings.iter().all(|r| r.value == 0.0));
}

#[test]
fn a_subsonic_page_carries_record_and_song_ratings() {
    let (server, key) = subsonic();
    let net = Canned::new()
        .file("navidrome/subsonic-scan.json")
        .file("navidrome/subsonic-albums.json")
        .file("navidrome/subsonic-album-0-rated.json")
        .file("navidrome/subsonic-album-1.json")
        .file("navidrome/subsonic-album-2.json")
        .file("navidrome/subsonic-album-3.json");
    let page = crate::page(&net, &server, &key, None).unwrap();
    let of = |rated: Rated, id: &str| page.ratings.iter().find(|r| r.rated == rated && r.id == id).map(|r| r.value);
    assert_eq!(of(Rated::Album, "2NGalRARWUlolzE5BntvyN"), Some(4.0));
    assert_eq!(of(Rated::Track, "47gLTJvR49ADB1Pa0vPhB5"), Some(5.0));
    assert_eq!(of(Rated::Track, "4AbMVAGmME5H9RzBSSy1Rr"), Some(0.0), "a song without userRating is unrated");
    assert_eq!(of(Rated::Album, "6heNiLg85nzVaSDK0UGRBh"), Some(0.0), "the saved `userRating: 0` is unrated");
    assert_eq!(page.ratings.iter().filter(|r| r.rated == Rated::Track).count(), page.tracks.len());
}

#[test]
fn a_jellyfin_page_carries_no_rating() {
    let server = crate::jellyfin::server("http://jf.lan:8096", client(), vec!["view-1".into()], "user");
    let page = crate::page(&Canned::new().file("jellyfin/jellyfin-items.json"), &server, &Credential::Token { token: "t".into() }, None).unwrap();
    assert_eq!(page.tracks.len(), 12);
    assert!(page.ratings.is_empty(), "its Rating number has no scale said");
}

#[test]
fn plex_takes_a_rating_by_put_and_minus_one_takes_it_off() {
    let (server, key) = plex();
    let net = Canned::new().answer(200, b"").answer(200, b"").answer(404, b"");
    rate(&net, &server, &key, Rated::Track, "17", Some(8.0)).unwrap();
    rate(&net, &server, &key, Rated::Album, "16", None).unwrap();
    assert!(matches!(rate(&net, &server, &key, Rated::Track, "999", Some(2.0)), Err(Failure::Gone(_))), "a key it no longer has");
    let asked = net.asked();
    assert_eq!(asked[0].method, "PUT");
    assert_eq!(asked[0].url, "http://192.168.0.69:32400/:/rate?key=17&identifier=com.plexapp.plugins.library&rating=8");
    assert!(asked[0].headers.iter().any(|(k, v)| k == "X-Plex-Token" && v == "srv"), "the server's own token");
    assert!(asked[0].body.is_none());
    assert!(asked[0].timeout <= Duration::from_secs(15));
    assert_eq!(query(&asked[1].url, "rating").as_deref(), Some("-1"));
    assert_eq!(query(&asked[1].url, "key").as_deref(), Some("16"));
}

#[test]
fn subsonic_takes_a_rating_with_set_rating_and_zero_takes_it_off() {
    let (server, key) = subsonic();
    let ok = br#"{"subsonic-response":{"status":"ok","version":"1.16.1"}}"#;
    let gone = br#"{"subsonic-response":{"status":"failed","version":"1.16.1","error":{"code":70,"message":"not found"}}}"#;
    let refused = br#"{"subsonic-response":{"status":"failed","version":"1.16.1","error":{"code":50,"message":"no"}}}"#;
    let net = Canned::new().answer(200, ok).answer(200, ok).answer(200, gone).answer(200, refused);
    rate(&net, &server, &key, Rated::Track, "47gLTJvR49ADB1Pa0vPhB5", Some(4.0)).unwrap();
    rate(&net, &server, &key, Rated::Album, "2NGalRARWUlolzE5BntvyN", None).unwrap();
    assert!(matches!(rate(&net, &server, &key, Rated::Track, "x", Some(1.0)), Err(Failure::Gone(_))));
    assert!(matches!(rate(&net, &server, &key, Rated::Track, "x", Some(1.0)), Err(Failure::Refused(_))));
    let asked = net.asked();
    assert!(asked[0].url.contains("/rest/setRating.view?"));
    assert_eq!(query(&asked[0].url, "id").as_deref(), Some("47gLTJvR49ADB1Pa0vPhB5"));
    assert_eq!(query(&asked[0].url, "rating").as_deref(), Some("4"));
    assert_eq!(query(&asked[1].url, "rating").as_deref(), Some("0"), "0 takes the rating off");
    assert!(query(&asked[0].url, "p").is_none(), "never the password itself");
}

#[test]
fn a_kind_without_ratings_is_never_asked() {
    let server = crate::jellyfin::server("http://jf.lan:8096", client(), Vec::new(), "user");
    let net = Canned::new();
    assert!(rate(&net, &server, &Credential::Token { token: "t".into() }, Rated::Track, "1", Some(4.0)).is_err());
    assert!(net.asked().is_empty());
    let listed = album_ratings(&net, &server, &Credential::Token { token: "t".into() }, 10, later()).unwrap();
    assert_eq!(listed, Listed { found: Vec::new(), whole: true });
}

#[test]
fn plex_records_are_listed_apart_with_their_ratings() {
    let (server, key) = plex();
    let net = Canned::new().file("plex/plex-albums-rated.json");
    let listed = album_ratings(&net, &server, &key, 10, later()).unwrap();
    assert!(listed.whole);
    let values: Vec<(&str, f64)> = listed.found.iter().map(|r| (r.id.as_str(), r.value)).collect();
    assert_eq!(values, [("16", 7.0), ("6", 10.0), ("2", 0.0)]);
    assert!(listed.found.iter().all(|r| r.rated == Rated::Album));
    let asked = net.asked();
    assert!(asked[0].url.ends_with("/library/sections/1/all?type=9"), "{}", asked[0].url);
    assert!(asked[0].headers.iter().any(|(k, v)| k == "X-Plex-Container-Size" && v == "250"));
}

#[test]
fn a_records_listing_stops_at_its_budget_and_says_it_is_not_whole() {
    let (server, key) = plex();
    let page = |offset: usize| format!(
        r#"{{"MediaContainer":{{"size":1,"totalSize":3,"offset":{offset},"Metadata":[{{"ratingKey":"{}","type":"album","userRating":4.0}}]}}}}"#, offset + 1);
    let net = Canned::new().answer(200, page(0).as_bytes()).answer(200, page(1).as_bytes()).answer(200, page(2).as_bytes());
    let listed = album_ratings(&net, &server, &key, 2, later()).unwrap();
    assert!(!listed.whole, "the third page was not asked for");
    assert_eq!(listed.found.len(), 2);
    assert_eq!(net.asked().len(), 2);
    let none = album_ratings(&Canned::new(), &server, &key, 10, Instant::now()).unwrap();
    assert_eq!(none, Listed { found: Vec::new(), whole: false }, "out of time before the first request");
}

#[test]
fn a_rating_written_as_text_or_below_zero_reads_sensibly() {
    for (item, value) in [
        (serde_json::json!({"userRating": 7}), 7.0),
        (serde_json::json!({"userRating": "6.0"}), 6.0),
        (serde_json::json!({"userRating": -1}), 0.0),
        (serde_json::json!({"userRating": null}), 0.0),
        (serde_json::json!({}), 0.0),
    ] {
        assert_eq!(user_rating(&item), value, "{item}");
    }
    assert_eq!(number_text(8.0), "8");
    assert_eq!(number_text(7.5), "7.5");
}

#[test]
fn what_a_server_holds_is_read_back_with_the_library_it_is_in() {
    let (server, key) = plex();
    let net = Canned::new().file("plex/plex-metadata.json");
    let found = held(&net, &server, &key, Rated::Track, "17").unwrap();
    assert_eq!(found, Held { value: 0.0, library: Some("Music".into()) }, "the library a write would land in");
    assert!(net.asked()[0].url.ends_with("/library/metadata/17"));
    let (server, key) = subsonic();
    let net = Canned::new().file("navidrome/subsonic-album-0-rated.json");
    assert_eq!(held(&net, &server, &key, Rated::Album, "2NGalRARWUlolzE5BntvyN").unwrap(), Held { value: 4.0, library: None });
    assert!(net.asked()[0].url.contains("/rest/getAlbum.view?"));
}
