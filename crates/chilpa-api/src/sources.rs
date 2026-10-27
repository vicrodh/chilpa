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

mod collection;
mod ahead;
mod navigation;
mod parents;
mod view;
mod extras;
#[cfg(test)]
mod progress_tests;

use crate::source_opener::{entry_of, of_plugin};
use crate::Chilpa;
use chilpa_library::store::progress::Heard;
use chilpa_library::store::saved::SavedEntry;
use chilpa_plugin::messages::{self, Browse, Column, Entry, EntryKind, Resolve, Stream};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use chilpa_plugin::presentation::{Presentation, SourceDocument};

/// The id of the one folder that is Chilpa's and not the plugin's: what the
/// listener saved from that source. A plugin never sees it.
const SAVED: &str = "chilpa:saved";

/// Chilpa's other folder: what the listener left halfway from that source.
const UNFINISHED: &str = "chilpa:unfinished";

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
    pub icon_image: String,
    pub icon_tint: bool,
    /// Drawn by the catalog as records, not as its own tree.
    pub music: bool,
    /// It has a page for an artist: the artist page offers it as a source.
    pub artists: bool,
    pub children: Vec<PanelChild>,
}

/// An entry under a plugin's. `route` is the source screen's key when the
/// catalog draws it; empty, the plugin's tree opens at `id`.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct PanelChild { pub id: String, pub title: String, pub icon: String, pub route: String }

/// A step of the way from the root to where the window stands.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Crumb {
    id: String,
    title: String,
    /// The entry that was opened to get here, when it was one of the
    /// plugin's: it is what saving this place keeps.
    entry: Option<Entry>,
    presentation: Option<Presentation>,
}

/// How often the place in an episode is written down while it plays.
const KEEP_EVERY: Duration = Duration::from_secs(5);

/// How close to its end something counts as heard, in seconds.
const NEAR_THE_END: f64 = 30.0;

/// Something of a source that is sounding, with the card it was listed with.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
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
    Extra { generation: u64, id: String, kind: String, result: Result<serde_json::Value, String> },
    Native { source: String, serial: u64, request: serde_json::Value, result: Result<serde_json::Value, String> },
    Cover { generation: u64, path: String },
    Collection { serial: u64, result: Result<SourceDocument, String> },
    Library { generation: u64, result: Result<SourceDocument, String> },
    Section { generation: u64, at: String, title: String, result: Result<SourceDocument, String> },
    Imported { source: String, at: String, result: Result<usize, String> },
    Metadata { generation: u64, id: String, result: Result<serde_json::Value, String> },
    Listing { generation: u64, cursor: Option<String>, result: Result<SourceDocument, String> },
    Stream { source: String, name: String, entry: Entry, result: Result<Stream, String> },
    /// A picture for what sounds from `url`, on disk. `song` is the title
    /// it is the cover of, or empty when it is the station's own.
    Picture { url: String, song: String, path: String },
}

/// Where the window stands in a source, and what it sees there.
pub(crate) struct SourcesState {
    pub(crate) native_actions: BTreeMap<String, serde_json::Value>,
    pub(crate) playlist_refresh_pending: bool,
    native_action_serial: u64,
    collection: collection::Collection,
    panels: Vec<Panel>,
    pub(crate) layouts: BTreeMap<String, crate::source_layout::Layout>,
    /// The installed plugins as last read while the sandbox was free.
    pub(crate) listed: std::sync::Mutex<Vec<crate::plugins::PluginInfo>>,
    /// The plugin being browsed. Empty before any was opened.
    current: String,
    trail: Vec<Crumb>,
    /// The root's place to type, if the plugin offers one.
    search_id: String,
    text: String,
    /// What narrows the listing on screen without asking anybody.
    filter: String,
    /// How a listing of things with an end is ordered: as the plugin gave
    /// it when empty, or "oldest", or "unheard".
    order: String,
    /// The columns the source's root is narrowed by, kept from its first
    /// listing while the listener picks.
    columns: Vec<Column>,
    /// What is picked in them, by column id. An empty value is "all", which
    /// is a pick too: it is how a column that starts on something is left.
    picks: BTreeMap<String, String>,
    title: String,
    entries: Vec<Entry>,
    presentation: BTreeMap<String, Presentation>,
    medium: String,
    media_kinds: BTreeMap<String,String>,
    media_tab: String,
    attribution: String,
    music: bool,
    sections: Vec<serde_json::Value>,
    section_entries: HashMap<String, Entry>,
    sections_pending: usize,
    preview_cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    metadata_loading: bool,
    importing: bool,
    imported: Option<usize>,
    play_after_load: bool,
    roots: BTreeMap<String, Vec<Entry>>,
    next: Option<String>,
    loading: bool,
    error: String,
    /// The ids saved from the current source.
    saved: HashSet<String>,
    saved_cards: Vec<Entry>,
    extras: serde_json::Value,
    on_air_history: Vec<serde_json::Value>,
    /// What the listener started or finished from the current source.
    heard: HashMap<String, Heard>,
    /// The entry a page of its own is showing, and whose it is.
    detail: Option<(String, Entry)>,
    /// When the place in what plays was last written down.
    kept_at: Option<Instant>,
    progress_writer: Option<chilpa_library::store::progress_writer::ProgressWriter>,
    /// True once the last run's station was looked for.
    looked_back: bool,
    /// The servers' opener, for the one opener the player holds.
    pub(crate) servers_opener: Option<std::sync::Arc<dyn chilpa_core::remote::RemoteOpener>>,
    /// The opener the player holds, to resolve the next track ahead.
    openers: Option<std::sync::Arc<crate::source_opener::Openers>>,
    /// The queue entry already resolved ahead, so it is asked once.
    ahead_asked: String,
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
            native_actions: BTreeMap::new(),
            playlist_refresh_pending: false,
            native_action_serial: 0,
            collection: collection::Collection::default(),
            panels: Vec::new(),
            layouts: BTreeMap::new(),
            listed: std::sync::Mutex::default(),
            current: String::new(),
            trail: Vec::new(),
            search_id: String::new(),
            text: String::new(),
            filter: String::new(),
            order: String::new(),
            columns: Vec::new(),
            picks: BTreeMap::new(),
            title: String::new(),
            entries: Vec::new(),
            presentation: BTreeMap::new(),
            medium: String::new(),
            media_kinds: BTreeMap::new(),
            media_tab: "home".into(),
            attribution: String::new(),
            music: false,
            sections: Vec::new(),
            section_entries: HashMap::new(),
            sections_pending: 0,
            preview_cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            metadata_loading: false,
            importing: false,
            imported: None,
            play_after_load: false,
            roots: BTreeMap::new(),
            next: None,
            loading: false,
            error: String::new(),
            saved: HashSet::new(),
            saved_cards: Vec::new(),
            extras: serde_json::Value::Null,
            on_air_history: Vec::new(),
            heard: HashMap::new(),
            detail: None,
            kept_at: None,
            progress_writer: None,
            looked_back: false,
            servers_opener: None,
            openers: None,
            ahead_asked: String::new(),
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

/// Something with an end the listener started, as a row to pick it up from.
fn from_heard(heard: Heard) -> Entry {
    Entry {
        id: heard.external_id,
        title: heard.title,
        subtitle: heard.subtitle,
        image_url: Some(heard.image_url).filter(|u| !u.is_empty()),
        kind: EntryKind::Item,
        duration_ms: heard.duration_ms,
        playable: true,
        ..Entry::default()
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
pub(crate) fn playable_url(stream: &Stream) -> Result<String, String> {
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
            self.sources.layouts = plugins.layouts.clone();
            // A process plugin's pages come from what it can do.
            for id in self.natives.ids() {
                if let Some(layout) = self.natives.layout(&id) {
                    self.sources.layouts.insert(id, layout);
                }
            }
            *self.sources.listed.lock().unwrap_or_else(|e| e.into_inner()) = plugins.list();
            let found: Vec<(String, String, String, bool)> = plugins.panels().into_iter()
                .map(|(id, name, icon)| { let music = plugins.worker_domains(&id).is_some(); (id, name, icon, music) }).collect();
            drop(plugins);
            self.sources.panels = found.into_iter().map(|(id, name, icon, worker)| {
                    let icon_image = crate::plugin_icons::image_url(&Self::plugins_dir(), &id);
                    let icon = if icon.is_empty() { crate::plugin_icons::letter(&name) } else { icon };
                    let layout = self.sources.layouts.get(&id).filter(|layout| !layout.sections.is_empty());
                    let music = worker || layout.is_some();
                    self.catalog.remember_drawn(&id, music);
                    self.catalog.remember_name(&id, &name);
                    let children = if let Some(children) = self.sources.media_kinds.get(&id).and_then(|medium| navigation::media_children(medium)) { children } else if let Some(layout) = layout {
                        // Discover is the source's own page; the rest are its sections.
                        layout.ordered().into_iter().filter(|(_, section)| section.under.is_empty()).map(|(key, section)| PanelChild { id: format!("view:{key}"), title: section.title.clone(), icon: section.icon.clone(),
                            route: if key == "discover" { id.clone() } else { format!("{id} section:{key}") } }).collect()
                    } else if music {
                        let root = self.catalog.shelf(&format!("{id} root")).unwrap_or_default();
                        root.folders.iter().take(64).map(|f| PanelChild { id: f.id.clone(), title: f.title.clone(), icon: String::new(), route: format!("{id} at:{}", f.id) }).collect()
                    } else { self.sources.roots.get(&id).into_iter().flatten().map(|entry| PanelChild { id: entry.id.clone(), title: entry.title.clone(), icon: String::new(), route: String::new() }).collect() };
                    let artists = music && self.natives.has_pages(&id);
                    Panel { icon_tint: crate::plugin_icons::tint(&Self::plugins_dir(), &id), id, name, icon, icon_image, music, artists, children }
                }).collect();
        }
        let order = crate::plugin_sidebar::ordered(&Self::plugins_dir(),self.sources.panels.iter().map(|p|p.id.clone()).collect());
        self.sources.panels.sort_by_key(|p|order.iter().position(|id|id==&p.id).unwrap_or(usize::MAX));
        self.sources.panels.clone()
    }

    pub(crate) fn invalidate_source_menus(&mut self) {
        let state = &mut self.sources;
        state.generation += 1;
        state.roots.clear();
        state.panels.clear();
        state.entries.clear();
        state.presentation.clear();
        state.medium.clear();
        state.attribution.clear();
        state.trail.clear();
        state.current.clear();
        state.detail = None;
        state.metadata_loading = false;
        state.loading = false;
    }

    /// What the entry on a page of its own is called, if that is the one
    /// the page is showing.
    pub(crate) fn source_entry_title(&self, plugin: &str, id: &str) -> Option<String> {
        let (source, entry) = self.sources.detail.as_ref()?;
        (!id.is_empty() && source == plugin && entry.id == id).then(|| entry.title.clone())
    }

    /// What a source is called, for its tab and its header.
    pub fn source_name(&self, id: &str) -> String {
        self.sources.panels.iter().find(|p| p.id == id).map(|p| p.name.clone())
            .or_else(|| self.catalog.name(id)).unwrap_or_default()
    }

    /// Everything the source screen and the sidebar draw, as one JSON
    /// object. The window reads it again whenever it changed.
    pub fn sources_json(&mut self) -> String {
        self.advance_plugins();
        let panels = self.source_panels();
        let download_plugins: Vec<serde_json::Value> = self.plugins.as_ref().and_then(|plugins| plugins.try_lock().ok())
            .map(|plugins| plugins.list().into_iter().filter(|plugin| plugins.enabled(&plugin.id) && self.natives.has_downloads(&plugin.id))
                .map(|plugin| serde_json::json!({"id":plugin.id,"name":plugin.name,
                    "icon":crate::plugin_icons::image_url(&Self::plugins_dir(), &plugin.id),
                    "access":crate::plugin_access::read(&Self::plugins_dir(), &plugin.id)})).collect()).unwrap_or_default();
        let playing_source = self.playing_source_name();
        let s = &self.sources;
        let on_air = s.sounding.as_ref().filter(|p| self.now.playing && self.now.path == p.url);
        let playing = of_plugin(&self.now.path).filter(|_| self.now.playing);
        let sounds = |source: &str, id: &str| {
            on_air.is_some_and(|p| p.source == source && p.entry.id == id)
                || playing.as_ref().is_some_and(|(plugin, entry)| plugin == source && entry == id)
        };
        let needle = s.filter.to_lowercase();
        let mut shown: Vec<&Entry> = s
            .entries
            .iter()
            .filter(|e| {
                needle.is_empty()
                    || e.title.to_lowercase().contains(&needle)
                    || e.subtitle.to_lowercase().contains(&needle)
            })
            .collect();
        // Only a listing of things with an end has an order of its own to
        // change: episodes come newest first, and can be turned around or
        // sorted so what was not heard comes up.
        let sortable = s.entries.iter().any(|e| e.kind == EntryKind::Item && !e.live);
        if sortable {
            match s.order.as_str() {
                "oldest" => shown.sort_by_key(|e| e.published_at.unwrap_or(i64::MAX)),
                "unheard" => shown.sort_by_key(|e| s.heard.get(&e.id).is_some_and(|h| h.played)),
                _ => {}
            }
        }
        // At a root with columns, Chilpa's own folders go beside the field
        // as shortcuts: in the list they would sit above the stations.
        let explorer = s.trail.is_empty() && !s.columns.is_empty();
        let own = |e: &&Entry| e.id == SAVED || e.id == UNFINISHED;
        let shortcuts: Vec<serde_json::Value> = s
            .entries
            .iter()
            .filter(|e| explorer && own(e))
            .map(|e| serde_json::json!({ "id": e.id, "title": e.title }))
            .collect();
        let entries: Vec<serde_json::Value> = shown
            .into_iter()
            .filter(|e| !(explorer && own(e)))
            .map(|e| self.entry_json(&s.current, e, sounds(&s.current, &e.id)))
            .collect();
        let columns: Vec<serde_json::Value> = s
            .columns
            .iter()
            .map(|c| {
                let picked = s.picks.get(&c.id).cloned().unwrap_or_default();
                let named = c.entries.iter().find(|e| e.id == picked).map(|e| e.title.clone()).unwrap_or_default();
                let entries: Vec<serde_json::Value> = c
                    .entries
                    .iter()
                    .map(|e| serde_json::json!({ "id": e.id, "title": e.title, "subtitle": e.subtitle }))
                    .collect();
                serde_json::json!({ "id": c.id, "title": c.title, "picked": picked, "pickedTitle": named, "entries": entries })
            })
            .collect();
        let crumbs: Vec<&str> = s.trail.iter().map(|c| c.title.as_str()).collect();
        let place = s.trail.last();
        // The entry the window is standing in, as its own page would show
        // it: a show above its episodes.
        let about = place.and_then(|c| c.entry.as_ref()).map(|e| {
            serde_json::json!({
                "title": e.title,
                "subtitle": e.subtitle,
                "image": e.image_url.clone().unwrap_or_default(),
                "description": e.description,
                "facts": e.facts,
                "presentation": place.and_then(|crumb| crumb.presentation.as_ref()),
            })
        });
        let detail = s.detail.as_ref().map(|(source, entry)| {
            let mut card = self.entry_json(source, entry, sounds(source, &entry.id));
            card["source"] = source.clone().into();
            card["name"] = self.source_name(source).into();
            card["device_sync"] = self.device_episode(source, &entry.id).is_some().into();
            card["parent"] = self.episode_parent(source, &entry.id).map(|p| serde_json::json!({"id":p.id,"title":p.title,"source":source})).unwrap_or_default();
            card["description"] = entry.description.clone().into();
            card["metadata_loading"] = s.metadata_loading.into();
            card["facts"] = serde_json::json!(entry.facts);
            card["extras"] = if s.extras["id"] == entry.id { s.extras.clone() } else { serde_json::Value::Null };
            // On a station the title is the song it is playing right now.
            let song = if entry.live && sounds(source, &entry.id) && self.now.title != entry.title {
                self.now.title.clone()
            } else {
                String::new()
            };
            card["song"] = song.into();
            card["art"] = if sounds(source, &entry.id) { self.now.artwork_path.clone() } else { String::new() }.into();
            card
        });
        let playing_entry = s.sounding.as_ref().filter(|sound| sound.url == self.now.path).map(|sound| (sound.source.clone(), sound.entry.clone()))
            .or_else(|| of_plugin(&self.now.path).and_then(|(source, id)| self.entry_by_id(&id).filter(|(owner, _)| *owner == source)));
        let context = playing_entry.map(|(source, entry)| {
            let mut value = self.entry_json(&source, &entry, self.now.playing);
            value["description"] = entry.description.into();
            value["song"] = self.now.title.clone().into();
            value["extras"] = if s.extras["id"] == entry.id { s.extras.clone() } else { serde_json::Value::Null };
            value
        });
        let mut document = serde_json::json!({
            "collection": self.collection_json(),
            "releases": s.trail.last().is_some_and(|crumb| crumb.id == "view:library"),
            "home": s.trail.is_empty() && s.music,
            "sections": s.sections,
            "sections_loading": s.sections_pending > 0,
            "starting": self.plugins_starting(),
            "startup": self.startup_progress().map(|(done, total, name)| serde_json::json!({"done": done, "total": total, "name": name})),
            "panels": panels,
            "source": s.current,
            "importing": s.importing,
            "imported": s.imported,
            "music": s.music,
            "medium": s.medium,
            "attribution": s.attribution,
            "navigation_id": if s.medium.is_empty() { s.trail.last().map(|crumb| crumb.id.clone()).unwrap_or_default() } else { format!("media:{}",s.media_tab) },
            "media_tab": s.media_tab,
            "name": self.source_name(&s.current),
            "title": s.title,
            "crumbs": crumbs,
            "columns": columns,
            "explorer": explorer,
            "shortcuts": shortcuts,
            "saved_entries": s.saved_cards.iter().map(|entry| self.entry_json(&s.current, entry, sounds(&s.current, &entry.id))).collect::<Vec<_>>(),
            "continue_entries": s.heard.values().filter(|entry| !entry.played && entry.position_ms > 0).cloned().map(from_heard).map(|entry| self.entry_json(&s.current, &entry, sounds(&s.current, &entry.id))).collect::<Vec<_>>(),
            "entries": entries,
            "searchable": !s.search_id.is_empty(),
            "native_actions": s.native_actions,
            "text": s.text,
            "filter": s.filter,
            "total": s.entries.len(),
            "loading": s.loading,
            "more": s.next.is_some(),
            "error": s.error,
            "inSaved": place.is_some_and(|c| c.id == SAVED),
            "about": about,
            "sortable": sortable,
            "order": s.order,
            // A place that is an entry of the plugin can be saved whole: a
            // show is followed by saving its folder.
            "placeSavable": place.is_some_and(|c| c.entry.is_some()),
            "placeSaved": place.is_some_and(|c| s.saved.contains(&c.id)),
            // What the bar shows, when it is something of a source: the key
            // of its page. It holds while paused and after it ends, for as
            // long as the bar still names it.
            "onAir": s
                .sounding
                .as_ref()
                .filter(|p| self.now.path == p.url)
                .map(|p| format!("{} {}", p.source, p.entry.id))
                .or_else(|| of_plugin(&self.now.path).map(|(plugin, entry)| format!("{plugin} {entry}")))
                .unwrap_or_default(),
            "detail": detail,
        });
        document["download_plugins"] = serde_json::json!(download_plugins);
        document["playing_parent"] = self.playing_media_parent().map(|(source, _)| source).unwrap_or_default().into();
        document["playing_source"] = playing_source.into();
        document["playing_entry"] = context.into();
        document["on_air_history"] = serde_json::json!(s.on_air_history);
        document.to_string()
    }

    fn saved_elsewhere(&self, source: &str, id: &str) -> bool {
        self.library
            .as_ref()
            .and_then(|lib| lib.saved_entries(source).ok())
            .is_some_and(|saved| saved.iter().any(|e| e.external_id == id))
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
    /// - `pick`: in the root's column `id`, pick the entry `text`; empty is
    ///   all of them. The window goes back to the root.
    /// - `order`: order a listing of episodes by `id`: empty as it came,
    ///   `oldest`, or `unheard`.
    /// - `more`: bring the next page of the listing.
    /// - `play`: play the item `id`.
    /// - `queue`: add the item `id` to the end of the queue.
    /// - `save`: save the entry `id`, or take it out if it was saved. With
    ///   no id it is the place the window stands in that is saved.
    /// - `heard`: mark the item `id` as heard, or as not heard.
    /// - `detail`: show the entry `id` of the plugin `text` on its page.
    /// - `retry`: ask again for the listing that failed.
    pub fn sources_command(&mut self, action: &str, id: &str, text: &str) {
        match action {
            "media_extra" => self.load_media_extra(id, text),
            "play_from" => if let Ok(seconds) = text.parse::<f64>() {
                if seconds.is_finite() && (0.0..=864_000.0).contains(&seconds) { self.play_entry_at(id, Some(Duration::from_secs_f64(seconds))); }
            },
            "track_library" => if let Ok(enabled) = text.parse::<bool>() { self.show_track_library(id, enabled); },
            "album_library" => if let Ok(enabled) = text.parse::<bool>() { self.show_album_library(id, enabled); },
            "section_more" => self.append_native_section(id),
            "native" => {
                let Ok(input) = serde_json::from_str::<serde_json::Value>(text) else { return };
                if !self.natives.has_pages(id) && !(input["action"].as_str().is_some_and(|a| a.starts_with("playlist.")) && self.natives.edits_playlists(id)) { return; }
                if self.sources.native_actions.get(id).is_some_and(|s| s["pending"] == true) { return; }
                let request = input["request_id"].clone();
                self.sources.native_action_serial = self.sources.native_action_serial.wrapping_add(1);
                let serial = self.sources.native_action_serial;
                // A heart changes one thing: its answer marks it, it reads no page again.
                let heart = (input["action"] == "favorite").then(|| serde_json::json!({"ref":input["ref"],"on":input["on"] == true}));
                self.sources.native_actions.insert(id.into(), serde_json::json!({"request_id":request,"pending":true,"serial":serial,"favorite":heart}));
                let (natives, source, tx) = (self.natives.clone(), id.to_string(), self.sources.tx.clone());
                std::thread::spawn(move || {
                    let result = crate::playlist_sources::execute(&natives, &source, input);
                    let _ = tx.send(Answer::Native { source, serial, request, result });
                });
            },
            "home" => {
                if self.sources.media_kinds.contains_key(id) { self.open_media_section(id,"home"); return; }
                let already_open = self.sources.current == id;
                self.sources.media_tab = "home".into();
                self.open_source(id);
                self.sources.play_after_load = false;
                if already_open { self.sources.trail.clear(); self.sources.text.clear(); self.load_listing(); }
            },
            "open" => { self.sources.play_after_load = false; self.open_source(id); },
            "child" => {
                self.sources.play_after_load = false;
                if let Some(section) = text.strip_prefix("media:") { self.open_media_section(id,section); }
                else if let Some(section) = text.strip_prefix("view:") { self.open_source_layout(id, section); }
                else { self.open_source_child(id, text); }
            },
            "enter" => { self.sources.play_after_load = false; self.enter_entry(id); },
            "crumb" => {
                self.sources.play_after_load = false;
                self.sources.trail.truncate(id.parse().unwrap_or(0));
                self.sources.text.clear();
                self.load_listing();
            }
            "search" => { self.sources.media_tab = "search".into(); self.sources.play_after_load = false; self.search_source(text); },
            "filter" => self.sources.filter = text.trim().to_string(),
            "pick" => { self.sources.play_after_load = false; self.pick(id, text); },
            "order" => self.sources.order = id.to_string(),
            "more" => self.load_more(),
            "play" => self.play_entry(id),
            "play_collection" | "queue_collection" | "next_collection" | "play_preview_track" | "preview" | "close_preview" => self.collection_command(action, id),
            "queue" => self.queue_entry(id),
            "import" => self.import_source_collection(),
            "save" => self.toggle_saved(id),
            "heard" => self.toggle_heard(id),
            "detail" => self.show_detail(text, id),
            "parent" => self.open_episode_parent(),
            "playing_parent" => { if let Some((source, parent)) = self.playing_media_parent() { self.open_media_parent(source, parent); } },
            "retry" => self.load_listing(),
            "retry_listing" => self.source_retry(id),
            _ => {}
        }
    }

    fn import_source_collection(&mut self) {
        if self.sources.importing { return; }
        let Some(at) = self.sources.trail.last().map(|crumb| crumb.id.clone()) else { return };
        let Some(plugins) = self.plugins.clone() else { return };
        let source = self.sources.current.clone();
        let name = self.source_name(&source);
        let tx = self.sources.tx.clone();
        self.sources.importing = true;
        self.sources.imported = None;
        std::thread::spawn(move || {
            let result = crate::plugin_library::import(plugins, &source, &name, &at);
            let _ = tx.send(Answer::Imported { source, at, result });
        });
    }

    fn open_source(&mut self, id: &str) {
        // Only a plugin with a place in the window is opened: anything else
        // would throw away where the window stands for nothing.
        if !self.source_panels().iter().any(|p| p.id == id) {
            return;
        }
        let s = &mut self.sources;
        if s.current == id && (s.loading || !s.entries.is_empty()) {
            // Coming back to it leaves it where it was.
            return;
        }
        s.music = false;
        s.medium = s.media_kinds.get(id).cloned().unwrap_or_default();
        s.media_tab = "home".into();
        s.attribution.clear();
        s.current = id.to_string();
        s.trail.clear();
        s.text.clear();
        s.search_id.clear();
        s.columns.clear();
        s.picks.clear();
        self.read_saved();
        self.load_listing();
    }

    /// The sidebar passes only a source and an opaque child identity from
    /// its snapshot. A child is resolved against the host's current root.
    fn open_source_layout(&mut self, source: &str, section: &str) {
        if section == "discover" { self.sources_command("home", source, ""); return; }
        let Some(layout) = self.sources.layouts.get(source).and_then(|layout| layout.sections.get(section)).cloned() else { return };
        let Some(plugins) = self.plugins.clone() else { return };
        self.sources.preview_cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        self.sources.preview_cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let cancelled = self.sources.preview_cancel.clone();
        self.sources.sections_pending = 0;
        self.sources.sections.clear();
        self.sources.section_entries.clear();
        self.sources.filter.clear();
        self.sources.title = layout.title.clone();
        self.sources.current = source.into();
        self.sources.generation += 1;
        self.sources.loading = true;
        self.sources.music = true;
        self.sources.error.clear();
        self.sources.entries.clear();
        self.sources.presentation.clear();
        self.sources.text.clear();
        self.sources.next = None;
        self.sources.trail = vec![Crumb { id: format!("view:{section}"), title: layout.title.clone(), entry: None, presentation: None }];
        let (source, generation, tx) = (source.to_string(), self.sources.generation, self.sources.tx.clone());
        std::thread::spawn(move || {
            let result = crate::source_layout::browse(&plugins, &source, &layout, &cancelled);
            let _ = tx.send(Answer::Library { generation, result });
        });
    }

    fn open_source_child(&mut self, source: &str, id: &str) {
        if !self.source_panels().iter().any(|panel| panel.id == source) { return; }
        let Some(entry) = self.sources.roots.get(source).and_then(|entries| entries.iter().find(|entry| entry.id == id)).cloned() else { return };
        self.sources.current = source.into();
        self.sources.text.clear();
        self.sources.columns.clear();
        self.sources.picks.clear();
        self.sources.trail = vec![Crumb { id: entry.id.clone(), title: entry.title.clone(), presentation: self.sources.presentation.get(&entry.id).cloned(), entry: Some(entry) }];
        self.read_saved();
        self.load_listing();
    }

    /// The entry with that id, and whose it is: one of the listing on
    /// screen, the place the window stands in, or the one on its own page.
    fn entry_by_id(&self, id: &str) -> Option<(String, Entry)> {
        let s = &self.sources;
        if let Some(entry) = s.section_entries.get(id) { return Some((s.current.clone(), entry.clone())); }
        if let Some(entry) = s.entries.iter().find(|e| e.id == id) {
            return Some((s.current.clone(), entry.clone()));
        }
        if let Some(entry) = s.trail.iter().filter_map(|c| c.entry.as_ref()).find(|e| e.id == id) {
            return Some((s.current.clone(), entry.clone()));
        }
        if let Some(entry) = s.collection.document.listing.entries.iter().find(|e| e.id == id) {
            return Some((s.collection.source.clone(), entry.clone()));
        }
        if let Some(entry) = s.saved_cards.iter().find(|entry| entry.id == id) { return Some((s.current.clone(), entry.clone())); }
        if let Some(entry) = s.heard.get(id) { return Some((s.current.clone(), from_heard(entry.clone()))); }
        s.detail.as_ref().filter(|(_, e)| e.id == id).cloned()
    }

    /// Finds an entry for its page wherever Chilpa still knows it: on
    /// screen, on the air, saved, or heard before.
    fn show_detail(&mut self, source: &str, id: &str) {
        let known = self
            .entry_by_id(id)
            .filter(|(whose, _)| whose == source)
            .map(|(_, e)| e)
            .or_else(|| self.sources.sounding.as_ref().filter(|p| p.source == source && p.entry.id == id).map(|p| p.entry.clone()))
            .or_else(|| {
                let saved = self.library()?.saved_entries(source).ok()?;
                saved.into_iter().find(|e| e.external_id == id).map(from_saved)
            })
            .or_else(|| {
                self.library()?.heard(source, id).ok()?.map(from_heard)
            });
        self.source_panels();
        self.sources.extras = serde_json::Value::Null;
        self.sources.detail = known.map(|entry| (source.to_string(), entry));
        self.sources.metadata_loading = false;
        if self.natives.has_pages(source) && source == self.sources.current && self.sources.detail.as_ref().is_some_and(|(_, entry)| entry.playable && !entry.live) {
            self.sources.metadata_loading = true;
            let (generation, id, tx) = (self.sources.generation, id.to_string(), self.sources.tx.clone());
            let input = serde_json::to_vec(&serde_json::json!({"id":id})).unwrap_or_default();
            self.ask_plugin(source.to_string(), "source.metadata", input, move |answer| {
                let result = answer.and_then(|bytes| messages::read::<serde_json::Value>(&bytes));
                let _ = tx.send(Answer::Metadata { generation, id, result });
            });
        }
    }

    fn enter_entry(&mut self, id: &str) {
        let Some((_, entry)) = self.entry_by_id(id) else { return };
        match entry.kind {
            EntryKind::Folder => {
                let kept = (entry.id != SAVED && entry.id != UNFINISHED).then(|| entry.clone());
                self.sources.trail.push(Crumb { presentation: self.sources.presentation.get(&entry.id).cloned(), id: entry.id, title: entry.title, entry: kept });
                self.load_listing();
                if let Some(address) = self.sources.trail.last().and_then(|c| c.entry.as_ref()).and_then(|e| e.image_url.clone()).filter(|url| url.starts_with("https://") || url.starts_with("http://")) {
                    let (generation, tx) = (self.sources.generation, self.sources.tx.clone());
                    crate::pictures::keep_later(address, move |path| { let _ = tx.send(Answer::Cover { generation, path }); });
                }
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
            s.trail.push(Crumb { id: s.search_id.clone(), title: format!("“{text}”"), entry: None, presentation: None });
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
        s.trail = vec![Crumb { id: entry.id.clone(), title: s.title.clone(), entry: None, presentation: None }];
        s.entries = vec![entry.clone()];
        self.play_entry(&entry.id);
    }

    fn pick(&mut self, column: &str, value: &str) {
        let Some(offered) = self.sources.columns.iter().find(|c| c.id == column) else { return };
        if !value.is_empty() && !offered.entries.iter().any(|e| e.id == value) {
            return;
        }
        let s = &mut self.sources;
        s.picks.insert(column.to_string(), value.to_string());
        s.trail.clear();
        s.text.clear();
        self.load_listing();
    }

    fn read_saved(&mut self) {
        let source = self.sources.current.clone();
        let saved = self.library().and_then(|lib| lib.saved_entries(&source).ok()).unwrap_or_default();
        self.sources.saved_cards = saved.iter().cloned().map(from_saved).collect();
        self.sources.saved = saved.into_iter().map(|e| e.external_id).collect();
        self.sources.heard = self.library().and_then(|lib| lib.heard_of(&source).ok()).unwrap_or_default();
    }

    /// Asks for the listing of where the window stands, from its beginning.
    fn load_listing(&mut self) {
        self.sources.imported = None;
        self.sources.preview_cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        self.sources.preview_cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.sources.sections_pending = 0;
        self.sources.sections.clear();
        self.sources.section_entries.clear();
        let s = &mut self.sources;
        s.generation += 1;
        s.entries.clear();
        s.presentation.clear();
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
        if at == UNFINISHED {
            let source = s.current.clone();
            let halfway = self.library().and_then(|lib| lib.unfinished(&source).ok()).unwrap_or_default();
            self.sources.entries = halfway.into_iter().map(from_heard).collect();
            self.sources.loading = false;
            return;
        }
        if let Some(section) = at.strip_prefix("view:") {
            let source = self.sources.current.clone();
            self.open_source_layout(&source, section);
        } else { self.ask_listing(at, None); }
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
        // The root and a search stay inside what was picked.
        let picks = if at.is_empty() || at == s.search_id { s.picks.clone() } else { BTreeMap::new() };
        let ask = Browse { at, cursor: cursor.clone(), text, language, picks };
        let (generation, source, tx) = (s.generation, s.current.clone(), s.tx.clone());
        self.ask_plugin(source, messages::op::BROWSE, messages::write(&ask), move |answer| {
            let result = answer.and_then(|bytes| messages::read::<SourceDocument>(&bytes));
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

    /// Plays an item: something live is asked of the plugin and started
    /// when the answer arrives; something with an end goes to the queue as
    /// an entry the player resolves itself, from where the listener left it.
    fn play_entry(&mut self, id: &str) { self.play_entry_at(id, None); }

    fn play_entry_at(&mut self, id: &str, position: Option<Duration>) {
        let Some((source, entry)) = self.entry_by_id(id) else { return };
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
        if let Some(address) = entry.id.strip_prefix(ADDRESS) {
            self.sound(address.to_string(), source, entry.clone());
            return;
        }
        if !entry.live {
            if source == self.sources.current && self.sources.medium.is_empty() && !self.sources.presentation.is_empty() { self.play_source_collection(id); return; }
            let Some(path) = self.keep_card(&source, &entry) else { return };
            let heard = self.library().and_then(|lib| lib.heard(&source, &entry.id).ok().flatten());
            let from = position.or_else(|| heard.filter(|h| !h.played && h.position_ms > 0).map(|h| Duration::from_millis(h.position_ms)));
            let (device, _) = self.chosen_output();
            self.replace_queue(vec![path]);
            self.spawn_from(0, &device, false, from, crate::Relaunch::Track);
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

    fn play_source_collection(&mut self, first: &str) {
        let entries: Vec<_> = self.sources.entries.iter().filter(|entry| entry.kind == EntryKind::Item && entry.playable && !entry.live).cloned().collect();
        if entries.is_empty() { return; }
        let index = entries.iter().position(|entry| entry.id == first).unwrap_or(0);
        let source = self.sources.current.clone();
        let cards: Vec<_> = entries.iter().map(|entry| Heard { external_id: entry.id.clone(), title: entry.title.clone(),
            subtitle: entry.subtitle.clone(), image_url: entry.image_url.clone().unwrap_or_default(), duration_ms: entry.duration_ms,
            ..Heard::default() }).collect();
        if !self.library().is_some_and(|lib| lib.remember_heard_many(&source, &cards).is_ok()) {
            self.sources.error = chilpa_i18n::t("it could not be saved");
            return;
        }
        for card in cards { self.sources.heard.entry(card.external_id.clone()).or_insert(card); }
        let paths = entries.iter().map(|entry| entry_of(&source, &entry.id)).collect();
        let (device, bit_perfect) = self.chosen_output();
        self.replace_queue_from(paths, Some(index));
        self.spawn_from(index, &device, bit_perfect, None, crate::Relaunch::Track);
    }

    /// Adds something with an end to the queue, behind what is there.
    fn queue_entry(&mut self, id: &str) {
        let Some((source, entry)) = self.entry_by_id(id) else { return };
        if entry.kind != EntryKind::Item || entry.live || !entry.playable {
            return;
        }
        let Some(path) = self.keep_card(&source, &entry) else { return };
        let (device, bit_perfect) = self.chosen_output();
        self.enqueue(&[path], &device, bit_perfect);
    }

    /// Writes down what an entry is before it goes to the queue, which only
    /// carries its id, and goes for its picture. Answers its queue entry.
    fn keep_card(&mut self, source: &str, entry: &Entry) -> Option<String> {
        let card = Heard {
            external_id: entry.id.clone(),
            title: entry.title.clone(),
            subtitle: entry.subtitle.clone(),
            image_url: entry.image_url.clone().unwrap_or_default(),
            duration_ms: entry.duration_ms,
            ..Heard::default()
        };
        if !self.library().is_some_and(|lib| lib.remember_heard(source, &card).is_ok()) {
            self.now.error = chilpa_i18n::t("it could not be saved");
            return None;
        }
        if source == self.sources.current { self.sources.heard.entry(entry.id.clone()).or_insert(card); }
        if let Some(address) = entry.image_url.clone() {
            crate::pictures::keep_later(address, |_| {});
        }
        Some(entry_of(source, &entry.id))
    }

    /// Marks something with an end as heard, or takes the mark away.
    fn toggle_heard(&mut self, id: &str) {
        let Some((source, entry)) = self.entry_by_id(id) else { return };
        if self.keep_card(&source, &entry).is_none() {
            return;
        }
        let was = self.sources.heard.get(id).is_some_and(|h| h.played);
        if self.library().is_some_and(|lib| lib.set_heard(&source, id, 0, !was).is_ok()) {
            self.read_saved();
        }
    }

    /// Writes down where the listener is in an episode: every few seconds
    /// while it plays, and with `now` when it ends, is left for another
    /// track or is stopped, which is when the place matters most. Near its
    /// end it counts as heard and starts over the next time.
    pub(crate) fn keep_place(&mut self, now: bool) {
        let Some((source, id)) = of_plugin(&self.now.path) else { return };
        if !now && (!self.now.playing || self.sources.kept_at.is_some_and(|at| at.elapsed() < KEEP_EVERY)) {
            return;
        }
        self.sources.kept_at = Some(Instant::now());
        let (at, total) = (self.now.seconds, self.now.total_seconds);
        if total <= 0.0 && at <= 0.0 {
            // It has not started: there is no place yet.
            return;
        }
        let done = total > 0.0 && at >= total - NEAR_THE_END.min(total * 0.05);
        let place = if done { 0 } else { (at * 1000.0) as u64 };
        if self.sources.progress_writer.is_none() {
            self.sources.progress_writer = self.library.as_ref().and_then(|lib| match lib.progress_writer() {
                Ok(writer) => writer,
                Err(error) => { eprintln!("source-progress: worker failed: {error}"); None }
            });
        }
        let kept = self.sources.progress_writer.as_ref().is_some_and(|writer| writer.save(&source, &id, place, done));
        if !kept { self.sources.progress_writer = None; self.sources.kept_at = None; }
        if kept && source == self.sources.current {
            if let Some(heard) = self.sources.heard.get_mut(&id) { heard.position_ms = place; heard.played = done; }
        }
    }

    fn toggle_saved(&mut self, id: &str) {
        // With no id it is the place the window stands in.
        let id = if id.is_empty() {
            match self.sources.trail.last() {
                Some(place) => place.id.clone(),
                None => return,
            }
        } else {
            id.to_string()
        };
        let Some((source, entry)) = self.entry_by_id(&id) else { return };
        if entry.id == SAVED || entry.id == UNFINISHED {
            return;
        }
        let was = if source == self.sources.current { self.sources.saved.contains(&id) } else { self.saved_elsewhere(&source, &id) };
        let kept = self.library().is_some_and(|lib| {
            if was {
                lib.forget_entry(&source, &id).is_ok()
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
        if source != self.sources.current {
            return;
        }
        if was {
            self.sources.saved.remove(&id);
        } else {
            self.sources.saved.insert(id);
        }
        self.sources.saved_cards.retain(|saved| saved.id != entry.id);
        if !was { self.sources.saved_cards.insert(0, entry); }
    }

    /// Takes in what the plugin threads answered. Called from `poll`.
    pub(crate) fn advance_sources(&mut self) {
        self.advance_playlist_refresh();
        if self.natives.broker.changed.swap(false, std::sync::atomic::Ordering::AcqRel) { self.refresh_playlist_sources(); self.mark_library_changed(); }
        for plugin in self.natives.take_history_changes() { self.catalog.expire_personal_pages(&plugin); }
        while let Ok(answer) = self.sources.rx.try_recv() {
            match answer {
                Answer::Extra { generation, id, kind, result } => {
                    if generation == self.sources.generation && self.sources.extras["id"] == id && self.sources.extras["kind"] == kind {
                        self.sources.extras = match result {
                            Ok(value) => serde_json::json!({"id":id,"kind":kind,"rows":value}),
                            Err(error) => serde_json::json!({"id":id,"kind":kind,"error":error}),
                        };
                    }
                }
                Answer::Native { source, serial, request, result } => {
                    if !self.sources.native_actions.get(&source).is_some_and(|s| s["pending"] == true && s["serial"] == serial) { continue; }
                    if !self.natives.is_native(&source) { self.sources.native_actions.remove(&source); continue; }
                    let heart = self.sources.native_actions[&source]["favorite"].clone();
                    let state = match result {
                        Err(error) => { self.now.error = error.clone(); serde_json::json!({"request_id":request,"pending":false,"error":error}) }
                        Ok(value) => {
                            if value["read_only"] != true {
                                if let Some(reference) = heart["ref"].as_str() { self.catalog.mark_native_favorite(&source, reference, heart["on"] == true); }
                                else {
                                    if let Some(refs) = value["changed_refs"].as_array() { self.catalog.invalidate_native_cards(&source, refs); }
                                    self.catalog.expire_native_pages(&source);
                                }
                                for kind in self.natives.library_kinds(&source) { self.catalog.retry(&format!("{source} section:{kind}")); }
                                self.mark_library_changed();
                            }
                            serde_json::json!({"request_id":request,"pending":false,"result":value})
                        }
                    };
                    self.sources.native_actions.insert(source, state);
                },
                Answer::Cover { generation, path } => {
                    if generation == self.sources.generation {
                        if let Some(entry) = self.sources.trail.last_mut().and_then(|c| c.entry.as_mut()) {
                            entry.image_url = url::Url::from_file_path(path).ok().map(|url| url.to_string());
                        }
                    }
                },
                Answer::Collection { serial, result } => self.take_collection(serial, result),
                Answer::Section { generation, at, title, result } => {
                    if generation != self.sources.generation { continue; }
                    self.sources.sections_pending = self.sources.sections_pending.saturating_sub(1);
                    if let Ok(document) = result {
                        let mut cards = Vec::new();
                        for entry in document.listing.entries {
                            let display = document.presentation.get(&entry.id).cloned().unwrap_or_default();
                            if !matches!(display.entity, chilpa_plugin::presentation::Entity::Album | chilpa_plugin::presentation::Entity::Playlist | chilpa_plugin::presentation::Entity::Artist) { continue; }
                            self.sources.presentation.insert(entry.id.clone(), display);
                            cards.push(self.entry_json(&self.sources.current, &entry, false));
                            self.sources.section_entries.insert(entry.id.clone(), entry);
                            if cards.len() >= 20 { break; }
                        }
                        if !cards.is_empty() { self.sources.sections.push(serde_json::json!({"id":at,"title":title,"entries":cards})); }
                    }
                }
                Answer::Imported { source, at, result } => {
                    self.sources.importing = false;
                    if result.is_ok() { self.mark_library_changed(); }
                    if source == self.sources.current && self.sources.trail.last().is_some_and(|crumb| crumb.id == at) {
                        match result { Ok(count) => self.sources.imported = Some(count), Err(error) => self.sources.error = error }
                    }
                }
                Answer::Metadata { generation, id, result } => {
                    if generation == self.sources.generation && self.sources.detail.as_ref().is_some_and(|(_, entry)| entry.id == id) {
                        self.sources.metadata_loading = false;
                        if let Ok(reply) = result {
                            if reply["available"] == true {
                                if let Ok(mut value) = serde_json::from_value::<Presentation>(reply["presentation"].clone()) {
                                    if value.clean().is_ok() {
                                        let current = self.sources.presentation.entry(id).or_default();
                                        let quality = &mut current.advertised_quality;
                                        let extra = value.advertised_quality;
                                        if !extra.format.is_empty() { quality.format = extra.format; }
                                        if !extra.bitrate_label.is_empty() { quality.bitrate_label = extra.bitrate_label; }
                                        quality.sample_rate_hz = extra.sample_rate_hz.or(quality.sample_rate_hz);
                                        quality.bit_depth = extra.bit_depth.or(quality.bit_depth);
                                        quality.channels = extra.channels.or(quality.channels);
                                        current.metadata.extend(value.metadata);
                                    }
                                }
                            }
                        }
                    }
                }
                Answer::Library { generation, result } => {
                    if generation == self.sources.generation { self.take_document(None, result); }
                }
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
        self.write_sounding();
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
        let stations = song.is_empty();
        if stations {
            sounding.picture = path.clone();
        }
        // A song's cover that arrives after the song is over is not shown.
        let current = song.is_empty() && self.now.artwork_path.is_empty() || song == self.now.title;
        if self.now.path == url && current {
            self.now.artwork_path = path;
        }
        if stations {
            self.write_sounding();
        }
    }

    /// Where the last thing that sounded from a source is written down, so
    /// the next run can still say what the queue's address is.
    fn sounding_file() -> std::path::PathBuf {
        chilpa_dirs::data().join("sounding.json")
    }

    fn write_sounding(&self) {
        if let Some(text) = self.sources.sounding.as_ref().and_then(|s| serde_json::to_string(s).ok()) {
            let _ = std::fs::write(Self::sounding_file(), text);
        }
    }

    /// Reads back what the last run left sounding, once.
    fn look_back(&mut self) {
        if std::mem::replace(&mut self.sources.looked_back, true) || self.sources.sounding.is_some() {
            return;
        }
        self.sources.sounding = std::fs::read_to_string(Self::sounding_file()).ok().and_then(|text| serde_json::from_str(&text).ok());
    }

    /// A station said what is on. Its own picture comes back, and the
    /// song's cover is looked for when the title reads "artist - song".
    pub(crate) fn song_changed(&mut self, title: &str) {
        let Some(sounding) = self.sources.sounding.as_ref().filter(|s| s.url == self.now.path) else {
            return;
        };
        if !title.trim().is_empty() && title != sounding.entry.title {
            let key = format!("{} {}", sounding.source, sounding.entry.id);
            if self.sources.on_air_history.last().is_none_or(|row| row["text"] != title || row["key"] != key) {
                let seconds = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
                self.sources.on_air_history.push(serde_json::json!({"key":key,"text":title,"time":seconds}));
                if self.sources.on_air_history.len() > 200 { self.sources.on_air_history.remove(0); }
            }
        }
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

    fn take_listing(&mut self, cursor: Option<String>, result: Result<SourceDocument, String>) {
        self.take_document(cursor.clone(), result.and_then(|document| document.clean(cursor.as_deref()).map_err(str::to_string)));
    }

    // Aggregated library pages have already passed protocol validation individually.
    fn take_document(&mut self, cursor: Option<String>, result: Result<SourceDocument, String>) {
        self.sources.loading = false;
        let document = match result {
            Ok(document) => document,
            Err(e) => {
                self.sources.error = e;
                self.sources.play_after_load = false;
                return;
            }
        };
        if !document.medium.is_empty() { self.sources.media_kinds.insert(self.sources.current.clone(),document.medium.clone()); self.sources.medium = document.medium; self.sources.attribution = document.attribution; }
        self.sources.music |= self.sources.medium.is_empty() && !document.presentation.is_empty();
        if cursor.is_none() { self.sources.presentation.clear(); }
        self.sources.presentation.extend(document.presentation);
        let mut listing = document.listing;
        listing.columns.extend(document.facets);
        let at_root = self.sources.trail.is_empty();
        let mut entries = listing.entries;
        self.remember_episode_parents(&entries);
        if at_root && cursor.is_none() && !listing.columns.is_empty() {
            // What a column starts on counts as picked: the root was
            // answered as if it were.
            for column in &listing.columns {
                if let Some(preset) = &column.preset {
                    self.sources.picks.entry(column.id.clone()).or_insert_with(|| preset.clone());
                }
            }
            self.sources.columns = listing.columns;
        }
        // An item listed inside an entry, with nothing to say whose it is,
        // is that entry's: an episode is its show's.
        if let Some(whose) = self.sources.trail.last().filter(|c| c.entry.is_some()).map(|c| c.title.clone()) {
            for entry in entries.iter_mut().filter(|e| e.kind == EntryKind::Item && e.subtitle.trim().is_empty()) {
                entry.subtitle = whose.clone();
            }
        }
        if at_root {
            // The place to type is drawn as a field, not as a row.
            self.sources.search_id = entries
                .iter()
                .find(|e| e.kind == EntryKind::Search)
                .map(|e| e.id.clone())
                .unwrap_or_default();
            if cursor.is_none() {
                let source = self.sources.current.clone();
                let halfway = self.library().and_then(|lib| lib.unfinished(&source).ok()).is_some_and(|h| !h.is_empty());
                let folder = |id: &str, title: String| Entry { id: id.to_string(), title, playable: true, ..Entry::default() };
                self.sources.entries = vec![folder(SAVED, chilpa_i18n::t("Saved"))];
                // Only when there is something to pick up.
                if halfway {
                    self.sources.entries.push(folder(UNFINISHED, chilpa_i18n::t("Continue listening")));
                }
            }
        }
        entries.retain(|e| e.kind != EntryKind::Search && e.id != SAVED && e.id != UNFINISHED);
        self.sources.entries.extend(entries);
        if at_root {
            self.sources.roots.insert(self.sources.current.clone(), self.sources.entries.iter()
                .filter(|entry| entry.kind == EntryKind::Folder).take(64).cloned().collect());
        }
        self.sources.next = listing.next;
        if at_root && cursor.is_none() && self.sources.medium.is_empty() && !self.sources.presentation.is_empty() { self.load_source_sections(); }
        if self.sources.play_after_load {
            if self.sources.next.is_some() && self.sources.entries.len() < 10_000 { self.load_more(); }
            else {
                self.sources.play_after_load = false;
                if self.sources.next.is_some() { self.sources.error = "This collection exceeds the playback limit. Choose a smaller collection.".into(); }
                else { self.play_source_collection(""); }
            }
        }
        if self.sources.title.is_empty() {
            self.sources.title = listing.title;
        }
    }

    fn load_source_sections(&mut self) {
        let Some(plugins) = self.plugins.clone() else { return };
        let requested = self.sources.layouts.get(&self.sources.current).and_then(|layout| layout.sections.get("discover"));
        let sections: Vec<_> = self.sources.entries.iter().filter(|entry| self.sources.presentation.get(&entry.id).is_some_and(|display| display.can_preview))
            .filter(|entry| requested.is_none_or(|section| section.paths.iter().any(|route| route.path.first() == Some(&entry.title))))
            .take(20).cloned().collect();
        self.sources.sections_pending = sections.len();
        let (source, generation, tx, cancel) = (self.sources.current.clone(), self.sources.generation, self.sources.tx.clone(), self.sources.preview_cancel.clone());
        std::thread::spawn(move || {
            for entry in sections {
                if cancel.load(std::sync::atomic::Ordering::Relaxed) { break; }
                let result = plugins.lock().map_err(|_| "The plugin host is unavailable.".into()).and_then(|mut host| {
                    if cancel.load(std::sync::atomic::Ordering::Relaxed) { return Err("Navigation changed.".into()); }
                    host.call_waiting(&source, messages::op::BROWSE, &messages::write(&Browse { at: entry.id.clone(), cursor: None, text: None, language: "en".into(), picks: BTreeMap::new() }))
                }).and_then(|bytes| messages::read::<SourceDocument>(&bytes)).and_then(|document| document.clean(None).map_err(str::to_string));
                let _ = tx.send(Answer::Section { generation, at: entry.id, title: entry.title, result });
            }
        });
    }

    /// Puts the card of a browsed entry on what just started, if it is that
    /// entry. Answers whether what started is live.
    pub(crate) fn name_from_entry(&mut self, path: &std::path::Path) -> bool {
        self.look_back();
        let path = path.display().to_string();
        let Some(sounding) = self.sources.sounding.as_ref().filter(|s| s.url == path) else {
            return false;
        };
        self.now.title = sounding.entry.title.clone();
        self.now.artist = sounding.entry.subtitle.clone();
        self.now.album = String::new();
        if self.now.artwork_path.is_empty() {
            self.now.artwork_path = sounding.picture.clone();
        }
        sounding.entry.live
    }

    /// Names something with an end that waits in a restored queue, from
    /// the card kept when it was queued: the queue only carries its id.
    pub(crate) fn name_from_heard(&mut self) {
        let Some((source, id)) = of_plugin(&self.now.path) else { return };
        // What the catalog read of it names it whole: its record and cover too.
        if let Some(row) = self.source_row(&self.now.path) {
            self.now.title = row.title;
            self.now.artist = row.artist;
            self.now.album = row.album;
            if self.now.total_seconds <= 0.0 {
                self.now.total_seconds = row.duration_ms as f64 / 1000.0;
            }
            if self.now.artwork_path.is_empty() {
                self.now.artwork_path = row.artwork_path;
            }
            self.now.album_id = row.album_id;
            return;
        }
        let Some(heard) = self.library().and_then(|lib| lib.heard(&source, &id).ok().flatten()) else { return };
        self.now.title = heard.title;
        self.now.artist = heard.subtitle;
        if self.now.total_seconds <= 0.0 {
            self.now.total_seconds = heard.duration_ms as f64 / 1000.0;
        }
        if self.now.artwork_path.is_empty() {
            self.now.artwork_path = crate::pictures::kept(&heard.image_url).unwrap_or_default();
        }
    }

    /// Sets the one opener the player holds: plugins here, servers there.
    pub(crate) fn install_openers(&mut self) {
        self.catalog.attach(self.plugins.clone(), self.natives.clone());
        let openers = std::sync::Arc::new(crate::source_opener::Openers::new(self.sources.servers_opener.clone(), self.plugins.clone(), self.catalog.clone()));
        self.sources.openers = Some(openers.clone());
        chilpa_core::remote::install(openers);
    }

    /// The name of the station that is on, when what plays is a live entry
    /// of a source.
    pub(crate) fn station_on(&self) -> Option<String> {
        let sounding = self.sources.sounding.as_ref()?;
        (self.now.live && self.now.path == sounding.url).then(|| sounding.entry.title.clone())
    }
}
