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
mod technical;
use memory::{kept, read};
use metadata::{Hint, Reader};
use technical::Technical;
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

/// The facts the information modal keeps apart as legal data.
const LEGAL: [&str; 5] = ["Label", "Copyright", "ISRC", "Barcode", "Catalog number"];

/// The answer as the window reads it, its facts named in the language of now.
fn shown(mut found: Value) -> String {
    for key in ["facts", "technical"] {
        let facts: Vec<Value> = found[key].as_array().into_iter().flatten().map(|fact| {
            let what = fact[0].as_str().unwrap_or_default();
            json!({"label": i18n::t(what), "value": fact[1], "group": if LEGAL.contains(&what) { "legal" } else { "details" }})
        }).collect();
        found[key] = json!(facts);
    }
    for credit in found["credits"].as_array_mut().into_iter().flatten() { contributors::show(credit); }
    for key in ["title", "note"] {
        if let Some(said) = found["file"][key].as_str().filter(|said| !said.is_empty()) { found["file"][key] = json!(i18n::t(said)); }
    }
    copied(&mut found);
    found.to_string()
}

/// What the copy button takes from a track's answer: the track, its record
/// and its artist on one line, or everything the modal says as plain text.
fn copied(found: &mut Value) {
    let line: Vec<String> = ["title", "album", "artist"].iter()
        .filter_map(|key| found["heading"][key].as_str()).filter(|said| !said.is_empty()).map(str::to_string).collect();
    if line.is_empty() { return; }
    let said = |key: &str, group: Option<&str>| -> Vec<String> {
        found[key].as_array().into_iter().flatten().filter(|fact| group.is_none_or(|group| fact["group"] == group))
            .map(|fact| format!("{}: {}", fact["label"].as_str().unwrap_or_default(), fact["value"].as_str().unwrap_or_default())).collect()
    };
    let credits: Vec<String> = found["credits"].as_array().into_iter().flatten()
        .map(|credit| [credit["name"].as_str(), credit["roles"].as_str()].into_iter().flatten().filter(|said| !said.is_empty()).collect::<Vec<_>>().join(" — ")).collect();
    let file: Vec<String> = [found["file"]["value"].as_str(), found["file"]["note"].as_str()].into_iter().flatten()
        .filter(|said| !said.is_empty()).map(str::to_string).collect();
    let mut full = line.clone();
    for block in [said("facts", Some("details")), said("technical", None), credits, said("facts", Some("legal")), file] {
        if !block.is_empty() { full.push(String::new()); full.extend(block); }
    }
    found["copy_basic"] = json!(line.join(" - "));
    found["copy_full"] = json!(full.join("\n"));
}

/// Where a track lives, for whoever is hunting a file down: its path on this
/// machine, or its entry on a server. `title` and `note` are catalog keys,
/// said in the window's language by `shown`. A streamed address is not a
/// place, so it says nothing.
fn file_where(path: &str, from_source: bool) -> Value {
    if let Some(entry) = chilpa_models::RemoteEntry::parse(path) {
        return json!({"title": i18n::mark("Server item"), "note": "",
            "value": format!("{}/{}/{}", entry.source.provider, entry.source.instance, entry.external_id)});
    }
    if from_source || path.is_empty() { return Value::Null; }
    // A disc image's track is `sacd:<image>#<n>`; the file is the image.
    let image = path.strip_prefix("sacd:").map_or(path, |rest| rest.split('#').next().unwrap_or(rest));
    if image.contains("://") { return Value::Null; }
    let note = if std::path::Path::new(image).exists() { "" } else { i18n::mark("Not found on disk") };
    json!({"title": i18n::mark("File"), "value": path, "note": note})
}

fn answer(facts: Vec<Fact>, credits: Vec<Value>, origin: String, link: String, description: String) -> Value {
    json!({"facts": facts.into_iter().map(|(what, value)| json!([what, value])).collect::<Vec<_>>(),
        "credits": credits, "origin": origin, "link": link, "description": description})
}

/// I retain source and tag facts, supplementing their credits with
/// MusicBrainz recording and work relationships when it is allowed.
fn read_track(path: String, source: Option<(Value, String)>, musicbrainz: bool, mut hint: Hint, stored: Technical) -> Value {
    let (mut facts, mut credits, mut origin, mut link) = (Vec::new(), Vec::new(), String::new(), String::new());
    let mut heading = json!({"title": hint.title.clone(), "album": hint.album.clone(), "artist": hint.artist.clone()});
    let mut technical = stored;
    let (mut isrc, mut recording) = (String::new(), String::new());
    let from_source = source.is_some();
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
        // A server's entry is not a path on this machine.
        None if chilpa_models::RemoteEntry::parse(&path).is_some() => {}
        None => if let Ok(file) = chilpa_tags::read(std::path::Path::new(&path)) {
            technical.read_file(&path, &file);
            let tags = file.tags;
            // A file the library does not hold is named by its own tags.
            for (key, said) in [("title", tags.title.clone()), ("album", tags.album.clone()), ("artist", Some(tags.artists.join(", ")))] {
                if heading[key] == "" { if let Some(said) = said.filter(|said| !said.is_empty()) { heading[key] = json!(said); } }
            }
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
    let mut failed = false;
    if musicbrainz {
        let library = chilpa_library::Library::open_existing(&chilpa_library::Library::default_path()).ok();
        metadata::file_hint(&path, &mut hint, library.as_ref());
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
    found["file"] = file_where(&path, from_source);
    found["technical"] = json!(technical.facts().into_iter().map(|(what, value)| json!([what, value])).collect::<Vec<_>>());
    found["heading"] = heading;
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
        let library = chilpa_library::Library::open_existing(&chilpa_library::Library::default_path()).ok();
        metadata::file_hint(&hint.path.clone(), &mut hint, library.as_ref());
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

/// Whom an answer speaks of: the track that plays, or one chosen from a
/// list, named by its path.
#[derive(Clone, Debug, Default, PartialEq)]
struct Subject { path: String, title: String, artist: String, album: String, duration_ms: u64, album_id: i64, stored: Technical }

impl Chilpa {
    /// I discard one failed panel's session answer before its explicit retry.
    pub fn retry_now_about(&mut self, part: &str, path: &str) -> bool {
        if path.is_empty() || self.now_playing().path != path { return false; }
        let subject = self.playing_subject();
        self.retry_about_of(part, &subject)
    }

    fn retry_about_of(&mut self, part: &str, subject: &Subject) -> bool {
        let identity = match part {
            "track" => subject.path.clone(),
            "album" if subject.album_id != 0 => subject.album_id.to_string(),
            _ => return false,
        };
        let policy = chilpa_core::network::current();
        let key = format!("{part}:{}:{}:{identity}", policy.musicbrainz, policy.offline);
        if part == "album" && kept(&key).is_some_and(|found| found["refetch_failed"] == true) {
            return self.refetch_album_of(subject);
        }
        memory::retry_failed(&key)
    }

    /// I refresh only the currently requested album; an old view cannot target the next track.
    pub fn refetch_now_album_about(&mut self, path: &str) -> bool {
        if path.is_empty() || self.now_playing().path != path { return false; }
        let subject = self.playing_subject();
        self.refetch_album_of(&subject)
    }

    fn refetch_album_of(&mut self, subject: &Subject) -> bool {
        let id = subject.album_id;
        if id == 0 { return false; }
        let policy = chilpa_core::network::current();
        if !policy.musicbrainz || policy.offline { return false; }
        let key = format!("album:true:false:{id}");
        let record = self.about_record(id, subject);
        memory::refresh(self.catalog.clone(), key, id.to_string(), move || read_album(record, true, true))
    }

    /// What the About tab says of the track that plays, as JSON: `facts`
    /// (label, value), `credits` (name, roles), `origin` (who said it),
    /// `link` and `loading`. Empty when nothing plays.
    pub fn now_track_about(&mut self) -> String {
        if self.now_playing().path.is_empty() { return "{}".into(); }
        let subject = self.playing_subject();
        self.track_about_of(subject)
    }

    fn track_about_of(&mut self, subject: Subject) -> String {
        let policy = chilpa_core::network::current();
        let musicbrainz = policy.musicbrainz;
        let key = format!("track:{musicbrainz}:{}:{}", policy.offline, subject.path);
        if let Some(found) = kept(&key) { return shown(found); }
        let source = self.catalog.track_by_path(&subject.path).map(|(track, _)| {
            let name = self.catalog.name(&track.plugin).unwrap_or_else(|| track.plugin.clone());
            (track.metadata, name)
        });
        let hint = self.about_hint(&subject);
        let (path, scope, stored) = (subject.path, subject.album_id.to_string(), subject.stored);
        shown(read(self.catalog.clone(), key, scope, move || read_track(path, source, musicbrainz, hint, stored)))
    }

    fn about_hint(&mut self, subject: &Subject) -> Hint {
        let mut hint = Hint { title: subject.title.clone(), artist: subject.artist.clone(), album: subject.album.clone(),
            album_artist: subject.artist.clone(), duration_ms: subject.duration_ms,
            path: subject.path.clone(), ..Default::default() };
        if subject.album_id != 0 {
            let info: Value = serde_json::from_str(&self.album_information(subject.album_id)).unwrap_or_default();
            let artist = text(&info, "artist");
            if !artist.is_empty() { hint.album_artist = artist; }
            hint.release_id = text(&info, "musicbrainz_release_id");
            hint.catalog_number = text(&info, "catalog_number");
        }
        if let Some(library) = self.library() {
            if let Some(row) = library.tracks_by_paths(&[subject.path.clone()]).ok().and_then(|mut rows| rows.pop().flatten()) {
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
        let subject = self.playing_subject();
        self.album_about_of(&subject)
    }

    fn album_about_of(&mut self, subject: &Subject) -> String {
        let id = subject.album_id;
        if id == 0 { return "{}".into(); }
        let policy = chilpa_core::network::current();
        let musicbrainz = policy.musicbrainz;
        let key = format!("album:{musicbrainz}:{}:{id}", policy.offline);
        let mut found = if let Some(found) = kept(&key) { found } else {
            let record = self.about_record(id, subject);
            read(self.catalog.clone(), key, id.to_string(), move || read_album(record, musicbrainz, false))
        };
        found["can_refetch"] = json!(musicbrainz && !policy.offline);
        shown(found)
    }

    fn about_record(&mut self, id: i64, subject: &Subject) -> Record {
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
            label: text(&info, "label"), source, facts, hint: self.about_hint(subject) }
    }

    fn playing_subject(&self) -> Subject {
        let now = &self.now;
        let duration_ms = (now.total_seconds * 1000.0).max(0.0) as u64;
        Subject { path: now.path.clone(), title: now.title.clone(), artist: now.artist.clone(), album: now.album.clone(),
            duration_ms, album_id: now.album_id, stored: Technical { quality: now.quality.clone(), duration_ms, ..Default::default() } }
    }

    /// A track that does not play, named by its path, as the library, a
    /// source's catalog or the media open now hold it. Neither its file nor
    /// its source is read here: a hung mount or a slow server cannot hold
    /// the window, and what only they know is read on the background.
    fn subject_of(&mut self, path: &str) -> Subject {
        let known = self.library().and_then(|library| library.tracks_by_paths(&[path.to_string()]).ok()).and_then(|mut rows| rows.pop().flatten());
        let stored = |quality, duration_ms| Technical { quality, duration_ms, ..Default::default() };
        if let Some(row) = known {
            let quality = crate::Quality::of_track(path, row.codec.as_deref(), row.bit_depth, row.sample_rate_hz);
            return Subject { path: path.into(), title: row.title, artist: row.artist, album: row.album, duration_ms: row.duration_ms,
                album_id: row.album_id.unwrap_or(0), stored: stored(quality, row.duration_ms) };
        }
        if let Some((track, _)) = self.catalog.track_by_path(path) {
            let quality = track.format.as_ref().map(|format| format.quality()).unwrap_or_default();
            return Subject { path: path.into(), title: track.title, artist: track.artist, album: track.album, duration_ms: track.duration_ms,
                album_id: track.album_id, stored: stored(quality, track.duration_ms) };
        }
        self.media_row(path).map(|row| Subject { path: path.into(), stored: stored(row.quality, row.duration_ms), title: row.title, artist: row.artist,
            album: row.album, duration_ms: row.duration_ms, album_id: row.album_id })
            .unwrap_or_else(|| Subject { path: path.into(), ..Default::default() })
    }

    /// What the About tab says of the track `path` names, playing or not, in
    /// the shape of `now_track_about`; the track that plays gets that same
    /// answer. Asking plays nothing and leaves the queue as it is.
    pub fn track_about(&mut self, path: &str) -> String {
        if path.is_empty() { return "{}".into(); }
        if path == self.now.path { return self.now_track_about(); }
        let subject = self.subject_of(path);
        self.track_about_of(subject)
    }

    /// The record the track `path` names is on, as `now_album_about` says it.
    pub fn album_about(&mut self, path: &str) -> String {
        if path.is_empty() { return "{}".into(); }
        if path == self.now.path { return self.now_album_about(); }
        let subject = self.subject_of(path);
        self.album_about_of(&subject)
    }

    /// `retry_now_about` for the track `path` names, playing or not.
    pub fn retry_about(&mut self, part: &str, path: &str) -> bool {
        if path.is_empty() { return false; }
        if path == self.now.path { return self.retry_now_about(part, path); }
        let subject = self.subject_of(path);
        self.retry_about_of(part, &subject)
    }

    /// `refetch_now_album_about` for the record of the track `path` names.
    pub fn refetch_album_about(&mut self, path: &str) -> bool {
        if path.is_empty() { return false; }
        if path == self.now.path { return self.refetch_now_album_about(path); }
        let subject = self.subject_of(path);
        self.refetch_album_of(&subject)
    }
}

#[cfg(test)]
mod chosen_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_source_s_credits_are_its_own_and_its_facts_are_named_in_the_window_s_language() {
        let metadata = json!({"performers": "Michael Stipe, MainArtist, Vocals - Peter Buck, Guitar", "composer": "Bill Berry", "isrc": "USWB19900001", "copyright": "1991 Warner"});
        let found = read_track(String::new(), Some((metadata, "Fixture".into())), false, Hint::default(), Technical::default());
        assert_eq!(found["origin"], "Fixture", "credits from the source are the source's");
        assert_eq!(found["credits"][0]["name"], "Michael Stipe");
        assert_eq!(found["credits"][1]["roles"], "Guitar");
        let said: Value = serde_json::from_str(&shown(found)).unwrap();
        let facts: Vec<(String, String)> = said["facts"].as_array().unwrap().iter()
            .map(|fact| (fact["label"].as_str().unwrap().to_string(), fact["value"].as_str().unwrap().to_string())).collect();
        assert!(facts.contains(&("Composer".into(), "Bill Berry".into())));
        assert_eq!(facts.last().unwrap(), &("ISRC".to_string(), "USWB19900001".to_string()), "the code last");
    }

    #[test]
    fn a_source_that_says_little_is_not_named_and_musicbrainz_is_not_asked_when_off() {
        let found = read_track(String::new(), Some((json!({"genre": "Rock"}), "Fixture".into())), false, Hint::default(), Technical::default());
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
    fn a_file_says_where_it_is_and_whether_it_is_still_there() {
        let here = std::env::current_exe().unwrap();
        let found = file_where(here.to_str().unwrap(), false);
        assert_eq!(found["value"], here.to_str().unwrap());
        assert_eq!(found["note"], "");
        let gone = file_where("/nonexistent/Music/gone.flac", false);
        assert_eq!(gone["note"], "Not found on disk");
        let image = file_where("sacd:/nonexistent/disc.iso#3", false);
        assert_eq!(image["note"], "Not found on disk");
        assert_eq!(image["value"], "sacd:/nonexistent/disc.iso#3");
    }

    #[test]
    fn a_server_entry_names_its_server_and_a_stream_or_a_source_name_nothing() {
        let entry = chilpa_models::RemoteEntry::new(chilpa_models::source::SourceId::new("provider", "home"), "abc 123").to_entry();
        assert_eq!(file_where(&entry, false)["value"], "provider/home/abc 123");
        assert!(file_where("https://radio.example/stream", false).is_null());
        assert!(file_where("/music/a.flac", true).is_null());
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

#[cfg(test)]
mod shown_tests {
    use super::*;

    #[test]
    fn legal_facts_are_kept_apart_and_the_copy_says_what_the_modal_says() {
        let mut found = answer(vec![(i18n::mark("Composer"), "Writer".into()), (i18n::mark("Label"), "House".into()), ("ISRC", "XX0000000001".into())],
            vec![json!({"name": "Player", "roles": "Guitar"})], String::new(), String::new(), String::new());
        found["technical"] = json!([[i18n::mark("Format"), "FLAC"]]);
        found["heading"] = json!({"title": "Song", "album": "Record", "artist": ""});
        found["file"] = json!({"title": i18n::mark("Server item"), "value": "provider/home/7", "note": ""});
        let said: Value = serde_json::from_str(&shown(found)).unwrap();
        let groups: Vec<&str> = said["facts"].as_array().unwrap().iter().map(|fact| fact["group"].as_str().unwrap()).collect();
        assert_eq!(groups, ["details", "legal", "legal"]);
        assert_eq!(said["copy_basic"], "Song - Record");
        let full = said["copy_full"].as_str().unwrap();
        for said in ["Song", "Record", "Player — Guitar", "provider/home/7", "XX0000000001"] { assert!(full.contains(said), "{said} in {full}"); }
        assert!(full.find("Player").unwrap() < full.find("XX0000000001").unwrap(), "credits before legal data");
        let album: Value = serde_json::from_str(&shown(json!({"facts": []}))).unwrap();
        assert!(album.get("copy_basic").is_none(), "nothing to copy without a heading");
    }
}
