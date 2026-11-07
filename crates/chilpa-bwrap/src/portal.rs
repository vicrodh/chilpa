//! The Flatpak portal that starts a process in a sandbox of its own, from
//! inside Chilpa's: `org.freedesktop.portal.Flatpak`, version 3 or later.
//!
//! What it starts is not a child. `Spawn` answers with a pid that may mean
//! nothing in this namespace, `SpawnSignal` ends it, and `SpawnExited`
//! reports its exit, for the process it started and not for that process's
//! own children. `WATCH_BUS` ends each sandbox when the connection that
//! asked for it leaves the bus, so one connection carries every spawn for as
//! long as Chilpa runs.
//!
//! Resources, declared (ADR-018): one connection and its bus thread, one
//! thread that reads exits, blocked on the socket, never polling. The exits
//! it keeps are one per live process handle plus at most [`EARLY_EXITS`].

use crate::process::NativeProcess;
use std::collections::{HashMap, VecDeque};
use std::ffi::OsStr;
use std::os::fd::BorrowedFd;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::Duration;
use zbus::blocking::{connection::Builder, Connection, MessageIterator};
use zbus::zvariant::{Fd, OwnedValue, Value};

pub(crate) const NAME: &str = "org.freedesktop.portal.Flatpak";
pub(crate) const PATH: &str = "/org/freedesktop/portal/Flatpak";
pub(crate) const INTERFACE: &str = "org.freedesktop.portal.Flatpak";

/// The first version with `sandbox-expose-fd` and `sandbox-flags`.
const MIN_VERSION: u32 = 3;

pub(crate) const CLEAR_ENV: u32 = 1;
pub(crate) const SANDBOX: u32 = 4;
pub(crate) const NO_NETWORK: u32 = 8;
pub(crate) const WATCH_BUS: u32 = 16;

/// How long a call to the portal may take before it fails.
const CALL_TIMEOUT: Duration = Duration::from_secs(10);
/// How long `wait` gives a process to report its exit.
const EXIT_WAIT: Duration = Duration::from_secs(10);
/// Exits the connection queues for the reader before it holds back.
const QUEUED_EXITS: usize = 64;
/// Exits kept for spawns whose answer has not been read yet.
const EARLY_EXITS: usize = 32;

/// The session bus the portal is reached on.
#[derive(Clone, Debug)]
pub(crate) enum Bus {
    Session,
    /// A private bus, for tests.
    #[cfg(test)]
    Address(String),
}

/// The flags of every spawn: an empty environment, a sandbox of its own,
/// ended when Chilpa leaves the bus, and no network unless it was given.
/// Never `EXPOSE_PIDS` or `SHARE_PIDS`.
pub(crate) fn flags(network: bool) -> u32 {
    let flags = CLEAR_ENV | SANDBOX | WATCH_BUS;
    if network { flags } else { flags | NO_NETWORK }
}

/// What one spawn asks for.
pub(crate) struct Spawn<'a> {
    pub(crate) cwd: &'a Path,
    pub(crate) argv: Vec<&'a OsStr>,
    pub(crate) env: Vec<(&'static str, String)>,
    /// Its standard input, output and error, in that order.
    pub(crate) stdio: [BorrowedFd<'a>; 3],
    pub(crate) network: bool,
    /// Folders mounted read-write at their own paths.
    pub(crate) writable: Vec<BorrowedFd<'a>>,
    /// Folders mounted read-only at their own paths.
    pub(crate) readable: Vec<BorrowedFd<'a>>,
}

/// Folders handed over as descriptors, as the portal's options take them.
fn exposed<'a>(fds: &[BorrowedFd<'a>]) -> Value<'a> {
    Value::from(fds.iter().map(|fd| Fd::from(*fd)).collect::<Vec<_>>())
}

/// The portal reads each byte string up to a NUL it must find there, as
/// GLib's bytestrings are read.
fn byte_string(text: &OsStr) -> Vec<u8> {
    let mut bytes = text.as_bytes().to_vec();
    bytes.push(0);
    bytes
}

pub(crate) struct Portal {
    conn: Connection,
    exits: Arc<Exits>,
}

impl Portal {
    /// Connects, checks the portal's version and starts the reader of exits
    /// before any spawn can need it.
    pub(crate) fn connect(bus: &Bus) -> Result<Arc<Self>, String> {
        let text = |error: zbus::Error| error.to_string();
        let builder = match bus {
            Bus::Session => Builder::session(),
            #[cfg(test)]
            Bus::Address(address) => Builder::address(address.as_str()),
        }.map_err(text)?;
        let conn = builder.method_timeout(CALL_TIMEOUT).build().map_err(text)?;
        // Reading the version also starts the portal when it is not running.
        let version = conn.call_method(Some(NAME), PATH, Some("org.freedesktop.DBus.Properties"), "Get", &(INTERFACE, "version"))
            .and_then(|reply| reply.body().deserialize::<OwnedValue>()).map_err(text)?;
        let version = u32::try_from(version).map_err(|error| error.to_string())?;
        if version < MIN_VERSION {
            return Err(format!("the portal speaks version {version}, and {MIN_VERSION} is needed"));
        }
        // Exits are read only from the portal's own connection.
        let owner = conn.call_method(Some("org.freedesktop.DBus"), "/org/freedesktop/DBus", Some("org.freedesktop.DBus"), "GetNameOwner", &NAME)
            .and_then(|reply| reply.body().deserialize::<String>()).map_err(text)?;
        let rule = zbus::MatchRule::builder().msg_type(zbus::message::Type::Signal).sender(owner.as_str())
            .and_then(|rule| rule.path(PATH)).and_then(|rule| rule.interface(INTERFACE)).and_then(|rule| rule.member("SpawnExited"))
            .map_err(text)?.build();
        let exited = MessageIterator::for_match_rule(rule, &conn, Some(QUEUED_EXITS)).map_err(text)?;
        let exits = Arc::new(Exits::default());
        let reader = exits.clone();
        std::thread::Builder::new().name("chilpa-portal-exits".into()).spawn(move || {
            for message in exited {
                let Ok(message) = message else { break };
                if let Ok((pid, status)) = message.body().deserialize::<(u32, u32)>() { reader.exited(pid, status); }
            }
            // The bus is gone, and the portal ended every sandbox with it.
            reader.lose();
        }).map_err(|error| error.to_string())?;
        Ok(Arc::new(Self { conn, exits }))
    }

    /// True once the connection is gone: nothing it started still runs.
    pub(crate) fn lost(&self) -> bool {
        self.exits.state().lost
    }

    /// Waits for the reader of exits to see the bus go.
    #[cfg(test)]
    pub(crate) fn wait_lost(&self, bound: Duration) -> bool {
        let state = self.exits.state();
        let (state, _) = self.exits.changed.wait_timeout_while(state, bound, |state| !state.lost)
            .unwrap_or_else(|error| error.into_inner());
        state.lost
    }

    pub(crate) fn spawn(self: &Arc<Self>, request: &Spawn<'_>) -> zbus::Result<Arc<PortalProcess>> {
        let fds: HashMap<u32, Fd<'_>> = request.stdio.iter().zip(0u32..).map(|(fd, number)| (number, Fd::from(*fd))).collect();
        let options: HashMap<&str, Value<'_>> = HashMap::from([
            ("sandbox-flags", Value::U32(0)),
            ("sandbox-expose-fd", exposed(&request.writable)),
            ("sandbox-expose-fd-ro", exposed(&request.readable)),
        ]);
        let env: HashMap<&str, &str> = request.env.iter().map(|(name, value)| (*name, value.as_str())).collect();
        let argv: Vec<Vec<u8>> = request.argv.iter().map(|arg| byte_string(arg)).collect();
        let body = (byte_string(request.cwd.as_os_str()), argv, fds, env, flags(request.network), options);
        let pid: u32 = self.conn.call_method(Some(NAME), PATH, Some(INTERFACE), "Spawn", &body)?.body().deserialize()?;
        self.exits.started(pid);
        Ok(Arc::new(PortalProcess { pid, portal: self.clone(), signal_sent: AtomicBool::new(false) }))
    }
}

impl Drop for Portal {
    /// The last handle on the portal closes its connection, which ends the
    /// reader of exits.
    fn drop(&mut self) {
        let _ = self.conn.clone().close();
    }
}

/// A process the portal started, as far as its exit is known.
pub(crate) struct PortalProcess {
    pid: u32,
    portal: Arc<Portal>,
    signal_sent: AtomicBool,
}

impl NativeProcess for PortalProcess {
    fn alive(&self) -> bool {
        self.portal.exits.running(self.pid)
    }

    fn terminate(&self) {
        if !self.alive() || self.signal_sent.swap(true, Ordering::SeqCst) { return; }
        // To its process group, which takes the plugin's own children where
        // they stayed in it.
        let signal = (self.pid, libc::SIGKILL as u32, true);
        let _ = self.portal.conn.call_method(Some(NAME), PATH, Some(INTERFACE), "SpawnSignal", &signal);
    }

    fn wait(&self) {
        if !self.portal.exits.wait(self.pid, EXIT_WAIT) {
            eprintln!("flatpak: process {} did not report its exit in {} s", self.pid, EXIT_WAIT.as_secs());
        }
    }
}

impl Drop for PortalProcess {
    fn drop(&mut self) {
        self.portal.exits.forget(self.pid);
    }
}

#[derive(Default)]
struct Exits {
    state: Mutex<ExitState>,
    changed: Condvar,
}

#[derive(Default)]
struct ExitState {
    /// Each process with a handle, and its wait status once it ended.
    tracked: HashMap<u32, Option<u32>>,
    /// Exits read before the answer to their spawn, oldest first.
    early: VecDeque<(u32, u32)>,
    lost: bool,
}

impl ExitState {
    fn running(&self, pid: u32) -> bool {
        !self.lost && self.tracked.get(&pid) == Some(&None)
    }
}

impl Exits {
    fn state(&self) -> MutexGuard<'_, ExitState> {
        self.state.lock().unwrap_or_else(|error| error.into_inner())
    }

    /// The portal answered a spawn. Its exit may have been read already: the
    /// answer and the signal reach different threads.
    fn started(&self, pid: u32) {
        let mut state = self.state();
        let early = state.early.iter().position(|(exited, _)| *exited == pid);
        let status = early.and_then(|index| state.early.remove(index)).map(|(_, status)| status);
        state.tracked.insert(pid, status);
    }

    fn exited(&self, pid: u32, status: u32) {
        let mut state = self.state();
        match state.tracked.get_mut(&pid) {
            Some(slot) => *slot = Some(status),
            None => {
                if state.early.len() == EARLY_EXITS { state.early.pop_front(); }
                state.early.push_back((pid, status));
            }
        }
        drop(state);
        self.changed.notify_all();
    }

    fn lose(&self) {
        self.state().lost = true;
        self.changed.notify_all();
    }

    fn running(&self, pid: u32) -> bool {
        self.state().running(pid)
    }

    /// Waits for the process to end, or the bus to go; false if neither
    /// came within `bound`.
    fn wait(&self, pid: u32, bound: Duration) -> bool {
        let state = self.state();
        let (state, _) = self.changed.wait_timeout_while(state, bound, |state| state.running(pid))
            .unwrap_or_else(|error| error.into_inner());
        !state.running(pid)
    }

    fn forget(&self, pid: u32) {
        self.state().tracked.remove(&pid);
    }
}

#[cfg(test)]
#[path = "portal_tests.rs"]
mod tests;
