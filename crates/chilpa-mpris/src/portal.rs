//! The desktop's portals, for what a sandboxed Chilpa cannot read itself:
//! the Settings portal says which colours the desktop asks for, and the
//! Documents portal says where a file the file chooser handed over lives on
//! the host.
//!
//! Resources, declared (ADR-018): one session-bus connection and one thread
//! that reads `SettingChanged`, blocked on the socket, never polling, with at
//! most [`QUEUED_CHANGES`] signals queued for it. Every call gives up after
//! [`CALL_TIMEOUT`]. Dropping the [`Portal`] closes the connection, which ends
//! the thread.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use zbus::blocking::{connection::Builder, Connection, MessageIterator};
use zbus::zvariant::{OwnedValue, Value};

pub(crate) const DESKTOP: &str = "org.freedesktop.portal.Desktop";
pub(crate) const DESKTOP_PATH: &str = "/org/freedesktop/portal/desktop";
const SETTINGS: &str = "org.freedesktop.portal.Settings";
const APPEARANCE: &str = "org.freedesktop.appearance";
const DOCUMENTS: &str = "org.freedesktop.portal.Documents";
const DOCUMENTS_PATH: &str = "/org/freedesktop/portal/documents";
const UNKNOWN_METHOD: &str = "org.freedesktop.DBus.Error.UnknownMethod";

/// How long a call to a portal may take before it fails.
pub(crate) const CALL_TIMEOUT: Duration = Duration::from_secs(2);
/// Changes the connection queues for the reader before it holds back.
const QUEUED_CHANGES: usize = 8;
/// The most documents asked about in one call.
pub const MAX_DOCUMENTS: usize = 256;

/// The session bus the portals are reached on.
#[derive(Clone, Debug)]
pub(crate) enum Bus {
    Session,
    /// A private bus, for tests.
    #[cfg(test)]
    Address(String),
}

impl Bus {
    pub(crate) fn connect(&self) -> zbus::Result<Connection> {
        let builder = match self {
            Bus::Session => Builder::session(),
            #[cfg(test)]
            Bus::Address(address) => Builder::address(address.as_str()),
        }?;
        builder.method_timeout(CALL_TIMEOUT).build()
    }

    pub(crate) async fn connect_async(&self) -> zbus::Result<zbus::Connection> {
        let builder = match self {
            Bus::Session => zbus::connection::Builder::session(),
            #[cfg(test)]
            Bus::Address(address) => zbus::connection::Builder::address(address.as_str()),
        }?;
        builder.method_timeout(CALL_TIMEOUT).build().await
    }
}

/// Whether the desktop asks for dark or light colours.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Scheme {
    #[default]
    NoPreference,
    Dark,
    Light,
}

/// What the desktop asks of an application's colours.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Appearance {
    pub scheme: Scheme,
    /// The accent the person chose, as 8-bit RGB, when the desktop says one.
    pub accent: Option<[u8; 3]>,
}

/// The Settings and Documents portals, on one connection made on a thread
/// of its own, so that whoever starts it never waits on the bus.
pub struct Portal {
    shared: Arc<Shared>,
}

/// Asks the Documents portal, from a thread that may wait on the bus.
#[derive(Clone)]
pub struct Documents {
    shared: Arc<Shared>,
}

struct Shared {
    link: Mutex<Link>,
    opened: Condvar,
}

enum Link {
    Connecting,
    Open(Connection),
    Closed,
}

impl Shared {
    fn link(&self) -> MutexGuard<'_, Link> {
        self.link.lock().unwrap_or_else(|error| error.into_inner())
    }

    fn close(&self) {
        let link = std::mem::replace(&mut *self.link(), Link::Closed);
        if let Link::Open(conn) = link {
            let _ = conn.close();
        }
        self.opened.notify_all();
    }

    /// The connection, waiting at most `bound` while it is being made.
    fn open(&self, bound: Duration) -> Option<Connection> {
        let deadline = Instant::now() + bound;
        let mut link = self.link();
        loop {
            match &*link {
                Link::Open(conn) => return Some(conn.clone()),
                Link::Closed => return None,
                Link::Connecting => {
                    let left = deadline.checked_duration_since(Instant::now())?;
                    link = self.opened.wait_timeout(link, left).unwrap_or_else(|error| error.into_inner()).0;
                }
            }
        }
    }
}

impl Portal {
    /// Connects, reads the desktop's appearance and hands it to `tell`, then
    /// hands every change of it, until the portal is dropped. A desktop
    /// without the portal is told as no preference, once.
    pub fn start(tell: impl Fn(Appearance) + Send + 'static) -> Option<Self> {
        Self::on(Bus::Session, tell)
    }

    pub(crate) fn on(bus: Bus, tell: impl Fn(Appearance) + Send + 'static) -> Option<Self> {
        let shared = Arc::new(Shared { link: Mutex::new(Link::Connecting), opened: Condvar::new() });
        let kept = shared.clone();
        let started = std::thread::Builder::new().name("chilpa-portal".into()).spawn(move || follow(&bus, kept, tell));
        match started {
            Ok(_) => Some(Self { shared }),
            Err(error) => {
                eprintln!("chilpa: portal: its thread could not start: {error}");
                None
            }
        }
    }

    pub fn documents(&self) -> Documents {
        Documents { shared: self.shared.clone() }
    }
}

impl Drop for Portal {
    fn drop(&mut self) {
        self.shared.close();
    }
}

fn follow(bus: &Bus, shared: Arc<Shared>, tell: impl Fn(Appearance)) {
    // The changes are read from before the first answer, so none falls between.
    let opened = bus.connect().and_then(|conn| {
        let rule = zbus::MatchRule::builder().msg_type(zbus::message::Type::Signal).path(DESKTOP_PATH)
            .and_then(|rule| rule.interface(SETTINGS)).and_then(|rule| rule.member("SettingChanged"))?.build();
        let changes = MessageIterator::for_match_rule(rule, &conn, Some(QUEUED_CHANGES))?;
        Ok((conn, changes))
    });
    let (conn, changes) = match opened {
        Ok(opened) => opened,
        Err(error) => {
            eprintln!("chilpa: portal: the session bus is out of reach: {error}");
            shared.close();
            tell(Appearance::default());
            return;
        }
    };
    {
        let mut link = shared.link();
        if matches!(*link, Link::Closed) {
            drop(link);
            let _ = conn.close();
            return;
        }
        *link = Link::Open(conn.clone());
    }
    shared.opened.notify_all();
    drop(shared);
    // Reading starts the portal when it is not running, so its owner is known after.
    let mut appearance = read_appearance(&conn);
    tell(appearance);
    let owner = name_owner(&conn, DESKTOP);
    drop(conn);
    for message in changes {
        let Ok(message) = message else { break };
        if owner.is_some() && message.header().sender().map(|sender| sender.as_str()) != owner.as_deref() {
            continue;
        }
        let Ok((namespace, key, value)) = message.body().deserialize::<(String, String, OwnedValue)>() else { continue };
        if namespace != APPEARANCE {
            continue;
        }
        let changed = match key.as_str() {
            "color-scheme" => Appearance { scheme: scheme_of(&value), ..appearance },
            "accent-color" => Appearance { accent: accent_of(&value), ..appearance },
            _ => continue,
        };
        if changed != appearance {
            appearance = changed;
            tell(appearance);
        }
    }
}

fn name_owner(conn: &Connection, name: &str) -> Option<String> {
    conn.call_method(Some("org.freedesktop.DBus"), "/org/freedesktop/DBus", Some("org.freedesktop.DBus"), "GetNameOwner", &name)
        .and_then(|reply| reply.body().deserialize::<String>()).ok()
}

/// The desktop's appearance as the Settings portal says it now. A key it
/// does not know reads as no preference.
fn read_appearance(conn: &Connection) -> Appearance {
    let scheme = read_setting(conn, "color-scheme").map(|value| scheme_of(&value)).unwrap_or_default();
    let accent = read_setting(conn, "accent-color").and_then(|value| accent_of(&value));
    Appearance { scheme, accent }
}

/// `ReadOne`, from version 2 of the portal; `Read` before it, which wraps
/// the value in one more variant.
fn read_setting(conn: &Connection, key: &str) -> Option<OwnedValue> {
    let call = |method: &str| conn.call_method(Some(DESKTOP), DESKTOP_PATH, Some(SETTINGS), method, &(APPEARANCE, key))
        .and_then(|reply| reply.body().deserialize::<OwnedValue>());
    match call("ReadOne") {
        Ok(value) => Some(value),
        Err(zbus::Error::MethodError(name, ..)) if name.as_str() == UNKNOWN_METHOD => {
            let value = call("Read").ok()?;
            match &*value {
                Value::Value(inner) => inner.try_to_owned().ok(),
                _ => Some(value),
            }
        }
        Err(_) => None,
    }
}

/// 1 asks for dark, 2 for light, anything else for nothing.
fn scheme_of(value: &Value<'_>) -> Scheme {
    match u32::try_from(value) {
        Ok(1) => Scheme::Dark,
        Ok(2) => Scheme::Light,
        _ => Scheme::NoPreference,
    }
}

/// Three doubles from 0 to 1; any outside that range means no accent.
fn accent_of(value: &Value<'_>) -> Option<[u8; 3]> {
    let Value::Structure(fields) = value else { return None };
    let channel = |field: &Value<'_>| match field {
        Value::F64(c) if (0.0..=1.0).contains(c) => Some((c * 255.0).round() as u8),
        _ => None,
    };
    match fields.fields() {
        [r, g, b] => Some([channel(r)?, channel(g)?, channel(b)?]),
        _ => None,
    }
}

/// A path under the document portal: the document's id, and what follows
/// the document's own name.
#[derive(Debug, PartialEq, Eq)]
struct DocumentPath {
    id: String,
    rest: PathBuf,
}

fn document_path(path: &Path, mounts: &[PathBuf]) -> Option<DocumentPath> {
    let inside = mounts.iter().find_map(|mount| path.strip_prefix(mount).ok())?;
    let mut parts = inside.components();
    let id = parts.next()?.as_os_str().to_str()?.to_owned();
    parts.next()?;
    Some(DocumentPath { id, rest: parts.as_path().to_path_buf() })
}

/// Whether `path` is one the document portal shows, which only it can say
/// where it lives on the host. Asks nothing of the bus.
pub fn in_document_portal(path: &Path) -> bool {
    document_path(path, &chilpa_dirs::document_portal_mounts()).is_some()
}

impl Documents {
    /// Where each document-portal path among `paths` lives on the host, for
    /// showing it; the rest stay out of the answer. `GetHostPaths` is from
    /// version 5 of the portal: an older one answers nothing. Waits on the
    /// bus for at most twice [`CALL_TIMEOUT`].
    pub fn host_paths(&self, paths: &[PathBuf]) -> HashMap<PathBuf, PathBuf> {
        self.host_paths_under(paths, &chilpa_dirs::document_portal_mounts())
    }

    fn host_paths_under(&self, paths: &[PathBuf], mounts: &[PathBuf]) -> HashMap<PathBuf, PathBuf> {
        let wanted: Vec<(&PathBuf, DocumentPath)> = paths.iter()
            .filter_map(|path| document_path(path, mounts).map(|document| (path, document))).take(MAX_DOCUMENTS).collect();
        if wanted.is_empty() {
            return HashMap::new();
        }
        let Some(conn) = self.shared.open(CALL_TIMEOUT) else { return HashMap::new() };
        let mut ids: Vec<&str> = wanted.iter().map(|(_, document)| document.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        let found = conn.call_method(Some(DOCUMENTS), DOCUMENTS_PATH, Some(DOCUMENTS), "GetHostPaths", &(ids,))
            .and_then(|reply| reply.body().deserialize::<HashMap<String, Vec<u8>>>());
        let found = match found {
            Ok(found) => found,
            Err(error) => {
                eprintln!("chilpa: portal: the documents' host paths could not be read: {error}");
                return HashMap::new();
            }
        };
        wanted.into_iter().filter_map(|(path, document)| {
            use std::os::unix::ffi::OsStrExt;
            let bytes = found.get(&document.id)?;
            let host = Path::new(std::ffi::OsStr::from_bytes(bytes.strip_suffix(&[0]).unwrap_or(bytes)));
            let host = if document.rest.as_os_str().is_empty() { host.to_path_buf() } else { host.join(&document.rest) };
            host.is_absolute().then(|| (path.clone(), host))
        }).collect()
    }
}

#[cfg(test)]
#[path = "portal_tests.rs"]
mod tests;
