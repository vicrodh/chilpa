//! The LRC format, plain and enhanced.
//!
//! A text file where each line starts with one or more timestamps:
//! `[01:23.45]the words`. What is looked after here:
//!
//! - **A line can carry several stamps.** A refrain is written once with
//!   all its times; taking only the first loses the rest of the song, and
//!   joining them makes one line that lasts a minute.
//! - **Stamps come in every shape**: `mm:ss`, `mm:ss.x`, `mm:ss.xx` and
//!   `mm:ss.xxx`. Reading `.5` as 5 ms instead of 500 leaves the lyrics half
//!   a second early.
//! - **Enhanced LRC times each word** with `<mm:ss.xx>` inside the line.
//!   Those stamps are timing, not text: left in, they show on screen.
//! - **A stamp with no words ends the voice.** It closes the line before
//!   it; it is not a line of its own.
//! - **Header tags** (`[ti:]`, `[ar:]`, `[offset:]`, `[length:]`) are not
//!   lyric lines and must not show on screen.
//! - **A file without a single stamp is still lyrics**, just unsynced.

use crate::model::{Document, Line, Segment};

/// Reads LRC lyrics, or plain text when there are no stamps.
pub fn parse(text: &str) -> Document {
    let mut stamped: Vec<(u32, Option<Line>)> = Vec::new();
    let mut loose: Vec<String> = Vec::new();
    let mut offset_ms: i64 = 0;
    let mut doc = Document::new();

    for raw in text.lines() {
        let line = raw.trim_end_matches('\r');
        let (labels, rest) = labels_of(line);
        if labels.is_empty() {
            match tag(line) {
                Some(("offset", v)) => offset_ms = v.trim().trim_start_matches('+').parse().unwrap_or(0),
                Some(("ti", v)) => doc.title = Some(v.trim().to_string()).filter(|t| !t.is_empty()),
                Some(("ar", v)) => doc.artist = Some(v.trim().to_string()).filter(|t| !t.is_empty()),
                Some(("length", v)) => doc.duration_ms = time(v.trim()),
                // Any other known tag stays quiet; it is not lyrics.
                Some(_) => {}
                None => {
                    if !line.trim().is_empty() {
                        loose.push(strip_word_stamps(line));
                    }
                }
            }
            continue;
        }
        let first = labels[0];
        for at in labels {
            // Word stamps are absolute. On a repeated line they belong to
            // its first time; the others are the same words moved along.
            let shift = at as i64 - first as i64;
            stamped.push((at, words(rest, at, shift)));
        }
    }

    if stamped.is_empty() {
        doc.lines = loose.into_iter().map(|t| Line { text: t, ..Line::default() }).collect();
        return doc.finish();
    }

    // The file's offset moves the whole lyric. It is positive when it has to
    // come earlier, so it is subtracted.
    let shift = |ms: u32| (ms as i64 - offset_ms).max(0) as u32;
    stamped.sort_by_key(|(at, _)| *at);
    let mut lines: Vec<Line> = Vec::new();
    for (at, line) in stamped {
        let at = shift(at);
        match line {
            Some(mut line) => {
                line.start_ms = Some(at);
                line.end_ms = line.end_ms.map(shift);
                for segment in &mut line.segments {
                    segment.start_ms = shift(segment.start_ms);
                    segment.end_ms = segment.end_ms.map(shift);
                }
                lines.push(line);
            }
            None => {
                if let Some(previous) = lines.last_mut() {
                    if previous.end_ms.is_none() {
                        previous.end_ms = Some(at);
                    }
                }
            }
        }
    }
    doc.lines = lines;
    doc.finish()
}

/// The words of a stamped line, timed when the line carries `<mm:ss.xx>`.
///
/// Returns `None` for a line with no words, which ends the voice before it.
fn words(rest: &str, line_start: u32, shift: i64) -> Option<Line> {
    let moved = |ms: u32| (ms as i64 + shift).max(0) as u32;
    let mut chunks: Vec<(Option<u32>, &str)> = Vec::new();
    let mut text = rest;
    let mut current: Option<u32> = None;
    while let Some((before, ms, after)) = next_word_stamp(text) {
        chunks.push((current, before));
        current = Some(ms);
        text = after;
    }
    chunks.push((current, text));
    if chunks.iter().all(|(stamp, _)| stamp.is_none()) {
        let text = rest.trim();
        return (!text.is_empty()).then(|| Line { text: text.to_string(), ..Line::default() });
    }

    let mut line = Line::default();
    let count = chunks.len();
    for (i, (stamp, chunk)) in chunks.iter().enumerate() {
        let words = chunk.trim();
        if words.is_empty() {
            // A stamp followed by nothing closes the word before it, and
            // at the very end, the line.
            if let Some(last) = line.segments.last_mut() {
                if let Some(ms) = stamp {
                    last.end_ms = Some(moved(*ms));
                    if i + 1 == count {
                        line.end_ms = Some(moved(*ms));
                    }
                }
                last.space_after |= chunk.chars().any(char::is_whitespace);
            }
            continue;
        }
        if let Some(last) = line.segments.last_mut() {
            last.space_after |= chunk.starts_with(char::is_whitespace);
        }
        let start = stamp.map(moved).unwrap_or(line_start);
        line.segments.push(Segment {
            text: words.to_string(),
            start_ms: start,
            end_ms: None,
            space_after: chunk.ends_with(char::is_whitespace),
            background: false,
        });
    }
    if let Some(last) = line.segments.last_mut() {
        last.space_after = false;
    }
    (!line.segments.is_empty()).then_some(line)
}

/// The next `<mm:ss.xx>` in a line: the text before it, its time and the
/// text after it.
fn next_word_stamp(text: &str) -> Option<(&str, u32, &str)> {
    let mut from = 0;
    while let Some(open) = text[from..].find('<').map(|i| i + from) {
        if let Some(close) = text[open..].find('>').map(|i| i + open) {
            if let Some(ms) = time(&text[open + 1..close]) {
                return Some((&text[..open], ms, &text[close + 1..]));
            }
        }
        from = open + 1;
    }
    None
}

/// A line with its word stamps taken out, for lyrics shown without them.
fn strip_word_stamps(line: &str) -> String {
    let mut out = String::new();
    let mut text = line;
    while let Some((before, _, after)) = next_word_stamp(text) {
        out.push_str(before);
        text = after;
    }
    out.push_str(text);
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The stamps a line starts with, and what is left after them.
fn labels_of(line: &str) -> (Vec<u32>, &str) {
    let mut labels = Vec::new();
    let mut rest = line;
    loop {
        let trimmed = rest.trim_start();
        let Some(close) = trimmed.find(']') else { break };
        if !trimmed.starts_with('[') {
            break;
        }
        let Some(ms) = time(&trimmed[1..close]) else { break };
        labels.push(ms);
        rest = &trimmed[close + 1..];
    }
    (labels, rest)
}

/// `mm:ss`, `mm:ss.x`, `mm:ss.xx` or `mm:ss.xxx`, in milliseconds.
pub(crate) fn time(s: &str) -> Option<u32> {
    let (min, rest) = s.split_once(':')?;
    let minutes: u32 = min.trim().parse().ok()?;
    let (seconds, fraction) = match rest.split_once(['.', ':']) {
        Some((a, b)) => (a, Some(b)),
        None => (rest, None),
    };
    let seconds: u32 = seconds.trim().parse().ok()?;
    let millis = match fraction {
        None => 0,
        Some(f) => {
            let f = f.trim();
            if f.is_empty() || !f.chars().all(|c| c.is_ascii_digit()) {
                return None;
            }
            // Only the first three digits count: past a millisecond there
            // is nothing to show.
            let digits = &f[..f.len().min(3)];
            let n: u32 = digits.parse().ok()?;
            match digits.len() {
                1 => n * 100,
                2 => n * 10,
                _ => n,
            }
        }
    };
    minutes.checked_mul(60_000)?.checked_add(seconds.checked_mul(1_000)?)?.checked_add(millis)
}

/// A header `[key:value]`, if that is what the line is.
fn tag(line: &str) -> Option<(&str, &str)> {
    let l = line.trim();
    let inside = l.strip_prefix('[')?.strip_suffix(']')?;
    let (key, value) = inside.split_once(':')?;
    let key = key.trim();
    // Letters only: that way `[01:23]` does not pass for a tag.
    if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    Some((key, value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Timing;

    fn starts(doc: &Document) -> Vec<u32> {
        doc.lines.iter().map(|l| l.start_ms.unwrap()).collect()
    }

    #[test]
    fn the_timestamps_are_read() {
        assert_eq!(time("01:02"), Some(62_000));
        assert_eq!(time("00:01.50"), Some(1_500), "two digits are hundredths");
        assert_eq!(time("00:01.500"), Some(1_500), "three are thousandths");
        assert_eq!(time("00:01.5"), Some(1_500), "one is tenths");
        assert_eq!(time("00:01:50"), Some(1_500), "a colon before the fraction too");
        assert_eq!(time("ti:Something"), None);
    }

    #[test]
    fn synced_lyrics_end_up_in_order() {
        let l = parse("[00:12.00]One\n[00:05.50]Zero\n");
        assert_eq!(l.timing, Timing::Line);
        assert_eq!(l.lines[0].text, "Zero");
        assert_eq!(starts(&l), vec![5_500, 12_000]);
        assert_eq!(l.lines[0].end_ms, None, "nobody said when it ends");
    }

    #[test]
    fn a_line_with_several_stamps_comes_out_several_times() {
        let l = parse("[00:10.00][01:10.00][02:10.00]Chorus\n");
        assert_eq!(starts(&l), vec![10_000, 70_000, 130_000], "the chorus plays three times");
        assert!(l.lines.iter().all(|x| x.text == "Chorus"));
    }

    #[test]
    fn headers_are_not_lyrics() {
        let l = parse("[ti:A song]\n[ar:Someone]\n[al:An album]\n[length: 03:45]\n[00:01.00]Here it goes\n");
        assert_eq!(l.title.as_deref(), Some("A song"));
        assert_eq!(l.artist.as_deref(), Some("Someone"));
        assert_eq!(l.duration_ms, Some(225_000));
        assert_eq!(l.lines.len(), 1, "only the stamped line");
        assert_eq!(l.lines[0].text, "Here it goes");
    }

    #[test]
    fn the_offset_shifts_the_whole_lyric() {
        let l = parse("[offset:+500]\n[00:10.00]<00:10.00>Late <00:10.40>words\n");
        assert_eq!(l.lines[0].start_ms, Some(9_500), "half a second earlier");
        assert_eq!(l.lines[0].segments[1].start_ms, 9_900, "and the words with it");
        let l = parse("[offset:99000]\n[00:10.00]Late\n");
        assert_eq!(l.lines[0].start_ms, Some(0), "never below zero");
    }

    #[test]
    fn text_without_stamps_is_still_lyrics() {
        let l = parse("First\n\nSecond\n");
        assert_eq!(l.timing, Timing::Unsynced);
        assert_eq!(l.lines.len(), 2, "and empty lines do not count");
        assert_eq!(l.lines[1].text, "Second");
        assert_eq!(l.lines[1].start_ms, None);
    }

    #[test]
    fn a_stamp_without_words_ends_the_voice() {
        let l = parse("[00:01.00]Something\n[00:05.00]\n[00:09.00]More\n");
        assert_eq!(l.lines.len(), 2, "the silence is not a line");
        assert_eq!(l.lines[0].end_ms, Some(5_000), "it closes the line before it");
        assert_eq!(l.lines[1].end_ms, None);
    }

    #[test]
    fn enhanced_stamps_time_each_word() {
        let l = parse("[00:12.00]<00:12.00>Stay <00:12.60>until <00:13.20>morning<00:14.00>\n");
        assert_eq!(l.timing, Timing::Word);
        let line = &l.lines[0];
        assert_eq!(line.text, "Stay until morning", "the stamps never reach the screen");
        let times: Vec<(u32, Option<u32>)> = line.segments.iter().map(|s| (s.start_ms, s.end_ms)).collect();
        assert_eq!(times, vec![(12_000, None), (12_600, None), (13_200, Some(14_000))]);
        assert_eq!(line.end_ms, Some(14_000), "the last stamp ends the line");
        assert!(line.segments[0].space_after);
        assert!(!line.segments[2].space_after);
    }

    #[test]
    fn the_first_word_may_go_without_a_stamp() {
        let l = parse("[00:20.00]Hello <00:20.50>there\n");
        let s = &l.lines[0].segments;
        assert_eq!((s[0].text.as_str(), s[0].start_ms), ("Hello", 20_000), "it starts with its line");
        assert_eq!((s[1].text.as_str(), s[1].start_ms), ("there", 20_500));
    }

    #[test]
    fn a_word_split_by_stamps_stays_one_word() {
        let l = parse("[00:01.00]<00:01.00>Hel<00:01.20>lo <00:01.50>you\n");
        let line = &l.lines[0];
        assert_eq!(line.text, "Hello you");
        assert!(!line.segments[0].space_after, "the syllable continues");
        assert_eq!(l.timing, Timing::Syllable);
    }

    #[test]
    fn a_repeated_line_moves_its_words_along() {
        let l = parse("[00:10.00][01:10.00]<00:10.00>Na <00:10.50>na\n");
        assert_eq!(l.lines[1].segments[1].start_ms, 70_500, "the second time is a minute later");
    }

    #[test]
    fn unsynced_lines_lose_their_word_stamps_too() {
        let l = parse("<00:01.00>Only <00:01.50>words\n");
        assert_eq!(l.lines[0].text, "Only words");
    }
}
