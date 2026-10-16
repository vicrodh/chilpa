//! Whether some lyrics belong to the track that is playing.
//!
//! Every lyrics player that takes the first search result serves, sooner or
//! later, the lyrics of another song. The guards here, in order:
//!
//! 1. **The ISRC decides when both sides have one.** Equal is the same
//!    recording; different is another one, however alike the names.
//! 2. **Timed lyrics must fit the track**: within two seconds of its length,
//!    the rule lyrics databases themselves use. Lyrics timed for a live take
//!    drift a minute off on the studio one. Text without timings has no
//!    such problem and only has to be the same song.
//! 3. **Lyrics that end well after the track does are another recording.**
//! 4. **Names are compared after folding**: case, accents, punctuation,
//!    "feat." and what is in brackets ("Remastered 2011", "Live") do not
//!    make another song.
//!
//! The score weighs title and artist fully, the album less and the length as
//! much as the title, with steps instead of a straight line: a few hundred
//! milliseconds are rounding, a few seconds are another edit.

use crate::model::{Document, Timing};
use serde::{Deserialize, Serialize};

/// What is known about the track that is playing.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Track {
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub duration_ms: Option<u32>,
    pub isrc: Option<String>,
    /// The MusicBrainz recording, when the tags name it.
    pub mbid: Option<String>,
}

/// What a candidate says it is.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct Claim {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub isrc: Option<String>,
}

/// The widest difference in length timed lyrics may have.
pub const TIMED_TOLERANCE_MS: u32 = 2_000;
/// How long after the track's end lyrics may still be singing.
pub const OVERRUN_MS: u32 = 10_000;
/// The lowest score that is still the same song.
pub const ACCEPT: f32 = 0.70;

/// The verdict on a candidate.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Verdict {
    /// The same recording, by its ISRC.
    Exact,
    /// The same song, with this score from 0 to 1.
    Likely(f32),
    /// Another song or another recording.
    Rejected,
}

impl Verdict {
    pub fn score(self) -> f32 {
        match self {
            Verdict::Exact => 1.0,
            Verdict::Likely(s) => s,
            Verdict::Rejected => 0.0,
        }
    }

    pub fn accepted(self) -> bool {
        !matches!(self, Verdict::Rejected)
    }
}

/// Judges whether a document, claiming to be `claim`, belongs to `track`.
pub fn judge(track: &Track, claim: &Claim, doc: &Document) -> Verdict {
    if let (Some(a), Some(b)) = (track.isrc.as_deref(), claim.isrc.as_deref()) {
        if !a.trim().is_empty() && !b.trim().is_empty() {
            if !a.trim().eq_ignore_ascii_case(b.trim()) {
                return Verdict::Rejected;
            }
            return if overruns(track, doc) { Verdict::Rejected } else { Verdict::Exact };
        }
    }
    if overruns(track, doc) {
        return Verdict::Rejected;
    }
    let length = track.duration_ms.zip(claim.duration_ms.or(doc.duration_ms)).map(|(a, b)| a.abs_diff(b));
    if doc.timing > Timing::Unsynced && length.is_some_and(|d| d > TIMED_TOLERANCE_MS) {
        return Verdict::Rejected;
    }

    let mut weighted = 0.0f32;
    let mut weights = 0.0f32;
    let mut add = |weight: f32, value: Option<f32>| {
        if let Some(v) = value {
            weighted += weight * v;
            weights += weight;
        }
    };
    let title = claim.title.as_deref().or(doc.title.as_deref()).map(|t| similarity(&track.title, t));
    let artist = claim.artist.as_deref().or(doc.artist.as_deref()).map(|a| artist_similarity(&track.artist, a));
    add(1.0, title);
    add(1.0, artist);
    add(0.4, track.album.as_deref().zip(claim.album.as_deref()).map(|(a, b)| similarity(a, b)));
    if doc.timing > Timing::Unsynced { add(1.0, length.map(length_score)); }
    if title.is_none() || artist.is_none() {
        // A candidate that does not say what song it is cannot be trusted
        // on its length alone.
        return Verdict::Rejected;
    }
    let score = if weights > 0.0 { weighted / weights } else { 0.0 };
    // A title that is plainly another song sinks the candidate whatever the
    // rest says: two songs on one album share artist, album and often a
    // length.
    if title.is_some_and(|t| t < 0.5) || artist.is_some_and(|a| a < 0.4) || score < ACCEPT {
        Verdict::Rejected
    } else {
        Verdict::Likely(score)
    }
}

/// Lyrics still singing well after the track has ended.
fn overruns(track: &Track, doc: &Document) -> bool {
    let Some(length) = track.duration_ms else { return false };
    let last = doc.lines.iter().filter_map(|l| l.start_ms).max();
    last.is_some_and(|start| start > length.saturating_add(OVERRUN_MS))
}

/// From 1 for the same length down to 0 past a few seconds.
fn length_score(difference_ms: u32) -> f32 {
    match difference_ms {
        0..=299 => 1.0,
        300..=699 => 0.9,
        700..=1_499 => 0.75,
        1_500..=3_499 => 0.45,
        _ => 0.0,
    }
}

/// How alike two titles are, from 0 to 1, after folding.
pub fn similarity(a: &str, b: &str) -> f32 {
    let (a, b) = (fold(a), fold(b));
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    if a == b {
        return 1.0;
    }
    let edit = 1.0 - edit_distance(&a, &b) as f32 / a.chars().count().max(b.chars().count()) as f32;
    // One title inside the other ("Song" and "Song Part 2") is close but not
    // the same: it scores below an exact match and above a typo.
    let contained = if a.contains(&b) || b.contains(&a) { 0.8 } else { 0.0 };
    edit.max(contained)
}

/// How many letters have to change to turn one text into the other.
fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = (above + 1).min(row[j] + 1).min(diagonal + usize::from(ca != *cb));
            diagonal = above;
        }
    }
    row[b.len()]
}

/// Artists are compared by their first name too: "Someone feat. Other" and
/// "Someone & Other" are still Someone.
fn artist_similarity(a: &str, b: &str) -> f32 {
    similarity(a, b).max(similarity(main_artist(a), main_artist(b)))
}

fn main_artist(name: &str) -> &str {
    let lower = name.to_lowercase();
    let mut cut = name.len();
    for separator in [" feat.", " feat ", " ft.", " ft ", " featuring ", " & ", ", ", " x ", " and ", " with ", "; ", " / "] {
        if let Some(i) = lower.find(separator) {
            if name.is_char_boundary(i) {
                cut = cut.min(i);
            }
        }
    }
    name[..cut].trim()
}

/// Lowercase, without accents, without what is in brackets, without
/// "feat." and its guests, with punctuation made space and spaces collapsed.
pub fn fold(text: &str) -> String {
    let lower = text.to_lowercase();
    let mut kept = String::with_capacity(lower.len());
    let mut depth = 0u32;
    for c in lower.chars() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            _ if depth == 0 => kept.push(c),
            _ => {}
        }
    }
    // What follows a dash is usually an edition: "Song - Remastered 2011".
    for marker in [" - remaster", " - live", " - mono", " - stereo", " - single", " - radio edit", " - edit"] {
        if let Some(i) = kept.find(marker) {
            kept.truncate(i);
        }
    }
    for guest in [" feat. ", " feat ", " ft. ", " featuring "] {
        if let Some(i) = kept.find(guest) {
            kept.truncate(i);
        }
    }
    let plain: String = kept
        .chars()
        .map(strip_accent)
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    plain.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The letter without its accent, for the Latin letters that carry one.
fn strip_accent(c: char) -> char {
    match c {
        'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' | 'ā' | 'ă' | 'ą' => 'a',
        'é' | 'è' | 'ê' | 'ë' | 'ē' | 'ė' | 'ę' | 'ě' => 'e',
        'í' | 'ì' | 'î' | 'ï' | 'ī' | 'į' | 'ı' => 'i',
        'ó' | 'ò' | 'ô' | 'ö' | 'õ' | 'ø' | 'ō' | 'ő' => 'o',
        'ú' | 'ù' | 'û' | 'ü' | 'ū' | 'ů' | 'ű' | 'ų' => 'u',
        'ý' | 'ÿ' => 'y',
        'ñ' | 'ń' | 'ň' => 'n',
        'ç' | 'ć' | 'č' => 'c',
        'ś' | 'š' | 'ş' => 's',
        'ź' | 'ż' | 'ž' => 'z',
        'ř' => 'r',
        'ł' => 'l',
        'ď' => 'd',
        'ť' => 't',
        'ğ' => 'g',
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lrc;

    fn track() -> Track {
        Track {
            title: "Wasted Years".into(),
            artist: "Iron Maiden".into(),
            album: Some("Somewhere in Time".into()),
            duration_ms: Some(306_000),
            isrc: None,
            mbid: None,
        }
    }

    fn claim(title: &str, artist: &str, duration_ms: u32) -> Claim {
        Claim { title: Some(title.into()), artist: Some(artist.into()), album: None, duration_ms: Some(duration_ms), isrc: None }
    }

    fn timed() -> Document {
        lrc::parse("[00:10.00]From the coast of gold\n")
    }

    #[test]
    fn edits_are_counted() {
        assert_eq!(edit_distance("kitten", "sitting"), 3);
        assert_eq!(edit_distance("", "abc"), 3);
        assert_eq!(edit_distance("résumé", "resume"), 2);
        assert_eq!(similarity("Wasted Years", "Wasted Year"), 1.0 - 1.0 / 12.0);
    }

    #[test]
    fn folding_ignores_what_does_not_make_another_song() {
        assert_eq!(fold("Wasted Years (2015 Remaster)"), "wasted years");
        assert_eq!(fold("Wasted Years - Remastered 2015"), "wasted years");
        assert_eq!(fold("Naïve Résumé"), "naive resume");
        assert_eq!(fold("Song feat. Someone"), "song");
        assert_eq!(fold("Don't Stop"), "don t stop");
    }

    #[test]
    fn the_same_song_with_the_same_length_is_accepted() {
        let v = judge(&track(), &claim("Wasted Years (2015 Remaster)", "Iron Maiden", 306_400), &timed());
        assert!(matches!(v, Verdict::Likely(s) if s > 0.9), "{v:?}");
    }

    #[test]
    fn timed_lyrics_for_another_length_are_another_recording() {
        let v = judge(&track(), &claim("Wasted Years", "Iron Maiden", 309_000), &timed());
        assert_eq!(v, Verdict::Rejected, "three seconds off is another edit");
        let plain = lrc::parse("From the coast of gold\n");
        assert!(judge(&track(), &claim("Wasted Years", "Iron Maiden", 309_000), &plain).accepted(), "the words are still right");
    }

    #[test]
    fn another_title_by_the_same_band_is_rejected() {
        let v = judge(&track(), &claim("Stranger in a Strange Land", "Iron Maiden", 306_000), &timed());
        assert_eq!(v, Verdict::Rejected, "same artist, same length, another song");
    }

    #[test]
    fn plain_words_do_not_depend_on_the_album_or_edit_length() {
        let mut candidate = claim("Wasted Years", "Iron Maiden", 311_000);
        candidate.album = Some("Compilation".into());
        assert!(judge(&track(), &candidate, &lrc::parse("The same words")).accepted());
        assert_eq!(judge(&track(), &candidate, &timed()), Verdict::Rejected);
    }

    #[test]
    fn the_isrc_decides_when_both_have_one() {
        let mut t = track();
        t.isrc = Some("GBAYE8500152".into());
        let mut c = claim("Wasted Years", "Iron Maiden", 306_000);
        c.isrc = Some("gbaye8500152".into());
        assert_eq!(judge(&t, &c, &timed()), Verdict::Exact);
        c.isrc = Some("GBAYE9999999".into());
        assert_eq!(judge(&t, &c, &timed()), Verdict::Rejected, "another recording, whatever the names say");
    }

    #[test]
    fn lyrics_that_outlast_the_track_are_rejected() {
        let long = lrc::parse("[00:10.00]Start\n[05:30.00]Long after\n");
        let v = judge(&track(), &claim("Wasted Years", "Iron Maiden", 306_000), &long);
        assert_eq!(v, Verdict::Rejected);
    }

    #[test]
    fn guests_do_not_change_the_artist() {
        let mut t = track();
        t.artist = "Someone".into();
        t.title = "A Song".into();
        let v = judge(&t, &claim("A Song", "Someone feat. Other", 306_000), &timed());
        assert!(v.accepted(), "{v:?}");
    }

    #[test]
    fn a_candidate_that_does_not_say_what_it_is_is_not_trusted() {
        let c = Claim { duration_ms: Some(306_000), ..Claim::default() };
        assert_eq!(judge(&track(), &c, &timed()), Verdict::Rejected);
    }
}
