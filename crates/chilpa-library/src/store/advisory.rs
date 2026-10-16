//! Which tracks and records their files call explicit, for a search that
//! hides them.
use super::Library;
use std::collections::HashSet;

impl Library {
    /// Of these tracks, the ones whose file says explicit.
    pub fn explicit_tracks(&self, ids: &[i64]) -> rusqlite::Result<HashSet<i64>> {
        self.ids_where(ids, "SELECT id FROM tracks WHERE explicit = 1 AND id IN")
    }

    /// Of these records, the ones with an explicit track that is there.
    pub fn explicit_albums(&self, ids: &[i64]) -> rusqlite::Result<HashSet<i64>> {
        self.ids_where(ids, "SELECT DISTINCT album_id FROM tracks WHERE explicit = 1 AND present = 1 AND album_id IN")
    }

    fn ids_where(&self, ids: &[i64], query: &str) -> rusqlite::Result<HashSet<i64>> {
        if ids.is_empty() {
            return Ok(HashSet::new());
        }
        let marks = vec!["?"; ids.len()].join(",");
        self.db.prepare(&format!("{query} ({marks})"))?.query_map(rusqlite::params_from_iter(ids.iter()), |r| r.get(0))?.collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::store::fixtures;

    #[test]
    fn explicit_tracks_and_the_records_they_make_explicit() {
        let lib = fixtures::sample_library();
        lib.db.execute("UPDATE tracks SET explicit = 1 WHERE album_id = 2", []).unwrap();
        lib.db.execute("UPDATE tracks SET explicit = 0 WHERE album_id = 3", []).unwrap();
        let ids: Vec<i64> = lib.db.prepare("SELECT id FROM tracks ORDER BY album_id").unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
        let explicit = lib.explicit_tracks(&ids).unwrap();
        assert_eq!(explicit.len(), 1);
        assert_eq!(lib.explicit_albums(&[1, 2, 3]).unwrap(), [2].into_iter().collect());
        assert!(lib.explicit_albums(&[]).unwrap().is_empty());
    }
}
