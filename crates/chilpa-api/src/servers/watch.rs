//! Whether each folder and each server answers, asked every minute on a
//! thread of its own (ADR-017 §2).
//!
//! A NAS that hangs can block a `stat` for minutes and a server that does
//! not answer takes its timeout; neither may reach the window. What changes
//! is written to the library from here, with a connection of this thread's
//! own, and the window is only told that something did.

use super::keys::Keys;
use super::net::{self, Network};
use chilpa_servers::{Failure, Kind};
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// How often it looks without being asked.
const EVERY: Duration = Duration::from_secs(60);

pub(crate) struct Watcher {
    asks: Sender<()>,
    looked: Receiver<Look>,
}

/// What one look found.
pub(crate) struct Look {
    /// Something was written to the library.
    pub(crate) changed: bool,
    /// The servers that refused their credential, or whose credential could
    /// not be opened: they cannot play, and the row says why.
    pub(crate) refused: HashSet<i64>,
    pub(crate) sessions: HashMap<i64, super::session::Scope>,
}

impl Watcher {
    pub(crate) fn start(keys: Arc<Keys>, gate: Arc<Mutex<()>>,invalid:super::offline::InvalidCopies) -> Self {
        let (asks, inbox) = channel::<()>();
        let (outbox, looked) = channel::<Look>();
        let _ = std::thread::Builder::new().name("chilpa-reach".into()).spawn(move || {
            let mut wait = Duration::from_secs(2);
            let mut library = None;
            loop {
                match inbox.recv_timeout(wait) {
                    Ok(()) | Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => break,
                }
                // Several asks while it was busy are one look.
                while inbox.try_recv().is_ok() {}
                // I keep this worker's connection instead of repeating library maintenance.
                if library.is_none() { library = chilpa_core::open_library().ok(); }
                if let Some(lib) = &library {
                    let changed=invalidate_copies(lib,&invalid);
                    let mut result=look(lib,&Network,&keys,&gate);
                    result.changed|=changed;
                    let _ = outbox.send(result);
                }
                wait = EVERY;
            }
        });
        Self { asks, looked }
    }

    /// Looks now, on its thread.
    pub(crate) fn ask(&self) {
        let _ = self.asks.send(());
    }

    /// What the looks since the last time this was asked found: whether
    /// anything changed, and the latest set of refusals.
    pub(crate) fn looked(&self) -> Option<Look> {
        self.looked.try_iter().reduce(|a, b| Look { changed: a.changed || b.changed, refused: b.refused, sessions:b.sessions })
    }
}

pub(super) fn invalidate_copies(lib:&chilpa_library::Library,invalid:&super::offline::InvalidCopies) -> bool {
    use std::sync::atomic::Ordering;
    let scan=invalid.scan.swap(false,Ordering::AcqRel);
    let copies=std::mem::take(&mut *invalid.pending.lock().unwrap_or_else(|error| error.into_inner()));
    let mut changed=false;
    for snapshot in copies {changed|=invalidate_copy(lib,&snapshot,invalid);}
    if scan {
        let mut after=String::new();
        loop {
            let copies=match lib.offline_current_copy_page(&after) {
                Ok(copies)=>copies,
                Err(_)=>{invalid.scan.store(true,Ordering::Release);break;}
            };
            if copies.is_empty() {break;}
            for copy in copies {
                after=copy.key.clone();
                changed|=invalidate_copy(lib,&copy,invalid);
            }
        }
    }
    changed
}

fn invalidate_copy(lib:&chilpa_library::Library,snapshot:&chilpa_library::store::offline::Copy,
    invalid:&super::offline::InvalidCopies) -> bool {
    use std::sync::atomic::Ordering;
    let current=match lib.offline_copy_by_key(&snapshot.key) {
        Ok(copy)=>copy.filter(|copy| copy.state=="complete" && copy.path==snapshot.path
            && copy.checksum==snapshot.checksum && copy.bytes==snapshot.bytes),
        Err(_)=>{invalid.scan.store(true,Ordering::Release);return false;}
    };
    let Some(current)=current else {return false;};
    match super::offline::checked(&current,|| false) {
        Err(super::offline::CheckError::Invalid)=>match lib.invalidate_checked_offline_copy(&current,super::offline::disk_bytes(&current)) {
            Ok(changed)=>changed,
            Err(_)=>{invalid.scan.store(true,Ordering::Release);false}
        },
        Err(_)=>{invalid.scan.store(true,Ordering::Release);false}
        Ok(_)=>false,
    }
}

/// One look at every folder and every server.
///
/// Availability combines authenticated reachability and verified downloads.
pub(crate) fn look(lib: &chilpa_library::Library, net: &dyn chilpa_servers::Net, keys: &Keys, gate: &Mutex<()>) -> Look {
    let mut changed = lib.refresh_roots().unwrap_or(false);
    let mut refused = HashSet::new();
    let mut sessions = HashMap::new();
    for stored in lib.servers().unwrap_or_default() {
        let _watching = gate.lock().unwrap_or_else(|error| error.into_inner());
        let Some(stored) = lib.server(stored.source_id).ok().flatten() else { continue };
        if !stored.enabled { continue; }
        sessions.insert(stored.source_id,super::session::Scope::of(&stored));
        if chilpa_core::network::current().offline {
            changed |= lib.set_source_reachable(stored.source_id,false).unwrap_or(false);
            continue;
        }
        let (Some(mut server), Some(key)) = (net::server(&stored), net::key(keys, &stored)) else {
            refused.insert(stored.source_id);
            changed |= lib.set_source_reachable(stored.source_id, false).unwrap_or(false);
            continue;
        };
        let mut answers = match chilpa_servers::ping(net, &server, &key) {
            Ok(()) => true,
            Err(Failure::Refused(_)) => {
                refused.insert(stored.source_id);
                false
            }
            Err(_) => false,
        };
        // A Plex server that stopped answering at one address may answer at
        // another: the LAN one when back home, the remote one away from it.
        if !answers && !refused.contains(&stored.source_id) && server.kind == Kind::Plex && server.details.connections.len() > 1 {
            if let Ok(address) = chilpa_servers::plex::reach(net, &server.client, &stored.identity, &server.details.connections) {
                server.address = address.clone();
                answers = chilpa_servers::ping(net, &server, &key).is_ok();
                if answers {
                    let _ = lib.update_server(stored.source_id, &stored.label, &address, &stored.product, &stored.libraries, &stored.details);
                    changed = true;
                }
            }
        }
        changed |= lib.set_source_reachable(stored.source_id, answers).unwrap_or(false);
    }
    Look { changed, refused, sessions }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chilpa_library::Library;

    #[test]
    fn a_reused_connection_sees_external_root_edits_and_mount_changes() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("library.db");
        let writer = Library::open(&database).unwrap();
        let watcher = Library::open_existing(&database).unwrap();
        let keys = Keys::derived(directory.path().join("keys"));
        assert!(!look(&watcher, &Network, &keys, &Mutex::new(())).changed);

        let source = writer.source_id("local", "default").unwrap();
        let folder = directory.path().join("music");
        let root = writer.root_id(source, &folder).unwrap();
        assert!(look(&watcher, &Network, &keys, &Mutex::new(())).changed);
        assert!(!watcher.roots().unwrap()[0].reachable);
        assert!(!look(&watcher, &Network, &keys, &Mutex::new(())).changed);

        std::fs::create_dir(&folder).unwrap();
        assert!(look(&watcher, &Network, &keys, &Mutex::new(())).changed);
        assert!(watcher.roots().unwrap()[0].reachable);
        assert!(!look(&watcher, &Network, &keys, &Mutex::new(())).changed);

        writer.forget_root(root).unwrap();
        assert!(!look(&watcher, &Network, &keys, &Mutex::new(())).changed);
        assert!(watcher.roots().unwrap().is_empty());
    }
}
