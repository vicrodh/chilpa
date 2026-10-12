//! A place's artist scene. MusicBrainz discovery is shared; each selected
//! catalog validates its own page and keeps its own rows, errors and budget.

use chilpa_meta::artist::Origin;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

fn musicbrainz_row(artist: &chilpa_meta::scene::Candidate) -> Value {
    json!({"mbid":artist.id,"title":artist.name,"genres":artist.genres,
        "score":artist.score,"sources":[]})
}

fn source_ids(natives: &crate::natives::Natives) -> Vec<String> {
    let mut ids: Vec<_> = natives.ids().into_iter()
        .filter(|id| natives.searches(id) && natives.has_pages(id) && natives.scene_search_limit(id) > 0)
        .collect();
    // Full catalog scenes precede deliberately small, rate-limited sources.
    ids.sort_by(|a, b| natives.scene_search_limit(b).cmp(&natives.scene_search_limit(a))
        .then_with(|| natives.display_name(a).cmp(&natives.display_name(b))));
    ids
}

fn lane(id: &str, name: &str, artists: Vec<Value>, state: &str, error: &str,
    checked: usize, limit: usize, has_more: bool) -> Value {
    json!({"id":id,"name":name,"artists":artists,"state":state,"error":error,
        "checked":checked,"limit":limit,"hasMore":has_more})
}

struct Current {
    generation: u64,
    cancelled: Arc<AtomicBool>,
    origin: Option<Origin>,
    include_sources: bool,
    searches_used: HashMap<String, usize>,
    document: Value,
}

#[derive(Clone)]
pub(crate) struct Scene(Arc<Mutex<Current>>);

impl Scene {
    pub(crate) fn new() -> Self {
        Self(Arc::new(Mutex::new(Current {
            generation: 0,
            cancelled: Arc::new(AtomicBool::new(false)),
            origin: None,
            include_sources: false,
            searches_used: HashMap::new(),
            document: json!({"state":"idle","artists":[]}),
        })))
    }

    pub(crate) fn json(&self) -> String {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).document.to_string()
    }

    pub(crate) fn close(&self) {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        state.cancelled.store(true, Ordering::Relaxed);
        state.generation = state.generation.wrapping_add(1);
        state.searches_used.clear();
        state.document = json!({"state":"idle","artists":[]});
    }

    pub(crate) fn start(&self, origin: Origin, include_sources: bool,
        natives: Arc<crate::natives::Natives>, catalog: Arc<crate::source_catalog::Catalog>, more: bool) {
        let ids = if include_sources { source_ids(&natives) } else { Vec::new() };
        let (generation, cancelled, previous, offset, searches_used) = {
            let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
            if more && (state.document["state"] != "ready" || state.document["hasMore"] != true) { return; }
            let earlier = state.document.clone();
            let offset = if more { earlier["nextOffset"].as_u64().unwrap_or(0) as usize } else { 0 };
            state.cancelled.store(true, Ordering::Relaxed);
            state.generation = state.generation.wrapping_add(1);
            state.cancelled = Arc::new(AtomicBool::new(false));
            state.origin = Some(origin.clone());
            state.include_sources = include_sources;
            if !more { state.searches_used.clear(); }
            let searches_used = state.searches_used.clone();
            let previous = if more { earlier["sources"].as_array().cloned().unwrap_or_default() } else { Vec::new() };
            state.document = if more {
                let mut document = earlier;
                document["loadingMore"] = json!(true);
                document["hasMore"] = json!(false);
                document["error"] = Value::Null;
                document["progress"] = json!({"phase":"searching","percentage":0,"detail":""});
                document
            } else {
                let sources: Vec<_> = if include_sources {
                    ids.iter().map(|id| lane(id, &natives.display_name(id), Vec::new(), "pending", "", 0,
                        natives.scene_search_limit(id), false)).collect()
                } else {
                    vec![lane("musicbrainz", "MusicBrainz", Vec::new(), "pending", "", 0, 0, false)]
                };
                json!({"state":"loading","loadingMore":false,
                    "sources":sources,"progress":{"phase":"searching","percentage":0,"detail":""},
                    "countryCode":origin.country,"artistName":origin.name,
                    "includeSources":include_sources,"hasMore":false,"nextOffset":0})
            };
            (state.generation, state.cancelled.clone(), previous, offset, searches_used)
        };
        catalog.changed();
        let shared = self.clone();
        let _ = std::thread::Builder::new().name("chilpa-artist-scene".into()).spawn(move || {
            let http = crate::metadata::worker::Network::new();
            let net = chilpa_meta::Net::musicbrainz(&http);
            let report = |percentage: u8, detail: &str| {
                shared.publish(generation, |doc| {
                    doc["progress"] = json!({"phase":"searching","percentage":percentage,"detail":detail});
                }, &catalog);
            };
            let found = chilpa_meta::scene::discover(&net, &origin, offset, 100, &cancelled, &report);
            if cancelled.load(Ordering::Relaxed) { return; }
            match found {
                Ok(page) => {
                    let mut used = searches_used;
                    let mut lanes = Vec::new();
                    if !include_sources {
                        let mut artists = previous.first().and_then(|old| old["artists"].as_array()).cloned().unwrap_or_default();
                        artists.extend(page.artists.iter().map(musicbrainz_row));
                        lanes.push(lane("musicbrainz", "MusicBrainz", artists, "ready", "", page.next_offset, 0, page.has_more));
                    } else {
                        let mut credits = chilpa_meta::scene::identity::CreditsCache::new();
                        for (source_index, id) in ids.iter().enumerate() {
                            if cancelled.load(Ordering::Relaxed) { return; }
                            let earlier = previous.iter().find(|old| old["id"].as_str() == Some(id.as_str()));
                            let mut artists = earlier.and_then(|old| old["artists"].as_array()).cloned().unwrap_or_default();
                            let total_limit = natives.scene_search_limit(id);
                            let remaining = total_limit.saturating_sub(used.get(id).copied().unwrap_or(0));
                            let mut checked = 0usize;
                            let mut failures = 0usize;
                            let mut error = String::new();
                            let source_name = natives.display_name(id);
                            let span = ids.len().max(1) * page.artists.len().max(1);
                            let start_progress = 40 + ((source_index * page.artists.len() * 55 / span) as u8);
                            shared.publish(generation, |doc| {
                                doc["progress"] = json!({"phase":"validating","percentage":start_progress,
                                    "detail":format!("0/{}", page.artists.len()),"source":source_name});
                            }, &catalog);
                            for candidate in page.artists.iter().take(remaining.min(100)) {
                                if cancelled.load(Ordering::Relaxed) { return; }
                                checked += 1;
                                *used.entry(id.clone()).or_default() += 1;
                                match crate::scene_catalog::resolve(&natives, id, candidate, &net, &mut credits) {
                                    Ok(Some(row)) => artists.push(row),
                                    Ok(None) => {},
                                    Err(why) => {
                                        failures += 1;
                                        error = why.to_string();
                                        // Auth, a closed worker and rate limits are
                                        // terminal for this catalog page. The next
                                        // plugin still gets its independent scene.
                                        if crate::scene_catalog::terminal_error(&why) { break; }
                                    }
                                }
                                if checked % 5 == 0 {
                                    let progress = 40 + (((source_index * page.artists.len() + checked) * 55 / span) as u8);
                                    shared.publish(generation, |doc| {
                                        doc["progress"] = json!({"phase":"validating","percentage":progress,
                                            "detail":format!("{checked}/{}", page.artists.len()),
                                            "source":natives.display_name(id)});
                                    }, &catalog);
                                }
                            }
                            let state = if checked > 0 && failures == checked && artists.is_empty() { "error" }
                                else if failures > 0 { "partial" } else { "ready" };
                            if state == "error" { used.insert(id.clone(), total_limit); }
                            let consumed = used.get(id).copied().unwrap_or(0);
                            lanes.push(lane(id, &natives.display_name(id), artists, state, &error,
                                consumed, total_limit, page.has_more && consumed < total_limit));
                        }
                    }
                    if ids.is_empty() && include_sources {
                        shared.publish(generation, |doc| {
                            *doc = json!({"state":"error","sources":[],
                                "error":"No searchable plugins are available for this scene."});
                        }, &catalog);
                        return;
                    }
                    let has_more = page.has_more && (!include_sources || ids.iter().any(|id|
                        natives.scene_search_limit(id) > used.get(id).copied().unwrap_or(0)));
                    {
                        let mut state = shared.0.lock().unwrap_or_else(|e| e.into_inner());
                        if state.generation != generation { return; }
                        state.searches_used = used;
                    }
                    shared.publish(generation, |doc| {
                        *doc = json!({"state":"ready","sources":lanes,"sceneLabel":page.label,
                            "genreSummary":page.genre_summary,"seedGenres":page.seed_genres,
                            "countryCode":origin.country,"hasMore":has_more,
                            "nextOffset":page.next_offset,
                            "artistName":origin.name,
                            "totalCandidates":page.total,"includeSources":include_sources,
                            "progress":{"phase":"done","percentage":100,"detail":"done"}});
                    }, &catalog);
                }
                Err(error) => shared.publish(generation, |doc| {
                    let message = crate::metadata::failure_message(chilpa_meta::Provider::MusicBrainz, &error);
                    if more {
                        doc["loadingMore"] = json!(false);
                        doc["error"] = json!(message);
                        doc["hasMore"] = json!(true);
                    } else {
                        *doc = json!({"state":"error","sources":[],"error":message});
                    }
                }, &catalog),
            }
        });
    }

    fn publish(&self, generation: u64, update: impl FnOnce(&mut Value), catalog: &crate::source_catalog::Catalog) {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if state.generation != generation { return; }
        update(&mut state.document);
        drop(state);
        catalog.changed();
    }

    pub(crate) fn again(&self) -> Option<(Origin, bool)> {
        let state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        state.origin.clone().map(|origin| (origin, state.include_sources))
    }
}

impl crate::Chilpa {
    pub fn artist_scene_json(&self) -> String { self.artist_scene.json() }

    pub fn artist_scene_open(&mut self, name: &str, include_sources: bool) {
        if !self.musicbrainz_enabled() { self.artist_scene.close(); return; }
        let known = self.artist_network.of(name, &[], true);
        if let Some(origin) = known.origin.filter(|o| !o.place.is_empty()) {
            self.artist_scene.start(origin, include_sources, self.natives.clone(), self.catalog.clone(), false);
        }
    }

    pub fn artist_scene_more(&mut self) {
        if !self.musicbrainz_enabled() { self.artist_scene.close(); return; }
        if let Some((origin, include)) = self.artist_scene.again() {
            self.artist_scene.start(origin, include, self.natives.clone(), self.catalog.clone(), true);
        }
    }

    pub fn artist_scene_retry(&mut self) {
        if !self.musicbrainz_enabled() { self.artist_scene.close(); return; }
        if let Some((origin, include)) = self.artist_scene.again() {
            self.artist_scene.start(origin, include, self.natives.clone(), self.catalog.clone(), false);
        }
    }

    pub fn artist_scene_close(&mut self) { self.artist_scene.close(); }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn musicbrainz_only_scene_keeps_candidates_without_catalog_results() {
        let artist = chilpa_meta::scene::Candidate {
            id: "mbid".into(), name: "MusicBrainz name".into(), genres: vec!["rock".into()], score: 42,
        };
        assert_eq!(musicbrainz_row(&artist)["title"], "MusicBrainz name");
        assert_eq!(musicbrainz_row(&artist)["sources"], json!([]));
    }
}
