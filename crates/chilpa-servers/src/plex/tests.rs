//! Against Plex Media Server 1.43.4's own answers, saved on 2026-09-28 from
//! a lab server with a synthetic library, and plex.tv's answer to a PIN
//! request (its location removed). `resources` was not captured: its shape
//! is the one Plex documents (CHD-0060 §3.4), and the one a real account
//! gave on 2026-09-28 with only `plex.direct` names (addresses changed).

use super::*;
use crate::net::testing::Canned;

const AT: &str = "http://192.168.0.69:32400";

fn client() -> Client {
    Client { version: "0.1.0".into(), platform: "Linux".into(), device: "desk".into(), id: "9e2b7c1d4a6f8e30".into() }
}

fn lab() -> (Server, Credential) {
    (server(AT, client(), vec!["1".into()], Vec::new()), Credential::Plex { account: "acct".into(), server: "srv".into() })
}

fn header<'a>(request: &'a Request, name: &str) -> Option<&'a str> {
    request.headers.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
}

#[test]
fn a_pin_is_asked_for_short_and_waited_for_until_confirmed() {
    let net = Canned::new()
        .file("plex/plex-pin.json")
        .file("plex/plex-pin.json")
        .answer(200, br#"{"id":1725899155,"code":"YTNQ","authToken":"account-token"}"#)
        .answer(404, br#"{"errors":[{"code":1020,"message":"Code not found or expired","status":404}]}"#);
    let pin = pin_start(&net, &client()).unwrap();
    assert_eq!(pin, Pin { id: 1_725_899_155, code: "YTNQ".into() });
    assert_eq!(pin_token(&net, &client(), pin.id).unwrap(), None, "not confirmed yet");
    assert_eq!(pin_token(&net, &client(), pin.id).unwrap().as_deref(), Some("account-token"));
    assert!(matches!(pin_token(&net, &client(), pin.id), Err(Failure::Refused(_))), "expired");
    let asked = net.asked();
    assert_eq!(asked[0].method, "POST");
    assert!(asked[0].url.ends_with("strong=false"), "four characters, to type at plex.tv/link");
    assert_eq!(header(&asked[0], "X-Plex-Client-Identifier"), Some("9e2b7c1d4a6f8e30"));
    assert_eq!(header(&asked[0], "X-Plex-Product"), Some("Chilpa"));
}

#[test]
fn only_servers_are_offered_with_their_direct_addresses_local_first() {
    let resources = br#"[
        {"name":"BFC-Plex","product":"Plex Media Server","productVersion":"1.43.3","provides":"server","owned":true,
         "clientIdentifier":"abc","accessToken":"server-token","connections":[
            {"protocol":"https","address":"64.71.188.222","port":32403,"uri":"https://64-71-188-222.abc.plex.direct:32403","local":false,"relay":false,"IPv6":false},
            {"protocol":"https","address":"139.162.158.105","port":8443,"uri":"https://139-162-158-105.abc.plex.direct:8443","local":false,"relay":true,"IPv6":false},
            {"protocol":"http","address":"192.168.0.69","port":32400,"uri":"http://192.168.0.69:32400","local":true,"relay":false,"IPv6":false},
            {"protocol":"https","address":"192.168.0.69","port":32400,"uri":"https://192-168-0-69.abc.plex.direct:32400","local":true,"relay":false,"IPv6":false}]},
        {"name":"Phone","product":"Plexamp","provides":"player,pubsub-player","clientIdentifier":"phone","connections":[]}
    ]"#;
    let net = Canned::new().answer(200, resources);
    let found = servers(&net, &client(), "account-token").unwrap();
    assert_eq!(found.len(), 1, "a player is not a server");
    assert_eq!(found[0].token, "server-token", "each server is talked to with its own token");
    assert_eq!(found[0].connections, vec![
        "https://192-168-0-69.abc.plex.direct:32400".to_string(),
        "http://192.168.0.69:32400".to_string(),
        "https://64-71-188-222.abc.plex.direct:32403".to_string(),
    ], "local first, https first, and never the relay");
    assert!(net.asked()[0].url.contains("includeRelay=0"));
}

#[test]
fn a_server_without_its_own_token_is_not_offered_and_odd_addresses_are_dropped() {
    let resources = br#"[
        {"name":"NoToken","provides":"server","owned":true,"clientIdentifier":"a","connections":[
            {"protocol":"https","address":"192.168.0.2","port":32400,"uri":"https://192-168-0-2.a.plex.direct:32400","local":true,"relay":false}]},
        {"name":"Shared","provides":"server","owned":false,"httpsRequired":false,"clientIdentifier":"b","accessToken":"shared-token","connections":[
            {"protocol":"http","address":"127.0.0.1","port":22,"uri":"http://127.0.0.1:22","local":true,"relay":false},
            {"protocol":"http","address":"169.254.169.254","port":80,"uri":"http://169.254.169.254:80","local":true,"relay":false},
            {"protocol":"https","address":"example.org","port":443,"uri":"https://user:pw@example.org:443","local":false,"relay":false},
            {"protocol":"https","address":"example.org","port":443,"uri":"https://example.org:443/admin?x=1","local":false,"relay":false},
            {"protocol":"https","address":"203.0.113.9","port":32400,"uri":"https://203-0-113-9.b.plex.direct:32400/","local":false,"relay":false}]}
    ]"#;
    let net = Canned::new().answer(200, resources);
    let found = servers(&net, &client(), "account-token").unwrap();
    assert_eq!(found.len(), 1, "the one without a token of its own is left out");
    assert_eq!(found[0].token, "shared-token");
    assert_eq!(found[0].connections, vec!["https://203-0-113-9.b.plex.direct:32400".to_string()], "only a scheme, a host and a port; nothing on this machine");
}

#[test]
fn a_server_that_allows_http_is_also_tried_at_its_local_address() {
    // What a real account answered: only plex.direct names, over https.
    let resources = |required: bool| format!(r#"[
        {{"name":"BFC-Plex","productVersion":"1.43.4","provides":"server","owned":true,"httpsRequired":{required},
         "clientIdentifier":"abc","accessToken":"server-token","connections":[
            {{"protocol":"https","address":"64.71.188.222","port":32400,"uri":"https://64-71-188-222.abc.plex.direct:32400","local":false,"relay":false,"IPv6":false}},
            {{"protocol":"https","address":"192.168.0.69","port":32400,"uri":"https://192-168-0-69.abc.plex.direct:32400","local":true,"relay":false,"IPv6":false}},
            {{"protocol":"https","address":"fd00::69","port":32400,"uri":"https://fd00--69.abc.plex.direct:32400","local":true,"relay":false,"IPv6":true}}]}}
    ]"#);
    let net = Canned::new().answer(200, resources(false).as_bytes());
    assert_eq!(servers(&net, &client(), "account-token").unwrap()[0].connections, vec![
        "https://192-168-0-69.abc.plex.direct:32400".to_string(),
        "https://fd00--69.abc.plex.direct:32400".to_string(),
        "http://192.168.0.69:32400".to_string(),
        "http://[fd00::69]:32400".to_string(),
        "https://64-71-188-222.abc.plex.direct:32400".to_string(),
    ], "https first; plain http only on the local network; never outside it");
    let net = Canned::new().answer(200, resources(true).as_bytes());
    assert!(servers(&net, &client(), "account-token").unwrap()[0].connections.iter().all(|c| c.starts_with("https://")), "a server that requires https is only reached over https");
}

#[test]
fn the_address_used_is_the_first_that_answers_as_that_server() {
    let net = Canned::new()
        .silence()
        .answer(200, br#"{"MediaContainer":{"machineIdentifier":"someone-else"}}"#)
        .file("plex/plex-identity.json");
    let tried = ["https://a.plex.direct:32400".to_string(), "http://b:32400".to_string(), AT.to_string()];
    assert_eq!(reach(&net, &client(), "d4e455caa1f2f6c1ba9700919c63c6a4655700e0", &tried).unwrap(), AT);
    let net = Canned::new().silence();
    assert!(reach(&net, &client(), "x", &tried[..1]).unwrap_err().is_unreachable());
}

#[test]
fn only_music_sections_are_its_libraries() {
    let (server, key) = lab();
    let mut body: Value = serde_json::from_slice(&std::fs::read(format!("{}/tests/answers/plex/plex-sections.json", env!("CARGO_MANIFEST_DIR"))).unwrap()).unwrap();
    body["MediaContainer"]["Directory"].as_array_mut().unwrap().push(serde_json::json!({"key": "2", "type": "movie", "title": "Movies"}));
    let net = Canned::new().answer(200, body.to_string().as_bytes());
    assert_eq!(libraries(&net, &server, &key).unwrap(), vec![Library { id: "1".into(), name: "Music".into() }]);
    assert_eq!(header(&net.asked()[0], "X-Plex-Token"), Some("srv"), "the server's token, not the account's");
}

#[test]
fn the_listing_names_the_performer_of_a_compilation_and_waits_for_details_for_quality() {
    let (server, key) = lab();
    let net = Canned::new().file("plex/plex-tracks.json");
    let page = page(&net, &server, &key, None).unwrap();
    assert_eq!((page.tracks.len(), page.total, page.next.as_ref()), (12, Some(12), None));
    let compiled = page.tracks.iter().find(|t| t.title == "Comp Track 1").unwrap();
    assert_eq!((compiled.artist.as_str(), compiled.album_artist.as_str()), ("Singer Uno", "Various Artists"));
    let first = page.tracks.iter().find(|t| t.title == "First Song 1").unwrap();
    assert!(!first.complete && first.bits.is_none(), "no bit depth in a listing");
    assert!(first.revision.as_deref().unwrap().starts_with("/library/parts/"), "the part's key is its revision");
    assert_eq!((first.number, first.disc, first.year, first.duration_ms), (Some(1), Some(1), Some(2001), 12_000));
    let asked = &net.asked()[0];
    assert_eq!(header(asked, "X-Plex-Container-Size"), Some("250"));
    assert!(asked.url.contains("/library/sections/1/all?type=10"));
}

#[test]
fn details_come_for_many_tracks_in_one_request() {
    let (server, key) = lab();
    let net = Canned::new().file("plex/plex-metadata.json");
    let found = details(&net, &server, &key, &["17".into(), "18".into()]).unwrap();
    assert_eq!(found.len(), 12);
    assert!(found.iter().all(|t| t.complete));
    let hires = found.iter().find(|t| t.title == "Hi-Res D1 T1").unwrap();
    assert_eq!((hires.codec.as_deref(), hires.bits, hires.rate, hires.channels), (Some("flac"), Some(24), Some(96_000), Some(2)));
    let opus = found.iter().find(|t| t.title == "Canción ñ 日本").unwrap();
    assert_eq!((opus.codec.as_deref(), opus.bits), (Some("opus"), None));
    assert!(net.asked()[0].url.contains("/library/metadata/17,18?"));
}

#[test]
fn a_stream_is_the_part_s_file_with_the_server_s_token() {
    let (server, key) = lab();
    let stream = stream(&server, &key, "17", Some("flac"), Some("/library/parts/10/1790632420/file.flac"));
    assert_eq!(stream.url, "http://192.168.0.69:32400/library/parts/10/1790632420/file.flac?download=1");
    assert!(stream.headers.iter().any(|(k, v)| k == "X-Plex-Token" && v == "srv"));
    assert!(!stream.url.contains("srv"), "the token is not in the address");
    let cover = cover(&server, &key, "/library/metadata/16/thumb/1790634421", 1200);
    assert!(cover.url.contains("width=1200") && cover.url.contains("url=%2Flibrary%2Fmetadata%2F16"));
}
