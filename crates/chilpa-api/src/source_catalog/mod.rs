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
pub(crate) mod store;
mod view;

use crate::plugins::Plugins;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Instant;
use store::{CardKind, Store};

/// A listing younger than this is not asked for again.
const SHELF_FRESH: u64 = 300;
/// A record's tracks younger than this are not read again.
const TRACKS_FRESH: u64 = 6 * 3600;
/// How many records of a listing have their tracks read ahead.
const READ_AHEAD: usize = 8;
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
}

/// Something asked of records whose tracks were not read yet.
pub(crate) struct Deferred {
    pub albums: Vec<i64>,
    pub act: String,
    pub shuffled: bool,
    asked: Instant,
}

#[derive(Default)]
struct Queue {
    urgent: VecDeque<Job>,
    later: VecDeque<Job>,
    queued: HashSet<Job>,
    covers: VecDeque<String>,
    covers_queued: HashSet<String>,
    /// Covers that could not be fetched this run: not asked for again.
    covers_failed: HashSet<String>,
}

#[derive(Default)]
struct Inner {
    store: Store,
    loaded: bool,
    queue: Queue,
    /// Why the last attempt failed, by listing key or by card.
    shelf_errors: HashMap<String, String>,
    card_errors: HashMap<i64, String>,
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
    /// Bumped when a listing or a record's tracks change, and when a cover
    /// arrives: the window reads again on each.
    content_rev: AtomicI32,
    art_rev: AtomicI32,
    started: Mutex<bool>,
}

fn now() -> u64 {
    crate::now_unix().max(0) as u64
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
            content_rev: AtomicI32::new(0),
            art_rev: AtomicI32::new(0),
            started: Mutex::new(false),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Hands the catalog the plugin host, and starts its threads the first
    /// time: one that reads the disk and then asks plugins, and the cover
    /// fetchers.
    pub fn attach(self: &Arc<Self>, plugins: Option<Arc<Mutex<Plugins>>>) {
        *self.plugins.lock().unwrap_or_else(|e| e.into_inner()) = plugins;
        self.wake.notify_all();
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

    pub fn revisions(&self) -> (i32, i32) {
        (self.content_rev.load(Ordering::Relaxed), self.art_rev.load(Ordering::Relaxed))
    }

    fn changed(&self) {
        self.content_rev.fetch_add(1, Ordering::Relaxed);
    }

    fn queue(&self, inner: &mut Inner, job: Job, urgent: bool) {
        if inner.running.contains(&job) {
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
            while inner.queue.later.len() > READ_AHEAD {
                if let Some(old) = inner.queue.later.pop_front() {
                    inner.queue.queued.remove(&old);
                }
            }
        }
        self.wake.notify_all();
    }

    fn want_cover(&self, inner: &mut Inner, image: &str) {
        if image.is_empty() || inner.queue.covers_queued.contains(image) || inner.queue.covers_failed.contains(image) {
            return;
        }
        inner.queue.covers_queued.insert(image.to_string());
        inner.queue.covers.push_back(image.to_string());
        self.covers_wake.notify_one();
    }

    /// A listing as it is known now, asking for it when it is missing or
    /// stale. Its covers and the first records' tracks are asked for too.
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
            let ahead: Vec<i64> = shelf.cards.iter().filter_map(|id| inner.store.cards.get(id))
                .filter(|c| c.kind != CardKind::Artist && c.tracks.is_none() && c.handle.is_some())
                .take(READ_AHEAD).map(|c| c.id).collect();
            for id in ahead {
                self.queue(&mut inner, Job::Tracks(id), false);
            }
        }
        shelf
    }

    /// Asks again for a listing that failed, or that the listener wants
    /// fresh.
    pub fn retry(&self, key: &str) {
        let mut inner = self.lock();
        inner.shelf_errors.remove(key);
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

    pub fn loading(&self, key: &str) -> bool {
        let inner = self.lock();
        let job = Job::Shelf(key.to_string());
        !inner.loaded || inner.running.contains(&job) || inner.queue.queued.contains(&job)
    }

    pub fn card(&self, id: i64) -> Option<store::Card> {
        let mut inner = self.lock();
        let card = inner.store.cards.get(&id).cloned()?;
        if card.art.is_empty() {
            self.want_cover(&mut inner, &card.image);
        }
        Some(card)
    }

    /// A record's tracks as rows, asking for them when they were never
    /// read or are old. `None` while they are not known.
    pub fn tracks(&self, id: i64) -> Option<Vec<store::Track>> {
        let mut inner = self.lock();
        let card = inner.store.cards.get(&id)?.clone();
        let old = card.tracks.is_none() || now().saturating_sub(card.read_at) > TRACKS_FRESH;
        if old && card.handle.is_some() && !inner.card_errors.contains_key(&id) {
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
        use chilpa_plugin::messages::{Entry, EntryKind};
        let entry = Entry { id: found.id.clone(), title: found.title.clone(), subtitle: found.detail.clone(),
            image_url: Some(found.art.clone()).filter(|a| !a.is_empty()), duration_ms: found.duration_ms,
            kind: if found.collection { EntryKind::Folder } else { EntryKind::Item }, playable: true, ..Entry::default() };
        let mut document = chilpa_plugin::presentation::SourceDocument::default();
        let entity = if found.collection { chilpa_plugin::presentation::Entity::Album } else { chilpa_plugin::presentation::Entity::Track };
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
    pub fn defer(&self, albums: Vec<i64>, act: &str, shuffled: bool) {
        let mut inner = self.lock();
        for id in &albums {
            if inner.store.cards.get(id).is_some_and(|c| c.tracks.is_none()) {
                inner.card_errors.remove(id);
                self.queue(&mut inner, Job::Tracks(*id), true);
            }
        }
        inner.deferred.retain(|d| d.act != act || d.albums != albums);
        inner.deferred.push(Deferred { albums, act: act.into(), shuffled, asked: Instant::now() });
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

    pub fn cards(&self, ids: &[i64]) -> Vec<store::Card> {
        let inner = self.lock();
        ids.iter().filter_map(|id| inner.store.cards.get(id).cloned()).collect()
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

    /// What a track decoded as: its row says that from now on, instead of
    /// what the source advertised. The codec stays the source's word.
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
