//! What the queue's About tab says of what plays: the track, and the record
//! it is on. What its source knows comes first when it says much -- credits,
//! a review, a label -- then the file's own tags, and MusicBrainz when it is
//! allowed. I retain positive MusicBrainz metadata permanently and refresh
//! it only on request, using bounded jobs away from the window thread.

use crate::Chilpa;
use chilpa_i18n as i18n;
use serde_json::{json, Value};
mod memory;
mod metadata;
mod contributors;
use memory::{kept, read};
use metadata::{Hint, Reader};
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
    for credit in found["credits"].as_array_mut().into_iter().flatten() { contributors::show(credit); }
    found.to_string()
}

fn answer(facts: Vec<Fact>, credits: Vec<Value>, origin: String, link: String, description: String) -> Value {
    json!({"facts": facts.into_iter().map(|(what, value)| json!([what, value])).collect::<Vec<_>>(),
        "credits": credits, "origin": origin, "link": link, "description": description})
}

/// I retain source and tag facts, supplementing their credits with
/// MusicBrainz recording and work relationships when it is allowed.
fn read_track(path: String, source: Option<(Value, String)>, musicbrainz: bool, mut hint: Hint) -> Value {
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
            recording = text(&metadata, "musicbrainz_recording_id");
            let release_id = text(&metadata, "musicbrainz_release_id");
            if !release_id.is_empty() { hint.release_id = release_id; }
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
    if hint.isrc.is_empty() { hint.isrc = isrc.clone(); }
    if hint.recording_id.is_empty() { hint.recording_id = recording; }
    metadata::file_hint(&path, &mut hint);
    let mut failed = false;
    if musicbrainz {
        let library = chilpa_library::Library::open_existing(&chilpa_library::Library::default_path()).ok();
        let http = crate::metadata::worker::Network::new();
        let net = chilpa_meta::Net::musicbrainz(&http);
        let policy = chilpa_core::network::current();
        let reader = Reader { library: library.as_ref(), net: &net, online: policy.musicbrainz && !policy.offline, now: metadata::now() };
        match reader.track(&hint) {
            Ok((found, url)) if !found.is_empty() => {
                credits.extend(contributors::rows(&found));
                origin = if origin.is_empty() { "MusicBrainz".into() } else { format!("{origin} · MusicBrainz") };
                link = url;
            }
            Err(_) => failed = true,
            _ => {}
        }
    }
    push(&mut facts, "ISRC", isrc);
    let mut found = answer(facts, credits, origin, link, String::new());
    found["failed"] = json!(failed);
    found
}

/// What a record's information says, read where it lives (cheap), for the
/// background to complete.
#[derive(Default)]
struct Record {
    title: String,
    artist: String,
    description: String,
    label: String,
    source: String,
    facts: Vec<Fact>,
    hint: Hint,
}

/// A record's facts and review: its source's or the library's when it has
/// a review or a label, and MusicBrainz's release otherwise, when allowed.
fn read_album(record: Record, musicbrainz: bool, refresh: bool) -> Value {
    let Record { title, artist, description, label, source, mut facts, mut hint } = record;
    let rich = !description.is_empty() || !label.is_empty();
    let (mut origin, mut link) = (if rich { source } else { String::new() }, String::new());
    let (mut credits, mut failed) = (Vec::new(), false);
    let can_lookup = musicbrainz && !title.is_empty();
    if can_lookup {
        hint.album = title;
        hint.album_artist = artist;
        metadata::file_hint(&hint.path.clone(), &mut hint);
        let library = chilpa_library::Library::open_existing(&chilpa_library::Library::default_path()).ok();
        let http = crate::metadata::worker::Network::new();
        let net = chilpa_meta::Net::musicbrainz(&http);
        let policy = chilpa_core::network::current();
        let reader = Reader { library: library.as_ref(), net: &net, online: policy.musicbrainz && !policy.offline, now: metadata::now() };
        let result = if refresh { reader.refresh_release(&hint) } else { reader.release(&hint) };
        let result = match result {
            Err(_) if refresh => {
                failed = true;
                Reader { online: false, ..reader }.release(&hint)
            }
            result => result,
        };
        match result {
            Ok(Some(release)) => {
                for (label, key) in [(i18n::mark("Label"), "label"), (i18n::mark("Catalog number"), "catalog_number"),
                    (i18n::mark("Release date"), "date"), (i18n::mark("Country"), "country"), (i18n::mark("Genre"), "genres"), (i18n::mark("Barcode"), "barcode")] {
                    push(&mut facts, label, text(&release.facts, key));
                }
                credits = contributors::rows(&release.contributions);
                origin = if origin.is_empty() { "MusicBrainz".into() } else { format!("{origin} · MusicBrainz") };
                link = format!("https://musicbrainz.org/release/{}", release.id);
            }
            Err(_) => failed = true,
            _ => {}
        }
    }
    let mut found = answer(facts, credits, origin, link, description);
    found["failed"] = json!(failed || (refresh && !can_lookup));
    found["refetch_failed"] = json!(refresh && found["failed"] == true);
    found
}

impl Chilpa {
    /// I discard one failed panel's session answer before its explicit retry.
    pub fn retry_now_about(&mut self, part: &str, path: &str) -> bool {
        if path.is_empty() || self.now_playing().path != path { return false; }
        let identity = match part {
            "track" => path.to_string(),
            "album" if self.now.album_id != 0 => self.now.album_id.to_string(),
            _ => return false,
        };
        let policy = chilpa_core::network::current();
        let key = format!("{part}:{}:{}:{identity}", policy.musicbrainz, policy.offline);
        if part == "album" && kept(&key).is_some_and(|found| found["refetch_failed"] == true) {
            return self.refetch_now_album_about(path);
        }
        memory::retry_failed(&key)
    }

    /// I refresh only the currently requested album; an old view cannot target the next track.
    pub fn refetch_now_album_about(&mut self, path: &str) -> bool {
        if path.is_empty() || self.now_playing().path != path || self.now.album_id == 0 { return false; }
        let policy = chilpa_core::network::current();
        if !policy.musicbrainz || policy.offline { return false; }
        let id = self.now.album_id;
        let key = format!("album:true:false:{id}");
        let record = self.about_record(id);
        memory::refresh(self.catalog.clone(), key, id.to_string(), move || read_album(record, true, true))
    }

    /// What the About tab says of the track that plays, as JSON: `facts`
    /// (label, value), `credits` (name, roles), `origin` (who said it),
    /// `link` and `loading`. Empty when nothing plays.
    pub fn now_track_about(&mut self) -> String {
        let path = self.now_playing().path.clone();
        if path.is_empty() { return "{}".into(); }
        let policy = chilpa_core::network::current();
        let musicbrainz = policy.musicbrainz;
        let key = format!("track:{musicbrainz}:{}:{path}", policy.offline);
        if let Some(found) = kept(&key) { return shown(found); }
        let source = self.catalog.track_by_path(&path).map(|(track, _)| {
            let name = self.catalog.name(&track.plugin).unwrap_or_else(|| track.plugin.clone());
            (track.metadata, name)
        });
        let hint = self.about_hint(&path);
        shown(read(self.catalog.clone(), key, self.now.album_id.to_string(), move || read_track(path, source, musicbrainz, hint)))
    }

    fn about_hint(&mut self, path: &str) -> Hint {
        let mut hint = Hint { title: self.now.title.clone(), artist: self.now.artist.clone(), album: self.now.album.clone(),
            album_artist: self.now.artist.clone(), duration_ms: (self.now.total_seconds * 1000.0).max(0.0) as u64,
            path: path.into(), ..Default::default() };
        if self.now.album_id != 0 {
            let info: Value = serde_json::from_str(&self.album_information(self.now.album_id)).unwrap_or_default();
            let artist = text(&info, "artist");
            if !artist.is_empty() { hint.album_artist = artist; }
            hint.release_id = text(&info, "musicbrainz_release_id");
            hint.catalog_number = text(&info, "catalog_number");
        }
        if let Some(library) = self.library() {
            if let Some(row) = library.tracks_by_paths(&[path.to_string()]).ok().and_then(|mut rows| rows.pop().flatten()) {
                if let Some((isrc, mbid)) = library.fingerprints_for(&[row.id]).ok().and_then(|mut ids| ids.remove(&row.id)) {
                    hint.isrc = isrc.unwrap_or_default();
                    hint.recording_id = mbid.unwrap_or_default();
                }
            }
        }
        hint
    }

    /// What the About tab says of the record the track is on, as JSON, in
    /// the shape of `now_track_about`, with its review as `description`.
    pub fn now_album_about(&mut self) -> String {
        let id = self.now_playing().album_id;
        if id == 0 { return "{}".into(); }
        let policy = chilpa_core::network::current();
        let musicbrainz = policy.musicbrainz;
        let key = format!("album:{musicbrainz}:{}:{id}", policy.offline);
        let mut found = if let Some(found) = kept(&key) { found } else {
            let record = self.about_record(id);
            read(self.catalog.clone(), key, id.to_string(), move || read_album(record, musicbrainz, false))
        };
        found["can_refetch"] = json!(musicbrainz && !policy.offline);
        shown(found)
    }

    fn about_record(&mut self, id: i64) -> Record {
        let info: Value = serde_json::from_str(&self.album_information(id)).unwrap_or_default();
        let source = if id < 0 { info["plugin"].as_str().and_then(|plugin| self.catalog.name(plugin)).unwrap_or_default() } else { "Chilpa".to_string() };
        let mut facts = Vec::new();
        push(&mut facts, i18n::mark("Label"), text(&info, "label"));
        push(&mut facts, i18n::mark("Release date"), text(&info, "released"));
        push(&mut facts, i18n::mark("Genre"), text(&info, "genre"));
        push(&mut facts, i18n::mark("Quality"), text(&info, "quality_label"));
        push(&mut facts, i18n::mark("Copyright"), text(&info, "copyright"));
        push(&mut facts, i18n::mark("Barcode"), text(&info, "upc"));
        Record { title: text(&info, "title"), artist: text(&info, "artist"), description: text(&info, "description"),
            label: text(&info, "label"), source, facts, hint: self.about_hint(&self.now.path.clone()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_source_s_credits_are_its_own_and_its_facts_are_named_in_the_window_s_language() {
        let metadata = json!({"performers": "Michael Stipe, MainArtist, Vocals - Peter Buck, Guitar", "composer": "Bill Berry", "isrc": "USWB19900001", "copyright": "1991 Warner"});
        let found = read_track(String::new(), Some((metadata, "Fixture".into())), false, Hint::default());
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
        let found = read_track(String::new(), Some((json!({"genre": "Rock"}), "Fixture".into())), false, Hint::default());
        assert_eq!(found["origin"], "", "a genre alone is not rich");
        assert!(found["credits"].as_array().unwrap().is_empty());
        let record = Record { title: "Out of Time".into(), artist: "R.E.M.".into(), description: String::new(), label: String::new(),
            source: "Chilpa".into(), facts: vec![(i18n::mark("Genre"), "Rock".into())], ..Default::default() };
        let found = read_album(record, false, false);
        assert_eq!((found["origin"].as_str(), found["link"].as_str()), (Some(""), Some("")));
        let record = Record { title: "Out of Time".into(), artist: "R.E.M.".into(), description: "A review".into(), label: String::new(),
            source: "Fixture".into(), facts: Vec::new(), ..Default::default() };
        assert_eq!(read_album(record, false, false)["origin"], "Fixture", "a review is rich");
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
