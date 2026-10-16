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
//!    I try the sources in sidebar order with three query variants.
//!
//! The work runs on a thread of its own; the window asks how it goes.

use crate::{Chilpa, Leads};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// How long one plugin has to answer one question.
const ASK: Duration = Duration::from_secs(15);
const IDENTIFY: Duration = Duration::from_secs(35);

/// What a link came to.
#[derive(Clone, Debug, PartialEq)]
enum Found {
    /// A record of the library: its best edition.
    Library { id: i64, title: String, artist: String },
    /// Something of a source, as its search shows it.
    Source { plugin: String, name: String, item: Value },
    /// A playlist of another service, for the importer.
    Playlist,
}

impl Found {
    fn choice(&self) -> Value {
        match self {
            Self::Library { title, artist, .. } => json!({"source": "Chilpa", "title": title, "artist": artist}),
            Self::Source { name, item, .. } => json!({"source": name, "title": item["title"], "artist": item["artist"]}),
            Self::Playlist => Value::Null,
        }
    }
}

#[derive(Default)]
pub(crate) struct LinkState {
    url: String,
    resolving: bool,
    error: String,
    found: Option<Found>,
    choices: Vec<Found>,
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
fn match_item(answer: &Value, list: &str, title: &str, artist: &str) -> Option<Value> {
    let wanted = clean_title(title).to_lowercase();
    let artist = artist.to_lowercase();
    answer[list].as_array()?.iter().find(|item| {
        let name = item["title"].as_str().unwrap_or_default().to_lowercase();
        let credit = item["artist"].as_str().unwrap_or_default().to_lowercase();
        item["ref"].as_str().is_some_and(|r| !r.is_empty())
            && !wanted.is_empty()
            && (if list == "tracks" { name == title.to_lowercase() }
                else { clean_title(&name).to_lowercase() == wanted || name == title.to_lowercase() })
            && (artist.is_empty() || credit.contains(&artist) || artist.contains(&credit) && !credit.is_empty())
    }).cloned()
}

/// QBZ's three searches in one source: "title artist" (a track first when
/// the link is a track), the title without its edition, and the artist
/// alone with the title somewhere in the record's.
fn search_source(ask: &dyn Fn(&str, usize) -> Option<Value>, title: &str, artist: &str, track: bool) -> Option<Value> {
    let full = if artist.is_empty() { title.to_string() } else { format!("{title} {artist}") };
    if let Some(answer) = ask(&full, 5) {
        if track {
            return match_item(&answer, "tracks", title, artist);
        }
        if let Some(found) = match_item(&answer, "albums", title, artist) {
            return Some(found);
        }
    }
    if track { return None }
    let cleaned = clean_title(title);
    if cleaned != title && !cleaned.is_empty() {
        let query = if artist.is_empty() { cleaned.clone() } else { format!("{cleaned} {artist}") };
        if let Some(found) = ask(&query, 5).and_then(|answer| match_item(&answer, "albums", title, artist)) {
            return Some(found);
        }
    }
    if !artist.is_empty() && artist != title {
        let answer = ask(artist, 10)?;
        return match_item(&answer, "albums", &cleaned, artist);
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

enum Resolution { Direct(Found), Choices(Vec<Found>) }

fn resolve(url: &str, natives: &Arc<crate::natives::Natives>, sources: &[(String, String)]) -> Result<Resolution, String> {
    // Ask source resolvers together so an unrelated plugin cannot hold the
    // link modal for another source's timeout. Keep sidebar priority.
    let direct = std::thread::scope(|scope| {
        let handles: Vec<_> = sources.iter().map(|(plugin, name)| scope.spawn(move || {
            let answer = natives.call(plugin, "link.resolve", json!({"url": url}), ASK).ok()?;
            answer["ref"].as_str().filter(|r| !r.is_empty())?;
            source_found(natives, plugin, name, answer).ok()
        })).collect();
        handles.into_iter().filter_map(|handle| handle.join().ok().flatten()).next()
    });
    if let Some(found) = direct { return Ok(Resolution::Direct(found)) }
    // What the link is, from a plugin that knows other services' links.
    let mut identifiers = natives.ids();
    identifiers.sort_by_key(|id| !natives.imports_playlists(id));
    let what = identifiers.into_iter()
        .find_map(|id| natives.call(&id, "link.identify", json!({"url": url}), IDENTIFY).ok().filter(|v| v["kind"].is_string()))
        .ok_or_else(|| chilpa_i18n::t("Chilpa cannot read this link"))?;
    if what["kind"] == "playlist" {
        return Ok(Resolution::Direct(Found::Playlist));
    }
    let text = |key: &str| what[key].as_str().unwrap_or_default().trim().to_string();
    let (title, artist, album) = (text("title"), text("artist"), text("album"));
    let track = what["kind"] == "track";
    // Keep every verified destination so the user can choose a source.
    let record = if track { album.clone() } else { title.clone() };
    let mut choices = Vec::new();
    if !record.is_empty() {
        if let Some(id) = in_library(&record, &artist) {
            choices.push(Found::Library { id, title: record, artist: artist.clone() });
        }
    }
    if track && choices.is_empty() {
        if let Some((id, album)) = local_track_album(&title, &artist) {
            choices.push(Found::Library { id, title: album, artist: artist.clone() });
        }
    }
    // Search sources concurrently, then present results in sidebar order.
    let remote = std::thread::scope(|scope| {
        let handles: Vec<_> = sources.iter().map(|(plugin, name)| {
            let (title, artist, album) = (&title, &artist, &album);
            scope.spawn(move || {
                let ask = |text: &str, limit: usize| natives.call(plugin, "search", json!({"text": text, "limit": limit}), ASK).ok();
                let found = search_source(&ask, title, artist, track);
                let found = if track && found.is_none() && !album.is_empty() {
                    search_source(&ask, album, artist, false)
                } else { found }?;
                source_found(natives, plugin, name, found).ok()
            })
        }).collect();
        handles.into_iter().filter_map(|handle| handle.join().ok().flatten()).collect::<Vec<_>>()
    });
    choices.extend(remote);
    if choices.is_empty() { Err(chilpa_i18n::t("Not in your library or in your sources")) }
    else { Ok(Resolution::Choices(choices)) }
}

fn local_track_album(title: &str, artist: &str) -> Option<(i64, String)> {
    let library = chilpa_library::Library::open_read_only(&chilpa_library::Library::default_path()).ok()?;
    library.album_of_matching_track(title, artist).ok().flatten()
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
            state.choices.clear();
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
                Ok(Resolution::Direct(found)) => state.found = Some(found),
                Ok(Resolution::Choices(choices)) => state.choices = choices,
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

    /// How the link goes, including destination choices for foreign links.
    pub fn open_link_state(&self) -> String {
        let state = lock(&self.open_link);
        json!({"resolving": state.resolving, "error": state.error, "playlist": state.found == Some(Found::Playlist),
            "choices": state.choices.iter().map(Found::choice).collect::<Vec<_>>()}).to_string()
    }

    pub fn choose_open_link(&mut self, index: usize) -> Option<Leads> {
        let found = {
            let mut state = lock(&self.open_link);
            if index >= state.choices.len() { return None }
            state.choices.remove(index)
        };
        self.lead_for_link(found)
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
        self.lead_for_link(found)
    }

    fn lead_for_link(&mut self, found: Found) -> Option<Leads> {
        match found {
            Found::Library { id, .. } => Some(Leads::Screen("album", id)),
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
            no_audio: false,
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
                "Iron Maiden" => Some(json!({"albums": [{"ref": "album:1", "title": "Killers", "artist": "Iron Maiden"}, {"ref": "album:2", "title": "Senjutsu", "artist": "Iron Maiden"}]})),
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
        let answers = |_: &str, _: usize| Some(json!({"tracks": [{"kind": "track", "ref": "track:9", "title": "Song", "artist": "Band"}], "albums": [{"ref": "album:1", "title": "Song", "artist": "Band"}]}));
        assert_eq!(search_source(&answers, "Song", "Band", true).unwrap()["ref"], "track:9");
        assert_eq!(search_source(&answers, "Song", "Band", false).unwrap()["ref"], "album:1");
        let only_album = |_: &str, _: usize| Some(json!({"tracks": [], "albums": [{"ref": "album:1", "title": "Song", "artist": "Band"}]}));
        assert!(search_source(&only_album, "Song", "Band", true).is_none());
    }

    #[test]
    fn nothing_found_is_none() {
        let answers = |_: &str, _: usize| Some(json!({"albums": [{"ref": "album:1", "title": "Other"}]}));
        assert!(search_source(&|_: &str, _: usize| None, "Song", "Band", false).is_none());
        assert!(search_source(&answers, "Song", "", false).is_none());
    }
}
