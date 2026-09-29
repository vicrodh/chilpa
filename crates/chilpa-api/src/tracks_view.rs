//! The Tracks screen: every track of the library in one list, found by
//! what is typed, in an order, and grouped or not.

use crate::{Filter, Reader, Snapshot, TrackOrder, TrackRow};

/// What the screen asks for, as it travels in one string: the order, the
/// grouping and what was typed, a unit separator apart.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TracksQuery {
    pub order: String,
    /// "off", "album", "artist" or "name".
    pub group: String,
    pub text: String,
}

impl TracksQuery {
    pub fn parse(key: &str) -> Self {
        let mut parts = key.splitn(3, '\u{1f}');
        let mut next = || parts.next().unwrap_or_default().to_string();
        Self { order: next(), group: next(), text: next() }
    }

    /// The order the list is read in. A grouping decides it: groups that
    /// are not together are not groups.
    pub fn order(&self) -> TrackOrder {
        TrackOrder::from_name(match self.group.as_str() {
            "album" => "album",
            "artist" => "performer",
            "name" => "title",
            _ if self.order.is_empty() => "album",
            _ => &self.order,
        })
    }

    pub fn grouped(&self) -> bool {
        matches!(self.group.as_str(), "album" | "artist" | "name")
    }
}

/// What a query leaves: the tracks, and where each letter starts among
/// them as `[["A", 12], ["B", 40]]` when they are grouped.
#[derive(Clone, Debug, Default)]
pub struct FoundTracks {
    pub snapshot: Snapshot,
    pub letters: String,
    pub group: String,
}

/// The group a row belongs to, which is what its header says.
pub fn group_of(group: &str, row: &TrackRow) -> String {
    match group {
        "album" => row.album.clone(),
        "artist" => row.artist.clone(),
        "name" => chilpa_library::letter_of(&row.title),
        _ => String::new(),
    }
}

impl Reader {
    /// The tracks of a query. The sources, qualities and formats left out
    /// come from `extras`, which the Library screen shares.
    pub fn tracks(&self, query: &str, extras: &Filter) -> FoundTracks {
        let query = TracksQuery::parse(query);
        let filter = Filter {
            text: query.text.clone(),
            excluded_sources: extras.excluded_sources.clone(),
            qualities: extras.qualities.clone(),
            formats: extras.formats.clone(),
            ..Filter::default()
        };
        let snapshot = self.snapshot(&filter, query.order());
        let letters = if query.grouped() {
            let by = if query.group == "name" { "title" } else { query.group.as_str() };
            let found = self.letters(&snapshot.ids, by);
            serde_json::to_string(&found).unwrap_or_default()
        } else {
            String::new()
        };
        FoundTracks { snapshot, letters, group: query.group }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_grouping_decides_the_order_and_no_grouping_leaves_it() {
        let asked = |key: &str| TracksQuery::parse(key).order();
        assert_eq!(asked("title-desc\u{1f}off\u{1f}"), TrackOrder::TitleDescending);
        assert_eq!(asked("title-desc\u{1f}artist\u{1f}"), TrackOrder::Performer);
        assert_eq!(asked("year\u{1f}name\u{1f}"), TrackOrder::Title);
        assert_eq!(asked(""), TrackOrder::Album, "nothing asked is the order of the records");
        assert_eq!(TracksQuery::parse("added\u{1f}album\u{1f}two words").text, "two words");
    }
}
