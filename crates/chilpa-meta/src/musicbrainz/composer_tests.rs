use super::*;
use crate::http::testing::Canned;
use serde_json::json;

const RECORDING: &str = "00000000-0000-4000-8000-000000000001";
const WORK: &str = "00000000-0000-4000-8000-000000000002";
const OTHER_WORK: &str = "00000000-0000-4000-8000-000000000003";

#[test]
fn isrcs_are_folded_only_after_their_shape_is_validated() {
    for valid in ["USABC1200001", "usA1z1200001", "GB0000000000"] {
        assert_eq!(isrc_key(valid), Some(valid.to_ascii_uppercase()));
    }
    for invalid in ["", "USABC120001", "USABC12000011", "1SABC1200001", "USAB-1200001",
        "USABC12A0001", "US-ABC-12-00001", " USABC1200001", "USABC1200001 ", "ÜSABC1200001"] {
        let http = Canned::new();
        assert!(isrc_key(invalid).is_none(), "{invalid}");
        assert!(composers_by_isrc(&Net::free(&http), invalid, &mut WorkCache::default()).unwrap().is_empty());
        assert!(http.asked().is_empty());
    }
}

#[test]
fn all_three_urls_send_only_the_required_identifiers_and_includes() {
    let http = Canned::new()
        .json(&json!({"recordings":[{"id":RECORDING}]}).to_string())
        .json(&json!({"relations":[{"type":"performance","work":{"id":WORK}}]}).to_string())
        .json(&json!({"relations":[{"type":"composer","artist":{"name":"Composer"}}]}).to_string());
    assert_eq!(composers_by_isrc(&Net::free(&http), "usabc1200001", &mut WorkCache::default()).unwrap(), ["Composer"]);
    assert_eq!(http.asked(), [
        format!("{API}/isrc/USABC1200001?inc=recordings&fmt=json"),
        format!("{API}/recording/{RECORDING}?inc=work-rels&fmt=json"),
        format!("{API}/work/{WORK}?inc=artist-rels&fmt=json"),
    ]);
    assert_eq!(relation_url("work", "../work", "artist-rels"), Err(Failure::BadId));
}

#[test]
fn each_work_prefers_composers_and_keeps_credit_order_without_duplicates() {
    let work = json!({"relations":[
        {"type":"writer","artist":{"name":"Ignored writer"}},
        {"type":"lyricist","artist":{"name":"Ignored lyricist"}},
        {"type":"composer","ordering-key":2,"artist":{"id":"b","name":"Second"}},
        {"type":"composer","ordering-key":1,"target-credit":"First credit","artist":{"id":"a","name":"First"}},
        {"type":"composer","ordering-key":3,"artist":{"id":"a","name":"Alias"}},
        {"type":"composer","ordering-key":4,"artist":{"id":"c","name":"second"}}
    ]});
    assert_eq!(work_composers(&work).unwrap(), ["First credit", "Second"]);
    assert_eq!(work_composers(&json!({"relations":[
        {"type":"writer","artist":{"name":"Writer B"}},
        {"type":"lyricist","artist":{"name":"Ignored"}},
        {"type":"writer","artist":{"name":"Writer A"}},
        {"type":"writer","artist":{"name":"Writer B"}}
    ]})).unwrap(), ["Writer B", "Writer A"]);
    assert!(work_composers(&json!({"relations":[
        {"type":"composer","artist":{"name":""}},
        {"type":"writer","artist":{"name":"Writer"}}
    ]})).unwrap().is_empty(), "a composer relation prevents the writer fallback");
}

#[test]
fn every_recording_and_performed_work_contributes_and_works_are_shared_across_tracks() {
    let recordings = json!({"recordings":(1..=7).map(|n| json!({"id":format!("00000000-0000-4000-8000-{n:012}")})).collect::<Vec<_>>()});
    let performance = json!({"relations":[
        {"type":"performance","work":{"id":WORK}}, {"type":"performance","work":{"id":OTHER_WORK}},
        {"type":"arrangement","work":{"id":"ignored"}}
    ]});
    let mut http = Canned::new().json(&recordings.to_string()).json(&performance.to_string())
        .json(&json!({"relations":[{"type":"composer","artist":{"name":"Composer"}}]}).to_string())
        .json(&json!({"relations":[{"type":"writer","artist":{"name":"Writer"}},{"type":"writer","artist":{"name":"Composer"}}]}).to_string());
    for _ in 1..7 { http = http.json(&performance.to_string()); }
    http = http.json(&json!({"recordings":[{"id":RECORDING}]}).to_string()).json(&performance.to_string());
    let mut works = WorkCache::default();
    assert_eq!(composers_by_isrc(&Net::free(&http), "USABC1200001", &mut works).unwrap(), ["Composer", "Writer"]);
    assert_eq!(http.asked().len(), 10);
    assert_eq!(composers_by_isrc(&Net::free(&http), "USABC1200002", &mut works).unwrap(), ["Composer", "Writer"]);
    assert_eq!(http.asked().len(), 12, "the second track reads no shared work again");
}

#[test]
fn absent_isrcs_are_empty_but_broken_responses_and_failed_work_reads_are_errors() {
    for http in [Canned::new().status(404, &[], b"{}"), Canned::new().json(r#"{"recordings":[]}"#)] {
        assert!(composers_by_isrc(&Net::free(&http), "USABC1200001", &mut WorkCache::default()).unwrap().is_empty());
    }
    let http = Canned::new().json("{}");
    assert!(composers_by_isrc(&Net::free(&http), "USABC1200001", &mut WorkCache::default()).is_err());
    let http = Canned::new().json(&json!({"recordings":[{"id":RECORDING}]}).to_string())
        .json(&json!({"relations":[{"type":"performance","work":{"id":WORK}}]}).to_string())
        .status(404, &[], b"{}");
    let mut works = WorkCache::default();
    assert!(composers_by_isrc(&Net::free(&http), "USABC1200001", &mut works).is_err());
    assert!(works.entries.is_empty());
}

#[test]
fn work_storage_and_credit_bytes_have_fixed_budgets() {
    let mut works = WorkCache::default();
    for n in 0..600 {
        works.insert(&n.to_string(), vec!["Composer".into()]);
        assert!(works.entries.len() <= 512 && works.bytes <= 2 * 1024 * 1024);
    }
    let work = json!({"relations":[{"type":"composer","artist":{"name":"x".repeat(COMPOSER_BYTES)}}]});
    assert!(work_composers(&work).is_err(), "I reject oversized credits instead of silently truncating them");
    for n in 0..100 {
        works.insert(&n.to_string(), vec!["x".repeat(9_000)]);
        assert!(works.bytes <= 2 * 1024 * 1024);
    }
}
