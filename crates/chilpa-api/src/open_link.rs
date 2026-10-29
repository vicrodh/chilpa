//! Music links resolve across the local library and enabled sources.
//! Public metadata identifies foreign links without installing their source.
//! One destination opens directly; several are offered in sidebar order.

use crate::{Chilpa, Leads};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use chilpa_meta::{Http, music_link::Identity};

/// How long one plugin has to answer one question.
const ASK: Duration = Duration::from_secs(15);
const IDENTIFY: Duration = Duration::from_secs(35);
// At most four transient source requests per resolution; no idle worker or cache.
const PARALLEL: usize = 4;
type Ask<'a> = dyn Fn(&str, &str, Value) -> Option<Value> + Sync + 'a;

/// What a link came to.
#[derive(Clone, Debug, PartialEq)]
enum Found {
    /// A record of the library: its best edition.
    Library { id: i64, title: String, artist: String },
    Artist(String),
    /// Something of a source, as its search shows it.
    Source { plugin: String, name: String, item: Value },
    /// A playlist of another service, for the importer.
    Playlist,
}

impl Found {
    fn choice(&self) -> Value {
        match self {
            Self::Library { title, artist, .. } => json!({"source": chilpa_i18n::t("Library"), "title": title, "artist": artist, "local": true, "kind": "album"}),
            Self::Artist(title) => json!({"source": chilpa_i18n::t("Library"), "title": title, "local": true, "kind": "artist"}),
            Self::Source { name, item, .. } => json!({"source": name, "title": item["title"], "artist": item["artist"], "kind": item["kind"], "local": false}),
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
    // One running resolution and one replaceable pending request per facade.
    working: bool,
    request: Option<LinkRequest>,
}

struct LinkRequest { generation: u64, url: String, sources: Vec<(String, String)> }

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
fn album_of(ask: &Ask<'_>, plugin: &str, track: &Value) -> Option<Value> {
    let full;
    let track = if track["album_ref"].as_str().is_some_and(|r| !r.is_empty()) {
        track
    } else {
        full = ask(plugin, "track.get", json!({"ref": track["ref"]}))?;
        full.get("track").unwrap_or(&full)
    };
    let album = track["album_ref"].as_str().filter(|r| !r.is_empty())?;
    Some(json!({"kind": "album", "ref": album, "title": track["album"], "artist": track["artist"], "image": track["image"]}))
}

/// What one source makes of what it found: a track is its album.
fn source_found(ask: &Ask<'_>, plugin: &str, name: &str, item: Value) -> Result<Found, String> {
    let item = if item["kind"] == "track" {
        album_of(ask, plugin, &item).ok_or_else(|| chilpa_i18n::t("This track has no album to open"))?
    } else {
        item
    };
    Ok(Found::Source { plugin: plugin.to_string(), name: name.to_string(), item })
}

#[derive(Debug, PartialEq)]
enum Resolution { Direct(Found), Choices(Vec<Found>) }

fn together<T: Send>(sources: &[(String, String)], work: impl Fn(&str, &str) -> Option<T> + Sync) -> Vec<T> {
    sources.chunks(PARALLEL).flat_map(|chunk| std::thread::scope(|scope| {
        let work = &work;
        let handles: Vec<_> = chunk.iter().map(|(plugin, name)| scope.spawn(move || work(plugin, name))).collect();
        handles.into_iter().filter_map(|handle| handle.join().ok().flatten()).collect::<Vec<_>>()
    })).collect()
}

fn identity(item: &Value) -> Identity {
    let text = |key: &str| item[key].as_str().unwrap_or_default().trim().to_string();
    Identity { kind: text("kind"), title: text("title"), artist: text("artist"), album: text("album") }
}

fn public_identity(url: &str) -> Option<Identity> {
    let url = url::Url::parse(url).ok()?;
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() { return None; }
    let reply = crate::metadata::worker::Network::new().get(url.as_str(), "text/html", ASK, 2 * 1024 * 1024).ok()?;
    if reply.status != 200 { return None; }
    chilpa_meta::music_link::identify(std::str::from_utf8(&reply.body).ok()?)
}

fn resolve_with(url: &str, sources: &[(String, String)], identifiers: &[String], ask: &Ask<'_>,
    web: &dyn Fn(&str) -> Option<Identity>, local: &dyn Fn(&Identity) -> Option<Found>) -> Result<Resolution, String> {
    let direct = together(sources, |plugin, name| {
        let answer = ask(plugin, "link.resolve", json!({"url": url}))?;
        answer["ref"].as_str().filter(|r| !r.is_empty())?;
        let found = source_found(ask, plugin, name, answer.clone()).ok()?;
        Some((plugin.to_string(), answer, found))
    });
    // A resolver contributes a destination and metadata, never an early exit.
    let what = direct.iter().map(|(_, item, _)| identity(item)).find(Identity::usable)
        .or_else(|| web(url))
        .or_else(|| identifiers.iter().find_map(|id| ask(id, "link.identify", json!({"url": url}))
            .map(|value| identity(&value)).filter(Identity::usable)));
    let Some(what) = what else {
        if direct.is_empty() { return Err(chilpa_i18n::t("Chilpa cannot read this link")); }
        return finish_choices(direct.into_iter().map(|(_, _, found)| found).collect());
    };
    if what.kind == "playlist" {
        return if direct.is_empty() { Ok(Resolution::Direct(Found::Playlist)) }
            else { finish_choices(direct.into_iter().map(|(_, _, found)| found).collect()) };
    }
    let mut choices: Vec<_> = local(&what).into_iter().collect();
    let remote = together(sources, |plugin, name| {
        if let Some((_, _, found)) = direct.iter().find(|(id, _, _)| id == plugin) { return Some(found.clone()); }
        let search = |text: &str, limit: usize| ask(plugin, "search", json!({"text": text, "limit": limit}));
        let found = if what.kind == "artist" {
            let answer = search(&what.title, 5)?;
            answer["artists"].as_array()?.iter().find(|item| item["title"].as_str()
                .is_some_and(|title| title.to_lowercase() == what.title.to_lowercase())
                && item["ref"].as_str().is_some_and(|r| !r.is_empty())).cloned()
        } else {
            let track = what.kind == "track";
            search_source(&search, &what.title, &what.artist, track).or_else(|| {
                (track && !what.album.is_empty()).then(|| search_source(&search, &what.album, &what.artist, false)).flatten()
            })
        }?;
        source_found(ask, plugin, name, found).ok()
    });
    choices.extend(remote);
    finish_choices(choices)
}

fn finish_choices(mut choices: Vec<Found>) -> Result<Resolution, String> {
    match choices.len() {
        0 => Err(chilpa_i18n::t("Not in your library or in your sources")),
        1 => Ok(Resolution::Direct(choices.remove(0))),
        _ => Ok(Resolution::Choices(choices)),
    }
}

fn local_destination(what: &Identity) -> Option<Found> {
    if what.kind == "artist" {
        let library = chilpa_library::Library::open_read_only(&chilpa_library::Library::default_path()).ok()?;
        return library.has_artist(&what.title).ok().filter(|found| *found).map(|_| Found::Artist(what.title.clone()));
    }
    // A track must exist locally; merely owning its album is not a match.
    if what.kind == "track" {
        let (id, title) = local_track_album(&what.title, &what.artist)?;
        return Some(Found::Library { id, title, artist: what.artist.clone() });
    }
    let id = in_library(&what.title, &what.artist)?;
    Some(Found::Library { id, title: what.title.clone(), artist: what.artist.clone() })
}

fn resolve(url: &str, natives: &Arc<crate::natives::Natives>, sources: &[(String, String)], current: &(dyn Fn() -> bool + Sync)) -> Result<Resolution, String> {
    let mut identifiers = natives.ids();
    identifiers.sort_by_key(|id| (!natives.imports_playlists(id), id.clone()));
    let ask = |plugin: &str, method: &str, params: Value| {
        if !current() { return None; }
        natives.call(plugin, method, params, if method == "link.identify" { IDENTIFY } else { ASK }).ok()
    };
    let web = |url: &str| if current() { public_identity(url) } else { None };
    let local = |what: &Identity| if current() { local_destination(what) } else { None };
    resolve_with(url, sources, &identifiers, &ask, &web, &local)
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
        if !url::Url::parse(&url).is_ok_and(|url| url.scheme() == "https" && url.host_str().is_some()
            && url.username().is_empty() && url.password().is_none()) {
            let mut state = lock(&self.open_link);
            state.resolving = false;
            state.error = chilpa_i18n::t("Paste a valid HTTPS music link");
            return;
        }
        let sources = self.link_sources();
        {
            let mut state = lock(&self.open_link);
            state.request = Some(LinkRequest { generation, url, sources });
            if state.working { return; }
            state.working = true;
        }
        let (natives, shared) = (self.natives.clone(), self.open_link.clone());
        let started = std::thread::Builder::new().name("chilpa-open-link".into()).spawn(move || loop {
            let request = {
                let mut state = lock(&shared);
                match state.request.take() {
                    Some(request) => request,
                    None => { state.working = false; return; }
                }
            };
            let current = || lock(&shared).generation == request.generation;
            let result = resolve(&request.url, &natives, &request.sources, &current);
            let mut state = lock(&shared);
            if state.generation == request.generation {
                state.resolving = false;
                match result {
                    Ok(Resolution::Direct(found)) => state.found = Some(found),
                    Ok(Resolution::Choices(choices)) => state.choices = choices,
                    Err(error) => state.error = error,
                }
            }
        });
        if started.is_err() {
            let mut state = lock(&self.open_link);
            state.working = false;
            state.request = None;
            state.resolving = false;
            state.error = chilpa_i18n::t("Chilpa cannot read this link");
        }
    }

    /// Forgets the link: the modal closed.
    pub fn close_link(&mut self) {
        let mut state = lock(&self.open_link);
        state.generation += 1;
        *state = LinkState { generation: state.generation, working: state.working, ..LinkState::default() };
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
            Found::Artist(name) => Some(Leads::Source(format!("local artist:{name}"))),
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

    fn record() -> Value { json!({"kind":"album","ref":"album:1","title":"Record","artist":"Band"}) }
    fn metadata(_: &str) -> Option<Identity> {
        Some(Identity {kind:"album".into(), title:"Record".into(), artist:"Band".into(), album:String::new()})
    }
    fn sources() -> Vec<(String, String)> { vec![("destination".into(), "Destination".into())] }

    #[test]
    fn foreign_link_opens_the_only_matching_source_without_an_origin_plugin() {
        let ask = |_: &str, method: &str, _: Value| (method == "search").then(|| json!({"albums":[record()]}));
        let answer = resolve_with("https://origin.example/album/1", &sources(), &[], &ask, &metadata, &|_| None).unwrap();
        assert!(matches!(answer, Resolution::Direct(Found::Source { plugin, .. }) if plugin == "destination"));
    }

    #[test]
    fn foreign_link_can_open_locally_without_any_plugins() {
        let local = Found::Library {id:7, title:"Record".into(), artist:"Band".into()};
        let answer = resolve_with("https://origin.example/album/1", &[], &[], &|_, _, _| None, &metadata, &|_| Some(local.clone())).unwrap();
        assert_eq!(answer, Resolution::Direct(local));
    }

    #[test]
    fn local_and_remote_matches_are_choices_in_source_order() {
        let sources = vec![("second".into(), "Second".into()), ("first".into(), "First".into())];
        let ask = |_: &str, method: &str, _: Value| (method == "search").then(|| json!({"albums":[record()]}));
        let answer = resolve_with("https://origin.example/album/1", &sources, &[], &ask, &metadata,
            &|_| Some(Found::Library {id:7, title:"Record".into(), artist:"Band".into()})).unwrap();
        let Resolution::Choices(rows) = answer else { panic!("multiple matches must offer a choice") };
        assert!(matches!(rows[0], Found::Library {id:7, ..}));
        assert_eq!(rows[1].choice()["source"], "Second");
        assert_eq!(rows[2].choice()["source"], "First");
    }

    #[test]
    fn recognizing_the_link_does_not_bypass_other_sources_or_the_library() {
        let sources = vec![("origin".into(), "Origin".into()), ("destination".into(), "Destination".into())];
        let ask = |plugin: &str, method: &str, _: Value| match (plugin, method) {
            ("origin", "link.resolve") => Some(record()),
            ("origin", "search") => panic!("a resolved source must not be searched twice"),
            ("destination", "search") => Some(json!({"albums":[record()]})),
            _ => None,
        };
        let answer = resolve_with("https://origin.example/album/1", &sources, &[], &ask, &|_| panic!("direct metadata is sufficient"),
            &|_| Some(Found::Library {id:7, title:"Record".into(), artist:"Band".into()})).unwrap();
        assert!(matches!(answer, Resolution::Choices(rows) if rows.len() == 3));
    }

    #[test]
    fn an_identification_plugin_remains_a_fallback_for_private_links() {
        let ask = |_: &str, method: &str, _: Value| match method {
            "link.identify" => Some(record()), "search" => Some(json!({"albums":[record()]})), _ => None,
        };
        assert!(matches!(resolve_with("https://origin.example/1", &sources(), &["identifier".into()], &ask, &|_| None, &|_| None), Ok(Resolution::Direct(_))));
    }

    #[test]
    fn foreign_playlists_reach_the_importer_and_unknown_pages_report_an_error() {
        let playlist = |_: &str| Some(Identity {kind:"playlist".into(), ..Default::default()});
        assert_eq!(resolve_with("https://origin.example/list", &[], &[], &|_, _, _| None, &playlist, &|_| None).unwrap(), Resolution::Direct(Found::Playlist));
        assert!(resolve_with("https://origin.example/page", &[], &[], &|_, _, _| None, &|_| None, &|_| None).is_err());
    }

    #[test]
    fn tracks_use_the_album_reference_of_the_matching_source() {
        let ask = |_: &str, method: &str, _: Value| (method == "search").then(|| json!({"tracks":[{
            "kind":"track","ref":"track:1","title":"Song","artist":"Band","album":"Record","album_ref":"album:remote"}]}));
        let metadata = |_: &str| Some(Identity {kind:"track".into(), title:"Song".into(), artist:"Band".into(), album:"Record".into()});
        let answer = resolve_with("https://origin.example/song", &sources(), &[], &ask, &metadata, &|_| None).unwrap();
        let Resolution::Direct(Found::Source {item, ..}) = answer else { panic!("one matched track opens its album") };
        assert_eq!(item["ref"], "album:remote");
        assert_eq!(item["kind"], "album");
    }

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
