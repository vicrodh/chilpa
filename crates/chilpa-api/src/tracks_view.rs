//! The Tracks screen: every track of the library in one list, found by
//! what is typed, in an order, and grouped or not.

use crate::{Filter, Reader, Snapshot, TrackOrder, TrackRow};

/// What the screen asks for, as it travels in one string: the order, the
/// grouping, what was typed and the stars kept, a unit separator apart.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TracksQuery {
    pub order: String,
    /// I accept album, artist, name, year, decade, genre, label and composer.
    pub group: String,
    pub text: String,
    /// A track's own stars to keep, 0 for none, written "5,0"; empty keeps
    /// every track.
    pub ratings: Vec<u8>,
}

impl TracksQuery {
    pub fn parse(key: &str) -> Self {
        let mut parts = key.splitn(4, '\u{1f}');
        let mut next = || parts.next().unwrap_or_default().to_string();
        let (order, group, text) = (next(), next(), next());
        let ratings = next().split(',').filter_map(|stars| stars.trim().parse().ok()).filter(|stars| *stars <= 5).collect();
        Self { order, group, text, ratings }
    }

    /// The order inside each group, or of the whole list when it has none:
    /// the one asked for, and the order of the records when none was.
    pub fn order(&self) -> TrackOrder {
        TrackOrder::from_name(if self.order.is_empty() { "album" } else { &self.order })
    }

    pub fn grouped(&self) -> bool {
        matches!(self.group.as_str(), "album" | "artist" | "name" | "year" | "decade" | "genre" | "label" | "composer")
    }
}

/// What a query leaves: the tracks, and where each letter starts among
/// them as `[["A", 12], ["B", 40]]` when they are grouped.
#[derive(Clone, Debug, Default)]
pub struct FoundTracks {
    pub snapshot: Snapshot,
    pub letters: String,
    pub group: String,
    /// Where each group starts and what it is called, in row order.
    pub sections: Vec<(usize, String)>,
}

/// The group a row is in, from where the groups start: what a view asks of
/// every row between two far places, answered without reading one.
pub fn section_at(sections: &[(usize, String)], row: usize) -> Option<&str> {
    sections.partition_point(|(at, _)| *at <= row).checked_sub(1).map(|at| sections[at].1.as_str())
}

/// Where each letter first starts, from where the groups start.
fn letters_of(sections: &[(usize, String)], by: &str) -> String {
    if matches!(by, "year" | "decade") { return "[]".into(); }
    let mut letters: Vec<(String, usize)> = Vec::new();
    for (at, name) in sections {
        let letter = if by == "title" { name.clone() } else { chilpa_library::letter_of(name) };
        if !letters.iter().any(|(seen, _)| *seen == letter) { letters.push((letter, *at)); }
    }
    serde_json::to_string(&letters).unwrap_or_default()
}

/// I keep source metadata with loaded rows; local snapshots carry sections.
/// This allocation lives only as long as its catalog row or cached page.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TrackGroups {
    pub year: Option<i32>,
    pub genre: String,
    pub label: String,
    pub composer: String,
}

pub(crate) fn metadata_group(by: &str, year: Option<i32>, genre: &str, label: &str, composer: &str) -> String {
    let named = |name: &str, missing: &str| if name.is_empty() { chilpa_i18n::t(missing) } else { name.to_string() };
    match by {
        "year" | "decade" => match year.filter(|y| *y > 0) {
            Some(y) if by == "decade" => format!("{}s", y / 10 * 10),
            Some(y) => y.to_string(),
            None => chilpa_i18n::t("Unknown year"),
        },
        "genre" => named(genre, "No genre"),
        "label" => named(label, "No label"),
        "composer" => named(composer, "No composer"),
        _ => String::new(),
    }
}

/// The group a row belongs to, which is what its header says.
pub fn group_of(group: &str, row: &TrackRow) -> String {
    match group {
        "album" => row.album.clone(),
        "artist" => row.artist.clone(),
        "name" => chilpa_library::letter_of(&row.title),
        "year" | "decade" | "genre" | "label" | "composer" => {
            let empty = TrackGroups::default();
            let groups = row.groups.as_deref().unwrap_or(&empty);
            metadata_group(group, groups.year, &groups.genre, &groups.label, &groups.composer)
        }
        _ => String::new(),
    }
}

impl Reader {
    /// The tracks of a query, with the integrated sources' saved tracks
    /// merged into the same order. The sources, qualities and formats left
    /// out come from `extras`, which the Library screen shares.
    pub fn tracks(&self, query: &str, extras: &Filter) -> FoundTracks {
        self.tracks_using(query, extras, &self.source_index().1)
    }

    pub(crate) fn tracks_using(&self, query: &str, extras: &Filter, index: &crate::library_sources::SourceIndex) -> FoundTracks {
        let query = TracksQuery::parse(query);
        let filter = Filter {
            text: query.text.clone(),
            excluded_sources: extras.excluded_sources.clone(),
            qualities: extras.qualities.clone(),
            formats: extras.formats.clone(),
            track_ratings: query.ratings.clone(),
            grouped_by: if query.grouped() { query.group.clone() } else { String::new() },
            ..Filter::default()
        };
        let by = if query.group == "name" { "title" } else { query.group.as_str() };
        if !index.items.iter().any(|item| item.saved_track) && !query.grouped() {
            return FoundTracks { snapshot: self.snapshot(&filter, query.order()), group: query.group, ..Default::default() };
        }
        let (local, keys) = self.lib.keyed_snapshot(&filter, query.order()).unwrap_or_else(|e| {
            eprintln!("chilpa-queries: library snapshot failed: {e}");
            Default::default()
        });
        let stars = crate::library_sources::Stars::read(&self.lib, index);
        let (snapshot, sections) = crate::library_sources::merge_snapshot(local, keys, &filter, query.order(), index, &stars,
            crate::library_sources::Scope::Tracks, query.grouped().then_some(by), None);
        let letters = if query.grouped() { letters_of(&sections, by) } else { String::new() };
        FoundTracks { snapshot, letters, group: query.group, sections }
    }

    /// The same, handed over once: the merge reads nothing of the library
    /// again, so there is no local answer worth showing first.
    pub fn tracks_progressive(&self, query: &str, extras: &Filter, mut emit: impl FnMut(FoundTracks)) {
        emit(self.tracks(query, extras));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_order_asked_for_stands_whatever_the_grouping() {
        let asked = |key: &str| TracksQuery::parse(key).order();
        assert_eq!(asked("title-desc\u{1f}off\u{1f}"), TrackOrder::TitleDescending);
        assert_eq!(asked("title-desc\u{1f}artist\u{1f}"), TrackOrder::TitleDescending, "a grouping orders the groups, not what is in them");
        assert_eq!(asked(""), TrackOrder::Album, "nothing asked is the order of the records");
        assert_eq!(TracksQuery::parse("added\u{1f}album\u{1f}two words").text, "two words");
        for group in ["year", "decade", "genre", "label", "composer"] {
            let query = TracksQuery::parse(&format!("title-desc\u{1f}{group}\u{1f}"));
            assert!(query.grouped(), "{group}");
            assert_eq!(query.order(), TrackOrder::TitleDescending);
        }
    }

    #[test]
    fn metadata_names_have_letters_and_dates_have_no_letter_strip() {
        for by in ["genre", "label", "composer"] {
            assert_eq!(letters_of(&[(0, "Alpha".into()), (3, "another".into()), (9, "Beta".into()), (12, "Écho".into()), (15, "3 names".into())], by),
                r##"[["A",0],["B",9],["E",12],["#",15]]"##, "{by}");
        }
        for by in ["year", "decade"] {
            assert_eq!(letters_of(&[(0, "2001".into()), (3, "1999".into()), (9, chilpa_i18n::t("Unknown year"))], by), "[]");
        }
    }

    #[test]
    fn a_row_s_group_is_read_from_where_the_groups_start() {
        let sections = vec![(0, "A".to_string()), (3, "B".to_string()), (9, "Z".to_string())];
        assert_eq!([0, 2, 3, 8, 9, 20_000].map(|row| section_at(&sections, row)), [Some("A"), Some("A"), Some("B"), Some("B"), Some("Z"), Some("Z")]);
        assert_eq!(section_at(&[], 4), None, "an ungrouped list says nothing");
        assert_eq!(letters_of(&[(0, "Abbey Road".into()), (12, "Animals".into()), (30, "Blue".into())], "album"), r#"[["A",0],["B",30]]"#);
    }

    #[test]
    fn the_stars_kept_travel_last_and_nothing_else_reads_as_one() {
        let query = TracksQuery::parse("added\u{1f}off\u{1f}two words\u{1f}5,0,x,9");
        assert_eq!((query.text.as_str(), query.ratings), ("two words", vec![5, 0]));
        assert!(TracksQuery::parse("added\u{1f}off\u{1f}").ratings.is_empty(), "none asked keeps every track");
    }
}

impl crate::Chilpa {
    /// A track's origin, resolved from memory even when its source is disconnected.
    pub fn track_source_name(&self, path: &str) -> String {
        if let Some((id, _)) = crate::source_opener::of_plugin(path) {
            let name = self.source_name(&id);
            return if name.is_empty() { id } else { name };
        }
        chilpa_i18n::t(if crate::on_a_server(path) { "Server" } else { "Local" })
    }
}
