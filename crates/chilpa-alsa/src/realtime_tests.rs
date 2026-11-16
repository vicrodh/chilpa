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
use std::time::Instant;
use chilpa_dirs::Sandbox;

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
    fn opener(&self) -> impl Fn() -> Result<Connection, dbus::Error> + Clone + Send + Sync + 'static {
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

/// RealtimeKit's limit is set on this process, which every test shares:
/// the tests that may set it, or that read it, take turns.
fn limits() -> std::sync::MutexGuard<'static, ()> {
    static LIMITS: Mutex<()> = Mutex::new(());
    LIMITS.lock().unwrap_or_else(|e| e.into_inner())
}

/// The system bus inside a Flatpak that was not given it: not there.
fn no_system_bus() -> Result<Connection, dbus::Error> {
    Err(dbus::Error::new_failed("no system bus in this sandbox"))
}

#[test]
fn the_portal_grants_real_time_when_the_system_bus_cannot() {
    let _limits = limits();
    // Inside a Flatpak the scheduler says no and no system bus reaches
    // RealtimeKit, so the thread fed the device at an ordinary priority,
    // behind whatever else the machine was doing. The portal is where a
    // sandbox asks, with its own process and thread ids.
    let bus = TestBus::new();
    let (asked, _portal) = grantor(&bus, Grantor::Portal, false);
    let got = ask(Sandbox::Flatpak, || None, no_system_bus, &Shared::new(bus.opener()));
    assert_eq!(got, Elevation::Granted(15), "real time, within the 15 the portal allows");
    assert_eq!(
        *asked.lock().unwrap(),
        [("MakeThreadRealtimeWithPID".to_string(), process_id(), thread_id(), 15)],
        "the process and the thread as this process sees them",
    );
}

#[test]
fn a_portal_that_refuses_real_time_still_puts_the_thread_ahead() {
    let _limits = limits();
    let bus = TestBus::new();
    let (asked, _portal) = grantor(&bus, Grantor::Portal, true);
    let got = ask(Sandbox::Flatpak, || None, no_system_bus, &Shared::new(bus.opener()));
    assert_eq!(got, Elevation::Nicer(NICE), "-11, within the -15 it allows");
    let methods: Vec<String> = asked.lock().unwrap().iter().map(|a| a.0.clone()).collect();
    assert_eq!(methods, ["MakeThreadRealtimeWithPID", "MakeThreadHighPriorityWithPID"]);
}

#[test]
fn where_the_system_bus_grants_the_session_bus_is_never_asked() {
    let _limits = limits();
    // Outside a sandbox nothing changes: RealtimeKit answers on the system
    // bus and the portal hears nothing.
    let (system, session) = (TestBus::new(), TestBus::new());
    let (kit_asked, _kit) = grantor(&system, Grantor::Kit, false);
    let (portal_asked, _portal) = grantor(&session, Grantor::Portal, false);
    let got = ask(Sandbox::Flatpak, || None, system.opener(), &Shared::new(session.opener()));
    assert_eq!(got, Elevation::Granted(15));
    assert_eq!(*kit_asked.lock().unwrap(), [("MakeThreadRealtime".to_string(), 0, thread_id(), 15)]);
    assert!(portal_asked.lock().unwrap().is_empty(), "the portal was asked: {:?}", portal_asked.lock().unwrap());
}

#[test]
fn a_nicer_level_from_the_system_bus_is_taken_before_the_portal_is_asked() {
    let _limits = limits();
    let (system, session) = (TestBus::new(), TestBus::new());
    let (_kit_asked, _kit) = grantor(&system, Grantor::Kit, true);
    let (portal_asked, _portal) = grantor(&session, Grantor::Portal, false);
    let got = ask(Sandbox::Flatpak, || None, system.opener(), &Shared::new(session.opener()));
    assert_eq!(got, Elevation::Nicer(NICE));
    assert!(portal_asked.lock().unwrap().is_empty());
}

#[test]
fn the_scheduler_comes_first_and_then_nobody_is_asked() {
    let bus = TestBus::new();
    let (asked, _portal) = grantor(&bus, Grantor::Portal, false);
    let system = || -> Result<Connection, dbus::Error> { panic!("the system bus was asked") };
    let got = ask(Sandbox::Flatpak, || Some(Elevation::Scheduled(20)), system, &Shared::new(bus.opener()));
    assert_eq!(got, Elevation::Scheduled(20));
    assert!(asked.lock().unwrap().is_empty());
}

#[test]
fn a_session_without_the_portal_leaves_the_thread_as_it_was_without_waiting() {
    let bus = TestBus::new();
    let portal = Shared::new(bus.opener());
    let started = std::time::Instant::now();
    assert_eq!(ask(Sandbox::Flatpak, || None, no_system_bus, &portal), Elevation::AsItWas);
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
    let _limits = limits();
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
        assert_eq!(ask(Sandbox::Flatpak, || None, no_system_bus, &portal), Elevation::Granted(15));
    }
    assert_eq!(opened.load(Ordering::Relaxed), 1, "one connection for three asks");
    assert_eq!(asked.lock().unwrap().len(), 3);
}

#[test]
fn a_portal_that_refuses_real_time_leaves_the_time_limit_as_it_was() {
    // RealtimeKit wants RLIMIT_RTTIME set before it grants anything, and it
    // was set on the process and kept when the portal said no.
    let _limits = limits();
    let before = libc::rlimit { rlim_cur: 50_000, rlim_max: 200_000 };
    // SAFETY: the struct is whole; the limit applies to this test process.
    assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_RTTIME, &before) }, 0, "a limit the test may set");
    let bus = TestBus::new();
    let (_asked, _portal) = grantor(&bus, Grantor::Portal, true);
    assert_eq!(ask(Sandbox::Flatpak, || None, no_system_bus, &Shared::new(bus.opener())), Elevation::Nicer(NICE));
    let mut now = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
    // SAFETY: the struct is whole, and getrlimit only writes it.
    assert_eq!(unsafe { libc::getrlimit(libc::RLIMIT_RTTIME, &mut now) }, 0);
    assert_eq!((now.rlim_cur, now.rlim_max), (50_000, 200_000), "the limit set for the refused ask stayed");
}

#[test]
fn outside_a_flatpak_the_portal_is_never_asked() {
    // A host without RealtimeKit and with a desktop portal: the thread that
    // opens the device asked the session bus, and waited on it, before it
    // opened anything. Before the portal it went on at an ordinary
    // priority, and that is what it does again.
    let _limits = limits();
    let bus = TestBus::new();
    let (asked, _portal) = grantor(&bus, Grantor::Portal, false);
    for sandbox in [Sandbox::None, Sandbox::Snap] {
        let opened = Arc::new(AtomicUsize::new(0));
        let (count, address) = (Arc::clone(&opened), bus.address.clone());
        let portal = Shared::new(move || {
            count.fetch_add(1, Ordering::Relaxed);
            Connection::new_address(&address)
        });
        assert_eq!(ask(sandbox, || None, no_system_bus, &portal), Elevation::AsItWas, "{sandbox:?}");
        assert_eq!(opened.load(Ordering::Relaxed), 0, "{sandbox:?}: the session bus was opened");
    }
    assert!(asked.lock().unwrap().is_empty(), "the portal was asked: {:?}", asked.lock().unwrap());
}

#[test]
fn a_session_bus_that_does_not_answer_holds_the_thread_no_longer_than_the_patience() {
    // Opening the session bus has no time limit of its own, and the thread
    // waiting on it is the one about to open the device.
    let portal = Shared::new(|| -> Result<Connection, dbus::Error> {
        std::thread::sleep(PATIENCE * 3);
        Err(dbus::Error::new_failed("a bus that answered too late"))
    });
    let started = Instant::now();
    assert_eq!(ask(Sandbox::Flatpak, || None, no_system_bus, &portal), Elevation::AsItWas);
    let waited = started.elapsed();
    assert!(waited < PATIENCE + Duration::from_millis(500), "it waited {waited:?} for the bus");
}

#[test]
fn a_thread_does_not_wait_behind_another_threads_questions() {
    // The lock was held while the portal was asked, so a second thread that
    // opened a device waited out the first one's questions, up to two
    // seconds each.
    let _limits = limits();
    let bus = TestBus::new();
    let (_asked, _portal) = grantor(&bus, Grantor::Portal, false);
    let portal = Shared::new(bus.opener());
    let (inside, entered) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        let first = &portal;
        scope.spawn(move || {
            first.with(|connection| {
                inside.send(()).unwrap();
                std::thread::sleep(PATIENCE * 2);
                granted_by_portal(connection)
            })
        });
        entered.recv().unwrap();
        let started = Instant::now();
        assert_eq!(portal.with(granted_by_portal), Some(Elevation::Granted(15)));
        let waited = started.elapsed();
        assert!(waited < PATIENCE, "it waited {waited:?} behind the other thread");
    });
}

/// Whatever this machine grants, asking twice on one thread asks once,
/// and the answer is the same answer; given back, it can be asked again.
#[test]
fn a_thread_asks_once_and_gives_back() {
    let _limits = limits();
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

/// A limit's soft and hard parts, which the libc struct does not compare.
fn parts(limit: libc::rlimit) -> (libc::rlim_t, libc::rlim_t) {
    (limit.rlim_cur, limit.rlim_max)
}

fn limit(soft: libc::rlim_t, hard: libc::rlim_t) -> libc::rlimit {
    libc::rlimit { rlim_cur: soft, rlim_max: hard }
}

const MS: libc::rlim_t = 1000;

#[test]
fn asking_through_the_portal_never_raises_the_time_limit() {
    let none = libc::RLIM_INFINITY;
    // From no limit at all, the most RealtimeKit allows: it reads the hard
    // limit and refuses one above that.
    assert_eq!(parts(rt_time_for(limit(none, none), 200 * MS)), (200 * MS, 200 * MS));
    // A limit already lower stays: raising it is not allowed, and
    // RealtimeKit takes it as it is.
    assert_eq!(parts(rt_time_for(limit(50 * MS, 100 * MS), 200 * MS)), (50 * MS, 100 * MS));
    assert_eq!(parts(rt_time_for(limit(none, 100 * MS), 200 * MS)), (100 * MS, 100 * MS));
}

#[test]
fn after_a_refusal_from_no_limit_the_soft_limit_comes_back_as_far_as_the_hard_one_lets_it() {
    let none = libc::RLIM_INFINITY;
    // The hard limit RealtimeKit required stays: without CAP_SYS_RESOURCE it
    // does not go back up, and the soft one cannot pass it.
    let now = limit(200 * MS, 200 * MS);
    assert_eq!(parts(rt_time_back(limit(none, none), now)), (200 * MS, 200 * MS));
    assert_eq!(parts(rt_time_back(limit(50 * MS, none), now)), (50 * MS, 200 * MS));
}

