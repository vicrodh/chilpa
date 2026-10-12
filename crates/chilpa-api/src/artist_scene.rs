//! A place's artist scene. Catalog projections are optional and run only
//! after the listener chooses them.

use chilpa_meta::artist::Origin;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

struct Current {
    generation: u64,
    cancelled: Arc<AtomicBool>,
    origin: Option<Origin>,
    include_sources: bool,
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
        state.document = json!({"state":"idle","artists":[]});
    }

    pub(crate) fn start(&self, origin: Origin, include_sources: bool,
        natives: Arc<crate::natives::Natives>, catalog: Arc<crate::source_catalog::Catalog>, more: bool) {
        let (generation, cancelled, previous) = {
            let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
            if more && (state.document["state"] != "ready" || state.document["hasMore"] != true) { return; }
            state.cancelled.store(true, Ordering::Relaxed);
            state.generation = state.generation.wrapping_add(1);
            state.cancelled = Arc::new(AtomicBool::new(false));
            state.origin = Some(origin.clone());
            state.include_sources = include_sources;
            let previous = if more { state.document["artists"].as_array().cloned().unwrap_or_default() } else { Vec::new() };
            state.document = json!({
                "state": if more { "ready" } else { "loading" }, "loadingMore": more,
                "artists": previous.clone(), "progress":{"percentage":0,"detail":""},
                "place":origin.place, "countryCode":origin.country,
                "artistName":origin.name,
                "includeSources":include_sources, "hasMore": false,
            });
            (state.generation, state.cancelled.clone(), previous)
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
            let found = chilpa_meta::scene::discover(&net, &origin, previous.len(), 100, &cancelled, &report);
            if cancelled.load(Ordering::Relaxed) { return; }
            match found {
                Ok(page) => {
                    let mut rows = previous;
                    let mut source_ids: Vec<_> = if include_sources { natives.ids().into_iter().filter(|id| natives.searches(id) && natives.has_pages(id)).collect() } else { Vec::new() };
                    source_ids.sort();
                    let mut budgets: Vec<_> = source_ids.iter().map(|id| (id.clone(), natives.scene_search_limit(id))).collect();
                    for (at, artist) in page.artists.iter().enumerate() {
                        if cancelled.load(Ordering::Relaxed) { return; }
                        let mut sources = Vec::new();
                        for (id, budget) in &mut budgets {
                            if *budget == 0 { continue; }
                            *budget -= 1;
                            let result = natives.call(id, "search", json!({"text":artist.name,"limit":10}), Duration::from_secs(12));
                            let Ok(answer) = result else { *budget = 0; continue; };
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
                                sources.push(json!({"plugin":id,"name":natives.display_name(id),"ref":item["ref"],"image":item["image"],"albums":item["albums"]}));
                            }
                        }
                        rows.push(json!({"mbid":artist.id,"title":artist.name,"genres":artist.genres,"score":artist.score,"sources":sources}));
                        if at % 5 == 0 { report(40 + (at * 55 / page.artists.len().max(1)) as u8, &format!("{}/{}", at, page.artists.len())); }
                    }
                    shared.publish(generation, |doc| {
                        *doc = json!({"state":"ready","artists":rows,"sceneLabel":page.label,
                            "genreSummary":page.genre_summary,"seedGenres":page.seed_genres,
                            "countryCode":origin.country,"hasMore":page.has_more,
                            "artistName":origin.name,
                            "totalCandidates":page.total,"includeSources":include_sources,
                            "progress":{"percentage":100,"detail":"done"}});
                    }, &catalog);
                }
                Err(error) => shared.publish(generation, |doc| {
                    if more {
                        doc["loadingMore"] = json!(false);
                        doc["error"] = json!(format!("{error:?}"));
                        doc["hasMore"] = json!(true);
                    } else {
                        *doc = json!({"state":"error","artists":[],"error":format!("{error:?}")});
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
