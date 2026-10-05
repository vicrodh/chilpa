//! Metadata-only candidates for playlist conversion. I never resolve or decode audio here.
use super::Library;
use rusqlite::params;

#[derive(Clone, Debug)]
pub struct Candidate {
    pub id: i64,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: u64,
    pub isrc: Option<String>,
    pub codec: Option<String>,
    pub bits: Option<u16>,
    pub rate: Option<u32>,
}
impl Library {
    pub fn import_candidates(
        &self,
        text: &str,
        isrc: &str,
        limit: usize,
    ) -> rusqlite::Result<Vec<Candidate>> {
        let query = super::fts_query(text).unwrap_or_default();
        let mut statement = self.db.prepare("SELECT t.id,t.title,t.artist,COALESCE(a.title,''),t.duration_ms,t.isrc,t.codec,t.bit_depth,t.sample_rate_hz
            FROM tracks t JOIN sources s ON s.id=t.source_id LEFT JOIN albums a ON a.id=t.album_id
            WHERE s.provider='local' AND t.present=1 AND t.reachable=1
              AND ((?1<>'' AND t.isrc=?1 COLLATE NOCASE) OR t.id IN (SELECT rowid FROM track_search WHERE track_search MATCH ?2))
            ORDER BY CASE WHEN ?1<>'' AND t.isrc=?1 COLLATE NOCASE THEN 0 ELSE 1 END,t.id LIMIT ?3")?;
        // FTS does not accept an empty expression; an impossible word keeps ISRC-only lookup valid.
        let rows = statement.query_map(
            params![
                isrc,
                if query.is_empty() {
                    "zznomatchzz"
                } else {
                    &query
                },
                limit.min(100) as i64
            ],
            |r| {
                Ok(Candidate {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    artist: r.get(2)?,
                    album: r.get(3)?,
                    duration_ms: r.get::<_, i64>(4)?.max(0) as u64,
                    isrc: r.get(5)?,
                    codec: r.get(6)?,
                    bits: r.get(7)?,
                    rate: r.get(8)?,
                })
            },
        )?;
        rows.collect()
    }
    pub fn is_available_local_track(&self, id: i64) -> rusqlite::Result<bool> {
        self.db.query_row("SELECT EXISTS(SELECT 1 FROM tracks t JOIN sources s ON s.id=t.source_id WHERE t.id=?1 AND s.provider='local' AND t.present=1 AND t.reachable=1)", [id], |r| r.get(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn candidates_include_local_quality_and_isrc_but_exclude_unavailable_and_remote_tracks() {
        let library = Library::in_memory().unwrap();
        let local = library.source_id("local", "default").unwrap();
        let remote = library.source_id("plugin", "source-a").unwrap();
        for (id, source, present, reachable) in [
            (1, local, 1, 1),
            (2, local, 0, 1),
            (3, local, 1, 0),
            (4, remote, 1, 1),
        ] {
            library.connection().execute("INSERT INTO tracks(id,source_id,external_id,title,artist,duration_ms,added_at,isrc,codec,bit_depth,sample_rate_hz,present,reachable) VALUES(?1,?2,?3,'First','Band',123000,1,'ABC','flac',24,96000,?4,?5)",params![id,source,format!("/{id}.flac"),present,reachable]).unwrap();
        }
        let found = library.import_candidates("unrelated", "abc", 20).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, 1);
        assert_eq!(found[0].bits, Some(24));
        assert_eq!(found[0].rate, Some(96000));
        assert_eq!(
            library
                .import_candidates("Band First", "", 20)
                .unwrap()
                .len(),
            1
        );
        assert!(!library.is_available_local_track(4).unwrap());
    }
}
