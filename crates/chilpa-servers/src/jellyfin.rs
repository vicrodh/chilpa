//! Jellyfin, 10.10 and after, 12.x included (ADR-017, CHD-0060 §2).
//!
//! What the research found and this follows:
//!
//! - **One header carries everything:** `Authorization: MediaBrowser`
//!   with the client, the device, its id and the token. Jellyfin 12 turns
//!   the older ones (`X-Emby-Token`, `api_key`) off by default.
//! - **One session per device id.** Signing in again with the same id
//!   ends the previous session, so each server gets an id of its own and
//!   keeps it.
//! - **The listing does not carry the quality.** `MediaSources` costs about
//!   ten times the rest, so it is asked only for tracks whose `Etag`
//!   changed. A resync of an unchanged library pays nothing for it.
//! - **Paging has no tie-breaker** by id: the caller counts what it saw
//!   against the total before it takes anything out.
//! - **The MBID is `MusicBrainzRecording`** (measured on 12.1).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::{extension_of, image, Client, Credential, Failure, Kind, Library, Page, Probe, Server, Stream, Track};
use crate::net::{Net, Request};
use crate::text::{artist_line, encode, number, string, unix_time};

/// Tracks per page.
const PAGE: u64 = 250;

const LISTED: &str = "Etag,ProviderIds,Genres,DateCreated,ParentId";

fn authorization(client: &Client, token: Option<&str>) -> String {
    let mut header = format!(
        "MediaBrowser Client=\"{}\", Device=\"{}\", DeviceId=\"{}\", Version=\"{}\"",
        Client::PRODUCT,
        encode(&client.device),
        encode(&client.id),
        encode(&client.version)
    );
    if let Some(token) = token {
        header.push_str(&format!(", Token=\"{}\"", encode(token)));
    }
    header
}

fn headers(client: &Client, key: Option<&Credential>) -> Vec<(String, String)> {
    let token = match key {
        Some(Credential::Token { token }) => Some(token.as_str()),
        _ => None,
    };
    vec![("Authorization".into(), authorization(client, token)), ("Accept".into(), "application/json".into())]
}

fn ask(net: &dyn Net, request: Request) -> Result<Value, Failure> {
    let reply = net.send(&request).map_err(Failure::Unreachable)?;
    match reply.status {
        200..=299 if reply.body.is_empty() => Ok(Value::Null),
        200..=299 => serde_json::from_slice(&reply.body).map_err(|_| not_jellyfin(reply.status)),
        300..=399 => Err(Failure::moved(reply.headers.get("location").map(String::as_str).unwrap_or(""))),
        401 | 403 => Err(Failure::Refused(chilpa_i18n::t("the server did not accept the session; sign in again"))),
        503 => Err(Failure::Busy(chilpa_i18n::t("the server is starting; try again in a moment"))),
        status => Err(not_jellyfin(status)),
    }
}

fn not_jellyfin(status: u16) -> Failure {
    Failure::Unexpected(chilpa_i18n::t_args("that address does not answer like a Jellyfin server (HTTP {})", &[&status.to_string()]))
}

/// What the server says of itself, before signing in.
pub fn probe(net: &dyn Net, address: &str) -> Result<Probe, Failure> {
    let info = ask(net, Request::get(format!("{address}/System/Info/Public"), Vec::new()))?;
    let product_name = string(&info, "ProductName").unwrap_or_default();
    if product_name.to_ascii_lowercase().contains("emby") {
        return Err(Failure::Unexpected(chilpa_i18n::t("that is an Emby server, and Chilpa does not speak Emby")));
    }
    let identity = string(&info, "Id").ok_or_else(|| not_jellyfin(200))?;
    let quick_connect = ask(net, Request::get(format!("{address}/QuickConnect/Enabled"), Vec::new()))
        .map(|v| v.as_bool() == Some(true))
        .unwrap_or(false);
    Ok(Probe {
        product: format!("Jellyfin {}", string(&info, "Version").unwrap_or_default()).trim().to_string(),
        name: string(&info, "ServerName").unwrap_or_default(),
        identity,
        quick_connect,
        ..Probe::default()
    })
}

/// What signing in gives.
pub struct SignedIn {
    pub key: Credential,
    pub user_id: String,
    pub account: String,
}

fn signed_in(answer: &Value) -> Result<SignedIn, Failure> {
    let token = string(answer, "AccessToken").ok_or_else(|| not_jellyfin(200))?;
    let user = answer.get("User").cloned().unwrap_or(Value::Null);
    Ok(SignedIn {
        key: Credential::Token { token },
        user_id: string(&user, "Id").unwrap_or_default(),
        account: string(&user, "Name").unwrap_or_default(),
    })
}

/// Signs in with a user and a password. Only the token is kept.
pub fn sign_in(net: &dyn Net, address: &str, client: &Client, user: &str, password: &str) -> Result<SignedIn, Failure> {
    let body = serde_json::json!({ "Username": user.trim(), "Pw": password }).to_string().into_bytes();
    let request = Request::post(format!("{address}/Users/AuthenticateByName"), headers(client, None), Some(("application/json".into(), body)));
    match ask(net, request) {
        Err(Failure::Refused(_)) => Err(Failure::Refused(chilpa_i18n::t("the server did not accept the user or the password"))),
        other => signed_in(&other?),
    }
}

/// Starts Quick Connect: the code to type in a Jellyfin that is already
/// signed in, and the secret to ask with. The password never passes
/// through Chilpa.
pub fn quick_connect_start(net: &dyn Net, address: &str, client: &Client) -> Result<(String, String), Failure> {
    let answer = ask(net, Request::post(format!("{address}/QuickConnect/Initiate"), headers(client, None), None))
        .map_err(|e| match e {
            Failure::Refused(_) => Failure::Refused(chilpa_i18n::t("Quick Connect is turned off on that server")),
            other => other,
        })?;
    let secret = string(&answer, "Secret").ok_or_else(|| not_jellyfin(200))?;
    let code = string(&answer, "Code").ok_or_else(|| not_jellyfin(200))?;
    Ok((secret, code))
}

/// Whether the code was accepted on the other side yet.
pub fn quick_connect_ready(net: &dyn Net, address: &str, secret: &str) -> Result<bool, Failure> {
    let reply = net
        .send(&Request::get(format!("{address}/QuickConnect/Connect?secret={}", encode(secret)), Vec::new()))
        .map_err(Failure::Unreachable)?;
    match reply.status {
        200 => Ok(serde_json::from_slice::<Value>(&reply.body).ok().and_then(|v| v.get("Authenticated").and_then(Value::as_bool)) == Some(true)),
        404 => Err(Failure::Refused(chilpa_i18n::t("the code expired; start again"))),
        status => Err(not_jellyfin(status)),
    }
}

pub fn quick_connect_finish(net: &dyn Net, address: &str, client: &Client, secret: &str) -> Result<SignedIn, Failure> {
    let body = serde_json::json!({ "Secret": secret }).to_string().into_bytes();
    let request = Request::post(format!("{address}/Users/AuthenticateWithQuickConnect"), headers(client, None), Some(("application/json".into(), body)));
    signed_in(&ask(net, request)?)
}

pub fn ping(net: &dyn Net, server: &Server, key: &Credential) -> Result<(), Failure> {
    ask(net, Request::get(format!("{}/System/Info", server.address), headers(&server.client, Some(key)))).map(|_| ())
}

pub fn libraries(net: &dyn Net, server: &Server, key: &Credential) -> Result<Vec<Library>, Failure> {
    let url = format!("{}/UserViews?userId={}", server.address, encode(&server.details.user_id));
    let views = ask(net, Request::get(url, headers(&server.client, Some(key))))?;
    Ok(views
        .get("Items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|v| string(v, "CollectionType").as_deref() == Some("music"))
        .filter_map(|v| Some(Library { id: string(v, "Id")?, name: string(v, "Name").unwrap_or_default() }))
        .collect())
}

#[derive(Serialize, Deserialize, Default)]
struct Cursor {
    views: Vec<String>,
    view: usize,
    offset: u64,
}

pub fn page(net: &dyn Net, server: &Server, key: &Credential, cursor: Option<&str>) -> Result<Page, Failure> {
    let cursor = match cursor {
        Some(text) => serde_json::from_str(text).map_err(|_| Failure::Unexpected(chilpa_i18n::t("the sync lost its place; start it again")))?,
        None => {
            let views = if server.libraries.is_empty() {
                libraries(net, server, key)?.into_iter().map(|l| l.id).collect()
            } else {
                server.libraries.clone()
            };
            Cursor { views, view: 0, offset: 0 }
        }
    };
    let Some(view) = cursor.views.get(cursor.view).cloned() else {
        return Ok(Page::default());
    };
    let url = format!(
        "{}/Items?userId={}&parentId={}&IncludeItemTypes=Audio&Recursive=true&StartIndex={}&Limit={PAGE}\
         &SortBy=SortName,DateCreated&SortOrder=Ascending,Ascending&Fields={LISTED}\
         &EnableImages=true&ImageTypeLimit=1&EnableUserData=false&EnableTotalRecordCount=true",
        server.address,
        encode(&server.details.user_id),
        encode(&view),
        cursor.offset
    );
    let found = ask(net, Request::get(url, headers(&server.client, Some(key))))?;
    let items = found.get("Items").and_then(Value::as_array).cloned().unwrap_or_default();
    let total = found.get("TotalRecordCount").and_then(Value::as_u64);
    let tracks: Vec<Track> = items.iter().filter_map(track).collect();
    let partial = tracks.len() < items.len() || total.is_none();
    let read = cursor.offset + items.len() as u64;
    let next = if !items.is_empty() && total.is_some_and(|t| read < t) {
        Some(Cursor { offset: read, ..cursor })
    } else if cursor.view + 1 < cursor.views.len() {
        Some(Cursor { view: cursor.view + 1, offset: 0, ..cursor })
    } else {
        None
    };
    Ok(Page { tracks, next: next.map(|c| serde_json::to_string(&c).unwrap_or_default()), total, scope: view.clone(), partial })
}

pub fn details(net: &dyn Net, server: &Server, key: &Credential, ids: &[String]) -> Result<Vec<Track>, Failure> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<String> = ids.iter().map(|id| encode(id)).collect();
    let url = format!(
        "{}/Items?userId={}&Ids={}&Fields={LISTED},MediaSources&EnableImages=true&ImageTypeLimit=1&EnableUserData=false",
        server.address,
        encode(&server.details.user_id),
        ids.join(",")
    );
    let found = ask(net, Request::get(url, headers(&server.client, Some(key))))?;
    Ok(found.get("Items").and_then(Value::as_array).into_iter().flatten().filter_map(track).collect())
}

fn track(item: &Value) -> Option<Track> {
    let artists: Vec<String> = item.get("Artists").and_then(Value::as_array).into_iter().flatten().filter_map(|a| a.as_str().map(str::to_string)).collect();
    let artist = artist_line(&artists);
    let provider = |name: &str| {
        item.get("ProviderIds")
            .and_then(Value::as_object)
            .and_then(|ids| ids.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)))
            .and_then(|(_, v)| v.as_str())
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string)
    };
    // The first audio stream: a file with a cover carries it as a second,
    // video stream, and it is not the music.
    let audio = item
        .get("MediaSources")
        .and_then(Value::as_array)
        .and_then(|sources| sources.first())
        .and_then(|source| source.get("MediaStreams"))
        .and_then(Value::as_array)
        .and_then(|streams| streams.iter().find(|s| string(s, "Type").as_deref() == Some("Audio")))
        .cloned();
    let album_id = string(item, "AlbumId").unwrap_or_default();
    let cover = string(item, "AlbumPrimaryImageTag").filter(|_| !album_id.is_empty()).map(|tag| format!("{album_id}/{tag}"));
    Some(Track {
        id: string(item, "Id")?,
        title: string(item, "Name").unwrap_or_default(),
        album: string(item, "Album").unwrap_or_default(),
        album_artist: string(item, "AlbumArtist").unwrap_or_else(|| artist.clone()),
        artist,
        album_id,
        cover,
        disc: number(item, "ParentIndexNumber").map(|n| n as u16),
        discs: None,
        number: number(item, "IndexNumber").map(|n| n as u16),
        year: number(item, "ProductionYear").map(|y| y as u32),
        genres: item.get("Genres").and_then(Value::as_array).into_iter().flatten().filter_map(|g| g.as_str().map(str::to_string)).collect(),
        // Ticks of 100 ns.
        duration_ms: number(item, "RunTimeTicks").unwrap_or(0) / 10_000,
        codec: audio.as_ref().and_then(|a| string(a, "Codec")).map(|c| c.to_ascii_lowercase()),
        bits: audio.as_ref().and_then(|a| number(a, "BitDepth")).map(|b| b as u8),
        rate: audio.as_ref().and_then(|a| number(a, "SampleRate")).map(|r| r as u32),
        channels: audio.as_ref().and_then(|a| number(a, "Channels")).map(|c| c as u16),
        isrc: provider("ISRC"),
        recording_mbid: provider("MusicBrainzRecording").or_else(|| provider("MusicBrainzTrack")),
        revision: string(item, "Etag"),
        added_at: string(item, "DateCreated").as_deref().and_then(unix_time),
        complete: audio.is_some(),
    })
}

pub fn stream(server: &Server, key: &Credential, id: &str, codec: Option<&str>) -> Stream {
    Stream {
        url: format!("{}/Audio/{}/stream?static=true", server.address, encode(id)),
        headers: headers(&server.client, Some(key)),
        extension: codec.and_then(extension_of).map(str::to_string),
    }
}

pub fn cover(server: &Server, key: &Credential, cover: &str, size: u32) -> Request {
    let (item, tag) = cover.split_once('/').unwrap_or((cover, ""));
    let tag = if tag.is_empty() { String::new() } else { format!("&tag={}", encode(tag)) };
    image(format!("{}/Items/{}/Images/Primary?maxWidth={size}&quality=90{tag}", server.address, encode(item)), headers(&server.client, Some(key)))
}

/// A server of this kind, as a request is built for it.
pub fn server(address: &str, client: Client, libraries: Vec<String>, user_id: &str) -> Server {
    Server { kind: Kind::Jellyfin, address: address.to_string(), client, libraries, details: crate::Details { user_id: user_id.to_string(), ..Default::default() } }
}

#[cfg(test)]
mod tests;
