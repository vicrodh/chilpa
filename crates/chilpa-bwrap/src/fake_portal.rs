//! A stand-in for the Flatpak portal on a private bus, for the tests that
//! run without Flatpak. It records what each `Spawn` asked for and runs the
//! command after the runner's `--` with the descriptors it was handed, each
//! at its number, then reports its exit to the caller, as the portal does.
//! Of the runner, it does what its starter sees: the ready line on the
//! descriptor the command line names, once the command runs, and that
//! descriptor kept from the command. It starts no sandbox: what it checks is
//! the request, not the confinement.

use crate::portal::{Bus, INTERFACE, NAME, PATH};
use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::io::{BufRead, BufReader};
use std::os::fd::{AsFd, AsRawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use zbus::blocking::{connection::Builder, Connection, MessageIterator};
use zbus::message::{Message, Type};
use zbus::zvariant::{Fd, Value};

/// A session bus of its own, from the system's dbus-daemon, with no service
/// files: a name nobody owns is not started on demand.
pub(crate) struct PrivateBus {
    daemon: Child,
    address: String,
    _dir: tempfile::TempDir,
}

impl PrivateBus {
    pub(crate) fn start() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("bus.conf");
        std::fs::write(&config, format!(r#"<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:path={}</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
"#, dir.path().join("bus").display())).unwrap();
        let mut daemon = Command::new("/usr/bin/dbus-daemon").arg("--config-file").arg(&config)
            .args(["--nofork", "--nopidfile", "--print-address"])
            .stdout(Stdio::piped()).stderr(Stdio::null()).spawn().expect("these tests start a private dbus-daemon");
        let mut address = String::new();
        BufReader::new(daemon.stdout.take().unwrap()).read_line(&mut address).unwrap();
        Self { daemon, address: address.trim().to_string(), _dir: dir }
    }

    pub(crate) fn bus(&self) -> Bus {
        Bus::Address(self.address.clone())
    }

    /// Ends the bus, as a session that crashed would.
    pub(crate) fn end(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        self.end();
    }
}

/// A folder handed over as a descriptor, as the portal received it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Exposed {
    pub(crate) path: PathBuf,
    /// Opened with `O_PATH`, as the portal requires.
    pub(crate) o_path: bool,
}

/// What one `Spawn` asked for.
#[derive(Clone, Debug)]
pub(crate) struct Spawned {
    pub(crate) cwd: Vec<u8>,
    pub(crate) argv: Vec<Vec<u8>>,
    /// The descriptor numbers it named, in order.
    pub(crate) fds: Vec<u32>,
    pub(crate) env: HashMap<String, String>,
    pub(crate) flags: u32,
    pub(crate) sandbox_flags: Option<u32>,
    pub(crate) writable: Vec<Exposed>,
    pub(crate) readable: Vec<Exposed>,
    pub(crate) pid: u32,
}

#[derive(Default)]
struct Record {
    spawned: Vec<Spawned>,
    signals: Vec<(u32, u32, bool)>,
    /// What it started and has not seen end.
    running: Vec<u32>,
    /// How many of the next signals it refuses.
    refused: u32,
    /// How long it holds each `Spawn` answer back, after starting the process.
    delay: std::time::Duration,
    /// Starts nothing behind each spawn, as a runner that refused would.
    refusing: bool,
}

pub(crate) struct FakePortal {
    record: Arc<Mutex<Record>>,
    _conn: Connection,
}

impl FakePortal {
    /// Owns the portal's name on `bus` and answers as `version` of it.
    pub(crate) fn serve(bus: &PrivateBus, version: u32) -> Self {
        let conn = Builder::address(bus.address.as_str()).unwrap().build().unwrap();
        // Read from before the name is taken, so no call is missed.
        let messages = MessageIterator::from(&conn);
        conn.request_name(NAME).unwrap();
        let record = Arc::<Mutex<Record>>::default();
        let (server, kept) = (conn.clone(), record.clone());
        std::thread::spawn(move || {
            for message in messages {
                let Ok(message) = message else { break };
                answer(&server, &message, version, &kept);
            }
        });
        Self { record, _conn: conn }
    }

    pub(crate) fn spawned(&self) -> Vec<Spawned> {
        self.record.lock().unwrap().spawned.clone()
    }

    /// Each `SpawnSignal`, refused or taken: pid, signal and whether it went
    /// to the group.
    pub(crate) fn signals(&self) -> Vec<(u32, u32, bool)> {
        self.record.lock().unwrap().signals.clone()
    }

    /// What it started and has not seen end.
    pub(crate) fn running(&self) -> Vec<u32> {
        self.record.lock().unwrap().running.clone()
    }

    /// Answers the next `count` signals with an error, without sending them.
    pub(crate) fn refuse_signals(&self, count: u32) {
        self.record.lock().unwrap().refused = count;
    }

    /// Holds each `Spawn` answer back for `delay` after starting the process,
    /// as a portal too slow for its caller would.
    pub(crate) fn delay_answers(&self, delay: std::time::Duration) {
        self.record.lock().unwrap().delay = delay;
    }

    /// Runs a process that exits at once without its ready line in place of
    /// each command, as a runner that refuses to start its plugin does.
    pub(crate) fn refuse_starts(&self) {
        self.record.lock().unwrap().refusing = true;
    }
}

impl Drop for FakePortal {
    /// What it started and is still running goes with it.
    fn drop(&mut self) {
        for pid in &self.record.lock().unwrap().running {
            unsafe { libc::killpg(*pid as libc::pid_t, libc::SIGKILL); }
        }
    }
}

fn answer(conn: &Connection, message: &Message, version: u32, record: &Arc<Mutex<Record>>) {
    if message.message_type() != Type::MethodCall { return; }
    let header = message.header();
    let _ = match header.member().map(|member| member.as_str()) {
        Some("Get") => conn.reply(&header, &Value::U32(version)),
        Some("Spawn") => match spawn(conn, message, record) {
            Ok(pid) => {
                let delay = record.lock().unwrap().delay;
                std::thread::sleep(delay);
                conn.reply(&header, &pid)
            }
            Err(problem) => conn.reply_dbus_error(&header, zbus::fdo::Error::Failed(problem)),
        },
        Some("SpawnSignal") => {
            let (pid, signal, group): (u32, u32, bool) = message.body().deserialize().unwrap();
            let mut record = record.lock().unwrap();
            record.signals.push((pid, signal, group));
            if record.refused > 0 {
                record.refused -= 1;
                return drop(conn.reply_dbus_error(&header, zbus::fdo::Error::Failed("refused".into())));
            }
            // Only what still runs: a reaped pid can be another process's.
            if record.running.contains(&pid) {
                let (pid, signal) = (pid as libc::pid_t, signal as libc::c_int);
                unsafe { if group { libc::killpg(pid, signal); } else { libc::kill(pid, signal); } }
            }
            conn.reply(&header, &())
        }
        _ => Ok(()),
    };
}

type Request<'m> = (Vec<u8>, Vec<Vec<u8>>, HashMap<u32, Fd<'m>>, HashMap<String, String>, u32, HashMap<String, Value<'m>>);

fn spawn(conn: &Connection, message: &Message, record: &Arc<Mutex<Record>>) -> Result<u32, String> {
    let body = message.body();
    let (cwd, argv, fds, env, flags, options): Request<'_> = body.deserialize().map_err(|error| error.to_string())?;
    let text = |bytes: &[u8]| -> OsString { OsStr::from_bytes(bytes.strip_suffix(&[0]).unwrap_or(bytes)).to_os_string() };
    let start = argv.iter().position(|arg| arg.as_slice() == b"--\0").ok_or("no -- in argv")? + 1;
    let program = argv.get(start).ok_or("no program after --")?;
    let stdio = |number: u32| -> Result<Stdio, String> {
        let fd = fds.get(&number).ok_or(format!("no descriptor {number}"))?;
        fd.as_fd().try_clone_to_owned().map(Stdio::from).map_err(|error| error.to_string())
    };
    let refusing = record.lock().unwrap().refusing;
    let mut command = if refusing { Command::new("/bin/true") } else { Command::new(text(program)) };
    if !refusing { command.args(argv[start + 1..].iter().map(|arg| text(arg))); }
    command.env_clear().envs(&env).current_dir(text(&cwd)).process_group(0).stdin(stdio(0)?).stdout(stdio(1)?).stderr(stdio(2)?);
    // The runner's ready descriptor, which it keeps from the plugin.
    let ready: Option<u32> = argv[..start].iter().find_map(|arg| {
        text(arg).to_str()?.strip_prefix("--ready=")?.parse().ok()
    });
    // Each other descriptor above the standard three at its own number, as
    // the portal's map puts it.
    let further: Vec<(libc::c_int, libc::c_int)> = fds.iter().filter(|(number, _)| **number >= 3 && Some(**number) != ready)
        .map(|(number, fd)| (*number as libc::c_int, fd.as_raw_fd())).collect();
    unsafe {
        command.pre_exec(move || {
            for (number, fd) in &further {
                if (*fd != *number && libc::dup2(*fd, *number) < 0) || libc::fcntl(*number, libc::F_SETFD, 0) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
            }
            Ok(())
        });
    }
    let mut child = command.spawn().map_err(|error| error.to_string())?;
    // Once the program runs, as the runner says it.
    if let Some(number) = ready.filter(|_| !refusing) {
        let started = fds.get(&number).ok_or(format!("no descriptor {number}"))?;
        std::io::Write::write_all(&mut std::fs::File::from(started.as_fd().try_clone_to_owned().map_err(|error| error.to_string())?), b"\n")
            .map_err(|error| error.to_string())?;
    }
    let pid = child.id();
    let exposed = |key: &str| -> Vec<Exposed> {
        match options.get(key) {
            Some(Value::Array(fds)) => fds.iter().filter_map(|fd| match fd { Value::Fd(fd) => Some(describe(fd)), _ => None }).collect(),
            _ => Vec::new(),
        }
    };
    let mut numbers: Vec<u32> = fds.keys().copied().collect();
    numbers.sort_unstable();
    let sandbox_flags = match options.get("sandbox-flags") { Some(Value::U32(flags)) => Some(*flags), _ => None };
    {
        let mut record = record.lock().unwrap();
        record.spawned.push(Spawned { cwd: cwd.clone(), argv: argv.clone(), fds: numbers, env: env.clone(), flags, sandbox_flags,
            writable: exposed("sandbox-expose-fd"), readable: exposed("sandbox-expose-fd-ro"), pid });
        record.running.push(pid);
    }
    let (conn, caller, record) = (conn.clone(), message.header().sender().map(|sender| sender.to_string()), record.clone());
    std::thread::spawn(move || {
        let status = child.wait().map(|status| status.into_raw() as u32).unwrap_or(0);
        record.lock().unwrap().running.retain(|running| *running != pid);
        let _ = conn.emit_signal(caller.as_deref(), PATH, INTERFACE, "SpawnExited", &(pid, status));
    });
    Ok(pid)
}

fn describe(fd: &Fd<'_>) -> Exposed {
    let raw = fd.as_raw_fd();
    let flags = unsafe { libc::fcntl(raw, libc::F_GETFL) };
    Exposed { path: std::fs::read_link(format!("/proc/self/fd/{raw}")).unwrap_or_default(), o_path: flags >= 0 && flags & libc::O_PATH != 0 }
}
