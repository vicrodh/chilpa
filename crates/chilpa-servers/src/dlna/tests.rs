//! Against the answers of Plex Media Server 1.43.5's DLNA side, captured on
//! 2026-11-03 from a lab server: the envelope and its escaping are its own,
//! rebuilt byte for byte from a capture; titles, ids and names are made up.
//! The `handmade-*` answers are written by hand for what no lab server here
//! gave: a server that searches, references, converted and foreign `res`,
//! an undeclared prefix and a SOAP fault.

use super::*;
use crate::net::testing::Canned;

const PLEX: &str = "http://192.168.0.69:32469/DeviceDescription.xml";
const PLEX_CONTROL: &str = "/ContentDirectory/f0e1d2c3-b4a5-4697-8879-6a5b4c3d2e1f/control.xml";
const MINI: &str = "http://192.168.0.20:8200/rootDesc.xml";
const BY_ALBUM: &str = "9f4b4647f8e03ff196aa";
const RECORD: &str = "f3e534d429a852d0bd40";
const FOLDER: &str = "551c51ca11833b35b735";

fn client() -> Client {
    Client {
        version: "0.1.0".into(),
        platform: "Linux".into(),
        device: "desk".into(),
        id: "9e2b7c1d4a6f8e30".into(),
    }
}

fn lab(location: &str, control: &str, search: bool, libraries: &[&str]) -> Server {
    let details = Details {
        control: control.into(),
        search,
        ..Details::default()
    };
    server(
        location,
        client(),
        libraries.iter().map(|l| l.to_string()).collect(),
        details,
    )
}

fn plex(libraries: &[&str]) -> Server {
    lab(PLEX, PLEX_CONTROL, false, libraries)
}

fn saved(name: &str) -> String {
    let path = format!("{}/tests/answers/dlna/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// A saved answer with other counts.
fn counted(answer: &str, returned: u64, total: u64) -> String {
    let start = answer.find("<NumberReturned>").expect("a count");
    let end = answer.find("<UpdateID>").expect("an update id");
    format!(
        "{}<NumberReturned>{returned}</NumberReturned><TotalMatches>{total}</TotalMatches>{}",
        &answer[..start],
        &answer[end..]
    )
}

fn header<'a>(request: &'a Request, name: &str) -> Option<&'a str> {
    request
        .headers
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.as_str())
}

fn body(request: &Request) -> String {
    String::from_utf8_lossy(&request.body.as_ref().expect("a body").1).into_owned()
}

#[test]
fn a_media_server_is_its_own_kind_without_a_sign_in() {
    assert_eq!(Kind::from_provider("dlna"), Some(Kind::Dlna));
    assert!(!Kind::Dlna.signs_in() && Kind::Plex.signs_in());
    assert_eq!(Kind::Dlna.icon("Plex Media Server 1.43.5.11079"), "dlna");
    assert_eq!(
        format!("{:?}", crate::Credential::Anonymous),
        "Credential(none)"
    );
    let before: Details = serde_json::from_str(r#"{"connections":[]}"#).unwrap();
    assert_eq!(
        (before.control.as_str(), before.search, before.folders.len()),
        ("", false, 0),
        "what a server kept before still reads"
    );
    assert_eq!(Kind::Dlna.name(), "DLNA / UPnP");
}

#[test]
fn a_media_server_is_asked_nothing_about_playlists() {
    let server = plex(&[]);
    let key = crate::Credential::Anonymous;
    let net = Canned::new();
    assert!(!Kind::Dlna.has_playlists() && Kind::Jellyfin.has_playlists());
    let listing = crate::playlists(&net, &server, &key, "").unwrap();
    assert!(
        listing.found.is_empty() && listing.whole,
        "none, and none kept is gone"
    );
    assert!(crate::playlist_tracks(&net, &server, &key, "1").is_err());
    let change = crate::Change::Create {
        name: "Mix".into(),
        tracks: Vec::new(),
    };
    assert!(crate::change_playlist(&net, &server, &key, "", &Default::default(), &change).is_err());
    assert!(!crate::playlist_editable(&net, &server, &key, "1"));
    assert!(!crate::playlist_describable(Kind::Dlna) && !crate::playlist_needs_tracks(Kind::Dlna));
    assert!(net.asked().is_empty(), "not one request went out");
}

/// A `Browse` answer listing `didl`, escaped as a server escapes it.
fn listed(didl: &str, returned: u64, total: u64) -> String {
    let didl = format!(
        "<DIDL-Lite xmlns=\"urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/\" \
         xmlns:dc=\"http://purl.org/dc/elements/1.1/\" \
         xmlns:upnp=\"urn:schemas-upnp-org:metadata-1-0/upnp/\">{didl}</DIDL-Lite>"
    );
    format!(
        "<?xml version=\"1.0\"?><s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\">\
         <s:Body><u:BrowseResponse xmlns:u=\"urn:schemas-upnp-org:service:ContentDirectory:1\">\
         <Result>{}</Result><NumberReturned>{returned}</NumberReturned>\
         <TotalMatches>{total}</TotalMatches><UpdateID>1</UpdateID>\
         </u:BrowseResponse></s:Body></s:Envelope>",
        escape(&didl)
    )
}

fn container(id: &str, title: &str) -> String {
    format!(
        "<container id=\"{id}\" parentID=\"x\" restricted=\"1\"><dc:title>{title}</dc:title>\
         <upnp:class>object.container.storageFolder</upnp:class></container>"
    )
}

fn song(id: &str, parent: &str, album: &str) -> String {
    format!(
        "<item id=\"{id}\" parentID=\"{parent}\" restricted=\"1\"><dc:title>Song {id}</dc:title>\
         <upnp:artist>Lab Artist</upnp:artist><upnp:album>{album}</upnp:album>\
         <res protocolInfo=\"http-get:*:audio/flac:*\">http://192.168.0.69:32469/object/{id}/file.flac</res>\
         <upnp:class>object.item.audioItem.musicTrack</upnp:class></item>"
    )
}

#[test]
fn the_discs_of_a_record_in_folders_of_their_own_are_one_record() {
    let set = [
        container("cd1", "CD1"),
        container("cd2", "Disc 2 - Live"),
        container("now", "Now 75 - CD1"),
    ]
    .concat();
    // The walk reads the last container it met first.
    let net = Canned::new()
        .answer(200, listed(&set, 3, 3).as_bytes())
        .answer(200, listed(&song("n1", "now", "Box"), 1, 1).as_bytes())
        .answer(200, listed(&song("t2", "cd2", "Box"), 1, 1).as_bytes())
        .answer(200, listed(&song("t1", "cd1", "Box"), 1, 1).as_bytes());
    let server = plex(&["box"]);
    let mut tracks = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..4 {
        let page = page(&net, &server, cursor.as_deref()).unwrap();
        assert!(!page.partial, "{page:?}");
        tracks.extend(page.tracks);
        cursor = page.next;
    }
    assert_eq!(cursor, None);
    let record = |id: &str| {
        tracks
            .iter()
            .find(|t| t.id == id)
            .map(|t| t.album_id.clone())
            .unwrap()
    };
    assert_eq!(
        record("t1"),
        "box\u{1f}Box",
        "a disc's folder stands for the one above it"
    );
    assert_eq!(record("t2"), record("t1"), "both discs, one record");
    assert_eq!(
        record("n1"),
        "now\u{1f}Box",
        "a folder that only ends in a disc is a record of its own"
    );
}

#[test]
fn a_disc_is_named_by_a_disc_word_and_its_number() {
    for disc in [
        "CD1",
        "cd 2",
        "Disc 2 - Live",
        "Disk_3",
        "Disco 1",
        "CD2 (Bonus)",
    ] {
        assert!(is_disc(disc), "{disc}");
    }
    for record in [
        "Now 75 - CD1",
        "CD-Rom Favourites",
        "Discography",
        "CD0",
        "cd",
    ] {
        assert!(!is_disc(record), "{record}");
    }
}

#[test]
fn a_description_names_the_server_and_where_its_catalogue_answers() {
    let found = description(PLEX, saved("plex-description.xml").as_bytes()).unwrap();
    assert_eq!(found.name, "Plex Media Server: Lab");
    assert_eq!(found.product, "Plex Media Server 1.43.5.11079");
    assert_eq!(found.identity, "uuid:f0e1d2c3-b4a5-4697-8879-6a5b4c3d2e1f");
    assert_eq!(
        found.control, PLEX_CONTROL,
        "the ContentDirectory, not the first service listed"
    );
    let renderer =
        saved("plex-description.xml").replace("device:MediaServer:1", "device:MediaRenderer:1");
    assert!(
        description(PLEX, renderer.as_bytes()).is_err(),
        "a renderer is not a media server"
    );
    let moved = |to: &str| {
        saved("plex-description.xml").replace(
            &format!("<controlURL>{PLEX_CONTROL}"),
            &format!("<controlURL>{to}{PLEX_CONTROL}"),
        )
    };
    assert!(
        description(PLEX, moved("http://203.0.113.9:32469").as_bytes()).is_err(),
        "never on another host"
    );
    assert_eq!(
        description(PLEX, moved("http://192.168.0.69:1900").as_bytes())
            .unwrap()
            .control,
        format!("http://192.168.0.69:1900{PLEX_CONTROL}"),
        "another port of the same host is kept whole"
    );
}

#[test]
fn a_host_and_port_try_the_usual_places_and_the_server_says_whether_it_searches() {
    let net = Canned::new()
        .answer(404, b"")
        .answer(404, b"")
        .file("dlna/plex-description.xml")
        .file("dlna/plex-search-caps.xml");
    let found = probe(&net, "http://192.168.0.69:32469").unwrap();
    assert_eq!(found.location, PLEX);
    assert!(!found.search, "an empty SearchCaps is a server to walk");
    let asked = net.asked();
    let urls: Vec<&str> = asked.iter().map(|r| r.url.as_str()).collect();
    assert_eq!(
        urls[..3],
        [
            "http://192.168.0.69:32469/rootDesc.xml",
            "http://192.168.0.69:32469/description.xml",
            PLEX
        ]
    );
    assert_eq!(urls[3], format!("http://192.168.0.69:32469{PLEX_CONTROL}"));
    assert_eq!(
        header(&asked[3], "SOAPAction"),
        Some("\"urn:schemas-upnp-org:service:ContentDirectory:1#GetSearchCapabilities\"")
    );

    let caps = br#"<?xml version="1.0"?><s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"><s:Body><u:GetSearchCapabilitiesResponse xmlns:u="urn:schemas-upnp-org:service:ContentDirectory:1"><SearchCaps>dc:creator,dc:date,dc:title,upnp:album,upnp:artist,upnp:class,upnp:genre,@id,@parentID,@refID</SearchCaps></u:GetSearchCapabilitiesResponse></s:Body></s:Envelope>"#;
    let net = Canned::new()
        .file("dlna/plex-description.xml")
        .answer(200, caps);
    assert!(
        probe(&net, PLEX).unwrap().search,
        "a server that searches by class is searched"
    );
    assert_eq!(
        net.asked().len(),
        2,
        "a description's own address is read as it is"
    );
}

#[test]
fn an_address_where_nothing_answers_is_said_at_once() {
    let net = Canned::new().silence();
    assert!(probe(&net, "http://192.168.0.69:32469")
        .unwrap_err()
        .is_unreachable());
    assert_eq!(net.asked().len(), 1, "no other place is tried");
    let net = Canned::new()
        .answer(404, b"")
        .answer(404, b"")
        .answer(404, b"");
    assert!(matches!(
        probe(&net, "http://192.168.0.69:8080"),
        Err(Failure::Unexpected(_))
    ));
}

#[test]
fn the_containers_are_offered_one_level_at_a_time() {
    let net = Canned::new().file("dlna/plex-root.xml");
    let found = crate::libraries(&net, &plex(&[]), &crate::Credential::Anonymous).unwrap();
    let names: Vec<&str> = found.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, ["Video", "Music", "Photos"]);
    assert_eq!(found[1].id, "abe6121c-1731-4683-815c-89e1dcd2bf11");
    let asked = &net.asked()[0];
    assert_eq!(asked.method, "POST");
    assert_eq!(
        asked.url,
        format!("http://192.168.0.69:32469{PLEX_CONTROL}")
    );
    assert_eq!(
        header(asked, "SOAPAction"),
        Some("\"urn:schemas-upnp-org:service:ContentDirectory:1#Browse\"")
    );
    assert_eq!(
        asked.body.as_ref().unwrap().0,
        "text/xml; charset=\"utf-8\""
    );
    let sent = body(asked);
    assert!(
        sent.contains("<ObjectID>0</ObjectID><BrowseFlag>BrowseDirectChildren</BrowseFlag>"),
        "{sent}"
    );
    assert!(
        sent.contains("<RequestedCount>200</RequestedCount>"),
        "{sent}"
    );
}

#[test]
fn a_walk_reads_every_container_below_the_chosen_one() {
    let server = plex(&[BY_ALBUM]);
    let net = Canned::new()
        .file("dlna/plex-albums.xml")
        .file("dlna/plex-album-two.xml")
        .file("dlna/plex-album-one.xml");
    let mut tracks = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..3 {
        let page = page(&net, &server, cursor.as_deref()).unwrap();
        assert!(!page.partial && page.total.is_none(), "{page:?}");
        assert_eq!(page.scope, BY_ALBUM);
        tracks.extend(page.tracks);
        cursor = page.next;
    }
    assert_eq!(cursor, None, "nothing waits after the last record");
    let asked: Vec<String> = net.asked().iter().map(body).collect();
    assert!(
        asked[0].contains(&format!("<ObjectID>{BY_ALBUM}</ObjectID>")),
        "{}",
        asked[0]
    );
    assert!(
        asked[1].contains("<ObjectID>6afbf8a54d0e842523ba</ObjectID>"),
        "{}",
        asked[1]
    );
    assert!(
        asked[2].contains(&format!("<ObjectID>{RECORD}</ObjectID>")),
        "{}",
        asked[2]
    );
    assert_eq!(tracks.len(), 4);

    let one = tracks.iter().find(|t| t.title == "Lab Song One").unwrap();
    assert_eq!(one.id, "b2000000000000000001");
    assert_eq!(
        (
            one.artist.as_str(),
            one.album.as_str(),
            one.album_artist.as_str()
        ),
        ("Lab Artist", "Lab Record One", "")
    );
    assert_eq!(
        one.album_id,
        format!("{RECORD}\u{1f}Lab Record One"),
        "the record is the container it is listed in"
    );
    assert_eq!(
        (one.number, one.year, one.duration_ms),
        (Some(1), Some(2016), 597_000)
    );
    assert_eq!(
        (one.codec.as_deref(), one.rate, one.channels, one.bits),
        (Some("flac"), Some(44_100), Some(2), None)
    );
    assert!(one.complete, "a listing is all there is to ask");
    assert_eq!(one.genres, vec!["Pop/Rock".to_string()]);
    assert_eq!(
        one.cover.as_deref(),
        Some("/proxy/a1000000000000000001/albumart.jpg")
    );
    assert!(one
        .revision
        .as_deref()
        .unwrap()
        .ends_with(" /object/b2000000000000000001/file.flac"));

    let two = tracks.iter().find(|t| t.number == Some(2)).unwrap();
    assert_eq!(
        two.title, "It\u{2019}s Lab & Two",
        "two levels of escaping, read through"
    );
    assert!(two.genres.is_empty(), "\"Unknown\" is no genre");
    assert_eq!(two.rate, Some(96_000));
}

#[test]
fn a_track_under_another_view_keeps_its_id_and_its_revision_follows_what_it_says() {
    let one = page(
        &Canned::new().file("dlna/plex-album-one.xml"),
        &plex(&[RECORD]),
        None,
    )
    .unwrap()
    .tracks;
    let folder = page(
        &Canned::new().file("dlna/plex-folder.xml"),
        &plex(&[FOLDER]),
        None,
    )
    .unwrap()
    .tracks;
    let ids = |tracks: &[Track]| tracks.iter().map(|t| t.id.clone()).collect::<Vec<_>>();
    assert_eq!(
        ids(&one),
        ids(&folder),
        "the same item, wherever it is listed"
    );
    assert_ne!(
        one[0].revision, folder[0].revision,
        "the folder view says no date: another revision"
    );
    let again = page(
        &Canned::new().file("dlna/plex-album-one.xml"),
        &plex(&[RECORD]),
        None,
    )
    .unwrap()
    .tracks;
    assert_eq!(
        one[0].revision, again[0].revision,
        "the same answer is the same revision"
    );
}

#[test]
fn pages_follow_what_each_one_returned_and_a_short_container_is_partial() {
    let server = plex(&[RECORD]);
    let two_pages = |second_returned: u64, second_total: u64| {
        Canned::new()
            .answer(200, counted(&saved("plex-album-one.xml"), 3, 5).as_bytes())
            .answer(
                200,
                counted(&saved("plex-album-two.xml"), second_returned, second_total).as_bytes(),
            )
    };
    let net = two_pages(2, 5);
    let first = page(&net, &server, None).unwrap();
    assert!(!first.partial && first.next.is_some());
    let second = page(&net, &server, first.next.as_deref()).unwrap();
    assert!(!second.partial && second.next.is_none(), "{second:?}");
    assert!(body(&net.asked()[1]).contains("<StartingIndex>3</StartingIndex>"));

    let net = two_pages(0, 5);
    let first = page(&net, &server, None).unwrap();
    assert!(
        page(&net, &server, first.next.as_deref()).unwrap().partial,
        "five counted, three given"
    );

    let net = two_pages(2, 6);
    let first = page(&net, &server, None).unwrap();
    assert!(
        page(&net, &server, first.next.as_deref()).unwrap().partial,
        "a count that changed halfway"
    );

    let net = Canned::new().answer(200, counted(&saved("plex-album-one.xml"), 3, 0).as_bytes());
    assert!(
        page(&net, &server, None).unwrap().partial,
        "a container that does not count cannot be shown whole"
    );
}

#[test]
fn a_server_that_searches_is_searched_and_one_that_refuses_is_walked() {
    let server = lab(MINI, "/ctl/ContentDir", true, &["1"]);
    let net = Canned::new().file("dlna/handmade-search.xml");
    let found = page(&net, &server, None).unwrap();
    let asked = &net.asked()[0];
    assert_eq!(asked.url, "http://192.168.0.20:8200/ctl/ContentDir");
    assert_eq!(
        header(asked, "SOAPAction"),
        Some("\"urn:schemas-upnp-org:service:ContentDirectory:1#Search\"")
    );
    assert!(body(asked).contains(
        "<ContainerID>1</ContainerID><SearchCriteria>upnp:class derivedfrom &quot;object.item.audioItem&quot;</SearchCriteria>"
    ));
    assert_eq!(
        found.tracks.len(),
        3,
        "{:?}",
        found.tracks.iter().map(|t| &t.title).collect::<Vec<_>>()
    );
    assert!(!found.partial && found.next.is_none());

    let net = Canned::new()
        .answer(500, saved("handmade-fault.xml").as_bytes())
        .file("dlna/handmade-undeclared.xml");
    let refused = page(&net, &server, None).unwrap();
    assert!(
        refused.tracks.is_empty() && refused.next.is_some(),
        "the scope starts again, walked"
    );
    let walked = page(&net, &server, refused.next.as_deref()).unwrap();
    let asked = net.asked();
    assert_eq!(
        header(&asked[1], "SOAPAction"),
        Some("\"urn:schemas-upnp-org:service:ContentDirectory:1#Browse\"")
    );
    assert!(body(&asked[1]).contains("<ObjectID>1</ObjectID>"));
    assert_eq!(walked.tracks.len(), 1);
}

#[test]
fn only_an_original_on_the_server_s_own_host_is_a_track() {
    let server = lab(MINI, "/ctl/ContentDir", true, &["1"]);
    let tracks = page(
        &Canned::new().file("dlna/handmade-search.xml"),
        &server,
        None,
    )
    .unwrap()
    .tracks;
    let rock = &tracks[0];
    assert_eq!(
        rock.id, "64$1$0",
        "a reference is named by what it refers to"
    );
    assert_eq!(rock.title, "Rock & Roll");
    assert_eq!(
        rock.revision
            .as_deref()
            .and_then(|r| r.split_once(' '))
            .map(|(_, url)| url),
        Some("/MediaItems/22.flac"),
        "the original, not the converted PCM listed before it"
    );
    assert_eq!(
        (
            rock.codec.as_deref(),
            rock.bits,
            rock.rate,
            rock.duration_ms
        ),
        (Some("flac"), Some(24), Some(96_000), 205_120)
    );
    assert_eq!(
        (rock.album_artist.as_str(), rock.album_id.as_str()),
        ("Lab Band", ""),
        "a reference with a record artist goes by it"
    );
    assert_eq!(
        (rock.disc, rock.number, rock.year),
        (Some(1), Some(2), Some(1999))
    );
    assert_eq!(rock.cover.as_deref(), Some("/AlbumArt/7-22.jpg"));
    let song = &tracks[1];
    assert_eq!(
        (song.artist.as_str(), song.codec.as_deref()),
        ("Lab Band", Some("mp3")),
        "a composer is not a performer"
    );
    assert_eq!(
        song.album_id, "64$1\u{1f}Lab Live",
        "an original goes by its folder"
    );
    assert_eq!(
        tracks[2].id, rock.id,
        "the same reference from another view: the sync keeps it once"
    );
    for left_out in ["Lab Radio", "Lab DSD", "Lab Elsewhere", "Lab Picture"] {
        assert!(tracks.iter().all(|t| t.title != left_out), "{left_out}");
    }
}

#[test]
fn a_prefix_the_server_did_not_declare_is_declared_for_it() {
    let server = lab(MINI, "/ctl/ContentDir", false, &["8"]);
    let tracks = page(
        &Canned::new().file("dlna/handmade-undeclared.xml"),
        &server,
        None,
    )
    .unwrap()
    .tracks;
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].cover.as_deref(), Some("/art/9.jpg"));
    assert!(
        matches!(
            declare_missing("<DIDL-Lite xmlns:dlna=\"x\"><item/></DIDL-Lite>"),
            Cow::Borrowed(_)
        ),
        "nothing missing, nothing copied"
    );
}

#[test]
fn a_soap_fault_is_said_with_its_code() {
    let net = Canned::new().answer(500, saved("handmade-fault.xml").as_bytes());
    let Err(Failure::Unexpected(why)) = folders(&net, &plex(&[]), ROOT) else {
        panic!("a fault is an error")
    };
    assert!(why.contains("708"), "{why}");
    let net = Canned::new().answer(500, b"<html>oops</html>");
    assert!(
        matches!(folders(&net, &plex(&[]), ROOT), Err(Failure::Unexpected(why)) if why.contains("500"))
    );
    assert!(matches!(
        ping(&Canned::new().answer(403, b""), &plex(&[])),
        Err(Failure::Unexpected(_))
    ));
}

#[test]
fn a_ping_asks_for_the_update_id() {
    let net = Canned::new().file("dlna/plex-update-id.xml");
    assert!(ping(&net, &plex(&[])).is_ok());
    assert_eq!(
        header(&net.asked()[0], "SOAPAction"),
        Some("\"urn:schemas-upnp-org:service:ContentDirectory:1#GetSystemUpdateID\"")
    );
    assert!(ping(&Canned::new().silence(), &plex(&[]))
        .unwrap_err()
        .is_unreachable());
}

#[test]
fn a_stream_is_the_kept_original_on_the_server_s_address_without_headers() {
    let server = plex(&[]);
    let found = stream(
        &server,
        Some("flac"),
        Some("00ff00ff00ff00ff /object/b2000000000000000001/file.flac"),
    );
    assert_eq!(
        found.url,
        "http://192.168.0.69:32469/object/b2000000000000000001/file.flac"
    );
    assert!(
        found.headers.is_empty(),
        "nothing to send: there is no credential"
    );
    assert_eq!(found.extension.as_deref(), Some("flac"));
    assert_eq!(
        stream(&server, None, Some("00 http://192.168.0.69:8200/x.mp3")).url,
        "http://192.168.0.69:8200/x.mp3"
    );
    assert_eq!(
        stream(&server, None, None).url,
        PLEX,
        "without a revision the reader gets the description, and refuses it"
    );
    assert_eq!(
        cover(&server, "/proxy/a1000000000000000001/albumart.jpg").url,
        "http://192.168.0.69:32469/proxy/a1000000000000000001/albumart.jpg"
    );
}

#[test]
fn an_address_a_server_names_is_kept_only_on_its_own_host() {
    assert_eq!(
        kept(PLEX, "http://192.168.0.69:32469/object/1/file.flac").as_deref(),
        Some("/object/1/file.flac")
    );
    assert_eq!(
        kept(PLEX, "http://192.168.0.69:9000/a.flac").as_deref(),
        Some("http://192.168.0.69:9000/a.flac")
    );
    for elsewhere in [
        "http://192.168.0.70:32469/a.flac",
        "http://user@192.168.0.69:32469/a.flac",
        "file:///etc/passwd",
        "ftp://192.168.0.69/a",
    ] {
        assert_eq!(kept(PLEX, elsewhere), None, "{elsewhere}");
    }
    assert_eq!(
        kept(
            "http://[fd00::69]:8200/rootDesc.xml",
            "http://[FD00::69]:8200/m/1.flac"
        )
        .as_deref(),
        Some("/m/1.flac")
    );
    assert_eq!(
        resolve(PLEX, "ctl/ContentDir").as_deref(),
        Some("http://192.168.0.69:32469/ctl/ContentDir"),
        "a path without its slash"
    );
    assert_eq!(resolve(PLEX, "a b"), None);
}

#[test]
fn a_duration_reads_in_milliseconds() {
    assert_eq!(duration_ms("0:09:57.000"), Some(597_000));
    assert_eq!(duration_ms("1:02:03.5"), Some(3_723_500));
    assert_eq!(duration_ms("0:00:07"), Some(7_000));
    assert_eq!(duration_ms("0:03:25.1/3"), Some(205_100));
    for odd in ["", "3:25", "0:61:00", "x:00:01", "0:00:01:00"] {
        assert_eq!(duration_ms(odd), None, "{odd}");
    }
}

#[test]
fn a_page_that_comes_again_ends_its_container_and_is_partial() {
    let server = plex(&[RECORD]);
    let again = counted(&saved("plex-album-one.xml"), 3, 6);
    let net = Canned::new()
        .answer(200, again.as_bytes())
        .answer(200, again.as_bytes());
    let first = page(&net, &server, None).unwrap();
    assert!(!first.partial && first.next.is_some(), "{first:?}");
    assert_eq!(first.tracks.len(), 3);
    let second = page(&net, &server, first.next.as_deref()).unwrap();
    assert!(second.partial, "six counted, the same three given twice");
    assert!(second.tracks.is_empty() && second.next.is_none(), "{second:?}");
    assert_eq!(net.asked().len(), 2, "not asked a third time");
}

#[test]
fn a_container_that_does_not_count_is_read_past_a_short_page() {
    let server = plex(&[RECORD]);
    let net = Canned::new()
        .answer(200, counted(&saved("plex-album-one.xml"), 3, 0).as_bytes())
        .answer(200, listed("", 0, 0).as_bytes());
    let first = page(&net, &server, None).unwrap();
    assert_eq!(first.tracks.len(), 3);
    assert!(first.partial, "a container that does not count cannot be shown whole");
    let next = first.next.as_deref().expect("three of two hundred is not its end");
    let second = page(&net, &server, Some(next)).unwrap();
    assert!(second.tracks.is_empty() && second.next.is_none(), "{second:?}");
    assert!(body(&net.asked()[1]).contains("<StartingIndex>3</StartingIndex>"));
}

#[test]
fn a_container_the_server_cannot_give_is_left_out_and_the_walk_goes_on() {
    let server = plex(&["top"]);
    let top = [container("good", "Good"), container("bad", "Bad")].concat();
    // The walk reads the last container met first: three tries, then a
    // ping, which answers.
    let net = Canned::new()
        .answer(200, listed(&top, 2, 2).as_bytes())
        .silence()
        .silence()
        .silence()
        .file("dlna/plex-update-id.xml")
        .answer(200, listed(&song("t1", "good", "Record"), 1, 1).as_bytes());
    let first = page(&net, &server, None).unwrap();
    let left = page(&net, &server, first.next.as_deref()).unwrap();
    assert_eq!(left.skipped, 1);
    assert!(left.partial && left.tracks.is_empty(), "{left:?}");
    let good = page(&net, &server, left.next.as_deref()).unwrap();
    assert_eq!(good.tracks.len(), 1, "the walk went on past it");
    assert!(good.next.is_none());
    let asked: Vec<String> = net.asked().iter().map(body).collect();
    assert!(asked[1..4].iter().all(|b| b.contains("<ObjectID>bad</ObjectID>")), "{asked:?}");
    assert!(asked[4].contains("GetSystemUpdateID"), "{}", asked[4]);

    // A server that no longer answers at all ends the walk.
    let net = Canned::new()
        .answer(200, listed(&top, 2, 2).as_bytes())
        .silence()
        .silence()
        .silence()
        .silence();
    let first = page(&net, &server, None).unwrap();
    assert!(page(&net, &server, first.next.as_deref())
        .unwrap_err()
        .is_unreachable());

    // One that answers with something that is not a listing is left out
    // without a ping.
    let net = Canned::new()
        .answer(200, listed(&top, 2, 2).as_bytes())
        .answer(200, b"<html>Bad gateway</html>")
        .answer(200, b"<html>Bad gateway</html>")
        .answer(200, b"<html>Bad gateway</html>");
    let first = page(&net, &server, None).unwrap();
    assert_eq!(page(&net, &server, first.next.as_deref()).unwrap().skipped, 1);
    assert_eq!(net.asked().len(), 4);
}

#[test]
fn a_record_s_art_is_its_own_an_image_it_carries_or_its_folder_s() {
    let art = |path: &str| format!("<upnp:albumArtURI>{path}</upnp:albumArtURI>");
    let folder = format!(
        "<container id=\"rec\" parentID=\"top\" restricted=\"1\"><dc:title>Record</dc:title>{}\
         <upnp:class>object.container.album.musicAlbum</upnp:class></container>",
        art("/art/Rec%20One.jpg")
    );
    let item = |id: &str, extra: &str| {
        format!(
            "<item id=\"{id}\" parentID=\"rec\" restricted=\"1\"><dc:title>Song {id}</dc:title>\
             <upnp:album>Record</upnp:album>{extra}\
             <res protocolInfo=\"http-get:*:audio/flac:*\">http://192.168.0.69:32469/object/{id}/file.flac</res>\
             <upnp:class>object.item.audioItem.musicTrack</upnp:class></item>"
        )
    };
    let songs = [
        item("own", &art("http://192.168.0.69:32469/proxy/own/albumart.jpg")),
        item(
            "res",
            "<res protocolInfo=\"http-get:*:image/jpeg:DLNA.ORG_PN=JPEG_TN\">http://192.168.0.69:32469/thumb/res.jpg</res>",
        ),
        item("away", &art("http://203.0.113.9/art/away.jpg")),
        item("none", ""),
        item("port", &art("http://192.168.0.69:8080/art/port.jpg")),
    ]
    .concat();
    let net = Canned::new()
        .answer(200, listed(&folder, 1, 1).as_bytes())
        .answer(200, listed(&songs, 5, 5).as_bytes());
    let server = plex(&["top"]);
    let first = page(&net, &server, None).unwrap();
    let tracks = page(&net, &server, first.next.as_deref()).unwrap().tracks;
    assert_eq!(tracks.len(), 5);
    let cover = |id: &str| tracks.iter().find(|t| t.id == id).and_then(|t| t.cover.clone());
    assert_eq!(cover("own").as_deref(), Some("/proxy/own/albumart.jpg"), "its own, as a path on the server");
    assert_eq!(cover("res").as_deref(), Some("/thumb/res.jpg"), "an image among its res");
    assert_eq!(cover("away").as_deref(), Some("/art/Rec%20One.jpg"), "another host's is not taken: its folder's is");
    assert_eq!(cover("none").as_deref(), Some("/art/Rec%20One.jpg"), "none of its own: its folder's");
    assert_eq!(
        cover("port").as_deref(),
        Some("http://192.168.0.69:8080/art/port.jpg"),
        "another port of the same host, whole"
    );
    assert_eq!(
        crate::dlna::cover(&server, "/art/Rec%20One.jpg").url,
        "http://192.168.0.69:32469/art/Rec%20One.jpg",
        "asked of the server it came from, as it was written"
    );
}
