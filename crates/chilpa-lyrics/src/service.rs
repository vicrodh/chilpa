//! What Chilpa and its lyrics service say to each other.
//!
//! Lyrics that did not come with the track and that its source does not
//! have are asked for at one address, with one question:
//!
//! ```text
//! POST /v1/lyrics/resolve
//! { "schema": 1,
//!   "track": { "title", "artist", "album"?, "duration_ms"?, "isrc"?, "mbid"? },
//!   "preferences": { "minimum_sync": "unsynced", "language": null } }
//! ```
//!
//! Behind it, the service asks whoever keeps lyrics, drops what is another
//! song and answers with the best that is left, already in the shape of a
//! [`Document`]: `{"schema":1,"status":"found","document":{…}}` or
//! `{"schema":1,"status":"none"}`. A failure is an HTTP failure, never
//! "none": "none" is remembered for days, a failure is asked again.
//!
//! This module is the contract and nothing else: it builds the question and
//! reads the answer. It makes no request; the facade does, and hands the
//! answer in. Who answers at that address, and which providers it asks, is
//! not known here and must not be.

use crate::matching::{Claim, Track};
use crate::model::{Document, Timing, SCHEMA as DOCUMENT_SCHEMA};
use crate::pipeline::Answer;
use crate::wire::Candidate;
use serde_json::{json, Map, Value};
use std::time::Duration;

/// The version of the question and the answer.
pub const SCHEMA: u64 = 1;
/// Where the service is. Released builds carry this address, so it never
/// changes; what answers behind it can.
pub const ORIGIN: &str = "https://lyrics.chilpa.app";
/// The one path asked.
pub const RESOLVE_PATH: &str = "/v1/lyrics/resolve";
/// The largest answer read. Lyrics with every word timed are a few tens of
/// kilobytes.
pub const MAX_ANSWER: usize = 1024 * 1024;
/// The longest pause the service may ask for.
const MAX_PAUSE: Duration = Duration::from_secs(3600);

/// The question for a track: everything known about it, and nothing made up.
/// What is unknown is left out.
pub fn request(track: &Track, minimum: Timing) -> Value {
    let mut identity = Map::new();
    identity.insert("title".into(), json!(track.title));
    identity.insert("artist".into(), json!(track.artist));
    let mut known = |key: &str, value: Option<&str>| {
        if let Some(value) = value.map(str::trim).filter(|v| !v.is_empty()) {
            identity.insert(key.into(), json!(value));
        }
    };
    known("album", track.album.as_deref());
    known("isrc", track.isrc.as_deref());
    known("mbid", track.mbid.as_deref());
    if let Some(length) = track.duration_ms.filter(|d| *d > 0) {
        identity.insert("duration_ms".into(), json!(length));
    }
    json!({
        "schema": SCHEMA,
        "track": identity,
        "preferences": { "minimum_sync": minimum, "language": null },
    })
}

/// Reads the service's answer: its HTTP status, the `Retry-After` it sent
/// and its body.
///
/// Only a 200 that says "none" is "none". Any other status, a body that is
/// not the contract's and a document that cannot be drawn are failures.
pub fn read(status: u16, retry_after: Option<&str>, body: &str) -> Answer {
    // Only "too many requests" is about this client, and pauses every
    // lookup. Any other failure is about this one track at this moment: the
    // next track is asked as usual.
    let failed = |message: String| Answer::Failed { retry_after: pause(retry_after).filter(|_| status == 429), message };
    if status != 200 {
        return failed(format!("the lyrics service answered {status}"));
    }
    if body.len() > MAX_ANSWER {
        return failed("the lyrics service sent more than is read".into());
    }
    let Ok(answer) = serde_json::from_str::<Value>(body) else {
        return failed("the lyrics service sent what is not JSON".into());
    };
    if answer.get("schema").and_then(Value::as_u64) != Some(SCHEMA) {
        return failed("the lyrics service speaks another version".into());
    }
    match answer.get("status").and_then(Value::as_str) {
        Some("none") => Answer::None,
        Some("found") => {
            let Some(document) = answer.get("document").cloned().and_then(|d| serde_json::from_value::<Document>(d).ok()) else {
                return failed("the lyrics service sent a document that cannot be read".into());
            };
            if document.schema != DOCUMENT_SCHEMA {
                return failed("the lyrics service sent a document of another version".into());
            }
            // Its own timing and order are not taken on trust.
            let document = document.finish();
            if document.is_empty() {
                return failed("the lyrics service sent an empty document".into());
            }
            Answer::Found(vec![Candidate { claim: claim_of(&document), document, raw: None }])
        }
        other => failed(format!("the lyrics service answered an unknown status {other:?}")),
    }
}

/// What the lyrics say they are for: what their provider claimed, or what
/// the document states about itself.
fn claim_of(document: &Document) -> Claim {
    let stated = document.source.as_ref().and_then(|s| s.claim.clone()).unwrap_or_default();
    Claim {
        title: stated.title.or_else(|| document.title.clone()),
        artist: stated.artist.or_else(|| document.artist.clone()),
        album: stated.album,
        duration_ms: stated.duration_ms.or(document.duration_ms),
        isrc: stated.isrc,
    }
}

/// `Retry-After` in whole seconds. A date is not read: the service sends
/// seconds.
fn pause(retry_after: Option<&str>) -> Option<Duration> {
    retry_after.and_then(|v| v.trim().parse::<u64>().ok()).map(|s| Duration::from_secs(s).min(MAX_PAUSE))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lrc;
    use crate::matching::{judge, Verdict};

    /// A question and its answer as the service's own tests wrote them.
    /// The files are copies of that repository's `test/golden`.
    struct Golden {
        request: Value,
        status: u16,
        retry_after: Option<String>,
        body: String,
    }

    fn golden(text: &str) -> Golden {
        let file: Value = serde_json::from_str(text).unwrap();
        Golden {
            request: file["request"].clone(),
            status: file["status"].as_u64().unwrap() as u16,
            retry_after: file.get("retry_after").and_then(Value::as_str).map(str::to_string),
            body: file["response"].to_string(),
        }
    }

    fn answer(text: &str) -> Answer {
        let g = golden(text);
        read(g.status, g.retry_after.as_deref(), &g.body)
    }

    fn found(text: &str) -> Candidate {
        match answer(text) {
            Answer::Found(mut list) => list.remove(0),
            other => panic!("expected lyrics, got {other:?}"),
        }
    }

    const LINE: &str = include_str!("../tests/service/found-line.json");
    const WORDS_FROM_A_FILE: &str = include_str!("../tests/service/found-word-lyricsfile.json");
    const WORDS_FROM_A_SYNC: &str = include_str!("../tests/service/found-word-richsync.json");
    const PLAIN: &str = include_str!("../tests/service/found-plain.json");
    const NONE: &str = include_str!("../tests/service/none.json");
    const FAILED: &str = include_str!("../tests/service/failed.json");

    fn small() -> Track {
        Track {
            title: "Small Hours".into(),
            artist: "Example Artist".into(),
            album: Some("Example Album".into()),
            duration_ms: Some(7_200),
            isrc: Some("QZES72500001".into()),
            mbid: None,
        }
    }

    #[test]
    fn the_question_is_the_one_the_service_reads() {
        assert_eq!(request(&small(), Timing::Unsynced), golden(WORDS_FROM_A_SYNC).request);
        let wasted = Track {
            title: "Wasted Years".into(),
            artist: "Iron Maiden".into(),
            album: Some("Somewhere in Time".into()),
            duration_ms: Some(306_834),
            isrc: None,
            mbid: None,
        };
        assert_eq!(request(&wasted, Timing::Unsynced), golden(LINE).request);
    }

    #[test]
    fn what_is_unknown_is_left_out_of_the_question() {
        let bare = Track { title: "Song".into(), artist: "Someone".into(), album: Some("  ".into()), duration_ms: Some(0), ..Track::default() };
        assert_eq!(request(&bare, Timing::Word)["track"], json!({"title": "Song", "artist": "Someone"}));
        assert_eq!(request(&bare, Timing::Word)["preferences"]["minimum_sync"], "word");
        let known = Track { mbid: Some("b1a9c0e9-d987-4042-ae91-78d6a3267d69".into()), ..small() };
        assert_eq!(request(&known, Timing::Unsynced)["track"]["mbid"], "b1a9c0e9-d987-4042-ae91-78d6a3267d69");
    }

    #[test]
    fn lyrics_by_line_are_read_and_say_who_gave_them() {
        let lines = found(LINE);
        assert_eq!(lines.document.timing, Timing::Line);
        assert_eq!(lines.document.lines.len(), 30);
        let source = lines.document.source.as_ref().unwrap();
        let stated = &golden(LINE);
        let stated: Value = serde_json::from_str(&stated.body).unwrap();
        assert_eq!(json!(source.provider), stated["document"]["source"]["provider"]);
        assert_eq!(json!(source.name), stated["document"]["source"]["name"], "the name the screen shows is the service's to say");
        assert_eq!(lines.claim.title.as_deref(), Some("Wasted Years"));
        assert_eq!(lines.claim.duration_ms, Some(307_000));
    }

    #[test]
    fn the_service_reads_lrc_as_chilpa_does() {
        // The same record the service was given, read here by Chilpa's own
        // reader: both sides must end with the same lines.
        let record: Value = serde_json::from_str(include_str!("../tests/service/provider-record-line.json")).unwrap();
        let here = lrc::parse(record["syncedLyrics"].as_str().unwrap());
        assert_eq!(found(LINE).document.lines, here.lines);
    }

    #[test]
    fn lyrics_by_word_keep_every_word() {
        for text in [WORDS_FROM_A_FILE, WORDS_FROM_A_SYNC] {
            let words = found(text);
            assert_eq!(words.document.timing, Timing::Word);
            let first = &words.document.lines[0];
            assert_eq!(first.text, "Stay until the morning");
            assert_eq!(first.segments.len(), 4);
            assert_eq!((first.segments[3].text.as_str(), first.segments[3].start_ms, first.segments[3].space_after), ("morning", 5_750, false));
        }
        assert_ne!(found(WORDS_FROM_A_FILE).document.source.unwrap().provider, found(WORDS_FROM_A_SYNC).document.source.unwrap().provider, "two providers, one shape");
        assert_eq!(found(WORDS_FROM_A_SYNC).claim.isrc.as_deref(), Some("QZES72500001"));
    }

    #[test]
    fn what_the_service_found_is_judged_again_here() {
        let words = found(WORDS_FROM_A_SYNC);
        assert_eq!(judge(&small(), &words.claim, &words.document), Verdict::Exact, "the same ISRC");
        let another = Track { isrc: Some("QZES79999999".into()), ..small() };
        assert_eq!(judge(&another, &words.claim, &words.document), Verdict::Rejected);
        let plain = found(PLAIN);
        assert_eq!(plain.document.timing, Timing::Unsynced);
        assert!(judge(&small(), &plain.claim, &plain.document).accepted());
    }

    #[test]
    fn none_is_none_and_a_failure_is_never_none() {
        assert_eq!(answer(NONE), Answer::None);
        match answer(FAILED) {
            Answer::Failed { retry_after, .. } => assert_eq!(retry_after, None, "one track that could not be answered pauses nothing"),
            other => panic!("a 503 is a failure, got {other:?}"),
        }
        let none_body = golden(NONE).body;
        match read(429, Some("30"), &none_body) {
            Answer::Failed { retry_after, .. } => assert_eq!(retry_after, Some(Duration::from_secs(30)), "asked to slow down, the pause is kept"),
            other => panic!("a 429 is a failure, got {other:?}"),
        }
        for status in [400, 403, 429, 500, 502, 503] {
            assert!(matches!(read(status, None, &none_body), Answer::Failed { .. }), "{status} is a failure whatever its body says");
        }
        assert!(matches!(read(200, None, "<html>"), Answer::Failed { .. }));
        assert!(matches!(read(200, None, r#"{"schema":2,"status":"none"}"#), Answer::Failed { .. }));
        assert!(matches!(read(200, None, r#"{"schema":1,"status":"maybe"}"#), Answer::Failed { .. }));
        assert!(matches!(read(200, None, r#"{"schema":1,"status":"found"}"#), Answer::Failed { .. }));
        assert!(matches!(read(200, None, r#"{"schema":1,"status":"found","document":{"schema":1,"timing":"line","lines":[]}}"#), Answer::Failed { .. }));
        assert!(matches!(read(200, None, r#"{"schema":1,"status":"found","document":{"schema":9,"timing":"line","lines":[{"text":"x"}]}}"#), Answer::Failed { .. }));
    }

    #[test]
    fn a_timing_the_service_claims_is_not_believed() {
        let body = r#"{"schema":1,"status":"found","document":{"schema":1,"timing":"syllable","lines":[{"start_ms":1000,"text":"Hi"}]}}"#;
        match read(200, None, body) {
            Answer::Found(list) => assert_eq!(list[0].document.timing, Timing::Line),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_pause_is_capped_and_a_date_is_not_read() {
        assert_eq!(pause(Some("999999")), Some(MAX_PAUSE));
        assert_eq!(pause(Some("Wed, 21 Oct 2026 07:28:00 GMT")), None);
        assert_eq!(pause(None), None);
    }
}
