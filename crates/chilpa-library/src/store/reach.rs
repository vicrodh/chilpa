//! Whether what is in the library answers right now (ADR-017).
//!
//! A folder that is not mounted and a server that does not answer leave
//! their records listed and unplayable. Nothing here deletes or hides: that
//! is `present`, and only a scan or a finished sync that did not find
//! something may touch it.

use super::Library;
use rusqlite::params;
use std::path::Path;

/// Whether a library folder can be read now.
///
/// A folder that exists and is empty while the library holds files from it
/// is a mount point with nothing mounted on it. Counting its files as
/// deleted emptied the library every time the NAS was away. A folder that
/// was really emptied on purpose is forgotten from Settings.
pub(crate) fn root_answers(lib: &Library, root_id: i64, root: &Path) -> rusqlite::Result<bool> {
    if !root.is_dir() {
        return Ok(false);
    }
    let empty = std::fs::read_dir(root).map(|mut entries| entries.next().is_none()).unwrap_or(true);
    if !empty {
        return Ok(true);
    }
    let known: i64 = lib.db.query_row("SELECT count(*) FROM scanned_files WHERE root_id = ?1", params![root_id], |r| r.get(0))?;
    Ok(known == 0)
}

impl Library {
    /// Looks at every library folder and marks it, and its tracks, as
    /// reachable or not. Returns whether anything changed.
    ///
    /// It reads the file system. A NAS that hangs can block a `stat` for
    /// minutes, so this runs on a thread of its own with its own connection,
    /// never on the one that draws the window.
    pub fn refresh_roots(&self) -> rusqlite::Result<bool> {
        let mut changed = false;
        for root in self.roots()? {
            let answers = root_answers(self, root.id, &root.path)?;
            changed |= self.set_root_reachable(root.id, answers)?;
        }
        Ok(changed)
    }

    /// Marks whether a source answers, and every track of it with it.
    /// Returns whether anything changed.
    ///
    /// For a server. The local source answers folder by folder, in
    /// `set_root_reachable`.
    pub fn set_source_reachable(&self, source_id: i64, reachable: bool) -> rusqlite::Result<bool> {
        let tx = self.own_transaction()?;
        // Hold the write lock before reading enabled so an in-flight look
        // cannot wake a server after the user paused it.
        let enabled: bool = tx.query_row("SELECT enabled FROM sources WHERE id = ?1", params![source_id], |r| r.get::<_, i64>(0)).map(|n| n != 0)?;
        let reachable = reachable && enabled;
        let changed = tx.execute(
            "UPDATE sources SET reachable = ?2 WHERE id = ?1 AND reachable <> ?2",
            params![source_id, i64::from(reachable)],
        )?;
        let moved = tx.execute(
            "UPDATE tracks SET reachable = ?2 WHERE source_id = ?1 AND reachable <> ?2",
            params![source_id, i64::from(reachable)],
        )?;
        tx.finish()?;
        Ok(changed + moved > 0)
    }

    /// Pauses or resumes a server. Its catalogue and credential are retained.
    /// Resuming leaves it unreachable until the next authenticated look.
    pub fn set_server_enabled(&self, source_id: i64, enabled: bool) -> rusqlite::Result<bool> {
        let tx = self.own_transaction()?;
        let changed = tx.execute(
            "UPDATE sources SET enabled = ?2, reachable = 0 WHERE id = ?1 AND kind = 'server' AND enabled <> ?2",
            params![source_id, i64::from(enabled)],
        )?;
        if changed > 0 {
            tx.execute("UPDATE tracks SET reachable = 0 WHERE source_id = ?1", params![source_id])?;
        }
        tx.finish()?;
        Ok(changed > 0)
    }

    /// What kind of source this is: `local`, `server` or `service`.
    pub fn source_kind(&self, source_id: i64) -> rusqlite::Result<Option<String>> {
        use rusqlite::OptionalExtension;
        self.db.query_row("SELECT kind FROM sources WHERE id = ?1", params![source_id], |r| r.get(0)).optional()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server_with_tracks(lib: &Library, n: usize) -> i64 {
        lib.db.execute("INSERT INTO sources (provider, instance, kind) VALUES ('example-server', 'one', 'server')", []).unwrap();
        let source = lib.db.last_insert_rowid();
        for i in 0..n {
            lib.db.execute(
                "INSERT INTO tracks (source_id, external_id, title, added_at) VALUES (?1, ?2, 'Song', 0)",
                params![source, format!("t{i}")],
            ).unwrap();
        }
        source
    }

    #[test]
    fn a_server_that_stops_answering_keeps_its_tracks_listed() {
        let lib = Library::in_memory().unwrap();
        let source = server_with_tracks(&lib, 3);

        assert!(lib.set_source_reachable(source, false).unwrap());
        assert!(!lib.set_source_reachable(source, false).unwrap(), "nothing changes the second time");
        assert_eq!(lib.track_count(false).unwrap(), 3, "listed");
        let down: i64 = lib.db.query_row("SELECT count(*) FROM tracks WHERE reachable = 0", [], |r| r.get(0)).unwrap();
        assert_eq!(down, 3, "unplayable");

        assert!(lib.set_source_reachable(source, true).unwrap());
        let down: i64 = lib.db.query_row("SELECT count(*) FROM tracks WHERE reachable = 0", [], |r| r.get(0)).unwrap();
        assert_eq!(down, 0);
    }

    #[test]
    fn a_paused_server_keeps_its_catalogue_and_cannot_be_woken_by_a_late_ping() {
        let lib = Library::in_memory().unwrap();
        let source = server_with_tracks(&lib, 3);
        assert!(lib.set_server_enabled(source, false).unwrap());
        assert_eq!(lib.track_count(false).unwrap(), 3);
        assert!(!lib.set_source_reachable(source, true).unwrap());
        let enabled: i64 = lib.db.query_row("SELECT enabled FROM sources WHERE id = ?1", [source], |r| r.get(0)).unwrap();
        assert_eq!(enabled, 0);
        let down: i64 = lib.db.query_row("SELECT count(*) FROM tracks WHERE source_id = ?1 AND reachable = 0", [source], |r| r.get(0)).unwrap();
        assert_eq!(down, 3);

        assert!(lib.set_server_enabled(source, true).unwrap());
        let reachable: i64 = lib.db.query_row("SELECT reachable FROM sources WHERE id = ?1", [source], |r| r.get(0)).unwrap();
        assert_eq!(reachable, 0, "resume waits for a ping");
        assert!(lib.set_source_reachable(source, true).unwrap());
        let reachable: i64 = lib.db.query_row("SELECT reachable FROM sources WHERE id = ?1", [source], |r| r.get(0)).unwrap();
        assert_eq!(reachable, 1);
    }

    #[test]
    fn a_folder_that_is_gone_or_an_empty_mount_point_does_not_answer() {
        let lib = Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        let dir = std::env::temp_dir().join(format!("chilpa-reach-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let root = lib.root_id(source, &dir).unwrap();

        assert!(root_answers(&lib, root, &dir).unwrap(), "an empty folder the library knows nothing of is fine");
        lib.db.execute(
            "INSERT INTO scanned_files (root_id, path, mtime, size, scanned_at) VALUES (?1, ?2, 0, 0, 0)",
            params![root, dir.join("a.flac").display().to_string()],
        ).unwrap();
        assert!(!root_answers(&lib, root, &dir).unwrap(), "empty while it should hold files: not mounted");
        std::fs::write(dir.join("a.flac"), b"x").unwrap();
        assert!(root_answers(&lib, root, &dir).unwrap());
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(!root_answers(&lib, root, &dir).unwrap(), "gone");
    }

    #[test]
    fn a_new_source_is_local_or_a_service_by_its_provider() {
        let lib = Library::in_memory().unwrap();
        let local = lib.source_id("local", "default").unwrap();
        let plugin = lib.source_id("example-source", "default").unwrap();
        assert_eq!(lib.source_kind(local).unwrap().as_deref(), Some("local"));
        assert_eq!(lib.source_kind(plugin).unwrap().as_deref(), Some("service"));
    }
}
