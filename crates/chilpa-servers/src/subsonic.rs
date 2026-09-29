//! OpenSubsonic, and any server that speaks Subsonic: Navidrome, gonic,
//! LMS, Ampache, Funkwhale (ADR-017, CHD-0060 §1).
//!
//! What the research found and this follows:
//!
//! - **Signing in is chosen per server.** OpenSubsonic's API key is asked
//!   for when the server announces `apiKeyAuthentication`; Navidrome 0.64.2
//!   does not, and takes md5(password + salt), with a fresh salt on every
//!   request.
//! - **The catalogue is walked by records**, `getAlbumList2` and `getAlbum`.
//!   An empty `search3` returns everything on Navidrome and gonic and not on
//!   LMS or Funkwhale, so it is not used.
//! - **Ids are opaque strings**, or numbers on Funkwhale. Navidrome's change
//!   when a file is retagged: the library takes that as one track gone and
//!   another arrived.
//! - **Errors come with HTTP 200**, as JSON even on a binary endpoint.
//! - **The original bytes are `stream?format=raw`.** Several servers
//!   convert per client or per user unless told not to.

use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::{extension_of, image, Client, Credential, Failure, Kind, Library, Page, Probe, Server, Stream, Track};
use crate::net::{Net, Request};
use crate::text::{artist_line, encode, id, number, random_hex, string, unix_time};

/// The protocol version asked for. Every server studied answers 1.15 or
/// 1.16, and a client may not ask for more than the server has.
const VERSION: &str = "1.16.1";

/// Records per page: the most `getAlbumList2` gives.
const RECORDS: usize = 500;

fn credentials(key: &Credential) -> String {
    match key {
        Credential::Password { user, password } => {
            let salt = random_hex(12);
            let token = format!("{:x}", Md5::digest(format!("{password}{salt}").as_bytes()));
            format!("u={}&t={token}&s={salt}", encode(user))
        }
        Credential::ApiKey { key } => format!("apiKey={}", encode(key)),
        _ => String::new(),
    }
}

fn url(address: &str, endpoint: &str, key: Option<&Credential>, extra: &str) -> String {
    let auth = key.map(credentials).filter(|a| !a.is_empty()).map(|a| format!("{a}&")).unwrap_or_default();
    format!("{address}/rest/{endpoint}.view?{auth}v={VERSION}&c={}&f=json{extra}", Client::PRODUCT)
}

/// Sends one request and returns what is inside `subsonic-response`, or why
/// not.
fn ask(net: &dyn Net, url: String) -> Result<Value, Failure> {
    let reply = net.send(&Request::get(url, Vec::new())).map_err(Failure::Unreachable)?;
    if (300..400).contains(&reply.status) {
        return Err(Failure::moved(reply.headers.get("location").map(String::as_str).unwrap_or("")));
    }
    let body: Value = match serde_json::from_slice(&reply.body) {
        Ok(v) => v,
        // Some errors come as XML even when JSON was asked for.
        Err(_) if reply.body.starts_with(b"<") => return Err(xml_error(&reply.body)),
        Err(_) if reply.status == 401 || reply.status == 403 => {
            return Err(Failure::Refused(chilpa_i18n::t("the server did not accept the user or the password")));
        }
        Err(_) => return Err(not_subsonic(reply.status)),
    };
    let Some(response) = body.get("subsonic-response") else { return Err(not_subsonic(reply.status)) };
    match response.get("status").and_then(Value::as_str) {
        Some("ok") => Ok(response.clone()),
        Some("failed") => Err(error_of(response.get("error").unwrap_or(&Value::Null))),
        _ => Err(not_subsonic(reply.status)),
    }
}

fn not_subsonic(status: u16) -> Failure {
    Failure::Unexpected(chilpa_i18n::t_args("that address does not answer like a Subsonic server (HTTP {})", &[&status.to_string()]))
}

fn error_of(error: &Value) -> Failure {
    let code = error.get("code").and_then(Value::as_i64).unwrap_or(0);
    let message = string(error, "message").unwrap_or_default();
    match code {
        40 | 44 => Failure::Refused(chilpa_i18n::t("the server did not accept the user or the password")),
        41 | 42 => Failure::Refused(chilpa_i18n::t("the server does not accept this way of signing in")),
        50 => Failure::Refused(chilpa_i18n::t("this user may not do that on the server")),
        70 => Failure::Unexpected(chilpa_i18n::t("the server no longer has it")),
        _ => Failure::Unexpected(chilpa_i18n::t_args("the server said: {}", &[&message])),
    }
}

fn xml_error(body: &[u8]) -> Failure {
    let text = String::from_utf8_lossy(body);
    let code = text.split("code=\"").nth(1).and_then(|rest| rest.split('"').next()).and_then(|c| c.parse().ok()).unwrap_or(0);
    error_of(&serde_json::json!({ "code": code }))
}

/// What the server says of itself, before signing in.
pub fn probe(net: &dyn Net, address: &str) -> Result<Probe, Failure> {
    let reply = net.send(&Request::get(url(address, "ping", None, ""), Vec::new())).map_err(Failure::Unreachable)?;
    if (300..400).contains(&reply.status) {
        return Err(Failure::moved(reply.headers.get("location").map(String::as_str).unwrap_or("")));
    }
    let body: Value = serde_json::from_slice(&reply.body).map_err(|_| not_subsonic(reply.status))?;
    let response = body.get("subsonic-response").ok_or_else(|| not_subsonic(reply.status))?;
    let mut probe = Probe { product: product(response), ..Probe::default() };
    // Public by the spec; gonic and Ampache still ask for credentials, and
    // then it is read again after signing in.
    if let Ok(found) = ask(net, url(address, "getOpenSubsonicExtensions", None, "")) {
        probe.extensions = extensions(&found);
        probe.api_key = probe.extensions.iter().any(|e| e == "apiKeyAuthentication");
    }
    Ok(probe)
}

fn product(response: &Value) -> String {
    let kind = string(response, "type").unwrap_or_else(|| "Subsonic".into());
    let mut name = kind.clone();
    if let Some(first) = name.get(..1) {
        name.replace_range(..1, &first.to_uppercase());
    }
    match string(response, "serverVersion") {
        // "0.64.2 (10114574)": the build is noise to anyone reading it.
        Some(version) => format!("{name} {}", version.split(' ').next().unwrap_or(&version)),
        None => name,
    }
}

fn extensions(response: &Value) -> Vec<String> {
    response.get("openSubsonicExtensions").and_then(Value::as_array).into_iter().flatten().filter_map(|e| string(e, "name")).collect()
}

/// What signing in gives: the credential to seal, who it is, and what the
/// server is.
pub struct SignedIn {
    pub key: Credential,
    pub account: String,
    pub identity: String,
    pub product: String,
    pub extensions: Vec<String>,
}

/// Signs in with a password, or with an API key when `api_key` says the
/// secret is one.
pub fn sign_in(net: &dyn Net, address: &str, user: &str, secret: &str, api_key: bool) -> Result<SignedIn, Failure> {
    let key = if api_key {
        Credential::ApiKey { key: secret.to_string() }
    } else {
        Credential::Password { user: user.trim().to_string(), password: secret.to_string() }
    };
    let pong = ask(net, url(address, "ping", Some(&key), ""))?;
    let account = if api_key {
        // An API key does not say whose it is; `tokenInfo` does.
        let info = ask(net, url(address, "tokenInfo", Some(&key), ""))?;
        info.get("tokenInfo").and_then(|t| string(t, "username")).unwrap_or_default()
    } else {
        user.trim().to_string()
    };
    let extensions = ask(net, url(address, "getOpenSubsonicExtensions", Some(&key), "")).map(|r| extensions(&r)).unwrap_or_default();
    // No server id in the protocol: the address and the user stand for it.
    let identity = format!("{}|{}", address.to_lowercase(), account.to_lowercase());
    Ok(SignedIn { key, account, identity, product: product(&pong), extensions })
}

pub fn ping(net: &dyn Net, server: &Server, key: &Credential) -> Result<(), Failure> {
    ask(net, url(&server.address, "ping", Some(key), "")).map(|_| ())
}

pub fn libraries(net: &dyn Net, server: &Server, key: &Credential) -> Result<Vec<Library>, Failure> {
    let found = ask(net, url(&server.address, "getMusicFolders", Some(key), ""))?;
    let folders = found.get("musicFolders").and_then(|f| f.get("musicFolder")).and_then(Value::as_array).cloned().unwrap_or_default();
    Ok(folders
        .iter()
        .filter_map(|f| {
            let id = id(f, "id")?;
            let name = string(f, "name").unwrap_or_else(|| id.clone());
            Some(Library { id, name })
        })
        .collect())
}

#[derive(Serialize, Deserialize, Default)]
struct Cursor {
    /// Which of the chosen folders, when some were chosen.
    folder: usize,
    offset: usize,
}

pub fn page(net: &dyn Net, server: &Server, key: &Credential, cursor: Option<&str>) -> Result<Page, Failure> {
    let cursor: Cursor = match cursor {
        Some(text) => serde_json::from_str(text).map_err(|_| Failure::Unexpected(chilpa_i18n::t("the sync lost its place; start it again")))?,
        None => {
            // A server that is reading its own library answers with half of
            // it. Waiting beats taking the other half for gone.
            let status = ask(net, url(&server.address, "getScanStatus", Some(key), ""))?;
            if status.get("scanStatus").and_then(|s| s.get("scanning")).and_then(Value::as_bool) == Some(true) {
                return Err(Failure::Busy(chilpa_i18n::t("the server is reading its own library; sync again when it finishes")));
            }
            Cursor::default()
        }
    };
    let folder = server.libraries.get(cursor.folder).map(|f| format!("&musicFolderId={}", encode(f))).unwrap_or_default();
    let list = ask(net, url(&server.address, "getAlbumList2", Some(key), &format!("&type=alphabeticalByName&size={RECORDS}&offset={}{folder}", cursor.offset)))?;
    let records = list.get("albumList2").and_then(|l| l.get("album")).and_then(Value::as_array).cloned().unwrap_or_default();
    let mut tracks = Vec::new();
    let mut partial = false;
    for record in &records {
        let Some(record_id) = id(record, "id") else {
            partial = true;
            continue;
        };
        let found = ask(net, url(&server.address, "getAlbum", Some(key), &format!("&id={}", encode(&record_id))))?;
        let Some(album) = found.get("album") else {
            partial = true;
            continue;
        };
        for song in album.get("song").and_then(Value::as_array).into_iter().flatten() {
            match track(song, album) {
                Some(track) => tracks.push(track),
                None => partial = true,
            }
        }
    }
    let next = if records.len() >= RECORDS {
        Some(Cursor { folder: cursor.folder, offset: cursor.offset + records.len() })
    } else if cursor.folder + 1 < server.libraries.len() {
        Some(Cursor { folder: cursor.folder + 1, offset: 0 })
    } else {
        None
    };
    Ok(Page { tracks, next: next.map(|c| serde_json::to_string(&c).unwrap_or_default()), total: None, scope: cursor.folder.to_string(), partial })
}

/// How many songs the chosen folders have now, from the record list alone.
///
/// Subsonic says no total, and paging records by offset can skip one when
/// the catalogue changes between pages. The sync compares this, asked after
/// the last page, with what it kept: fewer kept means nothing is taken out.
pub fn census(net: &dyn Net, server: &Server, key: &Credential) -> Result<u64, Failure> {
    let status = ask(net, url(&server.address, "getScanStatus", Some(key), ""))?;
    if status.get("scanStatus").and_then(|s| s.get("scanning")).and_then(Value::as_bool) == Some(true) {
        return Err(Failure::Busy(chilpa_i18n::t("the server is reading its own library; sync again when it finishes")));
    }
    let folders: Vec<String> = if server.libraries.is_empty() {
        vec![String::new()]
    } else {
        server.libraries.iter().map(|f| format!("&musicFolderId={}", encode(f))).collect()
    };
    let mut records = std::collections::HashSet::new();
    let mut songs = 0;
    for folder in folders {
        let mut offset = 0;
        loop {
            let list = ask(net, url(&server.address, "getAlbumList2", Some(key), &format!("&type=alphabeticalByName&size={RECORDS}&offset={offset}{folder}")))?;
            let page = list.get("albumList2").and_then(|l| l.get("album")).and_then(Value::as_array).cloned().unwrap_or_default();
            for record in &page {
                if records.insert(id(record, "id").unwrap_or_default()) {
                    songs += number(record, "songCount").unwrap_or(0);
                }
            }
            if page.len() < RECORDS {
                break;
            }
            offset += page.len();
        }
    }
    Ok(songs)
}

/// A song, read with the record it came in: the record says what the song
/// sometimes leaves out.
fn track(song: &Value, album: &Value) -> Option<Track> {
    let artists: Vec<String> = song.get("artists").and_then(Value::as_array).into_iter().flatten().filter_map(|a| string(a, "name")).collect();
    let artist = string(song, "displayArtist").or_else(|| string(song, "artist")).unwrap_or_else(|| artist_line(&artists));
    let genres: Vec<String> = match song.get("genres").and_then(Value::as_array) {
        Some(list) if !list.is_empty() => list.iter().filter_map(|g| string(g, "name")).collect(),
        _ => string(song, "genre").into_iter().collect(),
    };
    let bits = number(song, "bitDepth").map(|b| b as u8);
    Some(Track {
        id: id(song, "id")?,
        title: string(song, "title").unwrap_or_default(),
        album: string(song, "album").or_else(|| string(album, "name")).unwrap_or_default(),
        album_artist: string(song, "displayAlbumArtist")
            .or_else(|| string(album, "displayArtist"))
            .or_else(|| string(album, "artist"))
            .unwrap_or_else(|| artist.clone()),
        artist,
        artists,
        album_id: id(song, "albumId").or_else(|| id(album, "id")).unwrap_or_default(),
        cover: string(album, "coverArt").or_else(|| string(song, "coverArt")),
        disc: number(song, "discNumber").map(|n| n as u16),
        discs: None,
        number: number(song, "track").map(|n| n as u16),
        year: number(song, "year").or_else(|| number(album, "year")).map(|y| y as u32),
        genres,
        duration_ms: number(song, "duration").unwrap_or(0) * 1000,
        codec: codec(string(song, "suffix").as_deref(), bits),
        bits,
        rate: number(song, "samplingRate").map(|r| r as u32),
        channels: number(song, "channelCount").map(|c| c as u16),
        isrc: song.get("isrc").and_then(|v| match v {
            Value::Array(list) => list.iter().filter_map(Value::as_str).find(|s| !s.is_empty()).map(str::to_string),
            Value::String(s) if !s.is_empty() => Some(s.clone()),
            _ => None,
        }),
        recording_mbid: string(song, "musicBrainzId"),
        revision: None,
        added_at: string(song, "created").as_deref().and_then(unix_time),
        complete: true,
    })
}

/// Subsonic says the file's suffix, not its codec. An `m4a` is ALAC when it
/// has a bit depth and AAC when it has none.
fn codec(suffix: Option<&str>, bits: Option<u8>) -> Option<String> {
    let suffix = suffix?.to_ascii_lowercase();
    Some(match suffix.as_str() {
        "m4a" | "mp4" | "m4b" => if bits.is_some() { "alac" } else { "aac" }.to_string(),
        "ogg" | "oga" => "vorbis".to_string(),
        _ => suffix,
    })
}

pub fn stream(server: &Server, key: &Credential, id: &str, codec: Option<&str>) -> Stream {
    Stream {
        url: url(&server.address, "stream", Some(key), &format!("&id={}&format=raw", encode(id))),
        headers: Vec::new(),
        extension: codec.and_then(extension_of).map(str::to_string),
    }
}

pub fn cover(server: &Server, key: &Credential, cover: &str, size: u32) -> Request {
    image(url(&server.address, "getCoverArt", Some(key), &format!("&id={}&size={size}", encode(cover))), Vec::new())
}

/// A server of this kind with this address, as a request is built for it.
pub fn server(address: &str, client: Client, libraries: Vec<String>, extensions: Vec<String>) -> Server {
    Server { kind: Kind::Subsonic, address: address.to_string(), client, libraries, details: crate::Details { extensions, ..Default::default() } }
}

#[cfg(test)]
mod tests;
