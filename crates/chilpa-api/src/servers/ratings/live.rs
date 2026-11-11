//! Checks against the owner's test servers. Never run by default: each is
//! `#[ignore]` and reads its server from the environment, and without it
//! says which variables it needs and returns.
//!
//! - OpenSubsonic and Jellyfin: test servers, any write allowed. Each write
//!   is put back to the value read before it.
//! - Plex: only the library named `LAB` is read or written. The network
//!   below reads an item's library before every rating write and refuses the
//!   write unless it is that one. Nothing is ever deleted.
//!
//! Nothing secret reaches the log: a request is said by its method and its
//! path, and every error has the address, the user, the password and the
//! token taken out.

use super::super::keys::Keys;
use super::super::net::{self, Network};
use super::super::sync;
use crate::ratings::writer::{self, Tools};
use chilpa_servers::rating::{self, Rated};
use chilpa_servers::{Credential, Net, Reply, Request, Server};
use serde_json::Value;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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

/// The real network, held to what a rating check may send.
struct Guarded {
    secrets: Vec<String>,
    /// Plex: the client id its token was given to, and the lab library's
    /// section id once it is known. A Plex network takes no POST at all.
    plex: Option<String>,
    lab: Mutex<Option<String>>,
}

impl Guarded {
    /// `secrets` are taken out of everything it says, and so is each
    /// address's host without its port.
    fn new(secrets: Vec<String>) -> Self {
        let hosts: Vec<String> = secrets.iter().map(|s| s.split_once("://").map_or(s.as_str(), |(_, rest)| rest)).map(|s| s.split([':', '/']).next().unwrap_or("").to_string()).collect();
        let mut secrets: Vec<String> = secrets.into_iter().chain(hosts).filter(|s| s.len() >= 3).collect();
        secrets.sort_by_key(|s| std::cmp::Reverse(s.len()));
        Guarded { secrets, plex: None, lab: Mutex::new(None) }
    }

    fn plex(mut self, client_id: String) -> Self {
        self.secrets.push(client_id.clone());
        self.plex = Some(client_id);
        self
    }

    fn hide(&self, text: &str) -> String {
        self.secrets.iter().fold(text.to_string(), |text, secret| text.replace(secret.as_str(), "<redacted>"))
    }

    /// Plex: the item a `/:/rate` names is read right before it is written,
    /// and the write goes only when that item is in the lab library, by its
    /// title and by its section id.
    fn lab_only(&self, request: &Request) {
        let lab = self.lab.lock().unwrap().clone().expect("refused: the lab library is not known yet");
        let key = query(&request.url, "key").expect("a rating names its item");
        assert!(!key.is_empty() && key.chars().all(|c| c.is_ascii_digit()), "refused: an unexpected item key");
        let base = &request.url[..request.url.find("/:/rate").expect("a rating request")];
        let look = Request::get(format!("{base}/library/metadata/{key}"), request.headers.clone());
        let reply = Network.send(&look).map_err(|e| self.hide(&e)).expect("the item's library could be read");
        assert_eq!(reply.status, 200, "refused: the item's library could not be read");
        let answer: Value = serde_json::from_slice(&reply.body).unwrap_or(Value::Null);
        let container = &answer["MediaContainer"];
        let item = &container["Metadata"][0];
        let title = container["librarySectionTitle"].as_str().or_else(|| item["librarySectionTitle"].as_str());
        let section = container["librarySectionID"].as_u64().or_else(|| item["librarySectionID"].as_u64()).map(|id| id.to_string());
        assert_eq!(title, Some(LAB), "refused: a rating write outside the lab library");
        assert_eq!(section.as_deref(), Some(lab.as_str()), "refused: a rating write outside the lab library's section");
    }

    /// Plex: a section's items are read only from the lab library.
    fn lab_reads_only(&self, path: &str) {
        let Some(rest) = path.strip_prefix("/library/sections/") else { return };
        if rest.is_empty() {
            return;
        }
        let section = rest.split('/').next().unwrap_or("");
        let lab = self.lab.lock().unwrap().clone();
        assert_eq!(Some(section), lab.as_deref(), "refused: a read of another library's items");
    }
}

impl Net for Guarded {
    fn send(&self, request: &Request) -> Result<Reply, String> {
        let mut request = request.clone();
        let path = path_of(&request.url);
        assert_ne!(request.method, "DELETE", "a live check never deletes");
        if let Some(client_id) = &self.plex {
            for (name, value) in request.headers.iter_mut() {
                match name.as_str() {
                    "X-Plex-Client-Identifier" => *value = client_id.clone(),
                    "X-Plex-Product" => *value = "Chilpa live test".into(),
                    _ => {}
                }
            }
            match request.method {
                "GET" => self.lab_reads_only(&path),
                "PUT" => {
                    assert_eq!(path, "/:/rate", "the only PUT is a rating");
                    self.lab_only(&request);
                }
                other => panic!("refused: {other} to Plex"),
            }
        } else {
            match request.method {
                "GET" => {}
                "POST" => assert!(path.ends_with("/Users/AuthenticateByName") || path.ends_with("/UserData"), "refused: an unexpected POST"),
                other => panic!("refused: {other}"),
            }
        }
        eprintln!("live: {} {}", request.method, self.hide(&path));
        Network.send(&request).map_err(|e| self.hide(&e))
    }
}

fn client() -> chilpa_servers::Client {
    net::client("chilpa-live-ratings")
}

/// Puts an item's rating back as it was when it goes, even when a check
/// failed halfway.
struct PutBack<'a> {
    net: &'a Guarded,
    server: Server,
    key: Credential,
    rated: Rated,
    id: String,
    value: f64,
}

impl Drop for PutBack<'_> {
    fn drop(&mut self) {
        let now = rating::held(self.net, &self.server, &self.key, self.rated, &self.id).map(|held| held.value);
        if now.as_ref().ok() != Some(&self.value) {
            let back = (self.value > 0.0).then_some(self.value);
            let put = rating::rate(self.net, &self.server, &self.key, self.rated, &self.id, back);
            eprintln!("live: {:?} put back: {}", self.rated, put.is_ok());
        }
    }
}

/// Rates an item, reads it back, and puts back what it held before.
fn round_trip(net: &Guarded, server: &Server, key: &Credential, rated: Rated, id: &str, values: (f64, f64)) {
    let shown = |e: chilpa_servers::Failure| net.hide(&e.to_string());
    let before = rating::held(net, server, key, rated, id).map_err(shown).unwrap();
    let _put_back = PutBack { net, server: server.clone(), key: key.clone(), rated, id: id.to_string(), value: before.value };
    let target = if before.value == values.0 { values.1 } else { values.0 };
    rating::rate(net, server, key, rated, id, Some(target)).map_err(shown).unwrap();
    assert_eq!(rating::held(net, server, key, rated, id).map_err(shown).unwrap().value, target, "{rated:?} took the rating");
    let back = (before.value > 0.0).then_some(before.value);
    rating::rate(net, server, key, rated, id, back).map_err(shown).unwrap();
    assert_eq!(rating::held(net, server, key, rated, id).map_err(shown).unwrap().value, before.value, "{rated:?} is as it was");
    eprintln!("live: {rated:?} {} -> {target} -> {} (raw)", before.value, before.value);
}

/// The whole path: a sync takes the ratings in, a change made here goes out
/// through the writer, and the server holds it; then it is put back.
fn end_to_end(net: Arc<Guarded>, provider: &str, address: &str, libraries: &[String], key: Credential, track: &str) {
    let lib = chilpa_library::Library::in_memory().unwrap();
    let instance = super::super::keys::mint();
    let root = std::env::temp_dir().join(format!("chilpa-live-ratings-{instance}"));
    let keys = Arc::new(Keys::derived(root.clone()));
    assert!(keys.put(&instance, &key));
    let source = lib.add_server(&chilpa_library::NewServer {
        provider: provider.into(), instance: instance.clone(), label: "Live".into(), address: address.into(),
        libraries: serde_json::to_string(libraries).unwrap(), details: "{}".into(), ..Default::default()
    }).unwrap();
    let event = sync::run(&lib, net.as_ref(), &keys, &Mutex::new(()), source, &|_| {}, &|| false);
    eprintln!("live: sync ended: {}", matches!(event, sync::Event::Finished { .. }));
    let stored = lib.server(source).unwrap().unwrap();
    let server = net::server(&stored).unwrap();
    let held = rating::held(net.as_ref(), &server, &key, Rated::Track, track).map_err(|e| net.hide(&e.to_string())).unwrap();
    let _put_back = PutBack { net: net.as_ref(), server: server.clone(), key: key.clone(), rated: Rated::Track, id: track.to_string(), value: held.value };
    let origin = crate::ratings::sync::server_origin(&instance);
    let scale = crate::ratings::scale::Scale::of_server(server.kind.rating_scale().unwrap()).unwrap();
    let shown = scale.stars(held.value);
    let local: i64 = lib.connection().query_row("SELECT id FROM tracks WHERE external_id = ?1", [net::entry(&stored, track)], |r| r.get(0)).unwrap();
    assert_eq!(lib.stars_of(chilpa_library::store::ratings::RatingTarget::Track(local)).unwrap(), shown, "the sync took in what the server holds");
    let wanted = if shown == 2 { 4 } else { 2 };
    let tools = Tools { net: net.clone(), keys: keys.clone(), plugins: Arc::new(NoPlugins), open: Arc::new(|| None) };
    for stars in [wanted, shown] {
        lib.queue_rating(&origin, "track", track, stars, None).unwrap();
        let outcome = writer::run(&lib, &tools, crate::now_unix());
        assert_eq!((outcome.sent, outcome.failing), (1, 0));
        let now = rating::held(net.as_ref(), &server, &key, Rated::Track, track).map_err(|e| net.hide(&e.to_string())).unwrap();
        assert_eq!(scale.stars(now.value), stars, "the server holds what Chilpa sent");
    }
    if held.value > 0.0 && scale.value(shown) != Some(held.value) {
        rating::rate(net.as_ref(), &server, &key, Rated::Track, track, Some(held.value)).map_err(|e| net.hide(&e.to_string())).unwrap();
    }
    assert_eq!(rating::held(net.as_ref(), &server, &key, Rated::Track, track).map_err(|e| net.hide(&e.to_string())).unwrap().value, held.value, "put back as it was");
    let _ = std::fs::remove_dir_all(root);
}

struct NoPlugins;
impl writer::Plugins for NoPlugins {
    fn capability(&self, _: &str) -> Value { Value::Null }
    fn call(&self, _: &str, _: &str, _: Value) -> Result<Value, String> { Err("no plugin".into()) }
    fn took(&self, _: &str, _: &str, _: &str, _: f64) {}
}

#[test]
#[ignore = "live: needs PLEX_URL and PLEX_TOKEN; writes only to the Chilpa Lab library"]
fn live_plex_lab_ratings_go_both_ways() {
    let (Some(host), Some(token)) = (var(&["PLEX_URL", "PLEX_HOST", "CHILPA_LIVE_PLEX_URL"]), var(&["PLEX_TOKEN", "CHILPA_LIVE_PLEX_TOKEN"])) else {
        eprintln!("live: skipped: set PLEX_URL (address or host:port) and PLEX_TOKEN (the server's own token)");
        return;
    };
    let at = address(&host);
    let Some(client_id) = var(&["PLEX_CLIENT_ID"]) else {
        eprintln!("live: skipped: set PLEX_CLIENT_ID, the client id the token was given to");
        return;
    };
    let net = Arc::new(Guarded::new(vec![at.clone(), host, token.clone()]).plex(client_id));
    let key = Credential::Plex { account: String::new(), server: token };
    let every = chilpa_servers::plex::server(&at, client(), Vec::new(), Vec::new());
    let sections = chilpa_servers::libraries(net.as_ref(), &every, &key).map_err(|e| net.hide(&e.to_string())).unwrap();
    let lab = sections.into_iter().find(|s| s.name == LAB).expect("a library named Chilpa Lab");
    *net.lab.lock().unwrap() = Some(lab.id.clone());
    let server = chilpa_servers::plex::server(&at, client(), vec![lab.id.clone()], Vec::new());
    let page = chilpa_servers::page(net.as_ref(), &server, &key, None).map_err(|e| net.hide(&e.to_string())).unwrap();
    assert_eq!(page.ratings.len(), page.tracks.len(), "every track listed says its rating");
    eprintln!("live: lab page: {} tracks, {} rated", page.tracks.len(), page.ratings.iter().filter(|r| r.value > 0.0).count());
    let records = rating::album_ratings(net.as_ref(), &server, &key, 8, Instant::now() + Duration::from_secs(60)).map_err(|e| net.hide(&e.to_string())).unwrap();
    eprintln!("live: lab records: {} listed, {} rated, whole: {}", records.found.len(), records.found.iter().filter(|r| r.value > 0.0).count(), records.whole);
    let track = page.tracks.first().expect("the lab library has a track").clone();
    round_trip(&net, &server, &key, Rated::Track, &track.id, (6.0, 8.0));
    round_trip(&net, &server, &key, Rated::Album, &track.album_id, (6.0, 8.0));
    end_to_end(net, "plex", &at, &[lab.id], key, &track.id);
}

#[test]
#[ignore = "live: needs NAVIDROME_HOST, NAVIDROME_USER and NAVIDROME_PASSWORD"]
fn live_navidrome_ratings_go_both_ways() {
    let (Some(host), Some(user), Some(password)) = (
        var(&["NAVIDROME_HOST", "CHILPA_LIVE_NAVIDROME_URL"]),
        var(&["NAVIDROME_USER", "CHILPA_LIVE_NAVIDROME_USER"]),
        var(&["NAVIDROME_PASSWORD", "CHILPA_LIVE_NAVIDROME_PASSWORD"]),
    ) else {
        eprintln!("live: skipped: set NAVIDROME_HOST (host:port), NAVIDROME_USER and NAVIDROME_PASSWORD");
        return;
    };
    let at = address(&host);
    let net = Arc::new(Guarded::new(vec![at.clone(), host, user.clone(), password.clone()]));
    let signed = chilpa_servers::subsonic::sign_in(net.as_ref(), &at, &user, &password, false).map_err(|e| net.hide(&e.to_string())).unwrap();
    eprintln!("live: server says it is {}", signed.product);
    let server = chilpa_servers::subsonic::server(&at, client(), Vec::new(), signed.extensions.clone());
    let page = chilpa_servers::page(net.as_ref(), &server, &signed.key, None).map_err(|e| net.hide(&e.to_string())).unwrap();
    let tracks = page.ratings.iter().filter(|r| r.rated == Rated::Track).count();
    assert_eq!(tracks, page.tracks.len(), "every song listed says its rating");
    eprintln!("live: first page: {} songs, {} records, {} rated", page.tracks.len(), page.ratings.len() - tracks, page.ratings.iter().filter(|r| r.value > 0.0).count());
    let track = page.tracks.first().expect("the server has a song").clone();
    round_trip(&net, &server, &signed.key, Rated::Track, &track.id, (3.0, 5.0));
    round_trip(&net, &server, &signed.key, Rated::Album, &track.album_id, (3.0, 5.0));
    end_to_end(net, "subsonic", &at, &[], signed.key, &track.id);
}

/// What Jellyfin offers for ratings, read from the server itself: its
/// plugins, what a track's user data holds, and whether `Rating` written
/// there is kept. It writes one track's `Rating` and puts it back.
#[test]
#[ignore = "live: needs JELLYFIN_HOST, JELLYFIN_USER and JELLYFIN_PASSWORD"]
fn live_jellyfin_rating_discovery() {
    let (Some(host), Some(user), Some(password)) = (
        var(&["JELLYFIN_HOST", "CHILPA_LIVE_JELLYFIN_URL"]),
        var(&["JELLYFIN_USER", "CHILPA_LIVE_JELLYFIN_USER"]),
        var(&["JELLYFIN_PASSWORD", "CHILPA_LIVE_JELLYFIN_PASSWORD"]),
    ) else {
        eprintln!("live: skipped: set JELLYFIN_HOST (host:port), JELLYFIN_USER and JELLYFIN_PASSWORD");
        return;
    };
    let at = address(&host);
    let mut secrets = vec![at.clone(), host, user.clone(), password.clone()];
    let probe = Guarded::new(secrets.clone());
    let signed = chilpa_servers::jellyfin::sign_in(&probe, &at, &client(), &user, &password).map_err(|e| probe.hide(&e.to_string())).unwrap();
    let Credential::Token { token } = &signed.key else { panic!("a token") };
    secrets.extend([token.clone(), signed.user_id.clone()]);
    let net = Guarded::new(secrets);
    let headers = vec![
        ("Authorization".to_string(), format!("MediaBrowser Client=\"Chilpa\", Device=\"live\", DeviceId=\"chilpa-live-ratings\", Version=\"0\", Token=\"{token}\"")),
        ("Accept".to_string(), "application/json".to_string()),
    ];
    let get = |path: &str| -> Value {
        let reply = net.send(&Request::get(format!("{at}{path}"), headers.clone())).unwrap();
        assert!((200..300).contains(&reply.status), "HTTP {}", reply.status);
        serde_json::from_slice(&reply.body).unwrap_or(Value::Null)
    };
    let info = get("/System/Info/Public");
    eprintln!("live: Jellyfin {}", info["Version"].as_str().unwrap_or("?"));
    for plugin in get("/Plugins").as_array().into_iter().flatten() {
        eprintln!("live: plugin {} {} ({})", plugin["Name"].as_str().unwrap_or("?"), plugin["Version"].as_str().unwrap_or("?"), plugin["Status"].as_str().unwrap_or("?"));
    }
    let uid = &signed.user_id;
    let items = get(&format!("/Users/{uid}/Items?Recursive=true&IncludeItemTypes=Audio&Limit=1&Fields=UserData"));
    let item = items["Items"][0].clone();
    let id = item["Id"].as_str().expect("the server has a song").to_string();
    let keys: Vec<&str> = item["UserData"].as_object().map(|o| o.keys().map(String::as_str).collect()).unwrap_or_default();
    eprintln!("live: a song's user data says {keys:?}; Rating {}, Likes {}", item["UserData"]["Rating"], item["UserData"]["Likes"]);
    let full = get(&format!("/Users/{uid}/Items/{id}"));
    eprintln!("live: CommunityRating {}, CriticRating {}", full["CommunityRating"], full["CriticRating"]);
    // `Rating` set cannot be taken off again without a DELETE, which these
    // checks never send: only a song that already has one is written.
    let Some(before) = item["UserData"]["Rating"].as_f64() else {
        eprintln!("live: write skipped: this song has no Rating, and one written could not be taken off again");
        return;
    };
    let post = |rating: f64| {
        let body = serde_json::json!({ "Rating": rating }).to_string().into_bytes();
        let reply = net.send(&Request::post(format!("{at}/UserItems/{id}/UserData?userId={uid}"), headers.clone(), Some(("application/json".into(), body)))).unwrap();
        eprintln!("live: UpdateItemUserData answered HTTP {}", reply.status);
    };
    let target = if before == 8.0 { 4.0 } else { 8.0 };
    post(target);
    let after = get(&format!("/UserItems/{id}/UserData?userId={uid}"));
    eprintln!("live: after writing {target}: Rating {}, Likes {}", after["Rating"], after["Likes"]);
    post(before);
    let restored = get(&format!("/UserItems/{id}/UserData?userId={uid}"));
    eprintln!("live: put back: Rating {} (was {before}), Likes {}", restored["Rating"], restored["Likes"]);
    assert_eq!(restored["Rating"].as_f64(), Some(before));
}
