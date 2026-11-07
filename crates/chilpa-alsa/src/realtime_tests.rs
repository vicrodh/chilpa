//! The order real time is asked in, against grantors on private buses of
//! the test's own. Those grantors only write down what they were asked, so
//! no test here changes a thread's priority through them; the limit
//! RealtimeKit wants set first is set on this process, at RealtimeKit's own
//! default of 200 ms.

use super::*;
use dbus::arg::Variant;
use dbus::channel::{MatchingReceiver, Sender};
use dbus::message::MatchRule;
use dbus::Message;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

struct TestBus {
    child: Child,
    address: String,
    folder: std::path::PathBuf,
}

/// A bus that starts nothing on its own. The session bus's own
/// configuration lists the desktop's service files, so a question to a
/// portal nobody owns there started the desktop's real portal, and through
/// it the real RealtimeKit. Its socket lives in the bus's own folder, which
/// goes when the bus does.
const CONFIG: &str = r#"<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-BUS Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:dir=FOLDER</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
"#;

impl TestBus {
    fn new() -> Self {
        static COUNT: AtomicUsize = AtomicUsize::new(0);
        let folder = std::env::temp_dir()
            .join(format!("chilpa-rt-bus-{}-{}", std::process::id(), COUNT.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir_all(&folder).unwrap();
        let config = folder.join("bus.conf");
        std::fs::write(&config, CONFIG.replace("FOLDER", &folder.display().to_string())).unwrap();
        let mut child = Command::new("dbus-daemon")
            .arg(format!("--config-file={}", config.display()))
            .args(["--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .spawn()
            .expect("private bus");
        let mut address = String::new();
        BufReader::new(child.stdout.take().unwrap()).read_line(&mut address).unwrap();
        Self { child, address: address.trim().into(), folder }
    }

    fn connection(&self) -> Connection {
        Connection::new_address(&self.address).unwrap()
    }

    /// Opens a connection to this bus each time it is called, as
    /// `Connection::new_system` and `new_session` do to theirs.
    fn opener(&self) -> impl Fn() -> Result<Connection, dbus::Error> {
        let address = self.address.clone();
        move || Connection::new_address(&address)
    }
}

impl Drop for TestBus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.folder);
    }
}

/// What a grantor was asked: the method, the process (0 when the method
/// takes none), the thread and the priority or nice level.
type Asked = Arc<Mutex<Vec<(String, u64, u64, i64)>>>;

#[derive(Clone, Copy)]
enum Grantor {
    Kit,
    Portal,
}

impl Grantor {
    fn name(self) -> &'static str {
        match self {
            Grantor::Kit => KIT,
            Grantor::Portal => "org.freedesktop.portal.Desktop",
        }
    }

    fn interface(self) -> &'static str {
        match self {
            Grantor::Kit => KIT,
            Grantor::Portal => REALTIME,
        }
    }
}

/// A grantor's connection, answered on a thread until it is dropped.
struct Serving {
    stop: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Serving {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn failure(message: &Message, name: &str, text: &str) -> Message {
    message.error(&name.into(), &std::ffi::CString::new(text).unwrap())
}

/// A grantor on `bus` that allows priority 15, 200 ms without blocking and
/// a nice level of -15, refuses real time when `refuse_real_time`, and
/// writes down every grant it is asked for.
fn grantor(bus: &TestBus, who: Grantor, refuse_real_time: bool) -> (Asked, Serving) {
    let connection = bus.connection();
    connection.request_name(who.name(), false, false, true).unwrap();
    let asked: Asked = Arc::default();
    let log = Arc::clone(&asked);
    connection.start_receive(
        MatchRule::new_method_call(),
        Box::new(move |message, connection| {
            let member = message.member().map(|m| m.to_string()).unwrap_or_default();
            let reply = match member.as_str() {
                "Get" => {
                    let (interface, property): (String, String) = message.read2().unwrap();
                    match (interface == who.interface(), property.as_str()) {
                        (true, "MaxRealtimePriority") => message.method_return().append1(Variant(15i32)),
                        (true, "RTTimeUSecMax") => message.method_return().append1(Variant(200_000i64)),
                        (true, "MinNiceLevel") => message.method_return().append1(Variant(-15i32)),
                        _ => failure(&message, "org.freedesktop.DBus.Error.UnknownProperty", "no such property"),
                    }
                }
                "MakeThreadRealtime" | "MakeThreadHighPriority" | "MakeThreadRealtimeWithPID"
                | "MakeThreadHighPriorityWithPID" => {
                    let mut items = message.iter_init();
                    let process: u64 = if member.ends_with("WithPID") { items.read().unwrap() } else { 0 };
                    let thread: u64 = items.read().unwrap();
                    let real_time = member.starts_with("MakeThreadRealtime");
                    let priority: i64 =
                        if real_time { items.read::<u32>().unwrap().into() } else { items.read::<i32>().unwrap().into() };
                    log.lock().unwrap().push((member.clone(), process, thread, priority));
                    if real_time && refuse_real_time {
                        failure(&message, "org.freedesktop.DBus.Error.AccessDenied", "real time refused by the test")
                    } else {
                        message.method_return()
                    }
                }
                _ => failure(&message, "org.freedesktop.DBus.Error.UnknownMethod", "no such method"),
            };
            connection.send(reply).unwrap();
            true
        }),
    );
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = Arc::clone(&stop);
    let worker = std::thread::spawn(move || {
        while !worker_stop.load(Ordering::Relaxed) {
            connection.process(Duration::from_millis(10)).unwrap();
        }
    });
    (asked, Serving { stop, worker: Some(worker) })
}

/// The system bus inside a Flatpak that was not given it: not there.
fn no_system_bus() -> Result<Connection, dbus::Error> {
    Err(dbus::Error::new_failed("no system bus in this sandbox"))
}

#[test]
fn the_portal_grants_real_time_when_the_system_bus_cannot() {
    // Inside a Flatpak the scheduler says no and no system bus reaches
    // RealtimeKit, so the thread fed the device at an ordinary priority,
    // behind whatever else the machine was doing. The portal is where a
    // sandbox asks, with its own process and thread ids.
    let bus = TestBus::new();
    let (asked, _portal) = grantor(&bus, Grantor::Portal, false);
    let got = ask(|| None, no_system_bus, &Shared::new(bus.opener()));
    assert_eq!(got, Elevation::Granted(15), "real time, within the 15 the portal allows");
    assert_eq!(
        *asked.lock().unwrap(),
        [("MakeThreadRealtimeWithPID".to_string(), process_id(), thread_id(), 15)],
        "the process and the thread as this process sees them",
    );
}

#[test]
fn a_portal_that_refuses_real_time_still_puts_the_thread_ahead() {
    let bus = TestBus::new();
    let (asked, _portal) = grantor(&bus, Grantor::Portal, true);
    let got = ask(|| None, no_system_bus, &Shared::new(bus.opener()));
    assert_eq!(got, Elevation::Nicer(NICE), "-11, within the -15 it allows");
    let methods: Vec<String> = asked.lock().unwrap().iter().map(|a| a.0.clone()).collect();
    assert_eq!(methods, ["MakeThreadRealtimeWithPID", "MakeThreadHighPriorityWithPID"]);
}

#[test]
fn where_the_system_bus_grants_the_session_bus_is_never_asked() {
    // Outside a sandbox nothing changes: RealtimeKit answers on the system
    // bus and the portal hears nothing.
    let (system, session) = (TestBus::new(), TestBus::new());
    let (kit_asked, _kit) = grantor(&system, Grantor::Kit, false);
    let (portal_asked, _portal) = grantor(&session, Grantor::Portal, false);
    let got = ask(|| None, system.opener(), &Shared::new(session.opener()));
    assert_eq!(got, Elevation::Granted(15));
    assert_eq!(*kit_asked.lock().unwrap(), [("MakeThreadRealtime".to_string(), 0, thread_id(), 15)]);
    assert!(portal_asked.lock().unwrap().is_empty(), "the portal was asked: {:?}", portal_asked.lock().unwrap());
}

#[test]
fn a_nicer_level_from_the_system_bus_is_taken_before_the_portal_is_asked() {
    let (system, session) = (TestBus::new(), TestBus::new());
    let (_kit_asked, _kit) = grantor(&system, Grantor::Kit, true);
    let (portal_asked, _portal) = grantor(&session, Grantor::Portal, false);
    let got = ask(|| None, system.opener(), &Shared::new(session.opener()));
    assert_eq!(got, Elevation::Nicer(NICE));
    assert!(portal_asked.lock().unwrap().is_empty());
}

#[test]
fn the_scheduler_comes_first_and_then_nobody_is_asked() {
    let bus = TestBus::new();
    let (asked, _portal) = grantor(&bus, Grantor::Portal, false);
    let system = || -> Result<Connection, dbus::Error> { panic!("the system bus was asked") };
    let got = ask(|| Some(Elevation::Scheduled(20)), system, &Shared::new(bus.opener()));
    assert_eq!(got, Elevation::Scheduled(20));
    assert!(asked.lock().unwrap().is_empty());
}

#[test]
fn a_session_without_the_portal_leaves_the_thread_as_it_was_without_waiting() {
    let bus = TestBus::new();
    let portal = Shared::new(bus.opener());
    let started = std::time::Instant::now();
    assert_eq!(ask(|| None, no_system_bus, &portal), Elevation::AsItWas);
    assert!(started.elapsed() < PATIENCE, "nobody owns the name, and the bus says so at once");
    assert!(portal.held.lock().unwrap().is_none(), "a connection that got nothing is let go");
    let connection = bus.connection();
    let owner: Result<(String,), dbus::Error> = connection
        .with_proxy("org.freedesktop.DBus", "/org/freedesktop/DBus", PATIENCE)
        .method_call("org.freedesktop.DBus", "GetNameOwner", ("org.freedesktop.portal.Desktop",));
    assert!(owner.is_err(), "the bus started a portal of its own: {owner:?}");
}

#[test]
fn the_portal_connection_is_opened_once_and_kept_while_it_grants() {
    let bus = TestBus::new();
    let (asked, _portal) = grantor(&bus, Grantor::Portal, false);
    let opened = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&opened);
    let address = bus.address.clone();
    let portal = Shared::new(move || {
        count.fetch_add(1, Ordering::Relaxed);
        Connection::new_address(&address)
    });
    for _ in 0..3 {
        assert_eq!(ask(|| None, no_system_bus, &portal), Elevation::Granted(15));
    }
    assert_eq!(opened.load(Ordering::Relaxed), 1, "one connection for three asks");
    assert_eq!(asked.lock().unwrap().len(), 3);
}

/// Whatever this machine grants, asking twice on one thread asks once,
/// and the answer is the same answer; given back, it can be asked again.
#[test]
fn a_thread_asks_once_and_gives_back() {
    let first = elevate();
    let second = elevate();
    assert_eq!(first, second);
    relax();
    assert_eq!(HELD.get(), None);
    relax();
    assert_eq!(elevate(), first);
    relax();
}

#[test]
fn the_id_is_the_threads_own() {
    let here = thread_id();
    let there = std::thread::spawn(thread_id).join().unwrap();
    assert_ne!(here, 0);
    assert_ne!(here, there);
}
