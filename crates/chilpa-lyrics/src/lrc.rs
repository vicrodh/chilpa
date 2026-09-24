//! The LRC format.
//!
//! A text file where each line starts with one or more timestamps:
//! `[01:23.45]the words`. What is looked after here:
//!
//! - **A line can carry several stamps.** A refrain is written once with
//!   all its times; taking only the first loses the rest of the song.

//! - **Stamps come in every shape**: `mm:ss`, `mm:ss.xx` (hundredths) and
//!   `mm:ss.xxx` (thousandths). Reading `.5` as 5 ms instead of 500 leaves
//!   the lyrics half a second early.
//! - **Header tags** (`[ti:]`, `[ar:]`, `[offset:]`) are not lyric lines and
//!   must not show on screen.
//! - **A file without a single stamp is still lyrics**, just unsynced. It is
//!   shown whole.

use crate::{Lyrics, Line};

/// Reads LRC lyrics, or plain text when there are no stamps.
pub fn parse(text: &str) -> Lyrics {
    let mut lines: Vec<Line> = Vec::new();
    let mut loose: Vec<String> = Vec::new();
    let mut offset_ms: i64 = 0;
    let mut title = None;
    let mut artist = None;

    for raw in text.lines() {
        let line = raw.trim_end_matches('\r');
        let (labels, rest) = labels_of(line);
        if labels.is_empty() {
            // It may be a header tag or unsynced text.
            match tag(line) {
                Some(("offset", v)) => {
                    offset_ms = v.trim().parse().unwrap_or(0);
                }
                Some(("ti", v)) => title = Some(v.trim().to_string()),
                Some(("ar", v)) => artist = Some(v.trim().to_string()),
                // Any other known tag stays quiet; it is not lyrics.
                Some(_) => {}
                None => {
                    if !line.trim().is_empty() {
                        loose.push(line.trim().to_string());
                    }
                }
            }
            continue;
        }
        let text = rest.trim().to_string();
        for at in labels {
            lines.push(Line { at_ms: at, text: text.clone() });
        }
    }

    if lines.is_empty() {
        return Lyrics {
            title,
            artist,
            lines: loose.into_iter().map(|t| Line { at_ms: 0, text: t }).collect(),
            synced: false,
        };
    }

    // The file's offset shifts the whole lyric. It is positive when it has
    // to move earlier, so it is subtracted.
    if offset_ms != 0 {
        for l in &mut lines {
            *l = Line {
                at_ms: (l.at_ms as i64 - offset_ms).max(0) as u64,
                text: std::mem::take(&mut l.text),
            };
        }
    }
    lines.sort_by_key(|l| l.at_ms);
    Lyrics { title, artist, lines: lines, synced: true }
}

/// The stamps a line starts with, and what is left after them.
fn labels_of(line: &str) -> (Vec<u64>, &str) {
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

/// `mm:ss`, `mm:ss.xx` or `mm:ss.xxx`, in milliseconds.
fn time(s: &str) -> Option<u64> {
    let (min, rest) = s.split_once(':')?;
    let minutes: u64 = min.trim().parse().ok()?;
    let (seconds, fraction) = match rest.split_once(['.', ':']) {
        Some((a, b)) => (a, Some(b)),
        None => (rest, None),
    };
    let seconds: u64 = seconds.trim().parse().ok()?;
    let millis = match fraction {
        None => 0,
        Some(f) => {
            let f = f.trim();
            if !f.chars().all(|c| c.is_ascii_digit()) || f.is_empty() {
                return None;
            }
            // Two digits are hundredths, three are thousandths. Reading ".5"
            // as 5 ms would leave the lyrics half a second early.
            let n: u64 = f.parse().ok()?;
            match f.len() {
                1 => n * 100,
                2 => n * 10,
                _ => n,
            }
        }
    };
    Some(minutes * 60_000 + seconds * 1_000 + millis)
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

    #[test]
    fn the_timestamps_are_read() {
        assert_eq!(time("01:02"), Some(62_000));
        assert_eq!(time("00:01.50"), Some(1_500), "two digits are hundredths");
        assert_eq!(time("00:01.500"), Some(1_500), "three are thousandths");
        assert_eq!(time("00:01.5"), Some(1_500), "one is tenths");
        assert_eq!(time("ti:Something"), None);
    }

    #[test]
    fn synced_lyrics_end_up_in_order() {
        let l = parse("[00:12.00]One\n[00:05.50]Zero\n");
        assert!(l.synced);
        assert_eq!(l.lines[0].text, "Zero");
        assert_eq!(l.lines[0].at_ms, 5_500);
        assert_eq!(l.lines[1].at_ms, 12_000);
    }

    #[test]
    fn a_line_with_several_stamps_comes_out_several_times() {
        let l = parse("[00:10.00][01:10.00][02:10.00]Chorus\n");
        assert_eq!(l.lines.len(), 3, "the chorus plays three times");
        assert_eq!(
            l.lines.iter().map(|x| x.at_ms).collect::<Vec<_>>(),
            vec![10_000, 70_000, 130_000]
        );
        assert!(l.lines.iter().all(|x| x.text == "Chorus"));
    }

    #[test]
    fn headers_are_not_lyrics() {
        let l = parse("[ti:A song]\n[ar:Someone]\n[al:An album]\n[00:01.00]Here it goes\n");
        assert_eq!(l.title.as_deref(), Some("A song"));
        assert_eq!(l.artist.as_deref(), Some("Someone"));
        assert_eq!(l.lines.len(), 1, "only the stamped line");
        assert_eq!(l.lines[0].text, "Here it goes");
    }

    #[test]
    fn the_offset_shifts_the_whole_lyric() {
        let l = parse("[offset:500]\n[00:10.00]Late\n");
        assert_eq!(l.lines[0].at_ms, 9_500, "half a second earlier");
        // And it never goes negative.
        let l = parse("[offset:99000]\n[00:10.00]Late\n");
        assert_eq!(l.lines[0].at_ms, 0);
    }

    #[test]
    fn text_without_stamps_is_still_lyrics() {
        let l = parse("First\n\nSecond\n");
        assert!(!l.synced, "it is not synchronized");
        assert_eq!(l.lines.len(), 2, "and empty lines do not count");
        assert_eq!(l.lines[1].text, "Second");
    }

    #[test]
    fn a_line_without_text_stays_as_a_pause() {
        let l = parse("[00:01.00]Something\n[00:05.00]\n[00:09.00]More\n");
        assert_eq!(l.lines.len(), 3);
        assert_eq!(l.lines[1].text, "", "the silence is part of the lyrics too");
    }
}
