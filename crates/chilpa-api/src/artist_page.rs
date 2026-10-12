//! The artist page: one shape for every source, and for the library.
//!
//! A plugin sends an artist's page as shelves with ids of its own. The
//! window draws one page for all of them, so here each shelf gets the
//! role it plays on that page from its id, a title the host chooses, and
//! its place in the order; what belongs to the Network sidebar is marked
//! as such. The library's artist is written in the same shape from what
//! the library has, so the page and the sidebar are one.
//!
//! What no source knows -- where an artist began and ended, who played in
//! it, who sounds alike -- comes from MusicBrainz and Last.fm, read once
//! per artist in the background: `network`.

use chilpa_i18n as i18n;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// The sections of releases, in the order the page shows them, with the
/// shelf ids the plugins use for each and the title the page gives it.
const RELEASES: &[(&str, &[&str], &str)] = &[
    ("album", &["album", "albums"], i18n::mark("Albums")),
    ("single", &["epSingle", "ep", "single", "singles", "ep-single"], i18n::mark("EPs & Singles")),
    ("live", &["live"], i18n::mark("Live")),
    ("compilation", &["compilation", "compilations"], i18n::mark("Compilations")),
    ("composer", &["composer"], i18n::mark("Composer")),
    ("awarded", &["awardedRelease", "awarded"], i18n::mark("Critics' Picks")),
    ("next", &["next", "upcoming"], i18n::mark("Upcoming")),
];

/// A shelf's role on the page, the section it is (for releases), its
/// title and its rank in the order, from the id its plugin gave it.
fn role_of(id: &str, kind: &str) -> (&'static str, String, String, usize) {
    match id {
        "tracks" | "popular" => ("popular", String::new(), i18n::t(i18n::mark("Popular Tracks")), 0),
        "latest" => ("latest", String::new(), i18n::t(i18n::mark("Latest release")), 1),
        "other" => ("other", "other".into(), i18n::t(i18n::mark("Other")), 60),
        "appearances" | "appears" => ("appears", String::new(), i18n::t(i18n::mark("Appears On")), 40),
        "playlists" => ("playlists", String::new(), i18n::t(i18n::mark("Playlists")), 50),
        "labels" => ("labels", String::new(), i18n::t(i18n::mark("Labels")), 100),
        "similar" => ("similar", String::new(), i18n::t(i18n::mark("Similar artists")), 101),
        "influences" => ("influences", String::new(), i18n::t(i18n::mark("Influences")), 102),
        "stories" => ("stories", String::new(), i18n::t(i18n::mark("Magazine")), 103),
        _ => {
            if let Some((at, (section, _, title))) = RELEASES.iter().enumerate().find(|(_, (_, ids, _))| ids.contains(&id)) {
                return ("release", section.to_string(), i18n::t(title), 10 + at);
            }
            // A shelf of records the page does not know is a section too,
            // after the known ones; anything else keeps its own title.
            if kind == "album" { ("release", id.to_string(), String::new(), 30) } else { ("shelf", String::new(), String::new(), 55) }
        }
    }
}

/// Gives every shelf of an artist's document its role, title and place,
/// and writes the jump bar's entries.
pub(crate) fn shape(document: &mut Value) {
    let Some(shelves) = document["shelves"].as_array_mut() else { return };
    let mut ranked: Vec<(usize, usize, Value)> = Vec::new();
    for (at, shelf) in shelves.drain(..).enumerate() {
        let mut shelf = shelf;
        let id = shelf["id"].as_str().unwrap_or_default().to_string();
        let kind = shelf["kind"].as_str().unwrap_or_default().to_string();
        let (role, section, title, rank) = role_of(&id, &kind);
        shelf["role"] = json!(role);
        shelf["sidebar"] = json!(rank >= 100);
        if !section.is_empty() { shelf["section"] = json!(section); }
        if !title.is_empty() { shelf["title"] = json!(title); }
        ranked.push((rank, at, shelf));
    }
    ranked.sort_by_key(|(rank, at, _)| (*rank, *at));
    *shelves = ranked.into_iter().map(|(_, _, shelf)| shelf).collect();
    let mut jump = Vec::new();
    if document["header"]["description"].as_str().is_some_and(|d| !d.trim().is_empty()) {
        jump.push(json!({"id": "about", "title": i18n::t(i18n::mark("About"))}));
    }
    for shelf in document["shelves"].as_array().into_iter().flatten() {
        let empty = shelf["items"].as_array().is_some_and(|items| items.is_empty()) && shelf["model"].is_null();
        if empty || shelf["sidebar"] == true { continue; }
        match shelf["role"].as_str() {
            Some("popular" | "release" | "appears") => jump.push(json!({"id": shelf["key"], "title": shelf["title"]})),
            _ => {}
        }
    }
    document["jump"] = json!(jump);
    document["artist"] = json!(true);
}

/// How a month is named in a date of birth; the day and the year are
/// numbers.
pub(crate) const MONTHS: [&str; 12] = [
    i18n::mark("Jan"), i18n::mark("Feb"), i18n::mark("Mar"), i18n::mark("Apr"), i18n::mark("May"), i18n::mark("Jun"),
    i18n::mark("Jul"), i18n::mark("Aug"), i18n::mark("Sep"), i18n::mark("Oct"), i18n::mark("Nov"), i18n::mark("Dec"),
];

/// "1990", "May 1990" or "14 May 1990", as much as is known; the holes
/// are day, month and year, for a language to join as it does.
pub(crate) fn date_label(date: &chilpa_meta::artist::Date) -> String {
    let month = (1..=12).contains(&date.month).then(|| i18n::t(MONTHS[date.month as usize - 1]));
    match (month, date.day) {
        (Some(month), 1..) => i18n::t_args(i18n::mark("{} {} {}"), &[&date.day.to_string(), &month, &date.year.to_string()]),
        (Some(month), _) => i18n::t_args(i18n::mark("{} {}"), &[&month, &date.year.to_string()]),
        (None, _) => date.year.to_string(),
    }
}

/// What the network knows of one artist, as the page reads it.
#[derive(Clone, Default)]
pub(crate) struct Known {
    pub loading: bool,
    pub mbid: String,
    pub origin: Option<chilpa_meta::artist::Origin>,
    pub relationships: chilpa_meta::artist::Relationships,
    /// Last.fm's biography and the artists it holds similar.
    pub bio: String,
    pub bio_url: String,
    pub similar: Vec<String>,
}

impl Known {
    fn json(&self, has_sources: bool) -> Value {
        let origin = self.origin.as_ref().map(|origin| {
            let person = origin.person;
            json!({
                "person": person,
                "begin": origin.begin.as_ref().map(date_label).unwrap_or_default(),
                "end": origin.end.as_ref().map(date_label).unwrap_or_default(),
                "place": origin.place,
                "area_id": origin.area_id,
                "country": origin.country,
                "country_name": origin.country_name,
                "exact": origin.exact,
                "tags": origin.tags,
            })
        });
        let rows = |list: &[chilpa_meta::artist::Relation]| -> Vec<Value> {
            list.iter().map(|r| {
                let span = match (r.begin.is_empty(), r.end.is_empty(), r.ended) {
                    (true, true, _) => String::new(),
                    (false, true, false) => format!("{} - {}", r.begin, i18n::t(i18n::mark("present"))),
                    (false, true, true) => r.begin.clone(),
                    (_, false, _) => format!("{} - {}", r.begin, r.end),
                };
                let tooltip = if span.is_empty() { r.role.clone() } else { format!("{} ({span})", r.role) };
                json!({"mbid": r.id, "name": r.name, "role": r.role, "tooltip": tooltip})
            }).collect()
        };
        json!({
            "loading": self.loading,
            "mbid": self.mbid,
            "available": self.loading || !self.mbid.is_empty(),
            "origin": origin,
            "relationships": {
                "members": rows(&self.relationships.members),
                "groups": rows(&self.relationships.groups),
                "collaborators": rows(&self.relationships.collaborators),
                "any": !self.relationships.is_empty(),
            },
            "bio": self.bio,
            "bio_url": self.bio_url,
            "similar": self.similar,
            "resolves": has_sources,
        })
    }
}

/// Reads what MusicBrainz and Last.fm know of artists, one at a time, and
/// keeps it for the run.
pub(crate) struct Network {
    known: Arc<Mutex<HashMap<String, Known>>>,
    catalog: Arc<crate::source_catalog::Catalog>,
}

impl Network {
    pub(crate) fn new(catalog: Arc<crate::source_catalog::Catalog>) -> Self {
        Self { known: Arc::new(Mutex::new(HashMap::new())), catalog }
    }

    /// What is known of the artist now; the first time, nothing yet, and
    /// the reading starts. `mbids` are ids the library's tags name, which
    /// save the search.
    pub(crate) fn clear(&self) {
        self.known.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }

    pub(crate) fn of(&self, name: &str, mbids: &[String], musicbrainz: bool) -> Known {
        let name = name.trim();
        if name.is_empty() {
            return Known::default();
        }
        let cache_key = format!("{musicbrainz}:{name}");
        let mut known = self.known.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = known.get(&cache_key) {
            return entry.clone();
        }
        known.insert(cache_key.clone(), Known { loading: true, ..Default::default() });
        drop(known);
        let (map, catalog, name, mbid) = (self.known.clone(), self.catalog.clone(), name.to_string(), mbids.first().cloned());
        let _ = std::thread::Builder::new().name("chilpa-artist-network".into()).spawn(move || {
            let mut found = read(&name, mbid, musicbrainz);
            found.loading = false;
            map.lock().unwrap_or_else(|e| e.into_inner()).insert(cache_key, found);
            catalog.changed();
        });
        Known { loading: true, ..Default::default() }
    }

    pub(crate) fn json(&self, name: &str, mbids: &[String], has_sources: bool, musicbrainz: bool) -> Value {
        self.of(name, mbids, musicbrainz).json(has_sources)
    }
}

/// MusicBrainz first, then Last.fm: a failure in one leaves the other.
fn read(name: &str, mbid: Option<String>, musicbrainz: bool) -> Known {
    let mut known = Known::default();
    if musicbrainz {
        let http = crate::metadata::worker::Network::new();
        let net = chilpa_meta::Net::musicbrainz(&http);
        let id = mbid.filter(|id| chilpa_meta::text::is_uuid(id)).or_else(|| chilpa_meta::artist::find(&net, name).ok().flatten());
        if let Some(id) = id {
            if let Ok(artist) = chilpa_meta::artist::artist(&net, &id) {
                known.mbid = id;
                known.origin = Some(artist.origin);
                known.relationships = artist.relationships;
            }
        }
    }
    if let Some(info) = crate::scrobble::lastfm::artist_info(name) {
        known.bio = info.bio;
        known.bio_url = info.url;
        known.similar = info.similar;
    }
    known
}

impl crate::Chilpa {
    pub fn musicbrainz_enabled(&self) -> bool {
        self.preference("integrations.musicbrainz.enabled").as_deref() != Some("false")
    }

    pub fn set_musicbrainz_enabled(&mut self, enabled: bool) {
        self.remember_preference("integrations.musicbrainz.enabled", &enabled.to_string());
        self.artist_scene.close();
        self.artist_network.clear();
        self.catalog.changed();
    }

    /// The sidebar's data for an artist by name, as JSON; empty while it
    /// is read.
    pub fn artist_network(&mut self, name: &str) -> String {
        let has_sources = !self.natives.ids().is_empty();
        self.artist_network.json(name, &[], has_sources, self.musicbrainz_enabled()).to_string()
    }

    /// Whether the library has records under that artist's name.
    fn has_artist(&mut self, name: &str) -> bool {
        self.library().is_some_and(|lib| lib.albums_filtered(chilpa_library::AlbumSort::Year, Some(name), 1, 0).is_ok_and(|a| !a.is_empty()))
    }

    /// The library's artist, in the shape of a source's page: the window
    /// draws both with the same screen.
    pub fn artist_document(&mut self, name: &str) -> String {
        let records = self.artist_release_cards(name, false);
        let guest = self.artist_release_cards(name, true);
        let chosen = Some(self.artist_art(name)).filter(|a| !a.is_empty());
        let art = chosen.or_else(|| records.first().or(guest.first()).map(|a| a.artwork_path.clone())).filter(|a| !a.is_empty())
            .and_then(|path| url::Url::from_file_path(path).ok()).map(|u| u.to_string()).unwrap_or_default();
        let known = self.artist_network.of(name, &[], self.musicbrainz_enabled());
        // The labels its records' tracks name, which the sidebar lists.
        let ids: Vec<i64> = records.iter().map(|a| a.id).collect();
        let labels = self.library().and_then(|lib| lib.albums_labels(&ids).ok()).unwrap_or_default();
        let similar: Vec<Value> = known.similar.iter().take(12).cloned().collect::<Vec<_>>().into_iter()
            .map(|n| { let local = self.has_artist(&n); json!({"kind": "artist", "title": n, "local": local}) }).collect();
        // `model` names the window's model for the library ("artist",
        // "artistTop"...), which reads by the artist's name: `model_key`.
        let shelf = |id: &str, kind: &str, model: &str, items: Value| json!({"id": id, "kind": kind, "model": model, "model_key": name, "items": items});
        let mut shelves = vec![
            shelf("tracks", "track", "artistTop", json!([])),
            shelf("latest", "album", "artistLatest", json!([])),
            shelf("album", "album", "artist", json!(records.iter().map(|a| json!({"ref": a.id})).collect::<Vec<_>>())),
            shelf("appearances", "album", "appearsOn", json!(guest.iter().map(|a| json!({"ref": a.id})).collect::<Vec<_>>())),
        ];
        shelves.push(json!({"id": "labels", "kind": "label", "items": labels.iter().map(|l| json!({"kind": "label", "title": l})).collect::<Vec<_>>()}));
        shelves.push(json!({"id": "similar", "kind": "artist", "items": similar}));
        for shelf in &mut shelves {
            shelf["key"] = json!(format!("local artist:{name}|{}", shelf["id"].as_str().unwrap_or_default()));
            shelf["tracks"] = json!(shelf["kind"] == "track");
            shelf["round"] = json!(shelf["kind"] == "artist");
            if shelf["model"].is_null() { shelf["model"] = json!("local"); }
        }
        let mut document = json!({
            "music": true,
            "local": true,
            "title": name,
            "loading": false,
            "error": "",
            "header": {
                "kind": "artist",
                "title": name,
                "art": art,
                "description": known.bio,
                "description_source": if known.bio.is_empty() { "" } else { "Last.fm" },
                "description_url": known.bio_url,
                "local": true,
                "records": records.len(),
            },
            "shelves": shelves,
        });
        shape(&mut document);
        document.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shelves_take_their_role_title_and_place_from_their_id() {
        let mut document = json!({"header": {"kind": "artist", "description": "Born in Leyton."}, "shelves": [
            {"id": "similar", "kind": "artist", "key": "k|similar", "items": [{"ref": "artist:2"}]},
            {"id": "single", "kind": "album", "title": "EPs and singles", "key": "k|single", "items": [{"ref": "album:1"}]},
            {"id": "tracks", "kind": "track", "title": "Top tracks", "key": "k|tracks", "items": [{"ref": "track:1"}]},
            {"id": "other", "kind": "album", "key": "k|other", "items": [{"ref": "album:3"}]},
            {"id": "album", "kind": "album", "title": "Albums", "key": "k|album", "items": [{"ref": "album:2"}]},
            {"id": "live", "kind": "album", "key": "k|live", "items": []},
            {"id": "appears", "kind": "album", "key": "k|appears", "items": [{"ref": "album:4"}]},
            {"id": "boxsets", "kind": "album", "title": "Box sets", "key": "k|boxsets", "items": [{"ref": "album:5"}]},
            {"id": "latest", "kind": "album", "key": "k|latest", "items": [{"ref": "album:2"}]},
        ]});
        shape(&mut document);
        let shelves = document["shelves"].as_array().unwrap();
        let order: Vec<(&str, &str)> = shelves.iter().map(|s| (s["id"].as_str().unwrap(), s["role"].as_str().unwrap())).collect();
        assert_eq!(order, [("tracks", "popular"), ("latest", "latest"), ("album", "release"), ("single", "release"), ("live", "release"),
            ("boxsets", "release"), ("appears", "appears"), ("other", "other"), ("similar", "similar")]);
        assert_eq!(shelves[0]["title"], "Popular Tracks", "the host names the shelves");
        assert_eq!(shelves[3]["title"], "EPs & Singles");
        assert_eq!(shelves[3]["section"], "single");
        assert_eq!(shelves[5]["title"], "Box sets", "a section the host does not know keeps its own title");
        assert_eq!(shelves[8]["sidebar"], true);
        assert_eq!(shelves[0]["sidebar"], false);
        let jump: Vec<&str> = document["jump"].as_array().unwrap().iter().map(|j| j["title"].as_str().unwrap()).collect();
        assert_eq!(jump, ["About", "Popular Tracks", "Albums", "EPs & Singles", "Box sets", "Appears On"], "an empty section is not jumped to");
        assert_eq!(document["artist"], true);
    }

    #[test]
    fn a_date_says_as_much_as_is_known() {
        use chilpa_meta::artist::Date;
        assert_eq!(date_label(&Date { year: 1990, month: 0, day: 0 }), "1990");
        assert_eq!(date_label(&Date { year: 1990, month: 5, day: 0 }), "May 1990");
        assert_eq!(date_label(&Date { year: 1990, month: 5, day: 14 }), "14 May 1990");
    }
}
