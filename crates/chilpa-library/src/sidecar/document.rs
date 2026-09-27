//! The sidecar as a document: what it holds and how it is written down.

use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// What the file calls itself. Anything else is not a sidecar.
pub const FORMAT: &str = "chilpa-sidecar";

/// The version this code writes.
pub const VERSION: u32 = 1;

/// An album's fields, by the names the editor gives them, in its order.
pub const ALBUM_FIELDS: [&str; 11] = [
    "title",
    "album_artist",
    "album_artists",
    "compilation",
    "date",
    "genres",
    "catalog_number",
    "musicbrainz_release_id",
    "musicbrainz_release_group_id",
    "musicbrainz_album_artist_ids",
    "discogs_release_id",
];

/// A track's.
pub const TRACK_FIELDS: [&str; 10] = [
    "title",
    "artist_credit",
    "artists",
    "track_number",
    "disc_number",
    "composers",
    "performers",
    "musicbrainz_recording_id",
    "musicbrainz_track_id",
    "musicbrainz_artist_ids",
];

/// A field of a sidecar: not there, there, or there to say "empty".
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub enum Override<T> {
    #[default]
    Absent,
    Cleared,
    Value(T),
}

impl<T> Override<T> {
    pub fn is_absent(&self) -> bool {
        matches!(self, Override::Absent)
    }

    pub fn value(&self) -> Option<&T> {
        match self {
            Override::Value(v) => Some(v),
            _ => None,
        }
    }

    fn map<U>(&self, f: impl FnOnce(&T) -> U) -> Override<U> {
        match self {
            Override::Absent => Override::Absent,
            Override::Cleared => Override::Cleared,
            Override::Value(v) => Override::Value(f(v)),
        }
    }
}

/// Several values as the editor shows them: on one line, a semicolon apart.
pub fn join(values: &[String]) -> String {
    values.join("; ")
}

/// What somebody typed as several values: split at semicolons and line
/// ends, trimmed, without the empty ones or the repeats, whatever their
/// case. Never at a comma, a slash or an ampersand: "Crosby, Stills & Nash"
/// is one name.
pub fn split(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for part in text.split([';', '\n', '\r']) {
        let part = part.trim();
        if !part.is_empty() && !out.iter().any(|seen| seen.to_lowercase() == part.to_lowercase()) {
            out.push(part.to_string());
        }
    }
    out
}

#[derive(Debug)]
pub enum SidecarError {
    Io(std::io::Error),
    /// Not JSON, or not a sidecar.
    Unreadable(String),
    /// Written by a newer Chilpa: read, never rewritten.
    Newer(u32),
}

impl std::fmt::Display for SidecarError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SidecarError::Io(e) => write!(f, "{e}"),
            SidecarError::Unreadable(why) => write!(f, "{why}"),
            SidecarError::Newer(version) => write!(f, "version {version}"),
        }
    }
}

// Reading: what is understood is taken out of the map, and what is left in
// it is what this version does not know.

fn take_text(map: &mut Map<String, Value>, key: &str) -> Override<String> {
    match map.get(key) {
        Some(Value::Null) => {
            map.remove(key);
            Override::Cleared
        }
        Some(Value::String(_)) => match map.remove(key) {
            Some(Value::String(text)) => Override::Value(text),
            _ => Override::Absent,
        },
        _ => Override::Absent,
    }
}

fn take_list(map: &mut Map<String, Value>, key: &str) -> Override<Vec<String>> {
    let texts = match map.get(key) {
        Some(Value::Null) => Some(Vec::new()),
        Some(Value::Array(items)) => items.iter().map(|i| i.as_str().map(str::to_string)).collect::<Option<Vec<String>>>(),
        _ => None,
    };
    let Some(texts) = texts else { return Override::Absent };
    map.remove(key);
    if texts.is_empty() {
        Override::Cleared
    } else {
        Override::Value(texts)
    }
}

fn take_number(map: &mut Map<String, Value>, key: &str) -> Override<u32> {
    let number = match map.get(key) {
        Some(Value::Null) => Some(None),
        Some(Value::Number(n)) => n.as_u64().and_then(|n| u32::try_from(n).ok()).filter(|n| *n > 0).map(Some),
        _ => None,
    };
    let Some(number) = number else { return Override::Absent };
    map.remove(key);
    match number {
        Some(n) => Override::Value(n),
        None => Override::Cleared,
    }
}

fn take_flag(map: &mut Map<String, Value>, key: &str) -> Override<bool> {
    let flag = match map.get(key) {
        Some(Value::Null) => Some(None),
        Some(Value::Bool(b)) => Some(Some(*b)),
        _ => None,
    };
    let Some(flag) = flag else { return Override::Absent };
    map.remove(key);
    match flag {
        Some(b) => Override::Value(b),
        None => Override::Cleared,
    }
}

fn put_text(map: &mut Map<String, Value>, key: &str, field: &Override<String>) {
    match field {
        Override::Absent => {}
        Override::Cleared => {
            map.insert(key.to_string(), Value::Null);
        }
        Override::Value(text) => {
            map.insert(key.to_string(), Value::String(text.clone()));
        }
    }
}

fn put_list(map: &mut Map<String, Value>, key: &str, field: &Override<Vec<String>>) {
    match field {
        Override::Absent => {}
        Override::Cleared => {
            map.insert(key.to_string(), Value::Array(Vec::new()));
        }
        Override::Value(texts) => {
            map.insert(key.to_string(), Value::Array(texts.iter().cloned().map(Value::String).collect()));
        }
    }
}

fn put_number(map: &mut Map<String, Value>, key: &str, field: &Override<u32>) {
    match field {
        Override::Absent => {}
        Override::Cleared => {
            map.insert(key.to_string(), Value::Null);
        }
        Override::Value(n) => {
            map.insert(key.to_string(), Value::from(*n));
        }
    }
}

fn put_flag(map: &mut Map<String, Value>, key: &str, field: &Override<bool>) {
    match field {
        Override::Absent => {}
        Override::Cleared => {
            map.insert(key.to_string(), Value::Null);
        }
        Override::Value(b) => {
            map.insert(key.to_string(), Value::Bool(*b));
        }
    }
}

/// A list as the editor takes it: nothing typed is the list cleared.
fn list_of(value: Override<String>) -> Override<Vec<String>> {
    match value {
        Override::Absent => Override::Absent,
        Override::Cleared => Override::Cleared,
        Override::Value(text) => {
            let parts = split(&text);
            if parts.is_empty() {
                Override::Cleared
            } else {
                Override::Value(parts)
            }
        }
    }
}

/// What a sidecar lays over a record.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Album {
    pub title: Override<String>,
    pub album_artist: Override<String>,
    pub album_artists: Override<Vec<String>>,
    pub compilation: Override<bool>,
    pub date: Override<String>,
    pub genres: Override<Vec<String>>,
    pub catalog_number: Override<String>,
    pub musicbrainz_release_id: Override<String>,
    pub musicbrainz_release_group_id: Override<String>,
    pub musicbrainz_album_artist_ids: Override<Vec<String>>,
    pub discogs_release_id: Override<String>,
    /// The name of the cover's file, which sits beside the sidecar.
    pub cover: Override<String>,
    /// Keys this version does not know, kept as they came.
    pub unknown: Map<String, Value>,
}

impl Album {
    const TEXTS: [&'static str; 8] = [
        "title",
        "album_artist",
        "date",
        "catalog_number",
        "musicbrainz_release_id",
        "musicbrainz_release_group_id",
        "discogs_release_id",
        "cover",
    ];
    const LISTS: [&'static str; 3] = ["album_artists", "genres", "musicbrainz_album_artist_ids"];

    fn text(&self, name: &str) -> Option<&Override<String>> {
        Some(match name {
            "title" => &self.title,
            "album_artist" => &self.album_artist,
            "date" => &self.date,
            "catalog_number" => &self.catalog_number,
            "musicbrainz_release_id" => &self.musicbrainz_release_id,
            "musicbrainz_release_group_id" => &self.musicbrainz_release_group_id,
            "discogs_release_id" => &self.discogs_release_id,
            "cover" => &self.cover,
            _ => return None,
        })
    }

    fn text_mut(&mut self, name: &str) -> Option<&mut Override<String>> {
        Some(match name {
            "title" => &mut self.title,
            "album_artist" => &mut self.album_artist,
            "date" => &mut self.date,
            "catalog_number" => &mut self.catalog_number,
            "musicbrainz_release_id" => &mut self.musicbrainz_release_id,
            "musicbrainz_release_group_id" => &mut self.musicbrainz_release_group_id,
            "discogs_release_id" => &mut self.discogs_release_id,
            "cover" => &mut self.cover,
            _ => return None,
        })
    }

    fn list(&self, name: &str) -> Option<&Override<Vec<String>>> {
        Some(match name {
            "album_artists" => &self.album_artists,
            "genres" => &self.genres,
            "musicbrainz_album_artist_ids" => &self.musicbrainz_album_artist_ids,
            _ => return None,
        })
    }

    fn list_mut(&mut self, name: &str) -> Option<&mut Override<Vec<String>>> {
        Some(match name {
            "album_artists" => &mut self.album_artists,
            "genres" => &mut self.genres,
            "musicbrainz_album_artist_ids" => &mut self.musicbrainz_album_artist_ids,
            _ => return None,
        })
    }

    /// A field as the editor shows it: a list on one line, a flag as
    /// "true" or "false". A name that is not a field is `Absent`.
    pub fn get(&self, name: &str) -> Override<String> {
        if let Some(text) = self.text(name) {
            return text.clone();
        }
        if let Some(list) = self.list(name) {
            return list.map(|values| join(values));
        }
        if name == "compilation" {
            return self.compilation.map(|flag| flag.to_string());
        }
        Override::Absent
    }

    /// Sets a field from what the editor holds. False for a name that is
    /// not a field.
    pub fn put(&mut self, name: &str, value: Override<String>) -> bool {
        if let Some(text) = self.text_mut(name) {
            *text = value;
            return true;
        }
        if let Some(list) = self.list_mut(name) {
            *list = list_of(value);
            return true;
        }
        if name == "compilation" {
            self.compilation = value.map(|text| text == "true");
            return true;
        }
        false
    }

    pub fn is_empty(&self) -> bool {
        self.unknown.is_empty()
            && self.compilation.is_absent()
            && Self::TEXTS.iter().all(|name| self.text(name).is_some_and(Override::is_absent))
            && Self::LISTS.iter().all(|name| self.list(name).is_some_and(Override::is_absent))
    }

    fn from_map(mut map: Map<String, Value>) -> Self {
        let mut album = Album::default();
        for name in Self::TEXTS {
            if let Some(field) = album.text_mut(name) {
                *field = take_text(&mut map, name);
            }
        }
        for name in Self::LISTS {
            if let Some(field) = album.list_mut(name) {
                *field = take_list(&mut map, name);
            }
        }
        album.compilation = take_flag(&mut map, "compilation");
        album.unknown = map;
        album
    }

    fn to_map(&self) -> Map<String, Value> {
        let mut map = self.unknown.clone();
        for name in Self::TEXTS {
            if let Some(field) = self.text(name) {
                put_text(&mut map, name, field);
            }
        }
        for name in Self::LISTS {
            if let Some(field) = self.list(name) {
                put_list(&mut map, name, field);
            }
        }
        put_flag(&mut map, "compilation", &self.compilation);
        map
    }
}

/// What a sidecar lays over one track.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Track {
    pub title: Override<String>,
    pub artist_credit: Override<String>,
    pub artists: Override<Vec<String>>,
    pub track_number: Override<u32>,
    pub disc_number: Override<u32>,
    pub composers: Override<Vec<String>>,
    pub performers: Override<Vec<String>>,
    pub musicbrainz_recording_id: Override<String>,
    pub musicbrainz_track_id: Override<String>,
    pub musicbrainz_artist_ids: Override<Vec<String>>,
    pub unknown: Map<String, Value>,
}

impl Track {
    const TEXTS: [&'static str; 4] = ["title", "artist_credit", "musicbrainz_recording_id", "musicbrainz_track_id"];
    const LISTS: [&'static str; 4] = ["artists", "composers", "performers", "musicbrainz_artist_ids"];
    const NUMBERS: [&'static str; 2] = ["track_number", "disc_number"];

    fn text(&self, name: &str) -> Option<&Override<String>> {
        Some(match name {
            "title" => &self.title,
            "artist_credit" => &self.artist_credit,
            "musicbrainz_recording_id" => &self.musicbrainz_recording_id,
            "musicbrainz_track_id" => &self.musicbrainz_track_id,
            _ => return None,
        })
    }

    fn text_mut(&mut self, name: &str) -> Option<&mut Override<String>> {
        Some(match name {
            "title" => &mut self.title,
            "artist_credit" => &mut self.artist_credit,
            "musicbrainz_recording_id" => &mut self.musicbrainz_recording_id,
            "musicbrainz_track_id" => &mut self.musicbrainz_track_id,
            _ => return None,
        })
    }

    fn list(&self, name: &str) -> Option<&Override<Vec<String>>> {
        Some(match name {
            "artists" => &self.artists,
            "composers" => &self.composers,
            "performers" => &self.performers,
            "musicbrainz_artist_ids" => &self.musicbrainz_artist_ids,
            _ => return None,
        })
    }

    fn list_mut(&mut self, name: &str) -> Option<&mut Override<Vec<String>>> {
        Some(match name {
            "artists" => &mut self.artists,
            "composers" => &mut self.composers,
            "performers" => &mut self.performers,
            "musicbrainz_artist_ids" => &mut self.musicbrainz_artist_ids,
            _ => return None,
        })
    }

    fn number(&self, name: &str) -> Option<&Override<u32>> {
        Some(match name {
            "track_number" => &self.track_number,
            "disc_number" => &self.disc_number,
            _ => return None,
        })
    }

    fn number_mut(&mut self, name: &str) -> Option<&mut Override<u32>> {
        Some(match name {
            "track_number" => &mut self.track_number,
            "disc_number" => &mut self.disc_number,
            _ => return None,
        })
    }

    /// A field as the editor shows it. A name that is not a field is `Absent`.
    pub fn get(&self, name: &str) -> Override<String> {
        if let Some(text) = self.text(name) {
            return text.clone();
        }
        if let Some(list) = self.list(name) {
            return list.map(|values| join(values));
        }
        if let Some(number) = self.number(name) {
            return number.map(|n| n.to_string());
        }
        Override::Absent
    }

    /// Sets a field from what the editor holds. False for a name that is
    /// not a field, and for a number that is not one.
    pub fn put(&mut self, name: &str, value: Override<String>) -> bool {
        if let Some(text) = self.text_mut(name) {
            *text = value;
            return true;
        }
        if let Some(list) = self.list_mut(name) {
            *list = list_of(value);
            return true;
        }
        if let Some(number) = self.number_mut(name) {
            *number = match value {
                Override::Absent => Override::Absent,
                Override::Cleared => Override::Cleared,
                Override::Value(text) => match text.trim().parse::<u32>() {
                    Ok(n) if n > 0 => Override::Value(n),
                    _ => return false,
                },
            };
            return true;
        }
        false
    }

    pub fn is_empty(&self) -> bool {
        self.unknown.is_empty()
            && Self::TEXTS.iter().all(|name| self.text(name).is_some_and(Override::is_absent))
            && Self::LISTS.iter().all(|name| self.list(name).is_some_and(Override::is_absent))
            && Self::NUMBERS.iter().all(|name| self.number(name).is_some_and(Override::is_absent))
    }

    fn from_map(mut map: Map<String, Value>) -> Self {
        let mut track = Track::default();
        for name in Self::TEXTS {
            if let Some(field) = track.text_mut(name) {
                *field = take_text(&mut map, name);
            }
        }
        for name in Self::LISTS {
            if let Some(field) = track.list_mut(name) {
                *field = take_list(&mut map, name);
            }
        }
        for name in Self::NUMBERS {
            if let Some(field) = track.number_mut(name) {
                *field = take_number(&mut map, name);
            }
        }
        track.unknown = map;
        track
    }

    fn to_map(&self) -> Map<String, Value> {
        let mut map = self.unknown.clone();
        for name in Self::TEXTS {
            if let Some(field) = self.text(name) {
                put_text(&mut map, name, field);
            }
        }
        for name in Self::LISTS {
            if let Some(field) = self.list(name) {
                put_list(&mut map, name, field);
            }
        }
        for name in Self::NUMBERS {
            if let Some(field) = self.number(name) {
                put_number(&mut map, name, field);
            }
        }
        map
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct Sidecar {
    pub version: u32,
    /// In unix seconds.
    pub updated_at: i64,
    pub album: Album,
    /// By the name each track goes by in its home.
    pub tracks: BTreeMap<String, Track>,
    /// Keys this version does not know, kept as they came.
    pub unknown: Map<String, Value>,
    /// Read out of a `.qbz.json`, which is never written.
    pub imported: bool,
}

/// Lays an override over what a file says.
fn lay<T: Clone>(over: &Override<T>, onto: &mut Option<T>) {
    match over {
        Override::Absent => {}
        Override::Cleared => *onto = None,
        Override::Value(v) => *onto = Some(v.clone()),
    }
}

impl Sidecar {
    pub fn new(now: i64) -> Self {
        Self {
            version: VERSION,
            updated_at: now,
            album: Album::default(),
            tracks: BTreeMap::new(),
            unknown: Map::new(),
            imported: false,
        }
    }

    /// Nothing in it lays over anything.
    pub fn is_empty(&self) -> bool {
        self.unknown.is_empty() && self.album.is_empty() && self.tracks.values().all(Track::is_empty)
    }

    /// The track that goes by that name, made if it is not there.
    pub fn track_mut(&mut self, key: &str) -> &mut Track {
        self.tracks.entry(key.to_string()).or_default()
    }

    pub fn from_json(bytes: &[u8]) -> Result<Self, SidecarError> {
        let root: Value = serde_json::from_slice(bytes).map_err(|e| SidecarError::Unreadable(e.to_string()))?;
        let Value::Object(mut root) = root else {
            return Err(SidecarError::Unreadable("not an object".to_string()));
        };
        if root.remove("format").as_ref().and_then(Value::as_str) != Some(FORMAT) {
            return Err(SidecarError::Unreadable("not a Chilpa sidecar".to_string()));
        }
        let version = root.remove("version").as_ref().and_then(Value::as_u64).and_then(|v| u32::try_from(v).ok()).unwrap_or(0);
        if version == 0 {
            return Err(SidecarError::Unreadable("it does not say its version".to_string()));
        }
        let updated_at = root.remove("updated_at").as_ref().and_then(Value::as_i64).unwrap_or(0);
        let album = match root.remove("album") {
            Some(Value::Object(map)) => Album::from_map(map),
            _ => Album::default(),
        };
        let mut tracks = BTreeMap::new();
        if let Some(Value::Object(map)) = root.remove("tracks") {
            for (key, value) in map {
                if let Value::Object(fields) = value {
                    tracks.insert(key, Track::from_map(fields));
                }
            }
        }
        Ok(Self { version, updated_at, album, tracks, unknown: root, imported: false })
    }

    /// As it is written to disk: indented, so it can be read by eye.
    pub fn to_json(&self) -> Vec<u8> {
        let mut root = self.unknown.clone();
        root.insert("format".to_string(), Value::String(FORMAT.to_string()));
        root.insert("version".to_string(), Value::from(self.version));
        root.insert("updated_at".to_string(), Value::from(self.updated_at));
        root.insert("album".to_string(), Value::Object(self.album.to_map()));
        let tracks: Map<String, Value> = self
            .tracks
            .iter()
            .filter(|(_, track)| !track.is_empty())
            .map(|(key, track)| (key.clone(), Value::Object(track.to_map())))
            .collect();
        root.insert("tracks".to_string(), Value::Object(tracks));
        let mut bytes = serde_json::to_vec_pretty(&Value::Object(root)).unwrap_or_default();
        bytes.push(b'\n');
        bytes
    }

    /// Lays it over what a file says, for the fields the library keeps.
    /// `key` is what the track goes by in its home.
    pub fn overlay(&self, tags: &mut chilpa_tags::Tags, key: &str) {
        lay(&self.album.title, &mut tags.album);
        lay(&self.album.album_artist, &mut tags.album_artist);
        lay(&self.album.date, &mut tags.date);
        match &self.album.genres {
            Override::Absent => {}
            Override::Cleared => tags.genres.clear(),
            Override::Value(genres) => tags.genres = genres.clone(),
        }
        let Some(track) = self.tracks.get(key) else { return };
        lay(&track.title, &mut tags.title);
        match &track.artist_credit {
            Override::Absent => {}
            Override::Cleared => tags.artists.clear(),
            Override::Value(credit) => tags.artists = vec![credit.clone()],
        }
        for (over, onto) in [(&track.track_number, &mut tags.track_number), (&track.disc_number, &mut tags.disc_number)] {
            match over {
                Override::Absent => {}
                Override::Cleared => *onto = None,
                // A number that does not fit the column is not one.
                Override::Value(n) => {
                    if let Ok(n) = u16::try_from(*n) {
                        *onto = Some(n);
                    }
                }
            }
        }
        lay(&track.musicbrainz_recording_id, &mut tags.musicbrainz_recording_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(json: &str) -> Sidecar {
        Sidecar::from_json(json.as_bytes()).unwrap()
    }

    #[test]
    fn an_absent_key_a_null_and_a_value_are_three_things() {
        let s = read(r#"{"format":"chilpa-sidecar","version":1,"album":{"title":"A","catalog_number":null},"tracks":{}}"#);
        assert_eq!(s.album.title, Override::Value("A".to_string()));
        assert_eq!(s.album.catalog_number, Override::Cleared);
        assert_eq!(s.album.date, Override::Absent);

        let again = read(std::str::from_utf8(&s.to_json()).unwrap());
        assert_eq!(again.album, s.album, "and they come back as they went");
        let written: Value = serde_json::from_slice(&s.to_json()).unwrap();
        assert!(written["album"].get("date").is_none(), "what is absent is not written");
        assert!(written["album"]["catalog_number"].is_null());
    }

    #[test]
    fn an_empty_list_clears_and_a_missing_one_does_not() {
        let s = read(r#"{"format":"chilpa-sidecar","version":1,"album":{"genres":[]},"tracks":{"a.flac":{"composers":["One","Two"]}}}"#);
        assert_eq!(s.album.genres, Override::Cleared);
        assert_eq!(s.album.album_artists, Override::Absent);
        assert_eq!(s.tracks["a.flac"].composers, Override::Value(vec!["One".to_string(), "Two".to_string()]));
        let written: Value = serde_json::from_slice(&s.to_json()).unwrap();
        assert_eq!(written["album"]["genres"], serde_json::json!([]));
    }

    #[test]
    fn unknown_keys_survive_a_rewrite() {
        let s = read(
            r#"{"format":"chilpa-sidecar","version":1,"later":{"x":1},
                "album":{"title":"A","mood":"calm"},
                "tracks":{"a.flac":{"title":"T","rating":5}}}"#,
        );
        let written: Value = serde_json::from_slice(&s.to_json()).unwrap();
        assert_eq!(written["later"]["x"], 1);
        assert_eq!(written["album"]["mood"], "calm");
        assert_eq!(written["tracks"]["a.flac"]["rating"], 5);
        assert_eq!(written["album"]["title"], "A");
    }

    #[test]
    fn a_value_of_the_wrong_kind_is_kept_and_not_understood() {
        let s = read(r#"{"format":"chilpa-sidecar","version":1,"album":{"title":7},"tracks":{}}"#);
        assert_eq!(s.album.title, Override::Absent);
        let written: Value = serde_json::from_slice(&s.to_json()).unwrap();
        assert_eq!(written["album"]["title"], 7, "it is somebody's, and it is left as it was");
    }

    #[test]
    fn what_is_not_a_sidecar_is_unreadable() {
        for json in ["junk", "[]", r#"{"version":1}"#, r#"{"format":"something-else","version":1}"#, r#"{"format":"chilpa-sidecar"}"#] {
            assert!(matches!(Sidecar::from_json(json.as_bytes()), Err(SidecarError::Unreadable(_))), "{json}");
        }
    }

    #[test]
    fn a_newer_sidecar_is_read() {
        let s = read(r#"{"format":"chilpa-sidecar","version":2,"album":{"title":"A"},"tracks":{}}"#);
        assert_eq!(s.version, 2);
        assert_eq!(s.album.title, Override::Value("A".to_string()));
    }

    #[test]
    fn what_is_typed_as_several_is_several() {
        assert_eq!(split("Rock; Blues;rock ;\n Soul"), ["Rock", "Blues", "Soul"]);
        assert_eq!(split("Crosby, Stills & Nash"), ["Crosby, Stills & Nash"], "a comma is part of a name");
        assert_eq!(split("R&B/Soul"), ["R&B/Soul"]);
        assert!(split(" ; ;").is_empty());
        assert_eq!(join(&["A".to_string(), "B".to_string()]), "A; B");
    }

    #[test]
    fn a_field_goes_in_and_comes_out_as_the_editor_holds_it() {
        let mut album = Album::default();
        assert!(album.put("genres", Override::Value("Rock; Blues".to_string())));
        assert!(album.put("compilation", Override::Value("true".to_string())));
        assert!(album.put("catalog_number", Override::Cleared));
        assert!(!album.put("not_a_field", Override::Cleared));
        assert_eq!(album.genres, Override::Value(vec!["Rock".to_string(), "Blues".to_string()]));
        assert_eq!(album.get("genres"), Override::Value("Rock; Blues".to_string()));
        assert_eq!(album.get("compilation"), Override::Value("true".to_string()));
        assert_eq!(album.get("catalog_number"), Override::Cleared);
        assert_eq!(album.get("date"), Override::Absent);

        let mut track = Track::default();
        assert!(track.put("track_number", Override::Value("7".to_string())));
        assert!(!track.put("disc_number", Override::Value("zero".to_string())));
        assert!(!track.put("disc_number", Override::Value("0".to_string())));
        assert_eq!(track.get("track_number"), Override::Value("7".to_string()));
        assert_eq!(track.get("disc_number"), Override::Absent);
    }

    #[test]
    fn every_field_the_editor_names_is_one() {
        let mut album = Album::default();
        for name in ALBUM_FIELDS {
            assert!(album.put(name, Override::Cleared), "{name}");
        }
        let mut track = Track::default();
        for name in TRACK_FIELDS {
            assert!(track.put(name, Override::Cleared), "{name}");
        }
    }

    #[test]
    fn a_sidecar_with_nothing_in_it_is_empty() {
        let mut s = Sidecar::new(0);
        assert!(s.is_empty());
        s.track_mut("a.flac");
        assert!(s.is_empty(), "a track that lays nothing over is nothing");
        s.track_mut("a.flac").title = Override::Value("T".to_string());
        assert!(!s.is_empty());
        let written: Value = serde_json::from_slice(&s.to_json()).unwrap();
        assert_eq!(written["tracks"].as_object().unwrap().len(), 1);
    }

    #[test]
    fn the_overlay_touches_only_what_it_overrides() {
        let mut tags = chilpa_tags::Tags {
            title: Some("File title".into()),
            artists: vec!["One".into(), "Two".into()],
            album: Some("File album".into()),
            album_artist: Some("File artist".into()),
            genres: vec!["Rock".into()],
            date: Some("2019-03-15".into()),
            track_number: Some(3),
            disc_number: Some(1),
            isrc: Some("GBAYE9200070".into()),
            ..chilpa_tags::Tags::default()
        };
        let before = tags.clone();
        let mut s = Sidecar::new(0);
        s.album.title = Override::Value("Another album".into());
        s.album.genres = Override::Cleared;
        let track = s.track_mut("a.flac");
        track.artist_credit = Override::Value("One feat. Two".into());
        track.disc_number = Override::Cleared;
        track.composers = Override::Value(vec!["Nobody the library keeps".into()]);

        s.overlay(&mut tags, "a.flac");
        assert_eq!(tags.album.as_deref(), Some("Another album"));
        assert!(tags.genres.is_empty());
        assert_eq!(tags.artists, ["One feat. Two"]);
        assert_eq!(tags.disc_number, None);
        assert_eq!(tags.title, before.title);
        assert_eq!(tags.album_artist, before.album_artist);
        assert_eq!(tags.date, before.date, "the date is whole and is left whole");
        assert_eq!(tags.track_number, before.track_number);
        assert_eq!(tags.isrc, before.isrc);

        // Another track of the same home takes the album's and not the track's.
        let mut other = before.clone();
        s.overlay(&mut other, "b.flac");
        assert_eq!(other.album.as_deref(), Some("Another album"));
        assert_eq!(other.artists, before.artists);
    }
}
