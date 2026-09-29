//! Against Jellyfin 12.1.0's own answers, saved on 2026-09-28 from a lab
//! server with a synthetic library (CHD-0060 §4). The token in them was
//! replaced.

use super::*;
use crate::net::testing::Canned;

const AT: &str = "http://jf.lan:8096";

fn client() -> Client {
    Client { version: "0.1.0".into(), platform: "Linux".into(), device: "my desk".into(), id: "4f1c9a0b2d3e4f50".into() }
}

fn lab() -> (Server, Credential) {
    (server(AT, client(), Vec::new(), "501512e000854c8d832d2f1d2a26a0a0"), Credential::Token { token: "tok en".into() })
}

fn header(request: &Request) -> String {
    request.headers.iter().find(|(k, _)| k == "Authorization").map(|(_, v)| v.clone()).unwrap_or_default()
}

#[test]
fn a_probe_reads_the_server_s_id_and_whether_quick_connect_is_on() {
    let net = Canned::new().file("jellyfin/jellyfin-info.json").file("jellyfin/jellyfin-quickconnect-enabled.json");
    let found = probe(&net, AT).unwrap();
    assert_eq!(found.product, "Jellyfin 12.1.0");
    assert_eq!(found.identity, "168793d734f84a58916610a7d69470be");
    assert!(found.quick_connect);
}

#[test]
fn an_emby_server_is_told_apart() {
    let net = Canned::new().answer(200, br#"{"ServerName":"x","Version":"4.9","ProductName":"Emby Server","Id":"abc"}"#);
    assert!(matches!(probe(&net, AT), Err(Failure::Unexpected(why)) if why.contains("Emby")));
}

#[test]
fn signing_in_keeps_the_token_and_the_user_and_sends_the_device_it_is() {
    let net = Canned::new().file("jellyfin/jellyfin-auth.json");
    let signed = sign_in(&net, AT, &client(), "lab", "lab-password").unwrap();
    assert_eq!(signed.key, Credential::Token { token: "0123456789abcdef0123456789abcdef".into() });
    assert_eq!(signed.user_id, "501512e000854c8d832d2f1d2a26a0a0");
    assert_eq!(signed.account, "lab");
    let asked = &net.asked()[0];
    assert_eq!(asked.method, "POST");
    assert_eq!(header(asked), r#"MediaBrowser Client="Chilpa", Device="my%20desk", DeviceId="4f1c9a0b2d3e4f50", Version="0.1.0""#);
    assert!(!header(asked).contains("Token="), "no token before there is one");
}

#[test]
fn a_wrong_password_and_an_expired_token_are_refusals() {
    let net = Canned::new().answer(401, b"");
    assert!(matches!(sign_in(&net, AT, &client(), "lab", "nope"), Err(Failure::Refused(_))));
    let (server, key) = lab();
    let net = Canned::new().answer(401, b"");
    assert!(matches!(ping(&net, &server, &key), Err(Failure::Refused(_))));
}

#[test]
fn quick_connect_goes_code_then_wait_then_token() {
    let net = Canned::new()
        .answer(200, br#"{"Authenticated":false,"Secret":"s3cr3t","Code":"123456"}"#)
        .answer(200, br#"{"Authenticated":false}"#)
        .answer(200, br#"{"Authenticated":true}"#)
        .file("jellyfin/jellyfin-auth.json");
    assert_eq!(quick_connect_start(&net, AT, &client()).unwrap(), ("s3cr3t".into(), "123456".into()));
    assert!(!quick_connect_ready(&net, AT, "s3cr3t").unwrap());
    assert!(quick_connect_ready(&net, AT, "s3cr3t").unwrap());
    let signed = quick_connect_finish(&net, AT, &client(), "s3cr3t").unwrap();
    assert_eq!(signed.user_id, "501512e000854c8d832d2f1d2a26a0a0");
    let asked = net.asked();
    assert_eq!(asked[0].method, "POST", "12.0 removed the GET form");
    assert!(header(&asked[0]).contains("DeviceId="), "Initiate needs the four fields");
    let net = Canned::new().answer(404, b"Unknown secret");
    assert!(matches!(quick_connect_ready(&net, AT, "old"), Err(Failure::Refused(_))));
}

#[test]
fn its_music_views_are_its_libraries() {
    let (server, key) = lab();
    let net = Canned::new().file("jellyfin/jellyfin-views.json");
    let found = libraries(&net, &server, &key).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].name, "Music");
    assert!(header(&net.asked()[0]).ends_with(r#"Token="tok%20en""#));
}

#[test]
fn the_listing_has_everything_but_the_quality_and_its_etag_says_when_it_changed() {
    let (mut server, key) = lab();
    server.libraries = vec!["view-1".into()];
    let net = Canned::new().file("jellyfin/jellyfin-items.json");
    let page = page(&net, &server, &key, None).unwrap();
    assert_eq!(page.tracks.len(), 12);
    assert_eq!(page.total, Some(12));
    assert_eq!(page.next, None);
    assert!(page.tracks.iter().all(|t| !t.complete && t.revision.is_some()), "the quality comes from details");
    let first = page.tracks.iter().find(|t| t.title == "First Song 1").unwrap();
    assert_eq!(first.recording_mbid.as_deref(), Some("22222221-2222-4222-8222-222222222222"), "MusicBrainzRecording, as 12.1 writes it");
    assert_eq!((first.number, first.disc, first.year, first.duration_ms), (Some(1), Some(1), Some(2001), 12_000));
    // Jellyfin 12.1 keeps a tag's "Rock;Pop" whole. The library splits it
    // when it stores it (`chilpa_library::split_genres`), as for a file.
    assert_eq!(first.genres, vec!["Rock;Pop".to_string()]);
    let compiled = page.tracks.iter().find(|t| t.title == "Comp Track 1").unwrap();
    assert_eq!((compiled.artist.as_str(), compiled.album_artist.as_str()), ("Singer Uno", "Various Artists"));
    assert!(compiled.cover.as_deref().unwrap().starts_with(&compiled.album_id), "the record's image");
    assert!(net.asked()[0].url.contains("parentId=view-1"));
}

#[test]
fn a_long_view_pages_by_the_total_it_announces() {
    let (mut server, key) = lab();
    server.libraries = vec!["a".into(), "b".into()];
    let mut body: Value = serde_json::from_slice(&std::fs::read(format!("{}/tests/answers/jellyfin/jellyfin-items.json", env!("CARGO_MANIFEST_DIR"))).unwrap()).unwrap();
    body["TotalRecordCount"] = serde_json::json!(600);
    let net = Canned::new().answer(200, body.to_string().as_bytes());
    let first = page(&net, &server, &key, None).unwrap();
    let next: Value = serde_json::from_str(first.next.as_deref().unwrap()).unwrap();
    assert_eq!((next["view"].as_u64(), next["offset"].as_u64()), (Some(0), Some(12)));
}

#[test]
fn details_carry_the_quality_of_the_audio_stream_not_the_cover_s() {
    let (server, key) = lab();
    let net = Canned::new().file("jellyfin/jellyfin-details.json");
    let found = details(&net, &server, &key, &["a b".into(), "c".into()]).unwrap();
    assert!(!found.is_empty() && found.iter().all(|t| t.complete));
    let alac = found.iter().find(|t| t.title == "Comp Track 1").unwrap();
    assert_eq!((alac.codec.as_deref(), alac.bits, alac.rate, alac.channels), (Some("alac"), Some(16), Some(48_000), Some(2)));
    assert!(net.asked()[0].url.contains("Ids=a%20b,c"));
}

#[test]
fn a_stream_is_the_static_file_with_the_session_in_its_header() {
    let (server, key) = lab();
    let stream = stream(&server, &key, "5d0a04cb", Some("alac"));
    assert_eq!(stream.url, "http://jf.lan:8096/Audio/5d0a04cb/stream?static=true");
    assert!(stream.headers.iter().any(|(k, v)| k == "Authorization" && v.contains("Token=")));
    assert_eq!(stream.extension.as_deref(), Some("m4a"));
    let cover = cover(&server, &key, "e910/5188c5", 1200);
    assert_eq!(cover.url, "http://jf.lan:8096/Items/e910/Images/Primary?maxWidth=1200&quality=90&tag=5188c5");
}
