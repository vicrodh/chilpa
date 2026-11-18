//! The registry against a network that answers only when the test lets it,
//! with the clock in the test's hands. No renderer on the LAN is needed.
use chilpa_cast::{discovery::Sighting, registry::*, Device, Error, Kind, Result};
use std::{collections::HashMap, path::Path, sync::{atomic::{AtomicBool, Ordering}, Arc, Condvar, Mutex}, time::{Duration, Instant, SystemTime}};

#[derive(Default)]
struct Wire {
    at: HashMap<String, Device>, located: Vec<String>, receivers: Vec<Sighting>,
    hold_describe: bool, hold_search: bool, hold_browse: bool, fail_search: bool, calls: Vec<String>,
}
#[derive(Default)]
struct Fixture { wire: Mutex<Wire>, turn: Condvar }
impl Fixture {
    fn edit(&self, change: impl FnOnce(&mut Wire)) { change(&mut self.wire.lock().unwrap()); self.turn.notify_all(); }
    fn calls(&self) -> Vec<String> { self.wire.lock().unwrap().calls.clone() }
    fn count(&self, call: &str) -> usize { self.calls().iter().filter(|c| c.starts_with(call)).count() }
}
impl Network for Fixture {
    fn describe(&self, location: &str, _limit: Duration) -> Result<Device> {
        let mut wire = self.wire.lock().unwrap();
        wire.calls.push(format!("describe {location}"));
        while wire.hold_describe { wire = self.turn.wait(wire).unwrap(); }
        wire.at.get(location).cloned().ok_or(Error::Network)
    }
    // A held call ignores cancellation, as a slow network would.
    fn search(&self, _window: Duration, _running: &AtomicBool) -> Result<Vec<String>> {
        let mut wire = self.wire.lock().unwrap();
        wire.calls.push("search".into());
        while wire.hold_search { wire = self.turn.wait(wire).unwrap(); }
        if wire.fail_search { Err(Error::Network) } else { Ok(wire.located.clone()) }
    }
    fn browse(&self, _window: Duration, _running: &AtomicBool, found: &mut dyn FnMut(Sighting) -> bool) -> Result<()> {
        let mut wire = self.wire.lock().unwrap();
        wire.calls.push("browse".into());
        while wire.hold_browse { wire = self.turn.wait(wire).unwrap(); }
        let receivers = wire.receivers.clone();
        drop(wire);
        for sighting in receivers { if !found(sighting) { break; } }
        Ok(())
    }
}

fn renderer(id: &str, name: &str, url: &str) -> Device {
    Device { id: id.into(), name: name.into(), model: "Renderer".into(), kind: Kind::Dlna, address: "192.0.2.1:80".parse().unwrap(), description: url.into() }
}
fn receiver(id: &str, name: &str, address: &str) -> Sighting {
    Sighting { device: Device { id: id.into(), name: name.into(), model: "Receiver".into(), kind: Kind::Chromecast, address: address.parse().unwrap(), description: String::new() }, fullname: format!("{name}._googlecast._tcp.local.") }
}
fn upnp(id: &str) -> EndpointKey { EndpointKey::new(Kind::Dlna, id) }
fn cast(id: &str) -> EndpointKey { EndpointKey::new(Kind::Chromecast, id) }

/// A controller and the time the test says it is.
struct Bench { controller: Controller, start: Instant, wall: SystemTime, at: Duration, outcomes: Vec<Outcome> }
impl Bench {
    fn new(path: Option<&Path>, fixture: &Arc<Fixture>) -> Self {
        let mut bench = Self { controller: Controller::new(path.map(Path::to_path_buf), fixture.clone()), start: Instant::now(),
            wall: SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000), at: Duration::ZERO, outcomes: Vec::new() };
        bench.until(|c| !c.loading());
        bench
    }
    fn now(&self) -> Instant { self.start + self.at }
    fn pump(&mut self) { let outcomes = self.controller.pump(self.start + self.at, self.wall + self.at); self.outcomes.extend(outcomes); }
    /// Pumps a few rounds: enough for threads that can answer to do so.
    fn step(&mut self) { for _ in 0..12 { self.pump(); std::thread::sleep(Duration::from_millis(4)); } }
    #[track_caller]
    fn until(&mut self, done: impl Fn(&Controller) -> bool) {
        let limit = Instant::now() + Duration::from_secs(5);
        loop { self.pump(); if done(&self.controller) { return; } assert!(Instant::now() < limit, "the registry did not get there: load {:?}", self.controller.load()); std::thread::sleep(Duration::from_millis(3)); }
    }
    /// Pumps until nothing is in flight or queued.
    #[track_caller]
    fn settle(&mut self) { self.until(|c| c.load() == Load::default()); self.step(); self.until(|c| c.load() == Load::default()); }
    fn open(&mut self) { self.controller.set_open(true, self.now()); }
    fn close(&mut self) { self.controller.set_open(false, self.now()); }
    fn presence(&self, key: &EndpointKey) -> Option<Presence> { self.controller.registry().get(key).map(|e| e.presence()) }
    fn names(&self) -> Vec<String> { self.controller.registry().endpoints().iter().map(|e| e.name.clone()).collect() }
    fn connects(&self) -> usize { self.outcomes.iter().filter(|o| matches!(o, Outcome::Connect(_))).count() }
}
/// A fixture with one renderer and one receiver, and a bench that found them.
fn found(path: Option<&Path>) -> (Arc<Fixture>, Bench) {
    let fixture = Arc::new(Fixture::default());
    fixture.edit(|w| {
        w.at.insert("http://192.0.2.1/a.xml".into(), renderer("uuid:a", "Hall", "http://192.0.2.1/a.xml"));
        w.located = vec!["http://192.0.2.1/a.xml".into()];
        w.receivers = vec![receiver("cast-1", "Kitchen", "192.0.2.2:8009")];
    });
    let mut bench = Bench::new(path, &fixture);
    bench.open();
    assert!(bench.controller.discover(bench.now()));
    bench.settle();
    assert_eq!(bench.names(), ["Hall", "Kitchen"]);
    (fixture, bench)
}

#[test]
fn a_restart_shows_remembered_rows_before_the_network_answers_and_connects_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("network-endpoints.json");
    let (fixture, bench) = found(Some(&path));
    drop(bench);
    assert!(path.exists(), "what was found has to be on disk once the registry goes");
    let before = fixture.calls().len();
    fixture.edit(|w| { w.hold_describe = true; w.hold_search = true; w.hold_browse = true; });
    let mut bench = Bench::new(Some(&path), &fixture);
    assert_eq!(bench.names(), ["Hall", "Kitchen"], "rows must not wait for the network");
    assert_eq!(bench.presence(&upnp("uuid:a")), Some(Presence::Unknown), "a stored endpoint is never restored as reachable");
    assert_eq!(fixture.calls().len(), before, "loading the registry asks nothing of the network");
    bench.open(); bench.step();
    assert_eq!(bench.names(), ["Hall", "Kitchen"]);
    assert_eq!(bench.presence(&upnp("uuid:a")), Some(Presence::Checking));
    assert_eq!(bench.presence(&cast("cast-1")), Some(Presence::Checking));
    assert_eq!(fixture.count("search"), 1, "opening with a known inventory must not search again");
    fixture.edit(|w| { w.hold_describe = false; w.hold_browse = false; });
    bench.settle();
    assert_eq!(bench.presence(&upnp("uuid:a")), Some(Presence::Reachable));
    assert_eq!(bench.presence(&cast("cast-1")), Some(Presence::Reachable));
    assert!(bench.outcomes.is_empty(), "opening and checking never connect: {:?}", bench.outcomes);
}

#[test]
fn reopening_with_fresh_answers_asks_nothing_and_an_empty_first_use_searches_once() {
    let fixture = Arc::new(Fixture::default());
    let mut bench = Bench::new(None, &fixture);
    bench.open(); bench.settle();
    assert_eq!((fixture.count("search"), fixture.count("browse")), (1, 1), "an empty inventory searches when the picker opens");
    for second in 1..6 { bench.close(); bench.at = Duration::from_secs(second); bench.open(); bench.settle(); }
    assert_eq!((fixture.count("search"), fixture.count("browse")), (1, 1), "reopening inside the cooldown must not search again");

    let (fixture, mut bench) = found(None);
    let asked = fixture.calls().len();
    for second in 1..6 { bench.close(); bench.at = Duration::from_secs(second * 5); bench.open(); bench.settle(); }
    assert_eq!(fixture.calls().len(), asked, "fresh answers are reused: {:?}", fixture.calls());
    bench.close(); bench.at = Duration::from_secs(40); bench.open(); bench.settle();
    assert_eq!(fixture.count("search"), 1, "stale answers are checked one by one, not by a new search");
    assert_eq!(fixture.count("describe"), 2);
    assert!(bench.controller.discover(bench.now()));
    assert!(bench.controller.discover(bench.now()), "asking twice is one search");
    bench.settle();
    assert_eq!(fixture.count("search"), 2, "a search asked for by hand always runs, once");
}

#[test]
fn a_slow_or_broken_upnp_search_does_not_hold_cast_back_or_empty_the_inventory() {
    let (fixture, mut bench) = found(None);
    fixture.edit(|w| { w.hold_search = true; w.receivers.push(receiver("cast-2", "Bedroom", "192.0.2.3:8009")); });
    bench.at = Duration::from_secs(60);
    assert!(bench.controller.discover(bench.now()));
    bench.until(|c| c.registry().get(&cast("cast-2")).is_some());
    assert!(bench.controller.searching(), "the UPnP search is still held");
    fixture.edit(|w| { w.fail_search = true; w.hold_search = false; });
    bench.settle();
    assert_eq!(bench.names(), ["Bedroom", "Hall", "Kitchen"], "a failed search removes nothing");
    assert!(bench.controller.notices().search_failed);
    assert!(!bench.controller.searching());
}

#[test]
fn renames_address_changes_and_reused_addresses_keep_identity_straight() {
    let (fixture, mut bench) = found(None);
    // A new name and a new address for the same identifiers.
    fixture.edit(|w| {
        w.at.clear();
        w.at.insert("http://192.0.2.50/a.xml".into(), renderer("uuid:a", "Hall renamed", "http://192.0.2.50/a.xml"));
        w.located = vec!["http://192.0.2.50/a.xml".into()];
        w.receivers = vec![receiver("cast-1", "Kitchen renamed", "192.0.2.60:8009")];
    });
    bench.close(); bench.at = Duration::from_secs(40); bench.open(); bench.settle();
    assert_eq!(bench.names(), ["Hall renamed", "Kitchen renamed"], "the same key takes the new name");
    let hall = bench.controller.registry().get(&upnp("uuid:a")).unwrap();
    assert_eq!(hall.description(), Some("http://192.0.2.50/a.xml"), "the repair search moves the locator of the same UDN");
    assert_eq!(hall.presence(), Presence::Reachable);
    assert_eq!(bench.controller.registry().get(&cast("cast-1")).unwrap().locators[0], Locator::Address("192.0.2.60:8009".into()));
    assert_eq!(fixture.count("search"), 2, "one repair search, for the renderer that stopped answering");

    // Another renderer now answers at the remembered address.
    fixture.edit(|w| { w.at.insert("http://192.0.2.50/a.xml".into(), renderer("uuid:other", "Stranger", "http://192.0.2.50/a.xml")); w.located.clear(); });
    bench.close(); bench.at = Duration::from_secs(80); bench.open(); bench.settle();
    assert_eq!(bench.presence(&upnp("uuid:a")), Some(Presence::Unavailable(Absence::WrongIdentity)), "an address answering with another UDN proves nothing about this one");
    assert_eq!(bench.names(), ["Hall renamed", "Kitchen renamed"], "a check never adds an endpoint nobody searched for");
    assert!(matches!(bench.controller.select(&upnp("uuid:a"), bench.now()), Selection::Pending));
    bench.settle();
    bench.at = Duration::from_secs(85); bench.settle();
    assert_eq!(bench.connects(), 0);
    assert!(bench.outcomes.iter().any(|o| matches!(o, Outcome::Unreachable(key) if key == &upnp("uuid:a"))), "the choice ends with a readable failure: {:?}", bench.outcomes);
}

#[test]
fn a_choice_connects_only_what_was_chosen_and_only_after_it_answers() {
    let (fixture, mut bench) = found(None);
    assert!(matches!(bench.controller.select(&cast("cast-1"), bench.now()), Selection::Ready(d) if d.id == "cast-1" && d.kind == Kind::Chromecast));
    assert!(matches!(bench.controller.select(&upnp("cast-1"), bench.now()), Selection::Unknown), "the identifier alone selects nothing");
    bench.close(); bench.at = Duration::from_secs(40);
    fixture.edit(|w| w.hold_describe = true);
    assert!(matches!(bench.controller.select(&upnp("uuid:a"), bench.now()), Selection::Pending));
    bench.step();
    assert!(bench.outcomes.is_empty());
    fixture.edit(|w| w.hold_describe = false);
    bench.settle();
    assert!(matches!(bench.outcomes.as_slice(), [Outcome::Connect(d)] if d.id == "uuid:a" && d.kind == Kind::Dlna), "{:?}", bench.outcomes);
    assert_eq!(fixture.count("browse"), 1, "choosing a renderer does not ask the receivers");

    // A choice withdrawn before the answer connects nothing.
    bench.outcomes.clear(); bench.at = Duration::from_secs(80);
    fixture.edit(|w| w.hold_describe = true);
    assert!(matches!(bench.controller.select(&upnp("uuid:a"), bench.now()), Selection::Pending));
    bench.step();
    bench.controller.deselect();
    fixture.edit(|w| w.hold_describe = false);
    bench.settle();
    assert!(bench.outcomes.is_empty(), "an answer after the choice was withdrawn must not connect: {:?}", bench.outcomes);
    assert_eq!(bench.presence(&upnp("uuid:a")), Some(Presence::Reachable));
}

#[test]
fn offline_asks_nothing_and_answers_from_the_previous_network_are_void() {
    let (fixture, mut bench) = found(None);
    bench.close(); bench.at = Duration::from_secs(40);
    fixture.edit(|w| w.hold_describe = true);
    bench.open();
    assert!(matches!(bench.controller.select(&upnp("uuid:a"), bench.now()), Selection::Pending));
    bench.step();
    bench.controller.set_online(false);
    let asked = fixture.calls().len();
    fixture.edit(|w| w.hold_describe = false);
    bench.settle();
    assert_eq!(bench.connects(), 0, "an answer that arrives offline connects nothing");
    assert_eq!(bench.presence(&upnp("uuid:a")), Some(Presence::Unknown), "the answer belonged to the network that was left");
    assert_eq!(bench.names(), ["Hall", "Kitchen"], "offline keeps what is remembered");
    assert!(!bench.controller.discover(bench.now()));
    bench.controller.refresh_known(bench.now());
    bench.close(); bench.open(); bench.settle();
    assert_eq!(fixture.calls().len(), asked, "offline sends nothing: {:?}", &fixture.calls()[asked..]);

    bench.outcomes.clear();
    bench.close();
    bench.controller.set_online(true);
    bench.step();
    assert_eq!(bench.presence(&cast("cast-1")), Some(Presence::Unknown), "coming back online trusts nothing from before");
    assert_eq!(fixture.calls().len(), asked, "and asks nothing until somebody looks");
    bench.open(); bench.settle();
    assert_eq!(bench.presence(&upnp("uuid:a")), Some(Presence::Reachable));
    assert!(bench.outcomes.is_empty(), "coming back online never connects");
}

#[test]
fn a_sleep_voids_fresh_answers() {
    let (fixture, mut bench) = found(None);
    let asked = fixture.calls().len();
    bench.close();
    // The wall clock moved an hour while the monotonic one moved a second.
    bench.at = Duration::from_secs(1); bench.wall += Duration::from_secs(3600);
    bench.pump();
    assert_eq!(bench.presence(&upnp("uuid:a")), Some(Presence::Unknown));
    bench.open(); bench.settle();
    assert!(fixture.calls().len() > asked, "after a sleep the picker checks again");
    assert!(bench.outcomes.is_empty());
}

#[test]
fn quick_reopening_stays_within_its_threads_and_the_forgotten_stay_forgotten() {
    let (fixture, mut bench) = found(None);
    for n in 0..40 {
        fixture.edit(|w| { w.at.insert(format!("http://192.0.2.9/{n}.xml"), renderer(&format!("uuid:{n}"), &format!("R{n}"), &format!("http://192.0.2.9/{n}.xml"))); w.located.push(format!("http://192.0.2.9/{n}.xml")); });
    }
    bench.at = Duration::from_secs(40);
    assert!(bench.controller.discover(bench.now()));
    bench.settle();
    assert_eq!(bench.names().len(), 42);
    fixture.edit(|w| { w.hold_describe = true; w.hold_search = true; w.hold_browse = true; });
    let searches = fixture.count("search");
    for round in 0..30u64 {
        bench.at = Duration::from_secs(80) + Duration::from_millis(round * 50);
        bench.open(); bench.controller.discover(bench.now()); bench.pump();
        let load = bench.controller.load();
        assert!(load.probes <= PROBES && load.scans <= 2, "reopening must not add threads: {load:?}");
        bench.close(); bench.pump();
    }
    assert!(fixture.count("search") <= searches + 1, "a cancelled search still holds its place");
    assert!(fixture.count("describe") <= 42 + 41 + PROBES, "probes in flight are reused, not repeated");

    // Forgotten while a search asked for by hand is still out.
    bench.open(); bench.controller.discover(bench.now()); bench.step();
    assert!(bench.controller.forget(&cast("cast-1")));
    assert!(bench.controller.forget(&upnp("uuid:a")));
    fixture.edit(|w| { w.hold_describe = false; w.hold_search = false; w.hold_browse = false; });
    bench.settle();
    assert!(bench.controller.registry().get(&cast("cast-1")).is_none(), "an answer to an earlier search must not bring it back");
    assert!(bench.controller.registry().get(&upnp("uuid:a")).is_none());
    assert!(bench.outcomes.is_empty());
    bench.at = Duration::from_secs(200);
    assert!(bench.controller.discover(bench.now()));
    bench.settle();
    assert!(bench.controller.registry().get(&cast("cast-1")).is_some(), "a search asked for afterwards finds it again");
    assert!(bench.controller.registry().get(&upnp("uuid:a")).is_some());
}

#[test]
fn the_endpoint_in_use_is_never_probed_or_forgotten() {
    let (fixture, mut bench) = found(None);
    bench.controller.set_active(Some(upnp("uuid:a")));
    bench.close(); bench.at = Duration::from_secs(40); bench.open(); bench.settle();
    assert_eq!(fixture.count("describe"), 1, "the session's own answers are the evidence for its renderer");
    assert!(!bench.controller.forget(&upnp("uuid:a")), "the output in use is not forgotten");
    bench.controller.session_alive(&upnp("uuid:a"), bench.now());
    assert_eq!(bench.presence(&upnp("uuid:a")), Some(Presence::Reachable));
    bench.controller.set_active(None);
    assert!(bench.controller.forget(&upnp("uuid:a")));
}

#[test]
fn an_absent_receiver_is_unavailable_and_stays_remembered() {
    let (fixture, mut bench) = found(None);
    fixture.edit(|w| { w.receivers.clear(); w.at.clear(); w.located.clear(); });
    bench.close(); bench.at = Duration::from_secs(40); bench.open(); bench.settle();
    assert_eq!(bench.presence(&cast("cast-1")), Some(Presence::Unavailable(Absence::NoAnswer)));
    assert_eq!(bench.presence(&upnp("uuid:a")), Some(Presence::Unavailable(Absence::NoAnswer)));
    assert_eq!(bench.names(), ["Hall", "Kitchen"]);
    let searches = fixture.count("search");
    bench.close(); bench.at = Duration::from_secs(75); bench.open(); bench.settle();
    assert_eq!(fixture.count("search"), searches, "a renderer already known to be away does not start another repair search");
}

fn bytes(path: &Path) -> Vec<u8> { std::fs::read(path).unwrap() }

#[test]
fn unusable_files_are_kept_and_the_registry_works_in_memory() {
    let dir = tempfile::tempdir().unwrap();
    let long = format!(r#"{{"schema_version":1,"endpoints":[{{"protocol":"upnp","stable_id":"{}","name":"n","model":"m"}}]}}"#, "x".repeat(MAX_ID + 1));
    let twice = r#"{"schema_version":1,"endpoints":[{"protocol":"cast","stable_id":"a","name":"n","model":"m"},{"protocol":"cast","stable_id":"a","name":"n","model":"m"}]}"#;
    let huge = format!(r#"{{"schema_version":1,"endpoints":[],"padding":"{}"}}"#, "p".repeat(MAX_BYTES));
    let cases: [(&str, &str, Unusable); 6] = [
        ("cut", r#"{"schema_version":1,"endpoints":[{"protocol":"up"#, Unusable::Damaged),
        ("newer", r#"{"schema_version":2,"endpoints":[],"groups":[]}"#, Unusable::Newer),
        ("long", &long, Unusable::Damaged), ("twice", twice, Unusable::Damaged), ("huge", &huge, Unusable::Damaged),
        ("protocol", r#"{"schema_version":1,"endpoints":[{"protocol":"airplay","stable_id":"a","name":"n","model":"m"}]}"#, Unusable::Damaged),
    ];
    for (name, content, why) in cases {
        let path = dir.path().join(format!("{name}.json"));
        std::fs::write(&path, content).unwrap();
        let (fixture, _) = found(None);
        let mut bench = Bench::new(Some(&path), &fixture);
        assert_eq!(bench.controller.notices().kept, Some(why), "{name}");
        bench.open(); bench.settle();
        bench.close(); bench.at = Duration::from_secs(40); bench.open(); bench.settle();
        assert_eq!(bytes(&path), content.as_bytes(), "{name}: the original must not be overwritten silently");
        assert!(!set_aside_path(&path).exists(), "{name}");
        // A search asked for by hand starts a new list; only a damaged file gives way, and it is kept aside.
        assert!(bench.controller.discover(bench.now()));
        bench.settle();
        assert_eq!(bench.names(), ["Hall", "Kitchen"], "{name}: memory keeps working");
        drop(bench);
        if why == Unusable::Newer {
            assert_eq!(bytes(&path), content.as_bytes(), "a newer file is never replaced");
        } else {
            assert_eq!(bytes(&set_aside_path(&path)), content.as_bytes(), "{name}: the damaged original is kept aside");
            assert_eq!(parse(&bytes(&path)).unwrap().len(), 2, "{name}");
        }
    }
}

#[cfg(unix)]
#[test]
fn a_failed_write_leaves_memory_working_and_says_so() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let closed = dir.path().join("closed");
    std::fs::create_dir(&closed).unwrap();
    std::fs::set_permissions(&closed, std::fs::Permissions::from_mode(0o555)).unwrap();
    let path = closed.join("network-endpoints.json");
    let (_, mut bench) = found(Some(&path));
    bench.until(|c| c.notices().unsaved);
    assert_eq!(bench.names(), ["Hall", "Kitchen"]);
    assert_eq!(std::fs::read_dir(&closed).unwrap().count(), 0, "a failed write leaves nothing behind");
    for _ in 0..20 { bench.step(); }
    assert_eq!(std::fs::read_dir(&closed).unwrap().count(), 0, "a failed write is not tried again on every round");
    std::fs::set_permissions(&closed, std::fs::Permissions::from_mode(0o755)).unwrap();
    drop(bench);
    assert_eq!(parse(&bytes(&path)).unwrap().len(), 2, "what could not be written is written when the registry goes");
}

#[test]
fn a_stranger_at_a_remembered_address_is_found_by_a_search_asked_for_by_hand() {
    let (fixture, mut bench) = found(None);
    fixture.edit(|w| { w.at.insert("http://192.0.2.1/a.xml".into(), renderer("uuid:new", "Newcomer", "http://192.0.2.1/a.xml")); });
    bench.close(); bench.at = Duration::from_secs(40); bench.open(); bench.settle();
    assert_eq!(bench.presence(&upnp("uuid:a")), Some(Presence::Unavailable(Absence::WrongIdentity)));
    assert_eq!(bench.names(), ["Hall", "Kitchen"]);
    bench.at = Duration::from_secs(45);
    assert!(bench.controller.discover(bench.now()));
    bench.settle();
    assert_eq!(bench.names(), ["Hall", "Kitchen", "Newcomer"], "an address is not an identity: whoever answers there now is found");
    assert_eq!(bench.presence(&upnp("uuid:a")), Some(Presence::Unavailable(Absence::WrongIdentity)));
    assert_eq!(bench.presence(&upnp("uuid:new")), Some(Presence::Reachable));
}

#[test]
fn a_list_emptied_by_hand_stays_empty_until_a_search_is_asked_for() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("network-endpoints.json");
    let (fixture, mut bench) = found(Some(&path));
    bench.at = Duration::from_secs(60);
    assert!(bench.controller.forget(&upnp("uuid:a")) && bench.controller.forget(&cast("cast-1")));
    let asked = fixture.calls().len();
    for second in 61..140 { bench.at = Duration::from_secs(second); bench.step(); }
    bench.close(); bench.at = Duration::from_secs(200); bench.open(); bench.settle();
    assert!(bench.names().is_empty(), "nobody asked for a search: {:?}", bench.names());
    assert_eq!(fixture.calls().len(), asked, "an emptied list is not a first use");
    drop(bench);
    let mut bench = Bench::new(Some(&path), &fixture);
    bench.open(); bench.settle();
    assert_eq!(fixture.calls().len(), asked, "nor is it one after a restart");
    assert!(bench.controller.discover(bench.now()));
    bench.settle();
    assert_eq!(bench.names(), ["Hall", "Kitchen"]);
}

#[test]
fn a_search_left_behind_by_going_offline_admits_nothing() {
    let fixture = Arc::new(Fixture::default());
    fixture.edit(|w| {
        w.at.insert("http://192.0.2.1/a.xml".into(), renderer("uuid:a", "Hall", "http://192.0.2.1/a.xml"));
        w.located = vec!["http://192.0.2.1/a.xml".into()];
        w.receivers = vec![receiver("cast-1", "Kitchen", "192.0.2.2:8009")];
        w.hold_search = true; w.hold_browse = true;
    });
    let mut bench = Bench::new(None, &fixture);
    bench.open(); bench.step();
    assert_eq!((fixture.count("search"), fixture.count("browse")), (1, 1));
    bench.controller.set_online(false);
    bench.close();
    bench.controller.set_online(true);
    fixture.edit(|w| { w.hold_search = false; w.hold_browse = false; });
    bench.settle();
    assert!(bench.names().is_empty(), "answers to a search from before belong to the network that was left: {:?}", bench.names());
    assert_eq!(fixture.count("describe"), 0);
    assert!(!bench.controller.searching());
}

#[test]
fn equal_answers_write_nothing_and_rebuild_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("network-endpoints.json");
    let (fixture, mut bench) = found(Some(&path));
    bench.until(|_| path.exists());
    let stored = parse(&bytes(&path)).unwrap();
    assert_eq!(stored.len(), 2);
    assert_eq!(stored[0].locators, [Locator::Description("http://192.0.2.1/a.xml".into())]);
    let text = String::from_utf8(bytes(&path)).unwrap();
    assert!(text.contains("\"schema_version\": 1") && !text.contains("eachable") && !text.contains("resence"), "presence is never stored: {text}");
    std::fs::remove_file(&path).unwrap();
    let revision = bench.controller.registry().revision();
    for round in 1..4 { bench.close(); bench.at = Duration::from_secs(40 * round); bench.open(); bench.settle(); }
    assert!(fixture.count("describe") >= 4, "the endpoints were asked again");
    assert_eq!(bench.controller.registry().revision(), revision, "the same answers must not change what the rows show");
    assert!(!path.exists(), "the same answers must not be written again");
    assert!(bench.controller.forget(&cast("cast-1")));
    bench.until(|_| path.exists());
    assert_eq!(parse(&bytes(&path)).unwrap().len(), 1, "forgetting removes the stored record");
}

/// Every request a real HTTP renderer receives while the registry opens,
/// checks, fails, closes and goes away.
#[test]
fn checking_a_real_renderer_sends_only_description_reads() {
    struct Hybrid { location: String }
    impl Network for Hybrid {
        fn describe(&self, location: &str, limit: Duration) -> Result<Device> { Lan.describe(location, limit) }
        fn search(&self, _window: Duration, _running: &AtomicBool) -> Result<Vec<String>> { Ok(vec![self.location.clone()]) }
        fn browse(&self, _window: Duration, _running: &AtomicBool, _found: &mut dyn FnMut(Sighting) -> bool) -> Result<()> { Ok(()) }
    }
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let address = server.server_addr().to_ip().unwrap();
    let seen = Arc::new(Mutex::new(Vec::<String>::new()));
    let (log, done) = (seen.clone(), Arc::new(AtomicBool::new(false)));
    let stop = done.clone();
    let serving = std::thread::spawn(move || {
        while !stop.load(Ordering::Acquire) {
            let Some(request) = server.recv_timeout(Duration::from_millis(20)).unwrap() else { continue };
            log.lock().unwrap().push(format!("{} {}", request.method(), request.url()));
            let broken = log.lock().unwrap().len() == 3;
            let xml = "<root><device><deviceType>urn:schemas-upnp-org:device:MediaRenderer:1</deviceType><friendlyName>Busy renderer</friendlyName><UDN>uuid:busy</UDN><serviceList><service><serviceType>urn:schemas-upnp-org:service:AVTransport:1</serviceType><controlURL>/transport</controlURL></service></serviceList></device></root>";
            let _ = if broken { request.respond(tiny_http::Response::from_string("gone").with_status_code(500)) } else { request.respond(tiny_http::Response::from_string(xml)) };
        }
    });
    let location = format!("http://{address}/device.xml");
    let mut bench = Bench { controller: Controller::new(None, Arc::new(Hybrid { location })), start: Instant::now(), wall: SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000), at: Duration::ZERO, outcomes: Vec::new() };
    bench.open(); bench.settle();
    assert_eq!(bench.names(), ["Busy renderer"]);
    for round in 1..5 { bench.close(); bench.at = Duration::from_secs(40 * round); bench.open(); bench.settle(); }
    assert!(matches!(bench.controller.select(&upnp("uuid:busy"), bench.now()), Selection::Ready(_)));
    bench.close();
    bench.controller.set_online(false);
    drop(bench);
    std::thread::sleep(Duration::from_millis(100));
    done.store(true, Ordering::Release); serving.join().unwrap();
    let seen = seen.lock().unwrap();
    assert!(seen.len() >= 5, "{seen:?}");
    assert!(seen.iter().all(|request| request == "GET /device.xml"), "a check may only read the description: {seen:?}");
}

/// Half an hour of openings, checks and searches leaves nothing behind.
#[test]
fn many_cycles_end_with_nothing_in_flight_and_the_same_rows() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("network-endpoints.json");
    let (fixture, mut bench) = found(Some(&path));
    bench.until(|_| path.exists());
    let written = std::fs::metadata(&path).unwrap().modified().unwrap();
    let revision = bench.controller.registry().revision();
    for cycle in 1..=180u64 {
        bench.close();
        bench.at = Duration::from_secs(cycle * 10);
        bench.open();
        if cycle % 30 == 0 { assert!(bench.controller.discover(bench.now())); }
        bench.until(|c| c.load() == Load::default());
        let load = bench.controller.load();
        assert!(load.probes <= PROBES && load.scans <= 2 && load.queued <= 2, "cycle {cycle}: {load:?}");
    }
    bench.settle();
    assert_eq!(bench.controller.load(), Load::default());
    assert_eq!(bench.names(), ["Hall", "Kitchen"]);
    assert_eq!(bench.controller.registry().revision(), revision, "thirty minutes of the same answers rebuild no row");
    assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(), written, "and write nothing");
    assert!(bench.outcomes.is_empty());
    assert_eq!(fixture.count("search"), 1 + 6, "only the searches asked for by hand");
}
