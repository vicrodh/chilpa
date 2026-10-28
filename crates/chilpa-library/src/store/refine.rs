//! I count Refine alternatives in one grouped query per field.
use super::{browse, query::Field, Filter, Library};
use rusqlite::params_from_iter;

impl Library {
    /// Counts tracks, using the same eligibility clause as the Library.
    pub fn query_count(&self, filter: &Filter) -> rusqlite::Result<usize> {
        let (clause,args) = browse::clause(filter);
        self.connection().query_row(&format!("SELECT count(*) FROM tracks t JOIN albums a ON a.id=t.album_id {clause}"),
            params_from_iter(args.iter()), |r| r.get::<_,i64>(0).map(|n| n as usize))
    }

    /// Names available to automatic resolution, without counting each facet.
    pub fn query_vocabulary(&self) -> rusqlite::Result<Vec<(Field,Vec<String>)>> {
        let fields = [Field::Artist,Field::Composer,Field::Genre,Field::Label];
        let mut out: Vec<_> = fields.into_iter().map(|f| (f,Vec::new())).collect();
        let eligible = "t.present=1 AND t.source_id IN (SELECT id FROM sources WHERE enabled=1) AND t.album_id IN (SELECT id FROM albums)";
        let sql = format!("SELECT 0, min(r.artist) FROM album_artist_rollup r WHERE EXISTS
            (SELECT 1 FROM tracks t WHERE t.album_id=r.album_id AND {eligible}) GROUP BY r.artist COLLATE NOCASE
            UNION SELECT 1, c.name FROM composers c JOIN track_composers tc ON tc.composer_id=c.id
            JOIN tracks t ON t.id=tc.track_id WHERE {eligible}
            UNION SELECT 2, g.name FROM genres g JOIN track_genres tg ON tg.genre_id=g.id
            JOIN tracks t ON t.id=tg.track_id WHERE {eligible}
            UNION SELECT 3, min(t.label) FROM tracks t WHERE {eligible} AND coalesce(t.label,'')<>'' GROUP BY t.label COLLATE NOCASE");
        let mut stmt = self.connection().prepare(&sql)?;
        for row in stmt.query_map([],|r| Ok((r.get::<_,i64>(0)? as usize,r.get::<_,String>(1)?)))? {
            let (field,name) = row?; out[field].1.push(name);
        }
        for (_,names) in &mut out { names.sort(); names.dedup(); }
        Ok(out)
    }

    /// I count the tracks each filter leaves, in one pass over the library
    /// instead of one per filter.
    pub fn query_counts(&self, filters: &[&Filter]) -> rusqlite::Result<Vec<usize>> {
        if filters.is_empty() { return Ok(Vec::new()); }
        // Every clause opens with the same eligibility test: it is asked once,
        // not once per filter.
        const BASE: &str = "t.present = 1 AND t.source_id IN (SELECT id FROM sources WHERE enabled = 1)";
        let mut columns = Vec::new(); let mut args = Vec::new();
        for filter in filters {
            let (clause,own) = browse::clause(filter);
            let rest = condition(&clause).strip_prefix(BASE).map(|rest| rest.trim_start_matches(" AND ").trim()).unwrap_or(condition(&clause));
            columns.push(format!("count(*) FILTER (WHERE {})", if rest.is_empty() { "1" } else { rest }));
            args.extend(own);
        }
        self.connection().query_row(&format!("SELECT {} FROM tracks t JOIN albums a ON a.id=t.album_id WHERE {BASE}", columns.join(", ")),
            params_from_iter(args.iter()), |r| (0..filters.len()).map(|i| r.get::<_,i64>(i).map(|n| n as usize)).collect())
    }

    /// I count all alternatives together, including OR branches independent of the field.
    /// `low` is the filter that already leaves a value in, or none when no
    /// track does. Only the names that match what was typed are looked at,
    /// and only the first three of them are counted; the labels are read
    /// off the tracks a listener can reach, once, and nothing else walks
    /// the tracks per field.
    pub fn query_facet_counts(&self, high: &Filter, low: Option<&Filter>, field: Field, typed: &str) -> rusqlite::Result<(usize,Vec<(String,usize)>)> {
        let (name,join) = match field {
            Field::Artist => ("r.artist","JOIN album_artist_rollup r ON r.album_id=a.id"),
            Field::Composer => ("c.name","JOIN track_composers tc ON tc.track_id=t.id JOIN composers c ON c.id=tc.composer_id"),
            Field::Genre => ("g.name","JOIN track_genres tg ON tg.track_id=t.id JOIN genres g ON g.id=tg.genre_id"),
            Field::Label => ("t.label",""), _ => return Ok((0,Vec::new())),
        };
        let eligible="t.present=1 AND t.source_id IN (SELECT id FROM sources WHERE enabled=1) AND t.album_id IN (SELECT id FROM albums)";
        // The names that match what was typed come first and in order. An
        // artist or a label is spelled several ways, and the one shown is the
        // least among the rows a listener can reach, as `query_vocabulary`
        // spells it: never one only an absent track or a disabled source
        // wrote. A composer or a genre is one row per spelling, so only
        // whether a track still holds it is asked, down the list until three do.
        let (names,holds)=match field {
            Field::Artist => (format!("SELECT min(artist) AS name FROM album_artist_rollup WHERE instr(chilpa_fold(artist),?)>0 AND album_id IN (SELECT t.album_id FROM tracks t WHERE {eligible}) GROUP BY artist COLLATE NOCASE"), "1".to_string()),
            Field::Composer => ("SELECT name FROM composers WHERE instr(chilpa_fold(name),?)>0".to_string(),
                format!("EXISTS (SELECT 1 FROM track_composers tc JOIN tracks t ON t.id=tc.track_id WHERE tc.composer_id=(SELECT id FROM composers WHERE name=cand.name) AND {eligible})")),
            Field::Genre => ("SELECT name FROM genres WHERE instr(chilpa_fold(name),?)>0".to_string(),
                format!("EXISTS (SELECT 1 FROM track_genres tg JOIN tracks t ON t.id=tg.track_id WHERE tg.genre_id=(SELECT id FROM genres WHERE name=cand.name) AND {eligible})")),
            _ => (format!("SELECT min(t.label) AS name FROM tracks t WHERE {eligible} AND coalesce(t.label,'')<>'' AND instr(chilpa_fold(t.label),?)>0 GROUP BY t.label COLLATE NOCASE"), "1".to_string()),
        };
        let mut args: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(super::query::fold(typed))];
        let (high_clause,high_args) = browse::clause(high); args.extend(high_args);
        // A value already left in by the filter is counted under `low`; none
        // is, when nothing the filter says leaves it in.
        let (low_not,low_total) = match low {
            Some(low) => {
                let (clause,low_args) = browse::clause(low); args.extend(low_args);
                (format!("AND NOT ({})",condition(&clause)),format!("SELECT count(*) FROM tracks t JOIN albums a ON a.id=t.album_id {clause}"))
            }
            None => (String::new(),"SELECT 0".to_string()),
        };
        // The same filter is in the statement twice, under the counts and as the baseline.
        if let Some(low) = low { args.extend(browse::clause(low).1); }
        let sql = format!("WITH cand AS MATERIALIZED (SELECT name FROM ({names}) ORDER BY chilpa_fold(name),name),
            choices AS MATERIALIZED (SELECT name FROM cand WHERE {holds} LIMIT 3),
            counts AS (SELECT choices.name, count(DISTINCT t.id) AS n FROM choices
                JOIN tracks t JOIN albums a ON a.id=t.album_id {join}
                WHERE {name}=choices.name COLLATE NOCASE AND ({high_cond}) {low_not}
                GROUP BY choices.name COLLATE NOCASE)
            SELECT NULL, ({low_total}) UNION ALL
            SELECT choices.name,coalesce(counts.n,0) FROM choices LEFT JOIN counts ON counts.name=choices.name",
            high_cond=condition(&high_clause));
        let mut stmt = self.connection().prepare(&sql)?;
        let mut baseline = 0; let mut values = Vec::new();
        for row in stmt.query_map(params_from_iter(args.iter()),|r| Ok((r.get::<_,Option<String>>(0)?,r.get::<_,i64>(1)? as usize)))? {
            match row? { (None,n) => baseline=n, (Some(name),n) => values.push((name,n)) }
        }
        Ok((baseline,values))
    }
}

/// A clause's conditions without its `WHERE`, to embed in another statement.
fn condition(clause: &str) -> &str { clause.trim_start().strip_prefix("WHERE").unwrap_or(clause).trim() }

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    /// Four present spellings of one label and an absent track's fifth; one
    /// artist spelled one way on a record that can be reached and another
    /// way on one that cannot.
    fn library() -> Library {
        let lib = Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        let db = lib.connection();
        db.execute("INSERT INTO albums(id, title, album_artist) VALUES (1, 'Here', 'Opeth'), (2, 'Gone', 'OPETH')", []).unwrap();
        db.execute("INSERT INTO album_artist_rollup(album_id, artist, tracks) VALUES (1, 'Opeth', 4), (2, 'OPETH', 1)", []).unwrap();
        for (id, album, label, present) in [(1, 1, "cafe", 1), (2, 1, "Café", 1), (3, 1, "Càfe", 1), (4, 1, "Cáfe", 1), (5, 2, "CAFE", 0)] {
            db.execute("INSERT INTO tracks(id, source_id, external_id, title, album_id, label, present, added_at) VALUES (?1, ?2, ?3, 'Song', ?4, ?5, ?6, 1)",
                params![id, source, format!("/{id}.flac"), album, label, present]).unwrap();
        }
        lib
    }

    fn names(lib: &Library, field: Field, typed: &str) -> Vec<String> {
        lib.query_facet_counts(&Filter::default(), None, field, typed).unwrap().1.into_iter().map(|(name, _)| name).collect()
    }

    #[test]
    fn a_label_is_spelled_as_a_track_that_can_be_reached_spells_it() {
        let lib = library();
        assert_eq!(names(&lib, Field::Label, "cafe"), ["Café", "Càfe", "Cáfe"], "the absent track's CAFE neither names its group nor takes a place");
        lib.connection().execute("UPDATE tracks SET present = 1 WHERE id = 5", []).unwrap();
        assert_eq!(names(&lib, Field::Label, "cafe"), ["CAFE", "Café", "Càfe"], "reachable, it does both");
    }

    #[test]
    fn an_artist_is_spelled_as_a_record_that_can_be_reached_spells_it() {
        let lib = library();
        assert_eq!(names(&lib, Field::Artist, "opeth"), ["Opeth"], "the only record that can be reached spells it so");
        lib.connection().execute("UPDATE tracks SET present = 1 WHERE id = 5", []).unwrap();
        // The rollup's artist collates NOCASE, so `min` holds either spelling
        // once both records can be reached; it is one of theirs either way.
        let shown = names(&lib, Field::Artist, "opeth");
        assert_eq!(shown.len(), 1);
        assert!(["Opeth", "OPETH"].contains(&shown[0].as_str()), "{shown:?}");
    }
}
