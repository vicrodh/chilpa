//! A source's music as the library's own: records, playlists and artists
//! on the same grids, record pages, quick views and track lists as the
//! records on disk.
//!
//! The screens name a source's record by a negative id and ask the same
//! facade functions they ask for a local one; those functions come here
//! for a negative id. What the screens need is answered from memory at
//! once, and what is missing or stale is asked of the plugin on one thread
//! of the catalog's own, in order of who is waiting: a listing or record
//! someone opened goes before one read ahead of time. Covers come on a few
//! threads of their own, since they do not go through the plugin.
//!
//! A listing is kept for a few minutes and every page a worker answered is
//! reused for as long as its handles are good, so walking back and forth
//! does not ask again. What was read is written to disk, and the next run
//! draws it before asking.
//!
//! Where a listing is, is an address: `root`, `section:<name>` (a section
//! of the source's layout, every route of it merged), `route:<name>:<n>`
//! (one route of it), `card:<id>` (what a card opens to, as an artist) or
//! `at:<handle>` (a folder of the tree, good while its worker runs). A
//! listing's key is the plugin's id and its address, with a space.

mod fetch;
mod native;
mod pages;
pub(crate) mod store;
pub(crate) mod view;
mod sorting;

use crate::plugins::Plugins;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};
use store::Store;

/// A listing younger than this is not asked for again.
const SHELF_FRESH: u64 = 300;
/// A record's tracks younger than this are not read again.
const TRACKS_FRESH: u64 = 6 * 3600;
const BACKGROUND_JOBS: usize = 8;
/// How many of a listing's loose tracks have their covers fetched.
const TRACK_COVERS: usize = 200;
/// How many covers are fetched at once.
const COVER_THREADS: usize = 6;
/// An action waiting for a record's tracks is dropped after this.
const ACTION_PATIENCE: std::time::Duration = std::time::Duration::from_secs(60);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Job {
    Shelf(String),
    Tracks(i64),
    /// One track by its queue entry: a list holds it, but no page of its
    /// source was ever read, so the catalog knows nothing of it.
    Track(String),
}

/// Something asked of records whose tracks were not read yet.
pub(crate) struct Deferred {
    pub albums: Vec<i64>,
    pub act: String,
    pub shuffled: bool,
    pub origin: Option<crate::queue_origin::QueueOrigin>,
    asked: Instant,
}

#[derive(Default)]
struct Queue {
    urgent: VecDeque<Job>,
    later: VecDeque<Job>,
    queued: HashSet<Job>,
    covers: VecDeque<String>,
    covers_queued: HashSet<String>,
    /// Failed downloads cool down before another visible row can retry.
    covers_failed: HashMap<String, Instant>,
}

#[derive(Default)]
struct Inner {
    quality_requested: HashMap<i64, Instant>,
    quality_blocked: HashMap<String, Instant>,
    quality_running: usize,
    store: Store,
    loaded: bool,
    queue: Queue,
    /// Why the last attempt failed, by listing key or by card.
    shelf_errors: HashMap<String, String>,
    card_errors: HashMap<i64, String>,
    card_retry_at: HashMap<i64, Instant>,
    /// When a track asked for by its entry could not be read.
    track_failed_at: HashMap<String, Instant>,
    /// What is being read right now, so it is not queued twice.
    running: HashSet<Job>,
    deferred: Vec<Deferred>,
    /// Each plugin's approved domains, which its covers are fetched within.
    domains: HashMap<String, Vec<String>>,
    dirty: bool,
    /// The queue entry whose cover was last asked for, to ask once.
    cover_asked: String,
    /// Each plugin's load as last seen, for what is taken in from outside.
    epochs: HashMap<String, u64>,
    /// Covers asked for that belong to no card or track (a search result),
    /// by address, with the plugin whose domains they are fetched within.
    images: HashMap<String, String>,
}

pub(crate) struct Catalog {
    inner: Mutex<Inner>,
    wake: Condvar,
    covers_wake: Condvar,
    plugins: Mutex<Option<Arc<Mutex<Plugins>>>>,
    /// Process plugins, read without the sandbox lock and in parallel.
    natives: Mutex<Option<Arc<crate::natives::Natives>>>,
    /// Bumped when a listing or a record's tracks change, and when a cover
    /// arrives: the window reads again on each.
    content_rev: AtomicI32,
    art_rev: AtomicI32,
    started: Mutex<bool>,
    page_reads: Mutex<HashMap<String, Arc<Mutex<()>>>>,
}

fn now() -> u64 {
    crate::now_unix().max(0) as u64
}

/// Whether a card's tracks can be asked for now: a compatibility source's
/// needs a live handle, a process plugin's only its identity.
fn readable(card: &store::Card, natives: Option<&crate::natives::Natives>) -> bool {
    card.handle.is_some() || natives.is_some_and(|natives| natives.is_native(&card.plugin))
}

/// The plugin and the address of a listing key.
pub(crate) fn split(key: &str) -> (&str, &str) {
    key.split_once(' ').unwrap_or((key, ""))
}

impl Catalog {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(Inner::default()),
            wake: Condvar::new(),
            covers_wake: Condvar::new(),
            plugins: Mutex::new(None),
            natives: Mutex::new(None),
            content_rev: AtomicI32::new(0),
            art_rev: AtomicI32::new(0),
            started: Mutex::new(false),
            page_reads: Mutex::new(HashMap::new()),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Hands the catalog the plugin host, and starts its threads the first
    /// time: one that reads the disk and then asks plugins, and the cover
    /// fetchers.
    pub fn attach(self: &Arc<Self>, plugins: Option<Arc<Mutex<Plugins>>>, natives: Arc<crate::natives::Natives>) {
        *self.plugins.lock().unwrap_or_else(|e| e.into_inner()) = plugins;
        *self.natives.lock().unwrap_or_else(|e| e.into_inner()) = Some(natives);
        self.wake.notify_all();
        self.prefetch_homes();
        let mut started = self.started.lock().unwrap_or_else(|e| e.into_inner());
        if std::mem::replace(&mut *started, true) {
            return;
        }
        let me = self.clone();
        let _ = std::thread::Builder::new().name("chilpa-catalog".into()).spawn(move || fetch::run(&me));
        for n in 0..COVER_THREADS {
            let me = self.clone();
            let _ = std::thread::Builder::new().name(format!("chilpa-covers-{n}")).spawn(move || fetch::covers(&me));
        }
    }

    /// The process plugins, once the plugins have started.
    pub(crate) fn natives(&self) -> Option<Arc<crate::natives::Natives>> {
        self.natives.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn revisions(&self) -> (i32, i32) {
        (self.content_rev.load(Ordering::Relaxed), self.art_rev.load(Ordering::Relaxed))
    }

    pub(crate) fn changed(&self) {
        self.content_rev.fetch_add(1, Ordering::Relaxed);
    }

    /// Says that what the lists show of a source changed, from outside.
    pub(crate) fn touch(&self) {
        self.changed();
    }

    /// What the source calls each track of a listing.
    pub(crate) fn rating_identities_of(&self, key: &str) -> Vec<String> {
        let inner = self.lock();
        let Some(shelf) = inner.store.shelves.get(key) else { return Vec::new() };
        shelf.tracks.iter().filter_map(|id| inner.store.tracks.get(id))
            .filter_map(|track| track.metadata["ref"].as_str().filter(|_| track.metadata["kind"] == "track"))
            .map(|reference| reference.split_once("@playlist:").map_or(reference, |(song, _)| song).to_string())
            .collect()
    }

    fn queue(&self, inner: &mut Inner, job: Job, urgent: bool) {
        if matches!(&job, Job::Tracks(id) if inner.card_errors.contains_key(id) && inner.card_retry_at.get(id).is_none_or(|at| at.elapsed() < std::time::Duration::from_secs(30))) || inner.running.contains(&job) {
            return;
        }
        if inner.queue.queued.contains(&job) {
            if urgent {
                // Something read ahead that someone now waits for goes first.
                inner.queue.later.retain(|j| *j != job);
                if !inner.queue.urgent.contains(&job) {
                    inner.queue.urgent.push_front(job);
                }
            }
            return;
        }
        inner.queue.queued.insert(job.clone());
        if urgent {
            inner.queue.urgent.push_back(job);
        } else {
            // What is read ahead is for what was opened last: the oldest
            // guesses go, so someone who opens something waits behind few.
            inner.queue.later.push_back(job);
            while inner.queue.later.len() > BACKGROUND_JOBS {
                if let Some(old) = inner.queue.later.pop_front() {
                    inner.queue.queued.remove(&old);
                }
            }
        }
        self.wake.notify_all();
    }

    fn want_cover(&self, inner: &mut Inner, image: &str) {
        if image.is_empty() || inner.queue.covers_queued.contains(image)
            || inner.queue.covers_failed.get(image).is_some_and(|at| at.elapsed() < Duration::from_secs(30)) {
            return;
        }
        inner.queue.covers_failed.remove(image);
        inner.queue.covers_queued.insert(image.to_string());
        inner.queue.covers.push_back(image.to_string());
        self.covers_wake.notify_one();
    }

    /// I warm homes after either startup dependency becomes ready: plugins
    /// may finish before or after the disk catalog, including a later attach.
    pub(crate) fn prefetch_homes(&self) {
        if let Some(natives) = self.natives() {
            for plugin in natives.ids().into_iter().filter(|id| natives.has_pages(id)) {
                self.prefetch_shelf(&format!("{plugin} native:discover:home"));
            }
        }
    }

    /// I warm homes behind pages the listener has already opened.
    pub(super) fn prefetch_shelf(&self, key: &str) {
        let mut inner = self.lock();
        if inner.loaded && inner.store.shelves.get(key).is_none_or(|s| s.epoch == 0 || now().saturating_sub(s.read_at) > SHELF_FRESH)
            && !inner.shelf_errors.contains_key(key) {
            self.queue(&mut inner, Job::Shelf(key.to_string()), false);
        }
    }

    /// A listing as it is known now, asking for it when it is missing or stale.
    pub fn shelf(&self, key: &str) -> Option<store::Shelf> {
        let mut inner = self.lock();
        let shelf = inner.store.shelves.get(key).cloned();
        // What an earlier run read is drawn at once and asked for again once.
        let stale = shelf.as_ref().is_none_or(|s| s.epoch == 0 || now().saturating_sub(s.read_at) > SHELF_FRESH);
        if stale && inner.loaded && !inner.shelf_errors.contains_key(key) {
            self.queue(&mut inner, Job::Shelf(key.to_string()), true);
        }
        if let Some(shelf) = &shelf {
            let images: Vec<String> = shelf.cards.iter().filter_map(|id| inner.store.cards.get(id))
                .filter(|c| c.art.is_empty()).map(|c| c.image.clone())
                .chain(shelf.tracks.iter().take(TRACK_COVERS).filter_map(|id| inner.store.tracks.get(id)).map(|t| t.image.clone()))
                .collect();
            for image in images {
                self.want_cover(&mut inner, &image);
            }

        }
        shelf
    }

    /// Asks again for a listing that failed, or that the listener wants
    /// fresh.
    pub fn expire_personal_pages(&self, plugin: &str) {
        let mut inner = self.lock();
        for (key,shelf) in &mut inner.store.shelves { if key.starts_with(&format!("{plugin} native:")) && shelf.document["personal"] == true { shelf.read_at = 1; } }
        inner.dirty = true;
        self.changed();
    }

    pub fn expire_native_pages(&self, plugin: &str) {
        let mut inner = self.lock();
        for (key,shelf) in &mut inner.store.shelves { if key.starts_with(&format!("{plugin} native:")) { shelf.read_at = 0; } }
        inner.dirty = true;
        self.changed();
    }

    pub fn invalidate_native_cards(&self, plugin: &str, references: &[serde_json::Value]) {
        let mut inner = self.lock();
        let mut changed = Vec::new();
        for card in inner.store.cards.values_mut().filter(|c| c.plugin == plugin && references.iter().any(|r| r.as_str() == Some(c.identity.as_str()))) {
            card.tracks = None;
            card.read_at = 0;
            changed.push(card.id);
        }
        for id in changed { inner.card_errors.remove(&id); inner.card_retry_at.remove(&id); }
        inner.dirty = true;
        self.changed();
    }

    pub fn retry(&self, key: &str) {
        let mut inner = self.lock();
        inner.shelf_errors.remove(key);
        let cards = inner.store.shelves.get(key).map(|s| s.cards.clone()).unwrap_or_default();
        for id in cards {
            inner.card_errors.remove(&id);
            inner.card_retry_at.remove(&id);
            if let Some(card) = inner.store.cards.get_mut(&id).filter(|c| c.metadata["available"] == false) { card.tracks = None; }
        }
        if let Some(shelf) = inner.store.shelves.get_mut(key) {
            shelf.read_at = 0;
        }
        self.queue(&mut inner, Job::Shelf(key.to_string()), true);
    }

    pub fn shelf_title(&self, key: &str) -> Option<String> {
        self.lock().store.shelves.get(key).map(|s| s.title.clone())
    }

    pub fn shelf_error(&self, key: &str) -> Option<String> {
        self.lock().shelf_errors.get(key).cloned()
    }

    /// A listing as it is kept, without asking for it again.
    pub fn stored_shelf(&self, key: &str) -> Option<store::Shelf> {
        self.lock().store.shelves.get(key).cloned()
    }

    pub fn loading(&self, key: &str) -> bool {
        let inner = self.lock();
        let job = Job::Shelf(key.to_string());
        !inner.loaded || inner.running.contains(&job) || inner.queue.queued.contains(&job)
    }

    /// I map transient display ids to stable identities without requesting the network.
    pub(crate) fn rating_identities(&self) -> Vec<(String, i64, String, String)> {
        let inner = self.lock();
        let cards = inner.store.cards.values().filter_map(|card| {
            let kind = match card.kind { store::CardKind::Album => "album", store::CardKind::Playlist => "playlist", _ => return None };
            Some((kind.into(), card.id, card.plugin.clone(), card.identity.clone()))
        });
        cards.chain(inner.store.tracks.values().map(|track| {
            // A playlist occurrence identifies an editable row; personal stars identify its song.
            let identity = track.metadata["ref"].as_str().filter(|_| track.metadata["kind"] == "track")
                .map(|reference| reference.split_once("@playlist:").map_or(reference, |(song, _)| song).to_string())
                .unwrap_or_else(|| track.path.clone());
            ("track".into(), track.id, track.plugin.clone(), identity)
        })).collect()
    }

    pub fn card(&self, id: i64) -> Option<store::Card> {
        let mut inner = self.lock();
        let card = inner.store.cards.get(&id).cloned()?;
        if card.art.is_empty() {
            self.want_cover(&mut inner, &card.image);
        }
        drop(inner);
        if card.kind == store::CardKind::Label && card.image.is_empty() && self.natives().is_some_and(|natives| natives.has_pages(&card.plugin)) {
            self.shelf(&format!("{} native:{}", card.plugin, card.identity));
        }
        Some(card)
    }

    /// A record's tracks as rows, asking for them when they were never
    /// read or are old. `None` while they are not known.
    pub fn tracks(&self, id: i64) -> Option<Vec<store::Track>> {
        let mut inner = self.lock();
        let card = inner.store.cards.get(&id)?.clone();
        let old = card.tracks.is_none() || now().saturating_sub(card.read_at) > TRACKS_FRESH;
        if old && readable(&card, self.natives().as_deref()) {
            self.queue(&mut inner, Job::Tracks(id), card.tracks.is_none());
        }
        let ids = card.tracks?;
        Some(ids.iter().filter_map(|t| inner.store.tracks.get(t).cloned()).collect())
    }

    pub fn card_error(&self, id: i64) -> Option<String> {
        self.lock().card_errors.get(&id).cloned()
    }

    pub fn track_by_path(&self, path: &str) -> Option<(store::Track, String)> {
        let inner = self.lock();
        let track = inner.store.track_by_path(path)?.clone();
        let art = inner.store.art_of(&track);
        Some((track, art))
    }

    pub fn tracks_by_ids(&self, ids: &[i64]) -> Vec<(store::Track, String)> {
        let inner = self.lock();
        ids.iter().filter_map(|id| inner.store.tracks.get(id)).map(|t| (t.clone(), inner.store.art_of(t))).collect()
    }

    pub fn art_of_card(&self, id: i64) -> String {
        self.lock().store.cards.get(&id).map(|c| c.art.clone()).unwrap_or_default()
    }

    /// Takes in a record or a track a search found, which no listing showed:
    /// the search's handle is good for the worker's current load.
    pub fn adopt(&self, plugin: &str, epoch: Option<u64>, found: &crate::source_search::RemoteFound) -> i64 {
        if let Some(item) = &found.item {
            let mut inner = self.lock();
            let id = if found.collection { inner.store.take_item(plugin, item, now()) } else { inner.store.take_item_track(plugin, item, None, now()) }.unwrap_or_default();
            inner.dirty = true;
            drop(inner);
            self.changed();
            return id;
        }
        use chilpa_plugin::messages::{Entry, EntryKind};
        let entry = Entry { id: found.id.clone(), title: found.title.clone(), subtitle: found.detail.clone(),
            image_url: Some(found.art.clone()).filter(|a| !a.is_empty()), duration_ms: found.duration_ms,
            kind: if found.collection { EntryKind::Folder } else { EntryKind::Item }, playable: true, ..Entry::default() };
        let mut document = chilpa_plugin::presentation::SourceDocument::default();
        let entity = match found.entity.as_str() { "artist" => chilpa_plugin::presentation::Entity::Artist, "playlist" => chilpa_plugin::presentation::Entity::Playlist, _ => if found.collection { chilpa_plugin::presentation::Entity::Album } else { chilpa_plugin::presentation::Entity::Track } };
        document.presentation.insert(found.id.clone(), chilpa_plugin::presentation::Presentation { entity, ..Default::default() });
        document.listing.entries.push(entry);
        let mut inner = self.lock();
        let epoch = epoch.unwrap_or_else(|| inner.epochs.get(plugin).copied().unwrap_or_default());
        let read = inner.store.take(plugin, epoch, &document, now());
        let image = found.art.clone();
        self.want_cover(&mut inner, &image);
        inner.dirty = true;
        drop(inner);
        self.changed();
        read.cards.first().or(read.tracks.first()).copied().unwrap_or_default()
    }

    /// Waits for records' tracks, then does what was asked with them.
    pub fn defer(&self, albums: Vec<i64>, act: &str, shuffled: bool, origin: Option<crate::queue_origin::QueueOrigin>) {
        let mut inner = self.lock();
        for id in &albums {
            if inner.store.cards.get(id).is_some_and(|c| c.tracks.is_none()) {
                inner.card_errors.remove(id);
                self.queue(&mut inner, Job::Tracks(*id), true);
            }
        }
        inner.deferred.retain(|d| d.act != act || d.albums != albums);
        inner.deferred.push(Deferred { albums, act: act.into(), shuffled, origin, asked: Instant::now() });
    }

    /// The deferred actions whose records are all read now. Those that
    /// waited too long, or whose records failed, are dropped.
    pub fn ready(&self) -> Vec<Deferred> {
        let mut inner = self.lock();
        let deferred = std::mem::take(&mut inner.deferred);
        let mut ready = Vec::new();
        for d in deferred {
            let read = d.albums.iter().all(|id| inner.store.cards.get(id).is_none_or(|c| c.tracks.is_some()));
            let failed = d.albums.iter().any(|id| inner.card_errors.contains_key(id));
            if read {
                ready.push(d);
            } else if !failed && d.asked.elapsed() < ACTION_PATIENCE {
                inner.deferred.push(d);
            }
        }
        ready
    }

    /// The track ids of a listing, for its track list.
    pub fn shelf_tracks(&self, key: &str) -> Vec<(store::Track, String)> {
        let ids = self.lock().store.shelves.get(key).map(|s| s.tracks.clone()).unwrap_or_default();
        self.tracks_by_ids(&ids)
    }

    pub fn saved_playlist_cards(&self) -> Vec<store::Card> {
        let inner = self.lock();
        let mut cards = Vec::new();
        for (key, shelf) in &inner.store.shelves {
            if key.ends_with(" section:playlists") {
                cards.extend(shelf.cards.iter().filter_map(|id| inner.store.cards.get(id)).cloned());
            }
        }
        cards
    }

    pub fn cards(&self, ids: &[i64]) -> Vec<store::Card> {
        let inner = self.lock();
        ids.iter().filter_map(|id| inner.store.cards.get(id).cloned()).collect()
    }

    pub fn forget_plugin(&self, plugin: &str) {
        let mut inner = self.lock();
        inner.store.forget_plugin(plugin);
        let cards: HashSet<_> = inner.store.cards.keys().copied().collect();
        let belongs = |job: &Job| match job {
            Job::Shelf(key) => key.starts_with(&format!("{plugin} ")),
            Job::Tracks(id) => !cards.contains(id),
            Job::Track(path) => crate::source_opener::of_plugin(path).is_some_and(|(p, _)| p == plugin),
        };
        inner.queue.urgent.retain(|job| !belongs(job));
        inner.queue.later.retain(|job| !belongs(job));
        inner.queue.queued.retain(|job| !belongs(job));
        inner.deferred.retain(|d| d.albums.iter().all(|id| cards.contains(id)));
        inner.domains.remove(plugin);
        inner.epochs.remove(plugin);
        fetch::save(&inner.store);
        inner.dirty = true;
        drop(inner);
        self.changed();
    }
    /// Forgets the handles a plugin gave out, as when its worker restarts.
    /// What was read stays; it is only asked again.
    pub fn forget_handles(&self, plugin: &str) {
        let mut inner = self.lock();
        for card in inner.store.cards.values_mut().filter(|c| c.plugin == plugin) {
            card.handle = None;
        }
        let prefix = format!("{plugin} ");
        for (_, shelf) in inner.store.shelves.iter_mut().filter(|(k, _)| k.starts_with(&prefix)) {
            shelf.folders.clear();
            shelf.read_at = 0;
        }
        inner.shelf_errors.retain(|k, _| !k.starts_with(&prefix));
        drop(inner);
        self.changed();
    }

    pub fn drawn(&self, plugin: &str) -> bool {
        self.lock().store.drawn.get(plugin).copied().unwrap_or(false)
    }

    pub fn remember_drawn(&self, plugin: &str, drawn: bool) {
        let mut inner = self.lock();
        if inner.loaded && inner.store.drawn.insert(plugin.to_string(), drawn) != Some(drawn) {
            inner.dirty = true;
        }
    }

    /// Read technical metadata for a displayed row without opening audio.
    pub fn want_quality(self: &Arc<Self>, id: i64) {
        let Some(natives) = self.natives() else { return };
        let mut inner = self.lock();
        let Some(track) = inner.store.tracks.get(&id) else { return };
        if track.format.as_ref().is_some_and(|f| f.rate.is_some() && (f.bits.is_some() || matches!(f.codec.as_str(),"aac"|"mp3"|"m4a"))) { return; }
        let (plugin, reference) = (track.plugin.clone(), track.metadata["ref"].as_str().unwrap_or_default().to_string());
        if reference.is_empty() || !natives.has_track_quality(&plugin) || !natives.is_native(&plugin) || inner.quality_running >= 2 { return; }
        if inner.quality_blocked.get(&plugin).is_some_and(|at| at.elapsed().as_secs()<60) || inner.quality_requested.get(&id).is_some_and(|at| at.elapsed().as_secs()<300) { return; }
        inner.quality_requested.retain(|_,at| at.elapsed().as_secs()<300);
        inner.quality_requested.insert(id,Instant::now()); inner.quality_running += 1;
        drop(inner);
        let catalog = self.clone();
        std::thread::spawn(move || {
            let result = natives.call(&plugin,"track.quality",serde_json::json!({"ref":reference}),std::time::Duration::from_secs(20));
            let mut inner = catalog.lock(); inner.quality_running = inner.quality_running.saturating_sub(1);
            if natives.is_native(&plugin) {
                match result {
                    Ok(answer) => if let Some(format) = native::format(&answer) {
                        if let Some(track) = inner.store.tracks.get_mut(&id) { track.format = Some(format); inner.dirty = true; }
                    },
                    Err(_) => { inner.quality_blocked.insert(plugin,Instant::now()); }
                }
            }
            drop(inner); catalog.changed();
        });
    }

    /// Keep the format actually decoded instead of a generic catalog label.
    pub fn measured(&self, path: &str, codec: Option<&str>, bits: Option<u8>, rate: u32) {
        let mut inner = self.lock();
        let Some(id) = inner.store.track_by_path(path).map(|t| t.id) else { return };
        let Some(track) = inner.store.tracks.get_mut(&id) else { return };
        let format = track.format.get_or_insert_with(store::Format::default);
        let codec = codec.map(str::to_string).unwrap_or_else(|| format.codec.clone());
        if format.bits == bits && format.rate == Some(rate) && format.codec == codec {
            return;
        }
        format.codec = codec;
        format.bits = bits;
        format.advertised_hires = false;
        format.rate = Some(rate).filter(|r| *r > 0);
        inner.dirty = true;
        drop(inner);
        self.changed();
    }

    /// What a plugin is called, kept so a restored tab has its name before
    /// the plugins start.
    pub fn name(&self, plugin: &str) -> Option<String> {
        self.lock().store.names.get(plugin).cloned()
    }

    pub fn remember_name(&self, plugin: &str, name: &str) {
        let mut inner = self.lock();
        if inner.loaded && !name.is_empty() && inner.store.names.get(plugin).map(String::as_str) != Some(name) {
            inner.store.names.insert(plugin.to_string(), name.to_string());
            inner.dirty = true;
        }
    }

    /// Fetches a picture a plugin gave outside any listing, as a search
    /// result's, within that plugin's domains.
    pub fn want_image(&self, plugin: &str, image: &str) {
        if image.is_empty() || !(image.starts_with("https://") || image.starts_with("http://")) {
            return;
        }
        let mut inner = self.lock();
        if inner.images.len() < 4096 {
            inner.images.insert(image.to_string(), plugin.to_string());
        }
        self.want_cover(&mut inner, image);
    }

    /// Asks for a source's track a list shows and the catalog does not know
    /// (it came into a list of mine without any page of its source being
    /// read): its title, record and artist arrive with the next change.
    pub fn want_track(&self, path: &str) {
        let Some((plugin, _)) = crate::source_opener::of_plugin(path) else { return };
        let mut inner = self.lock();
        if inner.store.track_by_path(path).is_some() || !self.natives().is_some_and(|n| n.is_native(&plugin)) {
            return;
        }
        if inner.track_failed_at.get(path).is_some_and(|at| at.elapsed() < Duration::from_secs(60)) {
            return;
        }
        self.queue(&mut inner, Job::Track(path.to_string()), true);
    }

    /// Asks for a track's cover when a list shows it without one: a track
    /// that came by search or radio, or into a list, was never on a shelf
    /// that asked for its record's cover.
    pub fn want_art_of(&self, path: &str) {
        let mut inner = self.lock();
        let Some(track) = inner.store.track_by_path(path).cloned() else { return };
        let image = inner.store.cards.get(&track.album_id).map(|c| c.image.clone()).filter(|i| !i.is_empty()).unwrap_or(track.image);
        self.want_cover(&mut inner, &image);
    }

    /// Asks for the cover of what started playing, once per entry.
    pub fn want_track_cover(&self, path: &str) {
        let mut inner = self.lock();
        if inner.cover_asked == path {
            return;
        }
        inner.cover_asked = path.to_string();
        let Some(track) = inner.store.track_by_path(path).cloned() else { return };
        let image = inner.store.cards.get(&track.album_id).map(|c| c.image.clone()).filter(|i| !i.is_empty()).unwrap_or(track.image);
        self.want_cover(&mut inner, &image);
    }
}

#[cfg(test)]
mod retry_tests {
    use super::*;
    #[test]
    fn opening_a_prefetched_home_promotes_one_job_and_keeps_cached_content() {
        let catalog = Catalog::new();
        let key = "fixture native:discover:home";
        catalog.lock().loaded = true;
        catalog.lock().store.shelves.insert(key.into(), store::Shelf {
            title: "Home".into(), read_at: now().saturating_sub(SHELF_FRESH + 1), epoch: 1, ..Default::default()
        });
        catalog.prefetch_shelf(key);
        catalog.prefetch_shelf(key);
        assert_eq!(catalog.lock().queue.later.len(), 1);
        assert_eq!(catalog.shelf(key).unwrap().title, "Home");
        assert!(catalog.lock().queue.later.is_empty());
        assert_eq!(catalog.lock().queue.urgent.len(), 1);
        assert_eq!(catalog.lock().queue.queued.len(), 1);
        let fresh = "fixture native:discover:fresh";
        catalog.lock().store.shelves.insert(fresh.into(), store::Shelf { read_at: now(), epoch: 1, ..Default::default() });
        catalog.prefetch_shelf(fresh);
        assert_eq!(catalog.lock().queue.queued.len(), 1);
    }

    #[test]
    fn a_failed_search_cover_can_retry_without_restarting_the_application() {
        let catalog = Catalog::new();
        let image = "https://images.example.org/cover.jpg";
        catalog.lock().queue.covers_failed.insert(image.into(), Instant::now());
        catalog.want_image("example.source", image);
        assert!(catalog.lock().queue.covers.is_empty());
        catalog.lock().queue.covers_failed.insert(image.into(), Instant::now() - Duration::from_secs(31));
        catalog.want_image("example.source", image);
        assert_eq!(catalog.lock().queue.covers.pop_front().as_deref(), Some(image));
    }
    #[test]
    fn redraws_do_not_repeat_failed_preloads_but_explicit_play_retries() {
        let catalog = Catalog::new();
        let key = "app.example.music root";
        {
            let mut inner = catalog.lock();
            inner.loaded = true;
            inner.store.cards.insert(-1, store::Card { id: -1, handle: Some(store::Handle { epoch: 1, id: "record".into() }), ..Default::default() });
            inner.store.shelves.insert(key.into(), store::Shelf { cards: vec![-1], read_at: now(), epoch: 1, ..Default::default() });
            inner.card_errors.insert(-1, "Not found".into());
        }
        for _ in 0..20 { catalog.shelf(key); catalog.tracks(-1); }
        assert!(catalog.lock().queue.queued.is_empty());
        catalog.defer(vec![-1], "play", false, None);
        assert!(catalog.lock().queue.queued.contains(&Job::Tracks(-1)));
        assert!(catalog.card_error(-1).is_none());
        catalog.lock().card_errors.insert(-1, "Not found".into());
        catalog.retry(key);
        assert!(catalog.card_error(-1).is_none());
    }
}
