//! What the search box finds as someone types: the artists and the records
//! whose names hold every word typed, and the songs the search index ranks
//! best. A record is also found by who composed its tracks, and a song by
//! who composed it, after the ones the index ranks.
//!
//! Artists include the credits on individual files, so a compilation stays
//! discoverable through every performer. Songs go through the full-text
//! index, which folds accents and ranks by how well each one matches.

use crate::store::albums::StoredArtist;
use crate::store::{fts_query, Library};
use rusqlite::types::ToSql;
use rusqlite::params_from_iter;

/// The songs ranked, out of the first this many the index matches. Ranking
/// every match of a one-letter prefix would cost what the search saves: at
/// fifty thousand tracks, 13 ms for five thousand against about 7 for one.
const CONSIDERED: usize = 1_000;

/// What a search found, best first.
#[derive(Clone, Debug, Default)]
pub struct Found {
    pub artists: Vec<StoredArtist>,
    pub albums: Vec<i64>,
    pub tracks: Vec<i64>,
}

/// The words typed, each as a `LIKE` pattern that finds it anywhere.
pub(crate) fn patterns(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| format!("%{}%", w.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")))
        .collect()
}

impl Library {
    /// Up to `limit` of each kind that matches every word of `text`.
    pub fn find(&self, text: &str, limit: usize) -> rusqlite::Result<Found> {
        let words = patterns(text);
        let Some(query) = fts_query(text) else { return Ok(Found::default()) };
        // A name with a word that starts with what was typed goes before one
        // that only holds it inside a word: "01" is Artist 010 before
        // Artist 001. One point per word typed that starts a word.
        let starts: Vec<String> = words.iter().map(|w| format!("% {}", w.trim_start_matches('%'))).collect();
        let score = |column: &str| {
            vec![format!("((' ' || {column}) LIKE ? ESCAPE '\\')"); starts.len()].join(" + ")
        };

        let mut args: Vec<Box<dyn ToSql>> = Vec::new();
        let mut artist_clause = String::new();
        for w in &words {
            artist_clause.push_str(" AND m.artist LIKE ? ESCAPE '\\'");
            args.push(Box::new(w.clone()));
        }
        args.extend(starts.iter().map(|w| Box::new(w.clone()) as Box<dyn ToSql>));
        args.push(Box::new(limit as i64));
        let sql = format!(
            concat!("SELECT min(m.artist), count(a.id), sum(m.tracks), max(", album_art!("a"), ")
             FROM albums a JOIN album_artist_rollup m ON m.album_id = a.id
             WHERE EXISTS (SELECT 1 FROM tracks t JOIN sources s ON s.id = t.source_id AND s.enabled = 1
                           WHERE t.album_id = a.id AND t.present = 1) {artist_clause}
             GROUP BY m.artist COLLATE NOCASE
             ORDER BY max({}) DESC, count(a.id) DESC, m.artist COLLATE NOCASE
             LIMIT ?"),
            score("m.artist"),
            artist_clause = artist_clause
        );
        let mut stmt = self.db.prepare(&sql)?;
        let artists = stmt
            .query_map(params_from_iter(args.iter()), |r| {
                Ok(StoredArtist {
                    name: r.get(0)?,
                    album_count: r.get::<_, i64>(1)? as usize,
                    track_count: r.get::<_, i64>(2)? as usize,
                    artwork_path: r.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let mut args: Vec<Box<dyn ToSql>> = Vec::new();
        let mut album_clause = String::new();
        for w in &words {
            album_clause.push_str(" AND (a.title LIKE ? ESCAPE '\\' OR a.id IN
                (SELECT album_id FROM album_artist_rollup WHERE artist LIKE ? ESCAPE '\\') OR a.id IN
                (SELECT t.album_id FROM tracks t JOIN track_composers tc ON tc.track_id = t.id
                 JOIN composers c ON c.id = tc.composer_id WHERE t.present = 1 AND c.name LIKE ? ESCAPE '\\'))");
            args.push(Box::new(w.clone()));
            args.push(Box::new(w.clone()));
            args.push(Box::new(w.clone()));
        }
        args.extend(starts.iter().map(|w| Box::new(w.clone()) as Box<dyn ToSql>));
        args.extend(starts.iter().map(|w| Box::new(w.clone()) as Box<dyn ToSql>));
        args.push(Box::new(limit as i64));
        let sql = format!(
            "SELECT a.id
             FROM albums a JOIN album_rollup r ON r.album_id = a.id AND r.present_tracks > 0
             WHERE EXISTS (SELECT 1 FROM tracks t JOIN sources s ON s.id = t.source_id AND s.enabled = 1
                           WHERE t.album_id = a.id AND t.present = 1) {album_clause}
             ORDER BY {} DESC, {} DESC, a.album_artist COLLATE NOCASE, a.year DESC
             LIMIT ?",
            score("a.title"),
            score("a.album_artist")
        );
        let mut stmt = self.db.prepare(&sql)?;
        let albums = stmt
            .query_map(params_from_iter(args.iter()), |r| r.get(0))?
            .collect::<rusqlite::Result<Vec<i64>>>()?;

        let mut stmt = self.db.prepare(
            "SELECT s.rowid
             FROM (SELECT rowid, rank FROM track_search WHERE track_search MATCH ?1 LIMIT ?3) s
             JOIN tracks t ON t.id = s.rowid AND t.present = 1
             JOIN sources source ON source.id = t.source_id AND source.enabled = 1
             ORDER BY s.rank
             LIMIT ?2",
        )?;
        let mut tracks = stmt
            .query_map(rusqlite::params![query, limit as i64, CONSIDERED as i64], |r| r.get(0))?
            .collect::<rusqlite::Result<Vec<i64>>>()?;
        if tracks.len() < limit && !words.is_empty() {
            // The songs whose composers say every word, after the ranked ones.
            // From the composers, a small table, to their tracks: a scan of
            // every track with a probe each costs what a library is large.
            let clause = vec!["t.id IN (SELECT tc.track_id FROM composers c JOIN track_composers tc ON tc.composer_id = c.id
                WHERE c.name LIKE ? ESCAPE '\\')"; words.len()].join(" AND ");
            let sql = format!("SELECT t.id FROM tracks t JOIN sources s ON s.id = t.source_id AND s.enabled = 1
                WHERE t.present = 1 AND {clause}
                ORDER BY t.album_id, t.disc_number, t.track_number LIMIT ?");
            let mut args: Vec<Box<dyn ToSql>> = words.iter().map(|w| Box::new(w.clone()) as Box<dyn ToSql>).collect();
            args.push(Box::new(limit as i64));
            let composed = self.db.prepare(&sql)?.query_map(params_from_iter(args.iter()), |r| r.get(0))?.collect::<rusqlite::Result<Vec<i64>>>()?;
            for id in composed {
                if tracks.len() < limit && !tracks.contains(&id) {
                    tracks.push(id);
                }
            }
        }

        Ok(Found { artists, albums, tracks })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    fn shelf() -> Library {
        let lib = Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        for (id, title, artist, year) in [
            (1, "Low Orbit", "Mora Vale", 2019),
            (2, "Salt & Static", "Mora Vale", 2016),
            (3, "Moraine", "Nils Okafor", 2021),
            (4, "100% Pure", "Halden", 2020),
        ] {
            lib.db
                .execute(
                    "INSERT INTO albums (id, title, album_artist, year) VALUES (?1, ?2, ?3, ?4)",
                    params![id, title, artist, year],
                )
                .unwrap();
        }
        for (id, album, title, artist) in [
            (10, 1, "Faded Lines", "Mora Vale"),
            (20, 2, "Quiet Letters", "Mora Vale"),
            (30, 3, "Glacier", "Nils Okafor"),
            (40, 4, "Pure", "Halden"),
        ] {
            lib.db
                .execute(
                    "INSERT INTO tracks (id, source_id, external_id, title, artist, album_id, duration_ms, added_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1000, ?1)",
                    params![id, source, format!("/{id}.flac"), title, artist, album],
                )
                .unwrap();
        }
        lib
    }

    #[test]
    fn every_word_has_to_match() {
        let lib = shelf();
        let found = lib.find("mora vale", 10).unwrap();
        let names: Vec<&str> = found.artists.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["Mora Vale"]);
        assert_eq!(found.artists[0].album_count, 2);
        assert_eq!(found.albums, [1, 2], "Moraine holds \"mora\" but not \"vale\"");
    }

    #[test]
    fn a_name_that_starts_with_the_word_goes_first() {
        let lib = shelf();
        let found = lib.find("mora", 10).unwrap();
        assert_eq!(found.albums[0], 3, "Moraine starts with it; the others only by their artist");
        assert_eq!(found.tracks.len(), 3, "the artist's two songs, and one by its record's title");
    }

    #[test]
    fn a_word_that_starts_with_it_beats_one_that_holds_it() {
        let lib = shelf();
        let found = lib.find("or", 10).unwrap();
        assert_eq!(found.albums[0], 1, "Low Orbit has a word starting with it; Moraine only holds it");
    }

    #[test]
    fn a_record_and_its_songs_are_found_by_who_composed_them() {
        let lib = shelf();
        lib.set_track_composers(30, &["John Cage".to_string()]).unwrap();
        let found = lib.find("cage", 10).unwrap();
        assert_eq!(found.albums, [3], "Moraine, by a track's composer");
        assert_eq!(found.tracks, [30]);
        assert!(lib.find("cage okafor", 10).unwrap().albums.contains(&3), "a composer and the artist, one word each");
        assert!(lib.find("bach", 10).unwrap().albums.is_empty());
    }

    #[test]
    fn a_percent_sign_is_a_character_and_not_a_wildcard() {
        let lib = shelf();
        assert!(lib.find("%", 10).unwrap().albums.is_empty(), "nothing to search for");
        assert_eq!(lib.find("100", 10).unwrap().albums, [4]);
    }
}
