//! A server's own playlists coming into the library, on the sync's thread
//! (ADR-017, as amended for them).
//!
//! The server is the source of truth. Each playlist is written as the
//! server has it, whole or not at all; one the server no longer lists
//! leaves only when its whole list was read; one that cannot be read keeps
//! what it had. Only what the library already holds goes in a playlist: an
//! entry whose track it does not hold is counted, and a reading of the
//! playlists alone that finds one asks for the catalogue before counting
//! it as left out (`sync::asks_for_catalogue`).

use super::keys::Keys;
use super::net;
use super::sync::Event;
use chilpa_servers::{Failure, Net, Playlist, PlaylistTracks};
use std::sync::Mutex;

/// Reads the server's playlists into the library. `gate` is held for the
/// whole reading: a change of a playlist waits for it, and never lands in
/// the middle of it. One playlist's entries are held at a time, at most
/// `PLAYLIST_TRACKS`.
pub(crate) fn read(lib: &chilpa_library::Library, net: &dyn Net, keys: &Keys, source: i64, gate: &Mutex<()>, stopped: &dyn Fn() -> bool) -> Event {
    let failed = |why: String| Event::PlaylistsFailed { source, why: chilpa_i18n::t_args("Its playlists could not be read: {}.", &[&why]) };
    let Some(stored) = lib.server(source).ok().flatten() else {
        return failed(chilpa_i18n::t("that server is no longer in the library"));
    };
    if !stored.enabled {
        return failed(chilpa_i18n::t("that server is disabled"));
    }
    let (Some(server), Some(key)) = (net::server(&stored), keys.get(&stored.instance)) else {
        return failed(chilpa_i18n::t_args("sign in to {} again", &[&stored.label]));
    };
    let _reading = gate.lock().unwrap_or_else(|e| e.into_inner());
    let listing = match chilpa_servers::playlists(net, &server, &key, &stored.account) {
        Ok(listing) => listing,
        Err(failure) => return failed(failure.to_string()),
    };
    let (mut kept, mut left_out) = (0, 0);
    for playlist in &listing.found {
        if stopped() {
            return failed(chilpa_i18n::t("the sync was stopped"));
        }
        let read = match chilpa_servers::playlist_tracks(net, &server, &key, &playlist.id) {
            // Only longer than what is read: its first entries are written.
            Ok(read) if read.whole || read.entries.len() >= chilpa_servers::PLAYLIST_TRACKS => read,
            // Cut short: it keeps what it had.
            Ok(_) => continue,
            Err(failure) if stops(&failure) => return failed(failure.to_string()),
            // Gone since it was listed, or not this account's to read: it
            // keeps what it had until the listing says otherwise.
            Err(_) => continue,
        };
        let writable = playlist.editable.unwrap_or_else(|| chilpa_servers::playlist_editable(net, &server, &key, &playlist.id));
        match mirror(lib, &stored, playlist, writable, &read) {
            Some(out) => {
                kept += 1;
                left_out += out.left_out;
            }
            None => return failed(chilpa_i18n::t("the library could not be written")),
        }
    }
    if listing.whole {
        let listed: Vec<String> = listing.found.iter().map(|p| p.id.clone()).collect();
        if lib.prune_server_playlists(source, &listed).is_err() {
            return failed(chilpa_i18n::t("the library could not be written"));
        }
    }
    Event::Playlists { source, kept, left_out }
}

/// A failure of the server, not of one playlist: it stopped answering,
/// refused the session, moved or asks to wait. Nothing more is asked of it.
pub(crate) fn stops(failure: &Failure) -> bool {
    failure.is_unreachable() || matches!(failure, Failure::Refused(_) | Failure::Moved(_) | Failure::Busy(_))
}

/// Writes one playlist as the server just gave it: its tracks are the
/// queue entries of the server's tracks, and what is not a track is not in
/// it.
pub(crate) fn mirror(lib: &chilpa_library::Library, stored: &chilpa_library::StoredServer, playlist: &Playlist, writable: bool, read: &PlaylistTracks) -> Option<chilpa_library::MirroredPlaylist> {
    let mirrored = chilpa_library::ServerPlaylist {
        external_id: playlist.id.clone(),
        name: playlist.name.clone(),
        description: playlist.description.clone(),
        writable,
        entries: read.tracks().iter().map(|id| net::entry(stored, id)).collect(),
    };
    lib.mirror_server_playlist(stored.source_id, &mirrored).ok()
}
