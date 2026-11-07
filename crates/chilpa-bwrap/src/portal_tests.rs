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
    let exits = Exits::default();
    exits.exited(7, 0);
    exits.started(7);
    assert!(!exits.running(7), "the exit that came first is not lost");
    exits.started(8);
    assert!(exits.running(8));
    for pid in 100..100 + EARLY_EXITS as u32 + 5 { exits.exited(pid, 0); }
    assert_eq!(exits.state().early.len(), EARLY_EXITS);
    exits.forget(8);
    assert!(!exits.state().tracked.contains_key(&8));
    exits.lose();
    exits.started(9);
    assert!(!exits.running(9), "after the bus is lost nothing runs");
}
