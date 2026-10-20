//! I fill only missing source composers, without changing the source's stored metadata.
//! One catalog owns one transient worker and at most 60 queued ISRC/source pairs.
//! The worker exits after each pass, on hide, or on opt-out; an in-flight HTTP read
//! may finish within its six-second timeout. No idle thread or polling I/O remains.
//! I release the cache when every source opts out; each HTTP body is at most 4 MiB.
//! A write shares the cache snapshot; a concurrent expiry can copy at most one cache.
//! Work credits live only for a pass, capped at 512 works and 2 MiB.
//! At capacity I discard absent ISRCs; the temporary set is bounded by stored tracks.
mod cache;
#[cfg(test)]
mod tests;

use super::{store::{Store, Track}, Catalog};
use cache::Cache;
use chilpa_meta::{musicbrainz, Http, Net, Reply};
use serde_json::{json, Value};
use std::{collections::HashSet, path::Path, sync::Arc, time::Duration};

const PASS: usize = 60;
const RETRY: u64 = 300;

pub(super) struct State {
    enabled: HashSet<String>,
    cache: Option<Arc<Cache>>,
    shown: bool,
    running: bool,
    scanned: Option<i32>,
    retry_at: u64,
    expires_at: u64,
    pub revision: u64,
}

impl Default for State {
    fn default() -> Self {
        Self { enabled: HashSet::new(), cache: None, shown: true, running: false, scanned: None, retry_at: 0, expires_at: u64::MAX, revision: 0 }
    }
}

pub(crate) fn own_composers(metadata: &Value) -> Vec<String> {
    let mut names = super::library_index::names(metadata, "composers", "composer");
    if names.is_empty() {
        if let Some(name) = metadata["composer"].as_str().map(str::trim).filter(|s| !s.is_empty()) { names.push(name.to_string()); }
    }
    names
}

impl State {
    pub fn of_track(&self, track: &Track, now: u64) -> Vec<String> {
        let own = own_composers(&track.metadata);
        if !own.is_empty() || !self.enabled.contains(&track.plugin) { return own; }
        track.metadata["isrc"].as_str().and_then(|isrc| self.cache.as_ref()?.get(isrc, now)).unwrap_or_default().to_vec()
    }

    pub fn track(&self, mut track: Track, now: u64) -> Track {
        if !self.enabled.contains(&track.plugin) { return track; }
        if own_composers(&track.metadata).is_empty() {
            let composers = self.of_track(&track, now);
            if !composers.is_empty() {
                if !track.metadata.is_object() { track.metadata = json!({}); }
                track.metadata["composer"] = json!(composers.join(", "));
                track.metadata["composers"] = json!(composers);
            }
        }
        track
    }

    fn pending(&self, store: &Store, now: u64) -> Vec<(String, String)> {
        if !self.shown || self.enabled.is_empty() || self.cache.as_ref().is_some_and(|cache| cache.full()) { return Vec::new(); }
        let mut seen = HashSet::new();
        store.tracks.values().filter(|track| self.enabled.contains(&track.plugin)
            && own_composers(&track.metadata).is_empty()).filter_map(|track| {
                let key = musicbrainz::isrc_key(track.metadata["isrc"].as_str()?)?;
                if self.cache.as_ref().is_some_and(|cache| cache.get(&key, now).is_some()) || !seen.insert(key.clone()) { return None; }
                Some((track.plugin.clone(), key))
            }).take(PASS).collect()
    }
}

impl Catalog {
    pub(crate) fn composer_source(&self, plugin: &str, enabled: bool) {
        let mut inner = self.lock();
        let state = &mut inner.composers;
        let changed = if enabled { state.enabled.insert(plugin.to_string()) } else { state.enabled.remove(plugin) };
        if !changed { return; }
        if state.enabled.is_empty() { state.cache = None; state.expires_at = u64::MAX; }
        state.scanned = None;
        state.retry_at = 0;
        state.revision = state.revision.wrapping_add(1);
        drop(inner);
        self.changed();
    }

    pub(crate) fn composer_window(&self, shown: bool) {
        let mut inner = self.lock();
        if inner.composers.shown != shown {
            inner.composers.shown = shown;
            inner.composers.scanned = None;
        }
    }

    pub(crate) fn advance_composers(self: &Arc<Self>) {
        let mut inner = self.lock();
        let state = &mut inner.composers;
        let now = super::now();
        if !state.shown || state.enabled.is_empty() || state.running { return; }
        let revision = self.revisions().0;
        if state.scanned == Some(revision) && now < state.expires_at { return; }
        if now >= state.expires_at {
            if state.cache.as_mut().is_some_and(|cache| Arc::make_mut(cache).expire(now)) {
                state.revision = state.revision.wrapping_add(1);
                self.changed();
            }
            state.expires_at = state.cache.as_ref().map_or(u64::MAX, |cache| cache.expiry());
        }
        if inner.composers.cache.as_ref().is_some_and(|cache| cache.full()) {
            let retained = inner.store.tracks.values().filter_map(|track| track.metadata["isrc"].as_str().and_then(musicbrainz::isrc_key)).collect();
            if let Some(cache) = inner.composers.cache.as_mut() { Arc::make_mut(cache).retain(&retained); }
            inner.composers.expires_at = inner.composers.cache.as_ref().map_or(u64::MAX, |cache| cache.expiry());
        }
        let policy = chilpa_core::network::current();
        if now < inner.composers.retry_at || (inner.composers.cache.is_some() && (policy.offline || !policy.musicbrainz)) { return; }
        let pending = inner.composers.pending(&inner.store, now);
        inner.composers.scanned = Some(self.revisions().0);
        if pending.is_empty() { return; }
        inner.composers.running = true;
        drop(inner);
        let catalog = self.clone();
        if std::thread::Builder::new().name("chilpa-source-composers".into()).spawn(move || {
            let http = crate::metadata::worker::Network::new();
            catalog.composer_pass(&pending, &http, &chilpa_dirs::cache().join("source-composers.json"), true);
        }).is_err() {
            let mut inner = self.lock();
            inner.composers.running = false;
            inner.composers.scanned = None;
            inner.composers.retry_at = now + RETRY;
        }
    }

    fn composer_active(&self, plugin: &str) -> bool {
        let inner = self.lock();
        inner.composers.shown && inner.composers.enabled.contains(plugin)
    }

    fn composer_allowed(&self, plugin: &str) -> bool {
        let policy = chilpa_core::network::current();
        self.composer_active(plugin) && !policy.offline && policy.musicbrainz
    }

    fn composer_pass(&self, pending: &[(String, String)], http: &dyn Http, path: &Path, paced: bool) {
        if !pending.iter().any(|(plugin, _)| self.composer_active(plugin)) {
            let mut inner = self.lock();
            inner.composers.running = false;
            inner.composers.scanned = None;
            return;
        }
        if self.lock().composers.cache.is_none() {
            let cache = Cache::read(path, super::now());
            let mut inner = self.lock();
            if inner.composers.shown && !inner.composers.enabled.is_empty() {
                if cache.has_composers() {
                    inner.composers.revision = inner.composers.revision.wrapping_add(1);
                    self.changed();
                }
                inner.composers.expires_at = cache.expiry();
                inner.composers.cache = Some(Arc::new(cache));
            }
        }
        let mut works = musicbrainz::WorkCache::default();
        let mut failed = false;
        for (plugin, isrc) in pending.iter().take(PASS) {
            if !self.composer_allowed(plugin) { continue; }
            if self.lock().composers.cache.as_ref().is_some_and(|cache| cache.get(isrc, super::now()).is_some() || cache.full()) { continue; }
            let guarded = Guarded { catalog: self, plugin, http };
            let net = if paced { Net::musicbrainz(&guarded) } else { Net::free(&guarded) };
            match musicbrainz::composers_by_isrc(&net, isrc, &mut works) {
                Ok(composers) if self.composer_allowed(plugin) => {
                    let mut inner = self.lock();
                    let changed = !composers.is_empty();
                    if let Some(cache) = inner.composers.cache.as_mut() {
                        Arc::make_mut(cache).insert(isrc, composers, super::now());
                        let snapshot = cache.clone();
                        inner.composers.expires_at = snapshot.expiry();
                        if changed {
                            inner.composers.revision = inner.composers.revision.wrapping_add(1);
                            self.changed();
                        }
                        drop(inner);
                        if let Err(error) = snapshot.write(path) { eprintln!("source-composers: {error}"); }
                    }
                }
                Err(_) if self.composer_allowed(plugin) => { failed = true; break; }
                _ => {}
            }
        }
        let mut inner = self.lock();
        inner.composers.running = false;
        inner.composers.scanned = None;
        if failed { inner.composers.retry_at = super::now() + RETRY; }
    }
}

/// I recheck after pacing, including retries and each recording or work request.
struct Guarded<'a> { catalog: &'a Catalog, plugin: &'a str, http: &'a dyn Http }
impl Http for Guarded<'_> {
    fn get(&self, url: &str, accept: &str, timeout: Duration, max_bytes: usize) -> Result<Reply, String> {
        if !self.catalog.composer_allowed(self.plugin) {
            return Ok(Reply { status: 499, headers: Default::default(), body: Vec::new() });
        }
        self.http.get(url, accept, timeout, max_bytes)
    }
}
