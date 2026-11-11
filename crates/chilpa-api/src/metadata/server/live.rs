//! Checks against the owner's test servers, through the editor's own path:
//! what opening reads, the plan, the save and the read-back. Never run by
//! default: each is `#[ignore]` and reads its server from the environment,
//! and without it says which variables it needs and returns.
//!
//! - Jellyfin and OpenSubsonic: test servers, any write allowed. Every item
//!   changed is posted back whole as it was read, and an image sent is
//!   taken off again and the server's own found.
//! - Plex: only the library titled `LAB` is read item by item or written.
//!   The network below reads an item's library before every write and
//!   refuses it unless it is that library, by title and by section id. It
//!   refuses every `DELETE`. Every field changed is put back with the lock it
//!   had, and the poster shown before is shown again.
//!
//! Nothing secret reaches the log: a request is said by its method and its
//! path, and every error has the address, the user, the password, the token
//! and the client id taken out.

use super::super::draft::{Draft, HomeState};
use super::*;
use chilpa_library::{Hosted, StoredAlbum};
use chilpa_servers::{Reply, Request};
use serde_json::{json, Value};
use std::sync::Mutex;

/// The only Plex library these checks may touch.
const LAB: &str = "Chilpa Lab";

fn var(names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| std::env::var(name).ok().filter(|v| !v.trim().is_empty()))
}

fn address(host: &str) -> String {
    let host = host.trim().trim_end_matches('/');
    if host.contains("://") { host.to_string() } else { format!("http://{host}") }
}

/// A URL's path, without its address or its query.
fn path_of(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let path = rest.find('/').map_or("/", |at| &rest[at..]);
    path.split('?').next().unwrap_or("").to_string()
}

fn query(url: &str, name: &str) -> Option<String> {
    url.split('?').nth(1)?.split('&').find_map(|pair| pair.strip_prefix(&format!("{name}=")).map(str::to_string))
}

/// The network a check is held to. `inner` is what it sends through: the
/// real network in a live check, a fake in the guard's own test.
pub(super) struct Guarded {
    inner: Arc<dyn Net>,
    secrets: Vec<String>,
    /// Plex: the client id its token was given to.
    plex: Option<String>,
    /// Plex: the lab library's section id, once it is known.
    pub(super) lab: Mutex<Option<String>>,
    /// Jellyfin: the one item whose image may be taken off (put back).
    pub(super) image_back: Mutex<Option<String>>,
    pub(super) refused: Mutex<Vec<String>>,
    pub(super) sent: Mutex<Vec<(String, String)>>,
}

impl Guarded {
    pub(super) fn new(inner: Arc<dyn Net>, secrets: Vec<String>) -> Self {
        let hosts: Vec<String> = secrets.iter().map(|s| s.split_once("://").map_or(s.as_str(), |(_, rest)| rest)).map(|s| s.split([':', '/']).next().unwrap_or("").to_string()).collect();
        let mut secrets: Vec<String> = secrets.into_iter().chain(hosts).filter(|s| s.len() >= 3).collect();
        secrets.sort_by_key(|s| std::cmp::Reverse(s.len()));
        Guarded { inner, secrets, plex: None, lab: Mutex::new(None), image_back: Mutex::new(None), refused: Mutex::new(Vec::new()), sent: Mutex::new(Vec::new()) }
    }

    pub(super) fn plex(mut self, client_id: String) -> Self {
        self.secrets.push(client_id.clone());
        self.plex = Some(client_id);
        self
    }

    fn hide(&self, text: &str) -> String {
        self.secrets.iter().fold(text.to_string(), |text, secret| text.replace(secret.as_str(), "<redacted>"))
    }

    /// Plex: the item is read, and it is the lab's by its library's title
    /// and by its section id, or the request is refused.
    fn in_lab(&self, request: &Request, id: &str) -> Result<(), String> {
        let lab = self.lab.lock().unwrap().clone().ok_or("refused: the lab library is not known yet")?;
        if id.is_empty() || !id.chars().all(|c| c.is_ascii_digit()) {
            return Err("refused: an unexpected item key".into());
        }
        let base = request.url.split("/library/").next().unwrap_or("");
        let look = Request::get(format!("{base}/library/metadata/{id}"), request.headers.clone());
        let reply = self.inner.send(&look).map_err(|e| self.hide(&e))?;
        if reply.status != 200 {
            return Err("refused: the item's library could not be read".into());
        }
        let answer: Value = serde_json::from_slice(&reply.body).unwrap_or(Value::Null);
        let container = &answer["MediaContainer"];
        let item = &container["Metadata"][0];
        let title = container["librarySectionTitle"].as_str().or_else(|| item["librarySectionTitle"].as_str());
        let section = container["librarySectionID"].as_u64().or_else(|| item["librarySectionID"].as_u64()).map(|id| id.to_string());
        if title != Some(LAB) {
            return Err("refused: a write outside the lab library".into());
        }
        if section.as_deref() != Some(lab.as_str()) {
            return Err("refused: a write outside the lab library's section".into());
        }
        Ok(())
    }

    /// Whether the request may go.
    pub(super) fn check(&self, request: &Request) -> Result<(), String> {
        let path = path_of(&request.url);
        if self.plex.is_some() {
            let lab = self.lab.lock().unwrap().clone();
            return match request.method {
                "GET" => match path.strip_prefix("/library/sections/") {
                    Some(rest) if !rest.is_empty() && Some(rest.split('/').next().unwrap_or("")) != lab.as_deref() => {
                        Err("refused: a read of another library's items".into())
                    }
                    _ => Ok(()),
                },
                "PUT" => {
                    let section = path.strip_prefix("/library/sections/").and_then(|rest| rest.strip_suffix("/all"));
                    let poster = path.strip_prefix("/library/metadata/").and_then(|rest| rest.strip_suffix("/poster"));
                    match (section, poster) {
                        (Some(section), _) => {
                            if Some(section) != lab.as_deref() {
                                return Err("refused: a change to another library".into());
                            }
                            let ids = query(&request.url, "id").ok_or("refused: a change without its item")?;
                            ids.split(',').try_for_each(|id| self.in_lab(request, id))
                        }
                        (None, Some(id)) => self.in_lab(request, id),
                        _ => Err(format!("refused: PUT {path}")),
                    }
                }
                "POST" => match path.strip_prefix("/library/metadata/").and_then(|rest| rest.strip_suffix("/posters")) {
                    Some(id) => self.in_lab(request, id),
                    None => Err(format!("refused: POST {path}")),
                },
                other => Err(format!("refused: {other} to Plex")),
            };
        }
        match request.method {
            "GET" => Ok(()),
            "POST" if path.ends_with("/Users/AuthenticateByName") || path.starts_with("/Items/") => Ok(()),
            // Test server only: the image sent is taken off again.
            "DELETE" if self.image_back.lock().unwrap().as_deref().is_some_and(|id| path == format!("/Items/{id}/Images/Primary")) => Ok(()),
            other => Err(format!("refused: {other} {path}")),
        }
    }
}

impl Net for Guarded {
    fn send(&self, request: &Request) -> Result<Reply, String> {
        let mut request = request.clone();
        if let Some(client_id) = &self.plex {
            for (name, value) in request.headers.iter_mut() {
                match name.as_str() {
                    "X-Plex-Client-Identifier" => *value = client_id.clone(),
                    "X-Plex-Product" => *value = "Chilpa live test".into(),
                    _ => {}
                }
            }
        }
        let path = path_of(&request.url);
        if let Err(why) = self.check(&request) {
            eprintln!("live: {why}");
            self.refused.lock().unwrap().push(why.clone());
            return Err(why);
        }
        self.sent.lock().unwrap().push((request.method.to_string(), path.clone()));
        eprintln!("live: {} {}", request.method, self.hide(&path));
        self.inner.send(&request).map_err(|e| self.hide(&e))
    }
}

/// A Plex network that answers item reads as `items` say, and every other
/// request with 200: what the guard's own test sends through.
struct Shelf {
    items: Vec<(&'static str, &'static str, u64)>,
    reached: Mutex<Vec<(String, String)>>,
}

impl Net for Shelf {
    fn send(&self, request: &Request) -> Result<Reply, String> {
        let path = path_of(&request.url);
        self.reached.lock().unwrap().push((request.method.to_string(), path.clone()));
        let body = match path.strip_prefix("/library/metadata/").filter(|_| request.method == "GET") {
            Some(id) => match self.items.iter().find(|(key, _, _)| *key == id) {
                Some((key, title, section)) => json!({ "MediaContainer": { "librarySectionTitle": title, "librarySectionID": section,
                    "Metadata": [{ "ratingKey": key, "librarySectionTitle": title, "librarySectionID": section, "type": "album", "title": "A record", "parentRatingKey": "1" }] } })
                .to_string(),
                None => return Ok(Reply { status: 404, ..Reply::default() }),
            },
            None => String::new(),
        };
        Ok(Reply { status: 200, headers: Default::default(), body: body.into_bytes() })
    }
}

#[test]
fn the_lab_guard_lets_through_only_writes_to_the_lab_s_items_and_never_a_delete() {
    // Item 2 is the lab's; item 1 is in the music library; item 3 says the
    // lab's title from another section.
    let shelf = Arc::new(Shelf { items: vec![("1", "Music", 3), ("2", LAB, 12), ("3", LAB, 4)], reached: Mutex::new(Vec::new()) });
    let guard = Guarded::new(shelf.clone(), vec!["http://pms.lan:32400".into()]).plex("cid".into());
    let at = |method: &'static str, path: &str| Request { method, url: format!("http://pms.lan:32400{path}"), headers: Vec::new(), body: None, timeout: std::time::Duration::from_secs(1) };
    assert!(guard.check(&at("PUT", "/library/sections/12/all?type=9&id=2&title.value=x")).is_err(), "nothing before the lab is known");
    *guard.lab.lock().unwrap() = Some("12".into());
    let refused = [
        at("PUT", "/library/sections/12/all?type=9&id=1&title.value=x"),
        at("PUT", "/library/sections/3/all?type=9&id=1&title.value=x"),
        at("PUT", "/library/sections/3/all?type=9&id=2&title.value=x"),
        at("PUT", "/library/sections/12/all?type=9&id=3&title.value=x"),
        at("PUT", "/library/sections/12/all?type=9&id=2,1&title.value=x"),
        at("PUT", "/library/sections/12/all?type=9&title.value=x"),
        at("PUT", "/library/metadata/1/poster?url=x"),
        at("POST", "/library/metadata/1/posters"),
        at("POST", "/library/metadata/3/posters"),
        at("DELETE", "/library/metadata/2/thumb"),
        at("DELETE", "/library/metadata/2"),
        at("GET", "/library/sections/3/all?type=10"),
        at("PUT", "/:/rate?key=2&rating=4"),
        at("POST", "/playlists?uri=x"),
    ];
    for request in &refused {
        assert!(guard.check(request).is_err(), "{} {}", request.method, request.url);
    }
    let allowed = [
        at("GET", "/library/sections"),
        at("GET", "/library/sections/12/all?type=9"),
        at("GET", "/library/metadata/1"),
        at("PUT", "/library/sections/12/all?type=9&id=2&title.value=x"),
        at("PUT", "/library/metadata/2/poster?url=upload%3A%2F%2Fposters%2Fx"),
        at("POST", "/library/metadata/2/posters"),
    ];
    for request in &allowed {
        assert!(guard.check(request).is_ok(), "{} {}", request.method, request.url);
    }
    // The editor's own write, to an item of the music library, never leaves.
    let server = chilpa_servers::plex::server("http://pms.lan:32400", crate::servers::net::client("x"), vec!["12".into()], Vec::new());
    let key = Credential::Plex { account: String::new(), server: "tok".into() };
    shelf.reached.lock().unwrap().clear();
    let edit = edit::Edit { title: Some("Renamed".into()), ..edit::Edit::default() };
    assert!(edit::write(&guard, &server, &key, Entity::Album, "1", &edit).is_err());
    assert!(edit::upload_image(&guard, &server, &key, Entity::Album, "1", &[0xFF, 0xD8, 0xFF], "image/jpeg").is_err());
    let reached = shelf.reached.lock().unwrap().clone();
    assert!(reached.iter().all(|(method, _)| method == "GET"), "only reads reached the network: {reached:?}");
    assert!(edit::write(&guard, &server, &key, Entity::Album, "2", &edit).is_ok(), "the lab's own item is written");
    assert!(shelf.reached.lock().unwrap().iter().any(|(method, path)| method == "PUT" && path == "/library/sections/12/all"));
}

/// A record and two of its tracks, as the library would keep them.
fn edit_tracks(provider: &str, album: &str, ids: &[String]) -> Vec<EditTrack> {
    ids.iter()
        .enumerate()
        .map(|(at, id)| EditTrack {
            id: at as i64 + 1,
            external_id: format!("remote:{provider}/live/{id}"),
            container: None,
            hosted: Some(Hosted { source_id: 1, album_key: Some(album.to_string()) }),
            title: String::new(),
            artist: String::new(),
            track_number: None,
            disc_number: None,
            disc_subtitle: String::new(),
            work: String::new(),
            recording_mbid: None,
            explicit: None,
            genres: Vec::new(),
        })
        .collect()
}

fn stored(provider: &str, at: &str, libraries: &[String], details: Value) -> chilpa_library::StoredServer {
    chilpa_library::StoredServer {
        source_id: 1,
        provider: provider.into(),
        instance: "chilpa-live-metadata".into(),
        label: "Lab".into(),
        reachable: true,
        enabled: true,
        address: at.into(),
        account: String::new(),
        identity: String::new(),
        product: String::new(),
        libraries: serde_json::to_string(libraries).unwrap(),
        details: details.to_string(),
        offline_folder: String::new(),
        added_at: 0,
        session_revision: 0,
        synced_at: None,
        tracks: 0,
        albums: 0,
    }
}

/// A 32 by 32 PNG nobody has seen before.
fn picture() -> super::super::artwork::Waiting {
    let mut bytes = std::io::Cursor::new(Vec::new());
    let seed = (crate::now_unix() % 200) as u8;
    image::RgbImage::from_fn(32, 32, |x, y| image::Rgb([seed, (x * 8) as u8, (y * 8) as u8])).write_to(&mut bytes, image::ImageFormat::Png).unwrap();
    super::super::artwork::Waiting {
        bytes: bytes.into_inner(),
        extension: "png",
        mime: "image/png",
        width: 32,
        height: 32,
        preview: std::path::PathBuf::from("/nowhere/preview.png"),
        from: "file",
    }
}

/// Opens the record as the editor does, changes what `change` changes,
/// saves to the server and says what came back.
fn save(reach: &Reach, tracks: &[EditTrack], change: impl FnOnce(&mut Draft), cover: Option<super::super::artwork::Waiting>) -> Outcome {
    let (reads, state) = read(Some(reach), tracks);
    let state = state.expect("a record on a server");
    assert!(state.writable, "the lab server takes changes: {}", state.why);
    assert!(reads.iter().all(Option::is_some), "every track was read from the server");
    let album = StoredAlbum { id: 1, title: reads[0].as_ref().unwrap().said.album.clone().unwrap_or_default(), album_artist: String::new(), year: None, artwork_path: None, track_count: tracks.len(), explicit: None };
    let home = HomeState { home: tracks[0].home(), of: tracks[0].external_id.clone(), sidecar: None, unreadable: String::new(), xml: None };
    let mut draft = Draft::open(&album, tracks.to_vec(), reads.into_iter().map(Option::unwrap).collect(), vec![home]);
    change(&mut draft);
    let made = plan(&draft, &state, cover, crate::now_unix()).expect("the server keeps what was changed");
    match run(1, made, reach.clone()) {
        Done::ServerSaved { outcome, .. } => outcome,
        _ => panic!("a save to the server ends as one"),
    }
}

/// What a Plex item said before a check, to be put back as it was: each
/// field with its lock, and the poster shown.
struct PlexBack<'a> {
    net: &'a Guarded,
    at: String,
    token: String,
    lab: String,
    items: Vec<(u8, Value)>,
    poster: Option<(String, String)>,
}

impl PlexBack<'_> {
    fn put(&self, kind: u8, item: &Value, pairs: Vec<(String, String)>) {
        let key = item["ratingKey"].as_str().unwrap_or("");
        let tail: Vec<String> = pairs.into_iter().map(|(k, v)| format!("{k}={}", encode(&v))).collect();
        let url = format!("{}/library/sections/{}/all?type={kind}&id={key}&{}", self.at, self.lab, tail.join("&"));
        let request = Request::put(url, plex_headers(&self.token), None);
        let answer = self.net.send(&request).map(|r| r.status);
        eprintln!("live: put back type {kind}: {answer:?}");
    }
}

fn encode(text: &str) -> String {
    text.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

fn plex_headers(token: &str) -> Vec<(String, String)> {
    vec![
        ("Accept".into(), "application/json".into()),
        ("X-Plex-Token".into(), token.into()),
        ("X-Plex-Client-Identifier".into(), String::new()),
        ("X-Plex-Product".into(), String::new()),
    ]
}

fn locked(item: &Value) -> Vec<String> {
    item["Field"].as_array().into_iter().flatten().filter(|f| f["locked"] == true).filter_map(|f| f["name"].as_str().map(str::to_string)).collect()
}

fn genres(item: &Value) -> Vec<String> {
    item["Genre"].as_array().into_iter().flatten().filter_map(|g| g["tag"].as_str().map(str::to_string)).collect()
}

impl Drop for PlexBack<'_> {
    fn drop(&mut self) {
        for (kind, was) in &self.items {
            let key = was["ratingKey"].as_str().unwrap_or("");
            let lock = |name: &str| if locked(was).iter().any(|l| l == name) { "1" } else { "0" }.to_string();
            let text = |name: &str| was[name].as_str().map(str::to_string).or_else(|| was[name].as_u64().map(|n| n.to_string())).unwrap_or_default();
            let mut pairs = Vec::new();
            let fields: &[&str] = if *kind == 9 { &["title", "studio", "originallyAvailableAt", "year"] } else { &["title", "originalTitle", "index", "parentIndex"] };
            for name in fields {
                pairs.push((format!("{name}.value"), text(name)));
                pairs.push((format!("{name}.locked"), lock(name)));
            }
            if *kind == 9 {
                pairs.push(("artist.id.value".into(), text("parentRatingKey")));
            }
            self.put(*kind, was, pairs);
            // Genres: the old ones added back, what the check added taken off.
            let old = genres(was);
            let mut pairs: Vec<(String, String)> = old.iter().enumerate().map(|(i, g)| (format!("genre[{i}].tag.tag"), g.clone())).collect();
            pairs.push(("genre.locked".into(), lock("genre")));
            self.put(*kind, was, pairs);
            let look = Request::get(format!("{}/library/metadata/{key}", self.at), plex_headers(&self.token));
            let now: Value = self.net.send(&look).ok().and_then(|r| serde_json::from_slice(&r.body).ok()).unwrap_or(Value::Null);
            let extra: Vec<String> = genres(&now["MediaContainer"]["Metadata"][0]).into_iter().filter(|g| !old.contains(g)).collect();
            if !extra.is_empty() {
                self.put(*kind, was, vec![("genre[].tag.tag-".into(), extra.join(",")), ("genre.locked".into(), lock("genre"))]);
            }
        }
        if let Some((key, poster)) = &self.poster {
            let url = format!("{}/library/metadata/{key}/poster?url={}", self.at, encode(poster));
            let answer = self.net.send(&Request::put(url, plex_headers(&self.token), None)).map(|r| r.status);
            eprintln!("live: poster put back: {answer:?}");
            if let Some((_, album)) = self.items.iter().find(|(kind, _)| *kind == 9) {
                let lock = if locked(album).iter().any(|l| l == "thumb") { "1" } else { "0" };
                self.put(9, album, vec![("thumb.locked".into(), lock.into())]);
            }
        }
    }
}

#[test]
#[ignore = "live: needs PLEX_URL, PLEX_TOKEN and PLEX_CLIENT_ID; writes only to the Chilpa Lab library"]
fn live_plex_lab_record_saved_to_the_server_and_put_back() {
    let (Some(host), Some(token), Some(client_id)) = (var(&["PLEX_URL", "PLEX_HOST"]), var(&["PLEX_TOKEN"]), var(&["PLEX_CLIENT_ID"])) else {
        eprintln!("live: skipped: set PLEX_URL, PLEX_TOKEN and PLEX_CLIENT_ID");
        return;
    };
    let at = address(&host);
    let net = Arc::new(Guarded::new(Arc::new(crate::servers::net::Network), vec![at.clone(), host, token.clone()]).plex(client_id));
    let key = Credential::Plex { account: String::new(), server: token.clone() };
    let every = chilpa_servers::plex::server(&at, crate::servers::net::client("chilpa-live-metadata"), Vec::new(), Vec::new());
    let sections = chilpa_servers::libraries(net.as_ref(), &every, &key).map_err(|e| net.hide(&e.to_string())).unwrap();
    let lab = sections.into_iter().find(|s| s.name == LAB).expect("a library named Chilpa Lab").id;
    *net.lab.lock().unwrap() = Some(lab.clone());
    let get = |path: &str| -> Value {
        let reply = net.send(&Request::get(format!("{at}{path}"), plex_headers(&token))).unwrap();
        assert_eq!(reply.status, 200, "{path}");
        serde_json::from_slice(&reply.body).unwrap()
    };
    let albums = get(&format!("/library/sections/{lab}/all?type=9&X-Plex-Container-Start=3&X-Plex-Container-Size=1"));
    let album = albums["MediaContainer"]["Metadata"][0]["ratingKey"].as_str().expect("a lab record").to_string();
    let children = get(&format!("/library/metadata/{album}/children"));
    let ids: Vec<String> = children["MediaContainer"]["Metadata"].as_array().unwrap().iter().take(2).filter_map(|t| t["ratingKey"].as_str().map(str::to_string)).collect();
    let item = |id: &str| get(&format!("/library/metadata/{id}"))["MediaContainer"]["Metadata"][0].clone();
    let mut items = vec![(9u8, item(&album))];
    items.extend(ids.iter().map(|id| (10u8, item(id))));
    let posters = get(&format!("/library/metadata/{album}/posters"));
    let shown = posters["MediaContainer"]["Metadata"].as_array().into_iter().flatten().find(|p| p["selected"] == true).and_then(|p| p["ratingKey"].as_str()).map(|p| (album.clone(), p.to_string()));
    let before: Vec<(u8, Value)> = items.clone();
    let shown_before = shown.as_ref().map(|(_, poster)| poster.clone());
    let back = PlexBack { net: net.as_ref(), at: at.clone(), token: token.clone(), lab: lab.clone(), items, poster: shown };

    let reach = Reach { net: net.clone(), keys: None, credential: Some(key.clone()), stored: stored("plex", &at, std::slice::from_ref(&lab), json!({})), offline: false };
    let tracks = edit_tracks("plex", &album, &ids);
    let outcome = save(&reach, &tracks, |draft| {
        assert!(draft.set_album("title", "Chilpa live title"));
        assert!(draft.set_album("genres", "Chilpa Live Genre"));
        assert!(draft.set_album("date", "1999-04-05"));
        assert!(draft.set_album("label", "Chilpa Live Label"));
        assert!(draft.set_track(1, "title", "Chilpa live track"));
        assert!(draft.set_track(1, "artist_credit", "Chilpa Live Artist"));
        assert!(draft.set_track(2, "track_number", "77"));
    }, Some(picture()));
    eprintln!("live: plex sent {} of {}, cover {}, unlike {}, read back {} tracks, failed {:?}", outcome.sent, outcome.total, outcome.cover_sent, outcome.unlike, outcome.tracks.len(), outcome.failed.as_deref().map(|w| net.hide(w)));
    assert!(outcome.failed.is_none());
    assert_eq!((outcome.sent, outcome.total), (3, 4), "the record and its two tracks, and the cover");
    assert!(outcome.cover_sent && outcome.cover.is_some(), "the cover went, and came back from the server");
    assert_eq!(outcome.unlike, 0, "the server keeps every field as it was sent");
    let first = outcome.tracks.iter().find(|t| t.id == ids[0]).expect("read back");
    assert_eq!((first.title.as_str(), first.artist.as_str(), first.album.as_str()), ("Chilpa live track", "Chilpa Live Artist", "Chilpa live title"));
    assert_eq!(first.genres, ["Chilpa Live Genre"], "the record's genres reached its tracks");
    assert_eq!(first.year, Some(1999));
    assert_eq!(outcome.tracks.iter().find(|t| t.id == ids[1]).and_then(|t| t.number), Some(77));
    let after = item(&album);
    assert!(["title", "studio", "originallyAvailableAt", "genre", "thumb"].iter().all(|f| locked(&after).iter().any(|l| l == f)), "each field written is locked: {:?}", locked(&after));
    drop(back);

    // Everything is as it was, and nothing was refused or deleted.
    for (_, was) in &before {
        let now = item(was["ratingKey"].as_str().unwrap());
        for field in ["title", "originalTitle", "studio", "originallyAvailableAt", "year", "index", "parentIndex"] {
            assert_eq!(now[field], was[field], "{field} put back");
        }
        assert_eq!(genres(&now), genres(was), "genres put back");
        assert_eq!(locked(&now), locked(was), "locks put back");
    }
    let posters = get(&format!("/library/metadata/{album}/posters"));
    let now_shown = posters["MediaContainer"]["Metadata"].as_array().into_iter().flatten().find(|p| p["selected"] == true).and_then(|p| p["ratingKey"].as_str()).map(str::to_string);
    assert_eq!(now_shown, shown_before, "the poster shown before is shown again");
    assert!(net.refused.lock().unwrap().is_empty(), "the guard refused nothing the editor sent");
    let sent = net.sent.lock().unwrap().clone();
    assert!(sent.iter().all(|(method, _)| method != "DELETE"), "no DELETE to Plex");
    eprintln!("live: plex requests: {} GET, {} PUT, {} POST", sent.iter().filter(|(m, _)| m == "GET").count(), sent.iter().filter(|(m, _)| m == "PUT").count(), sent.iter().filter(|(m, _)| m == "POST").count());
}

/// What Jellyfin items said before a check, posted back whole; and the
/// record's image taken off again so the server finds its own.
struct JellyfinBack<'a> {
    net: &'a Guarded,
    at: String,
    user: String,
    headers: Vec<(String, String)>,
    items: Vec<Value>,
    image_of: Option<String>,
}

impl JellyfinBack<'_> {
    fn post(&self, item: &Value) {
        let id = item["Id"].as_str().unwrap_or("");
        let request = Request::post(format!("{}/Items/{id}", self.at), self.headers.clone(), Some(("application/json".into(), item.to_string().into_bytes())));
        eprintln!("live: item put back: {:?}", self.net.send(&request).map(|r| r.status));
    }
}

/// What a record says of itself that Jellyfin may work out again from its
/// tracks on a refresh.
const DERIVED: [&str; 5] = ["Name", "AlbumArtist", "ProductionYear", "PremiereDate", "Genres"];

impl Drop for JellyfinBack<'_> {
    fn drop(&mut self) {
        // The tracks first, then the record: a refresh Jellyfin queued while
        // the check ran works the record out of its tracks.
        for item in self.items.iter().skip(1) {
            self.post(item);
        }
        if let Some(record) = self.items.first() {
            self.post(record);
        }
        if let Some(id) = &self.image_of {
            *self.net.image_back.lock().unwrap() = Some(id.clone());
            let gone = self.net.send(&Request::delete(format!("{}/Items/{id}/Images/Primary", self.at), self.headers.clone())).map(|r| r.status);
            let refresh = format!("{}/Items/{id}/Refresh?metadataRefreshMode=None&imageRefreshMode=Default&replaceAllImages=false&replaceAllMetadata=false", self.at);
            let found = self.net.send(&Request::post(refresh, self.headers.clone(), None)).map(|r| r.status);
            eprintln!("live: image taken off: {gone:?}, its own looked for: {found:?}");
        }
        // A refresh queued earlier can still write the record over what was
        // put back: it is read until it stays as it was for ten seconds.
        let Some(record) = self.items.first() else { return };
        let id = record["Id"].as_str().unwrap_or("");
        let mut steady = 0;
        for _ in 0..30 {
            std::thread::sleep(std::time::Duration::from_secs(2));
            let look = Request::get(format!("{}/Items/{id}?userId={}", self.at, self.user), self.headers.clone());
            let now: Value = self.net.send(&look).ok().and_then(|r| serde_json::from_slice(&r.body).ok()).unwrap_or(Value::Null);
            if DERIVED.iter().all(|key| now[key] == record[key]) {
                steady += 1;
                if steady >= 5 {
                    return;
                }
            } else {
                steady = 0;
                eprintln!("live: the record drifted after it was put back; put back again");
                self.post(record);
            }
        }
        eprintln!("live: the record did not stay put back");
    }
}

#[test]
#[ignore = "live: needs JELLYFIN_HOST, JELLYFIN_USER and JELLYFIN_PASSWORD"]
fn live_jellyfin_record_saved_to_the_server_and_put_back() {
    let (Some(host), Some(user), Some(password)) = (var(&["JELLYFIN_HOST"]), var(&["JELLYFIN_USER"]), var(&["JELLYFIN_PASSWORD"])) else {
        eprintln!("live: skipped: set JELLYFIN_HOST, JELLYFIN_USER and JELLYFIN_PASSWORD");
        return;
    };
    let at = address(&host);
    let mut secrets = vec![at.clone(), host, user.clone(), password.clone()];
    let probe = Guarded::new(Arc::new(crate::servers::net::Network), secrets.clone());
    let client = crate::servers::net::client("chilpa-live-metadata");
    let signed = chilpa_servers::jellyfin::sign_in(&probe, &at, &client, &user, &password).map_err(|e| probe.hide(&e.to_string())).unwrap();
    let Credential::Token { token } = &signed.key else { panic!("a token") };
    secrets.extend([token.clone(), signed.user_id.clone()]);
    let net = Arc::new(Guarded::new(Arc::new(crate::servers::net::Network), secrets));
    let headers = vec![
        ("Authorization".to_string(), format!("MediaBrowser Client=\"Chilpa\", Device=\"live\", DeviceId=\"chilpa-live-metadata\", Version=\"0\", Token=\"{token}\"")),
        ("Accept".to_string(), "application/json".to_string()),
    ];
    let uid = signed.user_id.clone();
    let get = |path: &str| -> Value {
        let reply = net.send(&Request::get(format!("{at}{path}"), headers.clone())).unwrap();
        assert!((200..300).contains(&reply.status), "HTTP {}", reply.status);
        serde_json::from_slice(&reply.body).unwrap_or(Value::Null)
    };
    let albums = get(&format!("/Items?userId={uid}&IncludeItemTypes=MusicAlbum&Recursive=true&Limit=1&StartIndex=4&SortBy=SortName"));
    let album = albums["Items"][0]["Id"].as_str().expect("a record").to_string();
    let children = get(&format!("/Items?userId={uid}&ParentId={album}&IncludeItemTypes=Audio&Recursive=true&SortBy=SortName"));
    let ids: Vec<String> = children["Items"].as_array().unwrap().iter().take(2).filter_map(|t| t["Id"].as_str().map(str::to_string)).collect();
    let full = |id: &str| get(&format!("/Items/{id}?userId={uid}"));
    let mut items = vec![full(&album)];
    items.extend(ids.iter().map(|id| full(id)));
    // Every track carries the record's fields: all of them are put back.
    let all: Vec<String> = children["Items"].as_array().unwrap().iter().filter_map(|t| t["Id"].as_str().map(str::to_string)).collect();
    items.extend(all.iter().skip(2).map(|id| full(id)));
    let image_before = get(&format!("/Items/{album}/Images")).as_array().cloned().unwrap_or_default().iter()
        .filter(|i| i["ImageType"] == "Primary").filter_map(|i| i["Path"].as_str().map(|p| p.rsplit('/').next().unwrap_or("").to_string())).collect::<Vec<_>>();
    let before = items.clone();
    let back = JellyfinBack { net: net.as_ref(), at: at.clone(), user: uid.clone(), headers: headers.clone(), items, image_of: Some(album.clone()) };

    let reach = Reach { net: net.clone(), keys: None, credential: Some(signed.key.clone()), stored: stored("jellyfin", &at, &[], json!({ "user_id": uid })), offline: false };
    let tracks = edit_tracks("jellyfin", &album, &all);
    let outcome = save(&reach, &tracks, |draft| {
        assert!(draft.set_album("title", "Chilpa live title"));
        assert!(draft.set_album("album_artist", "Chilpa Live Album Artist"));
        assert!(draft.set_album("genres", "Chilpa Live Genre"));
        assert!(draft.set_album("date", "1999-04-05"));
        assert!(draft.set_album("label", "Chilpa Live Label"));
        assert!(draft.set_track(1, "title", "Chilpa live track"));
        assert!(draft.set_track(1, "artist_credit", "Chilpa Live Artist"));
        assert!(draft.set_track(2, "track_number", "77"));
    }, Some(picture()));
    eprintln!("live: jellyfin sent {} of {}, cover {}, unlike {}, read back {} tracks, failed {:?}", outcome.sent, outcome.total, outcome.cover_sent, outcome.unlike, outcome.tracks.len(), outcome.failed.as_deref().map(|w| net.hide(w)));
    assert!(outcome.failed.is_none());
    assert_eq!(outcome.sent, 1 + all.len(), "the record, and each track for the fields it carries");
    assert!(outcome.cover_sent && outcome.cover.is_some());
    assert_eq!(outcome.unlike, 0, "the server keeps every field as it was sent");
    let first = outcome.tracks.iter().find(|t| t.id == ids[0]).expect("read back");
    assert_eq!((first.title.as_str(), first.artist.as_str(), first.album.as_str(), first.album_artist.as_str()), ("Chilpa live track", "Chilpa Live Artist", "Chilpa live title", "Chilpa Live Album Artist"));
    assert_eq!((first.genres.clone(), first.year), (vec!["Chilpa Live Genre".to_string()], Some(1999)));
    assert_eq!(outcome.tracks.iter().find(|t| t.id == ids[1]).and_then(|t| t.number), Some(77));
    let record = full(&album);
    let locks: Vec<&str> = record["LockedFields"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    eprintln!("live: the record's locks after: {locks:?}");
    assert!(locks.contains(&"Name") && locks.contains(&"Genres") && locks.contains(&"Studios"));
    drop(back);

    let keys = ["Name", "Album", "AlbumArtist", "Artists", "IndexNumber", "ParentIndexNumber", "ProductionYear", "PremiereDate", "Genres", "Studios", "ProviderIds", "LockedFields", "LockData"];
    for was in &before {
        let now = full(was["Id"].as_str().unwrap());
        // A record's `Artists` is worked out from its tracks', which are
        // compared one by one.
        for key in keys.iter().filter(|key| !(**key == "Artists" && was["Type"] == "MusicAlbum")) {
            assert_eq!(now[key], was[key], "{key} put back");
        }
    }
    let mut image_after = Vec::new();
    for _ in 0..20 {
        image_after = get(&format!("/Items/{album}/Images")).as_array().cloned().unwrap_or_default().iter()
            .filter(|i| i["ImageType"] == "Primary").filter_map(|i| i["Path"].as_str().map(|p| p.rsplit('/').next().unwrap_or("").to_string())).collect();
        if image_after == image_before {
            break;
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    assert_eq!(image_after, image_before, "the record shows its own image again");
    assert!(net.refused.lock().unwrap().is_empty());
}

#[test]
#[ignore = "live: needs NAVIDROME_HOST, NAVIDROME_USER and NAVIDROME_PASSWORD"]
fn live_navidrome_takes_no_change_and_is_never_asked_for_one() {
    let (Some(host), Some(user), Some(password)) = (var(&["NAVIDROME_HOST"]), var(&["NAVIDROME_USER"]), var(&["NAVIDROME_PASSWORD"])) else {
        eprintln!("live: skipped: set NAVIDROME_HOST, NAVIDROME_USER and NAVIDROME_PASSWORD");
        return;
    };
    let at = address(&host);
    let net = Arc::new(Guarded::new(Arc::new(crate::servers::net::Network), vec![at.clone(), host, user.clone(), password.clone()]));
    let signed = chilpa_servers::subsonic::sign_in(net.as_ref(), &at, &user, &password, false).map_err(|e| net.hide(&e.to_string())).unwrap();
    eprintln!("live: server says it is {}", signed.product);
    let asked = net.sent.lock().unwrap().len();
    let reach = Reach { net: net.clone(), keys: None, credential: Some(signed.key.clone()), stored: stored("subsonic", &at, &[], json!({})), offline: false };
    let (reads, state) = read(Some(&reach), &edit_tracks("subsonic", "al", &["s1".to_string()]));
    let state = state.unwrap();
    eprintln!("live: navidrome: writable {}, why: {}", state.writable, state.why);
    assert!(!state.writable && !state.why.is_empty());
    assert!(reads.iter().all(Option::is_none));
    assert_eq!(net.sent.lock().unwrap().len(), asked, "nothing was asked of it");
}
