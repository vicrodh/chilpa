//! A record on a server of mine, saved to the server through the same
//! calls the window makes, against a Jellyfin that keeps what it is sent in
//! memory: nothing leaves the machine.
//!
//! One test on purpose: it points the library, the settings and the cache
//! at a temporary directory, and those belong to the whole process.

use chilpa_api::Chilpa;
use chilpa_library::{Library, NewServer, ServerTrack};
use chilpa_meta::{Http, Reply as MetaReply};
use chilpa_servers::{Credential, Net, Reply, Request};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// A network nothing can go out through: looking records up is not part of this.
struct Nowhere;

impl Http for Nowhere {
    fn get(&self, url: &str, _: &str, _: Duration, _: usize) -> Result<MetaReply, String> {
        panic!("the editor went out to the network: {url}");
    }
}

/// What the fake server holds.
#[derive(Default)]
struct Held {
    items: BTreeMap<String, Value>,
    image: Option<Vec<u8>>,
    /// Answer every change with this status instead of taking it.
    refuse: Option<u16>,
    asked: Vec<(String, String)>,
    etag: u32,
}

/// A Jellyfin that answers from `Held` and takes what is posted to it.
struct Lab(Arc<Mutex<Held>>);

fn base64_decode(text: &[u8]) -> Vec<u8> {
    let value = |c: u8| match c {
        b'A'..=b'Z' => c - b'A',
        b'a'..=b'z' => c - b'a' + 26,
        b'0'..=b'9' => c - b'0' + 52,
        b'+' => 62,
        _ => 63,
    };
    let clean: Vec<u8> = text.iter().copied().filter(|c| *c != b'=').collect();
    let mut out = Vec::new();
    for chunk in clean.chunks(4) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, c)| n | (u32::from(value(*c)) << (18 - 6 * i)));
        for i in 0..chunk.len().saturating_sub(1) {
            out.push((n >> (16 - 8 * i)) as u8);
        }
    }
    out
}

impl Net for Lab {
    fn send(&self, request: &Request) -> Result<Reply, String> {
        let mut held = self.0.lock().unwrap();
        let path = request.url.split('?').next().unwrap_or("").trim_start_matches("http://lab.lan").to_string();
        held.asked.push((request.method.to_string(), path.clone()));
        assert_ne!(request.method, "DELETE", "the editor never deletes");
        let ok = |body: Value| Ok(Reply { status: 200, headers: Default::default(), body: body.to_string().into_bytes() });
        let status = |status: u16| Ok(Reply { status, headers: Default::default(), body: Vec::new() });
        match (request.method, path.as_str()) {
            ("GET", "/Users/Me") => ok(json!({ "Policy": { "IsAdministrator": true } })),
            ("GET", "/Items") => {
                let ids = request.url.split("Ids=").nth(1).unwrap_or("").split('&').next().unwrap_or("").to_string();
                let items: Vec<Value> = ids.split(',').filter_map(|id| held.items.get(id).cloned()).collect();
                ok(json!({ "Items": items, "TotalRecordCount": items.len() }))
            }
            ("GET", p) if p.ends_with("/Images/Primary") => match held.image.clone() {
                Some(bytes) => Ok(Reply { status: 200, headers: [("content-type".to_string(), "image/png".to_string())].into(), body: bytes }),
                None => status(404),
            },
            ("GET", p) if p.starts_with("/Items/") => match held.items.get(&p[7..]) {
                Some(item) => ok(item.clone()),
                None => status(404),
            },
            ("POST", _) if held.refuse.is_some() => status(held.refuse.unwrap_or(500)),
            ("POST", p) if p.ends_with("/Images/Primary") => {
                let (_, body) = request.body.clone().unwrap();
                held.image = Some(base64_decode(&body));
                held.etag += 1;
                let tag = format!("tag{}", held.etag);
                for item in held.items.values_mut() {
                    if item["Type"] == "Audio" {
                        item["AlbumPrimaryImageTag"] = json!(tag);
                    }
                }
                status(204)
            }
            ("POST", p) if p.starts_with("/Items/") => {
                let id = p[7..].to_string();
                let mut posted: Value = serde_json::from_slice(&request.body.clone().unwrap().1).unwrap();
                held.etag += 1;
                posted["Etag"] = json!(format!("e{}", held.etag));
                // Jellyfin keeps the names of the artists posted.
                if let Some(artists) = posted.get("ArtistItems").and_then(Value::as_array).cloned() {
                    posted["Artists"] = Value::Array(artists.iter().map(|a| a["Name"].clone()).collect());
                }
                if let Some(first) = posted.get("AlbumArtists").and_then(Value::as_array).and_then(|a| a.first()).cloned() {
                    posted["AlbumArtist"] = first["Name"].clone();
                }
                held.items.insert(id, posted);
                status(204)
            }
            _ => status(404),
        }
    }
}

fn audio(id: &str, title: &str, number: u32) -> Value {
    json!({
        "Id": id, "Type": "Audio", "Name": title, "Album": "One", "AlbumId": "al", "AlbumArtist": "Band",
        "AlbumArtists": [{ "Name": "Band" }], "Artists": ["Band"], "ArtistItems": [{ "Name": "Band" }],
        "IndexNumber": number, "ParentIndexNumber": 1, "ProductionYear": 2001, "Genres": ["Rock"], "Studios": [],
        "ProviderIds": {}, "LockedFields": [], "LockData": false, "Etag": "e0", "RunTimeTicks": 1_800_000_000u64,
        "MediaSources": [{ "MediaStreams": [{ "Type": "Audio", "Codec": "flac", "BitDepth": 16, "SampleRate": 44100, "Channels": 2 }] }]
    })
}

fn record() -> Value {
    json!({
        "Id": "al", "Type": "MusicAlbum", "Name": "One", "AlbumArtist": "Band", "AlbumArtists": [{ "Name": "Band" }],
        "ProductionYear": 2001, "Genres": ["Rock"], "Studios": [], "ProviderIds": {}, "LockedFields": [], "LockData": false
    })
}

fn track(id: &str, title: &str, number: u16) -> ServerTrack {
    ServerTrack {
        external_id: format!("remote:jellyfin/jjjj/{id}"),
        title: title.into(),
        artist: "Band".into(),
        artists: vec!["Band".into()],
        album: "One".into(),
        album_artist: "Band".into(),
        album_key: "al".into(),
        track_number: Some(number),
        disc_number: Some(1),
        year: Some(2001),
        genres: vec!["Rock".into()],
        duration_ms: 180_000,
        codec: Some("flac".into()),
        bit_depth: Some(16),
        sample_rate_hz: Some(44_100),
        channels: Some(2),
        revision: Some("e0".into()),
        ..ServerTrack::default()
    }
}

fn until(chilpa: &mut Chilpa, state: &str) {
    let limit = Instant::now() + Duration::from_secs(20);
    while Instant::now() < limit {
        if chilpa.metadata_poll().state == state {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("the editor never got to \"{state}\": {:?}", chilpa.metadata_view());
}

fn until_not(chilpa: &mut Chilpa, state: &str) {
    let limit = Instant::now() + Duration::from_secs(20);
    while Instant::now() < limit {
        if chilpa.metadata_poll().state != state {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("the editor stayed \"{state}\"");
}

fn until_cover(chilpa: &mut Chilpa) {
    let limit = Instant::now() + Duration::from_secs(20);
    while Instant::now() < limit {
        if !chilpa.metadata_poll().artwork.loading {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("the cover never came back");
}

/// A record's field as the window is handed it.
fn album_field(view: &chilpa_api::MetadataView, name: &str) -> Value {
    let doc = serde_json::to_value(view).unwrap();
    doc["album"].as_array().unwrap().iter().find(|f| f["name"] == name).cloned().unwrap_or_else(|| panic!("no record field {name}"))
}

fn track_field(view: &chilpa_api::MetadataView, at: usize, name: &str) -> Value {
    let doc = serde_json::to_value(view).unwrap();
    doc["tracks"][at]["fields"].as_array().unwrap().iter().find(|f| f["name"] == name).cloned().unwrap_or_else(|| panic!("no track field {name}"))
}

fn titles(chilpa: &mut Chilpa, album: i64) -> Vec<String> {
    chilpa.album_track_rows(album).into_iter().map(|r| r.title).collect()
}

#[test]
fn a_record_on_a_server_is_saved_to_the_server_and_the_sidecar_wins_only_when_chosen() {
    let dir = std::env::temp_dir().join("chilpa-api-metadata-server");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("XDG_DATA_HOME", dir.join("data"));
    std::env::set_var("XDG_CONFIG_HOME", dir.join("config"));
    std::env::set_var("XDG_CACHE_HOME", dir.join("cache"));

    let path = Library::default_path();
    let (album, first, second, navidrome_album) = {
        let lib = Library::open(&path).unwrap();
        let add = |provider: &str, instance: &str, details: Value| {
            lib.add_server(&NewServer {
                provider: provider.into(),
                instance: instance.into(),
                label: "Lab".into(),
                icon: "server".into(),
                address: "http://lab.lan".into(),
                libraries: "[]".into(),
                details: details.to_string(),
                ..Default::default()
            })
            .unwrap()
        };
        let source = add("jellyfin", "jjjj", json!({ "user_id": "u1" }));
        let generation = lib.begin_server_sync(source).unwrap();
        lib.save_server_tracks(source, generation, &[track("t1", "Song 1", 1), track("t2", "Song 2", 2)]).unwrap();
        lib.finish_server_sync(source, generation).unwrap();
        let other = add("subsonic", "ssss", json!({}));
        let generation = lib.begin_server_sync(other).unwrap();
        let mut theirs = track("n1", "Their song", 1);
        theirs.external_id = "remote:subsonic/ssss/n1".into();
        theirs.album = "Two".into();
        theirs.album_key = "nal".into();
        lib.save_server_tracks(other, generation, &[theirs]).unwrap();
        lib.finish_server_sync(other, generation).unwrap();
        let id = |sql: &str| -> i64 { lib.connection().query_row(sql, [], |r| r.get(0)).unwrap() };
        (
            id("SELECT id FROM albums WHERE title = 'One'"),
            id("SELECT id FROM tracks WHERE external_id LIKE '%/t1'"),
            id("SELECT id FROM tracks WHERE external_id LIKE '%/t2'"),
            id("SELECT id FROM albums WHERE title = 'Two'"),
        )
    };

    let held = Arc::new(Mutex::new(Held::default()));
    {
        let mut lab = held.lock().unwrap();
        lab.items.insert("al".into(), record());
        lab.items.insert("t1".into(), audio("t1", "Song 1", 1));
        lab.items.insert("t2".into(), audio("t2", "Song 2", 2));
    }
    let mut chilpa = Chilpa::new();
    chilpa.metadata_network(Arc::new(Nowhere));
    chilpa.metadata_server_network(Arc::new(Lab(held.clone())), Credential::Token { token: "tok".into() });

    // A server's track offers the editor, as a file's does; never its folder.
    let rows = chilpa.album_track_rows(album);
    assert!(rows[0].available);
    assert!(chilpa_api::track_can_go(&rows[0], "album", "", "metadata"), "the track menu opens its record in the editor");
    assert!(!chilpa_api::track_can_go(&rows[0], "album", "", "folder"));

    // It opens on what the server says, saving to the server.
    assert!(chilpa.metadata_open(album, 0));
    until(&mut chilpa, "ready");
    let view = chilpa.metadata_view().clone();
    assert_eq!(view.save.mode, "server", "{:?}", view.save);
    assert!(view.save.server.allowed && view.save.server.refused.is_empty());
    assert_eq!(view.save.server.name, "Lab");
    assert_eq!(album_field(&view, "title")["value"], "One");
    assert_eq!(track_field(&view, 0, "composers")["editable"], false, "the server does not keep it: it cannot be changed there");
    assert_eq!(track_field(&view, 0, "title")["editable"], true);

    // Saved to the server: written there, read back, and shown.
    assert!(chilpa.metadata_set_album("title", "One, corrected"));
    assert!(chilpa.metadata_set_album("genres", "Blues"));
    assert!(chilpa.metadata_set_track(first, "title", "First"));
    assert!(chilpa.metadata_save());
    until(&mut chilpa, "closed");
    let after = chilpa.metadata_view().clone();
    assert!(after.error.is_empty(), "{}", after.error);
    assert!(!after.notice.is_empty());
    {
        let lab = held.lock().unwrap();
        assert_eq!(lab.items["al"]["Name"], "One, corrected");
        assert_eq!(lab.items["t1"]["Name"], "First");
        assert_eq!(lab.items["t1"]["Album"], "One, corrected", "the record's title is carried by its tracks");
        assert_eq!(lab.items["t2"]["Genres"], json!(["Blues"]));
        assert_eq!(lab.items["t2"]["IndexNumber"], 2, "what was not changed is posted as it was");
        assert!(lab.items["t1"]["LockedFields"].as_array().unwrap().iter().any(|f| f == "Name"));
    }
    assert_eq!(chilpa.album(album).map(|a| a.title), Some("One, corrected".to_string()), "the record keeps its id");
    assert_eq!(titles(&mut chilpa, album), ["First", "Song 2"]);
    assert_eq!(chilpa.album_detail(album).map(|d| d.genre), Some("Blues".to_string()));
    {
        let lib = Library::open_read_only(&path).unwrap();
        let overlays: i64 = lib.connection().query_row("SELECT count(*) FROM source_overlay", [], |r| r.get(0)).unwrap();
        assert_eq!(overlays, 0, "no sidecar was made");
    }

    // Kept on this device on purpose: the sidecar wins over what the
    // server says, even when the server changes it later.
    assert!(chilpa.metadata_open(album, second));
    until(&mut chilpa, "ready");
    assert!(chilpa.metadata_set_mode("sidecar"));
    assert!(chilpa.metadata_set_track(second, "title", "Mine"));
    assert!(chilpa.metadata_save());
    until(&mut chilpa, "closed");
    assert!(chilpa.metadata_view().error.is_empty(), "{}", chilpa.metadata_view().error);
    assert_eq!(held.lock().unwrap().items["t2"]["Name"], "Song 2", "nothing went to the server");
    assert_eq!(titles(&mut chilpa, album), ["First", "Mine"]);
    held.lock().unwrap().items.get_mut("t2").unwrap()["Name"] = json!("Theirs");
    assert!(chilpa.metadata_open(album, 0));
    until(&mut chilpa, "ready");
    let view = chilpa.metadata_view().clone();
    let laid = track_field(&view, 1, "title");
    assert_eq!((&laid["value"], &laid["overridden"], &laid["beneath"]), (&json!("Mine"), &json!(true), &json!("Theirs")), "the sidecar shows, and what is under it is said");
    assert!(chilpa.metadata_set_album("label", "Other label"));
    assert!(chilpa.metadata_save());
    until(&mut chilpa, "closed");
    assert_eq!(held.lock().unwrap().items["al"]["Studios"], json!([{ "Name": "Other label" }]));
    assert_eq!(titles(&mut chilpa, album), ["First", "Mine"], "read back, the server's title stays under the sidecar");

    // Taken off: typed back to what the server says, the server's word shows.
    assert!(chilpa.metadata_open(album, 0));
    until(&mut chilpa, "ready");
    let posts_before = held.lock().unwrap().asked.iter().filter(|(m, _)| m == "POST").count();
    assert!(chilpa.metadata_set_track(second, "title", "Theirs"));
    assert!(chilpa.metadata_save());
    until(&mut chilpa, "closed");
    assert!(chilpa.metadata_view().error.is_empty(), "{}", chilpa.metadata_view().error);
    assert_eq!(held.lock().unwrap().asked.iter().filter(|(m, _)| m == "POST").count(), posts_before, "nothing to send");
    assert_eq!(titles(&mut chilpa, album), ["First", "Theirs"]);

    // A change the server refuses is said, and nothing is lost.
    held.lock().unwrap().refuse = Some(403);
    assert!(chilpa.metadata_open(album, 0));
    until(&mut chilpa, "ready");
    assert!(chilpa.metadata_set_album("title", "Refused"));
    assert!(chilpa.metadata_save());
    until_not(&mut chilpa, "saving");
    let view = chilpa.metadata_view().clone();
    assert_eq!(view.state, "ready", "the form stays open");
    assert!(!view.error.is_empty());
    assert!(view.dirty && album_field(&view, "title")["value"] == "Refused", "what was typed is still there");
    assert_eq!(chilpa.album(album).map(|a| a.title), Some("One, corrected".to_string()));
    held.lock().unwrap().refuse = None;
    chilpa.metadata_revert();

    // A cover goes to the server and is shown as the server gives it back.
    let picked = dir.join("picked.png");
    image::RgbImage::from_fn(16, 16, |x, y| image::Rgb([(x * 16) as u8, (y * 16) as u8, 90])).save(&picked).unwrap();
    assert!(chilpa.metadata_artwork_from_file(&picked));
    until_cover(&mut chilpa);
    assert!(chilpa.metadata_view().cover.waiting, "{}", chilpa.metadata_view().error);
    assert!(chilpa.metadata_save());
    until(&mut chilpa, "closed");
    assert!(chilpa.metadata_view().error.is_empty(), "{}", chilpa.metadata_view().error);
    assert_eq!(held.lock().unwrap().image.as_deref(), Some(std::fs::read(&picked).unwrap().as_slice()), "byte for byte, as base64");
    let cover = chilpa.album(album).map(|a| a.artwork_path).unwrap_or_default();
    assert!(Path::new(&cover).starts_with(dir.join("cache")), "kept where a sync keeps covers: {cover}");
    assert_eq!(std::fs::read(&cover).unwrap(), std::fs::read(&picked).unwrap());

    // A server that takes no changes: nothing is chosen, and the sidecar
    // only when asked.
    assert!(chilpa.metadata_open(navidrome_album, 0));
    until(&mut chilpa, "ready");
    let view = chilpa.metadata_view().clone();
    assert_eq!(view.save.mode, "");
    assert!(!view.save.server.allowed && !view.save.server.refused.is_empty());
    assert!(chilpa.metadata_set_album("title", "Two, mine"));
    assert!(!chilpa.metadata_save(), "no sidecar unasked");
    assert!(!chilpa.metadata_set_mode("server"));
    assert!(chilpa.metadata_set_mode("sidecar"));
    assert!(chilpa.metadata_save());
    until(&mut chilpa, "closed");
    assert_eq!(chilpa.album(navidrome_album).map(|a| a.title), Some("Two, mine".to_string()));

    let asked = held.lock().unwrap().asked.clone();
    assert!(asked.iter().all(|(m, _)| m != "DELETE"));
    assert!(asked.iter().all(|(_, p)| !p.contains("/rest/")), "the OpenSubsonic server was never asked");
    let _ = std::fs::remove_dir_all(dir);
}
