//! Streaming, read-only associations for derived genre navigation.
use super::Library;

impl Library {
    /// Visit distinct original tag/release pairs in release order. Albumless
    /// tracks keep their own identity; no full track inventory is materialized.
    pub fn visit_genre_releases(&self, mut visit: impl FnMut(bool, i64, &str, &str, Option<&str>)) -> rusqlite::Result<()> {
        let mut statement = self.db.prepare(concat!(
            "SELECT DISTINCT t.album_id IS NULL, coalesce(t.album_id,t.id), g.name,
             coalesce(a.album_artist,t.artist), coalesce(", album_art!("a"), ",t.release_artwork)
             FROM tracks t JOIN sources s ON s.id=t.source_id AND s.enabled=1
             LEFT JOIN albums a ON a.id=t.album_id
             JOIN track_genres tg ON tg.track_id=t.id JOIN genres g ON g.id=tg.genre_id
             WHERE t.present=1 ORDER BY 1,2,3"
        ))?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let tag: String = row.get(2)?;
            let artist: String = row.get(3)?;
            let artwork: Option<String> = row.get(4)?;
            visit(row.get(0)?, row.get(1)?, &tag, &artist, artwork.as_deref());
        }
        Ok(())
    }
}
