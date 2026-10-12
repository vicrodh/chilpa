//! Catalog projection of a MusicBrainz scene candidate. QBZ's
//! `resolve_scene_candidate` supplies the acceptance rule; the native plugin
//! contract supplies search results and artist pages instead of a direct SDK.

use chilpa_meta::scene::identity::{self, CreditsCache, IsrcCredits};
use chilpa_meta::scene::Candidate;
use serde_json::{json, Value};
use chilpa_plugin::native::NativeError;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use crate::natives::Natives;

const SEARCH_TIMEOUT: Duration = Duration::from_secs(30);
const PAGE_TIMEOUT: Duration = Duration::from_secs(30);

pub(crate) fn exact_candidates(answer: &Value, name: &str) -> Vec<Value> {
    let wanted = identity::normalize_name(name);
    answer["artists"].as_array().into_iter().flatten()
        .take(5)
        .filter(|item| item["ref"].as_str().is_some_and(|reference| reference.starts_with("artist:")))
        .filter(|item| item["title"].as_str().is_some_and(|title| identity::normalize_name(title) == wanted))
        .cloned().collect()
}

/// The top-track ISRCs of this artist, not appearances or similarly named
/// performers. QBZ caps this at two distinct codes per ambiguous candidate.
pub(crate) fn own_top_isrcs(page: &Value, artist_ref: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    page["shelves"].as_array().into_iter().flatten()
        .filter(|shelf| matches!(shelf["id"].as_str(), Some("tracks" | "top-songs")) && shelf["kind"] == "track")
        .flat_map(|shelf| shelf["items"].as_array().into_iter().flatten())
        .filter(|track| track["artist_ref"].as_str() == Some(artist_ref))
        .filter_map(|track| track["isrc"].as_str().and_then(identity::normalize_isrc))
        .filter(|isrc| seen.insert(isrc.clone()))
        .take(identity::MAX_ISRCS_PER_CANDIDATE).collect()
}

pub(crate) fn projection(id: &str, display_name: &str, artist: &Candidate,
    item: &Value) -> Value {
    let source = json!({"plugin":id,"name":display_name,"ref":item["ref"],
        "title":item["title"],"image":item["image"],"albums":item["albums"]});
    json!({"mbid":artist.id,"title":item["title"],"genres":artist.genres,
        "score":artist.score,"sources":[source]})
}

/// Search exactly five rows as QBZ does. One exact name is a provisional
/// catalog match. Several require top-track ISRC credits for the candidate's
/// MBID, with QBZ's decision function handling verified ties.
pub(crate) fn resolve(natives: &Arc<Natives>, source: &str, artist: &Candidate,
    net: &chilpa_meta::Net, cache: &mut CreditsCache) -> Result<Option<Value>, NativeError> {
    let answer = natives.call_raw(source, "search", json!({"text":artist.name,"limit":5}), SEARCH_TIMEOUT)
        ?;
    let exact = exact_candidates(&answer, &artist.name);
    let chosen = match exact.as_slice() {
        [] => None,
        [one] => Some(one),
        many => {
            let mut evidence = Vec::new();
            for candidate in many {
                let Some(reference) = candidate["ref"].as_str() else { continue; };
                let page = natives.call_raw(source, "page.get", json!({"ref":reference}), PAGE_TIMEOUT);
                let isrcs = page.as_ref().map(|document| own_top_isrcs(document, reference)).unwrap_or_default();
                let credits: Vec<IsrcCredits> = isrcs.iter().filter_map(|isrc| cache.get(net, isrc).ok()).collect();
                evidence.push((reference.to_string(), candidate["albums"].as_u64(), credits));
            }
            let winner = identity::decide_scene_candidate(&artist.id, &evidence);
            many.iter().find(|item| item["ref"].as_str() == winner.as_deref())
        }
    };
    Ok(chosen.map(|item| projection(source, &natives.display_name(source), artist, item)))
}

pub(crate) fn terminal_error(error: &NativeError) -> bool {
    match error {
        NativeError::Closed | NativeError::Busy | NativeError::Timeout => true,
        NativeError::Rpc { code, data, .. } => *code == -32001 || data.as_ref().is_some_and(|value|
            value["terminal"] == true || value["retry_after_ms"].as_u64().is_some()
                || value["status"].as_u64().is_some_and(|status| matches!(status, 401 | 403 | 429))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_takes_only_exact_names_and_own_tracks_supply_isrcs() {
        let result = json!({"artists":[{"ref":"artist:1","title":"Iron  Maiden"},
            {"ref":"artist:2","title":"Maiden"},{"ref":"album:3","title":"Iron Maiden"}]});
        assert_eq!(exact_candidates(&result, "iron maiden").len(), 1);
        let page = json!({"shelves":[{"id":"tracks","kind":"track","items":[
            {"artist_ref":"artist:1","isrc":"GB-AAA-20-12345"},
            {"artist_ref":"artist:2","isrc":"USBBB2012345"},
            {"artist_ref":"artist:1","isrc":"GBAAA2012345"}]}]});
        assert_eq!(own_top_isrcs(&page, "artist:1"), ["GBAAA2012345"]);
    }

    #[test]
    fn account_and_quota_failures_end_only_the_current_catalog() {
        let limited = NativeError::Rpc { code:-32000, message:"wait".into(),
            data:Some(json!({"status":429,"retry_after_ms":1500})) };
        let forbidden = NativeError::Rpc { code:-32000, message:"forbidden".into(),
            data:Some(json!({"status":403})) };
        let transient = NativeError::Rpc { code:-32000, message:"server".into(),
            data:Some(json!({"status":503})) };
        assert!(terminal_error(&limited));
        assert!(terminal_error(&forbidden));
        assert!(!terminal_error(&transient));
    }
}
