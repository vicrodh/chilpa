//! Streaming, read-only associations for derived genre navigation.
use super::Library;

// Stale statistics must not turn an unassigned genre into a full track scan.
const GENRE_NAMES: &str = "SELECT g.name FROM genres g WHERE EXISTS (
    SELECT 1 FROM track_genres tg INDEXED BY track_genres_genre JOIN tracks t ON t.id=tg.track_id
    JOIN sources s ON s.id=t.source_id
    WHERE tg.genre_id=g.id AND t.present=1 AND s.enabled=1 /* scope */)
    ORDER BY g.name COLLATE NOCASE";

const GENRE_RELEASES: &str = "SELECT DISTINCT t.album_id IS NULL, coalesce(t.album_id,t.id),
    g.name, coalesce(a.album_artist,t.artist)
    FROM tracks t JOIN sources s ON s.id=t.source_id AND s.enabled=1
    LEFT JOIN albums a ON a.id=t.album_id
    JOIN track_genres tg ON tg.track_id=t.id JOIN genres g ON g.id=tg.genre_id
    WHERE t.present=1 /* scope */ ORDER BY 1,2,3";

fn scoped(sql: &str, excluded: &[String]) -> String {
    let clause = if excluded.is_empty() { String::new() } else {
        format!(" AND (s.provider || ':' || s.instance) NOT IN ({})", vec!["?"; excluded.len()].join(","))
    };
    sql.replace("/* scope */", &clause)
}

impl Library {
    /// Stream exact-album associations into a bounded query-owned index.
    /// Duplicates are handled by the consumer without a library-wide sort.
    pub fn visit_album_genres(&self, album: i64, mut visit: impl FnMut(&str, &str) -> rusqlite::Result<()>) -> rusqlite::Result<()> {
        let mut statement = self.db.prepare("SELECT g.name,s.provider || ':' || s.instance FROM tracks t
            JOIN sources s ON s.id=t.source_id LEFT JOIN track_genres tg ON tg.track_id=t.id
            LEFT JOIN genres g ON g.id=tg.genre_id
            WHERE t.album_id=?1 AND t.present=1 AND s.enabled=1")?;
        let mut rows = statement.query([album])?;
        while let Some(row) = rows.next()? {
            let tag = row.get_ref(0)?;
            let name = match tag {
                rusqlite::types::ValueRef::Null => "",
                rusqlite::types::ValueRef::Text(bytes) if bytes.len() <= crate::genre_index::MAX_TAG_BYTES => std::str::from_utf8(bytes)
                    .map_err(|error|rusqlite::Error::FromSqlConversionFailure(0,rusqlite::types::Type::Text,Box::new(error)))?,
                _ => return Err(rusqlite::Error::InvalidParameterName("Genre text exceeds the 1 MiB query limit.".into())),
            };
            visit(name,&row.get::<_,String>(1)?)?;
        }
        Ok(())
    }

    /// Original spellings attached to present tracks in enabled sources.
    /// Discovery needs names, not the per-release counts of the facet query.
    /// Each genre's indexed existence lookup stops at its first eligible track.
    pub fn genre_names(&self) -> rusqlite::Result<Vec<String>> {
        self.genre_names_excluding(&[])
    }

    /// Apply the same source keys as browsing, before recognizing any tag.
    pub fn genre_names_excluding(&self, excluded: &[String]) -> rusqlite::Result<Vec<String>> {
        self.db.prepare(&scoped(GENRE_NAMES, excluded))?.query_map(rusqlite::params_from_iter(excluded), |row| row.get(0))?.collect()
    }

    /// Original tags and contributing sources of this exact visible edition.
    /// Other editions and disabled or absent tracks do not contribute links.
    pub fn album_genre_sources(&self, album: i64) -> rusqlite::Result<(Vec<String>, Vec<String>)> {
        let sources = self.db.prepare("SELECT DISTINCT s.provider || ':' || s.instance FROM tracks t
            JOIN sources s ON s.id=t.source_id WHERE t.album_id=?1 AND t.present=1 AND s.enabled=1 ORDER BY 1")?
            .query_map([album], |row| row.get(0))?.collect::<rusqlite::Result<Vec<String>>>()?;
        let tags = self.db.prepare("SELECT DISTINCT g.name FROM tracks t JOIN sources s ON s.id=t.source_id
            JOIN track_genres tg ON tg.track_id=t.id JOIN genres g ON g.id=tg.genre_id
            WHERE t.album_id=?1 AND t.present=1 AND s.enabled=1 ORDER BY g.name COLLATE NOCASE, g.name")?
            .query_map([album], |row| row.get(0))?.collect::<rusqlite::Result<Vec<String>>>()?;
        Ok((tags, sources))
    }

    /// Visit distinct original tag/release pairs in release order. Albumless
    /// tracks keep their own identity; no full track inventory is materialized.
    /// Only the credited artist crosses this boundary, never album artwork.
    pub fn visit_genre_releases(&self, visit: impl FnMut(bool, i64, &str, &str)) -> rusqlite::Result<()> {
        self.visit_genre_releases_excluding(&[], visit)
    }

    /// Restrict tracks before release deduplication, including shared albums.
    pub fn visit_genre_releases_excluding(&self, excluded: &[String], mut visit: impl FnMut(bool, i64, &str, &str)) -> rusqlite::Result<()> {
        let mut statement = self.db.prepare(&scoped(GENRE_RELEASES, excluded))?;
        let mut rows = statement.query(rusqlite::params_from_iter(excluded))?;
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
