//! The scene candidate identity decision, ported from QBZ's MusicBrainz
//! matcher. One exact catalog name is provisional; homonyms need ISRC credits.

use crate::http::{Failure, Net};
use crate::musicbrainz::{get, API};
use std::collections::{BTreeSet, HashMap};

/// QBZ inspects two distinct top-track ISRCs for each ambiguous catalog row.
pub const MAX_ISRCS_PER_CANDIDATE: usize = 2;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IsrcCredits {
    pub isrc: String,
    pub credited_mbids: BTreeSet<String>,
}

pub fn normalize_name(name: &str) -> String {
    name.trim().to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn normalize_isrc(raw: &str) -> Option<String> {
    let value: String = raw.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_uppercase();
    (!value.is_empty()).then_some(value)
}

/// Fetch all recordings for an ISRC through the same shared MusicBrainz pace
/// and retry policy as scene discovery. A search hit without the queried ISRC
/// in its own `isrcs` field is not evidence.
pub fn credits_for_isrc(net: &Net, isrc: &str) -> Result<IsrcCredits, Failure> {
    let normalized = normalize_isrc(isrc).ok_or(Failure::BadId)?;
    let answer = get(net, &format!("{API}/recording?query=isrc:{normalized}&fmt=json"))?;
    let mut credited_mbids = BTreeSet::new();
    for recording in answer["recordings"].as_array().into_iter().flatten() {
        let carries_isrc = recording["isrcs"].as_array().into_iter().flatten()
            .filter_map(|value| value.as_str())
            .any(|raw| normalize_isrc(raw).as_deref() == Some(normalized.as_str()));
        if !carries_isrc { continue; }
        for credit in recording["artist-credit"].as_array().into_iter().flatten() {
            if let Some(id) = credit["artist"]["id"].as_str() { credited_mbids.insert(id.to_string()); }
        }
    }
    Ok(IsrcCredits { isrc: normalized, credited_mbids })
}

/// QBZ's scene decision: count distinct ISRC votes for the expected MBID.
/// If several catalog homonyms have evidence, album count can break a strict
/// lead among already verified rows; it cannot establish identity by itself.
pub fn decide_scene_candidate(expected_mbid: &str,
    evidence: &[(String, Option<u64>, Vec<IsrcCredits>)]) -> Option<String> {
    let mut verified = Vec::new();
    for (catalog_id, albums, credits) in evidence {
        let votes = credits.iter().filter(|item| item.credited_mbids.contains(expected_mbid))
            .map(|item| item.isrc.as_str()).collect::<BTreeSet<_>>().len();
        if votes > 0 { verified.push((catalog_id, albums.unwrap_or(0), votes)); }
    }
    if verified.len() == 1 { return Some(verified[0].0.clone()); }
    if verified.len() < 2 { return None; }
    verified.sort_by(|a, b| b.1.cmp(&a.1));
    (verified[0].1 > verified[1].1).then(|| verified[0].0.clone())
}

/// Cache repeated ISRC lookups during a scene run. Separate plugin lanes can
/// encounter the same track; neither repeats a MusicBrainz request.
pub struct CreditsCache(HashMap<String, IsrcCredits>);

impl CreditsCache {
    pub fn new() -> Self { Self(HashMap::new()) }
    pub fn get(&mut self, net: &Net, isrc: &str) -> Result<IsrcCredits, Failure> {
        let key = normalize_isrc(isrc).ok_or(Failure::BadId)?;
        if let Some(found) = self.0.get(&key) { return Ok(found.clone()); }
        let found = credits_for_isrc(net, &key)?;
        self.0.insert(key, found.clone());
        Ok(found)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::testing::Canned;

    #[test]
    fn only_recordings_carrying_the_isrc_vote() {
        let answer = r#"{"recordings":[
            {"id":"right","isrcs":["GB-AAA-20-12345"],"artist-credit":[{"artist":{"id":"artist-a"}}]},
            {"id":"false-hit","isrcs":["USBBB2012345"],"artist-credit":[{"artist":{"id":"artist-b"}}]}
        ]}"#;
        let http = Canned::new().json(answer);
        let credits = credits_for_isrc(&Net::free(&http), "GB-AAA-20-12345").unwrap();
        assert_eq!(credits.credited_mbids, BTreeSet::from(["artist-a".to_string()]));
    }

    #[test]
    fn ambiguous_names_need_isrc_and_only_verified_rows_use_album_tiebreak() {
        let vote = IsrcCredits { isrc:"GBAAA2012345".into(), credited_mbids:BTreeSet::from(["mbid".into()]) };
        let no_vote = IsrcCredits { isrc:"USBBB2012345".into(), credited_mbids:BTreeSet::from(["other".into()]) };
        assert_eq!(decide_scene_candidate("mbid", &[("a".into(),Some(100),vec![no_vote.clone()]),
            ("b".into(),Some(2),vec![vote.clone()])]), Some("b".into()));
        assert_eq!(decide_scene_candidate("mbid", &[("a".into(),Some(100),vec![no_vote]),
            ("b".into(),Some(2),vec![])]), None);
        assert_eq!(decide_scene_candidate("mbid", &[("a".into(),Some(10),vec![vote.clone()]),
            ("b".into(),Some(10),vec![vote])]), None);
    }
}
