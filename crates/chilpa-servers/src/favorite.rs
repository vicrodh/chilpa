//! The listener's favorites (the heart), on the servers that keep them.
//!
//! What each protocol says, and this follows:
//!
//! - **OpenSubsonic** writes `starred`, the date it was starred, on a starred
//!   song and record. `getStarred2` lists every starred artist, record and
//!   song at once, without paging, optionally per music folder. `star` and
//!   `unstar` take `id` for a song and `albumId` for a record (ID3).
//! - **Jellyfin** keeps `IsFavorite` in a user's item data. `POST` and
//!   `DELETE /UserFavoriteItems/{itemId}?userId=` mark and unmark it; the
//!   items listing takes `IsFavorite=true`, per item type, paged.
//! - **Plex** keeps no favorite for music: its clients' like is the five-star
//!   rating, which goes through `rating`. Nothing is read or written here.
//! - **A media server** keeps none.

use serde_json::Value;
use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use crate::model::{Credential, Failure, Kind, Server};
use crate::net::{Net, Request};
use crate::rating::Rated;
use crate::text::{encode, id};
use crate::{jellyfin, subsonic};

/// How long a server has to take one change.
const WRITE_TIME: Duration = Duration::from_secs(15);

/// Items per page of a favorites listing.
const PAGE: u64 = 500;

impl Kind {
    /// Whether it keeps favorites Chilpa reads and writes.
    pub fn keeps_favorites(self) -> bool {
        matches!(self, Kind::Subsonic | Kind::Jellyfin)
    }
}

/// What a server holds as favorites: the ids of its tracks and records.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Favorites {
    pub tracks: BTreeSet<String>,
    pub albums: BTreeSet<String>,
    /// Every favorite of the chosen libraries was listed. When not, what
    /// was listed counts and nothing else is taken for not a favorite.
    pub whole: bool,
}

/// The server's favorites, at most `pages` requests and until `until`.
pub fn favorites(net: &dyn Net, server: &Server, key: &Credential, pages: usize, until: Instant) -> Result<Favorites, Failure> {
    let mut found = Favorites { whole: true, ..Favorites::default() };
    match server.kind {
        Kind::Subsonic => {
            let folders: Vec<String> = if server.libraries.is_empty() {
                vec![String::new()]
            } else {
                server.libraries.iter().map(|f| format!("&musicFolderId={}", encode(f))).collect()
            };
            for (asked, folder) in folders.iter().enumerate() {
                if asked >= pages || Instant::now() >= until {
                    found.whole = false;
                    break;
                }
                let url = subsonic::url(&server.address, "getStarred2", Some(key), folder);
                let answer = subsonic::send(net, Request::get(url, Vec::new()), subsonic::error_of)?;
                let starred = answer.get("starred2").cloned().unwrap_or(Value::Null);
                let ids = |field: &str| starred.get(field).and_then(Value::as_array).into_iter().flatten().filter_map(|item| id(item, "id")).collect::<Vec<_>>();
                found.tracks.extend(ids("song"));
                found.albums.extend(ids("album"));
            }
        }
        Kind::Jellyfin => {
            let mut asked = 0;
            'kinds: for (kind, album) in [("Audio", false), ("MusicAlbum", true)] {
                let mut offset: u64 = 0;
                loop {
                    if asked >= pages || Instant::now() >= until {
                        found.whole = false;
                        break 'kinds;
                    }
                    asked += 1;
                    let url = format!(
                        "{}/Items?userId={}&IncludeItemTypes={kind}&Recursive=true&IsFavorite=true&StartIndex={offset}&Limit={PAGE}\
                         &EnableImages=false&EnableUserData=false&EnableTotalRecordCount=true",
                        server.address,
                        encode(&server.details.user_id)
                    );
                    let answer = jellyfin::ask(net, Request::get(url, jellyfin::headers(&server.client, Some(key))))?;
                    let items = answer.get("Items").and_then(Value::as_array).cloned().unwrap_or_default();
                    let into = if album { &mut found.albums } else { &mut found.tracks };
                    into.extend(items.iter().filter_map(|item| id(item, "Id")));
                    offset += items.len() as u64;
                    let total = answer.get("TotalRecordCount").and_then(Value::as_u64);
                    if items.is_empty() || total.is_none_or(|total| offset >= total) {
                        break;
                    }
                }
            }
        }
        Kind::Plex | Kind::Dlna => {}
    }
    Ok(found)
}

/// Marks one track or record as a favorite on the server, or unmarks it.
pub fn favor(net: &dyn Net, server: &Server, key: &Credential, rated: Rated, id: &str, on: bool) -> Result<(), Failure> {
    match server.kind {
        Kind::Subsonic => {
            let field = if rated == Rated::Album { "albumId" } else { "id" };
            let url = subsonic::url(&server.address, if on { "star" } else { "unstar" }, Some(key), &format!("&{field}={}", encode(id)));
            let mut request = Request::get(url, Vec::new());
            request.timeout = WRITE_TIME;
            subsonic::send(net, request, refused).map(|_| ())
        }
        Kind::Jellyfin => {
            let url = format!("{}/UserFavoriteItems/{}?userId={}", server.address, encode(id), encode(&server.details.user_id));
            let headers = jellyfin::headers(&server.client, Some(key));
            let mut request = if on { Request::post(url, headers, None) } else { Request::delete(url, headers) };
            request.timeout = WRITE_TIME;
            let reply = net.send(&request).map_err(Failure::Unreachable)?;
            if reply.status == 404 {
                return Err(Failure::Gone(chilpa_i18n::t("the server no longer has it")));
            }
            jellyfin::read(&reply).map(|_| ())
        }
        Kind::Plex | Kind::Dlna => Err(Failure::Unexpected(chilpa_i18n::t("this server keeps no favorites Chilpa can change"))),
    }
}

/// A Subsonic error to a favorite: an id the server no longer has is gone.
fn refused(error: &Value) -> Failure {
    match error.get("code").and_then(Value::as_i64) {
        Some(70) => Failure::Gone(chilpa_i18n::t("the server no longer has it")),
        _ => subsonic::error_of(error),
    }
}

/// Reads back whether the server holds one track or record as a favorite.
pub fn is_favorite(net: &dyn Net, server: &Server, key: &Credential, rated: Rated, id: &str) -> Result<bool, Failure> {
    match server.kind {
        Kind::Subsonic => {
            let (endpoint, field) = if rated == Rated::Album { ("getAlbum", "album") } else { ("getSong", "song") };
            let url = subsonic::url(&server.address, endpoint, Some(key), &format!("&id={}", encode(id)));
            let found = subsonic::send(net, Request::get(url, Vec::new()), refused)?;
            Ok(found.get(field).and_then(|item| item.get("starred")).is_some_and(|starred| !starred.is_null()))
        }
        Kind::Jellyfin => {
            let url = format!("{}/UserItems/{}/UserData?userId={}", server.address, encode(id), encode(&server.details.user_id));
            let answer = jellyfin::ask(net, Request::get(url, jellyfin::headers(&server.client, Some(key))))?;
            Ok(answer.get("IsFavorite").and_then(Value::as_bool) == Some(true))
        }
        Kind::Plex | Kind::Dlna => Err(Failure::Unexpected(chilpa_i18n::t("this server keeps no favorites Chilpa can change"))),
    }
}

#[cfg(test)]
mod tests;
