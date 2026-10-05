//! What the screen needs from a document at a given moment.
//!
//! None of this is stored: it depends on the moment, on how far ahead the
//! screen looks and on how long a silence has to be before it is shown as
//! one. A document keeps what its source said; these functions fill in
//! what it did not, the same way every time.

use crate::model::{Document, Line, Timing};

/// How long the last line lasts when nothing says, and no line follows.
pub const LAST_LINE_MS: u32 = 6_000;

/// The last line that has started at that moment, by position.
///
/// Before the first line there is none: showing the first line from second
/// zero, when it starts at twenty, is worse than showing nothing.
pub fn current(doc: &Document, ms: u32) -> Option<usize> {
    if doc.timing == Timing::Unsynced {
        return None;
    }
    let started = doc.lines.partition_point(|l| l.start_ms.is_some_and(|s| s <= ms));
    started.checked_sub(1)
}

/// When a line stops being sung, as the screen treats it.
///
/// The end the source stated; if it stated none, the start of the next line
/// that starts later; for the last line, a few seconds, never past the end
/// of the track.
pub fn line_end(doc: &Document, index: usize, track_ms: Option<u32>) -> Option<u32> {
    let line = doc.lines.get(index)?;
    let start = line.start_ms?;
    if let Some(end) = line.end_ms {
        return Some(end);
    }
    if let Some(next) = doc.lines[index + 1..].iter().filter_map(|l| l.start_ms).find(|&s| s > start) {
        return Some(next);
    }
    let fallback = start.saturating_add(LAST_LINE_MS);
    Some(match track_ms.or(doc.duration_ms) {
        Some(track) if track > start => fallback.min(track),
        _ => fallback,
    })
}

/// The lines being sung at that moment, looking `lead_ms` ahead.
///
/// More than one when voices overlap. Looking a little ahead lets the
/// screen start moving to a line before its first word.
pub fn active(doc: &Document, ms: u32, lead_ms: u32, track_ms: Option<u32>) -> Vec<usize> {
    if doc.timing == Timing::Unsynced {
        return Vec::new();
    }
    let ahead = ms.saturating_add(lead_ms);
    let started = doc.lines.partition_point(|l| l.start_ms.is_some_and(|s| s <= ahead));
    (0..started).filter(|&i| line_end(doc, i, track_ms).is_some_and(|end| ms < end)).collect()
}

/// When a segment stops, as the screen treats it: its own end, or where the
/// next segment of the same voice starts, or where the line ends.
pub fn segment_end(line: &Line, index: usize, line_end: u32) -> u32 {
    let Some(segment) = line.segments.get(index) else { return line_end };
    if let Some(end) = segment.end_ms {
        return end;
    }
    line.segments[index + 1..]
        .iter()
        .filter(|s| s.background == segment.background && s.start_ms > segment.start_ms)
        .map(|s| s.start_ms)
        .next()
        .unwrap_or(line_end)
        .max(segment.start_ms)
}

/// How much of a segment has been sung, from 0 to 1.
pub fn progress(line: &Line, index: usize, ms: u32, line_end: u32) -> f32 {
    let Some(segment) = line.segments.get(index) else { return 0.0 };
    let end = segment_end(line, index, line_end);
    if ms <= segment.start_ms {
        return 0.0;
    }
    if ms >= end || end == segment.start_ms {
        return 1.0;
    }
    (ms - segment.start_ms) as f32 / (end - segment.start_ms) as f32
}

/// A stretch with nobody singing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Gap {
    pub start_ms: u32,
    pub end_ms: u32,
    /// The position of the line that comes after it, or the number of lines
    /// for the one at the end.
    pub before: usize,
}

/// The silences of at least `threshold_ms`: before the first line, between
/// lines whose end is known, and after the last one when the track's length
/// is known.
pub fn gaps(doc: &Document, threshold_ms: u32, track_ms: Option<u32>) -> Vec<Gap> {
    if doc.timing == Timing::Unsynced || doc.lines.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut sung_until = 0u32;
    for (i, line) in doc.lines.iter().enumerate() {
        let Some(start) = line.start_ms else { continue };
        if start >= sung_until.saturating_add(threshold_ms) {
            out.push(Gap { start_ms: sung_until, end_ms: start, before: i });
        }
        if let Some(end) = line_end(doc, i, track_ms) {
            sung_until = sung_until.max(end);
        }
    }
    if let Some(track) = track_ms.or(doc.duration_ms) {
        if track >= sung_until.saturating_add(threshold_ms) {
            out.push(Gap { start_ms: sung_until, end_ms: track, before: doc.lines.len() });
        }
    }
    out
}

/// The segments of a line grouped into words: a word ends at a segment
/// followed by a space.
pub fn words(line: &Line) -> Vec<std::ops::Range<usize>> {
    let mut out = Vec::new();
    let mut from = 0;
    for (i, segment) in line.segments.iter().enumerate() {
        let last = i + 1 == line.segments.len();
        let next_other_voice = line.segments.get(i + 1).is_some_and(|n| n.background != segment.background);
        if segment.space_after || last || next_other_voice {
            out.push(from..i + 1);
            from = i + 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lrc;

    #[test]
    fn the_current_line_is_the_last_one_that_started() {
        let l = lrc::parse("[00:05.00]One\n[00:10.00]Two\n[00:20.00]Three\n");
        assert_eq!(current(&l, 0), None, "before the first there is none");
        assert_eq!(current(&l, 4_999), None);
        assert_eq!(current(&l, 5_000), Some(0), "right as it comes in");
        assert_eq!(current(&l, 9_999), Some(0));
        assert_eq!(current(&l, 10_000), Some(1));
        assert_eq!(current(&l, 999_999), Some(2), "and the last one stays");
    }

    #[test]
    fn unsynced_lyrics_are_not_followed() {
        let l = lrc::parse("Just the text\n");
        assert_eq!(current(&l, 5_000), None);
        assert!(active(&l, 5_000, 0, None).is_empty());
    }

    #[test]
    fn a_line_without_an_end_lasts_until_the_next() {
        let l = lrc::parse("[00:05.00]One\n[00:10.00]Two\n");
        assert_eq!(line_end(&l, 0, None), Some(10_000));
        assert_eq!(line_end(&l, 1, None), Some(16_000), "the last one, a few seconds");
        assert_eq!(line_end(&l, 1, Some(12_000)), Some(12_000), "never past the track");
    }

    #[test]
    fn overlapping_voices_are_both_active() {
        let l = lrc::parse("[00:10.00]I will follow\n[00:12.00]Follow the river\n[00:16.00]\n");
        let mut doc = l.clone();
        doc.lines[0].end_ms = Some(15_000);
        assert_eq!(active(&doc, 13_000, 0, None), vec![0, 1]);
        assert_eq!(active(&doc, 15_500, 0, None), vec![1]);
        assert_eq!(active(&doc, 9_700, 500, None), vec![0], "it is seen coming");
    }

    #[test]
    fn a_word_fills_in_its_own_time() {
        let l = lrc::parse("[00:01.00]<00:01.00>Hello <00:02.00>world<00:04.00>\n");
        let line = &l.lines[0];
        let end = line_end(&l, 0, None).unwrap();
        assert_eq!(progress(line, 0, 1_500, end), 0.5, "halfway through until the next word");
        assert_eq!(progress(line, 1, 3_000, end), 0.5);
        assert_eq!(progress(line, 1, 900, end), 0.0);
        assert_eq!(progress(line, 1, 9_000, end), 1.0);
    }

    #[test]
    fn silences_are_found_where_the_ends_are_known() {
        let l = lrc::parse("[00:20.00]One\n[00:25.00]\n[00:40.00]Two\n");
        let found = gaps(&l, 5_000, Some(60_000));
        let spans: Vec<(u32, u32, usize)> = found.iter().map(|g| (g.start_ms, g.end_ms, g.before)).collect();
        assert_eq!(spans, vec![(0, 20_000, 0), (25_000, 40_000, 1), (46_000, 60_000, 2)]);
    }

    #[test]
    fn segments_are_grouped_into_words() {
        let l = lrc::parse("[00:01.00]<00:01.00>Hel<00:01.20>lo <00:01.50>you\n");
        assert_eq!(words(&l.lines[0]), vec![0..2, 2..3]);
    }
}
