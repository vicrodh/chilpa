//! Ratings between the library and a server, against the saved answers of
//! the lab servers (CHD-0060 §4) and a network that answers what it was
//! loaded with: nothing here leaves the machine.

use super::*;
use crate::ratings::writer::{self, Plugins, Tools};
use chilpa_servers::{Reply, Request};
use serde_json::Value;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// Answers in order with what it was loaded with, and keeps what it was
/// asked, method and address.
#[derive(Default)]
pub(crate) struct Answers {
    replies: Mutex<VecDeque<Result<(u16, Vec<u8>), String>>>,
    asked: Mutex<Vec<(String, String)>>,
}

impl Answers {
    pub(crate) fn file(self, name: &str) -> Self {
        let path = format!("{}/../chilpa-servers/tests/answers/{name}", env!("CARGO_MANIFEST_DIR"));
        let body = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        self.replies.lock().unwrap().push_back(Ok((200, body)));
        self
    }
    pub(crate) fn status(self, status: u16, body: &str) -> Self {
        self.replies.lock().unwrap().push_back(Ok((status, body.as_bytes().to_vec())));
        self
    }
    pub(crate) fn silence(self) -> Self {
        self.replies.lock().unwrap().push_back(Err("the server could not be reached".into()));
        self
    }
    pub(crate) fn asked(&self) -> Vec<(String, String)> {
        self.asked.lock().unwrap().clone()
    }
}

impl Net for Answers {
    fn send(&self, request: &Request) -> Result<Reply, String> {
        assert!(request.method != "DELETE" || request.url.contains("/UserFavoriteItems/"), "the only DELETE is taking a heart off");
        self.asked.lock().unwrap().push((request.method.to_string(), request.url.clone()));
        let (status, body) = self.replies.lock().unwrap().pop_front().unwrap_or_else(|| Err("nothing was loaded".into()))?;
        Ok(Reply { status, headers: Default::default(), body })
    }
}

/// A plugin side that has no plugin.
struct NoPlugins;
impl Plugins for NoPlugins {
    fn capability(&self, _: &str) -> Value {
        Value::Null
    }
    fn call(&self, _: &str, _: &str, _: Value) -> Result<Value, String> {
        panic!("no plugin is asked anything here")
    }
    fn took(&self, _: &str, _: &str, _: &str, _: f64) {}
}

pub(crate) struct Lab {
    pub(crate) lib: chilpa_library::Library,
    pub(crate) keys: Arc<Keys>,
    pub(crate) source: i64,
    pub(crate) instance: String,
    pub(crate) root: std::path::PathBuf,
}

impl Drop for Lab {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

impl Lab {
    pub(crate) fn new(provider: &str, libraries: &str, key: chilpa_servers::Credential) -> Lab {
        let lib = chilpa_library::Library::in_memory().unwrap();
        let instance = super::super::keys::mint();
        let root = std::env::temp_dir().join(format!("chilpa-ratings-{instance}"));
        let keys = Arc::new(Keys::derived(root.clone()));
        assert!(keys.put(&instance, &key));
        let source = lib
            .add_server(&chilpa_library::NewServer {
                provider: provider.into(),
                instance: instance.clone(),
                label: "Lab".into(),
                address: "http://lab.lan".into(),
                libraries: libraries.into(),
                details: "{}".into(),
                ..Default::default()
            })
            .unwrap();
        Lab { lib, keys, source, instance, root }
    }

    pub(crate) fn origin(&self) -> String {
        server_origin(&self.instance)
    }

    pub(crate) fn sync(&self, net: &Answers) -> Event {
        super::super::sync::run(&self.lib, net, &self.keys, &Mutex::new(()), self.source, &|_| {}, &|| false)
    }

    pub(crate) fn track(&self, server_id: &str) -> i64 {
        let stored = self.lib.server(self.source).unwrap().unwrap();
        self.lib.connection().query_row("SELECT id FROM tracks WHERE external_id = ?1", [net::entry(&stored, server_id)], |r| r.get(0)).unwrap()
    }

    pub(crate) fn album(&self, release: &str) -> i64 {
        self.lib.connection().query_row("SELECT id FROM albums WHERE version_key = ?1", [format!("{}:{release}", self.source)], |r| r.get(0)).unwrap()
    }

    fn stars(&self, target: chilpa_library::store::ratings::RatingTarget) -> u8 {
        self.lib.stars_of(target).unwrap()
    }

    pub(crate) fn tools(&self, net: Arc<Answers>) -> Tools {
        Tools { net, keys: self.keys.clone(), plugins: Arc::new(NoPlugins), open: Arc::new(|| None) }
    }

    fn outbox(&self) -> i64 {
        self.lib.connection().query_row("SELECT count(*) FROM rating_outbox", [], |r| r.get(0)).unwrap()
    }
}

use chilpa_library::store::ratings::RatingTarget::{Album, Track};

pub(crate) fn subsonic() -> Lab {
    Lab::new("subsonic", "[]", chilpa_servers::Credential::Password { user: "lab".into(), password: "lab-password".into() })
}

pub(crate) fn plex() -> Lab {
    Lab::new("plex", r#"["1"]"#, chilpa_servers::Credential::Plex { account: "acct".into(), server: "srv".into() })
}

/// Navidrome's saved catalogue, its first record rated, and the census
/// asked after the last page.
pub(crate) fn navidrome(album_zero: &str) -> Answers {
    Answers::default()
        .file("navidrome/subsonic-scan.json")
        .file("navidrome/subsonic-albums.json")
        .file(album_zero)
        .file("navidrome/subsonic-album-1.json")
        .file("navidrome/subsonic-album-2.json")
        .file("navidrome/subsonic-album-3.json")
        .file("navidrome/subsonic-scan.json")
        .file("navidrome/subsonic-albums.json")
}

/// The saved Plex track listing with two tracks rated, their details, and
/// the records listed apart.
pub(crate) fn plex_catalogue() -> Answers {
    Answers::default().file("plex/plex-tracks-rated.json").file("plex/plex-metadata.json").file("plex/plex-albums-rated.json")
}

pub(crate) fn ok() -> &'static str {
    r#"{"subsonic-response":{"status":"ok","version":"1.16.1"}}"#
}

#[test]
fn a_sync_takes_in_a_subsonic_server_s_ratings_and_writes_nothing_out() {
    let lab = subsonic();
    let net = navidrome("navidrome/subsonic-album-0-rated.json");
    assert_eq!(lab.sync(&net), Event::Finished { source: lab.source, gone: 0 });
    assert_eq!(lab.stars(Track(lab.track("47gLTJvR49ADB1Pa0vPhB5"))), 5);
    assert_eq!(lab.stars(Album(lab.album("2NGalRARWUlolzE5BntvyN"))), 4);
    assert_eq!(lab.stars(Track(lab.track("4AbMVAGmME5H9RzBSSy1Rr"))), 0);
    assert!(net.asked().iter().all(|(method, url)| method == "GET" && !url.contains("setRating")), "a reading writes nothing");
    assert_eq!(lab.outbox(), 0);
}

#[test]
fn a_sync_takes_in_plex_track_and_record_ratings_rounded_and_keeps_the_raw_value() {
    let lab = plex();
    let net = plex_catalogue();
    assert_eq!(lab.sync(&net), Event::Finished { source: lab.source, gone: 0 });
    assert_eq!(lab.stars(Track(lab.track("17"))), 4, "7 of 10 is three and a half stars, shown as 4");
    assert_eq!(lab.stars(Track(lab.track("18"))), 5);
    assert_eq!(lab.stars(Album(lab.album("16"))), 4);
    assert_eq!(lab.stars(Album(lab.album("6"))), 5);
    assert_eq!(lab.lib.remote_rating(&lab.origin(), "track", "17").unwrap(), Some(7.0), "what the server holds is kept as it is");
    let asked = net.asked();
    assert!(asked.last().unwrap().1.ends_with("/library/sections/1/all?type=9"), "records listed apart, after the catalogue");
    assert!(asked.iter().all(|(method, url)| method == "GET" && !url.contains("/:/rate")));
    assert_eq!(lab.outbox(), 0);
}

#[test]
fn turned_off_a_sync_reads_no_rating() {
    let lab = plex();
    lab.lib.set_rating_sync(&lab.origin(), false).unwrap();
    let net = Answers::default().file("plex/plex-tracks-rated.json").file("plex/plex-metadata.json");
    assert_eq!(lab.sync(&net), Event::Finished { source: lab.source, gone: 0 });
    assert_eq!(lab.stars(Track(lab.track("17"))), 0);
    assert_eq!(net.asked().len(), 2, "no records listing either");
}

#[test]
fn a_rating_the_listener_did_not_change_is_never_written() {
    let lab = plex();
    lab.sync(&plex_catalogue());
    let net = Arc::new(Answers::default());
    let outcome = writer::run(&lab.lib, &lab.tools(net.clone()), i64::MAX);
    assert_eq!(outcome, writer::Outcome::default());
    assert!(net.asked().is_empty(), "every value the sync took in stays where it is on the server");
}

#[test]
fn a_change_goes_to_plex_on_its_scale_and_a_clear_sends_minus_one() {
    let lab = plex();
    lab.sync(&plex_catalogue());
    let origin = lab.origin();
    assert!(lab.lib.queue_rating(&origin, "track", "19", 3, Some(0)).unwrap());
    assert!(lab.lib.queue_rating(&origin, "album", "6", 0, Some(5)).unwrap());
    let net = Arc::new(Answers::default().status(200, "").status(200, ""));
    let outcome = writer::run(&lab.lib, &lab.tools(net.clone()), crate::now_unix());
    assert_eq!((outcome.sent, outcome.failing, outcome.next_due), (2, 0, None));
    let urls: Vec<String> = net.asked().into_iter().map(|(method, url)| format!("{method} {url}")).collect();
    assert!(urls.contains(&"PUT http://lab.lan/:/rate?key=19&identifier=com.plexapp.plugins.library&rating=6".to_string()), "{urls:?}");
    assert!(urls.contains(&"PUT http://lab.lan/:/rate?key=6&identifier=com.plexapp.plugins.library&rating=-1".to_string()), "{urls:?}");
    assert_eq!(lab.lib.remote_rating(&origin, "track", "19").unwrap(), Some(6.0), "what the server holds now");
    assert_eq!(lab.outbox(), 0);
}

#[test]
fn a_change_waits_while_the_server_cannot_take_it_and_goes_when_it_can() {
    let lab = subsonic();
    lab.sync(&navidrome("navidrome/subsonic-album-0.json"));
    let origin = lab.origin();
    lab.lib.queue_rating(&origin, "track", "47gLTJvR49ADB1Pa0vPhB5", 4, None).unwrap();
    let net = Arc::new(Answers::default().silence().status(200, ok()));
    let now = 1_000_000;
    let failed = writer::run(&lab.lib, &lab.tools(net.clone()), now);
    assert_eq!((failed.sent, failed.failing, failed.next_due), (0, 1, Some(now + 30)));
    assert_eq!(writer::run(&lab.lib, &lab.tools(net.clone()), now + 29), writer::Outcome { failing: 1, next_due: Some(now + 30), ..Default::default() }, "not before its time");
    assert_eq!(net.asked().len(), 1);
    let sent = writer::run(&lab.lib, &lab.tools(net.clone()), now + 30);
    assert_eq!((sent.sent, sent.failing, sent.next_due), (1, 0, None));
    let url = &net.asked()[1].1;
    assert!(url.contains("/rest/setRating.view?") && url.contains("id=47gLTJvR49ADB1Pa0vPhB5") && url.contains("rating=4"), "{url}");
}

#[test]
fn a_change_for_an_item_the_server_no_longer_has_is_dropped() {
    let lab = subsonic();
    lab.sync(&navidrome("navidrome/subsonic-album-0.json"));
    lab.lib.queue_rating(&lab.origin(), "track", "gone", 2, None).unwrap();
    let net = Arc::new(Answers::default().status(200, r#"{"subsonic-response":{"status":"failed","version":"1.16.1","error":{"code":70,"message":"not found"}}}"#));
    let outcome = writer::run(&lab.lib, &lab.tools(net), crate::now_unix());
    assert_eq!((outcome.dropped, outcome.failing, outcome.next_due), (1, 0, None));
}

#[test]
fn a_disabled_or_removed_server_keeps_or_drops_its_changes() {
    let lab = subsonic();
    lab.sync(&navidrome("navidrome/subsonic-album-0.json"));
    lab.lib.queue_rating(&lab.origin(), "track", "47gLTJvR49ADB1Pa0vPhB5", 4, None).unwrap();
    lab.lib.set_server_enabled(lab.source, false).unwrap();
    let net = Arc::new(Answers::default());
    let waiting = writer::run(&lab.lib, &lab.tools(net.clone()), 100);
    assert_eq!(waiting.failing, 1, "kept for when it is on again");
    assert!(net.asked().is_empty());
    lab.lib.queue_rating("server:no-such-instance", "track", "x", 1, None).unwrap();
    let dropped = writer::run(&lab.lib, &lab.tools(net.clone()), i64::MAX);
    assert_eq!(dropped.dropped, 1);
    assert!(net.asked().is_empty());
}

#[test]
fn the_listener_s_change_goes_out_once_and_the_last_one_wins() {
    let mut api = crate::Chilpa::new();
    let lab = plex();
    lab.sync(&plex_catalogue());
    let (track, album) = (lab.track("17"), lab.album("16"));
    let origin = lab.origin();
    api.library = Some(lab_library(&lab));
    assert!(api.rate_as("track", track, 4), "the same stars it shows: no change");
    assert_eq!(api.library().unwrap().due_ratings(i64::MAX, 10).unwrap(), Vec::new(), "7 on the server is not rewritten as 8");
    assert!(api.rate_as("track", track, 2));
    assert!(api.rate_as("track", track, 5));
    assert!(api.rate_as("album", album, 1));
    let due = api.library().unwrap().due_ratings(i64::MAX, 10).unwrap();
    assert_eq!(due.iter().map(|c| (c.kind.as_str(), c.id.as_str(), c.stars)).collect::<Vec<_>>(), [("track", "17", 5), ("album", "16", 1)]);
    assert!(due.iter().all(|c| c.origin == origin));
    let net = Arc::new(Answers::default().status(200, "").status(200, ""));
    let outcome = writer::run(api.library().unwrap(), &lab.tools(net.clone()), crate::now_unix());
    assert_eq!(outcome.sent, 2);
    let urls: Vec<String> = net.asked().into_iter().map(|(_, url)| url).collect();
    assert_eq!(urls.iter().filter(|u| u.contains("key=17")).count(), 1, "one write for the track, its last value");
    assert!(urls.iter().any(|u| u.contains("key=17") && u.ends_with("rating=10")));
    assert!(urls.iter().any(|u| u.contains("key=16") && u.ends_with("rating=2")));
}

/// The lab's library, opened again by its file, for an api that owns it.
pub(crate) fn lab_library(lab: &Lab) -> chilpa_library::Library {
    let path = lab.root.join("library.db");
    std::fs::create_dir_all(&lab.root).unwrap();
    lab.lib.connection().execute("VACUUM INTO ?1", [path.to_string_lossy().as_ref()]).unwrap();
    chilpa_library::Library::open(&path).unwrap()
}

#[test]
fn a_server_row_turns_rating_sync_off_and_on_and_says_when_changes_wait() {
    let mut api = crate::Chilpa::new();
    let lab = subsonic();
    lab.sync(&navidrome("navidrome/subsonic-album-0.json"));
    api.library = Some(lab_library(&lab));
    let key = format!("sources.server/{}", lab.source);
    let row = |api: &mut crate::Chilpa| api.server_rows(&Default::default()).into_iter().find(|row| row.key == key).unwrap();
    let on = row(&mut api);
    assert!(on.values.contains(&"ratings-off".to_string()));
    assert!(on.options.contains(&"Stop syncing ratings".to_string()));
    assert!(api.adjust(&key, "ratings-off"));
    let off = row(&mut api);
    assert!(off.values.contains(&"ratings-on".to_string()));
    assert!(off.description.contains("Ratings are not synced with it."), "{}", off.description);
    assert!(!api.library().unwrap().rating_sync_enabled(&lab.origin()).unwrap());
    let track = lab.track("47gLTJvR49ADB1Pa0vPhB5");
    assert!(api.rate_as("track", track, 3));
    assert!(api.library().unwrap().due_ratings(i64::MAX, 10).unwrap().is_empty(), "off: nothing goes to the server");
    assert!(api.adjust(&key, "ratings-on"));
    assert!(api.rate_as("track", track, 4));
    let change = api.library().unwrap().due_ratings(i64::MAX, 10).unwrap().remove(0);
    api.library().unwrap().rating_failed(&change, "the server could not be reached", 0).unwrap();
    let waiting = row(&mut api);
    assert!(waiting.description.contains("1 rating waits to reach it: the server could not be reached"), "{}", waiting.description);
    assert!(!api.adjust(&key, "edit"), "anything else is not this feature's");
}
