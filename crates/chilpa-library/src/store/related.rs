//! The records that are like another.
//!
//! "Like" is said by genres, and most genres say nothing: in a library
//! where a third of the records are "Rock", sharing it is sharing nothing,
//! and ranking by it filled the shelf with whatever was near in years. So a
//! genre counts for as much as it is rare, and one that more than a fifth
//! of the records carry counts for nothing.
//!
//! A record is about its own genres and those of the other records of
//! whoever made it: one edition tagged "Rock" alone still belongs with what
//! its artist's other records say. When nothing is like it, the answer is
//! nothing. A shelf is not filled for the sake of it.

use super::{Library, StoredAlbum};
use std::collections::{HashMap, HashSet};

/// A genre that this share of the records carry says nothing about one.
const SAYS_NOTHING: f64 = 0.20;
/// What two records have to share, in weight, to be alike at all.
const ENOUGH: f64 = 0.5;
/// And how much of what the record is about: half of it.
const ENOUGH_OF_IT: f64 = 0.5;
/// How many records of one artist a shelf takes.
const OF_ONE_ARTIST: usize = 2;

struct Record {
    album: StoredAlbum,
    plays: usize,
    sources: HashSet<i64>,
    /// What it is about: its genres and its artist's.
    about: HashSet<i64>,
}

pub(crate) struct Profiles {
    read_at: (i64, u64),
    records: HashMap<i64, Record>,
    weights: HashMap<i64, f64>,
}

fn key(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

impl Library {
    /// The records of the same source that are like this one, the most
    /// alike first and the nearest in years among equals. None of its own
    /// artists', which have their own shelf. Empty when nothing is like it.
    pub fn related_albums(&self, album_id: i64, limit: usize) -> rusqlite::Result<Vec<(StoredAlbum, usize)>> {
        let now = self.written();
        if self.related.borrow().as_ref().is_none_or(|p| p.read_at != now) {
            let read = self.read_profiles(now)?;
            *self.related.borrow_mut() = Some(read);
        }
        let kept = self.related.borrow();
        let profiles = kept.as_ref().expect("read above");
        let Some(target) = profiles.records.get(&album_id) else { return Ok(Vec::new()) };
        let weight = |genre: &i64| profiles.weights.get(genre).copied().unwrap_or(0.0);
        let whole: f64 = target.about.iter().map(weight).sum();
        if whole < ENOUGH {
            return Ok(Vec::new());
        }
        let own: HashSet<String> = self.credit_names(&target.album.album_artist)?.iter().map(|n| key(n)).collect();
        let mut alike: Vec<(f64, i64, &Record)> = Vec::new();
        for (id, record) in &profiles.records {
            if *id == album_id || record.album.track_count == 0 || record.sources.is_disjoint(&target.sources) {
                continue;
            }
            let score: f64 = record.about.intersection(&target.about).map(weight).sum();
            if score < ENOUGH || score < ENOUGH_OF_IT * whole {
                continue;
            }
            if own.contains(&key(&record.album.album_artist)) || self.credit_names(&record.album.album_artist)?.iter().any(|n| own.contains(&key(n))) {
                continue;
            }
            let years = match (record.album.year, target.album.year) {
                (Some(a), Some(b)) => (i64::from(a) - i64::from(b)).abs(),
                _ => i64::MAX,
            };
            alike.push((score, years, record));
        }
        // Equals in an order that does not move under a session and is not
        // the order they were added in.
        let shuffled = |id: i64| (id.wrapping_mul(1_103_515_245).wrapping_add(album_id)) & 0x7fff_ffff;
        alike.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)).then(shuffled(a.2.album.id).cmp(&shuffled(b.2.album.id))));
        let mut of_artist: HashMap<String, usize> = HashMap::new();
        let mut titles: HashSet<(String, String)> = HashSet::new();
        let mut out = Vec::new();
        for (_, _, record) in alike {
            let artist = key(&record.album.album_artist);
            // The same record from two folders is one record on a shelf.
            if !titles.insert((key(&record.album.title), artist.clone())) {
                continue;
            }
            let taken = of_artist.entry(artist).or_default();
            if *taken >= OF_ONE_ARTIST {
                continue;
            }
            *taken += 1;
            out.push((record.album.clone(), record.plays));
            if out.len() >= limit {
                break;
            }
        }
        Ok(out)
    }

    fn read_profiles(&self, read_at: (i64, u64)) -> rusqlite::Result<Profiles> {
        let mut records: HashMap<i64, Record> = HashMap::new();
        {
            let mut stmt = self.db.prepare(
                concat!("SELECT a.id, a.title, a.album_artist, a.year, ", album_art!("a"), ", r.present_tracks, r.plays
                 FROM albums a JOIN album_rollup r ON r.album_id = a.id AND r.present_tracks > 0"),
            )?;
            let rows = stmt.query_map([], |r| {
                Ok((StoredAlbum { id: r.get(0)?, title: r.get(1)?, album_artist: r.get(2)?, year: r.get(3)?, artwork_path: r.get(4)?, track_count: r.get::<_, i64>(5)? as usize }, r.get::<_, i64>(6)? as usize))
            })?;
            for row in rows {
                let (album, plays) = row?;
                records.insert(album.id, Record { album, plays, sources: HashSet::new(), about: HashSet::new() });
            }
        }
        {
            let mut stmt = self.db.prepare("SELECT DISTINCT album_id, source_id FROM tracks WHERE present = 1 AND album_id IS NOT NULL")?;
            let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?;
            for row in rows {
                let (album, source) = row?;
                if let Some(record) = records.get_mut(&album) {
                    record.sources.insert(source);
                }
            }
        }
        let mut carried: HashMap<i64, usize> = HashMap::new();
        let mut of_artist: HashMap<String, HashSet<i64>> = HashMap::new();
        let mut with_genres = 0usize;
        {
            let mut stmt = self.db.prepare(
                "SELECT DISTINCT t.album_id, tg.genre_id FROM tracks t JOIN track_genres tg ON tg.track_id = t.id
                 WHERE t.present = 1 AND t.album_id IS NOT NULL ORDER BY t.album_id",
            )?;
            let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?;
            for row in rows {
                let (album, genre) = row?;
                let Some(record) = records.get_mut(&album) else { continue };
                if record.about.is_empty() {
                    with_genres += 1;
                }
                record.about.insert(genre);
                *carried.entry(genre).or_default() += 1;
                let artist = key(&record.album.album_artist);
                // A compilation's "artist" made nothing else.
                if !artist.is_empty() && artist != key(crate::VARIOUS_ARTISTS) {
                    of_artist.entry(artist).or_default().insert(genre);
                }
            }
        }
        for record in records.values_mut() {
            if let Some(genres) = of_artist.get(&key(&record.album.album_artist)) {
                record.about.extend(genres);
            }
        }
        let records_with = with_genres.max(1) as f64;
        let weights = carried.into_iter().map(|(genre, n)| (genre, (1.0 - (n as f64 / records_with) / SAYS_NOTHING).max(0.0))).collect();
        Ok(Profiles { read_at, records, weights })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    /// Eighty records of "Rock", so it says nothing, and a few that say more.
    fn library() -> Library {
        let lib = Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        let mut next = 0;
        let mut record = |artist: &str, title: &str, year: i64, genres: &[&str]| -> i64 {
            next += 1;
            lib.db.execute("INSERT INTO albums (id, title, album_artist, year, folder) VALUES (?1, ?2, ?3, ?4, ?2)", params![next, title, artist, year]).unwrap();
            lib.db
                .execute("INSERT INTO tracks (id, source_id, external_id, title, artist, album_id, added_at) VALUES (?1, ?2, ?3, 'T', ?4, ?1, 1)", params![next, source, format!("/{next}.flac"), artist])
                .unwrap();
            lib.set_track_genres(next, &genres.iter().map(|g| g.to_string()).collect::<Vec<_>>()).unwrap();
            next
        };
        for n in 0..80 {
            record(&format!("Band {n}"), &format!("Record {n}"), 1990, &["Rock"]);
        }
        record("Band A", "Plain", 1992, &["Rock"]);
        record("Band A", "Telling", 1993, &["Rock", "Latin"]);
        record("Band B", "Two Years On", 1994, &["Rock", "Latin"]);
        record("Band B", "Three Years On", 1995, &["Latin"]);
        record("Band B", "Seven Years On", 1999, &["Latin"]);
        record("Band C", "Alone", 1989, &["Rock"]);
        drop(record);
        lib
    }

    fn titles(lib: &Library, of: &str) -> Vec<String> {
        let id: i64 = lib.db.query_row("SELECT id FROM albums WHERE title = ?1", params![of], |r| r.get(0)).unwrap();
        lib.related_albums(id, 12).unwrap().into_iter().map(|(a, _)| a.title).collect()
    }

    #[test]
    fn a_record_is_like_what_its_artists_other_records_say() {
        let lib = library();
        // Tagged "Rock" alone, and its artist's other record says more.
        assert_eq!(titles(&lib, "Plain"), ["Two Years On", "Three Years On"], "nearest in years first, two of one artist");
    }

    #[test]
    fn a_genre_everybody_carries_makes_nothing_alike() {
        let lib = library();
        assert!(titles(&lib, "Alone").is_empty(), "a shelf is not filled for the sake of it");
        assert!(titles(&lib, "Record 3").is_empty());
    }

    #[test]
    fn its_own_artists_records_are_on_another_shelf() {
        let lib = library();
        assert!(!titles(&lib, "Telling").contains(&"Plain".to_string()));
    }

    #[test]
    fn what_changes_in_the_library_is_seen() {
        let lib = library();
        assert!(titles(&lib, "Alone").is_empty());
        let cure: i64 = lib.db.query_row("SELECT id FROM tracks WHERE artist = 'Band C'", [], |r| r.get(0)).unwrap();
        lib.set_track_genres(cure, &["Rock".into(), "Latin".into()]).unwrap();
        assert!(!titles(&lib, "Alone").is_empty());
    }
}
