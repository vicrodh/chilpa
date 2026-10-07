//! The catalog's threads: one that asks plugins, in order of who waits,
//! and a few that fetch covers.

use super::store::{CardKind, Format, Shelf, Store};
use super::{now, split, Catalog, Inner, Job};
use crate::plugins::Plugins;
use chilpa_plugin::messages::{self, Browse};
use chilpa_plugin::presentation::SourceDocument;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long a page a worker answered is reused without asking again.
const PAGE_FRESH: Duration = Duration::from_secs(300);
/// Pages of a listing read for a shelf, and for a record's tracks.
const SHELF_PAGES: usize = 4;
const TRACK_PAGES: usize = 20;
/// The disk is written at most this often while answers come in.
const SAVE_EVERY: Duration = Duration::from_secs(3);

fn file() -> std::path::PathBuf {
    chilpa_dirs::cache().join("source-catalog.json")
}

fn load() -> Store {
    let mut store: Store = std::fs::read(file()).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default();
    store.index();
    store
}

pub(super) fn save(store: &Store) {
    let Ok(text) = serde_json::to_vec(store) else { return };
    let path = file();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let temporary = path.with_extension("json.tmp");
    if std::fs::write(&temporary, text).is_ok() {
        let _ = std::fs::rename(&temporary, &path);
    }
}

/// Pages a worker answered, by plugin, load, place and cursor.
#[derive(Default)]
struct Pages {
    kept: HashMap<(String, u64, String, Option<String>), (Instant, SourceDocument)>,
    epochs: HashMap<String, u64>,
}

/// One plugin, asked through the shared host.
struct Asker<'a> {
    catalog: &'a Catalog,
    plugins: &'a Arc<Mutex<Plugins>>,
    pages: &'a mut Pages,
    plugin: String,
}

impl Asker<'_> {
    fn epoch(&self) -> u64 {
        self.pages.epochs.get(&self.plugin).copied().unwrap_or_default()
    }

    fn browse(&mut self, at: &str, cursor: Option<String>) -> Result<SourceDocument, String> {
        let key = (self.plugin.clone(), self.epoch(), at.to_string(), cursor.clone());
        if let Some((when, document)) = self.pages.kept.get(&key) {
            if when.elapsed() < PAGE_FRESH {
                return Ok(document.clone());
            }
        }
        let ask = messages::write(&Browse { at: at.into(), cursor: cursor.clone(), text: None, language: "en".into(), picks: BTreeMap::new() });
        let (answer, epoch, domains) = {
            let mut host = self.plugins.lock().map_err(|_| chilpa_i18n::t("the plugin sandbox is not started"))?;
            let answer = host.call_waiting(&self.plugin, messages::op::BROWSE, &ask);
            (answer, host.epoch(&self.plugin), host.worker_domains(&self.plugin).unwrap_or_default())
        };
        {
            let mut inner = self.catalog.lock();
            inner.domains.insert(self.plugin.clone(), domains);
            inner.epochs.insert(self.plugin.clone(), epoch);
        }
        if epoch != self.epoch() {
            // The worker started again: every handle it gave out is gone.
            let had = self.pages.epochs.insert(self.plugin.clone(), epoch).is_some();
            let plugin = self.plugin.clone();
            self.pages.kept.retain(|(p, ..), _| *p != plugin);
            if had {
                self.catalog.forget_handles(&self.plugin);
            }
        }
        let document = messages::read::<SourceDocument>(&answer?)?.clean(cursor.as_deref()).map_err(str::to_string)?;
        let key = (self.plugin.clone(), epoch, at.to_string(), cursor);
        self.pages.kept.insert(key, (Instant::now(), document.clone()));
        Ok(document)
    }

    /// A listing from its first page, as many pages as allowed.
    fn pages(&mut self, at: &str, most: usize) -> Result<Vec<SourceDocument>, String> {
        let mut documents = vec![self.browse(at, None)?];
        while documents.len() < most && documents[0].medium.is_empty() {
            let Some(next) = documents.last().and_then(|d| d.listing.next.clone()) else { break };
            documents.push(self.browse(at, Some(next))?);
        }
        Ok(documents)
    }

    /// Follows a route of the layout from the root, by the titles the
    /// worker gives its folders.
    fn walk(&mut self, path: &[String]) -> Result<String, String> {
        let mut at = String::new();
        for title in path {
            let found = self.pages(&at, SHELF_PAGES)?.into_iter().flat_map(|d| d.listing.entries)
                .find(|entry| &entry.title == title).map(|entry| entry.id);
            at = found.ok_or_else(|| chilpa_i18n::t("this part of the source is not there now"))?;
        }
        Ok(at)
    }
}

pub(super) fn run(catalog: &Arc<Catalog>) {
    let read = load();
    {
        let mut inner = catalog.lock();
        inner.store = read;
        inner.loaded = true;
    }
    catalog.changed();
    let mut pages = Pages::default();
    let mut saved = Instant::now();
    loop {
        let (job, plugins) = next(catalog);
        // A process plugin is asked on a thread of its own: it answers many
        // at once, and nothing of it waits behind a compatibility worker.
        if let Some(natives) = native_of(catalog, &job) {
            let (catalog, job) = (catalog.clone(), job.clone());
            let _ = std::thread::Builder::new().name("chilpa-catalog-native".into()).spawn(move || {
                let started = Instant::now();
                let outcome = match &job {
                    Job::Shelf(key) => super::native::read_shelf(&catalog, &natives, key),
                    Job::Tracks(id) => super::native::read_tracks(&catalog, &natives, *id),
                    Job::Track(path) => super::native::read_track(&catalog, &natives, path),
                };
                eprintln!("source-catalog: job={job:?} native ok={} elapsed_ms={}", outcome.is_ok(), started.elapsed().as_millis());
                drop(finish(&catalog, &job, outcome));
                catalog.changed();
            });
            continue;
        }
        let started = Instant::now();
        let outcome = match &job {
            Job::Shelf(key) => read_shelf(catalog, &plugins, &mut pages, key, 0),
            Job::Tracks(id) => read_tracks(catalog, &plugins, &mut pages, *id),
            // Only a process plugin answers for one track.
            Job::Track(_) => Ok(()),
        };
        eprintln!("source-catalog: job={job:?} ok={} elapsed_ms={}", outcome.is_ok(), started.elapsed().as_millis());
        let mut inner = finish(catalog, &job, outcome);
        let write = inner.dirty && saved.elapsed() > SAVE_EVERY;
        if write {
            inner.dirty = false;
            inner.store.prune();
            save(&inner.store);
            saved = Instant::now();
        }
        drop(inner);
        catalog.changed();
    }
}

/// The job is done: it is no longer running, and what failed says why.
fn finish<'a>(catalog: &'a Catalog, job: &Job, outcome: Result<(), String>) -> std::sync::MutexGuard<'a, Inner> {
    let mut inner = catalog.lock();
    inner.running.remove(job);
    match (job, outcome) {
        (Job::Shelf(key), Err(e)) => { inner.shelf_errors.insert(key.clone(), e); }
        (Job::Tracks(id), Err(e)) => { inner.card_errors.insert(*id, e); inner.card_retry_at.insert(*id, Instant::now()); }
        (Job::Track(path), Err(_)) => { inner.track_failed_at.insert(path.clone(), Instant::now()); }
        _ => {}
    }
    inner
}

/// The process plugins, when the job is one of theirs.
fn native_of(catalog: &Catalog, job: &Job) -> Option<Arc<crate::natives::Natives>> {
    let plugin = match job {
        Job::Shelf(key) => split(key).0.to_string(),
        Job::Tracks(id) => catalog.lock().store.cards.get(id)?.plugin.clone(),
        Job::Track(path) => crate::source_opener::of_plugin(path)?.0,
    };
    catalog.natives().filter(|natives| natives.is_native(&plugin))
}

/// Waits for a job and for a host to ask. Writes the disk while idle.
fn next(catalog: &Catalog) -> (Job, Arc<Mutex<Plugins>>) {
    let mut inner = catalog.lock();
    loop {
        let plugins = catalog.plugins.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if let Some(plugins) = plugins {
            let job = inner.queue.urgent.pop_front().or_else(|| inner.queue.later.pop_front());
            if let Some(job) = job {
                inner.queue.queued.remove(&job);
                inner.running.insert(job.clone());
                return (job, plugins);
            }
        }
        if inner.dirty {
            inner.dirty = false;
            save(&inner.store);
        }
        inner = catalog.wake.wait_timeout(inner, Duration::from_secs(5)).map(|(g, _)| g).unwrap_or_else(|e| e.into_inner().0);
    }
}

fn layout_section(plugins: &Arc<Mutex<Plugins>>, plugin: &str, name: &str) -> Result<crate::source_layout::Section, String> {
    let host = plugins.lock().map_err(|_| chilpa_i18n::t("the plugin sandbox is not started"))?;
    host.layouts.get(plugin).and_then(|l| l.sections.get(name)).cloned().ok_or_else(|| chilpa_i18n::t("this part of the source is not there now"))
}

/// Reading a card's listing can mean reading where the card was seen
/// first; this many levels down, it gives up instead of going in circles.
const DEPTH: u8 = 3;

fn read_shelf(catalog: &Catalog, plugins: &Arc<Mutex<Plugins>>, pages: &mut Pages, key: &str, depth: u8) -> Result<(), String> {
    let (plugin, address) = split(key);
    let mut asker = Asker { catalog, plugins, pages, plugin: plugin.to_string() };
    let (title, documents, membership) = if address == "root" || address.is_empty() {
        (String::new(), asker.pages("", SHELF_PAGES)?, false)
    } else if let Some(name) = address.strip_prefix("section:") {
        let section = layout_section(plugins, plugin, name)?;
        let merged = crate::source_layout::collect(&section, |at, cursor| asker.browse(at, cursor))?;
        (section.title.clone(), vec![merged], true)
    } else if let Some(route) = address.strip_prefix("route:") {
        let (name, index) = route.rsplit_once(':').ok_or_else(|| chilpa_i18n::t("this part of the source is not there now"))?;
        let section = layout_section(plugins, plugin, name)?;
        let path = section.paths.get(index.parse::<usize>().unwrap_or(usize::MAX)).map(|r| r.path.clone()).unwrap_or_default();
        let at = asker.walk(&path)?;
        (path.last().cloned().unwrap_or_default(), asker.pages(&at, SHELF_PAGES)?, false)
    } else if let Some(card) = address.strip_prefix("card:") {
        let id: i64 = card.parse().unwrap_or_default();
        let (handle, title) = handle_of(catalog, plugins, &mut asker, id, depth)?;
        (title, asker.pages(&handle, SHELF_PAGES)?, false)
    } else if let Some(at) = address.strip_prefix("at:") {
        let documents = asker.pages(at, SHELF_PAGES)?;
        (documents[0].listing.title.clone(), documents, false)
    } else {
        return Err(chilpa_i18n::t("this part of the source is not there now"));
    };
    let epoch = asker.epoch();
    let stamp = now();
    let mut inner = catalog.lock();
    let mut shelf = Shelf { title, read_at: stamp, epoch, ..Shelf::default() };
    for document in &documents {
        let read = inner.store.take(plugin, epoch, document, stamp);
        for (entry, id) in &read.entries {
            if let Some(card) = inner.store.cards.get_mut(id) {
                card.seen_in = key.to_string();
            }
            if membership {
                let what = document.presentation.get(entry).and_then(|p| p.metadata.get("membership")).cloned().unwrap_or_default();
                shelf.membership.insert(*id, what);
            }
        }
        shelf.cards.extend(read.cards);
        shelf.tracks.extend(read.tracks);
        shelf.folders.extend(read.folders);
    }
    if shelf.title.is_empty() {
        shelf.title = documents.first().map(|d| d.listing.title.clone()).unwrap_or_default();
    }
    let mut seen = std::collections::HashSet::new();
    shelf.cards.retain(|id| seen.insert(*id));
    inner.store.shelves.insert(key.to_string(), shelf);
    inner.shelf_errors.remove(key);
    inner.dirty = true;
    Ok(())
}

/// A card's handle in the running worker. One that was lost is found by
/// reading again the listing the card was last seen in.
fn handle_of(catalog: &Catalog, plugins: &Arc<Mutex<Plugins>>, asker: &mut Asker, id: i64, depth: u8) -> Result<(String, String), String> {
    let current = |inner: &Inner, epoch: u64| {
        let card = inner.store.cards.get(&id)?;
        card.handle.as_ref().filter(|h| h.epoch == epoch).map(|h| (h.id.clone(), card.title.clone()))
    };
    if let Some(found) = current(&catalog.lock(), asker.epoch()).filter(|_| asker.epoch() > 0) {
        return Ok(found);
    }
    let seen_in = catalog.lock().store.cards.get(&id).map(|c| c.seen_in.clone()).unwrap_or_default();
    if seen_in.is_empty() || split(&seen_in).1.starts_with("at:") || depth >= DEPTH {
        return Err(chilpa_i18n::t("open it again from where it is listed"));
    }
    let mut pages = std::mem::take(asker.pages);
    let outcome = read_shelf(catalog, plugins, &mut pages, &seen_in, depth + 1);
    *asker.pages = pages;
    outcome?;
    current(&catalog.lock(), asker.epoch()).ok_or_else(|| chilpa_i18n::t("the source no longer lists it"))
}

fn read_tracks(catalog: &Catalog, plugins: &Arc<Mutex<Plugins>>, pages: &mut Pages, id: i64) -> Result<(), String> {
    let plugin = catalog.lock().store.cards.get(&id).map(|c| c.plugin.clone()).ok_or_else(|| chilpa_i18n::t("this part of the source is not there now"))?;
    let mut asker = Asker { catalog, plugins, pages, plugin: plugin.clone() };
    let (handle, _) = handle_of(catalog, plugins, &mut asker, id, 0)?;
    let documents = asker.pages(&handle, TRACK_PAGES)?;
    let mut inner = catalog.lock();
    let under = inner.store.cards.get(&id).cloned().unwrap_or_default();
    let mut tracks = Vec::new();
    let mut genre = String::new();
    for document in &documents {
        for entry in &document.listing.entries {
            if entry.kind != chilpa_plugin::messages::EntryKind::Item || entry.live {
                continue;
            }
            let shown = document.presentation.get(&entry.id).cloned().unwrap_or_default();
            if !entry.playable && shown.entity != chilpa_plugin::presentation::Entity::Track {
                continue;
            }
            if genre.is_empty() {
                genre = shown.metadata.get("genre").cloned().unwrap_or_default();
            }
            tracks.push(inner.store.take_track(&plugin, entry, &shown, Some(&under)));
        }
    }
    let best = tracks.iter().filter_map(|t| inner.store.tracks.get(t)?.format.clone())
        .max_by_key(|f: &Format| (f.quality().tier.rank(), f.bits, f.rate));
    let images: Vec<String> = if under.kind == CardKind::Playlist {
        tracks.iter().filter_map(|t| inner.store.tracks.get(t)).map(|t| t.image.clone()).collect()
    } else {
        Vec::new()
    };
    if let Some(card) = inner.store.cards.get_mut(&id) {
        card.tracks = Some(tracks);
        card.read_at = now();
        if card.genre.is_empty() { card.genre = genre; }
        if card.format.is_none() { card.format = best; }
    }
    inner.card_errors.remove(&id);
    inner.dirty = true;
    for image in images {
        catalog.want_cover(&mut inner, &image);
    }
    Ok(())
}

/// Fetches covers, within the domains the plugin was approved for.
pub(super) fn covers(catalog: &Arc<Catalog>) {
    loop {
        let (image, plugin, known) = {
            let mut inner = catalog.lock();
            loop {
                if let Some(image) = inner.queue.covers.pop_front() {
                    let plugin = inner.store.cards.values().find(|c| c.image == image).map(|c| c.plugin.clone())
                        .or_else(|| inner.store.tracks.values().find(|t| t.image == image).map(|t| t.plugin.clone()))
                        .or_else(|| inner.images.get(&image).cloned());
                    let Some(plugin) = plugin else {
                        inner.queue.covers_queued.remove(&image);
                        continue;
                    };
                    let known = inner.domains.get(&plugin).cloned();
                    break (image, plugin, known);
                }
                inner = catalog.covers_wake.wait(inner).unwrap_or_else(|e| e.into_inner());
            }
        };
        // A picture found by a search comes from a plugin the catalog may not
        // have browsed yet: its domains are asked of the host, never guessed.
        let domains = match known {
            Some(domains) => domains,
            None => {
                let learned = catalog.natives().filter(|n| n.is_native(&plugin)).map(|n| n.domains(&plugin)).or_else(|| {
                    let host = catalog.plugins.lock().unwrap_or_else(|e| e.into_inner()).clone();
                    host.and_then(|host| host.lock().ok().and_then(|host| host.worker_domains(&plugin)))
                });
                let mut inner = catalog.lock();
                let Some(domains) = learned else {
                    inner.queue.covers_queued.remove(&image);
                    continue;
                };
                inner.domains.insert(plugin, domains.clone());
                domains
            }
        };
        let kept = crate::pictures::keep_approved(&image, &domains);
        let mut inner = catalog.lock();
        inner.queue.covers_queued.remove(&image);
        let Some(path) = kept else {
            inner.queue.covers_failed.insert(image, Instant::now());
            continue;
        };
        for card in inner.store.cards.values_mut().filter(|c| c.image == image) {
            card.art = path.clone();
        }
        inner.dirty = true;
        drop(inner);
        catalog.art_rev.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}
