//! An OpenSubsonic server's playlists (`getPlaylists`, `getPlaylist`,
//! `createPlaylist`, `updatePlaylist` and `deletePlaylist`, as the
//! OpenSubsonic reference describes them, read on 2026-11-03):
//!
//! - **The user's own and the public ones are listed, and only the owner
//!   may change one** ("Only the owner of a playlist is allowed to update
//!   it"). The listing says whose each is. Its name and its comment, the
//!   description, are changed with `updatePlaylist`.
//! - **An entry is its place.** Taking one out names its index, all in one
//!   request: a second would find the places moved. A new order is the
//!   whole list written again, `createPlaylist` with the playlist's id.
//! - **A long list goes in the body** when the server announces
//!   `formPost`; otherwise in the address, and one that does not fit there
//!   is refused before it goes out.

use serde_json::Value;

use super::{error_of, not_subsonic, send, url};
use crate::model::{Credential, Failure, Server};
use crate::net::{Net, Request};
use crate::playlist::{is_order, misfit, too_long, Change, Entry, Playlist, PlaylistListing, PlaylistTracks, IDS_PER_REQUEST, PLAYLISTS, PLAYLIST_TRACKS};
use crate::text::{encode, id, string};

/// The longest address sent: past it, servers and the proxies before them
/// start to refuse.
const LONGEST: usize = 8_000;

/// What an error code means for a playlist: 50 is "not yours to change",
/// 70 "not there".
fn playlist_error(error: &Value) -> Failure {
    match error.get("code").and_then(Value::as_i64) {
        Some(50) => Failure::Forbidden(chilpa_i18n::t("this user may not do that on the server")),
        Some(70) => Failure::Gone(chilpa_i18n::t("the server no longer has it")),
        _ => error_of(error),
    }
}

fn takes_forms(server: &Server) -> bool {
    server.details.extensions.iter().any(|e| e == "formPost")
}

/// A request to an endpoint with its parameters (`&name=value...`): in the
/// body when the server takes a form, in the address otherwise.
fn request(server: &Server, key: &Credential, endpoint: &str, params: &str) -> Result<Request, Failure> {
    let base = url(&server.address, endpoint, Some(key), "");
    if takes_forms(server) {
        let form = params.trim_start_matches('&').as_bytes().to_vec();
        return Ok(Request::post(base, Vec::new(), Some(("application/x-www-form-urlencoded".into(), form))));
    }
    let whole = format!("{base}{params}");
    if whole.len() > LONGEST {
        return Err(Failure::Unexpected(chilpa_i18n::t("the server takes no list this long in one request")));
    }
    Ok(Request::get(whole, Vec::new()))
}

/// One parameter for each value.
fn each(name: &str, values: &[String]) -> String {
    values.iter().map(|value| format!("&{name}={}", encode(value))).collect()
}

/// The playlists the user can read: their own and the public ones. Only
/// `account`'s own may be changed; without an account to tell, the server
/// is left to say.
pub fn playlists(net: &dyn Net, server: &Server, key: &Credential, account: &str) -> Result<PlaylistListing, Failure> {
    let found = send(net, Request::get(url(&server.address, "getPlaylists", Some(key), ""), Vec::new()), playlist_error)?;
    let Some(list) = found.get("playlists") else { return Err(not_subsonic(200)) };
    let all = list.get("playlist").and_then(Value::as_array).cloned().unwrap_or_default();
    let mine = |p: &Value| account.is_empty() || string(p, "owner").is_none_or(|owner| owner.eq_ignore_ascii_case(account));
    let found: Vec<Playlist> = all
        .iter()
        .filter_map(|p| Some(Playlist { id: id(p, "id")?, name: string(p, "name").unwrap_or_default(), description: string(p, "comment").unwrap_or_default(), editable: Some(mine(p)) }))
        .collect();
    // One without an id cannot be told apart from one deleted.
    let whole = found.len() == all.len() && found.len() <= PLAYLISTS;
    Ok(PlaylistListing { found: found.into_iter().take(PLAYLISTS).collect(), whole })
}

/// The entries of one playlist, in its order. A video, an episode or an
/// audiobook in it keeps its place and is not a track.
pub fn playlist_tracks(net: &dyn Net, server: &Server, key: &Credential, playlist: &str) -> Result<PlaylistTracks, Failure> {
    let asked = url(&server.address, "getPlaylist", Some(key), &format!("&id={}", encode(playlist)));
    let found = send(net, Request::get(asked, Vec::new()), playlist_error)?;
    // An answer without its playlist is not an empty one.
    let Some(list) = found.get("playlist") else { return Err(not_subsonic(200)) };
    let mut tracks = PlaylistTracks::default();
    for entry in list.get("entry").and_then(Value::as_array).into_iter().flatten() {
        let other = entry.get("isVideo").and_then(Value::as_bool) == Some(true) || matches!(string(entry, "type").as_deref(), Some("video" | "podcast" | "audiobook"));
        let item = id(entry, "id").unwrap_or_default();
        tracks.entries.push(Entry { track: !other && !item.is_empty(), id: item, entry: String::new() });
    }
    tracks.whole = tracks.entries.len() <= PLAYLIST_TRACKS;
    tracks.entries.truncate(PLAYLIST_TRACKS);
    Ok(tracks)
}

/// Asks the server for one change of `playlist`, read just before as
/// `as_read`. Returns the id of a playlist it created, when it says it:
/// before 1.14 the answer carries nothing.
pub fn change_playlist(net: &dyn Net, server: &Server, key: &Credential, playlist: &str, as_read: &PlaylistTracks, change: &Change) -> Result<Option<String>, Failure> {
    let ask = |endpoint: &str, params: String| send(net, request(server, key, endpoint, &params)?, playlist_error);
    let list = format!("&playlistId={}", encode(playlist));
    match change {
        Change::Create { name, tracks } => {
            let made = ask("createPlaylist", format!("&name={}{}", encode(name), each("songId", tracks)))?;
            return Ok(made.get("playlist").and_then(|p| id(p, "id")));
        }
        Change::Edit { name, description } => {
            let name = name.as_deref().map(|name| format!("&name={}", encode(name))).unwrap_or_default();
            let comment = description.as_deref().map(|text| format!("&comment={}", encode(text))).unwrap_or_default();
            if !name.is_empty() || !comment.is_empty() {
                ask("updatePlaylist", format!("{list}{name}{comment}"))?;
            }
        }
        Change::Delete => {
            ask("deletePlaylist", format!("&id={}", encode(playlist)))?;
        }
        Change::Add { tracks } => {
            // In a body, all at once; in an address, a hundred at a time.
            let size = if takes_forms(server) { tracks.len().max(1) } else { IDS_PER_REQUEST };
            for chunk in tracks.chunks(size) {
                ask("updatePlaylist", format!("{list}{}", each("songIdToAdd", chunk)))?;
            }
        }
        Change::Remove { at } => {
            if at.iter().any(|&place| place >= as_read.entries.len()) {
                return Err(misfit());
            }
            let places: Vec<String> = at.iter().map(|place| place.to_string()).collect();
            ask("updatePlaylist", format!("{list}{}", each("songIndexToRemove", &places)))?;
        }
        Change::Reorder { order } => {
            if !as_read.whole {
                return Err(too_long());
            }
            if !is_order(order, as_read.entries.len()) {
                return Err(misfit());
            }
            let ids: Vec<String> = order.iter().map(|&place| as_read.entries[place].id.clone()).collect();
            ask("createPlaylist", format!("{list}{}", each("songId", &ids)))?;
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests;
