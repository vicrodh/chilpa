//! The Albums screen: every record, found by what is typed, in an order,
//! through the sources and qualities chosen, and grouped or not. And how
//! any list of records is laid out in rows, which groups break.

use crate::{Filter, Reader, TrackOrder};
use chilpa_library::AlbumSort;
use std::collections::HashSet;

/// The records a question leaves, and where each group starts among them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AlbumsFound {
    pub ids: Vec<i64>,
    pub groups: Vec<(usize, String)>,
}

/// One row of a grid of records: which record it starts at, how many it
/// holds, and the group it opens, if it opens one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GridRow {
    pub first: usize,
    pub span: usize,
    pub label: String,
}

/// `len` records in rows of `columns`, a group starting a row of its own.
/// `max_rows` above zero keeps that many.
pub fn grid_rows(len: usize, columns: usize, max_rows: usize, groups: &[(usize, String)]) -> Vec<GridRow> {
    if columns == 0 {
        return Vec::new();
    }
    let mut starts: Vec<(usize, String)> = groups.iter().filter(|(at, _)| *at < len).cloned().collect();
    if starts.first().is_none_or(|(at, _)| *at > 0) {
        starts.insert(0, (0, String::new()));
    }
    let mut rows = Vec::new();
    for (i, (from, label)) in starts.iter().enumerate() {
        let to = starts.get(i + 1).map_or(len, |(next, _)| *next);
        let mut first = *from;
        while first < to {
            let span = columns.min(to - first);
            rows.push(GridRow { first, span, label: if first == *from { label.clone() } else { String::new() } });
            first += span;
        }
    }
    if max_rows > 0 {
        rows.truncate(max_rows);
    }
    rows
}

/// A grid that shows a source's records beside the library's own, a
/// server counting as the library: the library's records then wear the
/// player's badge where a source's wear their own. An artist, with no id,
/// is neither.
pub fn mixes_sources(ids: impl IntoIterator<Item = i64>) -> bool {
    let (mut own, mut source) = (false, false);
    for id in ids {
        own |= id > 0;
        source |= id < 0;
        if own && source {
            return true;
        }
    }
    false
}

/// Artists drawn as record cards: the name where a title goes and how
/// many records stand under it. Only those whose name holds every word of
/// `filter` stay.
pub fn artist_cards(artists: &[crate::Artist], filter: &str) -> Vec<crate::Album> {
    let words: Vec<String> = filter.to_lowercase().split_whitespace().map(String::from).collect();
    artists
        .iter()
        .filter(|a| {
            let name = a.name.to_lowercase();
            words.iter().all(|w| name.contains(w.as_str()))
        })
        .map(|a| crate::Album {
            id: 0,
            artist: chilpa_i18n::tf("{} record", "{} records", a.album_count as i64, &[&a.album_count.to_string()]),
            title: a.name.clone(),
            year: None,
            track_count: a.track_count,
            artwork_path: a.artwork_path.clone(),
            play_count: 0,
        })
        .collect()
}

impl Reader {
    /// Merge source album identities into the same order and groups as local albums.
    pub fn albums_view_with_sources(&self, order: &str, group: &str, text: &str, extras: &Filter, remote: Vec<crate::Album>) -> AlbumsFound {
        if remote.is_empty() { return self.albums_view(order, group, text, extras); }
        let local = self.albums_view(order, "", text, extras);
        let mut rows: Vec<_> = local.ids.chunks(500).flat_map(|ids| self.lib.albums_by_ids(ids).unwrap_or_default()).map(crate::browse::album_of).collect();
        rows.extend(remote);
        match order {
            "title" => rows.sort_by_cached_key(|a| a.title.to_lowercase()),
            "year" => rows.sort_by_key(|a| std::cmp::Reverse(a.year)),
            "played" => rows.sort_by_key(|a| std::cmp::Reverse(a.play_count)),
            "added" | "last_played" => {},
            _ => rows.sort_by_cached_key(|a| (a.artist.to_lowercase(), a.year, a.title.to_lowercase())),
        }
        if !matches!(group, "letter" | "artist") { return AlbumsFound { ids: rows.iter().map(|a| a.id).collect(), groups: Vec::new() }; }
        arrange(rows.iter().map(|a| (match (group, order) {
            ("artist", _) => a.artist.trim().into(), (_, "artist") => chilpa_library::letter_of(&a.artist), _ => chilpa_library::letter_of(&a.title),
        }, a.id)).collect())
    }

    /// The records of the Albums screen. `group` is "letter", "artist" or
    /// nothing. The groups come in their order, and the order asked for is
    /// the order inside each one. The sources, qualities and formats left
    /// out come from `extras`, which the Library screen shares.
    pub fn albums_view(&self, order: &str, group: &str, text: &str, extras: &Filter) -> AlbumsFound {
        let order = AlbumSort::from_name(order);
        let mut ids = self.lib.album_ids_matching(order, text).unwrap_or_default();
        if !extras.excluded_sources.is_empty() || !extras.qualities.is_empty() || !extras.formats.is_empty() {
            let through = Filter {
                excluded_sources: extras.excluded_sources.clone(),
                qualities: extras.qualities.clone(),
                formats: extras.formats.clone(),
                ..Filter::default()
            };
            let left: HashSet<i64> = self.lib.browse_ids(&through, TrackOrder::Album).map(|s| s.album_ids).unwrap_or_default().into_iter().collect();
            ids.retain(|id| left.contains(id));
        }
        let ids = self.lib.grouped_album_ids(&ids).unwrap_or(ids);
        if !matches!(group, "letter" | "artist") {
            return AlbumsFound { ids, groups: Vec::new() };
        }
        // Each record with the group it is in. A letter is the artist's when
        // the records are ordered by artist, and the title's otherwise.
        let by_artist = order == AlbumSort::Artist;
        let mut named: Vec<(String, i64)> = Vec::with_capacity(ids.len());
        for chunk in ids.chunks(500) {
            for (album, _) in self.lib.albums_by_ids(chunk).unwrap_or_default() {
                let label = match (group, by_artist) {
                    ("artist", _) => album.album_artist.trim().to_string(),
                    (_, true) => chilpa_library::letter_of(&album.album_artist),
                    _ => chilpa_library::letter_of(&album.title),
                };
                named.push((label, album.id));
            }
        }
        arrange(named)
    }
}

/// Records that came in the order asked for, put in groups: the groups by
/// name, whatever their case, and each one's records as they came.
pub(crate) fn arrange(mut named: Vec<(String, i64)>) -> AlbumsFound {
    // Stable: what shares a group stays in the order it came in.
    named.sort_by_cached_key(|(label, _)| label.to_lowercase());
    let mut found = AlbumsFound::default();
    for (at, (label, id)) in named.into_iter().enumerate() {
        if found.groups.last().is_none_or(|(_, last)| last.to_lowercase() != label.to_lowercase()) {
            found.groups.push((at, label));
        }
        found.ids.push(id);
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(len: usize, columns: usize, max_rows: usize, groups: &[(usize, &str)]) -> Vec<(usize, usize, String)> {
        let groups: Vec<(usize, String)> = groups.iter().map(|(at, label)| (*at, label.to_string())).collect();
        grid_rows(len, columns, max_rows, &groups).into_iter().map(|r| (r.first, r.span, r.label)).collect()
    }

    #[test]
    fn a_group_keeps_its_records_in_the_order_they_came() {
        // By year, newest first: 30, 20, 10. Two artists.
        let came = vec![("Band".to_string(), 30), ("Another".to_string(), 25), ("band".to_string(), 20), ("Band".to_string(), 10)];
        let found = arrange(came);
        assert_eq!(found.ids, [25, 30, 20, 10]);
        assert_eq!(found.groups, [(0, "Another".to_string()), (1, "Band".to_string())], "one group whatever its case");
    }

    #[test]
    fn records_of_the_library_beside_a_sources_mix_the_grid() {
        assert!(mixes_sources([3, -1, 7]));
        assert!(!mixes_sources([3, 7]), "the library and its servers are one");
        assert!(!mixes_sources([-1, -2]), "a source alone");
        assert!(!mixes_sources([0, 0, -1]), "an artist is no record");
        assert!(!mixes_sources([]));
    }

    #[test]
    fn artists_become_cards_and_the_filter_keeps_every_word() {
        let artist = |name: &str, albums| crate::Artist { name: name.into(), album_count: albums, track_count: 3, artwork_path: String::new() };
        let artists = [artist("Mora Vale", 2), artist("Vale Mora Trio", 1), artist("Band", 1)];
        let cards = artist_cards(&artists, "vale MORA");
        assert_eq!(cards.iter().map(|c| c.title.as_str()).collect::<Vec<_>>(), ["Mora Vale", "Vale Mora Trio"]);
        assert_eq!((cards[0].artist.as_str(), cards[1].artist.as_str()), ("2 records", "1 record"));
        assert_eq!((cards[0].id, cards[0].track_count), (0, 3));
        assert_eq!(artist_cards(&artists, " ").len(), 3, "no words keep everyone");
    }

    #[test]
    fn records_fill_rows_and_the_last_takes_what_is_left() {
        assert_eq!(rows(7, 3, 0, &[]), [(0, 3, String::new()), (3, 3, String::new()), (6, 1, String::new())]);
        assert_eq!(rows(7, 3, 1, &[]), [(0, 3, String::new())], "a shelf keeps one row");
        assert!(rows(7, 0, 0, &[]).is_empty(), "no width, nothing laid out");
        assert_eq!(rows(3, 1, 0, &[]).len(), 3, "a list is a grid of one column");
    }

    #[test]
    fn a_group_starts_a_row_of_its_own_and_names_it_once() {
        let laid = rows(7, 3, 0, &[(0, "A"), (4, "B")]);
        assert_eq!(laid, [(0, 3, "A".to_string()), (3, 1, String::new()), (4, 3, "B".to_string())]);
        assert_eq!(rows(2, 3, 0, &[(1, "B")]), [(0, 1, String::new()), (1, 1, "B".to_string())], "what comes before the first group has no name");
    }
}
