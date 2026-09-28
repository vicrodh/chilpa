//! Protocol tests use a private bus and never touch an audio device.

use super::*;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use dbus::blocking::stdintf::org_freedesktop_dbus::Properties;
use dbus::channel::{MatchingReceiver, Sender};
use dbus::message::MatchRule;

struct TestBus { child: Child, address: String }

impl TestBus {
    fn new() -> Self {
        let mut child = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped()).spawn().expect("private session bus");
        let mut address = String::new();
        BufReader::new(child.stdout.take().unwrap()).read_line(&mut address).unwrap();
        Self { child, address: address.trim().into() }
    }

    fn connection(&self) -> Connection { Connection::new_address(&self.address).unwrap() }

    fn owner(&self, name: &str) -> Option<String> {
        self.connection().with_proxy("org.freedesktop.DBus", "/org/freedesktop/DBus", PATIENCE)
            .method_call("org.freedesktop.DBus", "GetNameOwner", (name,))
            .ok().map(|(owner,): (String,)| owner)
    }
}

impl Drop for TestBus {
    fn drop(&mut self) { let _ = self.child.kill(); let _ = self.child.wait(); }
}

const CARD: u32 = 999;
const NAME: &str = "org.freedesktop.ReserveDevice1.Audio999";
const PATH: &str = "/org/freedesktop/ReserveDevice1/Audio999";

enum Reply { Release, Refuse, Ignore, Error }

fn sound_server(bus: &TestBus, reply: Reply) -> Server {
    let connection = bus.connection();
    assert_eq!(connection.request_name(NAME, false, false, true).unwrap(), RequestNameReply::PrimaryOwner);
    connection.start_receive(MatchRule::new_method_call(), Box::new(move |message, connection| {
        if message.member().is_some_and(|m| m == "RequestRelease") {
            assert_eq!(message.read1::<i32>().unwrap(), PRIORITY);
            // Inspect the requester before answering, as a real sound server
            // can do. A caller that simply blocks here would deadlock.
            let requester = message.sender().unwrap();
            let proxy = connection.with_proxy(requester, PATH, PATIENCE);
            let priority: i32 = proxy.get("org.freedesktop.ReserveDevice1", "Priority").unwrap();
            assert_eq!(priority, PRIORITY);
            match reply {
                Reply::Release => {
                    connection.release_name(NAME).unwrap();
                    connection.send(message.method_return().append1(true)).unwrap();
                }
                Reply::Refuse => { connection.send(message.method_return().append1(false)).unwrap(); }
                Reply::Ignore => {}
                Reply::Error => {
                    let error = dbus::Error::new_failed("Reservation denied by test server");
                    connection.send(message.error(&error.name().unwrap().into(),
                        &std::ffi::CString::new(error.message().unwrap()).unwrap())).unwrap();
                }
            }
        }
        true
    }));
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = Arc::clone(&stop);
    let worker = std::thread::spawn(move || {
        while !worker_stop.load(Ordering::Relaxed) {
            connection.process(Duration::from_millis(10)).unwrap();
        }
    });
    Server { stop, worker: Some(worker) }
}

#[test]
fn an_unowned_card_is_published_and_disconnect_releases_it() {
    let bus = TestBus::new();
    let reservation = Reservation::on_bus(CARD, bus.connection()).unwrap();
    assert_eq!(bus.owner(NAME), Some(reservation.owner_name.clone()));
    let connection = bus.connection();
    let proxy = connection.with_proxy(NAME, PATH, PATIENCE);
    assert_eq!(proxy.get::<String>("org.freedesktop.ReserveDevice1", "ApplicationName").unwrap(), "Chilpa");
    let (released,): (bool,) = proxy.method_call("org.freedesktop.ReserveDevice1", "RequestRelease", (100i32,)).unwrap();
    assert!(!released, "a playing sink must not lose its reservation");
    drop(reservation);
    assert_eq!(bus.owner(NAME), None);
}

#[test]
fn repeated_handoffs_answer_priority_queries_without_losing_replies() {
    let bus = TestBus::new();
    for _ in 0..40 {
        let server = sound_server(&bus, Reply::Release);
        let started = std::time::Instant::now();
        let reservation = Reservation::on_bus(CARD, bus.connection()).unwrap();
        assert!(started.elapsed() < Duration::from_secs(1), "handoff stalled");
        assert_eq!(bus.owner(NAME), Some(reservation.owner_name.clone()));
        drop(reservation);
        assert_eq!(bus.owner(NAME), None);
        drop(server);
    }
}

#[test]
fn a_refusal_never_steals_the_card_or_leaves_a_queued_request() {
    let bus = TestBus::new();
    let server = sound_server(&bus, Reply::Refuse);
    let previous = bus.owner(NAME);
    assert!(Reservation::on_bus(CARD, bus.connection()).is_err());
    assert_eq!(bus.owner(NAME), previous);
    drop(server);
    assert_eq!(bus.owner(NAME), None);
    drop(Reservation::on_bus(CARD, bus.connection()).unwrap());
}

#[test]
fn an_error_reply_is_preserved_and_does_not_wait_for_timeout() {
    let bus = TestBus::new();
    let _server = sound_server(&bus, Reply::Error);
    let started = std::time::Instant::now();
    let error = Reservation::on_bus(CARD, bus.connection()).err().unwrap();
    assert!(error.contains("Reservation denied by test server"), "{error}");
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[test]
fn an_unresponsive_owner_times_out_without_stranding_the_next_attempt() {
    let bus = TestBus::new();
    let server = sound_server(&bus, Reply::Ignore);
    let previous = bus.owner(NAME);
    let started = std::time::Instant::now();
    assert!(Reservation::on_bus(CARD, bus.connection()).is_err());
    assert!(started.elapsed() < PATIENCE + Duration::from_secs(1));
    assert_eq!(bus.owner(NAME), previous);
    drop(server);
    assert_eq!(bus.owner(NAME), None);
    drop(Reservation::on_bus(CARD, bus.connection()).unwrap());
}
