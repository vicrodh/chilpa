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

    /// I count all alternatives together, including OR branches independent of the field.
    pub fn query_facet_counts(&self, high: &Filter, low: &Filter, field: Field, typed: &str) -> rusqlite::Result<(usize,Vec<(String,usize)>)> {
        let (name,join) = match field {
            Field::Artist => ("r.artist","JOIN album_artist_rollup r ON r.album_id=a.id"),
            Field::Composer => ("c.name","JOIN track_composers tc ON tc.track_id=t.id JOIN composers c ON c.id=tc.composer_id"),
            Field::Genre => ("g.name","JOIN track_genres tg ON tg.track_id=t.id JOIN genres g ON g.id=tg.genre_id"),
            Field::Label => ("t.label",""), _ => return Ok((0,Vec::new())),
        };
        let (low_clause,mut args) = browse::clause(low);
        let (high_clause,high_args) = browse::clause(high); args.extend(high_args);
        args.push(Box::new(super::query::fold(typed)));
        let eligible="t.present=1 AND t.source_id IN (SELECT id FROM sources WHERE enabled=1) AND t.album_id IN (SELECT id FROM albums)";
        let names=match field {
            Field::Artist => format!("SELECT min(artist) AS name FROM album_artist_rollup WHERE album_id IN (SELECT t.album_id FROM tracks t WHERE {eligible}) GROUP BY artist COLLATE NOCASE"),
            Field::Composer => format!("SELECT name FROM composers WHERE id IN (SELECT tc.composer_id FROM track_composers tc JOIN tracks t ON t.id=tc.track_id WHERE {eligible})"),
            Field::Genre => format!("SELECT name FROM genres WHERE id IN (SELECT tg.genre_id FROM track_genres tg JOIN tracks t ON t.id=tg.track_id WHERE {eligible})"),
            _ => format!("SELECT min(t.label) AS name FROM tracks t WHERE {eligible} AND coalesce(t.label,'')<>'' GROUP BY t.label COLLATE NOCASE"),
        };
        let sql = format!("WITH low AS MATERIALIZED (SELECT t.id FROM tracks t JOIN albums a ON a.id=t.album_id {low_clause}),
            high AS MATERIALIZED (SELECT t.id FROM tracks t JOIN albums a ON a.id=t.album_id {high_clause}),
            names AS MATERIALIZED ({names}),
            choices AS MATERIALIZED (SELECT name FROM names WHERE instr(chilpa_fold(name),?)>0 ORDER BY chilpa_fold(name),name LIMIT 3),
            counts AS (SELECT choices.name, count(DISTINCT t.id) AS n FROM choices
                JOIN tracks t JOIN albums a ON a.id=t.album_id {join}
                WHERE {name}=choices.name COLLATE NOCASE AND t.id IN high AND t.id NOT IN low
                GROUP BY choices.name COLLATE NOCASE)
            SELECT NULL, count(*) FROM low UNION ALL
            SELECT choices.name,coalesce(counts.n,0) FROM choices LEFT JOIN counts ON counts.name=choices.name");
        let mut stmt = self.connection().prepare(&sql)?;
        let mut baseline = 0; let mut values = Vec::new();
        for row in stmt.query_map(params_from_iter(args.iter()),|r| Ok((r.get::<_,Option<String>>(0)?,r.get::<_,i64>(1)? as usize)))? {
            match row? { (None,n) => baseline=n, (Some(name),n) => values.push((name,n)) }
        }
        Ok((baseline,values))
    }
}
