//! The remembered renderers through the facade: rows before the network
//! answers, nothing sent but description reads until a row is chosen by hand.
use chilpa_api::Chilpa;
use chilpa_cast::{discovery::Sighting, registry::{Lan, Network}, Device, Result};
use serde_json::Value;
use std::{sync::{atomic::{AtomicBool, Ordering}, Arc, Condvar, Mutex}, time::{Duration, Instant}};

/// A renderer that is busy with somebody else: it records every request.
struct Renderer { location: String, seen: Arc<Mutex<Vec<String>>>, done: Arc<AtomicBool>, thread: Option<std::thread::JoinHandle<()>> }
impl Renderer {
    fn new() -> Self {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let address = server.server_addr().to_ip().unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let done = Arc::new(AtomicBool::new(false));
        let (log, stop) = (seen.clone(), done.clone());
        let thread = std::thread::spawn(move || {
            while !stop.load(Ordering::Acquire) {
                let Some(request) = server.recv_timeout(Duration::from_millis(20)).unwrap() else { continue };
                let action = request.headers().iter().find(|h| h.field.equiv("SOAPAction")).map(|h| h.value.as_str().trim_matches('"').rsplit('#').next().unwrap_or("").to_string());
                log.lock().unwrap().push(action.clone().unwrap_or_else(|| format!("{} {}", request.method(), request.url())));
                let body = match action.as_deref() {
                    None => "<root><device><deviceType>urn:schemas-upnp-org:device:MediaRenderer:1</deviceType><friendlyName>Busy renderer</friendlyName><modelName>Fixture</modelName><UDN>uuid:busy</UDN><serviceList><service><serviceType>urn:schemas-upnp-org:service:AVTransport:1</serviceType><controlURL>/transport</controlURL></service><service><serviceType>urn:schemas-upnp-org:service:RenderingControl:1</serviceType><controlURL>/volume</controlURL></service></serviceList></device></root>".to_string(),
                    Some("GetTransportInfo") => "<response><CurrentTransportState>STOPPED</CurrentTransportState></response>".into(),
                    Some(_) => "<response></response>".into(),
                };
                let _ = request.respond(tiny_http::Response::from_string(body));
            }
        });
        Self { location: format!("http://{address}/device.xml"), seen, done, thread: Some(thread) }
    }
    fn seen(&self) -> Vec<String> { self.seen.lock().unwrap().clone() }
}
impl Drop for Renderer { fn drop(&mut self) { self.done.store(true, Ordering::Release); self.thread.take().unwrap().join().unwrap(); } }

/// The LAN reduced to one renderer; description reads are real HTTP and can
/// be held back.
struct Wire { location: String, held: Mutex<bool>, turn: Condvar }
impl Wire {
    fn hold(&self, held: bool) { *self.held.lock().unwrap() = held; self.turn.notify_all(); }
}
impl Network for Wire {
    fn describe(&self, location: &str, limit: Duration) -> Result<Device> {
        let mut held = self.held.lock().unwrap();
        while *held { held = self.turn.wait(held).unwrap(); }
        drop(held);
        Lan.describe(location, limit)
    }
    fn search(&self, _window: Duration, _running: &AtomicBool) -> Result<Vec<String>> { Ok(vec![self.location.clone()]) }
    fn browse(&self, _window: Duration, _running: &AtomicBool, _found: &mut dyn FnMut(Sighting) -> bool) -> Result<()> { Ok(()) }
}

fn snapshot(api: &Chilpa) -> Value { serde_json::from_str(&api.cast_json()).unwrap() }
fn row(api: &Chilpa, act: &str) -> Option<Value> {
    snapshot(api)["entries"].as_array().unwrap().iter().find(|e| e["act"] == act && (act != "connect" || e["id"] == "uuid:busy")).cloned()
}
#[track_caller]
fn until(api: &mut Chilpa, what: &str, done: impl Fn(&Chilpa) -> bool) {
    let limit = Instant::now() + Duration::from_secs(15);
    loop { api.poll(); if done(api) { return; } assert!(Instant::now() < limit, "never happened: {what}; menu: {}", api.cast_json()); std::thread::sleep(Duration::from_millis(20)); }
}

#[test]
fn remembered_renderers_show_at_once_and_only_a_choice_by_hand_connects() {
    let dir = tempfile::tempdir().unwrap();
    for key in ["XDG_DATA_HOME", "XDG_CONFIG_HOME", "XDG_CACHE_HOME"] { std::env::set_var(key, dir.path().join(key)); }
    std::env::set_var("TMPDIR", dir.path());
    let renderer = Renderer::new();
    let wire = Arc::new(Wire { location: renderer.location.clone(), held: Mutex::new(false), turn: Condvar::new() });

    // First run: the search asked for by hand finds the renderer.
    let mut api = Chilpa::new();
    api.cast_inventory_over(wire.clone());
    api.cast_picker(true);
    api.cast_act("discover", "", "");
    until(&mut api, "the renderer is listed", |api| row(api, "connect").is_some());
    let found = row(&api, "connect").unwrap();
    assert_eq!((found["protocol"].as_str(), found["text"].as_str(), found["detail"].as_str()), (Some("upnp"), Some("Busy renderer"), Some("Fixture")));
    api.cast_picker(false);
    drop(api);

    // Second run: the network holds every answer back.
    wire.hold(true);
    let asked = renderer.seen().len();
    let mut api = Chilpa::new();
    api.cast_inventory_over(wire.clone());
    until(&mut api, "the stored list is read", |api| row(api, "connect").is_some());
    assert_eq!(renderer.seen().len(), asked, "starting with a stored list asks nothing of the network");
    api.cast_picker(true);
    api.poll();
    let remembered = row(&api, "connect").unwrap();
    assert_eq!(remembered["checked"], false);
    let detail = remembered["detail"].as_str().unwrap().to_string();
    assert!(detail.starts_with("Fixture · ") && detail != "Fixture", "a restored renderer is not shown as available: {detail}");
    assert!(api.cast_destination().is_none() && !api.poll().playing, "opening the picker connects nothing");
    wire.hold(false);
    until(&mut api, "the renderer answers", |api| row(api, "connect").is_some_and(|e| e["detail"] == "Fixture"));

    // Opening, closing and checking again, with the renderer busy elsewhere.
    for _ in 0..3 { api.cast_picker(false); api.poll(); api.cast_picker(true); api.poll(); }
    api.cast_act("connect", "cast", "uuid:busy");
    api.cast_act("forget-pick", "", "");
    assert!(row(&api, "forget").is_some_and(|e| e["keepOpen"] == true && e["enabled"] == true));
    api.cast_act("forget-done", "", "");
    for _ in 0..10 { api.poll(); std::thread::sleep(Duration::from_millis(20)); }
    let seen = renderer.seen();
    assert!(seen.iter().all(|request| request == "GET /device.xml"), "checking may only read the description: {seen:?}");
    assert!(api.cast_destination().is_none(), "the identifier under another protocol selects nothing");

    // Chosen by hand: the normal connection, and only now a transport command.
    api.cast_act("connect", "upnp", "uuid:busy");
    until(&mut api, "the chosen renderer connects", |api| snapshot(api)["active"] == true && renderer.seen().iter().any(|request| request == "GetTransportInfo"));
    assert_eq!(api.cast_destination().as_deref(), Some("Busy renderer"));
    assert!(renderer.seen().iter().any(|request| request == "Stop"), "an explicit choice still connects the way it did");
    assert!(row(&api, "connect").is_some_and(|e| e["checked"] == true));

    // The output in use is not forgotten; a free one is, for good.
    api.cast_act("forget", "upnp", "uuid:busy");
    assert!(row(&api, "connect").is_some(), "the renderer in use stays");
    assert!(!snapshot(&api)["error"].as_str().unwrap().is_empty(), "and the menu says why");
    api.cast_act("local", "", "");
    until(&mut api, "the renderer is released", |api| !snapshot(api)["active"].as_bool().unwrap());
    api.cast_act("forget", "upnp", "uuid:busy");
    assert!(row(&api, "connect").is_none());
    api.cast_picker(false);
    drop(api);
    let mut api = Chilpa::new();
    api.cast_inventory_over(wire);
    for _ in 0..15 { api.poll(); std::thread::sleep(Duration::from_millis(20)); }
    assert!(row(&api, "connect").is_none(), "a forgotten renderer does not come back with the next start");
}
