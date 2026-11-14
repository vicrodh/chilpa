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

    /// The badge of the same bounded, ordered tracks shown on the album page.
    /// Read only quality columns and retain one winner, without track metadata.
    pub fn album_quality(&self, album: i64) -> rusqlite::Result<crate::quality::Quality> {
        use super::{Filter, TrackOrder};
        use crate::quality::Quality;
        let (filter, args) = self.browse_clause(&Filter { album: Some(album), ..Default::default() });
        let sql = format!("SELECT t.external_id, t.codec, t.bit_depth, t.sample_rate_hz
            FROM tracks t LEFT JOIN albums a ON a.id = t.album_id {filter}
            ORDER BY {} LIMIT 10000", TrackOrder::Disc.keys());
        let mut statement = self.db.prepare_cached(&sql)?;
        let rows = statement.query_map(rusqlite::params_from_iter(args.iter()), |row| {
            let path: String = row.get(0)?;
            let codec: Option<String> = row.get(1)?;
            Ok(Quality::of_track(&path, codec.as_deref(), row.get(2)?, row.get(3)?))
        })?;
        let mut best: Option<Quality> = None;
        for quality in rows {
            let quality = quality?;
            if best.as_ref().is_none_or(|held| (quality.tier.rank(), quality.bits, quality.rate_hz)
                >= (held.tier.rank(), held.bits, held.rate_hz)) {
                best = Some(quality);
            }
        }
        Ok(best.unwrap_or_default())
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
    fn album_badges_keep_file_codec_and_visible_member_quality_rules() {
        use crate::quality::Quality;
        let lib = crate::store::fixtures::sample_library();
        lib.connection().execute_batch("UPDATE tracks SET bit_depth = 16, sample_rate_hz = 44100;
            UPDATE tracks SET codec = 'mp3' WHERE album_id = 1;
            UPDATE tracks SET external_id = '/other.dsf', sample_rate_hz = 11289600 WHERE album_id = 2;
            INSERT INTO sources(id, provider, instance, enabled) VALUES(99, 'fixture', 'disabled', 0);
            INSERT INTO tracks(source_id, external_id, title, album_id, bit_depth, sample_rate_hz, added_at)
            VALUES(99, '/disabled.dsf', 'Disabled', 1, 1, 11289600, 1);").unwrap();
        let source = lib.source_id("local", "default").unwrap();
        let insert = |path: &str, codec: Option<&str>, bits: Option<u8>, rate: Option<u32>, present: i64| {
            lib.connection().execute("INSERT INTO tracks(source_id, external_id, title, album_id, codec,
                bit_depth, sample_rate_hz, present, added_at) VALUES(?1, ?2, 'Member', 1, ?3, ?4, ?5, ?6, 1)",
                rusqlite::params![source, path, codec, bits, rate, present]).unwrap();
        };
        assert_eq!(lib.album_quality(1).unwrap().short(), "FLAC 16/44.1", "local extension wins over codec");
        insert("/missing.dsf", None, None, Some(11289600), 0);
        insert("/lossy.mp3", None, Some(32), Some(192000), 1);
        insert("remote:fixture/account/track", Some("alac"), Some(24), Some(96000), 1);
        assert_eq!(lib.album_quality(1).unwrap().short(), "ALAC 24/96", "remote codec wins over extension");
        insert("/member.dsf", None, Some(1), Some(2822400), 1);
        assert_eq!(lib.album_quality(1).unwrap().short(), "DSD64", "DSD outranks PCM without leaking absent or disabled members");
        let before = lib.connection().total_changes();
        let rows = lib.browse_tracks(&super::super::Filter { album: Some(1), ..Default::default() },
            super::super::TrackOrder::Disc, 10_000).unwrap();
        let expected = rows.iter().map(|track| Quality::of_track(&track.external_id, track.codec.as_deref(),
            track.bit_depth, track.sample_rate_hz)).max_by_key(|quality| (quality.tier.rank(), quality.bits, quality.rate_hz)).unwrap();
        assert_eq!(lib.album_quality(1).unwrap(), expected);
        assert_eq!(lib.album_quality(999).unwrap(), Quality::default());
        assert_eq!(lib.connection().total_changes(), before, "badge reads write nothing");
    }

    #[test]
    fn album_badges_keep_the_page_limit_and_last_equal_quality_format() {
        let lib = crate::store::Library::in_memory().unwrap();
        lib.connection().execute_batch("INSERT INTO sources(id,provider,instance) VALUES(1,'local','default');
            INSERT INTO albums(id,title,album_artist) VALUES(1,'Album','Artist');
            WITH RECURSIVE numbers(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM numbers WHERE n<10001)
            INSERT INTO tracks(source_id, external_id, title, album_id, track_number, bit_depth, sample_rate_hz, added_at)
            SELECT (SELECT id FROM sources WHERE provider='local' LIMIT 1),
                CASE WHEN n=10001 THEN '/last.dsf' WHEN n=10000 THEN '/tie.wav' ELSE '/member-'||n||'.flac' END,
                'Member', 1, n, 16, CASE WHEN n=10001 THEN 2822400 ELSE 44100 END, 1 FROM numbers;").unwrap();
        assert_eq!(lib.album_quality(1).unwrap().short(), "WAV 16/44.1",
            "the last tied row wins and a stronger track beyond the page limit is excluded");
    }

    #[test]
    fn a_tie_goes_to_the_first_name_whatever_order_the_rows_come_in() {
        let lib = crate::store::fixtures::sample_library();
        let id: i64 = lib.connection().query_row("SELECT id FROM tracks WHERE album_id = 2", [], |r| r.get(0)).unwrap();
        lib.set_track_genres(id, &["Rock".into(), "Folk".into()]).unwrap();
        assert_eq!(lib.album_primaries("genre").unwrap().get(&2).map(String::as_str), Some("Folk"));
    }
}
