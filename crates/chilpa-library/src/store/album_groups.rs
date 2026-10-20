//! What each record is filed under when a grid groups it by one of its
//! tags: the genre, label or composer most of its present tracks carry, the
//! source most of them come from, and what its files are. One query over the
//! whole library, run only when a grid asks for that grouping.

use super::Library;
use rusqlite::OptionalExtension;
use std::collections::HashMap;

impl Library {
    /// The value most of each record's present tracks carry: its "genre",
    /// "label", "composer" or "source" (`provider:instance`). A tie goes to
    /// the name first in the alphabet.
    pub fn album_primaries(&self, kind: &str) -> rusqlite::Result<HashMap<i64, String>> {
        let sql = match kind {
            "genre" => "SELECT t.album_id, g.name, count(*) FROM tracks t JOIN track_genres tg ON tg.track_id = t.id
                JOIN genres g ON g.id = tg.genre_id WHERE t.present = 1 AND t.album_id IS NOT NULL GROUP BY t.album_id, g.name",
            "label" => "SELECT t.album_id, min(t.label), count(*) FROM tracks t WHERE t.present = 1 AND t.album_id IS NOT NULL
                AND t.label IS NOT NULL AND t.label <> '' GROUP BY t.album_id, t.label COLLATE NOCASE",
            "composer" => "SELECT t.album_id, c.name, count(*) FROM tracks t JOIN track_composers tc ON tc.track_id = t.id
                JOIN composers c ON c.id = tc.composer_id WHERE t.present = 1 AND t.album_id IS NOT NULL GROUP BY t.album_id, c.name",
            "source" => "SELECT t.album_id, s.provider || ':' || s.instance, count(*) FROM tracks t JOIN sources s ON s.id = t.source_id
                WHERE t.present = 1 AND t.album_id IS NOT NULL GROUP BY t.album_id, s.id",
            _ => return Ok(HashMap::new()),
        };
        let mut best: HashMap<i64, (String, i64)> = HashMap::new();
        let mut stmt = self.db.prepare(sql)?;
        for row in stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)))? {
            let (album, name, count) = row?;
            match best.get(&album) {
                Some((held, most)) if *most > count || (*most == count && super::order_keys::nocase(held, &name).is_le()) => {}
                _ => { best.insert(album, (name, count)); }
            }
        }
        Ok(best.into_iter().map(|(album, (name, _))| (album, name)).collect())
    }

    /// One record's present tracks, how long they last and the genre most
    /// of them carry (a tie to the first name): what a list of records
    /// shows beside its title. By the record's index, cheap enough per row.
    pub fn album_facts(&self, album: i64) -> rusqlite::Result<(usize, i64, String)> {
        let (tracks, duration) = self.db.prepare_cached("SELECT count(*), coalesce(sum(duration_ms), 0) FROM tracks
            WHERE album_id = ?1 AND present = 1")?.query_row([album], |r| Ok((r.get::<_, i64>(0)? as usize, r.get(1)?)))?;
        let genre = self.db.prepare_cached("SELECT g.name FROM tracks t JOIN track_genres tg ON tg.track_id = t.id
            JOIN genres g ON g.id = tg.genre_id WHERE t.album_id = ?1 AND t.present = 1
            GROUP BY g.name ORDER BY count(*) DESC, g.name COLLATE NOCASE LIMIT 1")?
            .query_row([album], |r| r.get(0)).optional()?.unwrap_or_default();
        Ok((tracks, duration, genre))
    }

    /// How many present tracks each record holds and how long they last:
    /// what tells a single or an EP from an album when no tag says it.
    pub fn album_sizes(&self) -> rusqlite::Result<HashMap<i64, (usize, i64)>> {
        let mut stmt = self.db.prepare("SELECT album_id, count(*), coalesce(sum(duration_ms), 0) FROM tracks
            WHERE present = 1 AND album_id IS NOT NULL GROUP BY album_id")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, (r.get::<_, i64>(1)? as usize, r.get::<_, i64>(2)?))))?;
        rows.collect()
    }

    /// Each present track's record, address, codec, bits and rate: what a
    /// record's best quality is worked out from.
    pub fn album_files(&self) -> rusqlite::Result<Vec<(i64, String, Option<String>, Option<u8>, Option<u32>)>> {
        let mut stmt = self.db.prepare("SELECT album_id, external_id, codec, bit_depth, sample_rate_hz FROM tracks
            WHERE present = 1 AND album_id IS NOT NULL")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?;
        rows.collect()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_record_is_filed_under_what_most_of_its_tracks_carry() {
        let lib = crate::store::fixtures::sample_library();
        let source = lib.source_id("local", "default").unwrap();
        for path in ["/Bayou-2.flac", "/Bayou-3.flac"] {
            lib.connection().execute("INSERT INTO tracks (source_id, external_id, title, album_id, duration_ms, added_at)
                VALUES (?1, ?2, 'Track', 1, 1000, 1)", rusqlite::params![source, path]).unwrap();
        }
        let ids: Vec<i64> = lib.connection().prepare("SELECT id FROM tracks WHERE album_id = 1 ORDER BY id").unwrap()
            .query_map([], |r| r.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap();
        for (n, id) in ids.iter().enumerate() {
            lib.set_track_genres(*id, &[if n == 0 { "Rock".into() } else { "Jazz".into() }]).unwrap();
        }
        lib.set_track_genres(ids[0], &["Rock".into(), "Blues".into()]).unwrap();
        lib.connection().execute("UPDATE tracks SET label = 'label one' WHERE album_id = 1", []).unwrap();
        assert_eq!(lib.album_primaries("genre").unwrap().get(&1).map(String::as_str), Some("Jazz"), "two tracks against one");
        assert_eq!(lib.album_primaries("label").unwrap().get(&1).map(String::as_str), Some("label one"));
        assert_eq!(lib.album_primaries("source").unwrap().get(&1).map(String::as_str), Some("local:default"));
        assert!(lib.album_primaries("anything").unwrap().is_empty());
        assert_eq!(lib.album_files().unwrap().iter().filter(|(album, ..)| *album == 1).count(), 3);
        assert_eq!(lib.album_sizes().unwrap().get(&1), Some(&(3, 3000)));
        let (tracks, duration, genre) = lib.album_facts(1).unwrap();
        assert_eq!((tracks, duration), (3, 3000));
        assert_eq!(Some(genre), lib.album_primaries("genre").unwrap().get(&1).cloned(), "the genre the genre grouping files it under");
        assert_eq!(lib.album_facts(9_999).unwrap(), (0, 0, String::new()), "a record that is not there says nothing");
    }

    #[test]
    fn a_tie_goes_to_the_first_name_whatever_order_the_rows_come_in() {
        let lib = crate::store::fixtures::sample_library();
        let id: i64 = lib.connection().query_row("SELECT id FROM tracks WHERE album_id = 2", [], |r| r.get(0)).unwrap();
        lib.set_track_genres(id, &["Rock".into(), "Folk".into()]).unwrap();
        assert_eq!(lib.album_primaries("genre").unwrap().get(&2).map(String::as_str), Some("Folk"));
    }
}
