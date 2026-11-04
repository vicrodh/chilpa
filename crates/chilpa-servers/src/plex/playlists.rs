//! A Plex server's playlists, as python-plexapi's `Playlist` asks for them
//! (read on 2026-11-03; no official reference was found):
//!
//! - **Audio playlists are `/playlists?playlistType=audio`**, smart ones
//!   included, and their entries `/playlists/{ratingKey}/items`, paged like
//!   a section.
//! - **An entry has an id of its own, `playlistItemID`**, and that is what
//!   is taken out or moved. A move goes right after another entry, or to
//!   the top without one.
//! - **A smart playlist is the server's to compute**: read, never changed.
//! - **Its name and its description are `title` and `summary`** of the
//!   playlist itself.
//! - **Tracks are named by a URI of the server**,
//!   `server://{machineIdentifier}/com.plexapp.plugins.library/library/metadata/{ratingKeys}`.

use serde_json::Value;

use super::{headers, not_plex, read, server_token, PAGE};
use crate::model::{Credential, Failure, Server};
use crate::net::{Net, Request};
use crate::playlist::{is_order, misfit, moves, too_long, Change, Entry, Playlist, PlaylistListing, PlaylistTracks, IDS_PER_REQUEST, PLAYLISTS, PLAYLIST_TRACKS};
use crate::text::{encode, id, string};

/// What a request about one playlist answered. A refusal here is about the
/// playlist, not the session: 403 is "not yours to change", 404 "not there".
fn answer(net: &dyn Net, request: Request) -> Result<Value, Failure> {
    let reply = net.send(&request).map_err(Failure::Unreachable)?;
    match reply.status {
        403 => Err(Failure::Forbidden(chilpa_i18n::t("this user may not do that on the server"))),
        404 => Err(Failure::Gone(chilpa_i18n::t("the server no longer has it"))),
        _ => read(&reply),
    }
}

/// A request for one page of a list, from `offset` on.
fn paged(server: &Server, key: &Credential, url: String, offset: u64) -> Request {
    let mut headers = headers(&server.client, server_token(key));
    headers.push(("X-Plex-Container-Start".into(), offset.to_string()));
    headers.push(("X-Plex-Container-Size".into(), PAGE.to_string()));
    Request::get(url, headers)
}

/// These tracks as the server names them in a write, ready for a query.
fn uri(server: &Server, tracks: &[String]) -> String {
    let keys: Vec<String> = tracks.iter().map(|t| encode(t)).collect();
    encode(&format!("server://{}/com.plexapp.plugins.library/library/metadata/{}", server.details.machine, keys.join(",")))
}

/// The audio playlists of the server, smart ones too: their tracks are read
/// as the server computes them, and only the others may be changed.
pub fn playlists(net: &dyn Net, server: &Server, key: &Credential) -> Result<PlaylistListing, Failure> {
    let mut listing = PlaylistListing::default();
    let mut offset: u64 = 0;
    loop {
        let found = super::ask(net, paged(server, key, format!("{}/playlists?playlistType=audio", server.address), offset))?;
        let container = found.get("MediaContainer").cloned().unwrap_or(Value::Null);
        let items = container.get("Metadata").and_then(Value::as_array).cloned().unwrap_or_default();
        let total = container.get("totalSize").and_then(Value::as_u64);
        for item in &items {
            if string(item, "playlistType").is_some_and(|kind| kind != "audio") {
                continue;
            }
            if let Some(rating) = id(item, "ratingKey") {
                let smart = item.get("smart").and_then(Value::as_bool) == Some(true);
                let (name, description) = (string(item, "title").unwrap_or_default(), string(item, "summary").unwrap_or_default());
                listing.found.push(Playlist { id: rating, name, description, editable: Some(!smart) });
            }
        }
        offset += items.len() as u64;
        // Without `totalSize`, as in `page`, a short page is the last, and
        // the list is not taken as whole: no playlist is taken for gone.
        let more = match total {
            Some(t) => offset < t,
            None => items.len() as u64 >= PAGE,
        };
        if items.is_empty() || !more || listing.found.len() >= PLAYLISTS {
            listing.whole = total.is_some_and(|t| offset >= t) && listing.found.len() <= PLAYLISTS;
            listing.found.truncate(PLAYLISTS);
            return Ok(listing);
        }
    }
}

/// The entries of one playlist, in its order. A clip or a photo in it keeps
/// its place and is not a track.
pub fn playlist_tracks(net: &dyn Net, server: &Server, key: &Credential, playlist: &str) -> Result<PlaylistTracks, Failure> {
    let mut tracks = PlaylistTracks::default();
    let mut offset: u64 = 0;
    loop {
        let found = answer(net, paged(server, key, format!("{}/playlists/{}/items", server.address, encode(playlist)), offset))?;
        let container = found.get("MediaContainer").cloned().unwrap_or(Value::Null);
        let items = container.get("Metadata").and_then(Value::as_array).cloned().unwrap_or_default();
        let total = container.get("totalSize").and_then(Value::as_u64);
        for item in &items {
            let rating = id(item, "ratingKey").unwrap_or_default();
            let track = !rating.is_empty() && string(item, "type").as_deref() == Some("track");
            tracks.entries.push(Entry { id: rating, entry: id(item, "playlistItemID").unwrap_or_default(), track });
        }
        offset += items.len() as u64;
        let more = match total {
            Some(t) => offset < t,
            None => items.len() as u64 >= PAGE,
        };
        if items.is_empty() || !more || tracks.entries.len() >= PLAYLIST_TRACKS {
            tracks.whole = !more && tracks.entries.len() <= PLAYLIST_TRACKS;
            tracks.entries.truncate(PLAYLIST_TRACKS);
            return Ok(tracks);
        }
    }
}

/// Asks the server for one change of `playlist`, read just before as
/// `as_read`. Returns the id of a playlist it created.
pub fn change_playlist(net: &dyn Net, server: &Server, key: &Credential, playlist: &str, as_read: &PlaylistTracks, change: &Change) -> Result<Option<String>, Failure> {
    let head = || headers(&server.client, server_token(key));
    let list = format!("{}/playlists/{}", server.address, encode(playlist));
    // An entry by its own id, which a move and a removal name.
    let entry = |place: usize| as_read.entries.get(place).map(|e| encode(&e.entry)).filter(|e| !e.is_empty()).ok_or_else(misfit);
    match change {
        Change::Create { name, tracks } => {
            let (first, rest) = tracks.split_at(tracks.len().min(IDS_PER_REQUEST));
            let with = if first.is_empty() { String::new() } else { format!("&uri={}", uri(server, first)) };
            let asked = format!("{}/playlists?type=audio&title={}&smart=0{with}", server.address, encode(name));
            let made = answer(net, Request::post(asked, head(), None))?;
            let created = made.get("MediaContainer").and_then(|c| c.get("Metadata")).and_then(Value::as_array).and_then(|m| m.first()).and_then(|p| id(p, "ratingKey"));
            let created = created.ok_or_else(|| not_plex(200))?;
            for chunk in rest.chunks(IDS_PER_REQUEST) {
                answer(net, Request::put(format!("{}/playlists/{}/items?uri={}", server.address, encode(&created), uri(server, chunk)), head(), None))?;
            }
            return Ok(Some(created));
        }
        Change::Edit { name, description } => {
            let mut said: Vec<String> = Vec::new();
            if let Some(name) = name {
                said.push(format!("title={}", encode(name)));
            }
            if let Some(text) = description {
                said.push(format!("summary={}", encode(text)));
            }
            if !said.is_empty() {
                answer(net, Request::put(format!("{list}?{}", said.join("&")), head(), None))?;
            }
        }
        Change::Delete => {
            answer(net, Request::delete(list.clone(), head()))?;
        }
        Change::Add { tracks } => {
            for chunk in tracks.chunks(IDS_PER_REQUEST) {
                answer(net, Request::put(format!("{list}/items?uri={}", uri(server, chunk)), head(), None))?;
            }
        }
        Change::Remove { at } => {
            let entries: Vec<String> = at.iter().map(|&place| entry(place)).collect::<Result<_, _>>()?;
            for one in entries {
                answer(net, Request::delete(format!("{list}/items/{one}"), head()))?;
            }
        }
        Change::Reorder { order } => {
            if !as_read.whole {
                return Err(too_long());
            }
            if !is_order(order, as_read.entries.len()) {
                return Err(misfit());
            }
            let steps = moves(order);
            // Every entry it names, checked before the first move goes out.
            for step in &steps {
                entry(step.item)?;
                if let Some(after) = step.after {
                    entry(after)?;
                }
            }
            for step in steps {
                let after = match step.after {
                    Some(after) => format!("?after={}", entry(after)?),
                    None => String::new(),
                };
                answer(net, Request::put(format!("{list}/items/{}/move{after}", entry(step.item)?), head(), None))?;
            }
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests;
