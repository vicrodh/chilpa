//! Read-only work navigation. A title is grouped only with the same stated
//! composers; an unattributed title stays within its record. A track without
//! a work tag joins a work only when its title prints one before a part
//! marker ("Work: I. Part") and it has a composer; no composer is inferred.
use super::Library;
use rusqlite::{params, Connection, functions::FunctionFlags};
use serde_json::{json, Value};

pub const PAGE_SIZE: usize = 60;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Composition {
    pub key: String,
    pub title: String,
    pub composers: Vec<String>,
    pub albums: usize,
    pub tracks: usize,
    pub recordings: usize,
    pub duration_ms: u64,
    pub example_track: i64,
    pub sources: Vec<String>,
    /// Whether one of its tracks has a genre of the classical family asked for.
    pub classical_genre: bool,
}

pub struct CompositionPart {
    pub id: i64,
    pub album: i64,
    pub name: String,
    pub artist: String,
    pub disc: i64,
    pub number: i64,
    pub duration_ms: u64,
    pub source: String,
    pub year: Option<i32>,
}

/// Work tags share an identity only with the same stated composers. Unicode
/// casing is defined here, rather than by the host's SQLite extensions.
pub fn work_key(title: &str, composers: &[String], unknown: Value) -> String {
    let mut authors: Vec<_> = composers.iter().map(|name| name.trim().to_lowercase()).collect();
    authors.sort();
    authors.dedup();
    let unknown = if authors.is_empty() { unknown } else { Value::Null };
    json!([title.trim().to_lowercase(), authors, unknown]).to_string()
}

pub(crate) fn register(db: &Connection) -> rusqlite::Result<()> {
    let flags = FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC;
    db.create_scalar_function("chilpa_work_sort", 1, flags, |ctx| Ok(ctx.get::<String>(0)?.to_lowercase()))?;
    db.create_scalar_function("chilpa_title_work", 2, flags, |ctx| {
        let authors: String = ctx.get(1)?;
        let authors: Vec<String> = serde_json::from_str(&authors).map_err(|error| rusqlite::Error::UserFunctionError(Box::new(error)))?;
        Ok(crate::work_titles::work_from_title(&ctx.get::<String>(0)?, &authors).unwrap_or_default())
    })?;
    db.create_scalar_function("chilpa_work_key", 3, flags, |ctx| {
        let authors: String = ctx.get(1)?;
        let authors: Vec<String> = serde_json::from_str(&authors).map_err(|error| rusqlite::Error::UserFunctionError(Box::new(error)))?;
        Ok(work_key(&ctx.get::<String>(0)?, &authors, json!(ctx.get::<i64>(2)?)))
    })
}

// Display credits stay separate from the normalized comparison key. Unknown
// authors cannot make unrelated records share an identity.
const WORKS: &str = "WITH tagged AS (
    SELECT t.id, t.album_id, t.title AS track_title, trim(coalesce(t.work, '')) AS work, t.artist, t.duration_ms,
        CASE WHEN s.provider = 'local' THEN '' ELSE coalesce(nullif(s.label, ''), s.provider) END AS source,
        (SELECT json_group_array(name) FROM (SELECT c.name FROM composers c
            JOIN track_composers tc ON tc.composer_id = c.id
            WHERE tc.track_id = t.id ORDER BY c.name COLLATE NOCASE, c.id)) AS composers,
        EXISTS (SELECT 1 FROM track_genres tg JOIN genres g ON g.id = tg.genre_id
            WHERE tg.track_id = t.id AND g.name IN (SELECT value FROM json_each(?3))) AS classical
    FROM tracks t JOIN sources s ON s.id = t.source_id
    WHERE t.present = 1 AND s.enabled = 1 AND (trim(coalesce(t.work, '')) <> ''
        OR (instr(t.title, ': ') > 0 OR instr(t.title, ' - ') > 0 OR instr(t.title, ' – ') > 0)
            AND EXISTS (SELECT 1 FROM track_composers tc WHERE tc.track_id = t.id))
), credited AS (
    SELECT id, album_id, CASE WHEN work <> '' THEN work ELSE chilpa_title_work(track_title, composers) END AS title,
        artist, duration_ms, source, composers, classical
    FROM tagged
), identified AS (
    SELECT *, chilpa_work_key(title, composers, coalesce(album_id, -id)) AS work_key
    FROM credited WHERE title <> ''
) ";

const CONDITION: &str = " WHERE (?1 = '' OR EXISTS (SELECT 1 FROM json_each(composers) WHERE chilpa_work_sort(value) = chilpa_work_sort(?1)))
    AND (?2 = '' OR instr(chilpa_fold(title), chilpa_fold(?2)) > 0 OR EXISTS
        (SELECT 1 FROM json_each(composers) WHERE instr(chilpa_fold(value), chilpa_fold(?2)) > 0))";

const SUMMARY: &str = "SELECT work_key, min(title), composers, count(DISTINCT coalesce(album_id, -id)), count(*),
    count(DISTINCT json_array(coalesce(album_id, -id), chilpa_work_sort(artist))),
    sum(max(0, duration_ms)), min(id), json_group_array(DISTINCT source), max(classical) FROM identified";

fn composition(row: &rusqlite::Row<'_>) -> rusqlite::Result<Composition> {
    let composers: String = row.get(2)?;
    let sources: String = row.get(8)?;
    Ok(Composition { key: row.get(0)?, title: row.get(1)?, composers: serde_json::from_str(&composers).unwrap_or_default(),
        albums: row.get::<_, i64>(3)? as usize, tracks: row.get::<_, i64>(4)? as usize,
        recordings: row.get::<_, i64>(5)? as usize, duration_ms: row.get::<_, i64>(6)?.max(0) as u64, example_track: row.get(7)?,
        sources: serde_json::from_str(&sources).unwrap_or_default(), classical_genre: row.get::<_, i64>(9)? != 0 })
}

impl Library {
    /// A header count does not materialize or sort a page of work cards.
    pub fn composition_count(&self, composer: &str, text: &str) -> rusqlite::Result<usize> {
        self.db.query_row(&format!("{WORKS} SELECT count(DISTINCT work_key) FROM identified{CONDITION}"),
            params![composer, text.trim(), "[]"], |row| row.get::<_, i64>(0).map(|total| total as usize))
    }

    /// Sixty entries per page, plus a total, with filtering before pagination.
    /// SQLite owns grouping temporaries; only the requested page crosses here.
    pub fn compositions(&self, composer: &str, text: &str, page: usize) -> rusqlite::Result<(Vec<Composition>, usize)> {
        let condition = CONDITION;
        let total = self.composition_count(composer, text)?;
        let sql = format!("{WORKS} {SUMMARY}{condition}
            GROUP BY work_key ORDER BY chilpa_work_sort(min(title)) COLLATE BINARY, work_key LIMIT ?4 OFFSET ?5");
        let mut statement = self.db.prepare(&sql)?;
        let rows = statement.query_map(params![composer, text.trim(), "[]", PAGE_SIZE as i64, page.saturating_mul(PAGE_SIZE).min(i64::MAX as usize) as i64], composition)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok((rows, total))
    }

    /// Stream ordered works so a merged source page retains only 60 entries.
    /// This does not materialize the library's complete work inventory in Rust.
    pub fn visit_compositions(&self, composer: &str, text: &str, mut visit: impl FnMut(Composition)) -> rusqlite::Result<()> {
        self.visit_compositions_ordered(composer, text, false, &[], &mut visit)
    }

    /// `classical` names the genres whose tracks mark a work as classical.
    pub fn visit_compositions_ordered(&self, composer: &str, text: &str, descending: bool, classical: &[String], mut visit: impl FnMut(Composition)) -> rusqlite::Result<()> {
        let direction = if descending { "DESC" } else { "ASC" };
        let sql = format!("{WORKS} {SUMMARY}{CONDITION}
            GROUP BY work_key ORDER BY chilpa_work_sort(min(title)) COLLATE BINARY {direction}, work_key {direction}");
        let mut statement = self.db.prepare(&sql)?;
        let genres = serde_json::to_string(classical).unwrap_or_else(|_| "[]".into());
        for row in statement.query_map(params![composer, text.trim(), genres], composition)? { visit(row?); }
        Ok(())
    }

    /// Composers with a track in one of the `classical` genres, folded.
    pub fn composers_in_genres(&self, classical: &[String]) -> rusqlite::Result<Vec<String>> {
        let mut statement = self.db.prepare("SELECT DISTINCT c.name FROM composers c
            JOIN track_composers tc ON tc.composer_id = c.id JOIN tracks t ON t.id = tc.track_id AND t.present = 1
            JOIN track_genres tg ON tg.track_id = t.id JOIN genres g ON g.id = tg.genre_id
            WHERE g.name IN (SELECT value FROM json_each(?1))")?;
        let genres = serde_json::to_string(classical).unwrap_or_else(|_| "[]".into());
        let rows = statement.query_map([genres], |row| row.get::<_, String>(0))?.collect();
        rows
    }

    /// Every part of the selected work, in recording/disc/track order.
    pub fn composition_parts(&self, key: &str) -> rusqlite::Result<Vec<CompositionPart>> {
        let sql = format!("{WORKS} SELECT t.id, coalesce(t.album_id, 0), coalesce(nullif(a.title, ''), t.title),
            t.artist, coalesce(t.release_disc, t.disc_number, 0), coalesce(t.track_number, 0), max(0, t.duration_ms), w.source, a.year
            FROM identified w JOIN tracks t ON t.id = w.id
            LEFT JOIN albums a ON a.id = t.album_id WHERE w.work_key = ?1
            ORDER BY a.year, a.title COLLATE NOCASE, coalesce(t.album_id, -t.id),
                coalesce(t.release_disc, t.disc_number), t.track_number, t.id");
        let mut statement = self.db.prepare(&sql)?;
        // ?2 is unused here; ?3 is the classical genre list the shared CTE reads.
        let rows = statement.query_map(params![key, "", "[]"], |row| Ok(CompositionPart {
            id: row.get(0)?, album: row.get(1)?, name: row.get(2)?, artist: row.get(3)?,
            disc: row.get(4)?, number: row.get(5)?, duration_ms: row.get::<_, i64>(6)?.max(0) as u64,
            source: row.get(7)?, year: row.get(8)?,
        }))?.collect();
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn library() -> Library {
        let lib = Library::in_memory().unwrap();
        lib.db.execute_batch("INSERT INTO sources(id, provider, instance) VALUES (1, 'local', 'default');
            INSERT INTO albums(id,title,album_artist) VALUES (1,'First','Player'), (2,'Second','Orchestra');
            INSERT INTO composers(id,name) VALUES (1,'First writer'), (2,'Second writer');
            INSERT INTO tracks(id,source_id,external_id,title,album_id,work,added_at) VALUES
            (1,1,'/1','I',1,'Sonata',1), (2,1,'/2','II',1,'Sonata',1),
            (3,1,'/3','I',2,'Sonata',1), (4,1,'/4','Unrelated',2,NULL,1);
            INSERT INTO track_composers(track_id,composer_id) VALUES (1,1),(2,1),(3,2);").unwrap();
        lib
    }

    #[test]
    fn equal_titles_by_different_composers_remain_different_works() {
        let lib = library();
        let changes = lib.db.total_changes();
        let (works, total) = lib.compositions("", "", 0).unwrap();
        assert_eq!(total, 2);
        assert_eq!(lib.composition_count("", "").unwrap(), 2);
        assert_eq!(lib.composition_count("first writer", "son").unwrap(), 1);
        assert_eq!(works.iter().map(|work| work.tracks).sum::<usize>(), 3);
        let (works, total) = lib.compositions("first writer", "son", 0).unwrap();
        assert_eq!((total, works[0].tracks), (1, 2));
        assert_eq!(lib.composition_parts(&works[0].key).unwrap().into_iter().map(|part| part.id).collect::<Vec<_>>(), [1, 2]);
        assert_eq!(works[0].recordings, 1);
        assert_eq!(lib.db.total_changes(), changes);
    }

    #[test]
    fn descending_stream_reverses_title_ties_without_merging_authors() {
        let lib = library();
        let mut ascending = Vec::new();
        let mut descending = Vec::new();
        lib.visit_compositions_ordered("", "", false, &[], |work| ascending.push(work)).unwrap();
        lib.visit_compositions_ordered("", "", true, &[], |work| descending.push(work)).unwrap();
        assert_eq!(ascending.len(), 2);
        assert_eq!(ascending[0].title, ascending[1].title);
        assert_ne!(ascending[0].key, ascending[1].key);
        ascending.reverse();
        assert_eq!(descending, ascending);
    }

    #[test]
    fn missing_authors_do_not_join_unrelated_records_or_absent_tracks() {
        let lib = library();
        lib.db.execute_batch("DELETE FROM track_composers; UPDATE tracks SET present = 0 WHERE id = 2;").unwrap();
        let (works, total) = lib.compositions("", "", 0).unwrap();
        assert_eq!(total, 2);
        assert!(works.iter().all(|work| work.tracks == 1 && work.composers.is_empty()));
        lib.db.execute("UPDATE sources SET enabled = 0", []).unwrap();
        assert_eq!(lib.compositions("", "", 0).unwrap(), (vec![], 0));
    }

    #[test]
    fn filters_apply_before_the_page_limit() {
        let lib = library();
        for n in 0..125 {
            lib.db.execute("INSERT INTO tracks(source_id,external_id,title,work,added_at) VALUES (1,?1,'Part',?2,1)",
                params![format!("/extra/{n}"), format!("Work {n:03}")]).unwrap();
        }
        let (first, total) = lib.compositions("", "Work", 0).unwrap();
        let (last, _) = lib.compositions("", "Work", 2).unwrap();
        assert_eq!((first.len(), total, last.len()), (60, 125, 5));
        assert_eq!(last[0].title, "Work 120");
    }

    #[test]
    fn a_part_title_with_a_composer_joins_its_work_and_a_song_does_not() {
        let lib = library();
        lib.db.execute_batch("INSERT INTO albums(id,title,album_artist) VALUES (3,'Cantatas','Choir'), (4,'Live','Band');
            INSERT INTO tracks(id,source_id,external_id,title,album_id,added_at) VALUES
            (10,1,'/10','Cantata, JLB 5: I. Chorus',3,1), (11,1,'/11','Cantata, JLB 5: II. Aria',3,1),
            (12,1,'/12','Song - Live',4,1), (13,1,'/13','Untitled: I. Intro',4,1);
            INSERT INTO composers(id,name) VALUES (3,'Ludwig Writer'), (4,'Band Member');
            INSERT INTO track_composers(track_id,composer_id) VALUES (10,3),(11,3),(12,4);").unwrap();
        let (works, _) = lib.compositions("", "Cantata", 0).unwrap();
        assert_eq!(works.len(), 1);
        assert_eq!((works[0].title.as_str(), works[0].tracks), ("Cantata, JLB 5", 2));
        assert_eq!(lib.compositions("", "Song", 0).unwrap().1, 0, "a song title names no work");
        assert_eq!(lib.compositions("", "Untitled", 0).unwrap().1, 0, "a part title without a composer stays a track");
    }

    #[test]
    fn a_classical_genre_marks_its_works_and_names_their_composers() {
        let lib = library();
        lib.db.execute_batch("INSERT INTO genres(id,name) VALUES (1,'Baroque');
            INSERT INTO track_genres(track_id,genre_id) VALUES (1,1);").unwrap();
        let mut seen = Vec::new();
        lib.visit_compositions_ordered("", "", false, &["baroque".into()], |work| seen.push((work.composers.clone(), work.classical_genre))).unwrap();
        assert!(seen.contains(&(vec!["First writer".into()], true)));
        assert!(seen.contains(&(vec!["Second writer".into()], false)));
        assert_eq!(lib.composers_in_genres(&["Baroque".into()]).unwrap(), ["First writer"]);
    }
}
