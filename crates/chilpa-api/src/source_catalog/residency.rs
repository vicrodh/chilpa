//! I archive disabled sources before releasing their unrooted payloads.

use super::{store::{ColdSource, Store}, Catalog, Inner, Job};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Default)]
pub(super) struct State {
    sources: BTreeMap<String, bool>,
    revision: u64,
    pending: bool,
    retry_after: Option<Instant>,
    unavailable_archives: HashSet<String>,
}

impl State {
    pub(super) fn changed(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.pending = true;
        self.retry_after = None;
    }

    pub(super) fn forget(&mut self, plugin: &str) {
        self.sources.remove(plugin);
        self.unavailable_archives.remove(plugin);
        self.changed();
    }
}

fn job_source<'a>(inner: &'a Inner, job: &'a Job) -> Option<&'a str> {
    match job {
        Job::Shelf(key) | Job::Append(key) => Some(super::split(key).0),
        Job::Tracks(id) => inner.store.cards.get(id).map(|card| card.plugin.as_str())
            .or_else(|| inner.store.cold_sources.iter().find(|(_, source)| source.cards.contains_key(id)).map(|(plugin, _)| plugin.as_str())),
        Job::Track(_) => None,
    }
}

pub(super) fn blocked(inner: &Inner, job: &Job) -> bool {
    let owned = match job { Job::Track(path) => crate::source_opener::of_plugin(path).map(|(plugin, _)| plugin), _ => None };
    let plugin = owned.as_deref().or_else(|| job_source(inner, job));
    plugin.is_some_and(|plugin| source_inactive(inner, plugin))
}

pub(super) fn source_inactive(inner: &Inner, plugin: &str) -> bool {
    inner.residency.sources.get(plugin) == Some(&false) || (inner.store.cold_sources.contains_key(plugin)
        && !(inner.residency.sources.get(plugin) == Some(&true) && inner.residency.unavailable_archives.contains(plugin)))
}

/// I use an active owner when two sources share the same artwork address.
pub(super) fn cover_source(inner: &Inner, image: &str) -> Option<String> {
    inner.store.cards.values().filter(|card| card.image == image).map(|card| &card.plugin)
        .chain(inner.store.tracks.values().filter(|track| track.image == image).map(|track| &track.plugin))
        .chain(inner.images.get(image)).find(|plugin| !source_inactive(inner, plugin)).cloned()
}

fn belongs(inner: &Inner, job: &Job, plugin: &str) -> bool {
    match job {
        Job::Track(path) => crate::source_opener::of_plugin(path).is_some_and(|(source, _)| source == plugin),
        _ => job_source(inner, job) == Some(plugin),
    }
}

fn archive_path(root: &Path, plugin: &str) -> PathBuf {
    use sha2::{Digest, Sha256};
    root.join(format!("{:x}.json", Sha256::digest(plugin.as_bytes())))
}

fn read(root: &Path, plugin: &str) -> std::io::Result<Store> {
    let file = std::fs::File::open(archive_path(root, plugin))?;
    let mut source: Store = serde_json::from_reader(std::io::BufReader::new(file)).map_err(std::io::Error::other)?;
    if source.cards.values().any(|card| card.plugin != plugin) || source.tracks.values().any(|track| track.plugin != plugin)
        || source.shelves.keys().any(|key| super::split(key).0 != plugin)
        || source.names.keys().chain(source.drawn.keys()).any(|id| id != plugin) || !source.cold_sources.is_empty() {
        return Err(std::io::Error::other("Archived catalog contains another source"));
    }
    source.index();
    Ok(source)
}

fn write(root: &Path, plugin: &str, source: &Store) -> std::io::Result<()> {
    use std::io::Write;
    std::fs::create_dir_all(root)?;
    let mut temporary = tempfile::NamedTempFile::new_in(root)?;
    {
        let mut writer = std::io::BufWriter::new(temporary.as_file_mut());
        serde_json::to_writer(&mut writer, source).map_err(std::io::Error::other)?;
        writer.flush()?;
    }
    temporary.as_file().sync_all()?;
    temporary.persist(archive_path(root, plugin)).map_err(|error| error.error)?;
    Ok(())
}

fn roots(inner: &Inner) -> (HashSet<String>, HashSet<i64>, HashSet<String>) {
    let (tracks, mut cards) = inner.protection();
    let identities: HashSet<_> = inner.navigation.identities.iter().cloned()
        .chain(inner.selections.values().flat_map(|selection| selection.cards.iter().cloned())).collect();
    for (plugin, archived) in &inner.store.cold_sources {
        cards.extend(archived.cards.iter().filter(|(_, identity)| identities.contains(&(plugin.clone(), (*identity).clone()))).map(|(id, _)| *id));
    }
    (tracks, cards, inner.protected_shelves())
}

fn requested(inner: &Inner, plugin: &str) -> bool {
    let (tracks, cards, shelves) = roots(inner);
    let Some(archived) = inner.store.cold_sources.get(plugin) else { return false; };
    cards.iter().any(|id| archived.cards.contains_key(id) && inner.store.cards.get(id).is_none_or(|card| card.tracks.is_none()))
        || tracks.iter().any(|path| crate::source_opener::of_plugin(path).is_some_and(|(source, _)| source == plugin) && inner.store.track_by_path(path).is_none())
        || shelves.iter().any(|key| super::split(key).0 == plugin && !inner.store.shelves.contains_key(key))
}

impl Catalog {
    fn rebuild_requested(&self, inner: &mut Inner, plugin: &str) -> bool {
        let (_, roots, _) = roots(inner);
        let requested: Vec<_> = inner.store.cold_sources.get(plugin).into_iter().flat_map(|source| {
            source.cards.iter().filter(|(id, _)| roots.contains(*id)).filter_map(move |(id, reference)| {
                let summary = source.playlists.iter().find(|row| row.id == *id);
                let kind = if summary.is_some() { "playlist" } else {
                    let (kind, suffix) = reference.split_once(':')?;
                    if !matches!(kind, "album" | "playlist") || suffix.is_empty() { return None; }
                    kind
                };
                Some((*id, reference.clone(), kind.to_string(), summary.map(|row| row.title.clone()).unwrap_or_default()))
            })
        }).collect();
        let mut changed = false;
        for (id, reference, kind, title) in requested {
            if !inner.store.cards.contains_key(&id) {
                inner.store.take_item(plugin, &serde_json::json!({"kind":kind,"ref":reference,"title":title}), super::now());
                inner.card_errors.remove(&id);
                inner.dirty = true;
                changed = true;
            }
            if inner.store.cards.get(&id).is_some_and(|card| card.tracks.is_none()) { self.queue(inner, Job::Tracks(id), true); }
        }
        changed
    }

    /// I enqueue only registry entries that are known. An absent or not-yet
    /// scanned source never becomes disabled by inference.
    pub(crate) fn set_source_states(&self, states: impl IntoIterator<Item = (String, bool)>) {
        let mut inner = self.lock();
        for (plugin, enabled) in states {
            if !enabled { inner.domains.remove(&plugin); }
            if enabled && inner.residency.unavailable_archives.remove(&plugin) { inner.residency.changed(); }
            if inner.residency.sources.insert(plugin, enabled) != Some(enabled) { inner.residency.changed(); }
        }
        let cancelled: HashSet<_> = inner.queue.queued.iter().filter(|job| blocked(&inner, job)).cloned().collect();
        inner.queue.urgent.retain(|job| !cancelled.contains(job));
        inner.queue.later.retain(|job| !cancelled.contains(job));
        inner.queue.queued.retain(|job| !cancelled.contains(job));
        drop(inner);
        self.wake.notify_one();
    }

    /// I call this only from the catalog worker, never from a UI callback.
    pub(super) fn maintain_residency(&self) {
        let root = chilpa_dirs::cache().join("source-catalog-cold");
        self.maintain_residency_with(&root, write);
    }

    fn maintain_residency_with(&self, root: &Path, mut save: impl FnMut(&Path, &str, &Store) -> std::io::Result<()>) {
        let plugins: BTreeSet<_> = {
            let mut inner = self.lock();
            if !inner.loaded || !inner.residency.pending || inner.residency.retry_after.is_some_and(|after| after > Instant::now()) { return; }
            inner.residency.pending = false;
            inner.residency.sources.keys().chain(inner.store.cold_sources.keys()).cloned().collect()
        };
        for plugin in plugins {
            if let Err(error) = self.maintain_source(root, &plugin, &mut save) {
                eprintln!("source-catalog: could not change source residency: {error}");
                let mut inner = self.lock();
                inner.residency.pending = true;
                inner.residency.retry_after = Some(Instant::now() + Duration::from_secs(5));
            }
        }
    }

    fn maintain_source(&self, root: &Path, plugin: &str, save: &mut impl FnMut(&Path, &str, &Store) -> std::io::Result<()>) -> std::io::Result<()> {
        let (resident, revision, enabled, archived, hydrate) = {
            let mut inner = self.lock();
            let enabled = inner.residency.sources.get(plugin).copied();
            let archived = inner.store.cold_sources.contains_key(plugin);
            if enabled != Some(false) && !archived { return Ok(()); }
            if enabled == Some(true) && inner.residency.unavailable_archives.contains(plugin) {
                let changed = self.rebuild_requested(&mut inner, plugin);
                drop(inner);
                if changed { self.changed(); }
                return Ok(());
            }
            if inner.running.iter().any(|job| belongs(&inner, job, plugin)) {
                inner.residency.pending = true;
                return Ok(());
            }
            let resident = inner.store.source_snapshot(plugin);
            let hydrate = archived && requested(&inner, plugin);
            if archived && enabled != Some(true) && !hydrate {
                let (tracks, cards, shelves) = roots(&inner);
                let mut kept = resident.snapshot();
                kept.retain_source_roots(plugin, &tracks, &cards, &shelves);
                if kept.cards.len() == resident.cards.len() && kept.tracks.len() == resident.tracks.len() && kept.shelves.len() == resident.shelves.len() { return Ok(()); }
            }
            if !archived && resident.cards.is_empty() && resident.tracks.is_empty() && resident.shelves.is_empty() { return Ok(()); }
            (resident, inner.residency.revision, enabled, archived, hydrate)
        };
        let mut complete = resident.snapshot();
        if archived {
            match read(root, plugin) {
                Ok(source) => complete.merge_source(&source)?,
                Err(error) if enabled == Some(true) => {
                    let mut inner = self.lock();
                    if inner.residency.revision != revision { inner.residency.pending = true; return Ok(()); }
                    inner.residency.unavailable_archives.insert(plugin.to_string());
                    self.rebuild_requested(&mut inner, plugin);
                    drop(inner);
                    eprintln!("source-catalog: the enabled source's archive is unavailable; rebuilding from the source: {error}");
                    self.changed();
                    self.prefetch_homes();
                    return Ok(());
                }
                Err(error) => return Err(error),
            }
        }
        if enabled != Some(true) { save(root, plugin, &complete)?; }
        let mut inner = self.lock();
        if inner.residency.revision != revision || !inner.store.same_source(&resident, plugin)
            || inner.running.iter().any(|job| belongs(&inner, job, plugin)) {
            inner.residency.pending = true;
            return Ok(());
        }
        if enabled == Some(true) || hydrate { inner.store.merge_source(&complete)?; }
        if enabled == Some(true) {
            inner.store.cold_sources.remove(plugin);
        } else {
            inner.store.cold_sources.insert(plugin.to_string(), ColdSource {
                cards: complete.cards.iter().map(|(id, card)| (*id, card.identity.clone())).collect(),
                tracks: complete.tracks.iter().filter_map(|(id, track)| crate::source_opener::of_plugin(&track.path).map(|(_, reference)| (*id, reference))).collect(),
                playlists: complete.playlist_summaries(),
            });
            let (tracks, cards, shelves) = roots(&inner);
            inner.store.retain_source_roots(plugin, &tracks, &cards, &shelves);
        }
        inner.store.index();
        let Inner { store, entity_revisions, .. } = &mut *inner;
        entity_revisions.retain(|id, revision| revision.retains_current_cache(*id, store));
        inner.published = inner.store.snapshot();
        let images: HashSet<_> = inner.store.cards.values().map(|card| card.image.clone()).chain(inner.store.tracks.values().map(|track| track.image.clone())).collect();
        inner.images.retain(|image, _| images.contains(image));
        inner.art_received.retain(|image| images.contains(image));
        inner.queue.covers.retain(|image| images.contains(image));
        inner.queue.covers_queued.retain(|image| images.contains(image));
        inner.dirty = true;
        drop(inner);
        self.changed();
        Ok(())
    }
}

#[cfg(test)]
mod tests;
