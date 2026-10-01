//! Sources that are browsed: a plugin's own tree, walked from the window.
//!
//! A plugin that asked for a place in the window is a tree only it knows the
//! shape of. The window stands somewhere in it and sees one listing; this
//! module keeps where it stands, asks the plugin for the listing, and plays
//! what the listener picks.
//!
//! **Every call to a plugin here runs on its own thread.** Listing a folder
//! goes out to the network and back, and resolving a station does too; the
//! window cannot wait on either. The answer arrives through
//! [`Chilpa::poll`], and one that belongs to a place the listener already
//! left is dropped.
//!
//! What the listener saves is kept by the library, not by the plugin: it
//! survives the plugin being reinstalled and lists with the network down.

use crate::Chilpa;
use chilpa_library::store::saved::SavedEntry;
use chilpa_plugin::messages::{self, Browse, Entry, EntryKind, Listing, Resolve, Stream};
use std::collections::HashSet;
use std::sync::mpsc;

/// The id of the one folder that is Chilpa's and not the plugin's: what the
/// listener saved from that source. A plugin never sees it.
const SAVED: &str = "chilpa:saved";

/// What an entry's id starts with when it is a stream address the listener
/// typed. Chilpa plays it itself: no plugin gave it out, so none is asked.
const ADDRESS: &str = "chilpa:url:";

/// A plugin with an entry of its own in the window.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct Panel {
    pub id: String,
    pub name: String,
    /// The name of an icon, as a hint. Empty when it gave none.
    pub icon: String,
}

/// A step of the way from the root to where the window stands.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Crumb {
    id: String,
    title: String,
}

/// Something of a source that is sounding, with the card it was listed with.
#[derive(Clone, Debug)]
struct Sounding {
    url: String,
    source: String,
    entry: Entry,
    /// The station's own picture, once it is on disk. A song's cover goes
    /// over it and this comes back when the next song has none.
    picture: String,
}

/// What a thread asking a plugin brings back.
enum Answer {
    Listing { generation: u64, cursor: Option<String>, result: Result<Listing, String> },
    Stream { source: String, name: String, entry: Entry, result: Result<Stream, String> },
    /// A picture for what sounds from `url`, on disk. `song` is the title
    /// it is the cover of, or empty when it is the station's own.
    Picture { url: String, song: String, path: String },
}

/// Where the window stands in a source, and what it sees there.
pub(crate) struct SourcesState {
    panels: Vec<Panel>,
    /// The plugin being browsed. Empty before any was opened.
    current: String,
    trail: Vec<Crumb>,
    /// The root's place to type, if the plugin offers one.
    search_id: String,
    text: String,
    /// What narrows the listing on screen without asking anybody.
    filter: String,
    title: String,
    entries: Vec<Entry>,
    next: Option<String>,
    loading: bool,
    error: String,
    /// The ids saved from the current source.
    saved: HashSet<String>,
    /// The entry whose address is being asked for, so its row can say so.
    resolving: String,
    sounding: Option<Sounding>,
    /// Counts the places visited: an answer carrying an older number is for
    /// a place that was left.
    generation: u64,
    tx: mpsc::Sender<Answer>,
    rx: mpsc::Receiver<Answer>,
}

impl Default for SourcesState {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            panels: Vec::new(),
            current: String::new(),
            trail: Vec::new(),
            search_id: String::new(),
            text: String::new(),
            filter: String::new(),
            title: String::new(),
            entries: Vec::new(),
            next: None,
            loading: false,
            error: String::new(),
            saved: HashSet::new(),
            resolving: String::new(),
            sounding: None,
            generation: 0,
            tx,
            rx,
        }
    }
}

fn kind_name(kind: EntryKind) -> &'static str {
    match kind {
        EntryKind::Folder => "folder",
        EntryKind::Item => "item",
        EntryKind::Search => "search",
    }
}

fn from_saved(saved: SavedEntry) -> Entry {
    Entry {
        id: saved.external_id,
        title: saved.title,
        subtitle: saved.subtitle,
        image_url: Some(saved.image_url).filter(|u| !u.is_empty()),
        kind: if saved.kind == "folder" { EntryKind::Folder } else { EntryKind::Item },
        live: saved.live,
        playable: true,
        ..Entry::default()
    }
}

/// Checks a stream before it is believed, the way a search hit's is: an
/// address that is not http is not attempted, and neither is a delivery
/// Chilpa cannot read or a link that already expired.
fn playable_url(stream: &Stream) -> Result<String, String> {
    if !stream.usable() {
        return Err(chilpa_i18n::t("it answered something that is not an http address"));
    }
    if !stream.delivery.playable() {
        return Err(chilpa_i18n::t_args("it delivers {}", &[&messages::describe_delivery(stream.delivery)]));
    }
    if stream.expired(crate::now_unix()) {
        return Err(chilpa_i18n::t("the link it gave has already expired"));
    }
    Ok(stream.parts().join(" "))
}

impl Chilpa {
    /// The plugins with an entry of their own in the window.
    ///
    /// The list is read again whenever the sandbox is free. While a call to
    /// a plugin is in flight the last list stands: waiting for the lock here
    /// would hold the window for as long as that call takes.
    pub fn source_panels(&mut self) -> Vec<Panel> {
        if let Some(plugins) = self.plugins.as_ref().and_then(|p| p.try_lock().ok()) {
            self.sources.panels =
                plugins.panels().into_iter().map(|(id, name, icon)| Panel { id, name, icon }).collect();
        }
        self.sources.panels.clone()
    }

    /// What a source is called, for its tab and its header.
    pub fn source_name(&self, id: &str) -> String {
        self.sources.panels.iter().find(|p| p.id == id).map(|p| p.name.clone()).unwrap_or_default()
    }

    /// Everything the source screen and the sidebar draw, as one JSON
    /// object. The window reads it again whenever it changed.
    pub fn sources_json(&mut self) -> String {
        let panels = self.source_panels();
        let s = &self.sources;
        let sounding = s
            .sounding
            .as_ref()
            .filter(|p| self.now.playing && self.now.path == p.url && p.source == s.current)
            .map(|p| p.entry.id.as_str())
            .unwrap_or_default();
        let needle = s.filter.to_lowercase();
        let entries: Vec<serde_json::Value> = s
            .entries
            .iter()
            .filter(|e| {
                needle.is_empty()
                    || e.title.to_lowercase().contains(&needle)
                    || e.subtitle.to_lowercase().contains(&needle)
            })
            .map(|e| {
                serde_json::json!({
                    "id": e.id,
                    "title": e.title,
                    "subtitle": e.subtitle,
                    "image": e.image_url.clone().unwrap_or_default(),
                    "kind": kind_name(e.kind),
                    "live": e.live,
                    "playable": e.playable,
                    "whyNot": e.why_not,
                    "saved": s.saved.contains(&e.id),
                    "sounding": e.id == sounding,
                    "resolving": e.id == s.resolving,
                })
            })
            .collect();
        let crumbs: Vec<&str> = s.trail.iter().map(|c| c.title.as_str()).collect();
        serde_json::json!({
            "panels": panels,
            "source": s.current,
            "name": self.source_name(&s.current),
            "title": s.title,
            "crumbs": crumbs,
            "entries": entries,
            "searchable": !s.search_id.is_empty(),
            "text": s.text,
            "filter": s.filter,
            "total": s.entries.len(),
            "loading": s.loading,
            "more": s.next.is_some(),
            "error": s.error,
            "inSaved": s.trail.last().is_some_and(|c| c.id == SAVED),
        })
        .to_string()
    }

    /// What the window asks of a source. `id` and `text` mean what the
    /// action needs them to:
    ///
    /// - `open`: start browsing the plugin `id`, at its root.
    /// - `enter`: open the folder `id` of the listing, or play the item.
    /// - `crumb`: go back to the step numbered `id`; zero is the root.
    /// - `search`: list what matches `text`; empty goes back to the root. A
    ///   text that is an http address is played as a stream instead.
    /// - `filter`: narrow the listing on screen to what carries `text`.
    /// - `more`: bring the next page of the listing.
    /// - `play`: play the item `id`.
    /// - `save`: save the entry `id`, or take it out if it was saved.
    /// - `retry`: ask again for the listing that failed.
    pub fn sources_command(&mut self, action: &str, id: &str, text: &str) {
        match action {
            "open" => self.open_source(id),
            "enter" => self.enter_entry(id),
            "crumb" => {
                self.sources.trail.truncate(id.parse().unwrap_or(0));
                self.sources.text.clear();
                self.load_listing();
            }
            "search" => self.search_source(text),
            "filter" => self.sources.filter = text.trim().to_string(),
            "more" => self.load_more(),
            "play" => self.play_entry(id),
            "save" => self.toggle_saved(id),
            "retry" => self.load_listing(),
            _ => {}
        }
    }

    fn open_source(&mut self, id: &str) {
        let s = &mut self.sources;
        if s.current == id && (s.loading || !s.entries.is_empty()) {
            // Coming back to it leaves it where it was.
            return;
        }
        s.current = id.to_string();
        s.trail.clear();
        s.text.clear();
        s.search_id.clear();
        self.read_saved();
        self.load_listing();
    }

    fn enter_entry(&mut self, id: &str) {
        let Some(entry) = self.sources.entries.iter().find(|e| e.id == id).cloned() else { return };
        match entry.kind {
            EntryKind::Folder => {
                self.sources.trail.push(Crumb { id: entry.id, title: entry.title });
                self.load_listing();
            }
            EntryKind::Item => self.play_entry(id),
            EntryKind::Search => {}
        }
    }

    fn search_source(&mut self, text: &str) {
        let text = text.trim();
        if text.starts_with("http://") || text.starts_with("https://") {
            self.open_address(text);
            return;
        }
        let s = &mut self.sources;
        if s.search_id.is_empty() || text == s.text {
            return;
        }
        s.text = text.to_string();
        s.trail.clear();
        if !text.is_empty() {
            s.trail.push(Crumb { id: s.search_id.clone(), title: format!("“{text}”") });
        }
        self.load_listing();
    }

    /// A stream address the listener typed: it is listed as one entry, so
    /// it can be saved like any other, and it starts playing.
    fn open_address(&mut self, address: &str) {
        let host = address.split("://").nth(1).unwrap_or(address).split('/').next().unwrap_or(address);
        let entry = Entry {
            id: format!("{ADDRESS}{address}"),
            title: host.to_string(),
            subtitle: address.to_string(),
            kind: EntryKind::Item,
            live: true,
            playable: true,
            ..Entry::default()
        };
        let s = &mut self.sources;
        s.generation += 1;
        s.loading = false;
        s.error.clear();
        s.filter.clear();
        s.next = None;
        s.text = address.to_string();
        s.title = chilpa_i18n::t("Stream address");
        s.trail = vec![Crumb { id: entry.id.clone(), title: s.title.clone() }];
        s.entries = vec![entry.clone()];
        self.play_entry(&entry.id);
    }

    fn read_saved(&mut self) {
        let source = self.sources.current.clone();
        let saved = self.library().and_then(|lib| lib.saved_entries(&source).ok()).unwrap_or_default();
        self.sources.saved = saved.into_iter().map(|e| e.external_id).collect();
    }

    /// Asks for the listing of where the window stands, from its beginning.
    fn load_listing(&mut self) {
        let s = &mut self.sources;
        s.generation += 1;
        s.entries.clear();
        s.filter.clear();
        s.next = None;
        s.error.clear();
        s.title = s.trail.last().map(|c| c.title.clone()).unwrap_or_default();
        let at = s.trail.last().map(|c| c.id.clone()).unwrap_or_default();
        if at == SAVED {
            let source = s.current.clone();
            let saved = self.library().and_then(|lib| lib.saved_entries(&source).ok()).unwrap_or_default();
            self.sources.entries = saved.into_iter().map(from_saved).collect();
            self.sources.loading = false;
            return;
        }
        self.ask_listing(at, None);
    }

    fn load_more(&mut self) {
        let s = &self.sources;
        if s.loading {
            return;
        }
        let Some(cursor) = s.next.clone() else { return };
        let at = s.trail.last().map(|c| c.id.clone()).unwrap_or_default();
        self.ask_listing(at, Some(cursor));
    }

    fn ask_listing(&mut self, at: String, cursor: Option<String>) {
        let language = self.language().to_string();
        let s = &mut self.sources;
        s.loading = true;
        let text = (at == s.search_id && !s.text.is_empty()).then(|| s.text.clone());
        let ask = Browse { at, cursor: cursor.clone(), text, language };
        let (generation, source, tx) = (s.generation, s.current.clone(), s.tx.clone());
        self.ask_plugin(source, messages::op::BROWSE, messages::write(&ask), move |answer| {
            let result = answer.and_then(|bytes| messages::read::<Listing>(&bytes));
            let _ = tx.send(Answer::Listing { generation, cursor, result });
        });
    }

    /// Calls a plugin on a thread of its own and hands the answer to `then`.
    fn ask_plugin(
        &mut self,
        source: String,
        operation: &'static str,
        input: Vec<u8>,
        then: impl FnOnce(Result<Vec<u8>, String>) + Send + 'static,
    ) {
        let Some(plugins) = self.plugins.clone() else {
            then(Err(chilpa_i18n::t("the plugin sandbox is not started")));
            return;
        };
        let spawned = std::thread::Builder::new().name("chilpa-sources".into()).spawn(move || {
            let answer = match plugins.lock() {
                Ok(mut p) => p.call_waiting(&source, operation, &input),
                Err(_) => Err(chilpa_i18n::t("the plugin sandbox is not started")),
            };
            then(answer);
        });
        if spawned.is_err() {
            self.sources.loading = false;
            self.sources.error = chilpa_i18n::t("the plugin could not be asked");
        }
    }

    /// Plays an item of the listing: asks the plugin where it sounds from,
    /// and starts it when the answer arrives.
    fn play_entry(&mut self, id: &str) {
        let Some(entry) = self.sources.entries.iter().find(|e| e.id == id).cloned() else { return };
        if entry.kind != EntryKind::Item {
            return;
        }
        if !entry.playable {
            self.now.error = if entry.why_not.is_empty() {
                chilpa_i18n::t("this cannot be played")
            } else {
                entry.why_not.clone()
            };
            return;
        }
        let source = self.sources.current.clone();
        if let Some(address) = entry.id.strip_prefix(ADDRESS) {
            self.sound(address.to_string(), source, entry.clone());
            return;
        }
        let name = self.source_name(&source);
        self.sources.resolving = entry.id.clone();
        let tx = self.sources.tx.clone();
        let ask = messages::write(&Resolve::new(&entry.id));
        self.ask_plugin(source.clone(), messages::op::RESOLVER, ask, move |answer| {
            let result = answer.and_then(|bytes| messages::read::<Stream>(&bytes));
            let _ = tx.send(Answer::Stream { source, name, entry, result });
        });
    }

    fn toggle_saved(&mut self, id: &str) {
        let Some(entry) = self.sources.entries.iter().find(|e| e.id == id).cloned() else { return };
        let source = self.sources.current.clone();
        let was = self.sources.saved.contains(id);
        let kept = self.library().is_some_and(|lib| {
            if was {
                lib.forget_entry(&source, id).is_ok()
            } else {
                lib.save_entry(
                    &source,
                    &SavedEntry {
                        external_id: entry.id.clone(),
                        title: entry.title.clone(),
                        subtitle: entry.subtitle.clone(),
                        image_url: entry.image_url.clone().unwrap_or_default(),
                        kind: kind_name(entry.kind).to_string(),
                        live: entry.live,
                    },
                )
                .is_ok()
            }
        });
        if !kept {
            self.now.error = chilpa_i18n::t("it could not be saved");
            return;
        }
        if was {
            self.sources.saved.remove(id);
        } else {
            self.sources.saved.insert(id.to_string());
        }
    }

    /// Takes in what the plugin threads answered. Called from `poll`.
    pub(crate) fn advance_sources(&mut self) {
        while let Ok(answer) = self.sources.rx.try_recv() {
            match answer {
                Answer::Listing { generation, cursor, result } => {
                    if generation == self.sources.generation {
                        self.take_listing(cursor, result);
                    }
                }
                Answer::Stream { source, name, entry, result } => {
                    if self.sources.resolving == entry.id {
                        self.sources.resolving.clear();
                    }
                    match result.and_then(|stream| playable_url(&stream)) {
                        Ok(url) => self.sound(url, source, entry),
                        Err(e) => self.now.error = format!("{name}: {e}"),
                    }
                }
                Answer::Picture { url, song, path } => self.take_picture(url, song, path),
            }
        }
    }

    /// Starts an entry from the address it sounds from, and goes for its
    /// picture meanwhile.
    fn sound(&mut self, url: String, source: String, entry: Entry) {
        let picture = entry.image_url.clone();
        self.sources.sounding = Some(Sounding { url: url.clone(), source, entry, picture: String::new() });
        let (device, bit_perfect) = self.chosen_output();
        self.play_url(&url, &device, bit_perfect);
        if let Some(address) = picture {
            let tx = self.sources.tx.clone();
            crate::pictures::keep_later(address, move |path| {
                let _ = tx.send(Answer::Picture { url, song: String::new(), path });
            });
        }
    }

    /// Puts a picture that arrived on what is playing, if it is still that.
    fn take_picture(&mut self, url: String, song: String, path: String) {
        let Some(sounding) = self.sources.sounding.as_mut().filter(|s| s.url == url) else { return };
        if song.is_empty() {
            sounding.picture = path.clone();
        }
        // A song's cover that arrives after the song is over is not shown.
        let current = song.is_empty() && self.now.artwork_path.is_empty() || song == self.now.title;
        if self.now.path == url && current {
            self.now.artwork_path = path;
        }
    }

    /// A station said what is on. Its own picture comes back, and the
    /// song's cover is looked for when the title reads "artist - song".
    pub(crate) fn song_changed(&mut self, title: &str) {
        let Some(sounding) = self.sources.sounding.as_ref().filter(|s| s.url == self.now.path) else {
            return;
        };
        self.now.artwork_path = sounding.picture.clone();
        let Some((artist, song)) = title.split_once(" - ") else { return };
        let (artist, song, title) = (artist.trim().to_string(), song.trim().to_string(), title.to_string());
        if artist.is_empty() || song.is_empty() {
            return;
        }
        let (url, tx) = (sounding.url.clone(), self.sources.tx.clone());
        let _ = std::thread::Builder::new().name("chilpa-song-cover".into()).spawn(move || {
            let Some(address) = crate::scrobble::lastfm::track_cover(&artist, &song) else { return };
            if let Some(path) = crate::pictures::keep(&address) {
                let _ = tx.send(Answer::Picture { url, song: title, path });
            }
        });
    }

    fn take_listing(&mut self, cursor: Option<String>, result: Result<Listing, String>) {
        self.sources.loading = false;
        let listing = match result {
            Ok(listing) => listing.clean(cursor.as_deref()),
            Err(e) => {
                self.sources.error = e;
                return;
            }
        };
        let at_root = self.sources.trail.is_empty();
        let mut entries = listing.entries;
        if at_root {
            // The place to type is drawn as a field, not as a row.
            self.sources.search_id = entries
                .iter()
                .find(|e| e.kind == EntryKind::Search)
                .map(|e| e.id.clone())
                .unwrap_or_default();
            if cursor.is_none() {
                self.sources.entries = vec![Entry {
                    id: SAVED.to_string(),
                    title: chilpa_i18n::t("Saved"),
                    playable: true,
                    ..Entry::default()
                }];
            }
        }
        entries.retain(|e| e.kind != EntryKind::Search && e.id != SAVED);
        self.sources.entries.extend(entries);
        self.sources.next = listing.next;
        if self.sources.title.is_empty() {
            self.sources.title = listing.title;
        }
    }

    /// Puts the card of a browsed entry on what just started, if it is that
    /// entry. Answers whether what started is live.
    pub(crate) fn name_from_entry(&mut self, path: &std::path::Path) -> bool {
        let path = path.display().to_string();
        let Some(sounding) = self.sources.sounding.as_ref().filter(|s| s.url == path) else {
            return false;
        };
        self.now.title = sounding.entry.title.clone();
        self.now.artist = sounding.entry.subtitle.clone();
        self.now.album = String::new();
        sounding.entry.live
    }

    /// The name of the station that is on, when what plays is a live entry
    /// of a source.
    pub(crate) fn station_on(&self) -> Option<String> {
        let sounding = self.sources.sounding.as_ref()?;
        (self.now.live && self.now.path == sounding.url).then(|| sounding.entry.title.clone())
    }
}
