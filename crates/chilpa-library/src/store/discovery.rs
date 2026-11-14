//! Streaming, read-only associations for derived genre navigation.
use super::Library;

// Stale statistics must not turn an unassigned genre into a full track scan.
const GENRE_NAMES: &str = "SELECT g.name FROM genres g WHERE EXISTS (
    SELECT 1 FROM track_genres tg INDEXED BY track_genres_genre JOIN tracks t ON t.id=tg.track_id
    JOIN sources s ON s.id=t.source_id
    WHERE tg.genre_id=g.id AND t.present=1 AND s.enabled=1)
    ORDER BY g.name COLLATE NOCASE";

const GENRE_RELEASES: &str = "SELECT DISTINCT t.album_id IS NULL, coalesce(t.album_id,t.id),
    g.name, coalesce(a.album_artist,t.artist)
    FROM tracks t JOIN sources s ON s.id=t.source_id AND s.enabled=1
    LEFT JOIN albums a ON a.id=t.album_id
    JOIN track_genres tg ON tg.track_id=t.id JOIN genres g ON g.id=tg.genre_id
    WHERE t.present=1 ORDER BY 1,2,3";

impl Library {
    /// Original spellings attached to present tracks in enabled sources.
    /// Discovery needs names, not the per-release counts of the facet query.
    /// Each genre's indexed existence lookup stops at its first eligible track.
    pub fn genre_names(&self) -> rusqlite::Result<Vec<String>> {
        self.db.prepare(GENRE_NAMES)?.query_map([], |row| row.get(0))?.collect()
    }

    /// Visit distinct original tag/release pairs in release order. Albumless
    /// tracks keep their own identity; no full track inventory is materialized.
    /// Only the credited artist crosses this boundary, never album artwork.
    pub fn visit_genre_releases(&self, mut visit: impl FnMut(bool, i64, &str, &str)) -> rusqlite::Result<()> {
        let mut statement = self.db.prepare(GENRE_RELEASES)?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let tag: String = row.get(2)?;
            let artist: String = row.get(3)?;
            visit(row.get(0)?, row.get(1)?, &tag, &artist);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
