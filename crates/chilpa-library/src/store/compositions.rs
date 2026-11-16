//! Read-only work navigation. A title is grouped only with the same stated
//! composers; an unattributed title stays within its record. No title parsing
//! or inferred composer turns a track into a composition.
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
    db.create_scalar_function("chilpa_work_key", 3, flags, |ctx| {
        let authors: String = ctx.get(1)?;
        let authors: Vec<String> = serde_json::from_str(&authors).map_err(|error| rusqlite::Error::UserFunctionError(Box::new(error)))?;
        Ok(work_key(&ctx.get::<String>(0)?, &authors, json!(ctx.get::<i64>(2)?)))
    })
}

// Display credits stay separate from the normalized comparison key. Unknown
// authors cannot make unrelated records share an identity.
const WORKS: &str = "WITH credited AS (
    SELECT t.id, t.album_id, trim(t.work) AS title,
        (SELECT json_group_array(name) FROM (SELECT c.name FROM composers c
            JOIN track_composers tc ON tc.composer_id = c.id
            WHERE tc.track_id = t.id ORDER BY c.name COLLATE NOCASE, c.id)) AS composers
    FROM tracks t JOIN sources s ON s.id = t.source_id
    WHERE t.present = 1 AND s.enabled = 1 AND trim(coalesce(t.work, '')) <> ''
), identified AS (
    SELECT *, chilpa_work_key(title, composers, coalesce(album_id, -id)) AS work_key
    FROM credited
) ";

const CONDITION: &str = " WHERE (?1 = '' OR EXISTS (SELECT 1 FROM json_each(composers) WHERE chilpa_work_sort(value) = chilpa_work_sort(?1)))
    AND (?2 = '' OR instr(chilpa_fold(title), chilpa_fold(?2)) > 0 OR EXISTS
        (SELECT 1 FROM json_each(composers) WHERE instr(chilpa_fold(value), chilpa_fold(?2)) > 0))";

fn composition(row: &rusqlite::Row<'_>) -> rusqlite::Result<Composition> {
    let composers: String = row.get(2)?;
    Ok(Composition { key: row.get(0)?, title: row.get(1)?, composers: serde_json::from_str(&composers).unwrap_or_default(),
        albums: row.get::<_, i64>(3)? as usize, tracks: row.get::<_, i64>(4)? as usize })
}

impl Library {
    /// A header count does not materialize or sort a page of work cards.
    pub fn composition_count(&self, composer: &str, text: &str) -> rusqlite::Result<usize> {
        self.db.query_row(&format!("{WORKS} SELECT count(DISTINCT work_key) FROM identified{CONDITION}"),
            params![composer, text.trim()], |row| row.get::<_, i64>(0).map(|total| total as usize))
    }

    /// Sixty entries per page, plus a total, with filtering before pagination.
    /// SQLite owns grouping temporaries; only the requested page crosses here.
    pub fn compositions(&self, composer: &str, text: &str, page: usize) -> rusqlite::Result<(Vec<Composition>, usize)> {
        let condition = CONDITION;
        let total = self.composition_count(composer, text)?;
        let sql = format!("{WORKS} SELECT work_key, min(title), composers, count(DISTINCT coalesce(album_id, -id)), count(*)
            FROM identified{condition} GROUP BY work_key ORDER BY chilpa_work_sort(min(title)) COLLATE BINARY, work_key LIMIT ?3 OFFSET ?4");
        let mut statement = self.db.prepare(&sql)?;
        let rows = statement.query_map(params![composer, text.trim(), PAGE_SIZE as i64, page.saturating_mul(PAGE_SIZE).min(i64::MAX as usize) as i64], composition)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok((rows, total))
    }

    /// Stream ordered works so a merged source page retains only 60 entries.
    /// This does not materialize the library's complete work inventory in Rust.
    pub fn visit_compositions(&self, composer: &str, text: &str, mut visit: impl FnMut(Composition)) -> rusqlite::Result<()> {
        self.visit_compositions_ordered(composer, text, false, &mut visit)
    }

    pub fn visit_compositions_ordered(&self, composer: &str, text: &str, descending: bool, mut visit: impl FnMut(Composition)) -> rusqlite::Result<()> {
        let direction = if descending { "DESC" } else { "ASC" };
        let sql = format!("{WORKS} SELECT work_key, min(title), composers, count(DISTINCT coalesce(album_id, -id)), count(*)
            FROM identified{CONDITION} GROUP BY work_key ORDER BY chilpa_work_sort(min(title)) COLLATE BINARY {direction}, work_key {direction}");
        let mut statement = self.db.prepare(&sql)?;
        for row in statement.query_map(params![composer, text.trim()], composition)? { visit(row?); }
        Ok(())
    }

    /// Every part of the selected work, in recording/disc/track order.
    pub fn composition_tracks(&self, key: &str) -> rusqlite::Result<Vec<i64>> {
        let sql = format!("{WORKS} SELECT t.id FROM identified w JOIN tracks t ON t.id = w.id
            LEFT JOIN albums a ON a.id = t.album_id WHERE w.work_key = ?1
            ORDER BY a.year, a.title COLLATE NOCASE, coalesce(t.album_id, -t.id),
                coalesce(t.release_disc, t.disc_number), t.track_number, t.id");
        let mut statement = self.db.prepare(&sql)?;
        let rows = statement.query_map([key], |row| row.get(0))?.collect();
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
        assert_eq!(lib.composition_tracks(&works[0].key).unwrap(), [1, 2]);
        assert_eq!(lib.db.total_changes(), changes);
    }

    #[test]
    fn descending_stream_reverses_title_ties_without_merging_authors() {
        let lib = library();
        let mut ascending = Vec::new();
        let mut descending = Vec::new();
        lib.visit_compositions_ordered("", "", false, |work| ascending.push(work)).unwrap();
        lib.visit_compositions_ordered("", "", true, |work| descending.push(work)).unwrap();
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
}
