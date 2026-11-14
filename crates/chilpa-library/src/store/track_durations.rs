//! Small duration lookups for metadata matching, without loading file paths.
use super::Library;
use rusqlite::params_from_iter;

impl Library {
    pub fn track_durations(&self, ids: &[i64]) -> rusqlite::Result<Vec<(i64,u64)>> {
        let mut durations = Vec::with_capacity(ids.len());
        for chunk in ids.chunks(400) {
            let marks = vec!["?";chunk.len()].join(",");
            let mut query = self.db.prepare(&format!("SELECT id,duration_ms FROM tracks WHERE id IN ({marks})"))?;
            durations.extend(query.query_map(params_from_iter(chunk.iter()), |row| Ok((row.get(0)?,row.get::<_,i64>(1)?.max(0) as u64)))?.collect::<rusqlite::Result<Vec<_>>>()?);
        }
        Ok(durations)
    }
}
