//! What is asked of a server's own playlist, on a thread of its own
//! (ADR-017, as amended for them).
//!
//! The server is the source of truth. A change goes to the server, and the
//! library changes only after the server answers, with the playlist as the
//! server reads it back. A change by row (taking tracks out, a new order)
//! reads the playlist first and goes out only if the server still has the
//! rows the library shows; otherwise nothing is sent, and the library takes
//! what the server has now. Nothing is retried: a change that failed
//! halfway is read back and left for the listener to see.
//!
//! One change at a time, and none while the playlists are read
//! (`reading`): both hold the same gate.

use super::keys::Keys;
use super::net::{self, Network};
use super::reading::{mirror, stops};
use chilpa_servers::{Failure, Net, Playlist, PlaylistTracks};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

/// A change in the library's words: tracks by their queue entries, rows by
/// their place in the library's playlist.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Change {
    /// A description no server takes in its creation: it goes after, as an
    /// edit, when it is not empty.
    Create {
        name: String,
        description: String,
        tracks: Vec<String>,
    },
    /// A new name, a new description, or both; `None` keeps that one.
    Edit {
        name: Option<String>,
        description: Option<String>,
    },
    Delete,
    Add {
        tracks: Vec<String>,
    },
    /// These rows out.
    Remove {
        rows: Vec<usize>,
    },
    /// Every row in a new order: the row each one was, in the order they
    /// end up in.
    Reorder {
        rows: Vec<usize>,
    },
}

/// One change asked of one server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Write {
    /// The server's source.
    pub(crate) source: i64,
    /// The playlist in the library; 0 for one to create.
    pub(crate) playlist: i64,
    /// The request of the window that asked, to say how it went; empty
    /// when nobody waits for it.
    pub(crate) request: String,
    pub(crate) change: Change,
}

/// How a change went.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Written {
    pub(crate) source: i64,
    /// The playlist it was asked of; 0 for one to create.
    pub(crate) asked: i64,
    /// The playlist after: the one asked of, or the one created; 0 when it
    /// left.
    pub(crate) playlist: i64,
    pub(crate) request: String,
    /// Why it did not go through, for the window. Empty when it did.
    pub(crate) why: String,
    /// Read back, the playlist names a track the library does not hold: a
    /// sync may bring it.
    pub(crate) missing: bool,
}

pub(crate) struct Writer {
    orders: Sender<Write>,
    done: Receiver<Written>,
}

impl Writer {
    /// Starts the thread. It waits on its orders and does nothing else.
    pub(crate) fn start(keys: Arc<Keys>, gate: Arc<Mutex<()>>) -> Self {
        let (orders, inbox) = channel::<Write>();
        let (outbox, done) = channel::<Written>();
        let _ = std::thread::Builder::new().name("chilpa-server-writes".into()).spawn(move || {
            while let Ok(write) = inbox.recv() {
                let written = match chilpa_core::open_library() {
                    Ok(lib) => apply(&lib, &Network, &keys, &gate, &write),
                    Err(_) => refused(&write, chilpa_i18n::t("the library could not be opened")),
                };
                let _ = outbox.send(written);
            }
        });
        Self { orders, done }
    }

    pub(crate) fn ask(&self, write: Write) -> bool {
        self.orders.send(write).is_ok()
    }

    pub(crate) fn poll(&self) -> Vec<Written> {
        self.done.try_iter().collect()
    }
}

fn refused(write: &Write, why: String) -> Written {
    Written { source: write.source, asked: write.playlist, playlist: write.playlist, request: write.request.clone(), why, missing: false }
}

/// Asks the server for one change, and writes the playlist as the server
/// has it after.
pub(crate) fn apply(lib: &chilpa_library::Library, net: &dyn Net, keys: &Keys, gate: &Mutex<()>, write: &Write) -> Written {
    let failed = |why: String| refused(write, why);
    let Some(stored) = lib.server(write.source).ok().flatten() else {
        return failed(chilpa_i18n::t("that server is no longer in the library"));
    };
    if !stored.enabled {
        return failed(chilpa_i18n::t("that server is disabled"));
    }
    let (Some(server), Some(key)) = (net::server(&stored), keys.get(&stored.instance)) else {
        return failed(chilpa_i18n::t_args("sign in to {} again", &[&stored.label]));
    };
    let said = |why: &str| chilpa_i18n::t_args("{} did not take the change to the playlist: {}", &[&stored.label, why]);
    let kept = match write.playlist {
        0 => None,
        id => match lib.playlists().ok().and_then(|all| all.into_iter().find(|p| p.id == id && p.source_id == write.source)) {
            Some(kept) => Some(kept),
            None => return failed(said(&chilpa_i18n::t("the playlist is no longer in the library"))),
        },
    };
    let _writing = gate.lock().unwrap_or_else(|e| e.into_inner());
    let remote = kept.as_ref().map(|p| p.external_id.clone()).unwrap_or_default();
    let meta =
        |name: &str, writable: bool| Playlist { id: remote.clone(), name: name.to_string(), description: kept.as_ref().map(|p| p.description.clone()).unwrap_or_default(), editable: Some(writable) };
    // What the server has now, for a change by row, and which of its
    // entries are the library's rows.
    let (as_read, places) = match (&write.change, kept.as_ref()) {
        (Change::Remove { .. } | Change::Reorder { .. }, Some(p)) => {
            let read = match chilpa_servers::playlist_tracks(net, &server, &key, &remote) {
                Ok(read) => read,
                Err(failure) => return lost(lib, write, p.id, &stored.label, &failure),
            };
            let Some(now) = mirror(lib, &stored, &meta(&p.name, p.writable), p.writable, &read) else {
                return failed(said(&chilpa_i18n::t("the library could not be written")));
            };
            if now.changed {
                return Written { playlist: p.id, ..failed(chilpa_i18n::t_args("The playlist changed on {}; it now shows what the server has. Try again.", &[&stored.label])) };
            }
            let tracks: Vec<usize> = read.entries.iter().enumerate().filter(|(_, e)| e.track).map(|(at, _)| at).collect();
            let places: Vec<usize> = now.kept.iter().map(|&k| tracks[k]).collect();
            (read, places)
        }
        _ => (PlaylistTracks::default(), Vec::new()),
    };
    let Some(change) = protocol(&stored, &as_read, &places, &write.change) else {
        return failed(said(&chilpa_i18n::t("that change does not fit the playlist as it was read")));
    };
    let asked = chilpa_servers::change_playlist(net, &server, &key, &remote, &as_read, &change);
    // A playlist that waited here, unsent, has nothing on the server to read
    // back or to lose: a failure leaves it waiting.
    let unsent = kept.as_ref().is_some_and(|p| p.unsent);
    let created = match (&write.change, asked) {
        (_, Err(failure)) => {
            // Whatever reached the server before the failure is read back.
            if let Some(p) = kept.as_ref().filter(|_| !unsent && !stops(&failure) && !matches!(failure, Failure::Gone(_))) {
                // Some may change a playlist and not delete it: a refused
                // delete says nothing of the rest.
                let writable = p.writable && !(matches!(failure, Failure::Forbidden(_)) && !matches!(write.change, Change::Delete));
                if let Ok(read) = chilpa_servers::playlist_tracks(net, &server, &key, &remote) {
                    let _ = mirror(lib, &stored, &meta(&p.name, writable), writable, &read);
                }
            }
            return match (kept.as_ref(), &failure) {
                (Some(p), Failure::Gone(_)) if !unsent => lost(lib, write, p.id, &stored.label, &failure),
                _ => failed(said(&failure.to_string())),
            };
        }
        (Change::Delete, Ok(_)) => {
            if let Some(p) = &kept {
                let _ = lib.delete_playlist(p.id);
            }
            return Written { playlist: 0, ..failed(String::new()) };
        }
        (_, Ok(created)) => created,
    };
    // Read back: the server's word is what stays.
    let (id, name, description) = match (&write.change, created, kept.as_ref()) {
        (Change::Create { name, .. }, Some(id), _) => (id, name.clone(), String::new()),
        // Created, and the server did not say its id: a sync, which reads
        // its playlists after, brings it in, and one that waited here gives
        // way to it instead of being a second one.
        (Change::Create { .. }, None, _) => {
            if let Some(p) = kept.as_ref().filter(|_| unsent) {
                let _ = lib.delete_playlist(p.id);
            }
            return Written { playlist: 0, missing: true, ..failed(String::new()) };
        }
        (Change::Edit { name, description }, _, Some(p)) => (remote.clone(), name.clone().unwrap_or_else(|| p.name.clone()), description.clone().unwrap_or_else(|| p.description.clone())),
        (_, _, Some(p)) => (remote.clone(), p.name.clone(), p.description.clone()),
        (_, _, None) => return failed(String::new()),
    };
    // One that waited here takes the id the server gave it, so the playlist
    // read back lands on its row: its place in the sidebar, its folder.
    if let Some(p) = kept.as_ref().filter(|_| unsent) {
        if !lib.bind_unsent(p.id, &id).unwrap_or(false) {
            return failed(said(&chilpa_i18n::t("the library could not be written")));
        }
    }
    // Made, it takes its description as an edit. Refused, the playlist is
    // read in all the same, and the window says why.
    let (description, why) = match &write.change {
        Change::Create { description: text, .. } if !text.is_empty() => {
            let edit = chilpa_servers::Change::Edit { name: None, description: Some(text.clone()) };
            match chilpa_servers::change_playlist(net, &server, &key, &id, &PlaylistTracks::default(), &edit) {
                Ok(_) => (text.clone(), String::new()),
                Err(failure) => (description, said(&failure.to_string())),
            }
        }
        _ => (description, String::new()),
    };
    let writable = kept.as_ref().is_none_or(|p| p.writable);
    let read = match chilpa_servers::playlist_tracks(net, &server, &key, &id) {
        Ok(read) => read,
        Err(failure) => return failed(said(&failure.to_string())),
    };
    let playlist = Playlist { id, name, description, editable: Some(writable) };
    match mirror(lib, &stored, &playlist, writable, &read) {
        Some(now) => Written { playlist: now.id, missing: now.left_out > 0, ..failed(why) },
        None => failed(said(&chilpa_i18n::t("the library could not be written"))),
    }
}

/// The server no longer has the playlist: the library lets it go too.
fn lost(lib: &chilpa_library::Library, write: &Write, playlist: i64, server: &str, failure: &Failure) -> Written {
    if !matches!(failure, Failure::Gone(_)) {
        return refused(write, chilpa_i18n::t_args("{} did not take the change to the playlist: {}", &[server, &failure.to_string()]));
    }
    let _ = lib.delete_playlist(playlist);
    let why = if matches!(write.change, Change::Delete) { String::new() } else { chilpa_i18n::t_args("{} no longer has this playlist; it left your playlists here too.", &[server]) };
    Written { playlist: 0, ..refused(write, why) }
}

/// The change in the protocol's words: a track by its id on the server, a
/// row by its place among the entries read (`places[row]`).
fn protocol(stored: &chilpa_library::StoredServer, as_read: &PlaylistTracks, places: &[usize], change: &Change) -> Option<chilpa_servers::Change> {
    let ids = |tracks: &[String]| -> Option<Vec<String>> { tracks.iter().map(|entry| server_id(stored, entry)).collect() };
    Some(match change {
        Change::Create { name, tracks, .. } => chilpa_servers::Change::Create { name: name.clone(), tracks: ids(tracks.as_slice())? },
        Change::Edit { name, description } => chilpa_servers::Change::Edit { name: name.clone(), description: description.clone() },
        Change::Delete => chilpa_servers::Change::Delete,
        Change::Add { tracks } => chilpa_servers::Change::Add { tracks: ids(tracks.as_slice())? },
        Change::Remove { rows } => chilpa_servers::Change::Remove { at: rows.iter().map(|&row| places.get(row).copied()).collect::<Option<_>>()? },
        Change::Reorder { rows } => {
            if rows.len() != places.len() {
                return None;
            }
            // The rows fill the places the library's rows have, in their new
            // order; an entry that is not a row here keeps its place.
            let mut order: Vec<usize> = (0..as_read.entries.len()).collect();
            for (slot, &row) in rows.iter().enumerate() {
                order[places[slot]] = *places.get(row)?;
            }
            chilpa_servers::Change::Reorder { order }
        }
    })
}

/// A track's id on its server, from its queue entry, when the entry is one
/// of that server's.
fn server_id(stored: &chilpa_library::StoredServer, entry: &str) -> Option<String> {
    let parsed = chilpa_models::RemoteEntry::parse(entry)?;
    (parsed.source.provider == stored.provider && parsed.source.instance == stored.instance).then_some(parsed.external_id)
}

#[cfg(test)]
mod tests;
