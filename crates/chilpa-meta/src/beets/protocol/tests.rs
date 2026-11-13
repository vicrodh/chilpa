use super::*;
use serde_json::{json, Value};

fn request() -> Request { Request { tracks:vec![Track { id:74, ..Track::default() }], ..Request::default() } }
fn response() -> Value {
    json!({"protocol":1,"beets_version":BEETS_VERSION,"recommendation":"strong","candidates":[{
        "score":97,"mappings":[{"track":74,"index":0,"composers":["Writer"],"work":"Suite"}],
        "release":{"provider":"musicbrainz","id":"release-1","title":"Album","credits":[],"date":"2001",
            "genres":[],"label":"Label","catalog_number":"L1","catalog_numbers":[],"country":"","barcode":"",
            "discs":1,"media_titles":[],"release_group_id":"group-1","url":"https://musicbrainz.org/release/release-1",
            "tracks":[{"disc":1,"number":1,"title":"Part I","duration_ms":1000,"credits":[],"recording_id":"recording-1","track_id":"track-1"}]}
    }]})
}

#[test]
fn response_retains_source_identity_score_and_explicit_track_mapping() {
    let response = Response::decode(&serde_json::to_vec(&response()).unwrap(), &request()).unwrap();
    let candidate = &response.candidates[0];
    assert_eq!(candidate.release.provider, Source::MusicBrainz);
    assert_eq!(candidate.score, 97);
    assert_eq!(candidate.mappings[0].track, 74);
    assert_eq!(candidate.mappings[0].index, 0);
    assert_eq!(candidate.mappings[0].composers, ["Writer"]);
    assert!(candidate.release.genres.is_empty());
}

#[test]
fn foreign_ids_duplicate_targets_and_unknown_sources_are_rejected() {
    for mappings in [json!([{"track":75,"index":0,"composers":[],"work":""}]),
        json!([{"track":74,"index":1,"composers":[],"work":""}]),
        json!([{"track":74,"index":0,"composers":[],"work":""},{"track":74,"index":0,"composers":[],"work":""}])] {
        let mut value = response(); value["candidates"][0]["mappings"] = mappings;
        assert_eq!(Response::decode(&serde_json::to_vec(&value).unwrap(), &request()).unwrap_err(), Error::InvalidResponse);
    }
    let mut value = response(); value["candidates"][0]["release"]["provider"] = json!("unknown");
    assert_eq!(Response::decode(&serde_json::to_vec(&value).unwrap(), &request()).unwrap_err(), Error::InvalidResponse);
}

#[test]
fn limits_versions_and_failure_statuses_are_checked() {
    let mut value = response(); value["beets_version"] = json!("3.0.0");
    assert_eq!(Response::decode(&serde_json::to_vec(&value).unwrap(), &request()).unwrap_err(), Error::IncompatibleVersion);
    let mut value = response(); value["candidates"][0]["score"] = json!(101);
    assert_eq!(Response::decode(&serde_json::to_vec(&value).unwrap(), &request()).unwrap_err(), Error::InvalidResponse);
    assert_eq!(Response::decode(br#"{"protocol":1,"error":"unavailable"}"#, &Request::probe()).unwrap_err(), Error::Unavailable);
    let mut request = request(); request.tracks.push(request.tracks[0].clone());
    assert_eq!(request.encode().unwrap_err(), Error::InvalidRequest);
    request.tracks = vec![Track { title:"x".repeat(4097), ..Track::default() }];
    assert_eq!(request.encode().unwrap_err(), Error::InputLimit);
}
