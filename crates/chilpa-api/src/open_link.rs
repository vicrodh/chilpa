//! "Open music link", ported from QBZ (`qbz-music-link`,
//! `link_resolver_qt.rs`, `LinkResolverModal.qml`), with Chilpa's sources
//! where QBZ had one service, and without Odesli, whose keyless API was
//! retired on 2026-07-31.
//!
//! 1. A source that claims the link (`link.resolve`) opens what it names
//!    there: an album, an artist or a playlist; a track opens its album, as
//!    in QBZ.
//! 2. Otherwise a plugin that knows other services' links says what it is
//!    (`link.identify`): its kind, its title and its artist. A playlist goes
//!    to the playlist importer.
//! 3. With a title and an artist: the library first, its best edition, then
//!    the sources in the sidebar's order with QBZ's three searches
//!    (`qobuz_search.rs::search_qobuz_smart`).
//!
//! The work runs on a thread of its own; the window asks how it goes.

use crate::{Chilpa, Leads};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// How long one plugin has to answer one question.
const ASK: Duration = Duration::from_secs(15);

/// What a link came to.
#[derive(Clone, Debug, PartialEq)]
enum Found {
    /// A record of the library: its best edition.
    Library(i64),
    /// Something of a source, as its search shows it.
    Source { plugin: String, name: String, item: Value },
    /// A playlist of another service, for the importer.
    Playlist,
}

#[derive(Default)]
pub(crate) struct LinkState {
    url: String,
    resolving: bool,
    error: String,
    found: Option<Found>,
    generation: u64,
}

pub(crate) type Shared = Arc<Mutex<LinkState>>;

fn lock(shared: &Shared) -> std::sync::MutexGuard<'_, LinkState> {
    shared.lock().unwrap_or_else(|e| e.into_inner())
}

/// A title without the trailing "(...)" and "[...]" an edition adds:
/// "Senjutsu (2021 Remaster)" is "Senjutsu".
fn clean_title(title: &str) -> String {
    let mut result = title.to_string();
    while let Some(at) = result.rfind('(') {
        if result[at..].contains(')') { result = result[..at].trim_end().to_string() } else { break }
    }
    while let Some(at) = result.rfind('[') {
        if result[at..].contains(']') { result = result[..at].trim_end().to_string() } else { break }
    }
    result.trim().to_string()
}

/// The first item of one list of a native search's answer.
fn first(answer: &Value, list: &str) -> Option<Value> {
    answer[list].as_array()?.iter().find(|item| item["ref"].as_str().is_some_and(|r| !r.is_empty())).cloned()
}

/// QBZ's three searches in one source: "title artist" (a track first when
/// the link is a track), the title without its edition, and the artist
/// alone with the title somewhere in the record's.
fn search_source(ask: &dyn Fn(&str, usize) -> Option<Value>, title: &str, artist: &str, track: bool) -> Option<Value> {
    let full = if artist.is_empty() { title.to_string() } else { format!("{title} {artist}") };
    if let Some(answer) = ask(&full, 5) {
        if track {
            if let Some(found) = first(&answer, "tracks") {
                return Some(found);
            }
        }
        if let Some(found) = first(&answer, "albums") {
            return Some(found);
        }
    }
    let cleaned = clean_title(title);
    if cleaned != title && !cleaned.is_empty() {
        let query = if artist.is_empty() { cleaned.clone() } else { format!("{cleaned} {artist}") };
        if let Some(found) = ask(&query, 5).and_then(|answer| first(&answer, "albums")) {
            return Some(found);
        }
    }
    if !artist.is_empty() && artist != title {
        let answer = ask(artist, 10)?;
        let (title, cleaned) = (title.to_lowercase(), cleaned.to_lowercase());
        return answer["albums"].as_array()?.iter().find(|album| {
            let name = album["title"].as_str().unwrap_or_default().to_lowercase();
            !name.is_empty() && (name.contains(&cleaned) || cleaned.contains(&name) || name.contains(&title))
        }).cloned();
    }
    None
}

/// A track opens its album, as in QBZ: the record its `album_ref` names,
/// asked for with `track.get` when the track came without one.
fn album_of(natives: &Arc<crate::natives::Natives>, plugin: &str, track: &Value) -> Option<Value> {
    let full;
    let track = if track["album_ref"].as_str().is_some_and(|r| !r.is_empty()) {
        track
    } else {
        full = natives.call(plugin, "track.get", json!({"ref": track["ref"]}), ASK).ok()?;
        full.get("track").unwrap_or(&full)
    };
    let album = track["album_ref"].as_str().filter(|r| !r.is_empty())?;
    Some(json!({"kind": "album", "ref": album, "title": track["album"], "artist": track["artist"], "image": track["image"]}))
}

/// What one source makes of what it found: a track is its album.
fn source_found(natives: &Arc<crate::natives::Natives>, plugin: &str, name: &str, item: Value) -> Result<Found, String> {
    let item = if item["kind"] == "track" {
        album_of(natives, plugin, &item).ok_or_else(|| chilpa_i18n::t("This track has no album to open"))?
    } else {
        item
    };
    Ok(Found::Source { plugin: plugin.to_string(), name: name.to_string(), item })
}

fn resolve(url: &str, natives: &Arc<crate::natives::Natives>, sources: &[(String, String)]) -> Result<Found, String> {
    // A source that claims the link opens it.
    for (plugin, name) in sources {
        let Ok(answer) = natives.call(plugin, "link.resolve", json!({"url": url}), ASK) else { continue };
        if answer["ref"].as_str().is_some_and(|r| !r.is_empty()) {
            return source_found(natives, plugin, name, answer);
        }
    }
    // What the link is, from a plugin that knows other services' links.
    let what = natives
        .ids()
        .into_iter()
        .find_map(|id| natives.call(&id, "link.identify", json!({"url": url}), ASK).ok().filter(|v| v["kind"].is_string()))
        .ok_or_else(|| chilpa_i18n::t("Chilpa cannot read this link"))?;
    if what["kind"] == "playlist" {
        return Ok(Found::Playlist);
    }
    let text = |key: &str| what[key].as_str().unwrap_or_default().trim().to_string();
    let (title, artist, album) = (text("title"), text("artist"), text("album"));
    let track = what["kind"] == "track";
    // The library first, its best edition.
    let record = if track { album.clone() } else { title.clone() };
    if !record.is_empty() {
        if let Some(id) = in_library(&record, &artist) {
            return Ok(Found::Library(id));
        }
    }
    // Then the sources, in the sidebar's order.
    for (plugin, name) in sources {
        let ask = |text: &str, limit: usize| natives.call(plugin, "search", json!({"text": text, "limit": limit}), ASK).ok();
        if let Some(item) = search_source(&ask, &title, &artist, track) {
            return source_found(natives, plugin, name, item);
        }
    }
    Err(chilpa_i18n::t("Not in your library or in your sources"))
}

/// The best edition the library has of a record: the group's first, the
/// one that can play and sounds best. A local record opens in its best
/// quality.
fn in_library(title: &str, artist: &str) -> Option<i64> {
    let library = chilpa_library::Library::open_read_only(&chilpa_library::Library::default_path()).ok()?;
    let catalog = library.version_catalog().ok()?;
    catalog.best_of(title, artist).or_else(|| {
        let cleaned = clean_title(title);
        (cleaned != title).then(|| catalog.best_of(&cleaned, artist)).flatten()
    })
}

impl Chilpa {
    /// The sources a link is asked of, in the sidebar's order.
    fn link_sources(&self) -> Vec<(String, String)> {
        let ids: Vec<String> = self.natives.ids().into_iter().filter(|id| self.natives.searches(id)).collect();
        crate::plugin_sidebar::ordered(&Self::plugins_dir(), ids)
            .into_iter()
            .map(|id| {
                let name = self.natives.display_name(&id);
                (id, name)
            })
            .collect()
    }

    /// Starts resolving a link; a new one replaces the one in flight.
    pub fn open_link(&mut self, url: &str) {
        let url = url.trim().to_string();
        let generation = {
            let mut state = lock(&self.open_link);
            state.generation += 1;
            state.url = url.clone();
            state.found = None;
            state.error.clear();
            state.resolving = !url.is_empty();
            state.generation
        };
        if url.is_empty() {
            return;
        }
        let (natives, sources, shared) = (self.natives.clone(), self.link_sources(), self.open_link.clone());
        let started = std::thread::Builder::new().name("chilpa-open-link".into()).spawn(move || {
            let result = resolve(&url, &natives, &sources);
            let mut state = lock(&shared);
            if state.generation != generation {
                return;
            }
            state.resolving = false;
            match result {
                Ok(found) => state.found = Some(found),
                Err(error) => state.error = error,
            }
        });
        if started.is_err() {
            let mut state = lock(&self.open_link);
            state.resolving = false;
            state.error = chilpa_i18n::t("Chilpa cannot read this link");
        }
    }

    /// Forgets the link: the modal closed.
    pub fn close_link(&mut self) {
        let mut state = lock(&self.open_link);
        state.generation += 1;
        *state = LinkState { generation: state.generation, ..LinkState::default() };
    }

    /// How the link goes: `{resolving, error, playlist}`.
    pub fn open_link_state(&self) -> String {
        let state = lock(&self.open_link);
        json!({"resolving": state.resolving, "error": state.error, "playlist": state.found == Some(Found::Playlist)}).to_string()
    }

    /// Where the resolved link leads, once. A playlist stays for the
    /// importer's button.
    pub fn take_open_link(&mut self) -> Option<Leads> {
        let found = {
            let mut state = lock(&self.open_link);
            match state.found {
                None | Some(Found::Playlist) => return None,
                Some(_) => state.found.take()?,
            }
        };
        match found {
            Found::Library(id) => Some(Leads::Screen("album", id)),
            Found::Source { plugin, name, item } => Some(self.open_source_item(&plugin, &name, &item)),
            Found::Playlist => None,
        }
    }

    /// Something of a source opened as its search would open it: a record on
    /// its page, an artist on the source's page for it.
    fn open_source_item(&mut self, plugin: &str, name: &str, item: &Value) -> Leads {
        let kind = item["kind"].as_str().unwrap_or("album").to_string();
        let reference = item["ref"].as_str().unwrap_or_default().to_string();
        if kind == "artist" {
            return Leads::Source(format!("{plugin} native:{reference}"));
        }
        let row = crate::source_search::RemoteFound {
            source: plugin.to_string(),
            name: name.to_string(),
            id: reference,
            title: item["title"].as_str().unwrap_or_default().to_string(),
            detail: item["artist"].as_str().unwrap_or_default().to_string(),
            art: crate::pictures::kept(item["image"].as_str().unwrap_or_default()).unwrap_or_default(),
            duration_ms: 0,
            collection: true,
            entity: kind,
            item: Some(item.clone()),
        };
        self.act_on_remote_found(&row, "open")
    }

    /// The playlist the link names goes to the importer, which starts
    /// reading it; `false` when the link is not a playlist.
    pub fn open_link_in_importer(&mut self) -> bool {
        let url = {
            let state = lock(&self.open_link);
            if state.found != Some(Found::Playlist) {
                return false;
            }
            state.url.clone()
        };
        self.playlist_import_command("preview", &json!({"input": "url", "text": url}).to_string());
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_edition_is_cut_from_the_title() {
        assert_eq!(clean_title("Senjutsu (2021 Remaster)"), "Senjutsu");
        assert_eq!(clean_title("The Number of the Beast [Deluxe Edition]"), "The Number of the Beast");
        assert_eq!(clean_title("Plain"), "Plain");
        assert_eq!(clean_title("Open (no close"), "Open (no close");
    }

    #[test]
    fn the_three_searches_go_in_qbz_order() {
        let asked = std::cell::RefCell::new(Vec::new());
        let answers = |text: &str, limit: usize| -> Option<Value> {
            asked.borrow_mut().push((text.to_string(), limit));
            match text {
                "Senjutsu Iron Maiden" => Some(json!({"albums": [], "tracks": []})),
                "Senjutsu (2021 Remaster) Iron Maiden" => Some(json!({"albums": []})),
                "Iron Maiden" => Some(json!({"albums": [{"ref": "album:1", "title": "Killers"}, {"ref": "album:2", "title": "Senjutsu"}]})),
                _ => None,
            }
        };
        let found = search_source(&answers, "Senjutsu (2021 Remaster)", "Iron Maiden", false).unwrap();
        assert_eq!(found["ref"], "album:2", "the artist's records, the one with the title");
        assert_eq!(*asked.borrow(), vec![
            ("Senjutsu (2021 Remaster) Iron Maiden".to_string(), 5),
            ("Senjutsu Iron Maiden".to_string(), 5),
            ("Iron Maiden".to_string(), 10),
        ]);
    }

    #[test]
    fn a_track_link_finds_the_track_first() {
        let answers = |_: &str, _: usize| Some(json!({"tracks": [{"kind": "track", "ref": "track:9"}], "albums": [{"ref": "album:1"}]}));
        assert_eq!(search_source(&answers, "Song", "Band", true).unwrap()["ref"], "track:9");
        assert_eq!(search_source(&answers, "Song", "Band", false).unwrap()["ref"], "album:1");
    }

    #[test]
    fn nothing_found_is_none() {
        let answers = |_: &str, _: usize| Some(json!({"albums": [{"ref": "album:1", "title": "Other"}]}));
        assert!(search_source(&|_: &str, _: usize| None, "Song", "Band", false).is_none());
        assert_eq!(search_source(&answers, "Song", "", false).unwrap()["ref"], "album:1", "QBZ takes the first record of the full query");
    }
}
