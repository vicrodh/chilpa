//! One card per record, wherever it is kept.
//!
//! With "Group same album from different sources" on, a record the library
//! keeps and the copies the sources keep of it are one card: the grid shows
//! one of them, its card wears every source that has it, and its page offers
//! every copy. Which copies are one card follows "Group album versions":
//!
//! - on, every edition of the record is one card. The same record is the
//!   same `album_group` (title and artist without edition notes), or the
//!   same artist with a title that goes on from the other's in an edition's
//!   words: "Ballast der Republik" and "Ballast der Republik plus
//!   Jubiläums-Album", but never "Reich & Sexy" and its "II";
//! - off, only the same edition kept by different sources is: the same
//!   title, the same number of tracks and the same explicit mark when both
//!   say them. Two copies of one source stay two cards.
//!
//! The one shown is the listener's choice when they made one, else the
//! library's own (whose editions "Group album versions" already chose among),
//! else the sources' best by quality. A choice of a source's copy is kept by
//! the source and its reference, which outlive a run's ids, in one
//! preference.

use super::SourceIndex;
use chilpa_library::store::versions::VersionCatalog;
use std::collections::{HashMap, HashSet};

/// The preference that keeps which sources' copies the listener chose.
pub(crate) const CHOSEN: &str = "library.versionSources";
/// The preference that turns grouping across sources on and off.
pub(crate) const GROUP_SOURCES: &str = "library.groupSources";

/// Whether grids show a record kept by several sources once; on unless the
/// listener turned it off.
pub(crate) fn grouping_sources() -> bool {
    chilpa_core::settings::load().window.get(GROUP_SOURCES).is_none_or(|on| on != "false")
}

/// The copies the listener chose, as `plugin|reference`.
pub(crate) fn chosen() -> HashSet<String> {
    chilpa_core::settings::load().window.get(CHOSEN)
        .and_then(|text| serde_json::from_str(text).ok()).unwrap_or_default()
}

/// A source's record as one copy among the library's.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SourceVersion {
    pub group: String,
    /// Its title folded to words, which an edition is told by.
    pub edition: String,
    /// How many tracks it has, zero when unknown.
    pub tracks: usize,
    /// What the source says: explicit, clean, or nothing.
    pub explicit: Option<bool>,
    /// 4 DSD, 3 hi-res, 2 lossless, 1 lossy, 0 unknown, as the library ranks.
    pub tier: u8,
    pub plugin: String,
    /// `plugin|reference`: what a choice of it is kept as.
    pub key: String,
}

impl SourceVersion {
    pub(crate) fn key_of(plugin: &str, identity: &str) -> String {
        format!("{plugin}|{identity}")
    }
}

pub(crate) fn tier_rank(tier: &str) -> u8 {
    match tier { "dsd" => 4, "hires" => 3, "lossless" => 2, "lossy" => 1, _ => 0 }
}

/// A title as words: lower case, letters and digits only.
pub(crate) fn words(text: &str) -> String {
    text.to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Words that open an edition's part of a title, folded as `album_group`
/// leaves them.
const EDITION: &[&str] = &[
    "plus", "deluxe", "edition", "expanded", "anniversary", "remaster", "remastered", "bonus", "special",
    "spezialedition", "jubiläums", "jubilaums", "jubilaeums", "collector", "collectors", "limited", "super",
    "complete", "legacy", "version", "reissue", "mono", "stereo",
];

/// The artist and the title of a group key (`album_group`'s `len:artisttitle`).
fn parts(group: &str) -> (&str, &str) {
    let Some((len, rest)) = group.split_once(':') else { return ("", group) };
    let len: usize = len.parse().unwrap_or(0);
    if rest.is_char_boundary(len) { rest.split_at(len) } else { ("", rest) }
}

/// Whether two titles of one artist, with their track counts, name the
/// same record: equal, the longer one going on from the shorter in an
/// edition's words, or in a lone "live" when both hold as many tracks
/// ("Flight 666 - The Original Soundtrack" and "Flight 666: The Original
/// Soundtrack (Live)"; a studio record and its live one rarely do).
fn same_record((a, a_tracks): (&str, usize), (b, b_tracks): (&str, usize)) -> bool {
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    if short == long { return true; }
    let Some(rest) = long.strip_prefix(short).and_then(|rest| rest.strip_prefix(' ')) else { return false };
    !short.is_empty() && (rest.split_whitespace().next().is_some_and(|word| EDITION.contains(&word))
        || (rest == "live" && a_tracks != 0 && a_tracks == b_tracks))
}

/// One copy of a record, of the library or of a source.
#[derive(Clone, Debug)]
pub(crate) struct RecordCopy<'a> {
    pub id: i64,
    pub group: &'a str,
    /// Two copies of one source never make one edition; each of the
    /// library's records is a source of its own here.
    pub source: String,
    pub edition: String,
    pub tracks: usize,
    pub explicit: Option<bool>,
    pub tier: u8,
}

impl RecordCopy<'_> {
    fn same_edition(&self, other: &RecordCopy) -> bool {
        let known = |a: usize, b: usize| a == 0 || b == 0 || a == b;
        // A source that adds "(Live)" to a live record's title names the
        // same edition.
        let bare = |edition: &str| edition.strip_suffix(" live").unwrap_or(edition).to_string();
        bare(&self.edition) == bare(&other.edition) && known(self.tracks, other.tracks)
            && (self.explicit.is_none() || other.explicit.is_none() || self.explicit == other.explicit)
    }
}

/// The copies of `ids` the library and the sources know.
pub(crate) fn copies<'a>(ids: impl IntoIterator<Item = i64>, local: &'a VersionCatalog, sources: &'a HashMap<i64, SourceVersion>) -> Vec<RecordCopy<'a>> {
    ids.into_iter().filter_map(|id| if id > 0 {
        local.versions.get(&id).map(|v| RecordCopy { id, group: &v.group, source: format!("library:{id}"), edition: words(&v.title),
            tracks: v.tracks, explicit: v.explicit, tier: v.tier() })
    } else {
        sources.get(&id).map(|v| RecordCopy { id, group: &v.group, source: v.plugin.clone(), edition: v.edition.clone(),
            tracks: v.tracks, explicit: v.explicit, tier: v.tier })
    }).collect()
}

/// Copies gathered into the cards they make.
#[derive(Debug, Default)]
pub(crate) struct Clusters {
    cluster_of: HashMap<i64, usize>,
    members: Vec<Vec<i64>>,
}

impl Clusters {
    /// With `versions`, each record is one card; without, each edition.
    pub(crate) fn build(copies: &[RecordCopy], versions: bool) -> Self {
        let mut by_artist: HashMap<&str, Vec<&RecordCopy>> = HashMap::new();
        for copy in copies.iter().filter(|c| !c.group.is_empty()) {
            by_artist.entry(parts(copy.group).0).or_default().push(copy);
        }
        let mut out = Self::default();
        for (_, mut copies) in by_artist {
            copies.sort_by_key(|c| (parts(c.group).1.len(), c.id < 0, std::cmp::Reverse(c.tier)));
            // Each record joins the first one with a title it shares.
            let mut records: Vec<(Vec<(&str, usize)>, Vec<&RecordCopy>)> = Vec::new();
            for copy in copies {
                let title = (parts(copy.group).1, copy.tracks);
                match records.iter_mut().find(|(titles, _)| titles.iter().any(|t| same_record(*t, title))) {
                    Some((titles, members)) => { titles.push(title); members.push(copy); }
                    None => records.push((vec![title], vec![copy])),
                }
            }
            for (_, members) in records {
                let mut cards: Vec<Vec<&RecordCopy>> = Vec::new();
                for copy in members {
                    let joined = if versions { cards.first_mut() } else {
                        cards.iter_mut().find(|card| card[0].same_edition(copy) && !card.iter().any(|m| m.source == copy.source))
                    };
                    match joined { Some(card) => card.push(copy), None => cards.push(vec![copy]) }
                }
                for card in cards {
                    let at = out.members.len();
                    for copy in &card { out.cluster_of.insert(copy.id, at); }
                    out.members.push(card.iter().map(|c| c.id).collect());
                }
            }
        }
        out
    }

    /// Every copy of the card `id` is on, itself included.
    pub(crate) fn members_of(&self, id: i64) -> &[i64] {
        self.cluster_of.get(&id).map_or(&[], |at| self.members[*at].as_slice())
    }

    fn cards(&self) -> impl Iterator<Item = &Vec<i64>> {
        self.members.iter()
    }
}

/// What a grid groups by: the library's editions, whether editions are one
/// card, and the copies the listener chose.
#[derive(Clone, Copy)]
pub(crate) struct Grouping<'a> {
    pub library: &'a VersionCatalog,
    pub versions: bool,
    pub chosen: &'a HashSet<String>,
}

/// The copy a card shows among `members`: the one the listener chose, else
/// the library's own, else the sources' best. Several of the library's
/// stay, as "Group album versions" left them.
fn shown(members: &[i64], sources: &HashMap<i64, SourceVersion>, chosen: &HashSet<String>) -> Vec<i64> {
    if let Some(mine) = members.iter().find(|m| **m < 0 && sources.get(m).is_some_and(|v| chosen.contains(&v.key))) {
        return vec![*mine];
    }
    let library: Vec<i64> = members.iter().copied().filter(|m| *m > 0).collect();
    if !library.is_empty() { return library; }
    let mut best: Option<(i64, u8)> = None;
    for m in members.iter().copied() {
        let tier = sources.get(&m).map_or(0, |v| v.tier);
        if best.is_none_or(|(_, held)| tier > held) { best = Some((m, tier)); }
    }
    best.map(|(m, _)| vec![m]).unwrap_or_default()
}

/// The records a grid drops so that each card shows once: of `ids`, every
/// copy on a card that reaches into the sources except the ones shown.
pub(crate) fn dropped(ids: &[i64], grouping: Grouping, sources: &HashMap<i64, SourceVersion>) -> HashSet<i64> {
    if !ids.iter().any(|id| *id < 0) { return HashSet::new(); }
    let copies = copies(ids.iter().copied(), grouping.library, sources);
    let clusters = Clusters::build(&copies, grouping.versions);
    let mut dropped = HashSet::new();
    for members in clusters.cards().filter(|m| m.len() > 1 && m.iter().any(|id| *id < 0)) {
        let kept = shown(members, sources, grouping.chosen);
        dropped.extend(members.iter().copied().filter(|m| !kept.contains(m)));
    }
    dropped
}

impl SourceIndex {
    /// Each integrated record as a copy, by its id, from what the index
    /// says of it; the catalog's own account replaces this when it is read.
    pub(crate) fn versions(items: &[super::SourceItem]) -> HashMap<i64, SourceVersion> {
        let mut versions: HashMap<i64, SourceVersion> = HashMap::new();
        for item in items.iter().filter(|item| item.album != 0) {
            let version = versions.entry(item.album).or_insert_with(|| SourceVersion {
                group: chilpa_library::store::versions::album_group(&item.album_title, &item.album_artist),
                edition: words(&item.album_title),
                tracks: if item.track == 0 { item.tracks } else { 0 },
                explicit: None,
                tier: 0,
                plugin: item.plugin.clone(),
                key: SourceVersion::key_of(&item.plugin, &item.album_identity),
            });
            version.tier = version.tier.max(tier_rank(item.tier));
        }
        versions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chilpa_library::store::versions::album_group;

    fn version(title: &str, plugin: &str, tier: u8, explicit: Option<bool>) -> SourceVersion {
        SourceVersion { group: album_group(title, "Band"), edition: words(title), tracks: 12, explicit, tier,
            plugin: plugin.into(), key: SourceVersion::key_of(plugin, title) }
    }

    fn grouping<'a>(library: &'a VersionCatalog, chosen: &'a HashSet<String>, versions: bool) -> Grouping<'a> {
        Grouping { library, versions, chosen }
    }

    #[test]
    fn an_edition_s_longer_title_is_the_same_record_and_a_sequel_is_not() {
        let library = VersionCatalog::default();
        let sources: HashMap<i64, SourceVersion> = [
            (-1, version("Ballast der Republik", "one", 2, None)),
            (-2, version("\"Ballast der Republik\" plus Jubiläums-Album \"Die Geister, die wir riefen\"", "two", 2, None)),
            (-3, version("Reich & Sexy", "one", 2, None)), (-4, version("Reich & Sexy II", "two", 2, None)),
        ].into_iter().collect();
        let all = copies([-1, -2, -3, -4], &library, &sources);
        let records = Clusters::build(&all, true);
        let mut ballast = records.members_of(-1).to_vec();
        ballast.sort();
        assert_eq!(ballast, [-2, -1]);
        assert_eq!(records.members_of(-3), [-3], "a numbered sequel is another record");
        let editions = Clusters::build(&all, false);
        assert_eq!(editions.members_of(-1), [-1], "without versions grouped, another edition is another card");
    }

    #[test]
    fn a_source_s_live_suffix_and_another_separator_are_the_same_record() {
        let library = VersionCatalog::default();
        let mut sources: HashMap<i64, SourceVersion> = [
            (-1, version("Flight 666 - The Original Soundtrack", "one", 2, None)),
            (-2, version("Flight 666: The Original Soundtrack (Live)", "two", 2, None)),
            (-3, version("Somewhere in Time", "one", 2, None)), (-4, version("Somewhere in Time (Live)", "two", 2, None)),
        ].into_iter().collect();
        sources.get_mut(&-4).unwrap().tracks = 9;
        let all = copies([-1, -2, -3, -4], &library, &sources);
        let mut flight = Clusters::build(&all, false).members_of(-1).to_vec();
        flight.sort();
        assert_eq!(flight, [-2, -1], "one card, even with editions apart");
        assert_eq!(Clusters::build(&all, true).members_of(-3), [-3], "a live record with other tracks is another record");
        sources.get_mut(&-2).unwrap().tracks = 0;
        let unknown = copies([-1, -2], &library, &sources);
        assert_eq!(Clusters::build(&unknown, true).members_of(-1), [-1], "without both counts, a live title stays apart");
    }

    #[test]
    fn the_same_edition_of_two_sources_is_one_card_and_its_clean_version_another() {
        let library = VersionCatalog::default();
        let sources: HashMap<i64, SourceVersion> = [
            (-1, version("Guts", "one", 2, Some(true))), (-2, version("Guts", "two", 3, Some(true))),
            (-3, version("Guts", "two", 2, Some(false))),
        ].into_iter().collect();
        let none = HashSet::new();
        assert_eq!(dropped(&[-1, -2, -3], grouping(&library, &none, false), &sources), [-1].into_iter().collect(),
            "the best of the explicit copies stands and the clean one keeps its card");
        assert_eq!(dropped(&[-1, -2, -3], grouping(&library, &none, true), &sources), [-1, -3].into_iter().collect(),
            "with versions grouped, one card");
        let mine: HashSet<String> = [SourceVersion::key_of("one", "Guts")].into_iter().collect();
        assert_eq!(dropped(&[-1, -2, -3], grouping(&library, &mine, true), &sources), [-2, -3].into_iter().collect(),
            "the listener's choice stands");
    }

    #[test]
    fn the_library_s_own_copy_stands_unless_a_source_s_was_chosen() {
        let lib = chilpa_library::Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        let album = lib.source_edition(source, "rec", "Record", "Band", Some(2001)).unwrap();
        lib.connection().execute("INSERT INTO tracks(source_id, external_id, title, album_id, added_at) VALUES (?1, '/1.flac', 'Song', ?2, 1)",
            rusqlite::params![source, album]).unwrap();
        let library = lib.version_catalog().unwrap();
        let mut copy = version("Record", "one", 4, None);
        copy.tracks = 1;
        let sources: HashMap<i64, SourceVersion> = [(-7, copy)].into_iter().collect();
        let none = HashSet::new();
        assert_eq!(dropped(&[album, -7], grouping(&library, &none, false), &sources), [-7].into_iter().collect());
        let mine: HashSet<String> = [SourceVersion::key_of("one", "Record")].into_iter().collect();
        assert_eq!(dropped(&[album, -7], grouping(&library, &mine, false), &sources), [album].into_iter().collect());
    }
}
