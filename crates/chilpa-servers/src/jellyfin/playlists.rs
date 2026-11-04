//! A Jellyfin server's playlists, as 10.10.7 serves them
//! (`PlaylistsController` and `PlaylistManager`, read on 2026-11-03):
//!
//! - **A playlist is an item of type `Playlist`**, listed by `/Items` like
//!   the tracks. Its entries are `/Playlists/{id}/Items`, in its order,
//!   already filtered to what the user may see, with a `TotalRecordCount`
//!   to page by.
//! - **An entry is known by its item's id.** `PlaylistItemId` is the
//!   linked item's id: taking an entry out takes every entry of that
//!   track, and a move moves its first one. So one copy of a repeated track
//!   is not taken out alone, and a playlist that holds a track twice is not
//!   reordered here.
//! - **10.10 adds a track the playlist already holds only once**; later
//!   versions add it again.
//! - **Its owner may change it, and a user it is shared with to edit**:
//!   `/Playlists/{id}/Users/{user}` says which (`CanEdit`). Only the owner
//!   or an administrator may delete it (`Playlist.CanDelete`).
//! - **Its name is changed, never its description**: `UpdatePlaylistDto`
//!   has none, and `/Items/{id}` takes the whole item from administrators
//!   only (`ItemUpdateController`, `RequiresElevation`).

use serde_json::{json, Value};

use super::{headers, not_jellyfin, read, PAGE};
use crate::model::{Credential, Failure, Server};
use crate::net::{Net, Request};
use crate::playlist::{is_order, misfit, moves, too_long, Change, Entry, Playlist, PlaylistListing, PlaylistTracks, IDS_PER_REQUEST, PLAYLISTS, PLAYLIST_TRACKS};
use crate::text::{encode, string};

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

/// The playlists the user can read, by name. One of videos, photos or books
/// is left out; one that says nothing of its media, an empty one, stays.
pub fn playlists(net: &dyn Net, server: &Server, key: &Credential) -> Result<PlaylistListing, Failure> {
    let mut listing = PlaylistListing::default();
    let mut offset: u64 = 0;
    loop {
        let url = format!(
            "{}/Items?userId={}&IncludeItemTypes=Playlist&Recursive=true&StartIndex={offset}&Limit={PAGE}\
             &SortBy=SortName&SortOrder=Ascending&Fields=Overview&EnableImages=false&EnableUserData=false&EnableTotalRecordCount=true",
            server.address,
            encode(&server.details.user_id)
        );
        let found = super::ask(net, Request::get(url, headers(&server.client, Some(key))))?;
        let items = found.get("Items").and_then(Value::as_array).cloned().unwrap_or_default();
        let total = found.get("TotalRecordCount").and_then(Value::as_u64);
        for item in &items {
            if matches!(string(item, "MediaType").as_deref(), Some("Video" | "Photo" | "Book")) {
                continue;
            }
            if let Some(id) = string(item, "Id") {
                let (name, description) = (string(item, "Name").unwrap_or_default(), string(item, "Overview").unwrap_or_default());
                listing.found.push(Playlist { id, name, description, editable: None });
            }
        }
        offset += items.len() as u64;
        let more = total.is_some_and(|t| offset < t);
        if items.is_empty() || !more || listing.found.len() >= PLAYLISTS {
            // Without a total the end cannot be told from a cut.
            listing.whole = total.is_some_and(|t| offset >= t) && listing.found.len() <= PLAYLISTS;
            listing.found.truncate(PLAYLISTS);
            return Ok(listing);
        }
    }
}

/// The entries of one playlist, in its order. One that is not audio, a
/// music video, is kept in its place and is not a track.
pub fn playlist_tracks(net: &dyn Net, server: &Server, key: &Credential, id: &str) -> Result<PlaylistTracks, Failure> {
    let mut tracks = PlaylistTracks::default();
    let mut offset: u64 = 0;
    loop {
        let url = format!("{}/Playlists/{}/Items?userId={}&StartIndex={offset}&Limit={PAGE}&EnableImages=false&EnableUserData=false", server.address, encode(id), encode(&server.details.user_id));
        let found = answer(net, Request::get(url, headers(&server.client, Some(key))))?;
        let items = found.get("Items").and_then(Value::as_array).cloned().unwrap_or_default();
        let total = found.get("TotalRecordCount").and_then(Value::as_u64);
        for item in &items {
            let item_id = string(item, "Id").unwrap_or_default();
            let entry = string(item, "PlaylistItemId").unwrap_or_else(|| item_id.clone());
            let track = !item_id.is_empty() && string(item, "Type").as_deref() == Some("Audio");
            tracks.entries.push(Entry { id: item_id, entry, track });
        }
        offset += items.len() as u64;
        let more = total.is_some_and(|t| offset < t);
        if items.is_empty() || !more || tracks.entries.len() >= PLAYLIST_TRACKS {
            tracks.whole = total.is_some_and(|t| offset >= t) && tracks.entries.len() <= PLAYLIST_TRACKS;
            tracks.entries.truncate(PLAYLIST_TRACKS);
            return Ok(tracks);
        }
    }
}

/// Whether the account may change that playlist: its owner may, and a
/// user it is shared with to edit. Anything else, or no answer, is no.
pub fn playlist_editable(net: &dyn Net, server: &Server, key: &Credential, id: &str) -> bool {
    let url = format!("{}/Playlists/{}/Users/{}", server.address, encode(id), encode(&server.details.user_id));
    let said = answer(net, Request::get(url, headers(&server.client, Some(key)))).ok();
    said.and_then(|v| v.get("CanEdit").and_then(Value::as_bool)) == Some(true)
}

/// Asks the server for one change of `playlist`, read just before as
/// `as_read`. Returns the id of a playlist it created.
pub fn change_playlist(net: &dyn Net, server: &Server, key: &Credential, playlist: &str, as_read: &PlaylistTracks, change: &Change) -> Result<Option<String>, Failure> {
    let url = |path: String| format!("{}{path}", server.address);
    let head = || headers(&server.client, Some(key));
    let body = |value: Value| Some(("application/json".to_string(), value.to_string().into_bytes()));
    let (list, user) = (encode(playlist), encode(&server.details.user_id));
    match change {
        Change::Create { name, tracks } => {
            let asked = json!({"Name": name, "Ids": tracks, "UserId": server.details.user_id, "MediaType": "Audio", "IsPublic": false});
            let made = answer(net, Request::post(url("/Playlists".into()), head(), body(asked)))?;
            return string(&made, "Id").map(Some).ok_or_else(|| not_jellyfin(200));
        }
        Change::Edit { name, description } => {
            // `UpdatePlaylistDto` has no description; the item's own update
            // takes the whole item and is for administrators.
            if description.is_some() {
                return Err(Failure::Unexpected(chilpa_i18n::t("this server does not take a playlist's description from Chilpa")));
            }
            if let Some(name) = name {
                answer(net, Request::post(url(format!("/Playlists/{list}")), head(), body(json!({"Name": name}))))?;
            }
        }
        Change::Delete => {
            // One the user may not delete is answered 401, as a session that
            // is no longer good would be (`LibraryController.DeleteItem`).
            let reply = net.send(&Request::delete(url(format!("/Items/{list}")), head())).map_err(Failure::Unreachable)?;
            match reply.status {
                401 | 403 => return Err(Failure::Forbidden(chilpa_i18n::t("this user may not do that on the server"))),
                404 => return Err(Failure::Gone(chilpa_i18n::t("the server no longer has it"))),
                _ => {
                    read(&reply)?;
                }
            }
        }
        Change::Add { tracks } => {
            for chunk in tracks.chunks(IDS_PER_REQUEST) {
                let ids: Vec<String> = chunk.iter().map(|id| encode(id)).collect();
                answer(net, Request::post(url(format!("/Playlists/{list}/Items?ids={}&userId={user}", ids.join(","))), head(), None))?;
            }
        }
        Change::Remove { at } => {
            let mut entries: Vec<String> = Vec::new();
            for &place in at {
                let entry = &as_read.entries.get(place).ok_or_else(misfit)?.entry;
                // An entry id takes out every copy of its track: one copy
                // alone is refused rather than taking the others too.
                let copies = as_read.entries.iter().filter(|e| &e.entry == entry).count();
                if at.iter().filter(|&&other| as_read.entries.get(other).is_some_and(|e| &e.entry == entry)).count() < copies {
                    return Err(Failure::Unexpected(chilpa_i18n::t("this server takes every copy of a track out of a playlist at once; take them all out, or change it on the server")));
                }
                let entry = encode(entry);
                if !entries.contains(&entry) {
                    entries.push(entry);
                }
            }
            for chunk in entries.chunks(IDS_PER_REQUEST) {
                answer(net, Request::delete(url(format!("/Playlists/{list}/Items?entryIds={}", chunk.join(","))), head()))?;
            }
        }
        Change::Reorder { order } => {
            if !as_read.whole {
                return Err(too_long());
            }
            if !is_order(order, as_read.entries.len()) {
                return Err(misfit());
            }
            let mut seen = std::collections::HashSet::new();
            if !as_read.entries.iter().all(|e| seen.insert(e.entry.as_str())) {
                return Err(Failure::Unexpected(chilpa_i18n::t("this server cannot move a track that is in the playlist more than once")));
            }
            for step in moves(order) {
                let entry = encode(&as_read.entries[step.item].entry);
                answer(net, Request::post(url(format!("/Playlists/{list}/Items/{entry}/Move/{}", step.to)), head(), None))?;
            }
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests;
