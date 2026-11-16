//! Ratings between Chilpa and a plugin that declares `rating`: its lists
//! say the values, `rating.set` takes a change. The plugin is a fake: a
//! side that keeps what it was asked, and a process speaking the protocol
//! over pipes.

use super::*;
use crate::ratings::writer::{self, Plugins, Tools};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

const PLUGIN: &str = "test.rating.source";

fn api_with(capabilities: Value) -> Chilpa {
    let mut api = Chilpa::new();
    api.library = Some(chilpa_library::Library::in_memory().unwrap());
    let manifest: chilpa_plugin::Manifest = serde_json::from_value(json!({"id": PLUGIN, "name": "Fixture", "version": "1.0.0", "api_version": 1, "kind": "source", "runtime": "process"})).unwrap();
    crate::natives::tests::remember_source(&api.natives, manifest, capabilities);
    api
}

fn ten() -> Value {
    json!({"rating": {"min": 1, "max": 10, "step": 1, "kinds": ["track", "album"]}})
}

/// An item of the plugin's lists, as the catalog keeps it.
fn listed(api: &Chilpa, item: Value) -> i64 {
    let collection = item["kind"] != "track";
    api.catalog.adopt(PLUGIN, None, &crate::source_search::RemoteFound {
        source: PLUGIN.into(), name: "Fixture".into(), id: item["ref"].as_str().unwrap().into(), title: "Item".into(),
        detail: String::new(), art: String::new(), duration_ms: 0, collection, entity: item["kind"].as_str().unwrap().into(),
        item: Some(item), no_audio: false,
    })
}

/// A plugin side that keeps what it was asked, and says the source took
/// it, as the running one does, through the catalog.
struct Fake {
    capability: Value,
    asked: Mutex<Vec<(String, Value)>>,
    refuse: bool,
    catalog: Arc<crate::source_catalog::Catalog>,
}

impl Plugins for Fake {
    fn capability(&self, _: &str) -> Value {
        self.capability.clone()
    }
    fn call(&self, _: &str, method: &str, params: Value) -> Result<Value, String> {
        self.asked.lock().unwrap().push((method.into(), params));
        if self.refuse { Err("the plugin is not signed in".into()) } else { Ok(json!({})) }
    }
    fn took(&self, plugin: &str, kind: &str, identity: &str, value: f64) {
        self.catalog.set_source_rating(plugin, kind, identity, value);
    }
}

fn tools(fake: Arc<Fake>) -> Tools {
    let root = std::env::temp_dir().join(format!("chilpa-rating-keys-{}", crate::servers::keys::mint()));
    Tools { net: Arc::new(crate::servers::ratings::network()), keys: Arc::new(crate::servers::keys::Keys::derived(root)), plugins: fake, open: Arc::new(|| None) }
}

fn fake(api: &Chilpa, refuse: bool) -> Arc<Fake> {
    Arc::new(Fake { capability: ten()["rating"].clone(), asked: Mutex::new(Vec::new()), refuse, catalog: api.catalog.clone() })
}

#[test]
fn a_plugin_s_listed_ratings_come_in_as_stars() {
    let mut api = api_with(ten());
    let album = listed(&api, json!({"kind": "album", "ref": "album:5", "title": "Record", "rating": 7}));
    let track = listed(&api, json!({"kind": "track", "ref": "track:9", "title": "Song", "rating": 6}));
    let row = listed(&api, json!({"kind": "track", "ref": "track:9@playlist:2@entry:4", "title": "Song", "rating": 6}));
    let unrated = listed(&api, json!({"kind": "track", "ref": "track:10", "title": "Other", "rating": 0}));
    api.import_plugin_ratings();
    let marks = api.marks().unwrap();
    assert_eq!(marks.stars("album", album), 4, "7 of 10 shows as 4");
    assert_eq!((marks.stars("track", track), marks.stars("track", row), marks.stars("track", unrated)), (3, 3, 0));
    let lib = api.library().unwrap();
    assert_eq!(lib.remote_rating("plugin:test.rating.source", "album", "album:5").unwrap(), Some(7.0));
    assert!(lib.due_ratings(i64::MAX, 10).unwrap().is_empty(), "taking a value in never writes it back");
}

#[test]
fn a_plugin_without_the_capability_is_left_alone() {
    let mut api = api_with(json!({"search": true}));
    let track = listed(&api, json!({"kind": "track", "ref": "track:9", "title": "Song", "rating": 8}));
    api.import_plugin_ratings();
    assert_eq!(api.marks().unwrap().stars("track", track), 0);
    assert!(api.rate_as("track", track, 2));
    assert!(api.library().unwrap().due_ratings(i64::MAX, 10).unwrap().is_empty(), "nothing goes to a plugin that rates nothing");
}

#[test]
fn the_listener_s_change_goes_to_the_plugin_with_rating_set_and_untouched_items_stay() {
    let mut api = api_with(ten());
    let track = listed(&api, json!({"kind": "track", "ref": "track:9", "title": "Song", "rating": 7}));
    let other = listed(&api, json!({"kind": "track", "ref": "track:11", "title": "Other", "rating": 3}));
    api.import_plugin_ratings();
    assert!(api.rate_as("track", track, 4), "the stars it shows: no change");
    assert!(api.rate_as("track", track, 5));
    let plugin = fake(&api, false);
    let outcome = writer::run(api.library().unwrap(), &tools(plugin.clone()), crate::now_unix());
    assert_eq!((outcome.sent, outcome.failing), (1, 0));
    assert_eq!(*plugin.asked.lock().unwrap(), [("rating.set".to_string(), json!({"ref": "track:9", "kind": "track", "value": 10.0}))]);
    api.import_plugin_ratings();
    let marks = api.marks().unwrap();
    assert_eq!((marks.stars("track", track), marks.stars("track", other)), (5, 2), "its lists now say 10; the other keeps its 3 of 10");
    assert!(api.rate_as("track", track, 0));
    writer::run(api.library().unwrap(), &tools(plugin.clone()), crate::now_unix());
    assert_eq!(plugin.asked.lock().unwrap()[1].1, json!({"ref": "track:9", "kind": "track", "value": null}), "null takes it off");
    assert_eq!(plugin.asked.lock().unwrap().len(), 2, "track:11 was never written");
}

#[test]
fn a_kind_the_plugin_does_not_rate_is_not_sent() {
    let mut api = api_with(json!({"rating": {"min": 0.5, "max": 5, "step": 0.5, "kinds": ["track"]}}));
    let album = listed(&api, json!({"kind": "album", "ref": "album:5", "title": "Record", "rating": 4.5}));
    let track = listed(&api, json!({"kind": "track", "ref": "track:9", "title": "Song", "rating": 2.5}));
    api.import_plugin_ratings();
    let marks = api.marks().unwrap();
    assert_eq!((marks.stars("album", album), marks.stars("track", track)), (0, 3), "half stars go up; records are not this plugin's");
    assert!(api.rate_as("album", album, 2));
    assert!(api.library().unwrap().due_ratings(i64::MAX, 10).unwrap().is_empty());
}

#[test]
fn a_plugin_that_cannot_take_it_now_is_asked_again_later() {
    let mut api = api_with(ten());
    let track = listed(&api, json!({"kind": "track", "ref": "track:9", "title": "Song", "rating": 2}));
    api.import_plugin_ratings();
    assert!(api.rate_as("track", track, 3));
    let refusing = fake(&api, true);
    let now = 5_000;
    let failed = writer::run(api.library().unwrap(), &tools(refusing), now);
    assert_eq!((failed.failing, failed.next_due), (1, Some(now + 30)));
    let failures = api.rating_failures();
    assert_eq!(failures["plugin:test.rating.source"], (1, "the plugin is not signed in".to_string()));
    let taking = fake(&api, false);
    assert_eq!(writer::run(api.library().unwrap(), &tools(taking.clone()), now + 30).sent, 1);
    assert_eq!(taking.asked.lock().unwrap()[0].1["value"], json!(6.0));
}

#[test]
fn the_window_is_told_once_when_ratings_start_failing() {
    let mut api = api_with(ten());
    api.now.error.clear();
    api.servers.ratings.failing = 0;
    let (tx, rx) = std::sync::mpsc::channel();
    tx.send(writer::Outcome { failing: 2, next_due: Some(i64::MAX), ..Default::default() }).unwrap();
    tx.send(writer::Outcome { failing: 3, next_due: Some(i64::MAX), ..Default::default() }).unwrap();
    api.servers.ratings.writer = Some(writer::Writer::for_tests(rx));
    api.poll_ratings();
    assert_eq!(api.now.error, "Some ratings or favorites could not reach their source. Chilpa will try again.");
    api.now.error.clear();
    let (tx, rx) = std::sync::mpsc::channel();
    tx.send(writer::Outcome { failing: 3, next_due: Some(i64::MAX), ..Default::default() }).unwrap();
    api.servers.ratings.writer = Some(writer::Writer::for_tests(rx));
    api.poll_ratings();
    assert!(api.now.error.is_empty(), "not again while they keep failing");
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn rating_set_reaches_a_process_plugin_over_its_protocol() {
    use std::io::{BufRead, Write};
    let mut api = api_with(ten());
    let manifest: chilpa_plugin::Manifest = serde_json::from_value(json!({"id": PLUGIN, "name": "Fixture", "version": "1.0.0", "api_version": 1, "kind": "source", "runtime": "process"})).unwrap();
    let (client, (plugin_reads, mut plugin_writes)) = crate::natives::tests::channel();
    crate::natives::tests::started(&api.natives, manifest.clone(), std::path::Path::new("/nonexistent"), &client);
    crate::natives::tests::remember_source(&api.natives, manifest, ten());
    let told = Arc::new(Mutex::new(Vec::new()));
    let heard = told.clone();
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(plugin_reads).lines().map_while(Result::ok) {
            let message: Value = serde_json::from_str(&line).unwrap();
            heard.lock().unwrap().push((message["method"].clone(), message["params"].clone()));
            let reply = json!({"jsonrpc": "2.0", "id": message["id"], "result": {}});
            if writeln!(plugin_writes, "{reply}").is_err() { break; }
        }
    });
    let album = listed(&api, json!({"kind": "album", "ref": "album:5", "title": "Record", "rating": 0}));
    api.import_plugin_ratings();
    assert!(api.rate_as("album", album, 4));
    let root = std::env::temp_dir().join(format!("chilpa-rating-keys-{}", crate::servers::keys::mint()));
    let tools = Tools::new(Arc::new(crate::servers::keys::Keys::derived(root)), api.natives.clone(), api.catalog.clone());
    let outcome = writer::run(api.library().unwrap(), &tools, crate::now_unix());
    assert_eq!(outcome.sent, 1);
    assert_eq!(*told.lock().unwrap(), [(json!("rating.set"), json!({"ref": "album:5", "kind": "album", "value": 8.0}))]);
    assert_eq!(api.catalog.source_ratings().into_iter().find(|r| r.2 == "album:5").map(|r| r.3), Some(8.0), "its lists say what it took");
    client.stop();
}
