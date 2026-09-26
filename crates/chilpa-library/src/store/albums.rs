//! The album grid and the artists.
//!
//! Everything comes from one query with a `GROUP BY`: in QBZ every page
//! aggregated the whole library again, and that was the audit's high
//! finding.

use crate::album_folder::VARIOUS_ARTISTS;
use crate::store::Library;
use rusqlite::{params, OptionalExtension};
use crate::store::{row_to_track, StoredTrack};

/// An album, already grouped.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct StoredAlbum {
    pub id: i64,
    pub title: String,
    pub album_artist: String,
    pub year: Option<i32>,
    pub artwork_path: Option<String>,
    pub track_count: usize,
}


/// An artist in the library, with what they have.
///
/// It comes from `albums.album_artist`, which is what the record says about
/// itself. The `artists` table is there for when there are MusicBrainz
/// identities; today, inventing rows there would add nothing the tag does
/// not already say.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct StoredArtist {
    pub name: String,
    pub album_count: usize,
    pub track_count: usize,
    /// The cover of one of their records, so there is no empty slot.
    pub artwork_path: Option<String>,
}

/// How to sort the album grid.
///
/// These are the quick views: the same thing, looked at another way. They
/// all come from the same query with a different `ORDER BY`; none of them
/// aggregates the whole library twice.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum AlbumSort {
    /// Artist, year and title: the order of a shelf.
    #[default]
    Artist,
    Title,
    /// Newest to oldest.
    Year,
    /// The latest arrivals in the library.
    Added,
    /// The most played.
    Played,
    /// The last thing that played.
    LastPlayed,
}

impl AlbumSort {
    /// What this view is called, for storing and for reading back.
    pub fn name(&self) -> &'static str {
        match self {
            AlbumSort::Artist => "artist",
            AlbumSort::Title => "title",
            AlbumSort::Year => "year",
            AlbumSort::Added => "added",
            AlbumSort::Played => "played",
            AlbumSort::LastPlayed => "last_played",
        }
    }

    /// The other way round. An unrecognized name gives the usual order, not
    /// an error: an old preference must not leave the grid empty.
    pub fn from_name(name: &str) -> Self {
        match name {
            "title" => AlbumSort::Title,
            "year" => AlbumSort::Year,
            "added" => AlbumSort::Added,
            "played" => AlbumSort::Played,
            "last_played" => AlbumSort::LastPlayed,
            _ => AlbumSort::Artist,
        }
    }

    /// All of them, in the order they are offered.
    pub fn all() -> [AlbumSort; 6] {
        [
            AlbumSort::Artist,
            AlbumSort::Title,
            AlbumSort::Year,
            AlbumSort::Added,
            AlbumSort::Played,
            AlbumSort::LastPlayed,
        ]
    }

    /// True when the ordering needs to aggregate the whole library to decide
    /// what comes first.
    ///
    /// The ordering, in terms of columns that exist before anything is
    /// counted.
    ///
    /// Three of these used to be an aggregate over the whole play log, so
    /// the page could not be trimmed until after it: the order *was* the
    /// aggregate. Since the V6 migration the three live in `album_rollup`,
    /// kept by triggers, so every ordering here reads a column and the page
    /// is trimmed first -- which is the difference between seven
    /// milliseconds and a hundred and fifty.
    ///
    /// `r` is the rollup. Every album has a row -- the triggers make one the
    /// moment it gets a track -- so only `last_played` needs a default: an
    /// album nobody has played has none, and zero is what puts it last.
    fn order_by(&self) -> &'static str {
        match self {
            AlbumSort::Artist => "a.album_artist COLLATE NOCASE, a.year, a.title COLLATE NOCASE",
            AlbumSort::Title => "a.title COLLATE NOCASE",
            AlbumSort::Year => "a.year DESC, a.title COLLATE NOCASE",
            AlbumSort::Added => "r.added_at DESC, a.title COLLATE NOCASE",
            AlbumSort::Played => "r.plays DESC, a.title COLLATE NOCASE",
            AlbumSort::LastPlayed => "r.last_played DESC, a.title COLLATE NOCASE",
        }
    }
}

impl Library {
    /// The albums, sorted by artist and title, with their track count.
    ///
    /// One query with a `GROUP BY`: in QBZ every page aggregated the whole
    /// library again, and that was the audit's high finding.
        pub fn albums(&self, limit: usize, offset: usize) -> rusqlite::Result<Vec<StoredAlbum>> {
        let mut stmt = self.db.prepare(
            "SELECT a.id, a.title, a.album_artist, a.year, a.artwork_path, count(t.id)
             FROM albums a
             JOIN tracks t ON t.album_id = a.id AND t.present = 1
             GROUP BY a.id
             ORDER BY a.album_artist COLLATE NOCASE, a.year, a.title COLLATE NOCASE
             LIMIT ?1 OFFSET ?2",
        )?;
        let rows = stmt.query_map(params![limit as i64, offset as i64], |r| {
            Ok(StoredAlbum {
                id: r.get(0)?,
                title: r.get(1)?,
                album_artist: r.get(2)?,
                year: r.get(3)?,
                artwork_path: r.get(4)?,
                track_count: r.get::<_, i64>(5)? as usize,
            })
        })?;
        rows.collect()
    }

    /// The albums with their play count, in whatever order is asked for.
    ///
    /// One query: the plays come from a `GROUP BY` over the log joined once,
    /// not from a query per album. Asking the database once per row of a grid
    /// is exactly what made QBZ's library slow.
        pub fn albums_sorted(
        &self,
        order: AlbumSort,
        limit: usize,
        offset: usize,
    ) -> rusqlite::Result<Vec<(StoredAlbum, usize)>> {
        self.albums_filtered(order, None, limit, offset)
    }

    /// The records by someone else that an artist plays on -- the
    /// compilations they are on -- newest first, with their play counts.
    pub fn albums_appearing(&self, artist: &str) -> rusqlite::Result<Vec<(StoredAlbum, usize)>> {
        let mut stmt = self.db.prepare(
            "SELECT a.id, a.title, a.album_artist, a.year, a.artwork_path, r.present_tracks, r.plays
             FROM albums a
             JOIN album_rollup r ON r.album_id = a.id AND r.present_tracks > 0
             WHERE a.album_artist <> ?1 COLLATE NOCASE
               AND EXISTS (SELECT 1 FROM tracks t
                           WHERE t.album_id = a.id AND t.present = 1 AND t.artist = ?1 COLLATE NOCASE)
             ORDER BY a.year DESC, a.title COLLATE NOCASE",
        )?;
        let rows = stmt.query_map(params![artist], |r| {
            Ok((
                StoredAlbum {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    album_artist: r.get(2)?,
                    year: r.get(3)?,
                    artwork_path: r.get(4)?,
                    track_count: r.get::<_, i64>(5)? as usize,
                },
                r.get::<_, i64>(6)? as usize,
            ))
        })?;
        rows.collect()
    }

    /// The same, but only one artist's when one is asked for.
    pub fn albums_filtered(
        &self,
        order: AlbumSort,
        artist: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> rusqlite::Result<Vec<(StoredAlbum, usize)>> {
        // The page is trimmed first and only then is anything counted, for
        // the hundred rows that are going to come out. Counting the whole
        // library on every page was the QBZ audit's high finding; doing it
        // for three of the six orderings was what was left of it here.
        //
        // The filter goes as a parameter, never interpolated: a name with
        // quotes in it has no business breaking a query.
        let sql = format!(
            "WITH page AS MATERIALIZED (
                 SELECT a.id AS id
                 FROM albums a
                 JOIN album_rollup r ON r.album_id = a.id AND likely(r.present_tracks > 0)
                 WHERE (?3 IS NULL OR a.album_artist = ?3 COLLATE NOCASE)
                 ORDER BY {}
                 LIMIT ?1 OFFSET ?2
             )
             SELECT a.id, a.title, a.album_artist, a.year, a.artwork_path,
                    r.present_tracks,
                    r.plays
             FROM page g
             JOIN albums a ON a.id = g.id
             JOIN album_rollup r ON r.album_id = a.id
             ORDER BY {}",
            order.order_by(),
            order.order_by()
        );
        let mut stmt = self.db.prepare(&sql)?;
        let rows = stmt.query_map(params![limit as i64, offset as i64, artist], |r| {
            Ok((
                StoredAlbum {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    album_artist: r.get(2)?,
                    year: r.get(3)?,
                    artwork_path: r.get(4)?,
                    track_count: r.get::<_, i64>(5)? as usize,
                },
                r.get::<_, i64>(6)? as usize,
            ))
        })?;
        rows.collect()
    }

    /// Every album with something present, in that order: the snapshot a
    /// grid of the whole library scrolls over, as with the tracks. Ids only,
    /// so ten thousand albums are one query and 80 KB.
    pub fn album_ids(&self, order: AlbumSort) -> rusqlite::Result<Vec<i64>> {
        let sql = format!(
            "SELECT a.id FROM albums a
             JOIN album_rollup r ON r.album_id = a.id AND likely(r.present_tracks > 0)
             ORDER BY {}",
            order.order_by()
        );
        let mut stmt = self.db.prepare(&sql)?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        rows.collect()
    }

    /// The same, keeping only the records whose title or artist holds every
    /// word of `text`: the Albums screen's filter.
    pub fn album_ids_matching(&self, order: AlbumSort, text: &str) -> rusqlite::Result<Vec<i64>> {
        let words = crate::store::find::patterns(text);
        if words.is_empty() {
            return self.album_ids(order);
        }
        let clause = " AND (a.title LIKE ? ESCAPE '\\' OR a.album_artist LIKE ? ESCAPE '\\')".repeat(words.len());
        let sql = format!(
            "SELECT a.id FROM albums a
             JOIN album_rollup r ON r.album_id = a.id AND likely(r.present_tracks > 0)
             WHERE 1 {clause}
             ORDER BY {}",
            order.order_by()
        );
        let args: Vec<&String> = words.iter().flat_map(|w| [w, w]).collect();
        let mut stmt = self.db.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(args), |r| r.get(0))?;
        rows.collect()
    }

    /// Those albums with their play counts, in the order given. By primary
    /// key: a page anywhere in the snapshot costs what the first one costs.
    pub fn albums_by_ids(&self, ids: &[i64]) -> rusqlite::Result<Vec<(StoredAlbum, usize)>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let marks = vec!["?"; ids.len()].join(",");
        let sql = format!(
            "SELECT a.id, a.title, a.album_artist, a.year, a.artwork_path, r.present_tracks, r.plays
             FROM albums a JOIN album_rollup r ON r.album_id = a.id
             WHERE a.id IN ({marks})"
        );
        let mut stmt = self.db.prepare(&sql)?;
        let found: std::collections::HashMap<i64, (StoredAlbum, usize)> = stmt
            .query_map(rusqlite::params_from_iter(ids.iter()), |r| {
                Ok((
                    StoredAlbum {
                        id: r.get(0)?,
                        title: r.get(1)?,
                        album_artist: r.get(2)?,
                        year: r.get(3)?,
                        artwork_path: r.get(4)?,
                        track_count: r.get::<_, i64>(5)? as usize,
                    },
                    r.get::<_, i64>(6)? as usize,
                ))
            })?
            .map(|row| row.map(|(a, n)| (a.id, (a, n))))
            .collect::<rusqlite::Result<_>>()?;
        Ok(ids.iter().filter_map(|id| found.get(id).cloned()).collect())
    }

    /// The library's artists, by name.
    ///
    /// An artist with no present tracks does not show up: the view shows what
    /// can be played, not what was once there.
    pub fn artists(&self, limit: usize, offset: usize) -> rusqlite::Result<Vec<StoredArtist>> {
        // It groups over albums and not over tracks: the track count comes
        // from the rollup, so this walks five thousand rows where it used to
        // walk fifty thousand. Same answer, and the join it removes was the
        // whole cost.
        //
        // The one exception is a compilation: whoever plays on it is an
        // artist of the library too, with the record counted as theirs and
        // their own tracks on it. Those are read per track, but only for the
        // records credited to "Various Artists".
        let mut stmt = self.db.prepare(
            "SELECT name, count(DISTINCT album), sum(tracks), max(art)
             FROM (
                 SELECT a.album_artist AS name, a.id AS album, r.present_tracks AS tracks,
                        a.artwork_path AS art
                 FROM albums a
                 JOIN album_rollup r ON r.album_id = a.id AND likely(r.present_tracks > 0)
                 WHERE a.album_artist <> ''
                 UNION ALL
                 SELECT t.artist, a.id, 1, a.artwork_path
                 FROM albums a
                 JOIN tracks t ON t.album_id = a.id AND t.present = 1
                 WHERE a.album_artist = ?3 AND t.artist <> '' AND t.artist <> ?3
             )
             GROUP BY name COLLATE NOCASE
             ORDER BY name COLLATE NOCASE
             LIMIT ?1 OFFSET ?2",
        )?;
        let rows = stmt.query_map(params![limit as i64, offset as i64, VARIOUS_ARTISTS], |r| {
            Ok(StoredArtist {
                name: r.get(0)?,
                album_count: r.get::<_, i64>(1)? as usize,
                track_count: r.get::<_, i64>(2)? as usize,
                artwork_path: r.get(3)?,
            })
        })?;
        rows.collect()
    }

    /// One album by its id, with its play count.
    pub fn album(&self, album_id: i64) -> rusqlite::Result<Option<(StoredAlbum, usize)>> {
        let album = self
            .db
            .query_row(
                "SELECT a.id, a.title, a.album_artist, a.year, a.artwork_path, count(t.id)
                 FROM albums a
                 JOIN tracks t ON t.album_id = a.id AND t.present = 1
                 WHERE a.id = ?1
                 GROUP BY a.id",
                params![album_id],
                |r| {
                    Ok(StoredAlbum {
                        id: r.get(0)?,
                        title: r.get(1)?,
                        album_artist: r.get(2)?,
                        year: r.get(3)?,
                        artwork_path: r.get(4)?,
                        track_count: r.get::<_, i64>(5)? as usize,
                    })
                },
            )
            .optional()?;
        match album {
            Some(a) => {
                let listens = self.album_play_count(album_id)?;
                Ok(Some((a, listens)))
            }
            None => Ok(None),
        }
    }

    /// An album's tracks, in the record's order.
    pub fn album_tracks(&self, album_id: i64) -> rusqlite::Result<Vec<StoredTrack>> {
        let mut stmt = self.db.prepare(
            "SELECT t.id, t.external_id, t.title, t.artist, COALESCE(a.title, ''),
                    t.track_number, t.duration_ms, t.present
             FROM tracks t LEFT JOIN albums a ON a.id = t.album_id
             WHERE t.album_id = ?1
             ORDER BY t.disc_number, t.track_number, t.title COLLATE NOCASE",
        )?;
        let rows = stmt.query_map(params![album_id], row_to_track)?;
        rows.collect()
    }

    /// The most played albums, with their play count.
    ///
    /// It only counts what played more than halfway, like
    /// [`Library::play_count`]. It is the first smart list: it comes from the
    /// event log, not from a counter somebody has to maintain by hand.
    pub fn most_played_albums(&self, limit: usize) -> rusqlite::Result<Vec<(StoredAlbum, usize)>> {
        let mut stmt = self.db.prepare(
            "SELECT a.id, a.title, a.album_artist, a.year, a.artwork_path,
                    count(DISTINCT t.id), count(p.id)
             FROM albums a
             JOIN tracks t ON t.album_id = a.id
             JOIN play_events p ON p.track_id = t.id
                  AND (p.duration_ms = 0 OR p.played_ms * 2 >= p.duration_ms)
             GROUP BY a.id
             ORDER BY count(p.id) DESC, a.title COLLATE NOCASE
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |r| {
            Ok((
                StoredAlbum {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    album_artist: r.get(2)?,
                    year: r.get(3)?,
                    artwork_path: r.get(4)?,
                    track_count: r.get::<_, i64>(5)? as usize,
                },
                r.get::<_, i64>(6)? as usize,
            ))
        })?;
        rows.collect()
    }

    /// How many times each track of an album has been played.
    pub fn album_play_count(&self, album_id: i64) -> rusqlite::Result<usize> {
        let n: i64 = self.db.query_row(
            "SELECT count(*) FROM play_events p
             JOIN tracks t ON t.id = p.track_id
             WHERE t.album_id = ?1
               AND (p.duration_ms = 0 OR p.played_ms * 2 >= p.duration_ms)",
            params![album_id],
            |r| r.get(0),
        )?;
        Ok(n as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::fixtures::*;

    #[test]
    fn each_view_sorts_the_grid_its_own_way() {
        let lib = sample_library();
        assert_eq!(
            titles(&lib, AlbumSort::Artist),
            ["Bayou", "Cosmo", "Aqualung"],
            "artist, then year"
        );
        assert_eq!(titles(&lib, AlbumSort::Title), ["Aqualung", "Bayou", "Cosmo"]);
        assert_eq!(titles(&lib, AlbumSort::Year), ["Aqualung", "Cosmo", "Bayou"], "newest first");
        assert_eq!(titles(&lib, AlbumSort::Added), ["Aqualung", "Cosmo", "Bayou"], "the last thing that came in");
        assert_eq!(titles(&lib, AlbumSort::Played), ["Cosmo", "Bayou", "Aqualung"]);
        assert_eq!(
            titles(&lib, AlbumSort::LastPlayed),
            ["Cosmo", "Bayou", "Aqualung"],
            "the last thing that played, and what never played at the end"
        );
    }

    #[test]
    fn an_album_that_never_played_sorts_last_and_not_first() {
        // The ordering used to wrap last_played in a COALESCE to put a null
        // at the end. SQLite already sorts a null last on DESC, and the
        // COALESCE was the reason the index could not serve the ordering.
        // Same answer, so this is what says the answer is the same.
        let lib = sample_library();
        lib.db
            .execute(
                "INSERT INTO albums (id, title, album_artist, year)
                 VALUES (4, 'Zeta', 'Nobody', 1990)",
                [],
            )
            .unwrap();
        let source = lib.source_id("local", "default").unwrap();
        lib.db
            .execute(
                "INSERT INTO tracks (id, source_id, external_id, title, album_id,
                                     duration_ms, added_at)
                 VALUES (40, ?1, '/z.flac', 'Z', 4, 200000, 1)",
                params![source],
            )
            .unwrap();

        let order = titles(&lib, AlbumSort::LastPlayed);
        assert_eq!(
            order,
            ["Cosmo", "Bayou", "Aqualung", "Zeta"],
            "what played is first, and what never played comes after it, by title"
        );
    }

    #[test]
    fn the_snapshot_of_the_grid_is_the_grids_order() {
        let lib = sample_library();
        for order in AlbumSort::all() {
            let ids = lib.album_ids(order).unwrap();
            let paged: Vec<i64> = lib.albums_sorted(order, 50, 0).unwrap().into_iter().map(|(a, _)| a.id).collect();
            assert_eq!(ids, paged, "{}", order.name());
        }
    }

    #[test]
    fn albums_come_back_in_the_order_they_were_asked_for() {
        let lib = sample_library();
        let rows: Vec<(i64, usize)> =
            lib.albums_by_ids(&[3, 1]).unwrap().into_iter().map(|(a, n)| (a.id, n)).collect();
        assert_eq!(rows, [(3, 2), (1, 1)], "with their plays");
        assert!(lib.albums_by_ids(&[]).unwrap().is_empty());
    }

    #[test]
    fn the_grid_brings_play_counts_in_the_same_query() {
        let lib = sample_library();
        let listens: Vec<(String, usize)> = lib
            .albums_sorted(AlbumSort::Played, 50, 0)
            .unwrap()
            .into_iter()
            .map(|(a, n)| (a.title, n))
            .collect();
        assert_eq!(
            listens,
            vec![("Cosmo".into(), 2), ("Bayou".into(), 1), ("Aqualung".into(), 0)]
        );
    }


    #[test]
    fn a_single_album_brings_its_own_and_its_plays() {
        let lib = sample_library();
        let (album, listens) = lib.album(3).unwrap().expect("the album exists");
        assert_eq!(album.title, "Cosmo");
        assert_eq!(album.album_artist, "Creedence");
        assert_eq!(album.track_count, 1);
        assert_eq!(listens, 2);
        assert!(lib.album(99).unwrap().is_none(), "one that does not exist is not an error");
    }

    #[test]
    fn artists_come_out_with_what_they_have() {
        let lib = sample_library();
        let artists = lib.artists(50, 0).unwrap();
        assert_eq!(
            artists.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(),
            ["Creedence", "Tull"],
            "by name"
        );
        assert_eq!(artists[0].album_count, 2, "Creedence carries two albums");
        assert_eq!(artists[0].track_count, 2, "with one track each");
        assert_eq!(artists[1].album_count, 1);
    }

    #[test]
    fn the_grid_can_be_filtered_by_artist() {
        let lib = sample_library();
        let own: Vec<String> = lib
            .albums_filtered(AlbumSort::Title, Some("Creedence"), 50, 0)
            .unwrap()
            .into_iter()
            .map(|(a, _)| a.title)
            .collect();
        assert_eq!(own, ["Bayou", "Cosmo"]);

        // With no filter, all of them come out.
        assert_eq!(lib.albums_filtered(AlbumSort::Title, None, 50, 0).unwrap().len(), 3);
        // And a name that does not exist brings nothing, without crashing.
        assert!(lib.albums_filtered(AlbumSort::Title, Some("Nobody"), 50, 0).unwrap().is_empty());
    }

    #[test]
    fn a_name_with_quotes_does_not_break_the_query() {
        let lib = sample_library();
        let odd = "O'Connor'; DROP TABLE albums; --";
        assert!(lib.albums_filtered(AlbumSort::Title, Some(odd), 50, 0).unwrap().is_empty());
        assert_eq!(lib.albums_sorted(AlbumSort::Title, 50, 0).unwrap().len(), 3, "the table is still there");
    }

    #[test]
    fn an_unknown_view_name_does_not_leave_the_grid_empty() {
        assert_eq!(AlbumSort::from_name("whatever"), AlbumSort::Artist);
        for v in AlbumSort::all() {
            assert_eq!(AlbumSort::from_name(v.name()), v, "{} goes there and back", v.name());
        }
    }

    #[test]
    fn the_most_played_come_from_the_log_and_in_order() {
        let lib = Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        for (album, title) in [(1, "One"), (2, "Two")] {
            lib.db
                .execute(
                    "INSERT INTO albums (id, title, album_artist) VALUES (?1, ?2, 'Artist')",
                    params![album, title],
                )
                .unwrap();
            lib.db
                .execute(
                    "INSERT INTO tracks (source_id, external_id, title, album_id, duration_ms, added_at)
                     VALUES (?1, ?2, 'Track', ?3, 1000, 0)",
                    params![source, format!("/{title}.flac"), album],
                )
                .unwrap();
        }
        // Album 2 is played three times; album 1, once.
        lib.log_play(Some(1), Some(source), 1, 1000, 1000, false, None, None, true).unwrap();
        for i in 0..3 {
            lib.log_play(Some(2), Some(source), 10 + i, 1000, 1000, false, None, None, true)
                .unwrap();
        }
        // And one halfway through, which does not count.
        lib.log_play(Some(1), Some(source), 20, 100, 1000, true, None, None, true).unwrap();

        let top = lib.most_played_albums(10).unwrap();
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].0.title, "Two");
        assert_eq!(top[0].1, 3);
        assert_eq!(top[1].1, 1, "a half play does not count");
        assert_eq!(lib.album_play_count(2).unwrap(), 3);
    }

}

#[cfg(test)]
mod compilation_tests {
    use crate::store::Library;
    use rusqlite::params;

    /// A record of their own by Aimer, and a compilation Aimer and YUI are on.
    fn shelf() -> Library {
        let lib = Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        for (id, title, artist, year) in [(1, "Sun Dance", "Aimer", 2019), (2, "OP&ED", "Various Artists", 2024)] {
            lib.db
                .execute(
                    "INSERT INTO albums (id, title, album_artist, year) VALUES (?1, ?2, ?3, ?4)",
                    params![id, title, artist, year],
                )
                .unwrap();
        }
        for (id, album, artist) in [(10, 1, "Aimer"), (20, 2, "Aimer"), (21, 2, "YUI"), (22, 2, "YUI")] {
            lib.db
                .execute(
                    "INSERT INTO tracks (id, source_id, external_id, title, artist, album_id, duration_ms, added_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1000, ?1)",
                    params![id, source, format!("/{id}.flac"), format!("T{id}"), artist, album],
                )
                .unwrap();
        }
        lib
    }

    #[test]
    fn whoever_plays_on_a_compilation_is_an_artist_with_it_counted() {
        let lib = shelf();
        let artists: Vec<(String, usize, usize)> =
            lib.artists(10, 0).unwrap().into_iter().map(|a| (a.name, a.album_count, a.track_count)).collect();
        assert_eq!(
            artists,
            [
                ("Aimer".to_string(), 2, 2),
                ("Various Artists".to_string(), 1, 3),
                ("YUI".to_string(), 1, 2)
            ],
            "Aimer: their record and the compilation, one track on each"
        );
    }

    #[test]
    fn the_albums_filter_looks_at_the_title_and_the_artist() {
        let lib = shelf();
        assert_eq!(lib.album_ids_matching(super::AlbumSort::Title, "sun").unwrap(), [1]);
        assert_eq!(lib.album_ids_matching(super::AlbumSort::Title, "various").unwrap(), [2]);
        assert_eq!(lib.album_ids_matching(super::AlbumSort::Title, "").unwrap().len(), 2, "nothing typed, everything");
        assert!(lib.album_ids_matching(super::AlbumSort::Title, "sun various").unwrap().is_empty(), "every word");
    }

    #[test]
    fn an_artist_appears_on_the_compilations_they_play_on() {
        let lib = shelf();
        let on: Vec<i64> = lib.albums_appearing("aimer").unwrap().into_iter().map(|(a, _)| a.id).collect();
        assert_eq!(on, [2], "not their own record");
        assert!(lib.albums_appearing("Various Artists").unwrap().is_empty());
    }
}
