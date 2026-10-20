//! I fill only missing source genres, without changing the source's stored metadata.
//! One catalog owns one transient worker and at most 60 queued barcode/source pairs.
//! The worker exits after each pass, on hide, or on opt-out; an in-flight HTTP read
//! may finish within its six-second timeout. No idle thread or polling I/O remains.
//! I release the cache when every source opts out; each HTTP body is at most 4 MiB.
//! A write shares the cache snapshot; a concurrent expiry can copy at most one cache.
//! At capacity I discard absent barcodes, using one set bounded by the catalog's cards.
mod cache;
#[cfg(test)]
mod tests;

use super::{store::{Card, CardKind, Store, Track}, Catalog};
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

pub(crate) fn own_genres(metadata: &Value, genre: &str) -> Vec<String> {
    let listed: Vec<_> = metadata["genres"].as_array().into_iter().flatten()
        .filter_map(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect();
    if !listed.is_empty() { return listed; }
    let single = metadata["genre"].as_str().filter(|s| !s.trim().is_empty()).unwrap_or(genre);
    single.split(", ").map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect()
}

impl State {
    pub fn of_card(&self, card: &Card, now: u64) -> Vec<String> {
        let own = own_genres(&card.metadata, &card.genre);
        if !own.is_empty() || card.kind != CardKind::Album || !self.enabled.contains(&card.plugin) { return own; }
        card.metadata["upc"].as_str().and_then(|upc| self.cache.as_ref()?.get(upc, now)).unwrap_or_default().to_vec()
    }

    pub fn card(&self, mut card: Card, now: u64) -> Card {
        if !self.enabled.contains(&card.plugin) { return card; }
        if own_genres(&card.metadata, &card.genre).is_empty() {
            let genres = self.of_card(&card, now);
            if !genres.is_empty() {
                card.genre = genres.join(", ");
                if !card.metadata.is_object() { card.metadata = json!({}); }
                card.metadata["genre"] = json!(card.genre);
                card.metadata["genres"] = json!(genres);
            }
        }
        card
    }

    pub fn track(&self, mut track: Track, store: &Store, now: u64) -> Track {
        if !self.enabled.contains(&track.plugin) { return track; }
        if own_genres(&track.metadata, "").is_empty() {
            if let Some(card) = store.cards.get(&track.album_id).filter(|card| card.plugin == track.plugin) {
                let genres = self.of_card(card, now);
                if !genres.is_empty() {
                    if !track.metadata.is_object() { track.metadata = json!({}); }
                    track.metadata["genre"] = json!(genres.join(", "));
                    track.metadata["genres"] = json!(genres);
                }
            }
        }
        track
    }

    fn pending(&self, store: &Store, now: u64) -> Vec<(String, String)> {
        if !self.shown || self.enabled.is_empty() || self.cache.as_ref().is_some_and(|cache| cache.full()) { return Vec::new(); }
        let mut seen = HashSet::new();
        store.cards.values().filter(|card| card.kind == CardKind::Album && self.enabled.contains(&card.plugin)
            && own_genres(&card.metadata, &card.genre).is_empty()).filter_map(|card| {
                let barcode = card.metadata["upc"].as_str()?;
                let key = musicbrainz::barcode_key(barcode)?;
                if self.cache.as_ref().is_some_and(|cache| cache.get(&key, now).is_some()) || !seen.insert(key) { return None; }
                Some((card.plugin.clone(), barcode.chars().filter(char::is_ascii_digit).collect()))
            }).take(PASS).collect()
    }
}

impl Catalog {
    pub(crate) fn genre_source(&self, plugin: &str, enabled: bool) {
        let mut inner = self.lock();
        let state = &mut inner.genres;
        let changed = if enabled { state.enabled.insert(plugin.to_string()) } else { state.enabled.remove(plugin) };
        if !changed { return; }
        if state.enabled.is_empty() { state.cache = None; state.expires_at = u64::MAX; }
        state.scanned = None;
        state.retry_at = 0;
        state.revision = state.revision.wrapping_add(1);
        drop(inner);
        self.changed();
    }

    pub(crate) fn genre_window(&self, shown: bool) {
        let mut inner = self.lock();
        if inner.genres.shown != shown {
            inner.genres.shown = shown;
            inner.genres.scanned = None;
        }
    }

    pub(crate) fn advance_genres(self: &Arc<Self>) {
        let mut inner = self.lock();
        let state = &mut inner.genres;
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
        if inner.genres.cache.as_ref().is_some_and(|cache| cache.full()) {
            let retained = inner.store.cards.values().filter_map(|card| card.metadata["upc"].as_str().and_then(musicbrainz::barcode_key)).collect();
            if let Some(cache) = inner.genres.cache.as_mut() { Arc::make_mut(cache).retain(&retained); }
            inner.genres.expires_at = inner.genres.cache.as_ref().map_or(u64::MAX, |cache| cache.expiry());
        }
        let policy = chilpa_core::network::current();
        if now < inner.genres.retry_at || (inner.genres.cache.is_some() && (policy.offline || !policy.musicbrainz)) { return; }
        let pending = inner.genres.pending(&inner.store, now);
        inner.genres.scanned = Some(self.revisions().0);
        if pending.is_empty() { return; }
        inner.genres.running = true;
        drop(inner);
        let catalog = self.clone();
        if std::thread::Builder::new().name("chilpa-source-genres".into()).spawn(move || {
            let http = crate::metadata::worker::Network::new();
            catalog.genre_pass(&pending, &http, &chilpa_dirs::cache().join("source-genres.json"), true);
        }).is_err() {
            let mut inner = self.lock();
            inner.genres.running = false;
            inner.genres.scanned = None;
            inner.genres.retry_at = now + RETRY;
        }
    }

    fn genre_active(&self, plugin: &str) -> bool {
        let inner = self.lock();
        inner.genres.shown && inner.genres.enabled.contains(plugin)
    }

    fn genre_allowed(&self, plugin: &str) -> bool {
        let policy = chilpa_core::network::current();
        self.genre_active(plugin) && !policy.offline && policy.musicbrainz
    }

    fn genre_pass(&self, pending: &[(String, String)], http: &dyn Http, path: &Path, paced: bool) {
        if !pending.iter().any(|(plugin, _)| self.genre_active(plugin)) {
            let mut inner = self.lock();
            inner.genres.running = false;
            inner.genres.scanned = None;
            return;
        }
        if self.lock().genres.cache.is_none() {
            let cache = Cache::read(path, super::now());
            let mut inner = self.lock();
            if inner.genres.shown && !inner.genres.enabled.is_empty() {
                if cache.has_genres() {
                    inner.genres.revision = inner.genres.revision.wrapping_add(1);
                    self.changed();
                }
                inner.genres.expires_at = cache.expiry();
                inner.genres.cache = Some(Arc::new(cache));
            }
        }
        let mut failed = false;
        for (plugin, barcode) in pending.iter().take(PASS) {
            if !self.genre_allowed(plugin) { continue; }
            if self.lock().genres.cache.as_ref().is_some_and(|cache| cache.get(barcode, super::now()).is_some() || cache.full()) { continue; }
            let guarded = Guarded { catalog: self, plugin, http };
            let net = if paced { Net::musicbrainz(&guarded) } else { Net::free(&guarded) };
            match musicbrainz::genres_by_barcode(&net, barcode) {
                Ok(genres) if self.genre_allowed(plugin) => {
                    let mut inner = self.lock();
                    let changed = !genres.is_empty();
                    if let Some(cache) = inner.genres.cache.as_mut() {
                        Arc::make_mut(cache).insert(barcode, genres, super::now());
                        let snapshot = cache.clone();
                        inner.genres.expires_at = snapshot.expiry();
                        if changed {
                            inner.genres.revision = inner.genres.revision.wrapping_add(1);
                            self.changed();
                        }
                        drop(inner);
                        if let Err(error) = snapshot.write(path) { eprintln!("source-genres: {error}"); }
                    }
                }
                Err(_) if self.genre_allowed(plugin) => { failed = true; break; }
                _ => {}
            }
        }
        let mut inner = self.lock();
        inner.genres.running = false;
        inner.genres.scanned = None;
        if failed { inner.genres.retry_at = super::now() + RETRY; }
    }
}

/// I recheck after pacing, including retries and the second release request.
struct Guarded<'a> { catalog: &'a Catalog, plugin: &'a str, http: &'a dyn Http }
impl Http for Guarded<'_> {
    fn get(&self, url: &str, accept: &str, timeout: Duration, max_bytes: usize) -> Result<Reply, String> {
        if !self.catalog.genre_allowed(self.plugin) {
            return Ok(Reply { status: 499, headers: Default::default(), body: Vec::new() });
        }
        self.http.get(url, accept, timeout, max_bytes)
    }
}
