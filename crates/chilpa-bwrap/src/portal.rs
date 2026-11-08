//! The Flatpak portal that starts a process in a sandbox of its own, from
//! inside Chilpa's: `org.freedesktop.portal.Flatpak`, version 3 or later.
//!
//! What it starts is not a child. `Spawn` answers with a pid that may mean
//! nothing in this namespace, `SpawnSignal` ends it, and `SpawnExited`
//! reports its exit, for the process it started and not for that process's
//! own children. Once that exit is reported the portal forgets the pid and
//! refuses to signal it again, so nothing this side can reach what it left
//! behind: `chilpa-plugin-runner` stays in the sandbox until all of it has
//! ended, which keeps the portal's process alive while anything there lives.
//! `WATCH_BUS` ends each sandbox when the connection that asked for it leaves
//! the bus, so one connection carries every spawn for as long as Chilpa runs.
//!
//! A signal the portal refused is sent again while its process is waited
//! for. A process whose handle goes while it may still run is not forgotten:
//! its pid stays in the ledger and it is ended, and so is one whose `Spawn`
//! answer came after the spawn was given up on.
//!
//! Resources, declared (ADR-018): one connection and its bus thread, one
//! thread that reads exits and answers, blocked on the socket, never
//! polling, and a short-lived thread for each process ended after its
//! handle went. The ledger keeps one entry per live process handle, at most
//! [`EARLY_EXITS`] exits read early, the calls waiting for an answer and at
//! most [`ABANDONED`] processes nothing holds.

use crate::process::NativeProcess;
use std::collections::{HashMap, VecDeque};
use std::ffi::OsStr;
use std::os::fd::BorrowedFd;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use zbus::blocking::{connection::Builder, Connection, MessageIterator};
use zbus::message::{Message, Type};
use zbus::zvariant::{DynamicType, Fd, OwnedValue, Value};

pub(crate) const NAME: &str = "org.freedesktop.portal.Flatpak";
pub(crate) const PATH: &str = "/org/freedesktop/portal/Flatpak";
pub(crate) const INTERFACE: &str = "org.freedesktop.portal.Flatpak";

/// The first version with `sandbox-expose-fd` and `sandbox-flags`.
const MIN_VERSION: u32 = 3;

pub(crate) const CLEAR_ENV: u32 = 1;
pub(crate) const SANDBOX: u32 = 4;
pub(crate) const NO_NETWORK: u32 = 8;
pub(crate) const WATCH_BUS: u32 = 16;

/// Messages from the portal the connection queues for the reader before it
/// holds back.
const QUEUED: usize = 64;
/// Exits kept for spawns whose answer has not been read yet.
const EARLY_EXITS: usize = 32;
/// Processes kept after nothing holds them: a spawn given up on, or a
/// handle dropped while its process ran.
const ABANDONED: usize = 64;
/// Pause between two signals the portal refused.
const RETRY_PAUSE: Duration = Duration::from_millis(500);
/// Signals sent to a process nothing holds before it is left to the bus.
const SIGNAL_ATTEMPTS: usize = 5;

/// How long each wait on the portal lasts.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Timeouts {
    /// For the answer to `Spawn`, and to the calls made while connecting.
    pub(crate) spawn: Duration,
    /// For the answer to `SpawnSignal`.
    pub(crate) signal: Duration,
    /// For a process to report its exit after it was ended.
    pub(crate) exit: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Self { spawn: Duration::from_secs(10), signal: Duration::from_secs(3), exit: Duration::from_secs(10) }
    }
}

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

fn text(error: zbus::Error) -> String {
    error.to_string()
}

pub(crate) struct Portal {
    conn: Connection,
    ledger: Arc<Ledger>,
    timeouts: Timeouts,
}

impl Portal {
    /// Connects, checks the portal's version and starts the reader of exits
    /// and answers before any spawn can need it.
    pub(crate) fn connect(bus: &Bus) -> Result<Arc<Self>, String> {
        Self::connect_with(bus, Timeouts::default())
    }

    pub(crate) fn connect_with(bus: &Bus, timeouts: Timeouts) -> Result<Arc<Self>, String> {
        let builder = match bus {
            Bus::Session => Builder::session(),
            #[cfg(test)]
            Bus::Address(address) => Builder::address(address.as_str()),
        }.map_err(text)?;
        let conn = builder.method_timeout(timeouts.spawn).build().map_err(text)?;
        // Reading the version also starts the portal when it is not running.
        let version = conn.call_method(Some(NAME), PATH, Some("org.freedesktop.DBus.Properties"), "Get", &(INTERFACE, "version"))
            .and_then(|reply| reply.body().deserialize::<OwnedValue>()).map_err(text)?;
        let version = u32::try_from(version).map_err(|error| error.to_string())?;
        if version < MIN_VERSION {
            return Err(format!("the portal speaks version {version}, and {MIN_VERSION} is needed"));
        }
        // Exits and answers are read only from the portal's own connection.
        let owner = conn.call_method(Some("org.freedesktop.DBus"), "/org/freedesktop/DBus", Some("org.freedesktop.DBus"), "GetNameOwner", &NAME)
            .and_then(|reply| reply.body().deserialize::<String>()).map_err(text)?;
        let rule = zbus::MatchRule::builder().sender(owner.as_str()).map_err(text)?.build();
        let messages = MessageIterator::for_match_rule(rule, &conn, Some(QUEUED)).map_err(text)?;
        let ledger = Arc::new(Ledger::default());
        let (reader, late) = (ledger.clone(), conn.clone());
        std::thread::Builder::new().name("chilpa-portal-exits".into()).spawn(move || {
            for message in messages {
                let Ok(message) = message else { break };
                read(&reader, &late, &message, timeouts.signal);
            }
            // The bus is gone, and the portal ended every sandbox with it.
            reader.lose();
        }).map_err(|error| error.to_string())?;
        Ok(Arc::new(Self { conn, ledger, timeouts }))
    }

    /// True once the connection is gone: nothing it started still runs.
    pub(crate) fn lost(&self) -> bool {
        self.ledger.state().lost
    }

    /// Waits for the reader of exits to see the bus go.
    #[cfg(test)]
    pub(crate) fn wait_lost(&self, bound: Duration) -> bool {
        let state = self.ledger.state();
        let (state, _) = self.ledger.changed.wait_timeout_while(state, bound, |state| !state.lost)
            .unwrap_or_else(|error| error.into_inner());
        state.lost
    }

    /// The processes the ledger still answers for, for tests.
    #[cfg(test)]
    pub(crate) fn tracked(&self) -> Vec<u32> {
        let mut pids: Vec<u32> = self.ledger.state().tracked.keys().copied().collect();
        pids.sort_unstable();
        pids
    }

    pub(crate) fn spawn(self: &Arc<Self>, request: &Spawn<'_>) -> Result<Arc<PortalProcess>, String> {
        let fds: HashMap<u32, Fd<'_>> = request.stdio.iter().zip(0u32..).map(|(fd, number)| (number, Fd::from(*fd))).collect();
        let options: HashMap<&str, Value<'_>> = HashMap::from([
            ("sandbox-flags", Value::U32(0)),
            ("sandbox-expose-fd", exposed(&request.writable)),
            ("sandbox-expose-fd-ro", exposed(&request.readable)),
        ]);
        let env: HashMap<&str, &str> = request.env.iter().map(|(name, value)| (*name, value.as_str())).collect();
        let argv: Vec<Vec<u8>> = request.argv.iter().map(|arg| byte_string(arg)).collect();
        let body = (byte_string(request.cwd.as_os_str()), argv, fds, env, flags(request.network), options);
        let reply = call(&self.conn, &self.ledger, Kind::Spawn, "Spawn", &body, self.timeouts.spawn)?;
        // The reader put it in the ledger when it read this answer, in the
        // order the portal sent it and its exit.
        let pid: u32 = reply.body().deserialize().map_err(text)?;
        Ok(Arc::new(PortalProcess { pid, portal: self.clone(), asked: AtomicBool::new(false), delivered: AtomicBool::new(false) }))
    }

    /// A SIGKILL to the process group the portal started for `pid`.
    fn signal(&self, pid: u32) -> Result<(), String> {
        signal(&self.conn, &self.ledger, pid, self.timeouts.signal)
    }
}

impl Drop for Portal {
    /// The last handle on the portal closes its connection, which ends the
    /// reader of exits.
    fn drop(&mut self) {
        let _ = self.conn.clone().close();
    }
}

/// What a call waits for in its answer.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    /// A pid, which the reader puts in the ledger.
    Spawn,
    Signal,
}

/// Sends a call to the portal and waits up to `timeout` for its answer,
/// which the reader of exits hands over. A `Spawn` given up on stays
/// expected: what it started is ended when its answer comes.
fn call<B>(conn: &Connection, ledger: &Ledger, kind: Kind, member: &str, body: &B, timeout: Duration) -> Result<Message, String>
where B: zbus::export::serde::ser::Serialize + DynamicType {
    let message = Message::method_call(PATH, member).and_then(|call| call.destination(NAME))
        .and_then(|call| call.interface(INTERFACE)).and_then(|call| call.build(body)).map_err(text)?;
    let serial = message.primary_header().serial_num().get();
    ledger.expect(serial, kind)?;
    if let Err(error) = conn.send(&message) {
        ledger.withdraw(serial);
        return Err(text(error));
    }
    ledger.answer(serial, timeout)
}

fn signal(conn: &Connection, ledger: &Ledger, pid: u32, timeout: Duration) -> Result<(), String> {
    // To its process group, which takes the sandbox's own init with it, and
    // with that init everything in the sandbox.
    call(conn, ledger, Kind::Signal, "SpawnSignal", &(pid, libc::SIGKILL as u32, true), timeout).map(drop)
}

/// Ends a process nothing holds any more, on a thread of its own: the
/// reader of exits cannot wait for an answer it is the one to read.
fn end_abandoned(conn: Connection, ledger: Arc<Ledger>, pid: u32, timeout: Duration) {
    let (sender, kept) = (conn.clone(), ledger.clone());
    let work = move || {
        for attempt in 1..=SIGNAL_ATTEMPTS {
            if !kept.running(pid) { return; }
            match signal(&sender, &kept, pid, timeout) {
                Ok(()) => return,
                Err(problem) => eprintln!("flatpak: could not end process {pid}, which nothing holds (attempt {attempt}): {problem}"),
            }
            std::thread::sleep(RETRY_PAUSE);
        }
        eprintln!("flatpak: process {pid} is left to the portal: it refused every signal");
    };
    if let Err(error) = std::thread::Builder::new().name("chilpa-portal-end".into()).spawn(work) {
        // No thread: the signal goes out once, and its answer is not waited for.
        eprintln!("flatpak: no thread to end process {pid} ({error}); one signal is sent unanswered");
        let message = Message::method_call(PATH, "SpawnSignal").and_then(|call| call.destination(NAME))
            .and_then(|call| call.interface(INTERFACE)).and_then(|call| call.build(&(pid, libc::SIGKILL as u32, true)));
        if let Err(problem) = message.and_then(|message| conn.send(&message)) {
            eprintln!("flatpak: could not end process {pid}: {problem}");
        }
    }
}

/// What the reader does with one message from the portal.
fn read(ledger: &Arc<Ledger>, conn: &Connection, message: &Message, signal_timeout: Duration) {
    let header = message.header();
    match message.message_type() {
        Type::Signal => {
            let exited = header.path().is_some_and(|path| path.as_str() == PATH)
                && header.interface().is_some_and(|interface| interface.as_str() == INTERFACE)
                && header.member().is_some_and(|member| member.as_str() == "SpawnExited");
            if !exited { return; }
            if let Ok((pid, status)) = message.body().deserialize::<(u32, u32)>() { ledger.exited(pid, status); }
        }
        Type::MethodReturn | Type::Error => {
            let Some(serial) = header.reply_serial() else { return };
            if let Some(pid) = ledger.answered(serial.get(), message) {
                end_abandoned(conn.clone(), ledger.clone(), pid, signal_timeout);
            }
        }
        Type::MethodCall => {}
    }
}

/// A process the portal started, as far as its exit is known.
pub(crate) struct PortalProcess {
    pid: u32,
    portal: Arc<Portal>,
    /// `terminate` was called.
    asked: AtomicBool,
    /// The portal took the signal for its group.
    delivered: AtomicBool,
}

impl PortalProcess {
    fn send_kill(&self) {
        match self.portal.signal(self.pid) {
            Ok(()) => self.delivered.store(true, Ordering::SeqCst),
            Err(problem) => eprintln!("flatpak: could not end process {}: {problem}", self.pid),
        }
    }

    /// Asked to end and the portal has not taken the signal yet.
    fn undelivered(&self) -> bool {
        self.asked.load(Ordering::SeqCst) && !self.delivered.load(Ordering::SeqCst)
    }
}

impl NativeProcess for PortalProcess {
    fn alive(&self) -> bool {
        self.portal.ledger.running(self.pid)
    }

    fn terminate(&self) {
        self.asked.store(true, Ordering::SeqCst);
        if !self.alive() || self.delivered.load(Ordering::SeqCst) { return; }
        self.send_kill();
    }

    /// A signal the portal refused is sent again while it waits.
    fn wait(&self) {
        let bound = self.portal.timeouts.exit;
        let deadline = Instant::now() + bound;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let slice = if self.undelivered() { remaining.min(RETRY_PAUSE) } else { remaining };
            if self.portal.ledger.wait(self.pid, slice) { return; }
            if Instant::now() >= deadline { break; }
            if self.undelivered() { self.send_kill(); }
        }
        eprintln!("flatpak: process {} did not report its exit in {} s", self.pid, bound.as_secs_f32());
    }
}

impl Drop for PortalProcess {
    /// While it may still run, its pid stays in the ledger, so its exit is
    /// still read; and if the portal never took its signal, it is ended now.
    fn drop(&mut self) {
        if self.portal.ledger.release(self.pid) && !self.delivered.load(Ordering::SeqCst) {
            end_abandoned(self.portal.conn.clone(), self.portal.ledger.clone(), self.pid, self.portal.timeouts.signal);
        }
    }
}

/// What the reader of exits knows: the processes, the calls waiting for an
/// answer and whether the bus is still there.
#[derive(Default)]
struct Ledger {
    state: Mutex<Entries>,
    changed: Condvar,
}

#[derive(Default)]
struct Entries {
    /// Each process the ledger answers for, by pid.
    tracked: HashMap<u32, Tracked>,
    /// Exits read before the answer to their spawn, oldest first.
    early: VecDeque<(u32, u32)>,
    /// Calls sent and not answered yet, by serial.
    calls: HashMap<u32, Call>,
    lost: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Tracked {
    /// Its wait status once it ended.
    status: Option<u32>,
    /// Nothing holds a handle on it: its spawn was given up on, or its
    /// handle went while it ran. It leaves the ledger when it exits.
    abandoned: bool,
}

enum Call {
    Waiting(Kind),
    Answered(Result<Message, String>),
    /// A `Spawn` its caller stopped waiting for.
    GivenUp,
}

impl Entries {
    fn running(&self, pid: u32) -> bool {
        !self.lost && self.tracked.get(&pid).is_some_and(|tracked| tracked.status.is_none())
    }

    /// The portal answered a spawn. Its exit may have been read already.
    fn started(&mut self, pid: u32, abandoned: bool) {
        let early = self.early.iter().position(|(exited, _)| *exited == pid);
        let status = early.and_then(|index| self.early.remove(index)).map(|(_, status)| status);
        if abandoned {
            if status.is_some() { return; }
            if self.tracked.values().filter(|tracked| tracked.abandoned).count() >= ABANDONED {
                eprintln!("flatpak: {ABANDONED} processes nothing holds are kept already; process {pid} is left to the bus");
                return;
            }
        }
        self.tracked.insert(pid, Tracked { status, abandoned });
    }
}

impl Ledger {
    fn state(&self) -> MutexGuard<'_, Entries> {
        self.state.lock().unwrap_or_else(|error| error.into_inner())
    }

    fn expect(&self, serial: u32, kind: Kind) -> Result<(), String> {
        let mut state = self.state();
        if state.lost { return Err("the session bus is gone".into()); }
        state.calls.insert(serial, Call::Waiting(kind));
        Ok(())
    }

    fn withdraw(&self, serial: u32) {
        self.state().calls.remove(&serial);
    }

    /// Waits for the answer to `serial`.
    fn answer(&self, serial: u32, timeout: Duration) -> Result<Message, String> {
        let state = self.state();
        let (mut state, _) = self.changed.wait_timeout_while(state, timeout,
            |state| !state.lost && matches!(state.calls.get(&serial), Some(Call::Waiting(_))))
            .unwrap_or_else(|error| error.into_inner());
        match state.calls.remove(&serial) {
            Some(Call::Answered(answer)) => answer,
            _ if state.lost => Err("the session bus is gone".into()),
            Some(Call::Waiting(Kind::Spawn)) => {
                state.calls.insert(serial, Call::GivenUp);
                // Only so many: past that, a late answer is not kept for.
                let given_up: Vec<u32> = state.calls.iter().filter(|(_, call)| matches!(call, Call::GivenUp)).map(|(serial, _)| *serial).collect();
                if given_up.len() > ABANDONED {
                    if let Some(oldest) = given_up.into_iter().min() { state.calls.remove(&oldest); }
                }
                Err(format!("the portal did not answer within {} s", timeout.as_secs_f32()))
            }
            _ => Err(format!("the portal did not answer within {} s", timeout.as_secs_f32())),
        }
    }

    /// An answer came. Returns the pid of a process nothing holds, to end.
    fn answered(&self, serial: u32, reply: &Message) -> Option<u32> {
        let mut state = self.state();
        let call = state.calls.remove(&serial)?;
        let answer = match reply.message_type() {
            Type::Error => Err(zbus::Error::from(reply.clone()).to_string()),
            _ => Ok(reply.clone()),
        };
        let pid = || answer.as_ref().ok().and_then(|reply| reply.body().deserialize::<u32>().ok());
        match call {
            Call::Waiting(kind) => {
                if kind == Kind::Spawn {
                    if let Some(pid) = pid() { state.started(pid, false); }
                }
                state.calls.insert(serial, Call::Answered(answer));
                drop(state);
                self.changed.notify_all();
                None
            }
            Call::GivenUp => {
                let pid = pid()?;
                eprintln!("flatpak: the portal answered a spawn given up on; process {pid} is ended");
                state.started(pid, true);
                state.running(pid).then_some(pid)
            }
            Call::Answered(answer) => {
                state.calls.insert(serial, Call::Answered(answer));
                None
            }
        }
    }

    fn exited(&self, pid: u32, status: u32) {
        let mut state = self.state();
        match state.tracked.get_mut(&pid) {
            Some(tracked) if tracked.abandoned => { state.tracked.remove(&pid); }
            Some(tracked) => tracked.status = Some(status),
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

    /// Its handle goes. One that may still run stays, marked as held by
    /// nothing; true if so.
    fn release(&self, pid: u32) -> bool {
        let mut state = self.state();
        if state.running(pid) {
            if let Some(tracked) = state.tracked.get_mut(&pid) { tracked.abandoned = true; }
            true
        } else {
            state.tracked.remove(&pid);
            false
        }
    }
}

#[cfg(test)]
#[path = "portal_tests.rs"]
mod tests;
