//! Library visibility for collections explicitly imported from a source.
use super::Library;
use rusqlite::params;

impl Library {
    pub fn imported_source_state(&self, source: i64) -> rusqlite::Result<(usize, bool)> {
        self.db.query_row("SELECT count(*), coalesce(max(present = 1), 0) FROM tracks WHERE source_id = ?1 AND sync_generation IS NOT NULL AND present IN (1, 2)", [source], |row| Ok((row.get::<_, i64>(0)? as usize, row.get(1)?)))
    }

    /// Hidden imports retain their stable IDs, ratings and playlist membership.
    pub fn show_imported_source(&self, source: i64, show: bool) -> rusqlite::Result<()> {
        self.db.execute("UPDATE tracks SET present = ?2 WHERE source_id = ?1 AND sync_generation IS NOT NULL AND present IN (1, 2)", params![source, if show {1} else {2}])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hiding_imports_preserves_identity_and_does_not_promote_incidental_tracks() {
        let lib = Library::in_memory().unwrap();
        let source = lib.source_id("guest", "account").unwrap();
        for (id, generation) in [("imported", Some(1)), ("incidental", None)] {
            lib.db.execute("INSERT INTO tracks(source_id, external_id, title, added_at, present, sync_generation) VALUES (?1, ?2, ?2, 0, 2, ?3)", params![source, id, generation]).unwrap();
        }
        lib.show_imported_source(source, true).unwrap();
        assert_eq!(lib.imported_source_state(source).unwrap(), (1, true));
        let incidental: i64 = lib.db.query_row("SELECT present FROM tracks WHERE external_id = 'incidental'", [], |r| r.get(0)).unwrap();
        assert_eq!(incidental, 2);
        lib.show_imported_source(source, false).unwrap();
        assert_eq!(lib.imported_source_state(source).unwrap(), (1, false));
        let count: i64 = lib.db.query_row("SELECT count(*) FROM tracks", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 2);
    }
}
