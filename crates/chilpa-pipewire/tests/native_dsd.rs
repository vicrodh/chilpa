//! Isolated PipeWire graph tests. The only sink is an in-memory DSD fixture;
//! its ALSA properties simulate provenance and do not prove physical output.
#![cfg(target_os = "linux")]
use chilpa_audio::{
    backend::OutputBackend,
    format::{SampleFormat, StreamFormat},
    sink::SinkError,
};
use chilpa_pipewire::PipeWire;
use std::{
    ffi::{c_char, c_void, CString},
    fs,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

unsafe extern "C" {
    fn chilpa_pw_dsd_test_sink_new(name: *const c_char, rate: u32, interleave: i32) -> *mut c_void;
    fn chilpa_pw_dsd_test_sink_copy(handle: *mut c_void, bytes: *mut u8, size: usize) -> i64;
    fn chilpa_pw_dsd_test_sink_close(handle: *mut c_void);
}

struct Peer(*mut c_void);
impl Peer {
    fn new(name: &str, rate: u32, interleave: i32) -> Self {
        let name = CString::new(name).unwrap();
        let pointer = unsafe { chilpa_pw_dsd_test_sink_new(name.as_ptr(), rate, interleave) };
        assert!(!pointer.is_null(), "the isolated DSD fixture connects");
        Self(pointer)
    }
    fn copy(&self, size: usize) -> Vec<u8> {
        let mut bytes = vec![0; size];
        let count =
            unsafe { chilpa_pw_dsd_test_sink_copy(self.0, bytes.as_mut_ptr(), bytes.len()) };
        assert!(
            count >= 0,
            "the fixture has valid buffers within its budget"
        );
        bytes.truncate(count as usize);
        bytes
    }
}
impl Drop for Peer {
    fn drop(&mut self) {
        unsafe { chilpa_pw_dsd_test_sink_close(self.0) };
    }
}

struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

const SERVER: &str = r#"
context.properties = {
    core.name = chilpa-dsd-test
    core.daemon = true
    support.dbus = false
    default.clock.rate = 88200
    default.clock.allowed-rates = [ 88200 176400 352800 705600 ]
    default.clock.quantum = 256
}
context.spa-libs = { audio.convert.* = audioconvert/libspa-audioconvert support.* = support/libspa-support }
context.modules = [
    { name = libpipewire-module-protocol-native }
    { name = libpipewire-module-metadata }
    { name = libpipewire-module-spa-node-factory }
    { name = libpipewire-module-client-node }
    { name = libpipewire-module-adapter }
    { name = libpipewire-module-link-factory }
    { name = libpipewire-module-session-manager }
    { name = libpipewire-module-access args = { access.force = unrestricted } }
]
context.objects = [
    { factory = spa-node-factory args = {
        factory.name = support.node.driver node.name = DSD-Test-Driver priority.driver = 20000
    } }
]
"#;

#[test]
fn isolated_graph_transports_native_dsd() {
    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().join("config");
    fs::create_dir_all(config.join("wireplumber/wireplumber.conf.d")).unwrap();
    fs::write(directory.path().join("pipewire.conf"), SERVER).unwrap();
    fs::write(
        config.join("wireplumber/wireplumber.conf.d/isolated.conf"),
        r#"
wireplumber.profiles = { main = {
    monitor.alsa = disabled
    monitor.alsa-midi = disabled
    monitor.bluez = disabled
    monitor.bluez-midi = disabled
    monitor.v4l2 = disabled
    monitor.libcamera = disabled
} }
"#,
    )
    .unwrap();
    let output = Command::new("dbus-run-session")
        .args(["--"])
        .arg(std::env::current_exe().unwrap())
        .args(["--exact", "private_graph_worker", "--nocapture"])
        .env("CHILPA_DSD_TEST_WORKER", directory.path())
        .env("XDG_RUNTIME_DIR", directory.path())
        .env("PIPEWIRE_RUNTIME_DIR", directory.path())
        .env("XDG_CONFIG_HOME", &config)
        .env("XDG_STATE_HOME", directory.path().join("state"))
        .env("XDG_CACHE_HOME", directory.path().join("cache"))
        .env("PIPEWIRE_REMOTE", "chilpa-dsd-test")
        .env_remove("PIPEWIRE_CONFIG_DIR")
        .env_remove("PIPEWIRE_CONFIG_NAME")
        .env_remove("PIPEWIRE_NODE")
        .env_remove("PIPEWIRE_PROPS")
        .env_remove("PIPEWIRE_AUTOCONNECT")
        .env_remove("PIPEWIRE_RATE")
        .env_remove("PIPEWIRE_QUANTUM")
        .env_remove("PIPEWIRE_LATENCY")
        .output()
        .expect("dbus-run-session is installed for the isolated graph test");
    if !output.status.success() {
        let path = directory.keep();
        panic!(
            "isolated graph failed; logs in {}:\n{}\n{}",
            path.display(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

fn wait_caps(name: &str) -> chilpa_audio::negotiate::DeviceCaps {
    let start = Instant::now();
    loop {
        if let Some(caps) = PipeWire.untouched_caps(name) {
            return caps;
        }
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "the fixture advertises DSD: {name}"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn unresponsive_server_is_bounded() {
    let directory = tempfile::tempdir().unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "private_silent_worker", "--nocapture"])
        .env("CHILPA_DSD_SILENT_WORKER", directory.path())
        .env("PIPEWIRE_REMOTE", "silent-dsd-test")
        .env("XDG_RUNTIME_DIR", directory.path())
        .env("PIPEWIRE_RUNTIME_DIR", directory.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "unresponsive server test failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn private_silent_worker() {
    use std::{
        os::unix::net::UnixListener,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
    };
    let Some(directory) = std::env::var_os("CHILPA_DSD_SILENT_WORKER") else {
        return;
    };
    let listener =
        UnixListener::bind(std::path::PathBuf::from(directory).join("silent-dsd-test")).unwrap();
    listener.set_nonblocking(true).unwrap();
    let running = Arc::new(AtomicBool::new(true));
    let flag = running.clone();
    let peer = thread::spawn(move || {
        let mut connections = Vec::new();
        while flag.load(Ordering::Acquire) {
            if let Ok((stream, _)) = listener.accept() {
                assert!(connections.len() < 2);
                connections.push(stream);
            }
            thread::sleep(Duration::from_millis(5));
        }
    });
    let started = Instant::now();
    let caps = PipeWire.untouched_caps("missing");
    let query_elapsed = started.elapsed();
    let started = Instant::now();
    let opened = PipeWire.open(
        "missing",
        StreamFormat::new(SampleFormat::DsdU32Be, 88_200, 2),
        true,
    );
    let open_elapsed = started.elapsed();
    running.store(false, Ordering::Release);
    peer.join().unwrap();
    assert!(caps.is_none());
    assert!(opened.is_err());
    assert!(
        query_elapsed < Duration::from_secs(1),
        "query releases an unresponsive server"
    );
    assert!(
        open_elapsed < Duration::from_secs(1),
        "open releases an unresponsive server"
    );
}

#[test]
fn private_graph_worker() {
    let Some(directory) = std::env::var_os("CHILPA_DSD_TEST_WORKER") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    let log = fs::File::create(directory.join("pipewire.log")).unwrap();
    let _server = Process(
        Command::new("pipewire")
            .arg("-c")
            .arg(directory.join("pipewire.conf"))
            .stdout(Stdio::null())
            .stderr(log)
            .spawn()
            .unwrap(),
    );
    let start = Instant::now();
    while !directory.join("chilpa-dsd-test").exists() {
        assert!(
            start.elapsed() < Duration::from_secs(3),
            "private server starts"
        );
        thread::sleep(Duration::from_millis(10));
    }
    let log = fs::File::create(directory.join("wireplumber.log")).unwrap();
    let mut policy = Process(
        Command::new("wireplumber")
            .env("WIREPLUMBER_DEBUG", "2")
            .stdout(Stdio::null())
            .stderr(log)
            .spawn()
            .unwrap(),
    );
    thread::sleep(Duration::from_millis(500));
    assert!(
        policy.0.try_wait().unwrap().is_none(),
        "private policy remains running"
    );
    for index in 0..4 {
        for (format, interleave) in [(SampleFormat::DsdU32Be, 4), (SampleFormat::DsdU32Le, -4)] {
            let name = format!("dsd-fixture-{index}-{interleave}");
            let rate = 88_200 << index;
            let peer = Peer::new(&name, rate, interleave);
            let caps = wait_caps(&name);
            let requested = StreamFormat::new(format, rate, 2);
            assert!(caps.accepts(&requested));
            let started = Instant::now();
            let mut sink = PipeWire.open(&name, requested, true).unwrap();
            assert!(
                started.elapsed() < Duration::from_secs(1),
                "open is bounded"
            );
            assert_eq!(sink.format(), requested);
            let expected: Vec<u8> = (0..rate as usize * 8)
                .map(|i| (i.wrapping_mul(71) ^ (i / 8)).wrapping_add(31) as u8)
                .collect();
            let mut offset = 0;
            let mut paused = false;
            let started = Instant::now();
            while offset < expected.len() {
                match sink.write(&expected[offset..]) {
                    Ok(0) => thread::sleep(Duration::from_millis(2)),
                    Ok(frames) => offset += frames * 8,
                    Err(error) => panic!("payload write failed: {error}"),
                }
                if offset != 0 && !paused {
                    thread::sleep(Duration::from_millis(20));
                    sink.set_paused(true).unwrap();
                    let retained = sink.held();
                    thread::sleep(Duration::from_millis(20));
                    assert_eq!(
                        sink.held(),
                        retained,
                        "pause retains the queued frame count"
                    );
                    assert_eq!(sink.write(&expected[..8]).unwrap(), 0);
                    sink.set_paused(false).unwrap();
                    paused = true;
                }
                assert!(
                    started.elapsed() < Duration::from_secs(4),
                    "write completes"
                );
            }
            sink.drain(&|| false)
                .unwrap_or_else(|error| panic!("{name}: {error}; held={}", sink.held()));
            let captured = peer.copy(expected.len() + 65536);
            assert!(
                captured.len() >= expected.len(),
                "all payload frames reach the graph peer"
            );
            let mismatch = captured[..expected.len()]
                .iter()
                .zip(&expected)
                .position(|(actual, expected)| actual != expected);
            assert!(
                mismatch.is_none(),
                "native DSD payload differs at byte {mismatch:?}"
            );
            assert_eq!(sink.held(), 0);
            sink.flush().unwrap();
            assert_eq!(sink.write(&expected[..128]).unwrap(), 16);
            sink.set_paused(true).unwrap();
            assert_eq!(sink.write(&expected[..8]).unwrap(), 0);
            sink.set_paused(false).unwrap();
            sink.drain(&|| true).unwrap();
            assert_eq!(sink.held(), 0, "cancelled drain discards the pending ring");
            sink.suspend().unwrap();
            sink.wake().unwrap();
            assert_eq!(sink.write(&expected[..136]).unwrap(), 17);
            sink.drain(&|| false).unwrap();
            let tail = peer.copy(expected.len() + 65536);
            assert_eq!(
                &tail[expected.len()..],
                &expected[..136],
                "short tracks drain without prebuffering a quarter second"
            );
            sink.close().unwrap();
            assert_eq!(sink.write(&expected[..8]), Err(SinkError::Closed));
        }
    }
    let name = "dsd-removed-fixture";
    let peer = Peer::new(name, 88_200, 4);
    wait_caps(name);
    let mut sink = PipeWire
        .open(
            name,
            StreamFormat::new(SampleFormat::DsdU32Be, 88_200, 2),
            true,
        )
        .unwrap();
    drop(peer);
    let started = Instant::now();
    loop {
        if sink.write(&[0x69; 8]).is_err() {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "target removal closes payload admission"
        );
        thread::sleep(Duration::from_millis(5));
    }
    sink.suspend().unwrap();
    let _replacement = Peer::new(name, 88_200, 4);
    wait_caps(name);
    assert!(
        sink.wake().is_err(),
        "wake does not accept a replacement with the same name"
    );
    assert!(PipeWire.untouched_caps("missing-dsd-sink").is_none());
    for (name, rate, layout) in [("dsd-wrong-rate", 48_000, 4), ("dsd-u16", 88_200, 2)] {
        let _incompatible = Peer::new(name, rate, layout);
        thread::sleep(Duration::from_millis(50));
        assert!(PipeWire.untouched_caps(name).is_none());
        assert!(PipeWire
            .open(
                name,
                StreamFormat::new(SampleFormat::DsdU32Be, 88_200, 2),
                true
            )
            .is_err());
    }
    let pcm = PipeWire.caps("unchanged").unwrap();
    assert!(pcm.formats.iter().all(|format| !format.is_dsd()));
}
