//! Typed per-file patches. Only explicit changed fields enter FileChanges.
use crate::metadata::fields::{self as existing, Field, Kind};
use chilpa_library::sidecar::Sidecar;
use chilpa_tags::edit::{Change, FileChanges, Snapshot};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::path::Path;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Patch {
    pub field: String,
    pub operation: String,
    #[serde(default)]
    pub value: Value,
    #[serde(default, rename = "groupKey", skip_serializing_if = "Option::is_none")]
    pub group_key: Option<String>,
}

pub(super) fn definition(name: &str) -> Option<(Field, bool)> {
    if name == "discogs_release_id" {
        return None;
    }
    if matches!(name, "track_total" | "disc_total") {
        return Some((
            Field {
                name: if name == "track_total" {
                    "track_total"
                } else {
                    "disc_total"
                },
                kind: Kind::Number,
                required: false,
            },
            false,
        ));
    }
    if name == "album" {
        return Some((existing::ALBUM[0], true));
    }
    existing::TRACK
        .iter()
        .find(|field| field.name == name)
        .copied()
        .map(|field| (field, false))
        .or_else(|| {
            existing::ALBUM
                .iter()
                .find(|field| field.name == name && name != "title")
                .copied()
                .map(|field| (field, true))
        })
}

pub(super) fn label(name: &str) -> String {
    match name {
        "track_total" => return chilpa_i18n::t("Track total"),
        "disc_total" => return chilpa_i18n::t("Disc total"),
        _ => {}
    }
    definition(name)
        .map(|(field, album)| existing::label(field.name, !album))
        .unwrap_or_else(|| chilpa_i18n::t("Metadata"))
}

pub(super) fn definitions() -> Vec<Value> {
    let names = existing::TRACK
        .iter()
        .map(|field| field.name)
        .chain(
            existing::ALBUM
                .iter()
                .filter(|field| field.name != "discogs_release_id")
                .map(|field| {
                    if field.name == "title" {
                        "album"
                    } else {
                        field.name
                    }
                }),
        )
        .chain(["track_total", "disc_total"]);
    names.map(|name| {
        let (field,_) = definition(name).unwrap();
        let list = matches!(field.kind,Kind::List|Kind::UuidList);
        json!({"key":name,"label":label(name),
            "type":match field.kind { Kind::List|Kind::UuidList=>"list",Kind::Number=>"number",Kind::Flag|Kind::Advisory=>"boolean",_=>"text" },
            "operations":match (list, existing::replaceable(field.kind)) {
                (true, _) => vec!["keep","set","clear","append_unique","remove_values","replace_text"],
                (false, true) => vec!["keep","set","clear","replace_text"],
                _ => vec!["keep","set","clear"],
            }})
    }).collect()
}

pub(super) fn values(said: &Snapshot) -> Map<String, Value> {
    json!({"title":said.title,"artist_credit":said.artist_credit.join(", "),"artists":said.artists,
        "album":said.album,"album_artist":said.album_artist,"album_artists":said.album_artists,
        "compilation":said.compilation,"explicit":said.explicit,"date":said.date,"genres":said.genres,
        "catalog_number":said.catalog_number,"label":said.label,"track_number":said.track_number,"disc_number":said.disc_number,
        "track_total":said.track_total,"disc_total":said.disc_total,"disc_subtitle":said.disc_subtitle,"work":said.work,"composers":said.composers,"performers":said.performers,
        "musicbrainz_release_id":said.musicbrainz_release_id,"musicbrainz_release_group_id":said.musicbrainz_release_group_id,
        "musicbrainz_album_artist_ids":said.musicbrainz_album_artist_ids,"musicbrainz_recording_id":said.musicbrainz_recording_id,
        "musicbrainz_track_id":said.musicbrainz_track_id,"musicbrainz_artist_ids":said.musicbrainz_artist_ids})
        .as_object().unwrap().clone()
}

pub(super) fn effective(
    original: &Map<String, Value>,
    sidecar: Option<&Sidecar>,
    key: &str,
) -> Map<String, Value> {
    let mut values = original.clone();
    if let Some(sidecar) = sidecar {
        for (field, value) in sidecar.field_values(key) {
            if definition(&field).is_some() {
                values.insert(field, value);
            }
        }
    }
    values
}

fn text(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Array(values) => values
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join("; "),
        Value::Null => String::new(),
        value => value.to_string(),
    }
}

impl Patch {
    pub fn validate(&self) -> Result<(), String> {
        let (mut field, album) = definition(&self.field)
            .ok_or_else(|| chilpa_i18n::t("This metadata field is not supported."))?;
        let list = matches!(field.kind, Kind::List | Kind::UuidList);
        if !matches!(
            self.operation.as_str(),
            "keep" | "set" | "clear" | "append_unique" | "remove_values" | "replace_text"
        ) || !list && matches!(self.operation.as_str(), "append_unique" | "remove_values")
            || self.operation == "replace_text" && !existing::replaceable(field.kind)
        {
            return Err(chilpa_i18n::t(
                "This operation is not supported for that field.",
            ));
        }
        if matches!(self.operation.as_str(), "keep" | "clear") {
            return Ok(());
        }
        // Find and replace: what is left is each value's own, checked as
        // the preview shows it.
        if self.operation == "replace_text" {
            let shaped = self.value["find"].is_string() && self.value["replace"].is_string()
                && self.value["whole"].is_boolean() && self.value["case"].is_boolean();
            if !shaped {
                return Err(chilpa_i18n::t("The metadata value has the wrong type."));
            }
            if self.value["find"].as_str().unwrap_or_default().is_empty() {
                return Err(chilpa_i18n::t("Type the text to find."));
            }
            return Ok(());
        }
        let shape = match field.kind {
            Kind::List | Kind::UuidList => self
                .value
                .as_array()
                .is_some_and(|values| values.iter().all(Value::is_string)),
            Kind::Number => self.value.as_u64().is_some(),
            Kind::Flag | Kind::Advisory => self.value.is_boolean(),
            _ => self.value.is_string(),
        };
        if !shape {
            return Err(chilpa_i18n::t("The metadata value has the wrong type."));
        }
        field.required = false;
        let invalid = existing::invalid(&field, !album, &text(&self.value));
        if invalid.is_empty() {
            Ok(())
        } else {
            Err(invalid)
        }
    }

    pub fn result(&self, before: &Value) -> Value {
        let identity = |value: &Value| {
            let name = value.as_str().unwrap_or_default();
            if self.field == "genres" {
                crate::discovery::genre_identity(name)
            } else {
                name.trim().to_lowercase()
            }
        };
        match self.operation.as_str() {
            "keep" => before.clone(),
            "clear" => {
                if before.is_array() {
                    json!([])
                } else {
                    Value::Null
                }
            }
            "append_unique" => {
                let mut values = before.as_array().cloned().unwrap_or_default();
                for value in self.value.as_array().into_iter().flatten() {
                    if !values.iter().any(|held| identity(held) == identity(value)) {
                        values.push(value.clone());
                    }
                }
                Value::Array(values)
            }
            "replace_text" => {
                let swap = |text: &str| existing::substitute(text, self.value["find"].as_str().unwrap_or_default(),
                    self.value["replace"].as_str().unwrap_or_default(), self.value["whole"] == true, self.value["case"] == true);
                match before {
                    Value::String(text) => Value::String(swap(text)),
                    Value::Array(values) => Value::Array(values.iter().map(|value| match value.as_str() {
                        Some(text) => Value::String(swap(text)),
                        None => value.clone(),
                    }).filter(|value| value.as_str().is_none_or(|text| !text.trim().is_empty())).collect()),
                    other => other.clone(),
                }
            }
            "remove_values" => Value::Array(
                before
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|held| {
                        !self
                            .value
                            .as_array()
                            .into_iter()
                            .flatten()
                            .any(|value| identity(held) == identity(value))
                    })
                    .cloned()
                    .collect(),
            ),
            _ => self.value.clone(),
        }
    }
}

pub(super) fn write(path: &Path, changes: &Map<String, Value>) -> Result<FileChanges, String> {
    let mut out = FileChanges::none(path.to_path_buf());
    for (name, value) in changes {
        let string = || {
            if value.is_null() || value.as_str() == Some("") {
                Change::Clear
            } else {
                Change::Set(text(value))
            }
        };
        let list = || match value.as_array() {
            Some(values) if !values.is_empty() => Change::Set(
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect(),
            ),
            _ => Change::Clear,
        };
        let number = || {
            value
                .as_u64()
                .and_then(|number| u32::try_from(number).ok())
                .map_or(Change::Clear, Change::Set)
        };
        let flag = || value.as_bool().map_or(Change::Clear, Change::Set);
        match name.as_str() {
            "title" => out.title = string(),
            "album" => out.album = string(),
            "artist_credit" => out.artist_credit = string(),
            "album_artist" => out.album_artist = string(),
            "date" => out.date = string(),
            "catalog_number" => out.catalog_number = string(),
            "label" => out.label = string(),
            "disc_subtitle" => out.disc_subtitle = string(),
            "work" => out.work = string(),
            "musicbrainz_release_id" => out.musicbrainz_release_id = string(),
            "musicbrainz_release_group_id" => out.musicbrainz_release_group_id = string(),
            "musicbrainz_recording_id" => out.musicbrainz_recording_id = string(),
            "musicbrainz_track_id" => out.musicbrainz_track_id = string(),
            "artists" => out.artists = list(),
            "album_artists" => out.album_artists = list(),
            "genres" => out.genres = list(),
            "composers" => out.composers = list(),
            "performers" => out.performers = list(),
            "musicbrainz_album_artist_ids" => out.musicbrainz_album_artist_ids = list(),
            "musicbrainz_artist_ids" => out.musicbrainz_artist_ids = list(),
            "track_number" => out.track_number = number(),
            "disc_number" => out.disc_number = number(),
            "track_total" => out.track_total = number(),
            "disc_total" => out.disc_total = number(),
            "compilation" => out.compilation = flag(),
            "explicit" => out.explicit = flag(),
            _ => return Err(chilpa_i18n::t("This metadata field cannot be written.")),
        }
    }
    Ok(out)
}

pub(super) fn release_overlay(sidecar: &mut Sidecar, key: &str, changes: &Map<String, Value>) {
    for name in changes.keys() {
        if let Some((field, album)) = definition(name) {
            sidecar.use_file_field(key, album, field.name);
        }
    }
}
