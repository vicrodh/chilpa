//! Plex's items, read and changed (Plex Media Server 1.43, measured on the
//! lab library).
//!
//! - An item is read with `GET /library/metadata/{ratingKey}`, several at
//!   once with their keys joined by commas.
//! - A change is `PUT /library/sections/{section}/all?type=&id=` with a
//!   `<field>.value` for each field, as python-plexapi sends it; each field
//!   written goes with `<field>.locked=1`. A record's new title also names
//!   its artist (`artist.id.value`), as python-plexapi does.
//! - A tag (`genre[0].tag.tag=...`) is added to the ones the item has, not
//!   put in their place (measured): the old ones that are not in the new
//!   list are taken off first, with `genre[].tag.tag-=a,b`.
//! - A track reads its record's title, artist and year, and keeps genres
//!   of its own (measured: a record's new genre does not reach them).
//! - A record's date is two fields: `originallyAvailableAt` for a whole day
//!   and `year`. A date without its day writes the year and empties the
//!   day (measured).
//! - `POST /library/metadata/{ratingKey}/posters` with the image's bytes
//!   adds a poster, shows it and locks it (measured); a track takes none.
//! - Nothing here sends a `DELETE`.

use serde_json::Value;
use std::time::Duration;

use super::{refused, whole_day, year_of, Edit, Entity, Held, RECORDING};
use crate::model::{Credential, Failure, Server};
use crate::net::{Net, Request};
use crate::plex::{ask, headers, server_token};
use crate::text::{encode, id, number, string};

const WRITE_TIME: Duration = Duration::from_secs(20);
const IMAGE_TIME: Duration = Duration::from_secs(60);

/// Plex's type number for each kind of item.
fn type_of(entity: Entity) -> u8 {
    match entity {
        Entity::Artist => 8,
        Entity::Album => 9,
        Entity::Track => 10,
    }
}

/// Plex's names of the fields it locks, by the editor's.
const LOCKS: [(&str, &str); 9] = [
    ("title", "title"),
    ("artist_credit", "originalTitle"),
    ("track_number", "index"),
    ("disc_number", "parentIndex"),
    ("date", "originallyAvailableAt"),
    ("date", "year"),
    ("genres", "genre"),
    ("label", "studio"),
    ("summary", "summary"),
];

fn tags(item: &Value, key: &str) -> Vec<String> {
    item.get(key).and_then(Value::as_array).into_iter().flatten().filter_map(|t| string(t, "tag")).collect()
}

/// What one item says.
fn read(item: &Value) -> Held {
    let kind = string(item, "type").unwrap_or_default();
    let album_artist = string(item, "grandparentTitle").unwrap_or_default();
    let date = match (string(item, "originallyAvailableAt"), number(item, "year").or_else(|| number(item, "parentYear"))) {
        (Some(day), _) if day.len() >= 10 && kind == "album" => day[..10].to_string(),
        (_, Some(year)) => format!("{year:04}"),
        _ => String::new(),
    };
    let mbid = item
        .get("Guid")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|g| string(g, "id"))
        .find_map(|g| g.strip_prefix("mbid://").map(str::to_lowercase));
    let locked_names: Vec<String> = item
        .get("Field")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|f| f.get("locked").and_then(Value::as_bool) == Some(true))
        .filter_map(|f| string(f, "name"))
        .collect();
    let mut locked: Vec<String> = Vec::new();
    for (ours, theirs) in LOCKS {
        if locked_names.iter().any(|n| n == theirs) && !locked.iter().any(|l| l == ours) {
            locked.push(ours.to_string());
        }
    }
    let mut held = Held {
        title: string(item, "title").unwrap_or_default(),
        genres: tags(item, "Genre"),
        summary: string(item, "summary").unwrap_or_default(),
        date,
        locked,
        ..Held::default()
    };
    match kind.as_str() {
        "album" => {
            held.album_artist = string(item, "parentTitle").unwrap_or_default();
            held.label = string(item, "studio").unwrap_or_default();
        }
        "track" => {
            held.album = string(item, "parentTitle").unwrap_or_default();
            held.artist = string(item, "originalTitle").unwrap_or_else(|| album_artist.clone());
            held.album_artist = album_artist;
            held.track_number = number(item, "index").map(|n| n as u32);
            held.disc_number = number(item, "parentIndex").map(|n| n as u32);
            if let Some(mbid) = mbid {
                held.ids.insert(RECORDING.to_string(), mbid);
            }
        }
        _ => {}
    }
    held
}

fn items(found: &Value) -> Vec<Value> {
    found.get("MediaContainer").and_then(|c| c.get("Metadata")).and_then(Value::as_array).cloned().unwrap_or_default()
}

fn item(net: &dyn Net, server: &Server, key: &Credential, id: &str) -> Result<Value, Failure> {
    let url = format!("{}/library/metadata/{}?includeGuids=1", server.address, encode(id));
    let found = ask(net, Request::get(url, headers(&server.client, server_token(key))))?;
    items(&found).into_iter().next().ok_or_else(|| Failure::Gone(chilpa_i18n::t("the server no longer has it")))
}

pub(super) fn held(net: &dyn Net, server: &Server, key: &Credential, _entity: Entity, id: &str) -> Result<Held, Failure> {
    item(net, server, key, id).map(|found| read(&found))
}

pub(super) fn held_many(net: &dyn Net, server: &Server, key: &Credential, ids: &[String]) -> Result<Vec<(String, Held)>, Failure> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<String> = ids.iter().map(|id| encode(id)).collect();
    let url = format!("{}/library/metadata/{}?includeGuids=1", server.address, ids.join(","));
    let found = ask(net, Request::get(url, headers(&server.client, server_token(key))))?;
    Ok(items(&found).iter().filter_map(|item| Some((id(item, "ratingKey")?, read(item)))).collect())
}

/// The tags of an item's that a new list leaves out, as one removal; `None`
/// when it leaves none out.
pub(super) fn removal(edit: &Edit, before: &Held) -> Option<Vec<(String, String)>> {
    let genres = edit.genres.as_ref()?;
    let gone: Vec<String> = before.genres.iter().filter(|old| !genres.iter().any(|new| new.trim().eq_ignore_ascii_case(old))).map(|g| encode(g)).collect();
    (!gone.is_empty()).then(|| vec![("genre[].tag.tag-".to_string(), gone.join(",")), ("genre.locked".to_string(), "1".to_string())])
}

/// The query that carries a change, as `name=value` pairs, values encoded.
/// The genres are the ones to add; `removal` takes the others off.
pub(super) fn query(edit: &Edit, entity: Entity, before: &Item) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    if let (Some(_), Entity::Album, Some(artist)) = (&edit.title, entity, &before.artist) {
        out.push(("artist.id.value".into(), encode(artist)));
    }
    let mut field = |name: &str, value: &str| {
        out.push((format!("{name}.value"), encode(value.trim())));
        out.push((format!("{name}.locked"), "1".into()));
    };
    if let Some(title) = &edit.title {
        field("title", title);
    }
    if let Some(artist) = &edit.artist {
        field("originalTitle", artist);
    }
    if let Some(number) = edit.track_number {
        field("index", &number.map(|n| n.to_string()).unwrap_or_default());
    }
    if let Some(number) = edit.disc_number {
        field("parentIndex", &number.map(|n| n.to_string()).unwrap_or_default());
    }
    if let Some(date) = &edit.date {
        field("year", &year_of(date).map(|y| y.to_string()).unwrap_or_default());
        field("originallyAvailableAt", if whole_day(date) { date } else { "" });
    }
    if let Some(label) = &edit.label {
        field("studio", label);
    }
    if let Some(summary) = &edit.summary {
        field("summary", summary);
    }
    if let Some(genres) = &edit.genres {
        let new: Vec<&String> = genres.iter().filter(|g| !before.held.genres.iter().any(|old| old.eq_ignore_ascii_case(g.trim()))).collect();
        for (at, genre) in new.iter().enumerate() {
            out.push((format!("genre[{at}].tag.tag"), encode(genre.trim())));
        }
        out.push(("genre.locked".into(), "1".into()));
    }
    out
}

/// What a change needs to know of the item it goes to.
pub(super) struct Item {
    pub held: Held,
    /// Its library's section.
    pub section: String,
    /// A record's artist, by its key.
    pub artist: Option<String>,
}

impl Item {
    fn of(found: &Value) -> Option<Item> {
        Some(Item { held: read(found), section: id(found, "librarySectionID")?, artist: id(found, "parentRatingKey") })
    }
}

fn put(net: &dyn Net, server: &Server, key: &Credential, entity: Entity, id: &str, section: &str, pairs: Vec<(String, String)>) -> Result<(), Failure> {
    let tail: Vec<String> = pairs.into_iter().map(|(name, value)| format!("{name}={value}")).collect();
    let url = format!("{}/library/sections/{}/all?type={}&id={}&{}", server.address, encode(section), type_of(entity), encode(id), tail.join("&"));
    let mut request = Request::put(url, headers(&server.client, server_token(key)), None);
    request.timeout = WRITE_TIME;
    let reply = net.send(&request).map_err(Failure::Unreachable)?;
    match reply.status {
        200..=299 => Ok(()),
        status => Err(refused(status)),
    }
}

pub(super) fn write(net: &dyn Net, server: &Server, key: &Credential, entity: Entity, id: &str, edit: &Edit) -> Result<(), Failure> {
    // The item is read first: its library is part of the address, and the
    // genres it has are what a new list takes off or adds to.
    let found = item(net, server, key, id)?;
    let before = Item::of(&found).ok_or_else(|| Failure::Unexpected(chilpa_i18n::t("the server answered without the item's library")))?;
    if let Some(pairs) = removal(edit, &before.held) {
        put(net, server, key, entity, id, &before.section, pairs)?;
    }
    let pairs = query(edit, entity, &before);
    if pairs.is_empty() {
        return Ok(());
    }
    put(net, server, key, entity, id, &before.section, pairs)
}

pub(super) fn upload_image(net: &dyn Net, server: &Server, key: &Credential, id: &str, bytes: &[u8], mime: &str) -> Result<(), Failure> {
    let url = format!("{}/library/metadata/{}/posters", server.address, encode(id));
    let mut request = Request::post(url, headers(&server.client, server_token(key)), Some((mime.to_string(), bytes.to_vec())));
    request.timeout = IMAGE_TIME;
    let reply = net.send(&request).map_err(Failure::Unreachable)?;
    match reply.status {
        200..=299 => Ok(()),
        status => Err(refused(status)),
    }
}
