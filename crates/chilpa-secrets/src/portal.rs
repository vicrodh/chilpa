//! The per-app key of a Flatpak sandbox: `org.freedesktop.portal.Secret`,
//! version 1 or later, on the desktop portal.
//!
//! Inside the sandbox the system keyring sits behind a bus name the app is
//! not given. What it has instead is this portal: `RetrieveSecret` takes a
//! descriptor and answers with a request object; the desktop's own store
//! writes the app's value into the descriptor, and the request then sends
//! `Response`. The value is the same on every start for as long as the app
//! stays installed. It is never used as the key itself: the master key is
//! HKDF over it, labelled and bound to the service name.
//!
//! Resources, declared (ADR-018): one connection and one thread for the
//! exchange, both gone when it ends. The caller waits at most
//! [`Waits::whole`] for all of it, connecting included; when that runs out
//! the connection is closed and the thread ends with it. It runs once per
//! process ([`Shared`]), on the threads that open the boxes, never on the
//! window's.

use std::collections::HashMap;
use std::io::{ErrorKind, Read};
use std::os::fd::AsFd;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{mpsc, Arc, Mutex, MutexGuard};
use std::time::Duration;

use hkdf::Hkdf;
use sha2::Sha256;
use zbus::blocking::{connection::Builder, Connection, MessageIterator};
use zbus::zvariant::{Fd, OwnedObjectPath, OwnedValue, Value};

use crate::error::SecretError;

pub(crate) const NAME: &str = "org.freedesktop.portal.Desktop";
pub(crate) const PATH: &str = "/org/freedesktop/portal/desktop";
pub(crate) const INTERFACE: &str = "org.freedesktop.portal.Secret";
pub(crate) const REQUEST: &str = "org.freedesktop.portal.Request";

const MIN_VERSION: u32 = 1;
/// Fewer bytes than this is not worth deriving a key from. The desktop
/// stores hand out 64.
pub(crate) const MIN_LEN: usize = 16;
/// More than this is not what the portal hands out.
pub(crate) const MAX_LEN: usize = 1024;
/// What the reader gives the first bytes once the request has answered. The
/// store writes them before it answers, so they are normally there already.
const FIRST_BYTES: Duration = Duration::from_secs(2);
/// Answers the connection queues before it holds back; one is expected.
const QUEUED: usize = 4;
const HKDF_INFO: &[u8] = b"chilpa-secrets portal master-key derivation v1";
const KEY_LEN: usize = 32;

/// How long the portal is given.
pub(crate) struct Waits {
    /// Each call on the bus, which answers at once or not at all.
    pub(crate) call: Duration,
    /// The whole exchange. The desktop's store may be locked and ask the
    /// person to unlock it before it answers.
    pub(crate) whole: Duration,
}

pub(crate) const WAITS: Waits = Waits { call: Duration::from_secs(10), whole: Duration::from_secs(30) };

/// True inside a Flatpak sandbox, which marks itself with this file.
pub(crate) fn sandboxed() -> bool {
    Path::new("/.flatpak-info").exists()
}

/// The session bus the portal is reached on.
#[derive(Clone, Debug)]
pub(crate) enum Bus {
    Session,
    /// A private bus, for tests.
    #[cfg(test)]
    Address(String),
}

/// Why the portal's key is not used.
///
/// `what` is in words the session log keeps: its redaction blanks a whole
/// line that names what this portal is called, and the bus's errors do.
/// `detail` is the bus's own message, for a terminal.
#[derive(Clone, Debug)]
pub(crate) struct Unanswered {
    pub(crate) what: String,
    pub(crate) detail: String,
}

impl Unanswered {
    fn new(what: impl Into<String>, detail: impl std::fmt::Display) -> Self {
        Self { what: what.into(), detail: detail.to_string() }
    }

    /// Two lines, once per process.
    fn say(&self) {
        eprintln!("portal: not used by this process, which derives its keys from this machine: {}", self.what);
        if !self.detail.is_empty() {
            eprintln!("portal: the bus said: {}", self.detail);
        }
    }

    fn into_error(self) -> SecretError {
        if self.detail.is_empty() {
            SecretError::Portal(self.what)
        } else {
            SecretError::Portal(format!("{}: {}", self.what, self.detail))
        }
    }
}

/// The portal's value for the whole process.
///
/// The boxes that open at startup, each on a thread of its own, would each
/// ask the portal. Here the first one asks alone and the others wait for it,
/// for at most [`Waits::whole`], and take the same outcome, the refusal
/// included: a process either has the portal or it does not.
pub(crate) struct Shared {
    resolved: Mutex<Option<Result<Vec<u8>, Unanswered>>>,
}

pub(crate) static SHARED: Shared = Shared::new();

impl Shared {
    pub(crate) const fn new() -> Self {
        Self { resolved: Mutex::new(None) }
    }

    pub(crate) fn resolve(
        &self,
        ask: impl FnOnce() -> Result<Vec<u8>, Unanswered>,
    ) -> Result<Vec<u8>, SecretError> {
        let mut resolved = self.resolved.lock().unwrap_or_else(|e| e.into_inner());
        let outcome = resolved.get_or_insert_with(|| {
            let outcome = ask();
            if let Err(unanswered) = &outcome {
                unanswered.say();
            }
            outcome
        });
        outcome.clone().map_err(Unanswered::into_error)
    }
}

/// The master key for `service_name`, from the portal's value. The label
/// keeps it apart from every other use of the same value, and the service
/// name from another box's.
pub(crate) fn master_key(value: &[u8], service_name: &str) -> Result<[u8; KEY_LEN], SecretError> {
    let hk = Hkdf::<Sha256>::new(None, value);
    let mut out = [0u8; KEY_LEN];
    hk.expand_multi_info(&[HKDF_INFO, &[0], service_name.as_bytes()], &mut out)
        .map_err(|e| SecretError::Other(format!("HKDF: {e}")))?;
    Ok(out)
}

/// Where the exchange's connection is, so the caller can close it when the
/// wait runs out.
enum Slot {
    Connecting,
    Open(Connection),
    Abandoned,
}

fn lock(slot: &Mutex<Slot>) -> MutexGuard<'_, Slot> {
    slot.lock().unwrap_or_else(|e| e.into_inner())
}

/// The app's value from the portal on `bus`, within `waits.whole`.
pub(crate) fn retrieve(bus: &Bus, waits: &Waits) -> Result<Vec<u8>, Unanswered> {
    let slot = Arc::new(Mutex::new(Slot::Connecting));
    let (answer, answered) = mpsc::sync_channel(1);
    let (bus, call, kept) = (bus.clone(), waits.call, Arc::clone(&slot));
    std::thread::Builder::new()
        .name("chilpa-secrets-portal".into())
        .spawn(move || drop(answer.send(exchange(&bus, call, &kept))))
        .map_err(|e| Unanswered::new("no thread to ask it from", e))?;
    let outcome = match answered.recv_timeout(waits.whole) {
        Ok(outcome) => outcome,
        Err(mpsc::RecvTimeoutError::Timeout) => {
            Err(Unanswered::new(format!("no answer within {} ms", waits.whole.as_millis()), ""))
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(Unanswered::new("the exchange stopped without an answer", "")),
    };
    // Whatever still waits on the bus ends with its connection.
    if let Slot::Open(conn) = std::mem::replace(&mut *lock(&slot), Slot::Abandoned) {
        let _ = conn.close();
    }
    outcome
}

fn exchange(bus: &Bus, call: Duration, slot: &Mutex<Slot>) -> Result<Vec<u8>, Unanswered> {
    let no_bus = |e: zbus::Error| Unanswered::new("no session bus", e);
    let builder = match bus {
        Bus::Session => Builder::session(),
        #[cfg(test)]
        Bus::Address(address) => Builder::address(address.as_str()),
    }
    .map_err(no_bus)?;
    let conn = builder.method_timeout(call).build().map_err(no_bus)?;
    {
        let mut slot = lock(slot);
        if matches!(*slot, Slot::Abandoned) {
            let _ = conn.close();
            return Err(Unanswered::new("given up on before it connected", ""));
        }
        *slot = Slot::Open(conn.clone());
    }
    ask(&conn)
}

/// A name for the request object, unique within this connection.
fn request_name() -> String {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    format!("chilpa_{}_{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed))
}

fn ask(conn: &Connection) -> Result<Vec<u8>, Unanswered> {
    // Reading the version also starts the portal when it is not running,
    // and fails where no store of the desktop serves this interface.
    let version = conn
        .call_method(Some(NAME), PATH, Some("org.freedesktop.DBus.Properties"), "Get", &(INTERFACE, "version"))
        .and_then(|reply| reply.body().deserialize::<OwnedValue>())
        .map_err(|e| Unanswered::new("the desktop offers no per-app key", e))?;
    let version = u32::try_from(version).map_err(|e| Unanswered::new("the portal's version is not a number", e))?;
    if version < MIN_VERSION {
        return Err(Unanswered::new(format!("the portal speaks version {version}, and {MIN_VERSION} is needed"), ""));
    }
    // The answer is taken only from the portal's own connection.
    let owner = conn
        .call_method(Some("org.freedesktop.DBus"), "/org/freedesktop/DBus", Some("org.freedesktop.DBus"), "GetNameOwner", &NAME)
        .and_then(|reply| reply.body().deserialize::<String>())
        .map_err(|e| Unanswered::new("the portal has no owner on the bus", e))?;
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(owner.as_str())
        .and_then(|rule| rule.interface(REQUEST))
        .and_then(|rule| rule.member("Response"))
        .map_err(|e| Unanswered::new("the answer could not be listened for", e))?
        .build();
    // Listening before asking, so the answer cannot arrive first.
    let answers = MessageIterator::for_match_rule(rule, conn, Some(QUEUED))
        .map_err(|e| Unanswered::new("the answer could not be listened for", e))?;
    let (mut ours, theirs) = UnixStream::pair().map_err(|e| Unanswered::new("no socket to receive it on", e))?;
    let options = HashMap::from([("handle_token", Value::from(request_name()))]);
    let handle: OwnedObjectPath = conn
        .call_method(Some(NAME), PATH, Some(INTERFACE), "RetrieveSecret", &(Fd::from(theirs.as_fd()), options))
        .and_then(|reply| reply.body().deserialize())
        .map_err(|e| Unanswered::new("the portal refused the request", e))?;
    // Only the desktop holds the writing end now.
    drop(theirs);
    for message in answers {
        let message = message.map_err(|e| Unanswered::new("the bus failed before the answer", e))?;
        if message.header().path().map(|path| path.as_str()) != Some(handle.as_str()) {
            continue;
        }
        let (response, _): (u32, HashMap<String, OwnedValue>) =
            message.body().deserialize().map_err(|e| Unanswered::new("the answer is not a response", e))?;
        // 1 is a refusal by the person, 2 any other end.
        if response != 0 {
            return Err(Unanswered::new(format!("the request ended with response {response}"), ""));
        }
        return read_value(&mut ours);
    }
    Err(Unanswered::new("the bus closed before the answer", ""))
}

/// What the store wrote, which is in the socket by the time the request
/// answers. A writer that keeps its end open does not hold this up: once the
/// first bytes are in, only what is already there is read.
fn read_value(socket: &mut UnixStream) -> Result<Vec<u8>, Unanswered> {
    let failed = |e: std::io::Error| Unanswered::new("the value could not be read", e);
    socket.set_read_timeout(Some(FIRST_BYTES)).map_err(failed)?;
    let mut value = Vec::with_capacity(64);
    let mut chunk = [0u8; 256];
    loop {
        match socket.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => {
                value.extend_from_slice(&chunk[..read]);
                if value.len() > MAX_LEN {
                    return Err(Unanswered::new(format!("the value is longer than {MAX_LEN} bytes"), ""));
                }
                socket.set_nonblocking(true).map_err(failed)?;
            }
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => break,
            Err(e) => return Err(failed(e)),
        }
    }
    chunk.fill(0);
    if value.len() < MIN_LEN {
        return Err(Unanswered::new(format!("the value is {} bytes, and {MIN_LEN} are needed", value.len()), ""));
    }
    Ok(value)
}

#[cfg(test)]
#[path = "portal_tests.rs"]
mod tests;
