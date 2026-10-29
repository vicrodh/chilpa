//! Tracks without a record keep their own identity, year and artist credits.

use super::{FacetEntry, Filter, Library};
use rusqlite::params_from_iter;

/// Album membership cannot represent these credits. Expand only explicit or
/// matched credit parts, following the same rule as album_members.
pub(super) const ARTISTS: &str = "SELECT loose.id AS track_id, coalesce(parts.name, trim(loose.artist)) AS artist
    FROM tracks loose LEFT JOIN credit_parts parts ON parts.credit = trim(loose.artist)
    WHERE loose.album_id IS NULL AND trim(loose.artist) <> ''";

/// The suffix has placeholders twice: once for record credits, once for loose tracks.
pub(super) fn artist_condition(suffix: &str) -> String {
    format!("(a.id IN (SELECT album_id FROM album_artist_rollup WHERE artist {suffix})
        OR t.id IN (SELECT track_id FROM ({ARTISTS}) WHERE artist {suffix}))")
}

impl Library {
    /// Artist choices contributed by tracks without a record, under the same
    /// left-side conditions as record artists. No extra cache or stored grouping.
    pub fn facet_albumless_artists(&self, filter: &Filter) -> rusqlite::Result<Vec<FacetEntry>> {
        let mut left = filter.clone();
        left.choose(5, &[]);
        left.queries[5..].fill(String::new());
        left.albumless = true;
        let (clause, args) = self.browse_clause(&left);
        let sql = format!("SELECT min(credits.artist), count(DISTINCT t.id)
            FROM tracks t LEFT JOIN albums a ON a.id = t.album_id
            JOIN ({ARTISTS}) credits ON credits.track_id = t.id {clause}
            GROUP BY credits.artist COLLATE NOCASE ORDER BY 1 COLLATE NOCASE");
        let mut stmt = self.connection().prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(args.iter()), |row| {
            let name: String = row.get(0)?;
            Ok(FacetEntry { key: name.clone(), label: name, albums: row.get::<_, i64>(1)? as usize,
                year: None, icon: String::new() })
        })?;
        rows.collect()
    }
}
