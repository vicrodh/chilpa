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
use std::collections::HashSet;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
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
}

impl Watcher {
    pub(crate) fn start(keys: Arc<Keys>) -> Self {
        let (asks, inbox) = channel::<()>();
        let (outbox, looked) = channel::<Look>();
        let _ = std::thread::Builder::new().name("chilpa-reach".into()).spawn(move || {
            let mut wait = Duration::from_secs(2);
            loop {
                match inbox.recv_timeout(wait) {
                    Ok(()) | Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => break,
                }
                // Several asks while it was busy are one look.
                while inbox.try_recv().is_ok() {}
                if let Ok(lib) = chilpa_core::open_library() {
                    let _ = outbox.send(look(&lib, &Network, &keys));
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
        self.looked.try_iter().reduce(|a, b| Look { changed: a.changed || b.changed, refused: b.refused })
    }
}

/// One look at every folder and every server.
///
/// Only a server that answers with its credential accepted can play. One
/// that refuses it, or whose credential cannot be opened here, stays listed
/// and dimmed like one that does not answer (ADR-017 §2): its tracks would
/// fail at the first request.
pub(crate) fn look(lib: &chilpa_library::Library, net: &dyn chilpa_servers::Net, keys: &Keys) -> Look {
    let mut changed = lib.refresh_roots().unwrap_or(false);
    let mut refused = HashSet::new();
    for stored in lib.servers().unwrap_or_default() {
        if !stored.enabled { continue; }
        let (Some(mut server), Some(key)) = (net::server(&stored), keys.get(&stored.instance)) else {
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
    Look { changed, refused }
}
