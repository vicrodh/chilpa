//! One shape for a track's lyrics, whoever supplied them.
//!
//! A file next to the track, the tags, a source that has lyrics for its own
//! tracks and the lyrics service all end up here. Three rules hold it:
//!
//! - **It keeps what the source said, and nothing it did not.** An end time
//!   the source did not state stays `None`; whoever draws the lyrics decides
//!   how long a line lasts. Saving an invented end would make it look stated
//!   the next time the document is read.
//! - **No sentinel values.** Unknown is `None`, never zero. A zero-length
//!   word, an unknown end and "this line has no time" are three different
//!   things and the model says which.
//! - **What only the screen needs is derived, not stored.** Instrumental
//!   gaps, the progress of a word and which lines are active depend on the
//!   moment and on a threshold; they live in [`crate::timeline`].
//!
//! The same types travel on the wire (a plugin may answer with a document)
//! and into the cache, so field names are the JSON names.

use serde::{Deserialize, Serialize};

/// The version of this shape. A document of another version is not read as
/// this one: the cache throws it away and asks again.
pub const SCHEMA: u16 = 1;

/// How finely a document follows the music, from least to most.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Timing {
    /// Text only. Shown whole.
    Unsynced,
    /// Each line has its moment.
    Line,
    /// Each word has its moment.
    Word,
    /// Pieces smaller than a word have their moment.
    Syllable,
}

/// A track's lyrics.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct Document {
    pub schema: u16,
    /// The finest timing any line reaches. Computed by [`Document::finish`]
    /// from the lines themselves, never taken from what a provider claims.
    pub timing: Timing,
    /// The language sung, as the source states it (BCP 47).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// The length of the audio the timings were made against, when stated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u32>,
    /// What the lyrics say about themselves (LRC `[ti:]` and `[ar:]`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    /// Who sings. Empty means one voice nobody named.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agents: Vec<Agent>,
    /// Timed documents keep them in order of start; unsynced ones in the
    /// order they were written.
    pub lines: Vec<Line>,
    /// Translations and romanizations, each with its own origin.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layers: Vec<Layer>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub credits: Vec<Credit>,
    /// Who gave the document, when it says so itself. The lyrics service
    /// does; a file and a source plugin are known by where they came from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Provenance>,
}

/// Who gave a document, and the recording they gave it for.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Provenance {
    /// The provider's id where the document was put together.
    pub provider: String,
    /// What the screen shows.
    pub name: String,
    /// What the provider said the lyrics are for, to judge them again here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claim: Option<crate::matching::Claim>,
}

impl Default for Timing {
    fn default() -> Self {
        Timing::Unsynced
    }
}

/// Someone who sings.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Agent {
    pub kind: AgentKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentKind {
    Person,
    Group,
    Other,
}

/// One line as it is shown.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct Line {
    /// Milliseconds from the start of the track. `None` only in unsynced
    /// documents.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_ms: Option<u32>,
    /// `None` when the source did not say when the line ends.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_ms: Option<u32>,
    /// Position in [`Document::agents`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<u32>,
    /// The part of the song, when the source names it ("Verse", "Chorus").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    /// The lead vocal, as displayed and copied.
    pub text: String,
    /// A backing vocal that has no timing of its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    /// Empty for a line that is timed as a whole, or not at all.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub segments: Vec<Segment>,
}

/// A word or a syllable: whatever unit the source timed.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Segment {
    /// Without the space around it; [`Segment::space_after`] says whether
    /// one follows. That way syllables, words and scripts written without
    /// spaces have one representation.
    pub text: String,
    pub start_ms: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_ms: Option<u32>,
    /// False when the next segment continues this word.
    pub space_after: bool,
    /// Sung by the backing voices, not the lead.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub background: bool,
}

/// Lines in another language or another script, laid over the document.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Layer {
    pub kind: LayerKind,
    /// BCP 47: "es", "en", "ja-Latn".
    pub language: String,
    /// Who supplied it, when it is not whoever supplied the document.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    pub lines: Vec<LayerLine>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LayerKind {
    Translation,
    Romanization,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct LayerLine {
    /// Position in [`Document::lines`].
    pub line: u32,
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub segments: Vec<Segment>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Credit {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<CreditRole>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CreditRole {
    Lyricist,
    Composer,
    Songwriter,
}

impl Document {
    /// An empty document of the current schema.
    pub fn new() -> Self {
        Document { schema: SCHEMA, ..Document::default() }
    }

    pub fn is_empty(&self) -> bool {
        self.lines.iter().all(|l| l.text.trim().is_empty() && l.segments.is_empty())
    }

    /// The lyrics as plain text, one line each.
    pub fn plain(&self) -> String {
        self.lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n")
    }

    /// Puts a document that came from anywhere into the shape the rest of
    /// Chilpa can trust.
    ///
    /// - A line that ends before it starts is closed at its start, and so is
    ///   a segment: a negative length cannot be drawn.
    /// - Timed lines are put in order of start. Lines that overlap stay
    ///   overlapping: two voices may sing at once.
    /// - Segments are put in order of start, and a line's text is rebuilt
    ///   from its lead segments, so what is highlighted and what is copied
    ///   are the same words.
    /// - Unsynced means no line has a start, never "every line starts at 0".
    /// - The timing is read from the lines, not from anyone's claim.
    /// - Layers that point past the last line are dropped.
    pub fn finish(mut self) -> Self {
        self.schema = SCHEMA;
        for line in &mut self.lines {
            for segment in &mut line.segments {
                if let Some(end) = segment.end_ms {
                    segment.end_ms = Some(end.max(segment.start_ms));
                }
            }
            line.segments.sort_by_key(|s| s.start_ms);
            if line.start_ms.is_none() {
                line.start_ms = line.segments.first().map(|s| s.start_ms);
            }
            if let (Some(start), Some(end)) = (line.start_ms, line.end_ms) {
                line.end_ms = Some(end.max(start));
            }
            if line.segments.iter().any(|s| !s.background) {
                line.text = join(line.segments.iter().filter(|s| !s.background));
                let backing: Vec<&Segment> = line.segments.iter().filter(|s| s.background).collect();
                if !backing.is_empty() {
                    line.background = Some(join(backing.into_iter()));
                }
            }
        }
        let timed = self.lines.iter().any(|l| l.start_ms.is_some());
        if timed {
            // A line without a time inside timed lyrics cannot be placed; it
            // is dropped instead of being shown at second zero.
            self.lines.retain(|l| l.start_ms.is_some());
            self.lines.sort_by_key(|l| l.start_ms);
        }
        if timed && self.lines.iter().all(|l| l.start_ms == Some(0)) && self.lines.len() > 1 {
            for line in &mut self.lines {
                line.start_ms = None;
                line.end_ms = None;
                line.segments.clear();
            }
        }
        let count = self.lines.len() as u32;
        for layer in &mut self.layers {
            layer.lines.retain(|l| l.line < count);
        }
        self.layers.retain(|l| !l.lines.is_empty());
        self.timing = timing_of(&self.lines);
        self
    }
}

/// Rebuilds the text of some segments with the spaces they declare.
fn join<'a>(segments: impl Iterator<Item = &'a Segment>) -> String {
    let mut text = String::new();
    let mut pending_space = false;
    for segment in segments {
        if pending_space && !text.is_empty() {
            text.push(' ');
        }
        text.push_str(&segment.text);
        pending_space = segment.space_after;
    }
    text
}

/// The finest timing of some lines.
///
/// Segments that split a word mean syllables, but only in a line written
/// with spaces: in Japanese or Chinese every character is its own segment
/// with no space after it, and that is word timing.
fn timing_of(lines: &[Line]) -> Timing {
    let mut timing = Timing::Unsynced;
    for line in lines {
        let here = if line.segments.is_empty() {
            if line.start_ms.is_some() { Timing::Line } else { Timing::Unsynced }
        } else if line.text.contains(' ') && {
            let lead: Vec<&Segment> = line.segments.iter().filter(|s| !s.background).collect();
            lead.iter().rev().skip(1).any(|s| !s.space_after)
        } {
            Timing::Syllable
        } else {
            Timing::Word
        };
        timing = timing.max(here);
    }
    timing
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segment(text: &str, start: u32, end: Option<u32>, space_after: bool) -> Segment {
        Segment { text: text.into(), start_ms: start, end_ms: end, space_after, background: false }
    }

    #[test]
    fn the_timing_is_read_from_the_lines() {
        let plain = Document { lines: vec![Line { text: "Hello".into(), ..Line::default() }], ..Document::new() }.finish();
        assert_eq!(plain.timing, Timing::Unsynced);

        let line = Document {
            lines: vec![Line { start_ms: Some(1_000), text: "Hello".into(), ..Line::default() }],
            ..Document::new()
        }
        .finish();
        assert_eq!(line.timing, Timing::Line);

        let words = Document {
            lines: vec![Line {
                segments: vec![segment("Hello", 1_000, Some(1_500), true), segment("world", 1_500, None, false)],
                ..Line::default()
            }],
            ..Document::new()
        }
        .finish();
        assert_eq!(words.timing, Timing::Word);
        assert_eq!(words.lines[0].text, "Hello world", "the text is rebuilt from the words");
        assert_eq!(words.lines[0].start_ms, Some(1_000), "a line starts with its first word");

        let syllables = Document {
            lines: vec![Line {
                segments: vec![
                    segment("Hel", 1_000, Some(1_200), false),
                    segment("lo", 1_200, Some(1_500), true),
                    segment("world", 1_500, Some(2_000), false),
                ],
                ..Line::default()
            }],
            ..Document::new()
        }
        .finish();
        assert_eq!(syllables.timing, Timing::Syllable);
        assert_eq!(syllables.lines[0].text, "Hello world");
    }

    #[test]
    fn characters_without_spaces_are_words_not_syllables() {
        let doc = Document {
            lines: vec![Line {
                segments: vec![segment("君", 0, Some(400), false), segment("の", 400, Some(800), false), segment("名前", 800, None, false)],
                ..Line::default()
            }],
            ..Document::new()
        }
        .finish();
        assert_eq!(doc.timing, Timing::Word);
        assert_eq!(doc.lines[0].text, "君の名前");
    }

    #[test]
    fn a_negative_length_is_closed_at_the_start() {
        let doc = Document {
            lines: vec![Line {
                start_ms: Some(5_000),
                end_ms: Some(4_000),
                segments: vec![segment("Back", 5_000, Some(4_500), false)],
                ..Line::default()
            }],
            ..Document::new()
        }
        .finish();
        assert_eq!(doc.lines[0].end_ms, Some(5_000));
        assert_eq!(doc.lines[0].segments[0].end_ms, Some(5_000));
    }

    #[test]
    fn every_line_at_zero_is_unsynced_lyrics() {
        let doc = Document {
            lines: vec![
                Line { start_ms: Some(0), text: "One".into(), ..Line::default() },
                Line { start_ms: Some(0), text: "Two".into(), ..Line::default() },
            ],
            ..Document::new()
        }
        .finish();
        assert_eq!(doc.timing, Timing::Unsynced);
        assert!(doc.lines.iter().all(|l| l.start_ms.is_none()));
        assert_eq!(doc.plain(), "One\nTwo", "and the order is the one written");
    }

    #[test]
    fn backing_vocals_keep_their_own_text() {
        let mut backing = segment("(oh)", 1_200, Some(1_400), false);
        backing.background = true;
        let doc = Document {
            lines: vec![Line {
                segments: vec![segment("Stay", 1_000, Some(1_300), false), backing],
                ..Line::default()
            }],
            ..Document::new()
        }
        .finish();
        assert_eq!(doc.lines[0].text, "Stay");
        assert_eq!(doc.lines[0].background.as_deref(), Some("(oh)"));
    }

    #[test]
    fn a_layer_cannot_point_past_the_lyrics() {
        let doc = Document {
            lines: vec![Line { start_ms: Some(0), text: "One".into(), ..Line::default() }],
            layers: vec![Layer {
                kind: LayerKind::Translation,
                language: "es".into(),
                provider: None,
                lines: vec![
                    LayerLine { line: 0, text: "Uno".into(), segments: vec![] },
                    LayerLine { line: 7, text: "Siete".into(), segments: vec![] },
                ],
            }],
            ..Document::new()
        }
        .finish();
        assert_eq!(doc.layers[0].lines.len(), 1);
    }

    #[test]
    fn the_wire_names_are_the_field_names() {
        let doc = Document {
            lines: vec![Line { start_ms: Some(1_000), text: "Hi".into(), ..Line::default() }],
            ..Document::new()
        }
        .finish();
        let json = serde_json::to_value(&doc).unwrap();
        assert_eq!(json["timing"], "line");
        assert_eq!(json["lines"][0]["start_ms"], 1_000);
        assert!(json["lines"][0].get("end_ms").is_none(), "an unknown end is left out, not zero");
        let back: Document = serde_json::from_value(json).unwrap();
        assert_eq!(back, doc);
    }
}
