//! A place's artist scene. Catalog availability is verified before a scene
//! with plugin results is shown.

use chilpa_meta::artist::Origin;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn scene_row(artist: &chilpa_meta::scene::Candidate, sources: Vec<Value>, include_sources: bool) -> Option<Value> {
    // The opt-in catalog scene follows the reference: only a verified artist
    // receives a card. The MusicBrainz-only choice keeps discovery useful.
    if include_sources && sources.is_empty() { return None; }
    let title = sources.first().and_then(|source| source["title"].as_str())
        .unwrap_or(&artist.name);
    Some(json!({"mbid":artist.id,"title":title,"genres":artist.genres,
        "score":artist.score,"sources":sources}))
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
            let previous = if more { earlier["artists"].as_array().cloned().unwrap_or_default() } else { Vec::new() };
            state.document = if more {
                let mut document = earlier;
                document["loadingMore"] = json!(true);
                document["hasMore"] = json!(false);
                document["error"] = Value::Null;
                document["progress"] = json!({"percentage":0,"detail":""});
                document
            } else {
                json!({"state":"loading","loadingMore":false,
                    "artists":[],"progress":{"percentage":0,"detail":""},
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
                    doc["progress"] = json!({"percentage":percentage,"detail":detail});
                }, &catalog);
            };
            let found = chilpa_meta::scene::discover(&net, &origin, offset, 100, &cancelled, &report);
            if cancelled.load(Ordering::Relaxed) { return; }
            match found {
                Ok(page) => {
                    let mut rows = previous;
                    let mut source_ids: Vec<_> = if include_sources { natives.ids().into_iter().filter(|id| natives.searches(id) && natives.has_pages(id)).collect() } else { Vec::new() };
                    source_ids.sort();
                    let mut used = searches_used;
                    let mut budgets: Vec<_> = source_ids.iter().map(|id| {
                        (id.clone(), natives.scene_search_limit(id).saturating_sub(used.get(id).copied().unwrap_or(0)))
                    }).collect();
                    let mut attempted = 0usize;
                    let mut failures = 0usize;
                    for (at, artist) in page.artists.iter().enumerate() {
                        if cancelled.load(Ordering::Relaxed) { return; }
                        let mut sources = Vec::new();
                        for (id, budget) in &mut budgets {
                            if *budget == 0 { continue; }
                            *budget -= 1;
                            *used.entry(id.clone()).or_default() += 1;
                            attempted += 1;
                            let result = natives.call(id, "search", json!({"text":artist.name,"limit":10}), Duration::from_secs(12));
                            let Ok(answer) = result else {
                                *budget = 0;
                                used.insert(id.clone(), natives.scene_search_limit(id));
                                failures += 1;
                                continue;
                            };
                            let matches = answer["artists"].as_array().into_iter().flatten()
                                .filter(|item| item["title"].as_str().is_some_and(|name| name.trim().eq_ignore_ascii_case(&artist.name))
                                    && item["ref"].as_str().is_some_and(|reference| reference.starts_with("artist:")))
                                .collect::<Vec<_>>();
                            // A same-name search result is only provisional.
                            // Two exact names are ambiguous unless the source
                            // itself supplies the matching identifier.
                            let exact = matches.iter().copied().find(|item| item["mbid"] == artist.id)
                                .or_else(|| (matches.len() == 1).then(|| matches[0]));
                            if let Some(item) = exact {
                                sources.push(json!({"plugin":id,"name":natives.display_name(id),
                                    "ref":item["ref"],"title":item["title"],
                                    "image":item["image"],"albums":item["albums"]}));
                            }
                        }
                        if let Some(row) = scene_row(artist, sources, include_sources) { rows.push(row); }
                        if at % 5 == 0 { report(40 + (at * 55 / page.artists.len().max(1)) as u8, &format!("{}/{}", at, page.artists.len())); }
                    }
                    if include_sources && (attempted == 0 || attempted == failures) && !page.artists.is_empty() {
                        let message = if attempted == 0 { "No searchable plugins are available for this scene." }
                            else { "The selected plugin catalogs could not be reached." };
                        shared.publish(generation, |doc| {
                            if more {
                                doc["loadingMore"] = json!(false);
                                doc["hasMore"] = json!(true);
                                doc["error"] = json!(message);
                            } else {
                                *doc = json!({"state":"error","artists":[],"error":message});
                            }
                        }, &catalog);
                        return;
                    }
                    {
                        let mut state = shared.0.lock().unwrap_or_else(|e| e.into_inner());
                        if state.generation != generation { return; }
                        state.searches_used = used;
                    }
                    let has_more = page.has_more && (!include_sources || budgets.iter().any(|(_, remaining)| *remaining > 0));
                    shared.publish(generation, |doc| {
                        *doc = json!({"state":"ready","artists":rows,"sceneLabel":page.label,
                            "genreSummary":page.genre_summary,"seedGenres":page.seed_genres,
                            "countryCode":origin.country,"hasMore":has_more,
                            "nextOffset":page.next_offset,
                            "artistName":origin.name,
                            "totalCandidates":page.total,"includeSources":include_sources,
                            "progress":{"percentage":100,"detail":"done"}});
                    }, &catalog);
                }
                Err(error) => shared.publish(generation, |doc| {
                    let message = crate::metadata::failure_message(chilpa_meta::Provider::MusicBrainz, &error);
                    if more {
                        doc["loadingMore"] = json!(false);
                        doc["error"] = json!(message);
                        doc["hasMore"] = json!(true);
                    } else {
                        *doc = json!({"state":"error","artists":[],"error":message});
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
        let known = self.artist_network.of(name, &[]);
        if let Some(origin) = known.origin.filter(|o| !o.place.is_empty()) {
            self.artist_scene.start(origin, include_sources, self.natives.clone(), self.catalog.clone(), false);
        }
    }

    pub fn artist_scene_more(&mut self) {
        if let Some((origin, include)) = self.artist_scene.again() {
            self.artist_scene.start(origin, include, self.natives.clone(), self.catalog.clone(), true);
        }
    }

    pub fn artist_scene_retry(&mut self) {
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
    fn plugin_scene_contains_only_verified_catalog_artists() {
        let artist = chilpa_meta::scene::Candidate {
            id: "mbid".into(), name: "MusicBrainz name".into(), genres: vec!["rock".into()], score: 42,
        };
        assert!(scene_row(&artist, Vec::new(), true).is_none());
        assert_eq!(scene_row(&artist, Vec::new(), false).unwrap()["title"], "MusicBrainz name");
        let row = scene_row(&artist, vec![json!({"title":"Catalog name","ref":"artist:7"})], true).unwrap();
        assert_eq!(row["title"], "Catalog name");
        assert_eq!(row["sources"][0]["ref"], "artist:7");
    }
}
