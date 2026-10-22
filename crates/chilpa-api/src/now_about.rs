//! What the queue's About tab says of what plays: the track, and the record
//! it is on. What its source knows comes first when it says much -- credits,
//! a review, a label -- then the file's own tags, and MusicBrainz when it is
//! allowed and the rest says little. Each track and record is read once per
//! run, in the background, and the window reads the answer when it lands.

use crate::Chilpa;
use chilpa_i18n as i18n;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

/// How many answers are kept before the oldest go: a run plays many tracks.
const KEPT: usize = 256;

/// What was read for tracks and records, kept for the run of the one
/// player a process has.
static KNOWN: LazyLock<Mutex<HashMap<String, Value>>> = LazyLock::new(Default::default);

fn kept(key: &str) -> Option<Value> {
    KNOWN.lock().unwrap_or_else(|e| e.into_inner()).get(key).cloned()
}

/// Starts reading `key` in the background and says it is being read; the
/// catalog's change tells the window when the answer lands.
fn read(catalog: Arc<crate::source_catalog::Catalog>, key: String, read: impl FnOnce() -> Value + Send + 'static) -> Value {
    let loading = json!({"loading": true});
    {
        let mut known = KNOWN.lock().unwrap_or_else(|e| e.into_inner());
        if known.len() >= KEPT { known.clear(); }
        known.insert(key.clone(), loading.clone());
    }
    let _ = std::thread::Builder::new().name("chilpa-about".into()).spawn(move || {
        let mut found = read();
        found["loading"] = json!(false);
        KNOWN.lock().unwrap_or_else(|e| e.into_inner()).insert(key, found);
        catalog.changed();
    });
    loading
}

/// A fact as the tab lists it: what it is, and its value.
type Fact = (&'static str, String);

fn push(facts: &mut Vec<Fact>, what: &'static str, value: impl Into<String>) {
    let value = value.into();
    if !value.trim().is_empty() && !facts.iter().any(|(known, _)| *known == what) { facts.push((what, value.trim().to_string())); }
}

fn text(value: &Value, key: &str) -> String {
    match &value[key] {
        Value::String(text) => text.trim().to_string(),
        Value::Array(items) => items.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", "),
        Value::Number(number) => number.to_string(),
        _ => String::new(),
    }
}

/// The answer as the window reads it, its facts named in the language of now.
fn shown(mut found: Value) -> String {
    let facts: Vec<Value> = found["facts"].as_array().into_iter().flatten()
        .map(|fact| json!({"label": i18n::t(fact[0].as_str().unwrap_or_default()), "value": fact[1]})).collect();
    found["facts"] = json!(facts);
    found.to_string()
}

fn answer(facts: Vec<Fact>, credits: Vec<Value>, origin: String, link: String, description: String) -> Value {
    json!({"facts": facts.into_iter().map(|(what, value)| json!([what, value])).collect::<Vec<_>>(),
        "credits": credits, "origin": origin, "link": link, "description": description})
}

/// A track's facts and credits: its source's when it gives credits or a
/// composer, the file's tags otherwise, and MusicBrainz for the credits
/// when neither has them and it is allowed.
fn read_track(path: String, source: Option<(Value, String)>, musicbrainz: bool) -> Value {
    let (mut facts, mut credits, mut origin, mut link) = (Vec::new(), Vec::new(), String::new(), String::new());
    let (mut isrc, mut recording) = (String::new(), String::new());
    match source {
        Some((metadata, name)) => {
            credits = crate::album_information::participants(&metadata["performers"], &json!({})).into_iter()
                .map(|person| json!({"name": person["name"], "roles": person["roles"]})).collect();
            push(&mut facts, i18n::mark("Composer"), text(&metadata, "composer"));
            push(&mut facts, i18n::mark("Work"), text(&metadata, "work"));
            push(&mut facts, i18n::mark("Genre"), text(&metadata, "genre"));
            push(&mut facts, i18n::mark("Release date"), text(&metadata, "release_date"));
            push(&mut facts, i18n::mark("Copyright"), text(&metadata, "copyright"));
            isrc = text(&metadata, "isrc");
            if !credits.is_empty() || !text(&metadata, "composer").is_empty() { origin = name; }
        }
        None => if let Ok(file) = chilpa_tags::read(std::path::Path::new(&path)) {
            let tags = file.tags;
            push(&mut facts, i18n::mark("Composer"), tags.composers.join(", "));
            push(&mut facts, i18n::mark("Work"), tags.work.unwrap_or_default());
            push(&mut facts, i18n::mark("Genre"), tags.genres.join(", "));
            push(&mut facts, i18n::mark("Release date"), tags.date.unwrap_or_default());
            push(&mut facts, i18n::mark("Label"), tags.label.unwrap_or_default());
            isrc = tags.isrc.unwrap_or_default();
            recording = tags.musicbrainz_recording_id.unwrap_or_default();
        }
    }
    if credits.is_empty() && musicbrainz {
        let http = crate::metadata::worker::Network::new();
        let net = chilpa_meta::Net::musicbrainz(&http);
        let id = Some(recording).filter(|id| chilpa_meta::text::is_uuid(id))
            .or_else(|| chilpa_meta::credits::recording_by_isrc(&net, &isrc).ok().flatten());
        if let Some(found) = id.and_then(|id| chilpa_meta::credits::recording(&net, &id).ok()).filter(|found| !found.is_empty()) {
            credits = found.performers.iter().map(|person| json!({"name": person.name, "roles": person.roles.join(", ")})).collect();
            push(&mut facts, i18n::mark("Composer"), found.composers.join(", "));
            push(&mut facts, i18n::mark("Lyricist"), found.lyricists.join(", "));
            origin = "MusicBrainz".into();
            link = found.url;
        }
    }
    push(&mut facts, "ISRC", isrc);
    answer(facts, credits, origin, link, String::new())
}

/// What a record's information says, read where it lives (cheap), for the
/// background to complete.
struct Record {
    title: String,
    artist: String,
    description: String,
    label: String,
    source: String,
    facts: Vec<Fact>,
}

/// A record's facts and review: its source's or the library's when it has
/// a review or a label, and MusicBrainz's release otherwise, when allowed.
fn read_album(record: Record, musicbrainz: bool) -> Value {
    let Record { title, artist, description, label, source, mut facts } = record;
    let rich = !description.is_empty() || !label.is_empty();
    let (mut origin, mut link) = (if rich { source } else { String::new() }, String::new());
    if !rich && musicbrainz && !title.is_empty() {
        let http = crate::metadata::worker::Network::new();
        let net = chilpa_meta::Net::musicbrainz(&http);
        let best = chilpa_meta::musicbrainz::search(&net, &title, &artist, "").ok().into_iter().flatten()
            .filter(|hit| hit.score >= 90).max_by_key(|hit| hit.score);
        if let Some(release) = best.and_then(|hit| chilpa_meta::musicbrainz::release(&net, &hit.id).ok()) {
            push(&mut facts, i18n::mark("Label"), release.label.clone());
            push(&mut facts, i18n::mark("Catalog number"), release.catalog_number.clone());
            push(&mut facts, i18n::mark("Release date"), release.date.clone());
            push(&mut facts, i18n::mark("Country"), release.country.clone());
            push(&mut facts, i18n::mark("Genre"), release.genres.join(", "));
            push(&mut facts, i18n::mark("Barcode"), release.barcode.clone());
            origin = "MusicBrainz".into();
            link = release.url;
        }
    }
    answer(facts, Vec::new(), origin, link, description)
}

impl Chilpa {
    /// What the About tab says of the track that plays, as JSON: `facts`
    /// (label, value), `credits` (name, roles), `origin` (who said it),
    /// `link` and `loading`. Empty when nothing plays.
    pub fn now_track_about(&mut self) -> String {
        let path = self.now_playing().path.clone();
        if path.is_empty() { return "{}".into(); }
        let musicbrainz = self.musicbrainz_enabled();
        let key = format!("track:{musicbrainz}:{path}");
        if let Some(found) = kept(&key) { return shown(found); }
        let source = self.catalog.track_by_path(&path).map(|(track, _)| {
            let name = self.catalog.name(&track.plugin).unwrap_or_else(|| track.plugin.clone());
            (track.metadata, name)
        });
        shown(read(self.catalog.clone(), key, move || read_track(path, source, musicbrainz)))
    }

    /// What the About tab says of the record the track is on, as JSON, in
    /// the shape of `now_track_about`, with its review as `description`.
    pub fn now_album_about(&mut self) -> String {
        let id = self.now_playing().album_id;
        if id == 0 { return "{}".into(); }
        let musicbrainz = self.musicbrainz_enabled();
        let key = format!("album:{musicbrainz}:{id}");
        if let Some(found) = kept(&key) { return shown(found); }
        let info: Value = serde_json::from_str(&self.album_information(id)).unwrap_or_default();
        let source = if id < 0 { info["plugin"].as_str().and_then(|plugin| self.catalog.name(plugin)).unwrap_or_default() } else { "Chilpa".to_string() };
        let mut facts = Vec::new();
        push(&mut facts, i18n::mark("Label"), text(&info, "label"));
        push(&mut facts, i18n::mark("Release date"), text(&info, "released"));
        push(&mut facts, i18n::mark("Genre"), text(&info, "genre"));
        push(&mut facts, i18n::mark("Quality"), text(&info, "quality_label"));
        push(&mut facts, i18n::mark("Copyright"), text(&info, "copyright"));
        push(&mut facts, i18n::mark("Barcode"), text(&info, "upc"));
        let record = Record { title: text(&info, "title"), artist: text(&info, "artist"), description: text(&info, "description"),
            label: text(&info, "label"), source, facts };
        shown(read(self.catalog.clone(), key, move || read_album(record, musicbrainz)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_source_s_credits_are_its_own_and_its_facts_are_named_in_the_window_s_language() {
        let metadata = json!({"performers": "Michael Stipe, MainArtist, Vocals - Peter Buck, Guitar", "composer": "Bill Berry", "isrc": "USWB19900001", "copyright": "1991 Warner"});
        let found = read_track(String::new(), Some((metadata, "Fixture".into())), false);
        assert_eq!(found["origin"], "Fixture", "credits from the source are the source's");
        assert_eq!(found["credits"][0]["name"], "Michael Stipe");
        assert_eq!(found["credits"][1]["roles"], "Guitar");
        let shown: Value = serde_json::from_str(&shown(found)).unwrap();
        let facts: Vec<(String, String)> = shown["facts"].as_array().unwrap().iter()
            .map(|fact| (fact["label"].as_str().unwrap().to_string(), fact["value"].as_str().unwrap().to_string())).collect();
        assert!(facts.contains(&("Composer".into(), "Bill Berry".into())));
        assert_eq!(facts.last().unwrap(), &("ISRC".to_string(), "USWB19900001".to_string()), "the code last");
    }

    #[test]
    fn a_source_that_says_little_is_not_named_and_musicbrainz_is_not_asked_when_off() {
        let found = read_track(String::new(), Some((json!({"genre": "Rock"}), "Fixture".into())), false);
        assert_eq!(found["origin"], "", "a genre alone is not rich");
        assert!(found["credits"].as_array().unwrap().is_empty());
        let record = Record { title: "Out of Time".into(), artist: "R.E.M.".into(), description: String::new(), label: String::new(),
            source: "Chilpa".into(), facts: vec![(i18n::mark("Genre"), "Rock".into())] };
        let found = read_album(record, false);
        assert_eq!((found["origin"].as_str(), found["link"].as_str()), (Some(""), Some("")));
        let record = Record { title: "Out of Time".into(), artist: "R.E.M.".into(), description: "A review".into(), label: String::new(),
            source: "Fixture".into(), facts: Vec::new() };
        assert_eq!(read_album(record, false)["origin"], "Fixture", "a review is rich");
    }

    #[test]
    fn the_same_fact_is_said_once_and_blanks_not_at_all() {
        let mut facts = Vec::new();
        push(&mut facts, i18n::mark("Composer"), "A");
        push(&mut facts, i18n::mark("Composer"), "B");
        push(&mut facts, i18n::mark("Label"), "  ");
        assert_eq!(facts, [(i18n::mark("Composer"), "A".to_string())]);
    }
}
