//! What Chilpa and a plugin say to each other about lyrics.
//!
//! Two calls, one shape of answer:
//!
//! - `lyrics.find`, to a plugin whose job is lyrics: the identity of a track
//!   and a few ways of writing its names. It answers with candidates, each
//!   saying what song it is, and Chilpa judges them. A plugin never decides
//!   that its result is the right song.
//! - `track.lyrics`, to a plugin that plays music: the reference of one of
//!   its own tracks. Its answer is that track's lyrics.
//!
//! An answer is `{"status":"found",…}` or `{"status":"none"}`. A failure is
//! a JSON-RPC error, never "none": "none" is remembered for days, a failure
//! is asked again.
//!
//! Lyrics travel in an open format Chilpa reads itself (`lrc`, `ttml`,
//! `plain`) or already as a [`Document`]. A format only one service uses is
//! turned into a document inside that service's plugin.

use crate::matching::{Claim, Track};
use crate::model::Document;
use crate::{lrc, ttml};
use serde::Deserialize;
use serde_json::{json, Value};

/// The largest text of lyrics read from a plugin.
pub const MAX_TEXT: usize = 512 * 1024;
/// The most candidates read from one answer.
pub const MAX_CANDIDATES: usize = 20;

/// A candidate: what it claims to be, the lyrics, and what was sent, as it
/// came, for the cache.
#[derive(Clone, PartialEq, Debug)]
pub struct Candidate {
    pub claim: Claim,
    pub document: Document,
    /// The format and the text, when the plugin sent text.
    pub raw: Option<(String, String)>,
}

/// The parameters of `lyrics.find`.
///
/// `variants` are other ways of writing the names, tried in order by the
/// plugin until one finds something: the title without what is in brackets
/// and the first artist alone. Chilpa writes them once so every plugin tries
/// the same ones.
pub fn find_params(track: &Track) -> Value {
    let mut variants: Vec<(String, String)> = Vec::new();
    let mut push = |title: String, artist: String| {
        let same = |a: &str, b: &str| a.trim().eq_ignore_ascii_case(b.trim());
        if !title.trim().is_empty() && !variants.iter().any(|(t, a)| same(t, &title) && same(a, &artist)) {
            variants.push((title, artist));
        }
    };
    push(track.title.clone(), track.artist.clone());
    let bare_title = without_brackets(&track.title);
    push(bare_title.clone(), track.artist.clone());
    push(bare_title, first_artist(&track.artist));
    variants.truncate(3);
    json!({
        "title": track.title,
        "artist": track.artist,
        "album": track.album,
        "duration_ms": track.duration_ms,
        "isrc": track.isrc,
        "variants": variants.iter().map(|(t, a)| json!({"title": t, "artist": a})).collect::<Vec<_>>(),
    })
}

/// The parameters of `track.lyrics`.
pub fn source_params(reference: &str) -> Value {
    json!({ "ref": reference })
}

/// Reads the answer to `lyrics.find`. `Ok(None)` is "none".
pub fn read_find(answer: &Value) -> Result<Option<Vec<Candidate>>, String> {
    match status(answer)? {
        Status::None => Ok(None),
        Status::Found => {
            let list = answer.get("candidates").and_then(Value::as_array).ok_or("found without candidates")?;
            let candidates: Vec<Candidate> = list.iter().take(MAX_CANDIDATES).filter_map(candidate).collect();
            if candidates.is_empty() {
                return Err("no candidate could be read".into());
            }
            Ok(Some(candidates))
        }
    }
}

/// Reads the answer to `track.lyrics`. `Ok(None)` is "none".
///
/// The claim is the track itself plus what the plugin knows about it
/// (`fingerprint`): the lyrics are for that track, whatever their names say.
pub fn read_source(answer: &Value, track: &Track) -> Result<Option<Candidate>, String> {
    match status(answer)? {
        Status::None => Ok(None),
        Status::Found => {
            let lyrics = answer.get("lyrics").ok_or("found without lyrics")?;
            let (document, raw) = lyrics_of(lyrics).ok_or("the lyrics could not be read")?;
            let fingerprint = answer.get("fingerprint");
            let claim = Claim {
                title: Some(track.title.clone()),
                artist: Some(track.artist.clone()),
                album: track.album.clone(),
                duration_ms: fingerprint
                    .and_then(|f| f.get("duration_ms"))
                    .and_then(Value::as_u64)
                    .map(|d| d.min(u32::MAX as u64) as u32)
                    .or(track.duration_ms),
                isrc: fingerprint.and_then(|f| f.get("isrc")).and_then(Value::as_str).map(str::to_string).or(track.isrc.clone()),
            };
            Ok(Some(Candidate { claim, document, raw }))
        }
    }
}

/// The wait a failed answer asks for, from the error's `data`.
pub fn retry_after_ms(error: &Value) -> Option<u64> {
    error.get("data").and_then(|d| d.get("retry_after_ms")).and_then(Value::as_u64)
}

enum Status {
    Found,
    None,
}

fn status(answer: &Value) -> Result<Status, String> {
    match answer.get("status").and_then(Value::as_str) {
        Some("found") => Ok(Status::Found),
        Some("none") => Ok(Status::None),
        other => Err(format!("unknown status {other:?}")),
    }
}

fn candidate(value: &Value) -> Option<Candidate> {
    let text = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_string).filter(|s| !s.trim().is_empty());
    let (document, raw) = lyrics_of(value.get("lyrics")?)?;
    let claim = Claim {
        title: text("title"),
        artist: text("artist"),
        album: text("album"),
        duration_ms: value.get("duration_ms").and_then(Value::as_u64).map(|d| d.min(u32::MAX as u64) as u32),
        isrc: text("isrc"),
    };
    Some(Candidate { claim, document, raw })
}

#[derive(Deserialize)]
#[serde(tag = "format", rename_all = "lowercase")]
enum WireLyrics {
    Lrc { text: String },
    Ttml { text: String },
    Plain { text: String },
    Document { document: Document },
}

/// A document from the lyrics a plugin sent, and the text as it came.
fn lyrics_of(value: &Value) -> Option<(Document, Option<(String, String)>)> {
    let lyrics: WireLyrics = serde_json::from_value(value.clone()).ok()?;
    let (document, raw) = match lyrics {
        WireLyrics::Lrc { text } if text.len() <= MAX_TEXT => (lrc::parse(&text), Some(("lrc".to_string(), text))),
        WireLyrics::Ttml { text } if text.len() <= MAX_TEXT => (ttml::parse(&text)?, Some(("ttml".to_string(), text))),
        WireLyrics::Plain { text } if text.len() <= MAX_TEXT => {
            // Plain text is read as LRC with no stamps, but stamps in it are
            // not believed: the plugin said it was plain.
            let mut doc = lrc::parse(&text);
            if doc.timing > crate::model::Timing::Unsynced {
                doc = Document {
                    lines: text
                        .lines()
                        .map(str::trim)
                        .filter(|l| !l.is_empty())
                        .map(|l| crate::model::Line { text: l.to_string(), ..Default::default() })
                        .collect(),
                    ..Document::new()
                }
                .finish();
            }
            (doc, Some(("plain".to_string(), text)))
        }
        WireLyrics::Document { document } => (document.finish(), None),
        _ => return None,
    };
    (!document.is_empty()).then_some((document, raw))
}

fn without_brackets(title: &str) -> String {
    let mut out = String::new();
    let mut depth = 0u32;
    for c in title.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn first_artist(artist: &str) -> String {
    let lower = artist.to_lowercase();
    let cut = [" feat.", " feat ", " ft.", " featuring ", " & ", ", ", " x "]
        .iter()
        .filter_map(|s| lower.find(s))
        .filter(|&i| artist.is_char_boundary(i))
        .min()
        .unwrap_or(artist.len());
    artist[..cut].trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Timing;

    fn track() -> Track {
        Track {
            title: "Song (Remastered)".into(),
            artist: "Someone feat. Other".into(),
            album: None,
            duration_ms: Some(200_000),
            isrc: None,
            mbid: None,
        }
    }

    #[test]
    fn variants_are_written_once_and_without_repeats() {
        let params = find_params(&track());
        let variants: Vec<(String, String)> = params["variants"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| (v["title"].as_str().unwrap().to_string(), v["artist"].as_str().unwrap().to_string()))
            .collect();
        assert_eq!(
            variants,
            vec![
                ("Song (Remastered)".to_string(), "Someone feat. Other".to_string()),
                ("Song".to_string(), "Someone feat. Other".to_string()),
                ("Song".to_string(), "Someone".to_string()),
            ]
        );
        let plain = Track { title: "Song".into(), artist: "Someone".into(), ..track() };
        assert_eq!(find_params(&plain)["variants"].as_array().unwrap().len(), 1, "no variant repeats another");
        assert_eq!(params["duration_ms"], 200_000);
    }

    #[test]
    fn candidates_are_read_in_every_format() {
        let answer = json!({"status": "found", "candidates": [
            {"title": "Song", "artist": "Someone", "duration_ms": 200100, "lyrics": {"format": "lrc", "text": "[00:01.00]Hi"}},
            {"title": "Song", "artist": "Someone", "lyrics": {"format": "plain", "text": "Hi\n[00:01.00]not a stamp"}},
            {"title": "Song", "artist": "Someone", "lyrics": {"format": "document", "document": {
                "schema": 1, "timing": "syllable", "lines": [{"start_ms": 1000, "text": "Hi"}]}}},
            {"title": "Broken", "lyrics": {"format": "midi", "text": "?"}}
        ]});
        let found = read_find(&answer).unwrap().unwrap();
        assert_eq!(found.len(), 3, "what cannot be read is left out");
        assert_eq!(found[0].document.timing, Timing::Line);
        assert_eq!(found[0].claim.duration_ms, Some(200_100));
        assert_eq!(found[0].raw.as_ref().map(|(f, _)| f.as_str()), Some("lrc"));
        assert_eq!(found[1].document.timing, Timing::Unsynced, "plain is believed to be plain");
        assert_eq!(found[2].document.timing, Timing::Line, "the timing is read from the lines, not from the claim");
    }

    #[test]
    fn none_and_failures_are_different_things() {
        assert_eq!(read_find(&json!({"status": "none"})).unwrap(), None);
        assert!(read_find(&json!({"status": "maybe"})).is_err());
        assert!(read_find(&json!({"status": "found", "candidates": []})).is_err());
        assert_eq!(retry_after_ms(&json!({"code": -32000, "message": "busy", "data": {"retry_after_ms": 1500}})), Some(1_500));
    }

    #[test]
    fn a_source_answer_is_for_its_own_track() {
        let answer = json!({"status": "found",
            "lyrics": {"format": "lrc", "text": "[00:01.00]Hi"},
            "fingerprint": {"duration_ms": 199800, "isrc": "USXXX0000001"}});
        let found = read_source(&answer, &track()).unwrap().unwrap();
        assert_eq!(found.claim.title.as_deref(), Some("Song (Remastered)"));
        assert_eq!(found.claim.isrc.as_deref(), Some("USXXX0000001"));
        assert_eq!(found.claim.duration_ms, Some(199_800));
    }

    #[test]
    fn oversized_lyrics_are_not_read() {
        let huge = "a".repeat(MAX_TEXT + 1);
        let answer = json!({"status": "found", "candidates": [{"title": "S", "artist": "A", "lyrics": {"format": "plain", "text": huge}}]});
        assert!(read_find(&answer).is_err());
    }
}
