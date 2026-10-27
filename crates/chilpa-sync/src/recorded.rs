//! I read the last published facts without creating, migrating or leasing a journal.
use crate::{
    invalid, state::Managed, AssetKind, Cancellation, RelativePath, Result, State, MAX_ASSETS,
    MAX_METADATA_BYTES,
};
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use std::path::Path;

#[derive(Clone, Debug)]
pub struct RecordedFile {
    pub path: RelativePath,
    pub kind: AssetKind,
    pub size: u64,
    pub title: String,
    pub artist: String,
    pub album: String,
}

impl State {
    /// I return a consistent read-only snapshot, including only durably published files.
    pub fn recorded(
        path: &Path,
        target_id: &str,
        cancel: &Cancellation,
    ) -> Result<Vec<RecordedFile>> {
        cancel.check()?;
        let mut db = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        let tx = db.transaction()?;
        let version: u32 = tx.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version != 1 {
            return Err(invalid("unsupported sync journal version"));
        }
        let target: Option<String> = tx
            .query_row("SELECT id FROM sync_target", [], |r| r.get(0))
            .optional()?;
        if target.as_deref() != Some(target_id) {
            return Err(crate::conflict("this history belongs to another storage"));
        }
        let mut statement = tx.prepare("SELECT path,record FROM sync_files ORDER BY path")?;
        let mut rows = statement.query([])?;
        let mut files = Vec::new();
        let mut bytes = 0usize;
        while let Some(row) = rows.next()? {
            cancel.check()?;
            let path: String = row.get(0)?;
            let record: String = row.get(1)?;
            bytes = bytes
                .saturating_add(path.len())
                .saturating_add(record.len());
            if bytes > MAX_METADATA_BYTES || files.len() >= MAX_ASSETS {
                return Err(invalid("sync history exceeds its metadata budget"));
            }
            let managed: Managed =
                serde_json::from_str(&record).map_err(|_| invalid("damaged sync file record"))?;
            files.push(RecordedFile {
                path: RelativePath::new(path)?,
                kind: managed.kind,
                size: managed.digest()?.size,
                title: managed.title,
                artist: managed.artist,
                album: managed.album,
            });
        }
        Ok(files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{state::Operation, Fingerprint, Target};

    #[test]
    fn history_reads_published_metadata_without_a_writer_lease_or_mutation() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("sync.db");
        let mut state = State::open(&path).unwrap();
        state
            .bind(&Target {
                version: 1,
                id: "a".repeat(32),
                name: "Card".into(),
            })
            .unwrap();
        state
            .published(&Operation {
                path: RelativePath::new("Music/a.flac").unwrap(),
                previous: None,
                source: None,
                desired: Some(Managed {
                    source_stamp: None,
                    fingerprint: Some(Fingerprint {
                        size: 42,
                        sha256: "b".repeat(64),
                    }),
                    kind: AssetKind::Media,
                    key: "private stable key".into(),
                    title: "Song".into(),
                    artist: "Artist".into(),
                    album: "Album".into(),
                }),
            })
            .unwrap();
        let before = std::fs::read(&path).unwrap();
        let rows = State::recorded(&path, &"a".repeat(32), &Cancellation::default()).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!((&*rows[0].title, rows[0].size), ("Song", 42));
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert!(State::recorded(&path, &"c".repeat(32), &Cancellation::default()).is_err());
    }

    #[test]
    fn missing_history_does_not_create_a_database_or_lock() {
        let directory = tempfile::tempdir().unwrap();
        assert!(State::recorded(
            &directory.path().join("missing.db"),
            &"a".repeat(32),
            &Cancellation::default()
        )
        .is_err());
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    }
}
