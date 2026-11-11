//! Reading and changing items, against answers saved from the lab servers
//! (Jellyfin 12.1.0, Plex Media Server 1.43.5, 2026-11-11; trimmed to what
//! is read, with paths taken out) and a network that answers what it was
//! loaded with: nothing here leaves the machine.

use super::*;
use crate::model::Client;
use crate::net::testing::Canned;
use crate::net::Request;
use crate::subsonic;
use serde_json::Value;

fn client() -> Client {
    Client { version: "0.0.0".into(), platform: "Linux".into(), device: "desk".into(), id: "c0ffee".into() }
}

fn jellyfin() -> (Server, Credential) {
    (crate::jellyfin::server("http://jf.lan:8096", client(), Vec::new(), "u1"), Credential::Token { token: "tok".into() })
}

fn plex() -> (Server, Credential) {
    (crate::plex::server("http://pms.lan:32400", client(), vec!["12".into()], Vec::new()), Credential::Plex { account: "acct".into(), server: "srv".into() })
}

fn answer(path: &str) -> Vec<u8> {
    std::fs::read(format!("{}/tests/answers/{path}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

fn query(url: &str) -> Vec<(String, String)> {
    url.split_once('?').map_or("", |(_, q)| q).split('&').filter_map(|pair| pair.split_once('=')).map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

fn param(url: &str, name: &str) -> Option<String> {
    query(url).into_iter().find(|(k, _)| k == name).map(|(_, v)| v)
}

fn no_delete(asked: &[Request]) {
    assert!(asked.iter().all(|r| r.method != "DELETE"), "a change never deletes: {:?}", asked.iter().map(|r| r.method).collect::<Vec<_>>());
}

#[test]
fn only_plex_and_jellyfin_take_changes() {
    for kind in [Kind::Subsonic, Kind::Dlna] {
        for entity in [Entity::Album, Entity::Track, Entity::Artist] {
            assert!(kind.editable(entity).is_empty(), "{kind:?} {entity:?}");
            assert!(!kind.takes_images(entity));
        }
        assert!(!kind.not_editable().is_empty(), "{kind:?} says why");
    }
    assert!(Kind::Plex.editable(Entity::Album).contains(&"label"));
    assert!(!Kind::Plex.editable(Entity::Album).contains(&"album_artist"), "a Plex record's artist is the artist it hangs from");
    assert!(Kind::Jellyfin.editable(Entity::Album).contains(&"album_artist"));
    assert!(Kind::Plex.takes_images(Entity::Album) && Kind::Jellyfin.takes_images(Entity::Artist));
    assert!(!Kind::Plex.takes_images(Entity::Track), "a Plex track takes no poster");
    assert!(takes(Kind::Plex, Entity::Track, "genres"), "Plex tracks keep genres of their own");
    assert!(!takes(Kind::Plex, Entity::Track, "album"), "Plex tracks read their record's title");
    assert!(takes(Kind::Jellyfin, Entity::Track, "album") && takes(Kind::Jellyfin, Entity::Track, "date"));
    assert!(!takes(Kind::Jellyfin, Entity::Track, "label"));
}

#[test]
fn a_server_that_takes_nothing_is_never_asked() {
    let server = subsonic::server("http://music.lan:4533", client(), Vec::new(), Vec::new());
    let key = Credential::Password { user: "lab".into(), password: "pw".into() };
    let net = Canned::new();
    assert!(matches!(may_edit(&net, &server, &key), Err(Failure::Forbidden(_))));
    let edit = Edit { title: Some("New".into()), ..Edit::default() };
    assert!(matches!(write(&net, &server, &key, Entity::Album, "a1", &edit), Err(Failure::Forbidden(_))));
    assert!(upload_image(&net, &server, &key, Entity::Album, "a1", b"\x89PNG", "image/png").is_err());
    assert!(net.asked().is_empty());
}

#[test]
fn jellyfin_says_a_record_and_a_track_in_the_editor_s_words() {
    let (server, key) = jellyfin();
    let album = held(&Canned::new().file("jellyfin/edit-album.json"), &server, &key, Entity::Album, "c29b").unwrap();
    assert_eq!(album.title, "...And Justice For All");
    assert_eq!(album.album_artist, "Metallica");
    assert_eq!(album.date, "1988-01-01", "the day Jellyfin keeps, not the year alone");
    assert_eq!(album.genres, ["Metal"]);
    assert_eq!(album.label, "Elektra");
    assert_eq!(album.ids.get(field::RELEASE).map(String::as_str), Some("4dede30f-58cf-4d43-a857-b342ad7be945"));
    assert_eq!(album.ids.get(field::RELEASE_GROUP).map(String::as_str), Some("bd14f98a-8c45-4a0d-b148-17cbc96740aa"));
    assert!(album.locked.is_empty());
    let track = held(&Canned::new().file("jellyfin/edit-track.json"), &server, &key, Entity::Track, "000c").unwrap();
    assert_eq!((track.title.as_str(), track.album.as_str(), track.artist.as_str()), ("Blackened", "...And Justice For All", "Metallica"));
    assert_eq!((track.track_number, track.disc_number), (Some(1), Some(1)));
}

#[test]
fn jellyfin_takes_the_whole_item_back_with_the_change_and_its_locks() {
    let (server, key) = jellyfin();
    let net = Canned::new().file("jellyfin/edit-track.json").answer(204, b"");
    let edit = Edit {
        title: Some("Blackened (live)".into()),
        artists: Some(vec!["Metallica".into(), "Guest".into()]),
        track_number: Some(Some(7)),
        genres: Some(vec!["Thrash".into()]),
        date: Some("1989".into()),
        ids: [(field::RECORDING.to_string(), "3d3207f9-c4c2-425c-a364-f359b4320805".to_string())].into(),
        ..Edit::default()
    };
    write(&net, &server, &key, Entity::Track, "000c", &edit).unwrap();
    let asked = net.asked();
    assert_eq!(asked.iter().map(|r| r.method).collect::<Vec<_>>(), ["GET", "POST"], "read first, then posted whole");
    assert!(asked[0].url.ends_with("/Items/000c?userId=u1"));
    assert!(asked[1].url.ends_with("/Items/000c"));
    let (kind, body) = asked[1].body.clone().unwrap();
    assert_eq!(kind, "application/json");
    let posted: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(posted["Name"], "Blackened (live)");
    assert_eq!(posted["Artists"], serde_json::json!(["Metallica", "Guest"]));
    assert_eq!(posted["ArtistItems"][1]["Name"], "Guest");
    assert_eq!(posted["IndexNumber"], 7);
    assert_eq!(posted["Genres"], serde_json::json!(["Thrash"]));
    assert_eq!((posted["ProductionYear"].as_u64(), posted["PremiereDate"].is_null()), (Some(1989), true), "a year alone leaves no day");
    assert_eq!(posted["ProviderIds"]["MusicBrainzRecording"], "3d3207f9-c4c2-425c-a364-f359b4320805");
    // What was not asked for is posted as it was read, so nothing is cleared.
    assert_eq!(posted["Album"], "...And Justice For All");
    assert_eq!(posted["ParentIndexNumber"], 1);
    assert_eq!(posted["LockData"], false);
    let locks: Vec<&str> = posted["LockedFields"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
    assert_eq!(locks, ["Name", "Genres", "Cast"], "each field written that has a lock gets it");
    no_delete(&asked);
}

#[test]
fn jellyfin_clears_what_is_emptied() {
    let (server, key) = jellyfin();
    let net = Canned::new().file("jellyfin/edit-album.json").answer(204, b"");
    let edit = Edit { label: Some(String::new()), date: Some(String::new()), ids: [(field::RELEASE.to_string(), String::new())].into(), ..Edit::default() };
    write(&net, &server, &key, Entity::Album, "c29b", &edit).unwrap();
    let posted: Value = serde_json::from_slice(&net.asked()[1].body.clone().unwrap().1).unwrap();
    assert_eq!(posted["Studios"], serde_json::json!([]));
    assert!(posted["ProductionYear"].is_null() && posted["PremiereDate"].is_null());
    assert!(posted["ProviderIds"].get("MusicBrainzAlbum").is_none());
    assert_eq!(posted["ProviderIds"]["MusicBrainzReleaseGroup"], "bd14f98a-8c45-4a0d-b148-17cbc96740aa", "the other ids stay");
}

#[test]
fn jellyfin_lets_only_an_administrator_change_items() {
    let (server, key) = jellyfin();
    let admin = Canned::new().answer(200, br#"{"Name":"lab","Policy":{"IsAdministrator":true}}"#);
    assert!(may_edit(&admin, &server, &key).is_ok());
    assert!(admin.asked()[0].url.ends_with("/Users/Me"));
    let user = Canned::new().answer(200, br#"{"Name":"guest","Policy":{"IsAdministrator":false}}"#);
    assert!(matches!(may_edit(&user, &server, &key), Err(Failure::Forbidden(_))));
    let refused = Canned::new().file("jellyfin/edit-album.json").answer(403, b"");
    let edit = Edit { title: Some("x".into()), ..Edit::default() };
    assert!(matches!(write(&refused, &server, &key, Entity::Album, "c29b", &edit), Err(Failure::Forbidden(_))));
}

#[test]
fn jellyfin_takes_an_image_as_base64() {
    assert_eq!(jellyfin::base64(b""), "");
    assert_eq!(jellyfin::base64(b"f"), "Zg==");
    assert_eq!(jellyfin::base64(b"fo"), "Zm8=");
    assert_eq!(jellyfin::base64(b"foo"), "Zm9v");
    assert_eq!(jellyfin::base64(b"foobar"), "Zm9vYmFy");
    assert_eq!(jellyfin::base64(&[0xFF, 0xD8, 0xFF, 0xE0]), "/9j/4A==");
    let (server, key) = jellyfin();
    let net = Canned::new().answer(204, b"");
    upload_image(&net, &server, &key, Entity::Album, "c29b", &[0x89, b'P', b'N', b'G'], "image/png").unwrap();
    let asked = net.asked();
    assert_eq!(asked[0].method, "POST");
    assert!(asked[0].url.ends_with("/Items/c29b/Images/Primary"));
    assert_eq!(asked[0].body, Some(("image/png".into(), b"iVBORw==".to_vec())));
}

#[test]
fn an_image_that_cannot_be_one_is_never_sent() {
    let (server, key) = jellyfin();
    let net = Canned::new();
    assert!(upload_image(&net, &server, &key, Entity::Album, "c29b", b"", "image/png").is_err());
    assert!(upload_image(&net, &server, &key, Entity::Album, "c29b", b"GIF89a", "image/gif").is_err());
    assert!(upload_image(&net, &server, &key, Entity::Album, "c29b", &vec![0; IMAGE_BYTES + 1], "image/png").is_err());
    assert!(upload_image(&net, &server, &key, Entity::Track, "t1", b"\x89PNG", "image/png").is_err(), "a track takes no image");
    assert!(net.asked().is_empty());
}

#[test]
fn plex_says_a_record_and_a_track_in_the_editor_s_words() {
    let (server, key) = plex();
    let album = held(&Canned::new().file("plex/edit-album.json"), &server, &key, Entity::Album, "98777").unwrap();
    assert_eq!((album.title.as_str(), album.album_artist.as_str()), ("Back in Black", "AC/DC"));
    assert_eq!(album.date, "1980-07-25");
    assert_eq!(album.label, "Epic");
    assert_eq!(album.genres, ["Pop/Rock"]);
    let track = held(&Canned::new().file("plex/edit-track.json"), &server, &key, Entity::Track, "98778").unwrap();
    assert_eq!((track.title.as_str(), track.album.as_str()), ("Hells Bells", "Back in Black"));
    assert_eq!(track.artist, "AC/DC", "without its own artist a track is its record's");
    assert_eq!(track.date, "1980", "a track carries its record's year");
    assert_eq!((track.track_number, track.disc_number), (Some(1), Some(1)));
    assert_eq!(track.ids.get(field::RECORDING).map(String::as_str), Some("1342ecee-79f1-3477-adcd-9ab5ff7a6a4c"));
}

#[test]
fn plex_takes_each_field_on_its_library_s_item_and_locks_it() {
    let (server, key) = plex();
    let net = Canned::new().file("plex/edit-album.json").answer(200, b"").answer(200, b"");
    let edit = Edit {
        title: Some("Back in Black (Remaster)".into()),
        date: Some("1980".into()),
        label: Some("Atlantic".into()),
        genres: Some(vec!["Hard Rock".into(), "pop/rock".into()]),
        ..Edit::default()
    };
    write(&net, &server, &key, Entity::Album, "98777", &edit).unwrap();
    let asked = net.asked();
    assert_eq!(asked.iter().map(|r| r.method).collect::<Vec<_>>(), ["GET", "PUT"], "nothing to take off: one change");
    let url = &asked[1].url;
    assert!(url.starts_with("http://pms.lan:32400/library/sections/12/all?"), "{url}");
    assert_eq!(param(url, "type").as_deref(), Some("9"));
    assert_eq!(param(url, "id").as_deref(), Some("98777"));
    assert_eq!(param(url, "title.value").as_deref(), Some("Back%20in%20Black%20%28Remaster%29"));
    assert_eq!(param(url, "title.locked").as_deref(), Some("1"));
    assert_eq!(param(url, "artist.id.value").as_deref(), Some("98667"), "a record's new title names its artist");
    assert_eq!(param(url, "year.value").as_deref(), Some("1980"));
    assert_eq!(param(url, "originallyAvailableAt.value").as_deref(), Some(""), "a year alone empties the day");
    assert_eq!(param(url, "studio.value").as_deref(), Some("Atlantic"));
    assert_eq!(param(url, "genre[0].tag.tag").as_deref(), Some("Hard%20Rock"), "only what it does not have is added");
    assert_eq!(param(url, "genre[1].tag.tag"), None);
    assert_eq!(param(url, "genre.locked").as_deref(), Some("1"));
    no_delete(&asked);
}

#[test]
fn plex_takes_off_the_genres_a_new_list_leaves_out_first() {
    let (server, key) = plex();
    let net = Canned::new().file("plex/edit-track.json").answer(200, b"").answer(200, b"");
    let edit = Edit { genres: Some(vec!["Hard Rock".into()]), title: Some("Hells Bells (live)".into()), artist: Some("AC/DC & Guest".into()), ..Edit::default() };
    write(&net, &server, &key, Entity::Track, "98778", &edit).unwrap();
    let asked = net.asked();
    assert_eq!(asked.iter().map(|r| r.method).collect::<Vec<_>>(), ["GET", "PUT", "PUT"]);
    assert_eq!(param(&asked[1].url, "genre[].tag.tag-").as_deref(), Some("Pop%2FRock"));
    assert_eq!(param(&asked[1].url, "type").as_deref(), Some("10"));
    assert_eq!(param(&asked[2].url, "genre[0].tag.tag").as_deref(), Some("Hard%20Rock"));
    assert_eq!(param(&asked[2].url, "originalTitle.value").as_deref(), Some("AC%2FDC%20%26%20Guest"), "a track's artist is its original title");
    assert_eq!(param(&asked[2].url, "artist.id.value"), None, "only a record's title names an artist");
    let emptied = Canned::new().file("plex/edit-track.json").answer(200, b"").answer(200, b"");
    write(&emptied, &server, &key, Entity::Track, "98778", &Edit { genres: Some(Vec::new()), ..Edit::default() }).unwrap();
    let asked = emptied.asked();
    assert_eq!(param(&asked[1].url, "genre[].tag.tag-").as_deref(), Some("Pop%2FRock"));
    assert!(query(&asked[2].url).iter().all(|(k, _)| !k.starts_with("genre[")));
    no_delete(&asked);
}

#[test]
fn a_field_the_server_does_not_take_is_refused_before_anything_is_sent() {
    let (server, key) = plex();
    let net = Canned::new();
    let edit = Edit { album_artist: Some("Someone".into()), ..Edit::default() };
    assert!(matches!(write(&net, &server, &key, Entity::Album, "98777", &edit), Err(Failure::Unexpected(_))));
    let edit = Edit { album: Some("Other".into()), ..Edit::default() };
    assert!(write(&net, &server, &key, Entity::Track, "98778", &edit).is_err(), "a Plex track reads its record's title");
    let edit = Edit { ids: [(field::RELEASE.to_string(), "x".to_string())].into(), ..Edit::default() };
    assert!(write(&net, &server, &key, Entity::Album, "98777", &edit).is_err(), "Plex keeps no MusicBrainz id Chilpa can set");
    assert!(net.asked().is_empty());
    assert!(write(&net, &server, &key, Entity::Album, "98777", &Edit::default()).is_ok(), "nothing to change asks nothing");
    assert!(net.asked().is_empty());
}

#[test]
fn plex_adds_a_poster_and_says_why_it_cannot() {
    let (server, key) = plex();
    let net = Canned::new().answer(200, b"");
    upload_image(&net, &server, &key, Entity::Album, "98777", &[0xFF, 0xD8, 0xFF], "image/jpeg").unwrap();
    let asked = net.asked();
    assert_eq!(asked[0].method, "POST");
    assert!(asked[0].url.ends_with("/library/metadata/98777/posters"));
    assert_eq!(asked[0].body, Some(("image/jpeg".into(), vec![0xFF, 0xD8, 0xFF])), "the image's own bytes");
    let refused = Canned::new().answer(401, b"");
    assert!(matches!(upload_image(&refused, &server, &key, Entity::Album, "98777", &[0xFF, 0xD8, 0xFF], "image/jpeg"), Err(Failure::Forbidden(_))));
    let gone = Canned::new().answer(404, b"");
    assert!(matches!(upload_image(&gone, &server, &key, Entity::Album, "98777", &[0xFF, 0xD8, 0xFF], "image/jpeg"), Err(Failure::Gone(_))));
}

#[test]
fn no_plex_request_this_makes_is_a_delete() {
    let (server, key) = plex();
    let net = Canned::new()
        .file("plex/edit-album.json")
        .file("plex/edit-album.json")
        .answer(200, b"")
        .answer(200, b"")
        .file("plex/edit-track.json")
        .answer(200, b"")
        .answer(200, b"")
        .answer(200, b"")
        .file("plex/edit-track.json")
        .file("plex/edit-album.json");
    assert!(may_edit(&net, &server, &key).is_ok());
    held(&net, &server, &key, Entity::Album, "98777").unwrap();
    let every = Edit { title: Some("t".into()), date: Some("2000-01-02".into()), genres: Some(Vec::new()), label: Some(String::new()), ..Edit::default() };
    write(&net, &server, &key, Entity::Album, "98777", &every).unwrap();
    let track = Edit { title: Some(String::new()), artist: Some(String::new()), track_number: Some(None), disc_number: Some(None), genres: Some(vec!["g".into()]), ..Edit::default() };
    write(&net, &server, &key, Entity::Track, "98778", &track).unwrap();
    upload_image(&net, &server, &key, Entity::Album, "98777", &[0xFF, 0xD8, 0xFF], "image/jpeg").unwrap();
    held_tracks(&net, &server, &key, &["98778".to_string()]).unwrap();
    let asked = net.asked();
    assert!(asked.len() >= 8, "{} requests", asked.len());
    no_delete(&asked);
    assert!(asked.iter().filter(|r| r.method == "PUT").all(|r| r.url.contains("/library/sections/12/all?")), "every change names the item's own library");
}

#[test]
fn several_tracks_are_read_in_one_request() {
    let (server, key) = plex();
    let net = Canned::new().file("plex/edit-track.json");
    let found = held_tracks(&net, &server, &key, &["98778".to_string(), "98779".to_string()]).unwrap();
    assert_eq!(found.len(), 1, "what the server does not describe is left out");
    assert_eq!(found[0].0, "98778");
    assert!(net.asked()[0].url.contains("/library/metadata/98778,98779?"));
    let (server, key) = jellyfin();
    let list = format!("{{\"Items\":[{}],\"TotalRecordCount\":1}}", String::from_utf8(answer("jellyfin/edit-track.json")).unwrap());
    let net = Canned::new().answer(200, list.as_bytes());
    let found = held_tracks(&net, &server, &key, &["000c".to_string()]).unwrap();
    assert_eq!(found[0].1.title, "Blackened");
    assert!(net.asked()[0].url.contains("&Ids=000c&"));
}
