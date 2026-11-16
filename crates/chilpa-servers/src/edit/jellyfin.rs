//! Jellyfin's items, read and changed (12.1, measured on the lab server).
//!
//! - `POST /Items/{id}` takes the whole item (`BaseItemDto`) and clears
//!   what is left out of it: posted with only a name, a track lost its
//!   record, numbers and year (measured). So the item is read with
//!   `GET /Items/{id}?userId=` and posted back whole with the changes.
//! - It answers 204, and its `Etag` moves: a sync reads it again.
//! - Artists are posted by name (`Artists`, and `AlbumArtists` as
//!   `{"Name": ...}`); Jellyfin finds or makes their items.
//! - The fields it can lock are few (`MetadataField`): `Name`, `Genres`,
//!   `Studios`, `Overview`, and `Cast`, which keeps a track's artists and
//!   record artists from being read again from its file. Each one written
//!   that has a lock gets it. Numbers, the year and the record's title have
//!   none: a file read again can bring its year back (Jellyfin source,
//!   `AudioFileProber`).
//! - `POST /Items/{id}/Images/Primary` takes the image as base64 text with
//!   the image's type: raw bytes answer 500 (measured, though the OpenAPI
//!   says binary). It is kept in the server's metadata folder.
//! - Both take an administrator (`RequiresElevation`).

use serde_json::{json, Map, Value};
use std::time::Duration;

use super::{refused, whole_day, year_of, Edit, Held, RECORDING, RELEASE, RELEASE_GROUP, TRACK};
use crate::jellyfin::{ask, headers};
use crate::model::{Credential, Failure, Server};
use crate::net::{Net, Request};
use crate::text::{artist_line, encode, number, string};

/// How long the server has to take one change, or one image.
const WRITE_TIME: Duration = Duration::from_secs(20);
const IMAGE_TIME: Duration = Duration::from_secs(60);

/// The provider ids Jellyfin keeps, by the editor's name.
const PROVIDERS: [(&str, &str); 4] =
    [(RELEASE, "MusicBrainzAlbum"), (RELEASE_GROUP, "MusicBrainzReleaseGroup"), (RECORDING, "MusicBrainzRecording"), (TRACK, "MusicBrainzTrack")];

/// The fields a lock exists for, by the editor's name.
const LOCKS: [(&str, &str); 7] = [
    ("title", "Name"),
    ("genres", "Genres"),
    ("label", "Studios"),
    ("summary", "Overview"),
    ("artist_credit", "Cast"),
    ("artists", "Cast"),
    ("album_artist", "Cast"),
];

pub(super) fn may_edit(net: &dyn Net, server: &Server, key: &Credential) -> Result<(), Failure> {
    let me = ask(net, Request::get(format!("{}/Users/Me", server.address), headers(&server.client, Some(key))))?;
    let admin = me.get("Policy").and_then(|p| p.get("IsAdministrator")).and_then(Value::as_bool) == Some(true);
    if admin {
        Ok(())
    } else {
        Err(Failure::Forbidden(chilpa_i18n::t("only an administrator of this server can change its metadata")))
    }
}

fn names(value: &Value, key: &str) -> Vec<String> {
    value.get(key).and_then(Value::as_array).into_iter().flatten().filter_map(|v| v.as_str().map(str::trim)).filter(|s| !s.is_empty()).map(str::to_string).collect()
}

/// The names of a list of `{"Name": ...}`.
fn named(value: &Value, key: &str) -> Vec<String> {
    value.get(key).and_then(Value::as_array).into_iter().flatten().filter_map(|v| string(v, "Name")).collect()
}

/// What one item says.
fn read(item: &Value) -> Held {
    let artists = names(item, "Artists");
    let ids = item.get("ProviderIds").and_then(Value::as_object);
    let provider = |name: &str| ids.and_then(|ids| ids.iter().find(|(k, _)| k.eq_ignore_ascii_case(name))).and_then(|(_, v)| v.as_str()).map(str::trim).filter(|v| !v.is_empty());
    let date = match string(item, "PremiereDate") {
        Some(day) if day.len() >= 10 => day[..10].to_string(),
        _ => number(item, "ProductionYear").map(|y| format!("{y:04}")).unwrap_or_default(),
    };
    let lock_all = item.get("LockData").and_then(Value::as_bool) == Some(true);
    let locked_names = names(item, "LockedFields");
    let locked = LOCKS
        .iter()
        .filter(|(_, theirs)| lock_all || locked_names.iter().any(|n| n == theirs))
        .map(|(ours, _)| ours.to_string())
        .collect();
    Held {
        title: string(item, "Name").unwrap_or_default(),
        album: string(item, "Album").unwrap_or_default(),
        album_artist: string(item, "AlbumArtist").unwrap_or_default(),
        artist: artist_line(&artists),
        artists,
        date,
        genres: names(item, "Genres"),
        label: named(item, "Studios").join("; "),
        track_number: number(item, "IndexNumber").map(|n| n as u32),
        disc_number: number(item, "ParentIndexNumber").map(|n| n as u32),
        summary: string(item, "Overview").unwrap_or_default(),
        ids: PROVIDERS.iter().filter_map(|(ours, theirs)| Some((ours.to_string(), provider(theirs)?.to_lowercase()))).collect(),
        locked,
    }
}

fn item(net: &dyn Net, server: &Server, key: &Credential, id: &str) -> Result<Value, Failure> {
    let url = format!("{}/Items/{}?userId={}", server.address, encode(id), encode(&server.details.user_id));
    let found = ask(net, Request::get(url, headers(&server.client, Some(key))))?;
    if found.get("Id").is_none() {
        return Err(Failure::Unexpected(chilpa_i18n::t("the server answered without the item")));
    }
    Ok(found)
}

pub(super) fn held(net: &dyn Net, server: &Server, key: &Credential, id: &str) -> Result<Held, Failure> {
    item(net, server, key, id).map(|found| read(&found))
}

pub(super) fn held_many(net: &dyn Net, server: &Server, key: &Credential, ids: &[String]) -> Result<Vec<(String, Held)>, Failure> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<String> = ids.iter().map(|id| encode(id)).collect();
    let url = format!(
        "{}/Items?userId={}&Ids={}&Fields=Genres,ProviderIds,Studios,Overview&EnableImages=false&EnableUserData=false",
        server.address,
        encode(&server.details.user_id),
        ids.join(",")
    );
    let found = ask(net, Request::get(url, headers(&server.client, Some(key))))?;
    Ok(found.get("Items").and_then(Value::as_array).into_iter().flatten().filter_map(|item| Some((string(item, "Id")?, read(item)))).collect())
}

/// Adds a lock for each field written that has one.
fn lock(fields: &mut Map<String, Value>, edit: &Edit) {
    let written = |ours: &str| match ours {
        "title" => edit.title.is_some(),
        "genres" => edit.genres.is_some(),
        "label" => edit.label.is_some(),
        "summary" => edit.summary.is_some(),
        "artist_credit" | "artists" => edit.artist.is_some() || edit.artists.is_some(),
        "album_artist" => edit.album_artist.is_some(),
        _ => false,
    };
    let mut locked: Vec<Value> = fields.get("LockedFields").and_then(Value::as_array).cloned().unwrap_or_default();
    for (ours, theirs) in LOCKS {
        if written(ours) && !locked.iter().any(|v| v.as_str() == Some(theirs)) {
            locked.push(Value::from(theirs));
        }
    }
    fields.insert("LockedFields".into(), Value::Array(locked));
}

/// The item as read, with the changes in it.
pub(super) fn changed(mut found: Value, edit: &Edit) -> Value {
    let Some(fields) = found.as_object_mut() else { return found };
    let text = |v: &str| if v.trim().is_empty() { Value::Null } else { Value::from(v.trim()) };
    if let Some(title) = &edit.title {
        fields.insert("Name".into(), text(title));
    }
    if let Some(album) = &edit.album {
        fields.insert("Album".into(), text(album));
    }
    if let Some(artist) = &edit.album_artist {
        let list = if artist.trim().is_empty() { json!([]) } else { json!([{ "Name": artist.trim() }]) };
        fields.insert("AlbumArtists".into(), list);
        fields.insert("AlbumArtist".into(), text(artist));
    }
    let artists = edit.artists.clone().or_else(|| edit.artist.as_ref().map(|a| if a.trim().is_empty() { Vec::new() } else { vec![a.trim().to_string()] }));
    if let Some(artists) = artists {
        fields.insert("Artists".into(), json!(artists));
        fields.insert("ArtistItems".into(), Value::Array(artists.iter().map(|a| json!({ "Name": a })).collect()));
    }
    if let Some(date) = &edit.date {
        fields.insert("ProductionYear".into(), year_of(date).map_or(Value::Null, Value::from));
        let day = if whole_day(date) { Value::from(format!("{date}T00:00:00.0000000Z")) } else { Value::Null };
        fields.insert("PremiereDate".into(), day);
    }
    if let Some(genres) = &edit.genres {
        fields.insert("Genres".into(), json!(genres));
        fields.remove("GenreItems");
    }
    if let Some(label) = &edit.label {
        let list = if label.trim().is_empty() { json!([]) } else { json!([{ "Name": label.trim() }]) };
        fields.insert("Studios".into(), list);
    }
    if let Some(number) = edit.track_number {
        fields.insert("IndexNumber".into(), number.map_or(Value::Null, Value::from));
    }
    if let Some(number) = edit.disc_number {
        fields.insert("ParentIndexNumber".into(), number.map_or(Value::Null, Value::from));
    }
    if let Some(summary) = &edit.summary {
        fields.insert("Overview".into(), text(summary));
    }
    if !edit.ids.is_empty() {
        let mut ids = fields.get("ProviderIds").and_then(Value::as_object).cloned().unwrap_or_default();
        for (ours, value) in &edit.ids {
            let Some((_, theirs)) = PROVIDERS.iter().find(|(o, _)| o == ours) else { continue };
            ids.retain(|k, _| !k.eq_ignore_ascii_case(theirs));
            if !value.trim().is_empty() {
                ids.insert(theirs.to_string(), Value::from(value.trim()));
            }
        }
        fields.insert("ProviderIds".into(), Value::Object(ids));
    }
    lock(fields, edit);
    found
}

pub(super) fn write(net: &dyn Net, server: &Server, key: &Credential, id: &str, edit: &Edit) -> Result<(), Failure> {
    let found = item(net, server, key, id)?;
    let body = serde_json::to_vec(&changed(found, edit)).map_err(|_| Failure::Unexpected(chilpa_i18n::t("the change could not be written down")))?;
    let mut request = Request::post(format!("{}/Items/{}", server.address, encode(id)), headers(&server.client, Some(key)), Some(("application/json".into(), body)));
    request.timeout = WRITE_TIME;
    let reply = net.send(&request).map_err(Failure::Unreachable)?;
    match reply.status {
        200..=299 => Ok(()),
        status => Err(refused(status)),
    }
}

pub(super) fn upload_image(net: &dyn Net, server: &Server, key: &Credential, id: &str, bytes: &[u8], mime: &str) -> Result<(), Failure> {
    let url = format!("{}/Items/{}/Images/Primary", server.address, encode(id));
    let mut request = Request::post(url, headers(&server.client, Some(key)), Some((mime.to_string(), base64(bytes).into_bytes())));
    request.timeout = IMAGE_TIME;
    let reply = net.send(&request).map_err(Failure::Unreachable)?;
    match reply.status {
        200..=299 => Ok(()),
        status => Err(refused(status)),
    }
}

/// Standard base64, with padding.
pub(super) fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16) | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8) | u32::from(*chunk.get(2).unwrap_or(&0));
        for at in 0..4 {
            if at <= chunk.len() {
                out.push(ALPHABET[((n >> (18 - 6 * at)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}
