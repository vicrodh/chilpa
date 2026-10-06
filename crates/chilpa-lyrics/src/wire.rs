//! What Chilpa and a source plugin say to each other about lyrics.
//!
//! One call: `track.lyrics`, to a plugin that plays music, with the
//! reference of one of its own tracks. Its answer is that track's lyrics,
//! `{"status":"found",…}`, or `{"status":"none"}`. A failure is a JSON-RPC
//! error, never "none": "none" is remembered for days, a failure is asked
//! again.
//!
//! A plugin is never asked to find lyrics by name. Lyrics for a track that
//! has none of its own come from the lyrics service ([`crate::service`]).
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

/// Some lyrics with what they claim to be for, and what was sent, as it
/// came, for the cache.
#[derive(Clone, PartialEq, Debug)]
pub struct Candidate {
    pub claim: Claim,
    pub document: Document,
    /// The format and the text, when text was sent.
    pub raw: Option<(String, String)>,
}

/// The parameters of `track.lyrics`.
pub fn source_params(reference: &str) -> Value {
    json!({ "ref": reference })
}

/// Reads the answer to `track.lyrics`. `Ok(None)` is "none".
///
/// The claim is the track itself plus what the plugin knows about it
/// (`fingerprint`): the lyrics are for that track, whatever their names say.
pub fn read_source(answer: &Value, track: &Track) -> Result<Option<Candidate>, String> {
    match answer.get("status").and_then(Value::as_str) {
        Some("none") => Ok(None),
        Some("found") => {
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
        other => Err(format!("unknown status {other:?}")),
    }
}

/// The wait a failed answer asks for, from the error's `data`.
pub fn retry_after_ms(error: &Value) -> Option<u64> {
    error.get("data").and_then(|d| d.get("retry_after_ms")).and_then(Value::as_u64)
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

    fn lyrics(value: Value) -> Result<Option<Candidate>, String> {
        read_source(&json!({"status": "found", "lyrics": value}), &track())
    }

    #[test]
    fn lyrics_are_read_in_every_format() {
        let lines = lyrics(json!({"format": "lrc", "text": "[00:01.00]Hi"})).unwrap().unwrap();
        assert_eq!(lines.document.timing, Timing::Line);
        assert_eq!(lines.raw.as_ref().map(|(f, _)| f.as_str()), Some("lrc"));

        let plain = lyrics(json!({"format": "plain", "text": "Hi\n[00:01.00]not a stamp"})).unwrap().unwrap();
        assert_eq!(plain.document.timing, Timing::Unsynced, "plain is believed to be plain");

        let document = lyrics(json!({"format": "document", "document": {"schema": 1, "timing": "syllable", "lines": [{"start_ms": 1000, "text": "Hi"}]}})).unwrap().unwrap();
        assert_eq!(document.document.timing, Timing::Line, "the timing is read from the lines, not from the claim");
        assert_eq!(document.raw, None);

        assert!(lyrics(json!({"format": "midi", "text": "?"})).is_err(), "a format nobody reads is a failure, not none");
        assert!(lyrics(json!({"format": "lrc", "text": "\n\n"})).is_err(), "and so are lyrics with nothing in them");
    }

    #[test]
    fn a_document_keeps_its_words_its_translation_and_its_credits() {
        // The shape a source sends for lyrics timed by word in a format of
        // its own, as the plugin documentation describes it.
        let found = lyrics(json!({"format": "document", "document": {
            "schema": 1, "timing": "word", "language": "en",
            "lines": [
                {"start_ms": 1750, "end_ms": 4770, "text": "Oh, mm-mm", "segments": [
                    {"text": "Oh,", "start_ms": 1750, "end_ms": 2480, "space_after": true},
                    {"text": "mm-mm", "start_ms": 2480, "end_ms": 4770, "space_after": false}]},
                {"start_ms": 6000, "text": "Second line"}],
            "layers": [{"kind": "translation", "language": "es", "lines": [{"line": 0, "text": "Oh, mm-mm"}, {"line": 1, "text": "Segunda"}]}],
            "credits": [{"name": "A Writer", "role": "songwriter"}]}}))
        .unwrap()
        .unwrap();
        let document = found.document;
        assert_eq!(document.timing, Timing::Word);
        assert_eq!(document.language.as_deref(), Some("en"));
        assert_eq!(document.lines[0].segments.len(), 2);
        assert_eq!(document.lines[0].segments[1].end_ms, Some(4_770));
        assert_eq!(document.lines[1].end_ms, None, "an end nobody stated stays unknown");
        assert_eq!(document.layers[0].lines.len(), 2);
        assert_eq!(document.credits[0].name, "A Writer");
    }

    #[test]
    fn none_and_failures_are_different_things() {
        assert_eq!(read_source(&json!({"status": "none"}), &track()).unwrap(), None);
        assert!(read_source(&json!({"status": "maybe"}), &track()).is_err());
        assert!(read_source(&json!({"status": "found"}), &track()).is_err());
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
        assert!(lyrics(json!({"format": "plain", "text": huge})).is_err());
    }
}
