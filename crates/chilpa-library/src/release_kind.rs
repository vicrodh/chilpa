//! I classify records identically for grouping and queries.

pub const NAMES: &[&str] = &["album", "single", "ep", "compilation", "live", "soundtrack"];

pub fn normalize(value: &str) -> Option<&'static str> {
    NAMES.iter().copied().find(|name| value.trim().eq_ignore_ascii_case(name))
}

/// What kind of release a record is: what its source says, else what its
/// artist, title and size say. A stand-in artist ("Various Artists") is a
/// compilation; a title that says EP, single or live is that; up to three
/// tracks is a single and up to six in under half an hour an EP.
pub fn release_kind(stated: &str, title: &str, artist: &str, tracks: usize, duration_ms: i64) -> &'static str {
    if let Some(kind) = normalize(stated) { return kind; }
    if crate::is_various(artist) { return "compilation"; }
    let lowered = title.to_lowercase();
    // A whole word, not part of one: "Liveforever" says nothing.
    let word = |phrase: &str| lowered.match_indices(phrase).any(|(at, _)| {
        lowered[..at].chars().next_back().is_none_or(|c| !c.is_alphanumeric())
            && lowered[at + phrase.len()..].chars().next().is_none_or(|c| !c.is_alphanumeric())
    });
    // Set apart as a version: "(Live ...)", "[EP]", "- Single".
    let tagged = |name: &str| word(name) && ["(", "[", "- "].iter().any(|open| lowered.contains(&format!("{open}{name}")));
    if tagged("ep") || (word("ep") && lowered.ends_with(" ep")) { return "ep"; }
    if tagged("single") { return "single"; }
    if tagged("live") || word("live at") || word("live in") || word("unplugged") { return "live"; }
    if tagged("soundtrack") || word("original soundtrack") { return "soundtrack"; }
    match tracks {
        1..=3 => "single",
        4..=6 if duration_ms > 0 && duration_ms < 30 * 60 * 1000 => "ep",
        _ => "album",
    }
}
