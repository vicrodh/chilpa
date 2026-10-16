//! Plex Media Server: its music, never its video (ADR-017, CHD-0060 §3).
//!
//! What the research found and this follows:
//!
//! - **Signing in is a PIN on plex.tv**, confirmed by the user at
//!   plex.tv/link. The account's token finds the servers (`resources`); each
//!   server has a token of its own, and that is what talks to it.
//! - **A server has several addresses.** A direct one is used, local
//!   first. Relay is never used: it is capped at 2 Mbps and cannot carry
//!   hi-res as it is.
//! - **`resources` lists only its `plex.direct` names over https** (measured
//!   on 1.43.4). When the server does not require https, its local
//!   addresses are also tried as plain `http://address:port`, after the
//!   https ones: a system without the certificate's root (Let's Encrypt's
//!   `ISRG Root YR`, measured) would otherwise never reach a server on its
//!   own network.
//! - **Only sections of type `artist`** are read. Nothing else of the
//!   server is asked for.
//! - **The listing does not carry bit depth or rate** (measured). They come
//!   from `/library/metadata/a,b,c` for what is new or changed.
//! - **The track's artist is `originalTitle`** on a compilation;
//!   `grandparentTitle` is the album artist. QBZ read the second as the
//!   first.
//! - **The part's key changes when its file does**
//!   (`/library/parts/10/<changestamp>/file.flac`), so it is the track's
//!   revision and where its bytes are.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

use crate::model::{extension_of, image, Client, Credential, Failure, Kind, Library, Page, Server, Stream, Track};
use crate::net::{Net, Request};
use crate::text::{encode, id, number, string};

/// Where the user confirms the PIN.
pub const LINK: &str = "https://plex.tv/link";

const PAGE: u64 = 250;

fn headers(client: &Client, token: Option<&str>) -> Vec<(String, String)> {
    let mut out = vec![
        ("Accept".into(), "application/json".into()),
        ("X-Plex-Product".into(), Client::PRODUCT.into()),
        ("X-Plex-Version".into(), client.version.clone()),
        ("X-Plex-Client-Identifier".into(), client.id.clone()),
        ("X-Plex-Platform".into(), client.platform.clone()),
        ("X-Plex-Device-Name".into(), client.device.clone()),
    ];
    if let Some(token) = token {
        out.push(("X-Plex-Token".into(), token.into()));
    }
    out
}

fn server_token(key: &Credential) -> Option<&str> {
    match key {
        Credential::Plex { server, .. } => Some(server.as_str()),
        _ => None,
    }
}

fn ask(net: &dyn Net, request: Request) -> Result<Value, Failure> {
    let reply = net.send(&request).map_err(Failure::Unreachable)?;
    match reply.status {
        200..=299 if reply.body.is_empty() => Ok(Value::Null),
        200..=299 => serde_json::from_slice(&reply.body).map_err(|_| not_plex(reply.status)),
        300..=399 => Err(Failure::moved(reply.headers.get("location").map(String::as_str).unwrap_or(""))),
        401 | 403 => Err(Failure::Refused(chilpa_i18n::t("Plex did not accept the session; sign in again"))),
        503 => Err(Failure::Busy(chilpa_i18n::t("the server is starting; try again in a moment"))),
        status => Err(not_plex(status)),
    }
}

fn not_plex(status: u16) -> Failure {
    Failure::Unexpected(chilpa_i18n::t_args("that address does not answer like a Plex server (HTTP {})", &[&status.to_string()]))
}

/// A PIN waiting to be confirmed at `LINK`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pin {
    pub id: u64,
    /// Four characters, typed at plex.tv/link.
    pub code: String,
}

pub fn pin_start(net: &dyn Net, client: &Client) -> Result<Pin, Failure> {
    let answer = ask(net, Request::post("https://plex.tv/api/v2/pins?strong=false".into(), headers(client, None), None))?;
    Ok(Pin { id: number(&answer, "id").ok_or_else(|| not_plex(200))?, code: string(&answer, "code").ok_or_else(|| not_plex(200))? })
}

/// The account's token once the PIN was confirmed; `None` while it waits.
pub fn pin_token(net: &dyn Net, client: &Client, pin: u64) -> Result<Option<String>, Failure> {
    let reply = net.send(&Request::get(format!("https://plex.tv/api/v2/pins/{pin}"), headers(client, None))).map_err(Failure::Unreachable)?;
    match reply.status {
        200 => Ok(serde_json::from_slice::<Value>(&reply.body).ok().and_then(|v| string(&v, "authToken"))),
        404 => Err(Failure::Refused(chilpa_i18n::t("the code expired; start again"))),
        status => Err(not_plex(status)),
    }
}

/// A server the account can reach.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlexServer {
    pub name: String,
    /// Its `machineIdentifier`.
    pub identity: String,
    pub owned: bool,
    pub product: String,
    /// Its own token, to talk to it.
    pub token: String,
    /// Direct addresses only, best first.
    pub connections: Vec<String>,
}

/// The servers the account can reach, with their direct addresses.
pub fn servers(net: &dyn Net, client: &Client, account: &str) -> Result<Vec<PlexServer>, Failure> {
    let url = "https://clients.plex.tv/api/v2/resources?includeHttps=1&includeRelay=0&includeIPv6=1".to_string();
    let found = ask(net, Request::get(url, headers(client, Some(account))))?;
    Ok(found
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| string(r, "provides").is_some_and(|p| p.split(',').any(|x| x.trim() == "server")))
        .filter_map(|r| {
            // A server is only ever talked to with its own token: without
            // one it is not offered, and the account's never goes to it.
            let token = string(r, "accessToken")?;
            let owned = r.get("owned").and_then(Value::as_bool) == Some(true);
            let https_required = r.get("httpsRequired").and_then(Value::as_bool) == Some(true);
            let mut connections: Vec<(bool, bool, bool, String)> = Vec::new();
            for c in r.get("connections").and_then(Value::as_array).into_iter().flatten() {
                if c.get("relay").and_then(Value::as_bool) == Some(true) {
                    continue;
                }
                let Some(uri) = string(c, "uri").and_then(|u| direct(&u, owned)) else { continue };
                let local = c.get("local").and_then(Value::as_bool) == Some(true);
                let v6 = c.get("IPv6").and_then(Value::as_bool) == Some(true);
                if local && !https_required && uri.starts_with("https://") {
                    if let (Some(address), Some(port)) = (string(c, "address"), number(c, "port")) {
                        let host = if v6 { format!("[{address}]") } else { address };
                        if let Some(plain) = direct(&format!("http://{host}:{port}"), owned) {
                            connections.push((false, true, v6, plain));
                        }
                    }
                }
                connections.push((!local, !uri.starts_with("https://"), v6, uri));
            }
            // Local before remote, https before http, IPv4 before IPv6.
            connections.sort();
            connections.dedup_by(|a, b| a.3 == b.3);
            Some(PlexServer {
                name: string(r, "name").unwrap_or_default(),
                identity: string(r, "clientIdentifier")?,
                owned,
                product: format!("Plex {}", string(r, "productVersion").unwrap_or_default()).trim().to_string(),
                token,
                connections: connections.into_iter().map(|(_, _, _, uri)| uri).collect(),
            })
        })
        .collect())
}

/// An address plex.tv gave, if it is only a scheme, a host and a port.
///
/// The addresses come from whoever runs the server, and each one is asked
/// for `/identity` before anything proves it is that server. So nothing
/// else rides along (a user, a query, a path), and a server shared by
/// someone else cannot point this machine at itself or at a link-local
/// address.
fn direct(uri: &str, owned: bool) -> Option<String> {
    let rest = uri.strip_prefix("https://").or_else(|| uri.strip_prefix("http://"))?;
    let authority = rest.strip_suffix('/').unwrap_or(rest);
    if authority.is_empty() || authority.contains(['/', '?', '#', '@', ' ']) {
        return None;
    }
    let host = match authority.strip_prefix('[') {
        Some(v6) => v6.split(']').next()?.to_ascii_lowercase(),
        None => authority.rsplit_once(':').map_or(authority, |(h, _)| h).to_ascii_lowercase(),
    };
    let here = host == "localhost"
        || host.starts_with("127.")
        || host == "0.0.0.0"
        || host == "::1"
        || host == "::"
        || host.starts_with("169.254.")
        || host.starts_with("fe80:");
    if here && !owned {
        return None;
    }
    Some(uri[..uri.len() - (rest.len() - authority.len())].to_string())
}

/// The first of the server's addresses that answers from here as that same
/// server.
pub fn reach(net: &dyn Net, client: &Client, identity: &str, connections: &[String]) -> Result<String, Failure> {
    for address in connections {
        let mut request = Request::get(format!("{address}/identity"), headers(client, None));
        request.timeout = Duration::from_secs(4);
        if let Ok(found) = ask(net, request) {
            let answered = found.get("MediaContainer").and_then(|c| string(c, "machineIdentifier"));
            if answered.as_deref() == Some(identity) {
                return Ok(address.clone());
            }
        }
    }
    Err(Failure::Unreachable(chilpa_i18n::t("none of that server's addresses answers from here")))
}

pub fn ping(net: &dyn Net, server: &Server, key: &Credential) -> Result<(), Failure> {
    ask(net, Request::get(format!("{}/library/sections", server.address), headers(&server.client, server_token(key)))).map(|_| ())
}

pub fn libraries(net: &dyn Net, server: &Server, key: &Credential) -> Result<Vec<Library>, Failure> {
    let found = ask(net, Request::get(format!("{}/library/sections", server.address), headers(&server.client, server_token(key))))?;
    Ok(found
        .get("MediaContainer")
        .and_then(|c| c.get("Directory"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|d| string(d, "type").as_deref() == Some("artist"))
        .filter_map(|d| Some(Library { id: id(d, "key")?, name: string(d, "title").unwrap_or_default() }))
        .collect())
}

#[derive(Serialize, Deserialize, Default)]
struct Cursor {
    sections: Vec<String>,
    section: usize,
    offset: u64,
}

pub fn page(net: &dyn Net, server: &Server, key: &Credential, cursor: Option<&str>) -> Result<Page, Failure> {
    let cursor = match cursor {
        Some(text) => serde_json::from_str(text).map_err(|_| Failure::Unexpected(chilpa_i18n::t("the sync lost its place; start it again")))?,
        None => {
            let sections = if server.libraries.is_empty() {
                libraries(net, server, key)?.into_iter().map(|l| l.id).collect()
            } else {
                server.libraries.clone()
            };
            Cursor { sections, section: 0, offset: 0 }
        }
    };
    let Some(section) = cursor.sections.get(cursor.section).cloned() else {
        return Ok(Page::default());
    };
    let mut headers = headers(&server.client, server_token(key));
    headers.push(("X-Plex-Container-Start".into(), cursor.offset.to_string()));
    headers.push(("X-Plex-Container-Size".into(), PAGE.to_string()));
    let url = format!("{}/library/sections/{}/all?type=10&includeGuids=1", server.address, encode(&section));
    let found = ask(net, Request::get(url, headers))?;
    let container = found.get("MediaContainer").cloned().unwrap_or(Value::Null);
    let items = container.get("Metadata").and_then(Value::as_array).cloned().unwrap_or_default();
    // `size` is this page's; only `totalSize` is the section's. Without it,
    // pages go on until a short one, and the sync takes nothing out.
    let total = container.get("totalSize").and_then(Value::as_u64);
    let tracks: Vec<Track> = items.iter().filter_map(track).collect();
    let partial = tracks.len() < items.len() || total.is_none();
    let read = cursor.offset + items.len() as u64;
    let more = match total {
        Some(t) => read < t,
        None => items.len() as u64 >= PAGE,
    };
    let next = if !items.is_empty() && more {
        Some(Cursor { offset: read, ..cursor })
    } else if cursor.section + 1 < cursor.sections.len() {
        Some(Cursor { section: cursor.section + 1, offset: 0, ..cursor })
    } else {
        None
    };
    Ok(Page { tracks, next: next.map(|c| serde_json::to_string(&c).unwrap_or_default()), total, scope: section, partial })
}

pub fn details(net: &dyn Net, server: &Server, key: &Credential, ids: &[String]) -> Result<Vec<Track>, Failure> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<String> = ids.iter().map(|id| encode(id)).collect();
    let url = format!("{}/library/metadata/{}?includeGuids=1", server.address, ids.join(","));
    let found = ask(net, Request::get(url, headers(&server.client, server_token(key))))?;
    Ok(found.get("MediaContainer").and_then(|c| c.get("Metadata")).and_then(Value::as_array).into_iter().flatten().filter_map(track).collect())
}

fn track(meta: &Value) -> Option<Track> {
    let media = meta.get("Media").and_then(Value::as_array).and_then(|m| m.first()).cloned().unwrap_or(Value::Null);
    let part = media.get("Part").and_then(Value::as_array).and_then(|p| p.first()).cloned().unwrap_or(Value::Null);
    let streams: Vec<Value> = part.get("Stream").and_then(Value::as_array).cloned().unwrap_or_default();
    let audio = streams
        .iter()
        .filter(|s| number(s, "streamType") == Some(2))
        .max_by_key(|s| s.get("selected").and_then(Value::as_bool) == Some(true))
        .cloned();
    let album_artist = string(meta, "grandparentTitle").unwrap_or_default();
    let mbid = meta
        .get("Guid")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|g| string(g, "id"))
        .find_map(|g| g.strip_prefix("mbid://").map(str::to_string));
    Some(Track {
        id: id(meta, "ratingKey")?,
        title: string(meta, "title").unwrap_or_default(),
        artist: string(meta, "originalTitle").unwrap_or_else(|| album_artist.clone()),
        artists: Vec::new(),
        album: string(meta, "parentTitle").unwrap_or_default(),
        album_artist,
        album_id: id(meta, "parentRatingKey").unwrap_or_default(),
        cover: string(meta, "parentThumb").or_else(|| string(meta, "thumb")),
        disc: number(meta, "parentIndex").map(|n| n as u16),
        discs: None,
        number: number(meta, "index").map(|n| n as u16),
        year: number(meta, "parentYear").or_else(|| number(meta, "year")).map(|y| y as u32),
        genres: meta.get("Genre").and_then(Value::as_array).into_iter().flatten().filter_map(|g| string(g, "tag")).collect(),
        duration_ms: number(meta, "duration").unwrap_or(0),
        codec: audio.as_ref().and_then(|s| string(s, "codec")).or_else(|| string(&media, "audioCodec")).map(|c| c.to_ascii_lowercase()),
        bits: audio.as_ref().and_then(|s| number(s, "bitDepth")).map(|b| b as u8),
        rate: audio.as_ref().and_then(|s| number(s, "samplingRate")).map(|r| r as u32),
        channels: audio.as_ref().and_then(|s| number(s, "channels")).or_else(|| number(&media, "audioChannels")).map(|c| c as u16),
        isrc: None,
        recording_mbid: mbid,
        revision: string(&part, "key"),
        added_at: meta.get("addedAt").and_then(Value::as_i64),
        complete: audio.is_some(),
    })
}

pub fn stream(server: &Server, key: &Credential, id: &str, codec: Option<&str>, revision: Option<&str>) -> Stream {
    // The part's key is where the file is. Without it only the track's
    // metadata can be named, and the reader refuses that as not audio.
    let path = match revision.filter(|r| r.starts_with("/library/parts/")) {
        Some(part) => format!("{part}{}download=1", if part.contains('?') { "&" } else { "?" }),
        None => format!("/library/metadata/{}", encode(id)),
    };
    Stream {
        url: format!("{}{path}", server.address),
        headers: headers(&server.client, server_token(key)),
        extension: codec.and_then(extension_of).map(str::to_string),
    }
}

pub fn cover(server: &Server, key: &Credential, cover: &str, size: u32) -> Request {
    image(
        format!("{}/photo/:/transcode?width={size}&height={size}&minSize=1&upscale=0&url={}", server.address, encode(cover)),
        headers(&server.client, server_token(key)),
    )
}

/// A server of this kind, as a request is built for it.
pub fn server(address: &str, client: Client, libraries: Vec<String>, connections: Vec<String>) -> Server {
    Server { kind: Kind::Plex, address: address.to_string(), client, libraries, details: crate::Details { connections, ..Default::default() } }
}

#[cfg(test)]
mod tests;
