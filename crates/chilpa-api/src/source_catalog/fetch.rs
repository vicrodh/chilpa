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
    let mut store: Store = std::fs::File::open(file()).ok().and_then(|file| serde_json::from_reader(std::io::BufReader::new(file)).ok()).unwrap_or_default();
    store.index();
    store
}

fn save(store: &Store) -> std::io::Result<()> {
    let path = file();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let temporary = path.with_extension("json.tmp");
    let mut writer = std::io::BufWriter::new(std::fs::File::create(&temporary)?);
    serde_json::to_writer(&mut writer, store).map_err(std::io::Error::other)?;
    std::io::Write::flush(&mut writer)?;
    drop(writer);
    std::fs::rename(&temporary, &path)
}

// I write snapshots only on the catalog thread, in capture order. Readers
// keep using the live store while serialization and disk writes run here.
fn persist(catalog: &Catalog, write: impl FnOnce(&Store) -> std::io::Result<()>) {
    let snapshot = {
        let mut inner = catalog.lock();
        if !inner.dirty { return; }
        let (protected_tracks, protected_cards) = inner.protection();
        let protected_shelves = inner.protected_shelves();
        inner.store.prune_with_shelves(&protected_tracks, &protected_cards, &protected_shelves);
        let live_cards: std::collections::HashSet<_> = inner.store.cards.keys().copied().collect();
        let live_tracks: std::collections::HashSet<_> = inner.store.tracks.keys().copied().collect();
        {
            let Inner { store, entity_revisions, .. } = &mut *inner;
            entity_revisions.retain(|id, revision| revision.retains_current_cache(*id, store));
        }
        let live_images: std::collections::HashSet<_> = inner.store.cards.values().map(|card| card.image.clone())
            .chain(inner.store.tracks.values().map(|track| track.image.clone())).collect();
        inner.art_received.retain(|image| live_images.contains(image));
        inner.images.retain(|image, _| live_images.contains(image));
        inner.card_errors.retain(|id, _| live_cards.contains(id));
        inner.card_retry_at.retain(|id, _| live_cards.contains(id));
        inner.published.cards.retain(|id, _| live_cards.contains(id));
        let live_shelves: std::collections::HashSet<_> = inner.store.shelves.keys().cloned().collect();
        inner.published.tracks.retain(|id, _| live_tracks.contains(id));
        inner.published.shelves.retain(|key, _| live_shelves.contains(key));
        let snapshot = inner.store.snapshot();
        inner.dirty = false;
        snapshot
    };
    if let Err(error) = write(&snapshot) {
        catalog.lock().dirty = true;
        eprintln!("source-catalog: could not save catalog: {error}");
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
        let (tracks, cards) = inner.protection();
        let shelves = inner.protected_shelves();
        inner.store.prune_with_shelves(&tracks, &cards, &shelves);
        inner.dirty = true;
        inner.loaded = true;
    }
    catalog.changed();
    catalog.prefetch_homes();
    let mut pages = Pages::default();
    let mut saved = Instant::now();
    loop {
        let (job, plugins) = next(catalog, &mut saved);
        // A process plugin is asked on a thread of its own: it answers many
        // at once, and nothing of it waits behind a compatibility worker.
        if let Some(natives) = native_of(catalog, &job) {
            let (worker_catalog, worker_job) = (catalog.clone(), job.clone());
            let spawned = std::thread::Builder::new().name("chilpa-catalog-native".into()).spawn(move || {
                let (catalog, job) = (worker_catalog, worker_job);
                let started = Instant::now();
                let outcome = match &job {
                    Job::Shelf(key) => super::native::read_shelf(&catalog, &natives, key),
                    Job::Append(key) => super::pages::append(&catalog, &natives, key),
                    Job::Tracks(id) => super::native::read_tracks(&catalog, &natives, *id),
                    Job::Track(path) => super::native::read_track(&catalog, &natives, path),
                };
                eprintln!("source-catalog: job={job:?} native ok={} elapsed_ms={}", outcome.is_ok(), started.elapsed().as_millis());
                finish(&catalog, &job, outcome);
            });
            if let Err(error) = spawned { finish(catalog, &job, Err(error.to_string())); }
            if saved.elapsed() > SAVE_EVERY { persist(catalog, save); saved = Instant::now(); }
            continue;
        }
        let started = Instant::now();
        let outcome = match &job {
            Job::Shelf(key) => read_shelf(catalog, &plugins, &mut pages, key, 0),
            Job::Tracks(id) => read_tracks(catalog, &plugins, &mut pages, *id),
            // Only a process plugin answers for one track.
            Job::Track(_) | Job::Append(_) => Ok(()),
        };
        eprintln!("source-catalog: job={job:?} ok={} elapsed_ms={}", outcome.is_ok(), started.elapsed().as_millis());
        finish(catalog, &job, outcome);
        if saved.elapsed() > SAVE_EVERY {
            persist(catalog, save);
            saved = Instant::now();
        }
    }
}

/// The job is done: it is no longer running, and what failed says why.
fn finish(catalog: &Catalog, job: &Job, outcome: Result<(), String>) {
    let mut inner = catalog.lock();
    inner.running.remove(job);
    inner.native_running.remove(job);
    catalog.wake.notify_one();
    match (job, outcome) {
        (Job::Shelf(key) | Job::Append(key), Err(e)) => { inner.shelf_errors.insert(key.clone(), e); }
        (Job::Tracks(id), Err(e)) => { inner.card_errors.insert(*id, e); inner.card_retry_at.insert(*id, Instant::now()); }
        (Job::Track(path), Err(_)) => { inner.track_failed_at.insert(path.clone(), Instant::now()); }
        _ => {}
    }
    drop(inner);
    // I publish completion even when cached content is unchanged, so loading and errors settle.
    catalog.publish(true);
}

/// The process plugins, when the job is one of theirs.
fn native_of(catalog: &Catalog, job: &Job) -> Option<Arc<crate::natives::Natives>> {
    let plugin = match job {
        Job::Shelf(key) | Job::Append(key) => split(key).0.to_string(),
        Job::Tracks(id) => catalog.lock().store.cards.get(id)?.plugin.clone(),
        Job::Track(path) => crate::source_opener::of_plugin(path)?.0,
    };
    catalog.natives().filter(|natives| natives.is_native(&plugin))
}

/// Waits for a job and for a host to ask. Writes the disk while idle.
fn next(catalog: &Catalog, saved: &mut Instant) -> (Job, Arc<Mutex<Plugins>>) {
    let mut inner = catalog.lock();
    loop {
        drop(inner);
        catalog.maintain_residency();
        inner = catalog.lock();
        let plugins = catalog.plugins.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if let Some(plugins) = plugins {
            let natives = catalog.natives();
            if let Some(job) = take_runnable(&mut inner, natives.as_deref()) {
                inner.queue.queued.remove(&job);
                inner.running.insert(job.clone());
                return (job, plugins);
            }
        }
        if inner.dirty && saved.elapsed() >= SAVE_EVERY {
            drop(inner);
            persist(catalog, save);
            *saved = Instant::now();
            inner = catalog.lock();
            // I recheck work queued during the write before waiting: its
            // notification may already have arrived while no one was waiting.
            let plugins_ready = catalog.plugins.lock().unwrap_or_else(|e| e.into_inner()).is_some();
            if jobs_waiting(&inner, plugins_ready) { continue; }
        }
        let timeout = if inner.dirty { SAVE_EVERY.saturating_sub(saved.elapsed()) } else { Duration::from_secs(5) };
        inner = catalog.wake.wait_timeout(inner, timeout).map(|(g, _)| g).unwrap_or_else(|e| e.into_inner().0);
    }
}

const NATIVE_JOBS: usize = 4;
const NATIVE_PER_PLUGIN: usize = 2;

fn job_plugin(inner: &Inner, job: &Job) -> Option<String> {
    match job {
        Job::Shelf(key) | Job::Append(key) => Some(split(key).0.to_string()),
        Job::Tracks(id) => inner.store.cards.get(id).map(|card| card.plugin.clone()),
        Job::Track(path) => crate::source_opener::of_plugin(path).map(|(plugin, _)| plugin),
    }
}

fn take_runnable(inner: &mut Inner, natives: Option<&crate::natives::Natives>) -> Option<Job> {
    take_runnable_with(inner, |plugin| natives.is_some_and(|natives| natives.is_native(plugin)))
}

fn take_runnable_with(inner: &mut Inner, is_native: impl Fn(&str) -> bool) -> Option<Job> {
    for urgent in [true, false] {
        let queue = if urgent { &inner.queue.urgent } else { &inner.queue.later };
        let found = queue.iter().position(|job| {
            let Some(plugin) = job_plugin(inner, job) else { return true; };
            if !is_native(&plugin) { return true; }
            let global = if urgent { NATIVE_JOBS } else { NATIVE_JOBS - 1 };
            let per_plugin = if urgent { NATIVE_PER_PLUGIN } else { NATIVE_PER_PLUGIN - 1 };
            inner.native_running.len() < global && inner.native_running.values().filter(|running| **running == plugin).count() < per_plugin
        });
        if let Some(index) = found {
            let job = if urgent { inner.queue.urgent.remove(index)? } else { inner.queue.later.remove(index)? };
            if let Some(plugin) = job_plugin(inner, &job).filter(|plugin| is_native(plugin)) {
                inner.native_running.insert(job.clone(), plugin);
            }
            return Some(job);
        }
    }
    None
}

fn jobs_waiting(inner: &Inner, plugins_ready: bool) -> bool {
    plugins_ready && (!inner.queue.urgent.is_empty() || !inner.queue.later.is_empty())
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
                    let plugin = super::residency::cover_source(&inner, &image);
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
                if super::residency::source_inactive(&inner, &plugin) {
                    inner.queue.covers_queued.remove(&image);
                    continue;
                }
                inner.domains.insert(plugin.clone(), domains.clone());
                domains
            }
        };
        {
            let mut inner = catalog.lock();
            if super::residency::source_inactive(&inner, &plugin) {
                inner.queue.covers_queued.remove(&image);
                continue;
            }
        }
        let kept = crate::pictures::keep_approved(&image, &domains);
        let mut inner = catalog.lock();
        inner.queue.covers_queued.remove(&image);
        if super::residency::source_inactive(&inner, &plugin) { continue; }
        let Some(path) = kept else {
            inner.queue.covers_failed.insert(image, Instant::now());
            continue;
        };
        let changed = receive_cover(&mut inner, image, &path);
        drop(inner);
        if changed { catalog.art_rev.fetch_add(1, std::sync::atomic::Ordering::Relaxed); }
    }
}

// I persist changed card paths; a track-only receipt is transient artwork state.
fn receive_cover(inner: &mut Inner, image: String, path: &str) -> bool {
    let mut changed = false;
    for card in inner.store.cards.values_mut_where(|card| card.image == image && card.art != path) {
        card.art = path.to_string();
        changed = true;
    }
    inner.dirty |= changed;
    inner.art_received.insert(image) || changed
}

#[cfg(test)]
mod persistence_tests {
    use super::*;
    use std::sync::mpsc::channel;

    #[test]
    fn polling_received_artwork_keeps_the_catalog_clean_and_the_payload_shared() {
        let catalog = Catalog::new();
        let image = "https://example.invalid/cover.jpg";
        let id = catalog.lock().store.take_item("fixture", &serde_json::json!({
            "kind":"album","ref":"album:1","title":"Record","image":image}), 1).unwrap();
        let initial = catalog.entity_revision(id);
        {
            let mut inner = catalog.lock();
            assert!(receive_cover(&mut inner, image.into(), "cover.jpg"));
            assert!(inner.dirty);
        }
        let received = catalog.entity_revision(id);
        assert_eq!(initial.0, received.0);
        assert_ne!(initial.1, received.1);
        persist(&catalog, |_| Ok(()));
        let payload = catalog.lock().store.cards.shared(&id).unwrap();
        for _ in 0..32 {
            assert!(!receive_cover(&mut catalog.lock(), image.into(), "cover.jpg"));
            assert_eq!(catalog.entity_revision(id), received);
            assert!(!catalog.lock().dirty);
            persist(&catalog, |_| panic!("received artwork must not rewrite the catalog"));
        }
        assert!(Arc::ptr_eq(&payload, &catalog.lock().store.cards.shared(&id).unwrap()));
        assert!(receive_cover(&mut catalog.lock(), image.into(), "replacement.jpg"));
        assert!(catalog.lock().dirty);
        assert_ne!(catalog.entity_revision(id).1, received.1);
    }

    #[test]
    fn a_received_cover_removed_from_disk_can_queue_and_notify_again() {
        let catalog = Catalog::new();
        let image = "https://example.invalid/removed-cover-for-catalog-retry-test.png";
        let mut inner = catalog.lock();
        inner.art_received.insert(image.into());
        catalog.want_cover(&mut inner, image);
        assert!(inner.queue.covers_queued.contains(image));
        assert_eq!(inner.queue.covers.front().map(String::as_str), Some(image));
        assert!(!inner.art_received.contains(image), "a new download must notify even if its path stays the same");
        assert!(receive_cover(&mut inner, image.into(), "same-path.png"));
        assert!(!inner.dirty, "a transient receipt does not change stored metadata");
    }

    #[test]
    fn track_only_artwork_notifies_once_without_rewriting_persistent_metadata() {
        let mut inner = Inner::default();
        let image = "https://example.invalid/track.jpg";
        let id = inner.store.take_item_track("fixture", &serde_json::json!({"ref":"track:1","image":image}), None, 1).unwrap();
        let payload = inner.store.tracks.shared(&id).unwrap();
        assert!(receive_cover(&mut inner, image.into(), "track.jpg"));
        assert!(!inner.dirty);
        assert!(!receive_cover(&mut inner, image.into(), "track.jpg"));
        assert!(Arc::ptr_eq(&payload, &inner.store.tracks.shared(&id).unwrap()));
    }

    #[test]
    fn queued_tracks_and_their_albums_survive_repeated_saves_above_the_card_limit() {
        let catalog = Catalog::new();
        let (id, album, path) = {
            let mut inner = catalog.lock();
            let cards: Vec<_> = (0..6001).map(|n| inner.store.card_id("guest", &format!("listed:{n}"))).collect();
            inner.store.shelves.insert("guest library".into(), Shelf { cards, ..Default::default() });
            let id = inner.store.take_item_track("guest", &serde_json::json!({
                "ref":"queued", "title":"Queued song", "artist":"Queue artist", "album":"Queue album",
                "album_ref":"queue-album", "duration_ms":248707, "image":"https://example.org/cover.png",
                "track_number":7, "disc_number":2
            }), None, 10).unwrap();
            let track = &inner.store.tracks[&id];
            (id, track.album_id, track.path.clone())
        };
        catalog.protect_tracks([path.clone()].into());
        for _ in 0..4 {
            catalog.lock().dirty = true;
            persist(&catalog, |snapshot| {
                let track = snapshot.track_by_path(&path).expect("a queued track remains in the saved catalog");
                assert_eq!((track.id, track.album_id, track.duration_ms, track.number, track.disc), (id, album, 248707, 7, 2));
                assert_eq!((track.artist.as_str(), track.album.as_str(), track.image.as_str()),
                    ("Queue artist", "Queue album", "https://example.org/cover.png"));
                assert!(snapshot.cards.contains_key(&album), "queue album navigation remains resolvable");
                Ok(())
            });
            assert_eq!(catalog.track_by_path(&path).unwrap().0.duration_ms, 248707, "live rows retain the same queue total");
        }
        catalog.protect_tracks(Default::default());
        catalog.lock().dirty = true;
        persist(&catalog, |snapshot| {
            assert!(!snapshot.cards.contains_key(&album), "closing the last queue releases the unlisted album");
            assert!(snapshot.track_by_path(&path).is_none(), "unreferenced tracks can be evicted again");
            assert_eq!(snapshot.cards.len(), 6001, "referenced library cards retain the existing policy");
            Ok(())
        });
    }

    #[test]
    fn a_mixed_source_queue_keeps_each_sources_metadata_when_opaque_ids_match() {
        let catalog = Catalog::new();
        let expected: Vec<_> = {
            let mut inner = catalog.lock();
            let cards = (0..6001).map(|n| inner.store.card_id("guest", &format!("listed:{n}"))).collect();
            inner.store.shelves.insert("guest library".into(), Shelf { cards, ..Default::default() });
            ["source-a", "source-b", "source-c"].into_iter().enumerate().map(|(index, source)| {
                let duration = 120_000 + index as u64 * 60_000;
                let artist = format!("Artist {index}");
                let image = format!("https://example.org/cover-{index}.png");
                let id = inner.store.take_item_track(source, &serde_json::json!({
                    "ref":"track:1", "title":format!("Song {index}"), "artist":artist,
                    "album":format!("Album {index}"), "album_ref":"album:1", "duration_ms":duration,
                    "image":image, "track_number":index + 1, "disc_number":index + 1
                }), None, 10).unwrap();
                let track = &inner.store.tracks[&id];
                (track.path.clone(), id, track.album_id, source, duration, artist, image)
            }).collect()
        };
        assert_eq!(expected.iter().map(|entry| entry.1).collect::<std::collections::HashSet<_>>().len(), 3);
        assert_eq!(expected.iter().map(|entry| entry.2).collect::<std::collections::HashSet<_>>().len(), 3);
        catalog.protect_tracks(expected.iter().map(|entry| entry.0.clone()).collect());
        for _ in 0..4 {
            catalog.lock().dirty = true;
            persist(&catalog, |snapshot| {
                for (path, id, album, source, duration, artist, image) in &expected {
                    let track = snapshot.track_by_path(path).expect("every source in the queue remains readable");
                    assert_eq!((track.id, track.album_id, track.duration_ms), (*id, *album, *duration));
                    assert_eq!((&track.artist, &track.image), (artist, image));
                    assert_eq!(snapshot.cards[album].plugin, *source);
                }
                Ok(())
            });
            let total: u64 = expected.iter().map(|entry| catalog.track_by_path(&entry.0).unwrap().0.duration_ms).sum();
            assert_eq!(total, 540_000, "mixing sources must not change the queue duration between saves");
        }
        catalog.protect_tracks(expected[1..].iter().map(|entry| entry.0.clone()).collect());
        catalog.lock().dirty = true;
        persist(&catalog, |snapshot| {
            assert!(snapshot.track_by_path(&expected[0].0).is_none(), "only the removed source releases its track");
            assert!(!snapshot.cards.contains_key(&expected[0].2));
            for (path, _, album, source, ..) in &expected[1..] {
                assert!(snapshot.track_by_path(path).is_some());
                assert_eq!(snapshot.cards[album].plugin, *source);
            }
            Ok(())
        });
    }

    #[test]
    fn reads_and_new_work_remain_available_while_a_snapshot_is_written() {
        let catalog = Catalog::new();
        {
            let mut inner = catalog.lock();
            inner.store.shelves.insert("guest root".into(), Shelf { title: "before".into(), ..Shelf::default() });
            inner.dirty = true;
        }
        let (started, writing) = channel();
        let (release, released) = channel();
        let saving = catalog.clone();
        let writer = std::thread::spawn(move || {
            persist(&saving, |snapshot| {
                started.send(snapshot.shelves["guest root"].title.clone()).unwrap();
                released.recv_timeout(Duration::from_secs(2)).unwrap();
                assert_eq!(snapshot.shelves["guest root"].title, "before", "the writer owns a stable snapshot");
                Ok(())
            });
        });
        assert_eq!(writing.recv_timeout(Duration::from_secs(2)).unwrap(), "before");
        let (read, read_back) = channel();
        let reading = catalog.clone();
        let reader = std::thread::spawn(move || {
            let title = reading.stored_shelf("guest root").unwrap().title;
            let mut inner = reading.lock();
            inner.store.shelves.get_mut("guest root").unwrap().title = "after".into();
            inner.dirty = true;
            reading.queue(&mut inner, Job::Shelf("guest later".into()), true);
            read.send(title).unwrap();
        });
        let observed = read_back.recv_timeout(Duration::from_millis(500));
        release.send(()).unwrap();
        writer.join().unwrap();
        reader.join().unwrap();
        assert_eq!(observed.unwrap(), "before", "catalog readers must not wait for disk writes");
        {
            let inner = catalog.lock();
            assert!(inner.dirty, "changes made during persistence still need a write");
            assert!(jobs_waiting(&inner, true), "work queued during persistence must bypass the idle wait");
            assert!(!jobs_waiting(&inner, false), "an unavailable host must not spin the catalog thread");
        }
        persist(&catalog, |snapshot| {
            assert_eq!(snapshot.shelves["guest root"].title, "after");
            Ok(())
        });
        assert!(!catalog.lock().dirty);
    }

    #[test]
    fn persistence_releases_evicted_rows_held_by_entity_observations() {
        let catalog = Catalog::new();
        let (old, recent, weak) = {
            let mut inner = catalog.lock();
            let old = inner.store.card_id("fixture", "list:old");
            let recent = inner.store.card_id("fixture", "list:recent");
            inner.store.shelves.insert("fixture section:playlists".into(), Shelf { cards:vec![old, recent], ..Default::default() });
            for number in 1..=40_000 {
                inner.store.tracks.insert(-number, super::super::store::Track { id:-number, path:format!("track:{number}"), ..Default::default() });
            }
            for (id, first, last, stamp) in [(old, 1, 20_000, 1), (recent, 20_001, 40_000, 2)] {
                let card = inner.store.cards.get_mut(&id).unwrap();
                card.tracks = Some((first..=last).map(|number| -number).collect()); card.read_at = stamp;
            }
            (old, recent, Arc::downgrade(&inner.store.tracks.shared(&-1).unwrap()))
        };
        catalog.publish(false);
        catalog.entity_revision(old);
        catalog.lock().dirty = true;
        persist(&catalog, |_| Ok(()));
        let inner = catalog.lock();
        assert!(inner.store.cards[&old].tracks.is_none());
        assert!(inner.store.cards[&recent].tracks.is_some());
        assert!(!inner.entity_revisions.contains_key(&old));
        assert!(weak.upgrade().is_none(), "neither published nor observed snapshots retain evicted row payloads");
    }

    #[test]
    fn persistence_releases_retired_rows_shared_by_observed_playlists() {
        let catalog = Catalog::new();
        let (first, second, track) = {
            let mut inner = catalog.lock();
            let first = inner.store.card_id("fixture", "list:first");
            let second = inner.store.card_id("fixture", "list:second");
            let track = inner.store.take_item_track("fixture", &serde_json::json!({"kind":"track","ref":"track:shared","title":"Before"}), None, 1).unwrap();
            for id in [first, second] { inner.store.cards.get_mut(&id).unwrap().tracks = Some(vec![track]); }
            (first, second, track)
        };
        for title in ["Before", "After"] {
            catalog.publish(false);
            catalog.entity_revision(first); catalog.entity_revision(second);
            let weak = Arc::downgrade(&catalog.lock().store.tracks.shared(&track).unwrap());
            catalog.lock().store.tracks.get_mut(&track).unwrap().title = title.into();
            catalog.publish(false);
            assert!(weak.upgrade().is_some(), "the observations still hold the retired payload before maintenance");
            catalog.lock().dirty = true;
            persist(&catalog, |_| Ok(()));
            assert_eq!(catalog.lock().store.tracks[&track].title, title);
            assert!(weak.upgrade().is_none(), "overlapping observations must release equal and changed retired payloads");
        }
    }

    #[test]
    fn a_failed_write_keeps_the_latest_live_store_dirty_for_retry() {
        let catalog = Catalog::new();
        catalog.lock().dirty = true;
        persist(&catalog, |_| {
            let mut inner = catalog.lock();
            inner.store.names.insert("guest".into(), "updated".into());
            inner.dirty = true;
            Err(std::io::Error::other("simulated write failure"))
        });
        assert!(catalog.lock().dirty);
        persist(&catalog, |snapshot| {
            assert_eq!(snapshot.names["guest"], "updated");
            Ok(())
        });
        assert!(!catalog.lock().dirty);
        persist(&catalog, |_| panic!("a clean store must not be written again"));
    }
}

#[cfg(test)]
mod scheduler_tests {
    use super::*;

    #[test]
    fn native_work_has_global_and_plugin_caps_and_leaves_room_for_visible_pages() {
        let mut inner = Inner::default();
        for plugin in ["first", "first", "second", "third", "fourth"] {
            let key = format!("{plugin} native:{}", inner.queue.later.len());
            inner.queue.later.push_back(Job::Shelf(key));
        }
        let first = take_runnable_with(&mut inner, |_| true).unwrap();
        assert_eq!(inner.native_running.len(), 1);
        assert!(take_runnable_with(&mut inner, |_| true).is_some());
        assert!(take_runnable_with(&mut inner, |_| true).is_some());
        assert!(take_runnable_with(&mut inner, |_| true).is_none(), "background work reserves one global slot");
        let visible = Job::Shelf("first native:visible".into());
        inner.queue.urgent.push_back(visible.clone());
        assert_eq!(take_runnable_with(&mut inner, |_| true), Some(visible));
        assert_eq!(inner.native_running.len(), NATIVE_JOBS);
        inner.queue.urgent.push_back(Job::Shelf("first native:blocked".into()));
        assert!(take_runnable_with(&mut inner, |_| true).is_none());
        inner.native_running.remove(&first);
        assert!(take_runnable_with(&mut inner, |_| true).is_some());
        assert!(inner.native_running.values().filter(|plugin| *plugin == "first").count() <= NATIVE_PER_PLUGIN);
    }

    #[test]
    fn a_saturated_plugin_does_not_hold_up_another_plugins_visible_page() {
        let mut inner = Inner::default();
        inner.native_running.insert(Job::Shelf("first native:one".into()), "first".into());
        inner.native_running.insert(Job::Shelf("first native:two".into()), "first".into());
        inner.queue.urgent.push_back(Job::Shelf("first native:blocked".into()));
        let ready = Job::Append("second native:home|albums".into());
        inner.queue.urgent.push_back(ready.clone());
        assert_eq!(take_runnable_with(&mut inner, |_| true), Some(ready));
        assert_eq!(inner.queue.urgent.len(), 1);
    }
}

#[cfg(test)]
mod append_failure_tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn identical_success_notifies_completion_without_changing_the_entity_revision() {
        let catalog = Catalog::new();
        let key = "fixture native:home|albums";
        super::super::pages::ingest_document(&catalog, "fixture", "home", &json!({"shelves":[{"id":"albums","items":[
            {"kind":"album","ref":"album:1","title":"One"}]}]}));
        catalog.lock().loaded = true;
        catalog.publish(false);
        let id = catalog.stored_shelf(key).unwrap().cards[0];
        let entity = catalog.entity_revision(id);
        let before = catalog.revisions();
        let job = Job::Shelf(key.into());
        catalog.lock().running.insert(job.clone());
        assert!(catalog.loading(key));
        finish(&catalog, &job, Ok(()));
        assert!(!catalog.loading(key));
        assert_ne!(catalog.revisions(), before, "the UI receives completion even when rows do not change");
        assert_eq!(catalog.entity_revision(id), entity, "an identical completion does not rebuild the album");
    }

    #[test]
    fn a_failed_append_keeps_the_cached_items_and_reports_the_child_error() {
        let catalog = Catalog::new();
        let key = "fixture native:home|albums";
        super::super::pages::ingest_document(&catalog, "fixture", "home", &json!({"shelves":[{"id":"albums","items":[
            {"kind":"album","ref":"album:1","title":"One"}],"more":"next"}]}));
        let before = catalog.stored_shelf(key).unwrap();
        let job = Job::Append(key.into());
        catalog.lock().running.insert(job.clone());
        finish(&catalog, &job, Err("Source failed".into()));
        assert_eq!(catalog.stored_shelf(key).unwrap().cards, before.cards);
        assert_eq!(catalog.stored_shelf(key).unwrap().document["items"], before.document["items"]);
        assert_eq!(catalog.shelf_error(key).as_deref(), Some("Source failed"));
        assert!(!catalog.lock().running.contains(&job));
    }
}
