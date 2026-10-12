//! Artists sharing a place and musical vocabulary.

mod genre;

use crate::artist::Origin;
use crate::http::{Failure, Net};
use crate::musicbrainz::{get, API};
use crate::text::{encode, is_uuid, query_escape};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

pub use genre::{extract_affinity_seeds, genre_summary, is_broad_genre, normalize_genre};

#[derive(Clone, Debug)]
pub struct Tag {
    pub name: String,
    pub count: Option<i32>,
}

#[derive(Clone, Debug, Default)]
pub struct AffinitySeeds {
    pub genres: Vec<String>,
    pub tags: Vec<String>,
    pub normalized_seeds: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Candidate {
    pub id: String,
    pub name: String,
    pub genres: Vec<String>,
    pub score: i32,
}

#[derive(Clone, Debug)]
pub struct Page {
    pub label: String,
    pub genre_summary: String,
    pub seed_genres: Vec<String>,
    pub artists: Vec<Candidate>,
    pub total: usize,
    pub has_more: bool,
}

fn useful_tags(value: &serde_json::Value) -> Vec<Tag> {
    value.as_array().into_iter().flatten().filter_map(|tag| {
        let name = tag["name"].as_str()?.trim();
        let count = tag["count"].as_i64().unwrap_or(0);
        (count > 0 && !name.is_empty()).then(|| Tag { name: name.into(), count: Some(count.min(i32::MAX as i64) as i32) })
    }).collect()
}

/// Walk the area's parent chain, stopping at a subdivision.
/// A failed lookup leaves the artist's own place usable.
fn subdivision(net: &Net, area_id: &str, cancelled: &AtomicBool) -> Option<String> {
    let mut current = area_id.to_string();
    for _ in 0..5 {
        if cancelled.load(Ordering::Relaxed) || !is_uuid(&current) { return None; }
        let detail = get(net, &format!("{API}/area/{current}?inc=area-rels&fmt=json")).ok()?;
        let own_type = detail["type"].as_str().unwrap_or_default();
        let parents: Vec<_> = detail["relations"].as_array().into_iter().flatten()
            .filter(|rel| rel["type"] == "part of" && rel["direction"] == "backward")
            .filter_map(|rel| rel.get("area")).collect();
        if parents.iter().any(|p| p["type"] == "Country") {
            return (!current.eq_ignore_ascii_case(area_id) && !own_type.eq_ignore_ascii_case("country"))
                .then(|| detail["name"].as_str().unwrap_or_default().to_string()).filter(|s| !s.is_empty());
        }
        let next = parents.iter().find(|p| p["type"] == "Subdivision")
            .or_else(|| parents.iter().find(|p| p["type"] != "City" && p["type"] != "Country"))
            .or_else(|| parents.first())?;
        current = next["id"].as_str()?.to_string();
    }
    None
}

/// Discover candidates by area and tag affinity. Catalog projection happens
/// outside this function.
pub fn discover(
    net: &Net,
    origin: &Origin,
    offset: usize,
    limit: usize,
    cancelled: &AtomicBool,
    progress: &dyn Fn(u8, &str),
) -> Result<Page, Failure> {
    if cancelled.load(Ordering::Relaxed) { return Err(Failure::Unexpected("cancelled".into())); }
    let tags: Vec<_> = origin.tags.iter().map(|name| Tag { name: name.clone(), count: Some(1) }).collect();
    let seeds = extract_affinity_seeds(&tags);
    let search_name = subdivision(net, &origin.area_id, cancelled).unwrap_or_else(|| origin.place.clone());
    let country = if origin.country_name.trim().is_empty() { None } else { Some(origin.country_name.trim()) };
    let display = match country {
        Some(name) if !name.eq_ignore_ascii_case(&search_name) => format!("{name}, {search_name}"),
        _ => search_name.clone(),
    };
    let mut search_genres: Vec<String> = if seeds.genres.is_empty() {
        seeds.tags.iter().filter(|g| !is_broad_genre(g)).take(3).cloned().collect()
    } else {
        seeds.genres.iter().chain(seeds.tags.iter().take(2)).filter(|g| !is_broad_genre(g)).cloned().collect()
    };
    if search_genres.is_empty() { search_genres = seeds.genres.iter().chain(seeds.tags.iter()).take(3).cloned().collect(); }
    let empty = || Page { label: format!("{display} scene"), genre_summary: genre_summary(&seeds), seed_genres: seeds.genres.clone(), artists: Vec::new(), total: 0, has_more: false };
    if search_genres.is_empty() || search_name.is_empty() { return Ok(empty()); }

    // MusicBrainz caps a single artist search at 100.
    let mut candidates: HashMap<String, (String, i32, usize, Vec<String>)> = HashMap::new();
    let mut failures = 0;
    let mut last_error = None;
    for (index, genre) in search_genres.iter().enumerate() {
        if cancelled.load(Ordering::Relaxed) { return Err(Failure::Unexpected("cancelled".into())); }
        progress((index * 40 / search_genres.len()) as u8, genre);
        let query = format!("tag:\"{}\" AND area:\"{}\"", query_escape(genre), query_escape(country.unwrap_or(&search_name)));
        let url = format!("{API}/artist?query={}&fmt=json&limit=100&offset=0", encode(&query));
        let response = match get(net, &url) {
            Ok(response) => response,
            Err(error) => { failures += 1; last_error = Some(error); continue; }
        };
        for artist in response["artists"].as_array().into_iter().flatten() {
            let Some(id) = artist["id"].as_str().filter(|id| is_uuid(id) && *id != origin.id) else { continue };
            let Some(name) = artist["name"].as_str().filter(|name| !name.is_empty()) else { continue };
            let candidate_tags = useful_tags(&artist["tags"]).into_iter().map(|t| t.name).collect::<Vec<_>>();
            let same_city = artist["begin-area"]["name"].as_str().is_some_and(|name| name.eq_ignore_ascii_case(&search_name))
                || artist["begin-area"]["id"].as_str() == Some(origin.area_id.as_str());
            let same_country = artist["area"]["name"].as_str().is_some_and(|name| name.eq_ignore_ascii_case(&search_name));
            let score = affinity(&candidate_tags, &seeds, same_city, same_country);
            let entry = candidates.entry(id.into()).or_insert_with(|| (name.into(), 0, 0, Vec::new()));
            entry.1 += score;
            entry.2 += 1;
            for tag in candidate_tags { if !entry.3.contains(&tag) { entry.3.push(tag); } }
        }
    }
    if failures == search_genres.len() { return Err(last_error.unwrap_or(Failure::Unreachable("scene search failed".into()))); }
    let mut scored: Vec<Candidate> = candidates.into_iter().map(|(id, (name, score, hits, tags))| {
        let tags = tags.into_iter().map(|name| Tag { name, count: Some(1) }).collect::<Vec<_>>();
        let genres = extract_affinity_seeds(&tags).genres;
        Candidate { id, name, genres, score: score + (hits.saturating_sub(1) as i32) * 15 }
    }).collect();
    scored.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.id.cmp(&b.id)));
    let total = scored.len();
    // Discovery owns 0..40; the host owns 40..95 while it projects the
    // selected catalogs. Keep the progress bar monotonic.
    progress(40, "candidates");
    Ok(Page { label: format!("{display} scene"), genre_summary: genre_summary(&seeds), seed_genres: seeds.genres, artists: scored.into_iter().skip(offset).take(limit).collect(), total, has_more: offset.saturating_add(limit) < total })
}

/// Affinity weights and the no-overlap penalty.
pub fn affinity(tags: &[String], seeds: &AffinitySeeds, same_city: bool, same_country: bool) -> i32 {
    let normalized: std::collections::HashSet<_> = tags.iter().map(|t| normalize_genre(t)).collect();
    let core = seeds.genres.iter().filter(|g| normalized.contains(*g)).count() as i32;
    let secondary = seeds.tags.iter().filter(|g| normalized.contains(*g)).count() as i32;
    (if same_city { 40 } else { 0 }) + (if same_country { 15 } else { 0 })
        + core * 20 + secondary * 8 + (if !normalized.is_empty() && core == 0 && secondary == 0 { -12 } else { 0 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::testing::Canned;
    #[test]
    fn scene_affinity_keeps_the_reference_weights() {
        let seeds = AffinitySeeds { genres: vec!["grunge".into()], tags: vec!["seattle".into()], ..Default::default() };
        assert_eq!(affinity(&["grunge".into(), "seattle".into()], &seeds, true, true), 83);
        assert_eq!(affinity(&["unrelated".into()], &seeds, false, false), -12);
    }

    #[test]
    fn discovery_queries_the_country_and_keeps_the_affinity_order() {
        let http = Canned::new().json(r#"{"artists":[
            {"id":"11111111-1111-4111-8111-111111111111","name":"Near","begin-area":{"name":"Seattle"},"tags":[{"name":"grunge","count":4}]},
            {"id":"22222222-2222-4222-8222-222222222222","name":"Far","tags":[{"name":"unrelated","count":3}]}
        ]}"#);
        let origin = Origin { id:"33333333-3333-4333-8333-333333333333".into(), name:"Seed".into(),
            place:"Seattle".into(), country:"US".into(), country_name:"United States".into(), tags:vec!["grunge".into()], ..Default::default() };
        let page = discover(&Net::free(&http), &origin, 0, 100, &AtomicBool::new(false), &|_, _| {}).unwrap();
        assert_eq!(page.artists.iter().map(|artist| artist.name.as_str()).collect::<Vec<_>>(), ["Near", "Far"]);
        assert!(http.asked()[0].contains("United%20States"), "the country is the search area");
    }
}
