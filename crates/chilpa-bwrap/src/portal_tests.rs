use super::*;
use crate::fake_portal::{FakePortal, PrivateBus};
use std::fs::File;
use std::io::{BufRead, BufReader, PipeReader, PipeWriter, Write};
use std::os::fd::AsFd;
use std::time::Instant;

const RUNNER: &str = "/app/libexec/chilpa-plugin-runner";

/// Spawns `script` behind the runner's options, as the backend would, and
/// keeps its input and output.
fn spawn(portal: &Arc<Portal>, cwd: &Path, script: &str, network: bool, writable: &[&File], readable: &[&File])
    -> (Arc<PortalProcess>, PipeWriter, BufReader<PipeReader>) {
    let (child_input, input) = std::io::pipe().unwrap();
    let (output, child_output) = std::io::pipe().unwrap();
    let errors = std::fs::OpenOptions::new().write(true).open("/dev/null").unwrap();
    let request = Spawn {
        cwd,
        argv: [RUNNER, "--nofile=256", "--", "/bin/sh", "-c", script].map(OsStr::new).to_vec(),
        env: vec![("MARK", "set".into())],
        stdio: [child_input.as_fd(), child_output.as_fd(), errors.as_fd()],
        network,
        writable: writable.iter().map(|file| file.as_fd()).collect(),
        readable: readable.iter().map(|file| file.as_fd()).collect(),
    };
    let process = portal.spawn(&request).unwrap();
    (process, input, BufReader::new(output))
}

fn ended_within(process: &PortalProcess, bound: Duration) -> bool {
    let start = Instant::now();
    process.wait();
    !process.alive() && start.elapsed() < bound
}

#[test]
fn a_spawn_asks_for_a_sandbox_of_its_own_and_hands_over_three_descriptors() {
    let bus = PrivateBus::start();
    let fake = FakePortal::serve(&bus, 8);
    let portal = Portal::connect(&bus.bus()).unwrap();
    let root = tempfile::tempdir().unwrap();
    let root_path = root.path().canonicalize().unwrap();
    for name in ["package", "data"] { std::fs::create_dir(root_path.join(name)).unwrap(); }
    let (package, data) = (File::open(root_path.join("package")).unwrap(), File::open(root_path.join("data")).unwrap());
    let (process, mut input, mut output) = spawn(&portal, &root_path, "read line; printf '%s %s %s\\n' \"$line\" \"$MARK\" \"$(pwd)\"",
        false, &[&data], &[&package]);
    writeln!(input, "hello").unwrap();
    let mut line = String::new();
    output.read_line(&mut line).unwrap();
    assert_eq!(line.trim_end(), format!("hello set {}", root_path.display()), "its standard input and output are the pipes handed over");
    assert!(ended_within(&process, Duration::from_secs(5)), "the shell exited after one line");

    let spawned = fake.spawned();
    assert_eq!(spawned.len(), 1);
    let spawned = &spawned[0];
    assert_eq!(spawned.flags, CLEAR_ENV | SANDBOX | WATCH_BUS | NO_NETWORK);
    assert_eq!(spawned.flags & (32 | 128), 0, "no pids are exposed or shared");
    assert_eq!(spawned.sandbox_flags, Some(0), "no display, sound, GPU, bus, devices or shared memory");
    assert_eq!(spawned.fds, vec![0, 1, 2]);
    assert_eq!(spawned.env, HashMap::from([("MARK".to_string(), "set".to_string())]));
    assert_eq!(spawned.argv[0], format!("{RUNNER}\0").into_bytes());
    assert!(spawned.argv.iter().chain([&spawned.cwd]).all(|bytes| bytes.ends_with(&[0]) && !bytes[..bytes.len() - 1].contains(&0)),
        "each byte string ends with one NUL");
    assert_eq!(spawned.cwd, format!("{}\0", root_path.display()).into_bytes());
    let paths = |exposed: &[crate::fake_portal::Exposed]| exposed.iter().map(|e| e.path.clone()).collect::<Vec<_>>();
    assert_eq!(paths(&spawned.writable), vec![root_path.join("data")]);
    assert_eq!(paths(&spawned.readable), vec![root_path.join("package")]);
    assert!(fake.signals().is_empty(), "a process that ended on its own is not signalled");
}

#[test]
fn an_exit_wakes_whoever_waits_and_a_second_end_sends_nothing() {
    let bus = PrivateBus::start();
    let fake = FakePortal::serve(&bus, 8);
    let portal = Portal::connect(&bus.bus()).unwrap();
    let root = tempfile::tempdir().unwrap();
    let (process, _input, _output) = spawn(&portal, root.path(), "exec sleep 30", true, &[], &[]);
    assert_eq!(fake.spawned()[0].flags, CLEAR_ENV | SANDBOX | WATCH_BUS, "given the network, it keeps it");
    assert!(process.alive());
    let waiting = { let process = process.clone(); std::thread::spawn(move || ended_within(&process, Duration::from_secs(5))) };
    std::thread::sleep(Duration::from_millis(100));
    process.terminate();
    assert!(waiting.join().unwrap(), "SpawnExited woke the waiter");
    process.terminate();
    process.wait();
    assert_eq!(fake.signals(), vec![(fake.spawned()[0].pid, libc::SIGKILL as u32, true)], "one SIGKILL, to the process group");
}

#[test]
fn losing_the_bus_ends_every_process_at_once() {
    let mut bus = PrivateBus::start();
    let _fake = FakePortal::serve(&bus, 8);
    let portal = Portal::connect(&bus.bus()).unwrap();
    let root = tempfile::tempdir().unwrap();
    let (first, _a, _b) = spawn(&portal, root.path(), "exec sleep 30", false, &[], &[]);
    let (second, _c, _d) = spawn(&portal, root.path(), "exec sleep 30", false, &[], &[]);
    let waiting = { let first = first.clone(); std::thread::spawn(move || ended_within(&first, Duration::from_secs(3))) };
    std::thread::sleep(Duration::from_millis(100));
    bus.end();
    assert!(waiting.join().unwrap(), "the waiter woke when the bus went, well before its own bound");
    assert!(!second.alive());
    assert!(portal.lost());
}

#[test]
fn a_portal_older_than_version_3_or_none_at_all_is_refused() {
    let bus = PrivateBus::start();
    assert!(Portal::connect(&bus.bus()).is_err(), "nobody owns the portal's name");
    let _fake = FakePortal::serve(&bus, 2);
    let refused = Portal::connect(&bus.bus()).err().unwrap_or_default();
    assert!(refused.contains("version 2"), "{refused}");
}

#[test]
fn an_exit_read_before_its_spawn_answer_is_kept_and_the_early_ones_are_bounded() {
    let ledger = Ledger::default();
    ledger.exited(7, 0);
    ledger.state().started(7, false);
    assert!(!ledger.running(7), "the exit that came first is not lost");
    ledger.state().started(8, false);
    assert!(ledger.running(8));
    for pid in 100..100 + EARLY_EXITS as u32 + 5 { ledger.exited(pid, 0); }
    assert_eq!(ledger.state().early.len(), EARLY_EXITS);
    assert!(ledger.release(8), "a process whose handle goes while it runs stays, held by nothing");
    assert!(ledger.running(8));
    ledger.exited(8, 0);
    assert!(!ledger.state().tracked.contains_key(&8), "its exit took it off");
    ledger.lose();
    ledger.state().started(9, false);
    assert!(!ledger.running(9), "after the bus is lost nothing runs");
}

/// Short waits, so a test that waits them out stays quick.
fn quick() -> Timeouts {
    Timeouts { spawn: Duration::from_secs(2), signal: Duration::from_secs(1), exit: Duration::from_secs(3) }
}

/// True once `condition` holds, checked until `bound` passes.
fn eventually(bound: Duration, condition: impl Fn() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < bound {
        if condition() { return true; }
        std::thread::sleep(Duration::from_millis(20));
    }
    condition()
}

#[test]
fn a_signal_the_portal_refused_is_sent_again_until_it_takes() {
    let bus = PrivateBus::start();
    let fake = FakePortal::serve(&bus, 8);
    let portal = Portal::connect_with(&bus.bus(), quick()).unwrap();
    let root = tempfile::tempdir().unwrap();
    let (process, _input, _output) = spawn(&portal, root.path(), "exec sleep 30", false, &[], &[]);
    fake.refuse_signals(2);
    process.terminate();
    process.wait();
    assert!(!process.alive(), "the signal was sent again while it was waited for");
    let pid = fake.spawned()[0].pid;
    assert_eq!(fake.signals(), vec![(pid, libc::SIGKILL as u32, true); 3], "two refused, then one taken");
}

#[test]
fn a_spawn_answered_after_its_caller_gave_up_is_ended_when_the_answer_comes() {
    let bus = PrivateBus::start();
    let fake = FakePortal::serve(&bus, 8);
    let portal = Portal::connect_with(&bus.bus(), Timeouts { spawn: Duration::from_millis(300), ..quick() }).unwrap();
    fake.delay_answers(Duration::from_millis(1500));
    let root = tempfile::tempdir().unwrap();
    let (child_input, _input) = std::io::pipe().unwrap();
    let (_output, child_output) = std::io::pipe().unwrap();
    let errors = std::fs::OpenOptions::new().write(true).open("/dev/null").unwrap();
    let request = Spawn {
        cwd: root.path(),
        argv: [RUNNER, "--", "/bin/sh", "-c", "exec sleep 30"].map(OsStr::new).to_vec(),
        env: Vec::new(),
        stdio: [child_input.as_fd(), child_output.as_fd(), errors.as_fd()],
        network: false,
        writable: Vec::new(),
        readable: Vec::new(),
    };
    assert!(portal.spawn(&request).is_err(), "its answer came after the caller stopped waiting");
    assert_eq!(fake.spawned().len(), 1, "the portal started it all the same");
    let pid = fake.spawned()[0].pid;
    assert!(eventually(Duration::from_secs(5), || fake.signals().contains(&(pid, libc::SIGKILL as u32, true))
        && !fake.running().contains(&pid)), "the late answer named it, and it was ended: {:?}", fake.signals());
    assert!(eventually(Duration::from_secs(2), || portal.tracked().is_empty()), "its exit took it off the ledger");
}

#[test]
fn a_handle_dropped_while_its_process_runs_keeps_its_pid_and_ends_it() {
    let bus = PrivateBus::start();
    let fake = FakePortal::serve(&bus, 8);
    let portal = Portal::connect_with(&bus.bus(), Timeouts { exit: Duration::from_millis(800), ..quick() }).unwrap();
    let root = tempfile::tempdir().unwrap();
    let (process, _input, _output) = spawn(&portal, root.path(), "exec sleep 30", false, &[], &[]);
    let pid = fake.spawned()[0].pid;
    fake.refuse_signals(u32::MAX);
    process.terminate();
    process.wait();
    assert!(process.alive(), "every signal so far was refused");
    fake.refuse_signals(0);
    drop(process);
    assert!(eventually(Duration::from_secs(5), || !fake.running().contains(&pid)), "what nothing holds was ended");
    assert!(eventually(Duration::from_secs(2), || portal.tracked().is_empty()), "its exit took it off the ledger");
}
