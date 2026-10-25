//! I read recording, work and release relationships without flattening roles.

use crate::{musicbrainz::{get, API}, text::is_uuid, Failure, Net};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// I expose provider data; the facade maps it to the library's role model.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Entity { Recording, Work, Release }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contribution {
    pub id: String,
    pub name: String,
    pub credited_name: String,
    pub relation: String,
    pub attributes: Vec<String>,
    pub scope: Entity,
    pub entity_id: String,
    pub entity_title: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CreditedTrack {
    pub title: String,
    pub artist: String,
    pub disc: u16,
    pub number: u16,
    pub duration_ms: u64,
    pub recording_id: String,
    pub contributions: Vec<Contribution>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CreditedRelease {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub facts: Value,
    pub contributions: Vec<Contribution>,
    pub tracks: Vec<CreditedTrack>,
}

fn text(v: &Value, key: &str) -> String { v[key].as_str().unwrap_or_default().trim().to_string() }
fn artist(v: &Value) -> String {
    v["artist-credit"].as_array().into_iter().flatten().map(|c| {
        format!("{}{}", c["name"].as_str().or_else(|| c["artist"]["name"].as_str()).unwrap_or_default(), c["joinphrase"].as_str().unwrap_or_default())
    }).collect::<String>().trim().to_string()
}

pub fn relations(answer: &Value, scope: Entity, id: &str, title: &str) -> Vec<Contribution> {
    let mut credits = Vec::new();
    for relation in answer["relations"].as_array().into_iter().flatten() {
        let person = &relation["artist"];
        let (person_id, name, kind) = (text(person, "id"), text(person, "name"), text(relation, "type"));
        if !is_uuid(&person_id) || name.is_empty() || kind.is_empty() { continue; }
        let credited = text(relation, "target-credit");
        let contribution = Contribution { id: person_id, name: name.clone(),
            credited_name: if credited.is_empty() { name } else { credited }, relation: kind,
            attributes: relation["attributes"].as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_string).collect(),
            scope, entity_id: id.into(), entity_title: title.into() };
        if !credits.contains(&contribution) { credits.push(contribution); }
    }
    credits
}

pub fn recording_credits(answer: &Value) -> Vec<Contribution> {
    let mut credits = relations(answer, Entity::Recording, &text(answer, "id"), &text(answer, "title"));
    for relation in answer["relations"].as_array().into_iter().flatten().filter(|r| r["type"] == "performance") {
        let work = &relation["work"];
        credits.extend(relations(work, Entity::Work, &text(work, "id"), &text(work, "title")));
    }
    credits
}

pub fn recording(net: &Net, id: &str) -> Result<Vec<Contribution>, Failure> {
    if !is_uuid(id) { return Err(Failure::BadId); }
    let answer = get(net, &format!("{API}/recording/{id}?inc=artist-rels+work-rels+work-level-rels&fmt=json"))?;
    if answer["id"] != id || !answer["relations"].is_array() { return Err(Failure::Unexpected("no recording relationships".into())); }
    Ok(recording_credits(&answer))
}

pub fn recordings_by_isrc(net: &Net, isrc: &str) -> Result<Vec<String>, Failure> {
    let Some(url) = crate::musicbrainz::isrc_url(isrc) else { return Ok(Vec::new()); };
    let answer = match get(net, &url) {
        Err(Failure::Unexpected(reason)) if reason == "not found" => return Ok(Vec::new()),
        result => result?,
    };
    let rows = answer["recordings"].as_array().ok_or_else(|| Failure::Unexpected("no ISRC recordings".into()))?;
    let mut ids: Vec<String> = rows.iter().filter_map(|r| r["id"].as_str()).filter(|id| is_uuid(id)).map(str::to_string).collect();
    ids.sort_unstable(); ids.dedup();
    Ok(ids)
}

pub fn release(net: &Net, id: &str) -> Result<CreditedRelease, Failure> { release_read(net, id, false) }

/// I require explicit relationship arrays before replacing permanent recording credits.
pub fn release_complete(net: &Net, id: &str) -> Result<CreditedRelease, Failure> { release_read(net, id, true) }

fn complete(answer: &Value) -> bool {
    answer["relations"].is_array() && answer["media"].as_array().is_some_and(|media| media.iter().all(|medium| {
        medium["tracks"].as_array().is_some_and(|tracks| tracks.iter().all(|track| {
            let recording = &track["recording"];
            is_uuid(recording["id"].as_str().unwrap_or_default()) && recording["relations"].as_array().is_some_and(|relations| {
                relations.iter().filter(|r| r["type"] == "performance").all(|r| {
                    is_uuid(r["work"]["id"].as_str().unwrap_or_default()) && r["work"]["relations"].is_array()
                })
            })
        }))
    }))
}

fn release_read(net: &Net, id: &str, require_complete: bool) -> Result<CreditedRelease, Failure> {
    if !is_uuid(id) { return Err(Failure::BadId); }
    let answer = get(net, &format!("{API}/release/{id}?inc=recordings+artist-credits+artist-rels+work-rels+recording-level-rels+work-level-rels+labels+genres+release-groups&fmt=json"))?;
    if answer["id"] != id || !answer["media"].is_array() { return Err(Failure::Unexpected("no release media".into())); }
    if require_complete && !complete(&answer) { return Err(Failure::Unexpected("incomplete release relationships".into())); }
    Ok(release_of(&answer))
}

pub fn release_of(answer: &Value) -> CreditedRelease {
    let (id, title) = (text(answer, "id"), text(answer, "title"));
    let mut release = CreditedRelease { contributions: relations(answer, Entity::Release, &id, &title), id, title, artist: artist(answer),
        facts: json!({"label": answer["label-info"][0]["label"]["name"], "catalog_number":answer["label-info"][0]["catalog-number"],
            "date":answer["date"], "country":answer["country"], "barcode":answer["barcode"],
            "genres":answer["genres"].as_array().into_iter().flatten().filter_map(|g| g["name"].as_str()).collect::<Vec<_>>() }), tracks: Vec::new() };
    for medium in answer["media"].as_array().into_iter().flatten() {
        for track in medium["tracks"].as_array().into_iter().flatten() {
            let recording = &track["recording"];
            release.tracks.push(CreditedTrack { title: text(track, "title"), artist: artist(recording),
                disc: medium["position"].as_u64().and_then(|n| n.try_into().ok()).unwrap_or(0),
                number: track["position"].as_u64().and_then(|n| n.try_into().ok()).unwrap_or(0),
                duration_ms: track["length"].as_u64().or_else(|| recording["length"].as_u64()).unwrap_or(0),
                recording_id: text(recording, "id"), contributions: recording_credits(recording) });
        }
    }
    release
}

/// I require title and artist equality after punctuation/case folding and
/// a close duration. Multiple editions are safe only for the same recording.
pub fn matching_track<'a>(release: &'a CreditedRelease, title: &str, artist: &str, duration_ms: u64) -> Option<&'a CreditedTrack> {
    let fold = |s: &str| s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect::<String>();
    if title.trim().is_empty() || artist.trim().is_empty() || duration_ms == 0 { return None; }
    let mut candidates = release.tracks.iter().filter(|t| fold(&t.title) == fold(title) && fold(&t.artist) == fold(artist)
        && t.duration_ms > 0 && t.duration_ms.abs_diff(duration_ms) <= 5_000 && is_uuid(&t.recording_id));
    let first = candidates.next()?;
    candidates.all(|t| t.recording_id == first.recording_id).then_some(first)
}

#[cfg(test)]
#[path = "contributions_tests.rs"]
mod tests;
