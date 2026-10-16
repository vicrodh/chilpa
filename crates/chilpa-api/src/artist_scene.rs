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
    candidates: Option<chilpa_meta::scene::Page>,
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
            candidates: None,
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
        state.candidates = None;
        state.document = json!({"state":"idle","artists":[]});
    }

    pub(crate) fn start(&self, origin: Origin, include_sources: bool,
        natives: Arc<crate::natives::Natives>, catalog: Arc<crate::source_catalog::Catalog>, more: bool) {
        let ids = if include_sources { source_ids(&natives) } else { Vec::new() };
        let (generation, cancelled, previous, offset, searches_used, candidates) = {
            let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
            if more && (state.document["state"] != "ready" || state.document["hasMore"] != true) { return; }
            let earlier = state.document.clone();
            let offset = if more { earlier["nextOffset"].as_u64().unwrap_or(0) as usize } else { 0 };
            state.cancelled.store(true, Ordering::Relaxed);
            state.generation = state.generation.wrapping_add(1);
            state.cancelled = Arc::new(AtomicBool::new(false));
            state.origin = Some(origin.clone());
            state.include_sources = include_sources;
            if !more { state.searches_used.clear(); state.candidates = None; }
            let searches_used = state.searches_used.clone();
            state.document = if more {
                let mut document = earlier;
                document["loadingMore"] = json!(true);
                document["hasMore"] = json!(false);
                document["error"] = Value::Null;
                document["progress"] = json!({"phase":"searching","percentage":0,"detail":""});
                document
            } else {
                let mut sources: Vec<_> = if include_sources {
                    ids.iter().map(|id| lane(id, &natives.display_name(id), Vec::new(), "pending", "", 0,
                        natives.scene_search_limit(id), false)).collect()
                } else {
                    vec![lane("musicbrainz", "MusicBrainz", Vec::new(), "pending", "", 0, 0, false)]
                };
                sources.insert(0, lane("library", "Chilpa", Vec::new(), "pending", "", 0, 0, false));
                json!({"state":"loading","loadingMore":false,
                    "sources":sources,"progress":{"phase":"searching","percentage":0,"detail":""},
                    "countryCode":origin.country,"artistName":origin.name,
                    "includeSources":include_sources,"hasMore":false,"nextOffset":0})
            };
            let previous = state.document["sources"].as_array().cloned().unwrap_or_default();
            (state.generation, state.cancelled.clone(), previous, offset, searches_used, state.candidates.clone())
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
            let found = candidates.map(Ok).unwrap_or_else(|| chilpa_meta::scene::discover(&net, &origin, 0, usize::MAX, &cancelled, &report));
            if cancelled.load(Ordering::Relaxed) { return; }
            match found {
                Ok(page) => {
                    {
                        let mut state = shared.0.lock().unwrap_or_else(|e| e.into_inner());
                        if state.generation != generation { return; }
                        state.candidates = Some(page.clone());
                    }
                    let local = crate::scene_results::Local::new(crate::Reader::open(&chilpa_library::Library::default_path())
                        .map(|reader| reader.artists("all")).unwrap_or_default());
                    let mut used = searches_used;
                    let mut lanes = previous;
                    let mut credits = chilpa_meta::scene::identity::CreditsCache::new();
                    shared.publish(generation, |doc| {
                        doc["sceneLabel"] = json!(page.label);
                        doc["genreSummary"] = json!(page.genre_summary);
                        doc["seedGenres"] = json!(page.seed_genres);
                        doc["totalCandidates"] = json!(page.total);
                    }, &catalog);
                    let next_offset = crate::scene_results::fill(offset, page.artists.len(), more, |range| {
                        if cancelled.load(Ordering::Relaxed) { return usize::MAX; }
                        let end = range.end;
                        let batch = &page.artists[range];
                        let library = lanes.iter_mut().find(|lane| lane["id"] == "library").expect("library lane");
                        let mut local_rows = library["artists"].as_array().cloned().unwrap_or_default();
                        local_rows.extend(batch.iter().filter_map(|candidate| local.project(candidate)));
                        *library = lane("library", "Chilpa", local_rows, "ready", "", end, 0, end < page.total);
                        if !include_sources {
                            let metadata = lanes.iter_mut().find(|lane| lane["id"] == "musicbrainz").expect("metadata lane");
                            let mut artists = metadata["artists"].as_array().cloned().unwrap_or_default();
                            artists.extend(batch.iter().map(musicbrainz_row));
                            *metadata = lane("musicbrainz", "MusicBrainz", artists, "ready", "", end, 0, end < page.total);
                        }
                        shared.publish(generation, |doc| { doc["sources"] = json!(lanes); }, &catalog);
                        for (source_index, id) in ids.iter().enumerate() {
                            if cancelled.load(Ordering::Relaxed) { return usize::MAX; }
                            let position = lanes.iter().position(|lane| lane["id"] == *id).unwrap_or_else(|| {
                                lanes.push(lane(id, &natives.display_name(id), Vec::new(), "pending", "", 0, natives.scene_search_limit(id), false));
                                lanes.len() - 1
                            });
                            let mut artists = lanes[position]["artists"].as_array().cloned().unwrap_or_default();
                            let total_limit = natives.scene_search_limit(id);
                            let remaining = total_limit.saturating_sub(used.get(id).copied().unwrap_or(0));
                            if remaining == 0 { continue; }
                            let mut checked = 0usize;
                            let mut failures = 0usize;
                            let mut error = String::new();
                            let mut terminal = false;
                            let source_name = natives.display_name(id);
                            let icon = crate::plugin_icons::image_url(&crate::Chilpa::plugins_dir(), id);
                            let span = ids.len().max(1) * batch.len().max(1);
                            for candidate in batch.iter().take(remaining) {
                                if cancelled.load(Ordering::Relaxed) { return usize::MAX; }
                                checked += 1;
                                *used.entry(id.clone()).or_default() += 1;
                                match crate::scene_catalog::resolve(&natives, id, candidate, &net, &mut credits) {
                                    Ok(Some(mut row)) => { row["sources"][0]["icon"] = json!(icon); artists.push(row); },
                                    Ok(None) => {},
                                    Err(why) => {
                                        failures += 1; error = why.to_string();
                                        if crate::scene_catalog::terminal_error(&why) {
                                            terminal = true; used.insert(id.clone(), total_limit); break;
                                        }
                                    }
                                }
                                if checked % 5 == 0 {
                                    lanes[position] = lane(id, &source_name, artists.clone(), "pending", &error,
                                        used.get(id).copied().unwrap_or(0), total_limit, false);
                                    let progress = 40 + (((source_index * batch.len() + checked) * 55 / span) as u8);
                                    shared.publish(generation, |doc| {
                                        doc["sources"] = json!(lanes);
                                        doc["progress"] = json!({"phase":"validating","percentage":progress,
                                            "detail":format!("{checked}/{}", batch.len()),"source":source_name});
                                    }, &catalog);
                                }
                            }
                            let status = if failures > 0 && artists.is_empty() { "error" }
                                else if failures > 0 { "partial" } else { "ready" };
                            let consumed = used.get(id).copied().unwrap_or(0);
                            lanes[position] = lane(id, &source_name, artists, status, &error, consumed,
                                total_limit, !terminal && end < page.total && consumed < total_limit);
                            shared.publish(generation, |doc| { doc["sources"] = json!(lanes); }, &catalog);
                        }
                        crate::scene_results::merge(&lanes).len()
                    });
                    if cancelled.load(Ordering::Relaxed) { return; }
                    let has_more = next_offset < page.total;
                    {
                        let mut state = shared.0.lock().unwrap_or_else(|e| e.into_inner());
                        if state.generation != generation { return; }
                        state.searches_used = used;
                    }
                    shared.publish(generation, |doc| {
                        doc["state"] = json!("ready");
                        doc["sources"] = json!(lanes);
                        doc["loadingMore"] = json!(false);
                        doc["hasMore"] = json!(has_more);
                        doc["nextOffset"] = json!(next_offset);
                        doc["progress"] = json!({"phase":"done","percentage":100,"detail":"done"});
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
        let lanes = state.document["sources"].as_array().cloned().unwrap_or_default();
        state.document["artists"] = json!(crate::scene_results::merge(&lanes));
        if state.document["state"] == "loading" && state.document["artists"].as_array().is_some_and(|rows| !rows.is_empty()) {
            state.document["state"] = json!("ready");
            state.document["loadingMore"] = json!(true);
        }
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
