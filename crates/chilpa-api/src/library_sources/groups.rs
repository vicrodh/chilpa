//! Records grouped by more than their letter or their artist: by year,
//! decade, genre, label, composer, quality, source or when they arrived, the
//! dimensions the library's columns and filters already have. A record goes
//! to one group only: the value most of its tracks carry. The groups come in
//! their own order (newest year first, the best quality first, the rest by
//! name, what is not known last); inside each, the order asked for.

use super::SourceIndex;
use crate::albums_view::AlbumsFound;
use std::collections::HashMap;

/// What a grid of records is grouped by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlbumGroup {
    None,
    Letter,
    Artist,
    Year,
    Decade,
    Genre,
    Label,
    Composer,
    Quality,
    Source,
    Added,
    /// Album, EP, single, live or compilation.
    Kind,
}

impl AlbumGroup {
    pub fn from_name(name: &str) -> Self {
        match name {
            "letter" => Self::Letter,
            "artist" => Self::Artist,
            "year" => Self::Year,
            "decade" => Self::Decade,
            "genre" => Self::Genre,
            "label" => Self::Label,
            "composer" => Self::Composer,
            "quality" => Self::Quality,
            "source" => Self::Source,
            "added" => Self::Added,
            "type" => Self::Kind,
            _ => Self::None,
        }
    }
}

/// A group's place among the others, then its name: what is not known goes
/// last, a number in its own direction, a name by its letters.
type Rank = (u8, i64, String);

/// The group a record goes to, and where that group goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Filed {
    pub rank: Rank,
    pub label: String,
}

fn named(name: &str, missing: String) -> Filed {
    let name = name.trim();
    if name.is_empty() {
        Filed { rank: (1, 0, String::new()), label: missing }
    } else {
        Filed { rank: (0, 0, name.to_lowercase()), label: name.to_string() }
    }
}

fn year(year: Option<i32>, decade: bool) -> Filed {
    match year.filter(|y| *y > 0) {
        Some(y) if decade => { let d = y.div_euclid(10) * 10; Filed { rank: (0, -(d as i64), String::new()), label: format!("{d}s") } }
        Some(y) => Filed { rank: (0, -(y as i64), String::new()), label: y.to_string() },
        None => Filed { rank: (1, 0, String::new()), label: chilpa_i18n::t("Unknown year") },
    }
}

/// A tier as `quality::Tier::rank` numbers it: 4 DSD down to 1 lossy.
fn quality(rank: i32) -> Filed {
    let label = match rank {
        4 => "DSD".to_string(),
        3 => chilpa_i18n::t("Hi-Res"),
        2 => chilpa_i18n::t("Lossless"),
        1 => chilpa_i18n::t("Lossy"),
        _ => return Filed { rank: (1, 0, String::new()), label: chilpa_i18n::t("Unknown quality") },
    };
    Filed { rank: (0, -(rank as i64), String::new()), label }
}

const DAY: i64 = 86_400;

/// When a record arrived, in spans that grow with age, then by year.
fn added(at: i64, now: i64) -> Filed {
    if at <= 0 { return Filed { rank: (1, 0, String::new()), label: chilpa_i18n::t("Unknown date") }; }
    let age = now.saturating_sub(at);
    let span = |n: i64, label: String| Filed { rank: (0, n, String::new()), label };
    if age < 7 * DAY { return span(0, chilpa_i18n::t("This week")); }
    if age < 31 * DAY { return span(1, chilpa_i18n::t("This month")); }
    if age < 92 * DAY { return span(2, chilpa_i18n::t("Last 3 months")); }
    if age < 365 * DAY { return span(3, chilpa_i18n::t("Last 12 months")); }
    let (year, _, _) = crate::listening_period::civil_from_days(at.div_euclid(DAY));
    Filed { rank: (0, 10_000 - year, String::new()), label: year.to_string() }
}

/// What kind of release a record is: what its source says, else what its
/// artist, title and size say. A stand-in artist ("Various Artists") is a
/// compilation; a title that says EP, single or live is that; up to three
/// tracks is a single and up to six in under half an hour an EP.
pub(crate) fn release_kind(stated: &str, title: &str, artist: &str, tracks: usize, duration_ms: i64) -> &'static str {
    match stated {
        "album" => return "album", "ep" => return "ep", "single" => return "single",
        "live" => return "live", "compilation" => return "compilation", _ => {}
    }
    if chilpa_library::is_various(artist) { return "compilation"; }
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
    match tracks {
        1..=3 => "single",
        4..=6 if duration_ms > 0 && duration_ms < 30 * 60 * 1000 => "ep",
        _ => "album",
    }
}

fn kind(kind: &str) -> Filed {
    let (rank, label) = match kind {
        "ep" => (1, chilpa_i18n::t("EPs")),
        "single" => (2, chilpa_i18n::t("Singles")),
        "live" => (3, chilpa_i18n::t("Live")),
        "compilation" => (4, chilpa_i18n::t("Compilations")),
        _ => (0, chilpa_i18n::t("Albums")),
    };
    Filed { rank: (0, rank, String::new()), label }
}

/// What each record is known by, for the groupings that read the records.
#[derive(Default)]
pub(crate) struct RecordFacts {
    pub title: String,
    pub artist: String,
    pub year: Option<i32>,
    pub genre: String,
    pub label: String,
    pub composer: String,
    pub quality: i32,
    pub source: String,
    pub added_at: i64,
    /// "album", "ep", "single", "live" or "compilation".
    pub kind: &'static str,
}

/// The integrated records' facts, from their items: the most common value
/// among a record's items, its best quality and its newest arrival.
pub(crate) fn source_facts(index: &SourceIndex, names: &HashMap<String, String>) -> HashMap<i64, RecordFacts> {
    fn most(counts: HashMap<String, usize>) -> String {
        let mut counts: Vec<(String, usize)> = counts.into_iter().collect();
        counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| chilpa_library::nocase(&a.0, &b.0)));
        counts.into_iter().next().map(|(name, _)| name).unwrap_or_default()
    }
    let mut votes: HashMap<i64, [HashMap<String, usize>; 3]> = HashMap::new();
    let mut sizes: HashMap<i64, (usize, i64, &str)> = HashMap::new();
    let mut facts: HashMap<i64, RecordFacts> = HashMap::new();
    for item in index.items.iter().filter(|item| item.album != 0) {
        let fact = facts.entry(item.album).or_insert_with(|| RecordFacts {
            title: item.album_title.clone(), artist: item.album_artist.clone(), year: item.year,
            source: names.get(&item.source_key()).cloned().unwrap_or_else(|| item.plugin.clone()), ..RecordFacts::default()
        });
        let size = sizes.entry(item.album).or_insert((0, 0, ""));
        size.0 += item.tracks;
        size.1 += item.duration_ms as i64;
        if !item.release_type.is_empty() { size.2 = item.release_type.as_str(); }
        let rank = match item.tier { "dsd" => 4, "hires" => 3, "lossless" => 2, "lossy" => 1, _ => 0 };
        fact.quality = fact.quality.max(rank);
        fact.added_at = fact.added_at.max(item.added_at);
        let vote = votes.entry(item.album).or_default();
        for (at, value) in [item.genres.first(), Some(&item.label), item.composers.first()].into_iter().enumerate() {
            if let Some(value) = value.filter(|v| !v.trim().is_empty()) { *vote[at].entry(value.clone()).or_default() += 1; }
        }
    }
    for (album, (tracks, duration, stated)) in sizes {
        if let Some(fact) = facts.get_mut(&album) { fact.kind = release_kind(stated, &fact.title, &fact.artist, tracks, duration); }
    }
    for (album, [genres, labels, composers]) in votes {
        if let Some(fact) = facts.get_mut(&album) {
            fact.genre = most(genres);
            fact.label = most(labels);
            fact.composer = most(composers);
        }
    }
    facts
}

/// The group of one record.
pub(crate) fn file(group: AlbumGroup, facts: &RecordFacts, by_artist: bool, now: i64) -> Filed {
    match group {
        AlbumGroup::Letter => {
            let letter = chilpa_library::letter_of(if by_artist { &facts.artist } else { &facts.title });
            Filed { rank: (0, 0, letter.clone()), label: letter }
        }
        AlbumGroup::Artist => named(&facts.artist, chilpa_i18n::t("Unknown artist")),
        AlbumGroup::Year => year(facts.year, false),
        AlbumGroup::Decade => year(facts.year, true),
        AlbumGroup::Genre => named(&facts.genre, chilpa_i18n::t("No genre")),
        AlbumGroup::Label => named(&facts.label, chilpa_i18n::t("No label")),
        AlbumGroup::Composer => named(&facts.composer, chilpa_i18n::t("No composer")),
        AlbumGroup::Quality => quality(facts.quality),
        AlbumGroup::Source => named(&facts.source, chilpa_i18n::t("Unknown source")),
        AlbumGroup::Added => added(facts.added_at, now),
        AlbumGroup::Kind => kind(facts.kind),
        AlbumGroup::None => Filed { rank: (0, 0, String::new()), label: String::new() },
    }
}

/// Records in the order they came, put in their groups. `turned` reverses
/// the groups of an order about names (from Z), as the letter and artist
/// groups always did.
pub(crate) fn arrange(filed: Vec<(Filed, i64)>, group: AlbumGroup, turned: bool) -> AlbumsFound {
    let mut filed = filed;
    let by_name = matches!(group, AlbumGroup::Letter | AlbumGroup::Artist);
    // Stable: what shares a group stays in the order it came in.
    if by_name && turned {
        filed.sort_by(|a, b| b.0.rank.cmp(&a.0.rank));
    } else {
        filed.sort_by(|a, b| a.0.rank.cmp(&b.0.rank));
    }
    let mut found = AlbumsFound::default();
    let mut last: Option<Rank> = None;
    for (at, (named, id)) in filed.into_iter().enumerate() {
        if last.as_ref() != Some(&named.rank) {
            last = Some(named.rank);
            found.groups.push((at, named.label));
        }
        found.ids.push(id);
    }
    found
}

impl crate::Reader {
    /// Records in the order they came, grouped. The library's own are read
    /// for what the grouping needs; the integrated ones come with the index.
    pub(crate) fn album_groups(&self, ids: Vec<i64>, group: AlbumGroup, by_artist: bool, turned: bool, index: &SourceIndex) -> AlbumsFound {
        if group == AlbumGroup::None { return AlbumsFound { ids, groups: Vec::new() }; }
        let mut names: HashMap<String, String> = self.lib.facet_sources().unwrap_or_default().into_iter().map(|s| (s.key, s.label)).collect();
        names.insert("local:default".into(), chilpa_i18n::t("Local library"));
        names.extend(index.sources.iter().map(|s| (s.key.clone(), s.label.clone())));
        let local: Vec<i64> = ids.iter().copied().filter(|id| *id > 0).collect();
        let mut facts = source_facts(index, &names);
        let keys = self.lib.album_keys(&local).unwrap_or_default();
        let primaries = match group {
            AlbumGroup::Genre => self.lib.album_primaries("genre"),
            AlbumGroup::Label => self.lib.album_primaries("label"),
            AlbumGroup::Composer => self.lib.album_primaries("composer"),
            AlbumGroup::Source => self.lib.album_primaries("source"),
            _ => Ok(HashMap::new()),
        }.unwrap_or_default();
        let sizes = if group == AlbumGroup::Kind { self.lib.album_sizes().unwrap_or_default() } else { HashMap::new() };
        let mut best: HashMap<i64, i32> = HashMap::new();
        if group == AlbumGroup::Quality {
            for (album, path, codec, bits, rate) in self.lib.album_files().unwrap_or_default() {
                let rank = crate::quality::Quality::of_track(&path, codec.as_deref(), bits, rate).tier.rank();
                let held = best.entry(album).or_default();
                *held = (*held).max(rank);
            }
        }
        for (id, key) in keys {
            let value = primaries.get(&id).cloned().unwrap_or_default();
            let (tracks, duration) = sizes.get(&id).copied().unwrap_or((0, 0));
            facts.insert(id, RecordFacts {
                kind: release_kind("", &key.title, &key.artist, tracks, duration),
                title: key.title, artist: key.artist, year: key.year, added_at: key.added_at,
                quality: best.get(&id).copied().unwrap_or(0),
                genre: if group == AlbumGroup::Genre { value.clone() } else { String::new() },
                label: if group == AlbumGroup::Label { value.clone() } else { String::new() },
                composer: if group == AlbumGroup::Composer { value.clone() } else { String::new() },
                source: if group == AlbumGroup::Source { names.get(&value).cloned().unwrap_or(value) } else { String::new() },
            });
        }
        let now = crate::now_unix();
        let blank = RecordFacts::default();
        let filed = ids.into_iter().map(|id| (file(group, facts.get(&id).unwrap_or(&blank), by_artist, now), id)).collect();
        arrange(filed, group, turned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(title: &str, year: Option<i32>, genre: &str, quality: i32, added_at: i64) -> RecordFacts {
        RecordFacts { title: title.into(), artist: "Band".into(), year, genre: genre.into(), quality, added_at, ..RecordFacts::default() }
    }

    #[test]
    fn years_and_decades_come_newest_first_and_the_unknown_last() {
        let records = [(1, facts("A", Some(1994), "", 0, 0)), (2, facts("B", None, "", 0, 0)), (3, facts("C", Some(2001), "", 0, 0)), (4, facts("D", Some(1999), "", 0, 0))];
        let decades = arrange(records.iter().map(|(id, f)| (file(AlbumGroup::Decade, f, false, 0), *id)).collect(), AlbumGroup::Decade, false);
        assert_eq!(decades.ids, [3, 1, 4, 2]);
        assert_eq!(decades.groups.iter().map(|(_, l)| l.as_str()).collect::<Vec<_>>(), ["2000s", "1990s", "Unknown year"]);
        let years = arrange(records.iter().map(|(id, f)| (file(AlbumGroup::Year, f, false, 0), *id)).collect(), AlbumGroup::Year, true);
        assert_eq!(years.ids, [3, 4, 1, 2], "an order turned around does not turn years");
    }

    #[test]
    fn genres_go_by_name_whatever_their_case_and_the_best_quality_goes_first() {
        let records = [(1, facts("A", None, "jazz", 2, 0)), (2, facts("B", None, "Blues", 4, 0)), (3, facts("C", None, "Jazz", 3, 0)), (4, facts("D", None, "", 0, 0))];
        let genres = arrange(records.iter().map(|(id, f)| (file(AlbumGroup::Genre, f, false, 0), *id)).collect(), AlbumGroup::Genre, false);
        assert_eq!(genres.ids, [2, 1, 3, 4]);
        assert_eq!(genres.groups.iter().map(|(_, l)| l.as_str()).collect::<Vec<_>>(), ["Blues", "jazz", "No genre"], "one group whatever its case");
        let tiers = arrange(records.iter().map(|(id, f)| (file(AlbumGroup::Quality, f, false, 0), *id)).collect(), AlbumGroup::Quality, false);
        assert_eq!(tiers.ids, [2, 3, 1, 4]);
        assert_eq!(tiers.groups.iter().map(|(_, l)| l.as_str()).collect::<Vec<_>>(), ["DSD", "Hi-Res", "Lossless", "Unknown quality"]);
    }

    #[test]
    fn a_release_is_what_its_source_says_else_what_its_title_and_size_say() {
        let minutes = |m: i64| m * 60 * 1000;
        assert_eq!(release_kind("live", "Anything", "Band", 12, minutes(60)), "live", "the source's word stands");
        assert_eq!(release_kind("", "Hits", "Various Artists", 20, minutes(70)), "compilation");
        assert_eq!(release_kind("", "Blue Room - EP", "Band", 4, minutes(18)), "ep");
        assert_eq!(release_kind("", "Hey Ya! - Single", "Band", 2, minutes(8)), "single");
        assert_eq!(release_kind("", "Alive (Live at Wembley)", "Band", 14, minutes(80)), "live");
        assert_eq!(release_kind("", "One More Time", "Band", 1, minutes(5)), "single", "three tracks or fewer");
        assert_eq!(release_kind("", "Five Songs", "Band", 5, minutes(21)), "ep");
        assert_eq!(release_kind("", "Five Long Songs", "Band", 5, minutes(48)), "album", "an EP is short");
        assert_eq!(release_kind("", "Steppenwolf", "Band", 11, minutes(45)), "album");
        assert_eq!(release_kind("", "Liveforever", "Band", 11, minutes(45)), "album", "a word that starts with live is not live");
        assert_eq!(release_kind("", "Live Through This", "Band", 12, minutes(38)), "album", "live in a title is not a live record");
        assert_eq!(release_kind("", "Live at Leeds", "Band", 6, minutes(37)), "live");
        assert_eq!(release_kind("", "Single Ladies", "Band", 9, minutes(40)), "album");
        assert_eq!(release_kind("", "Deep Purple in Rock", "Band", 7, minutes(43)), "album", "no EP inside a word");
        let records = [(1, RecordFacts { kind: "single", ..facts("A", None, "", 0, 0) }), (2, RecordFacts { kind: "album", ..facts("B", None, "", 0, 0) }),
            (3, RecordFacts { kind: "compilation", ..facts("C", None, "", 0, 0) }), (4, RecordFacts { kind: "ep", ..facts("D", None, "", 0, 0) })];
        let found = arrange(records.iter().map(|(id, f)| (file(AlbumGroup::Kind, f, false, 0), *id)).collect(), AlbumGroup::Kind, false);
        assert_eq!(found.ids, [2, 4, 1, 3], "albums, EPs, singles, live, compilations");
    }

    #[test]
    fn arrivals_go_in_spans_that_grow_with_age() {
        let now = 1_800_000_000;
        let records = [(1, facts("A", None, "", 0, now - 3 * DAY)), (2, facts("B", None, "", 0, now - 400 * DAY)), (3, facts("C", None, "", 0, now - 20 * DAY)), (4, facts("D", None, "", 0, 0))];
        let found = arrange(records.iter().map(|(id, f)| (file(AlbumGroup::Added, f, false, now), *id)).collect(), AlbumGroup::Added, false);
        assert_eq!(found.ids, [1, 3, 2, 4]);
        assert_eq!(found.groups[0].1, "This week");
        assert_eq!(found.groups[1].1, "This month");
        assert_eq!(found.groups[2].1, "2025");
    }
}
