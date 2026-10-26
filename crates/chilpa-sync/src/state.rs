use crate::{conflict, invalid, model::Source, AssetKind, Fingerprint, RelativePath, Result, Target, MAX_ASSETS, MAX_METADATA_BYTES};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::path::Path;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Managed {
    pub fingerprint: Fingerprint, pub kind: AssetKind, pub key: String,
    #[serde(default)] pub title: String,
    #[serde(default)] pub artist: String,
    #[serde(default)] pub album: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Operation {
    pub path: RelativePath,
    pub previous: Option<Fingerprint>,
    pub desired: Option<Managed>,
    pub source: Option<Source>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Pending {
    pub operation: Operation,
    pub stage: RelativePath,
    pub backup: RelativePath,
}

/// I keep the journal separate from the library and hold one local writer lease.
pub struct State { db: Connection, _lease: File }
impl State {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) { std::fs::create_dir_all(parent)?; }
        let lease = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(path.with_extension("sync-lock"))?;
        lease.try_lock().map_err(|_| conflict("another job owns this sync profile"))?;
        let db = Connection::open(path)?;
        db.execute_batch("PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON;")?;
        let version: u32 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        match version {
            0 => db.execute_batch("BEGIN IMMEDIATE;
                CREATE TABLE sync_target(id TEXT PRIMARY KEY, name TEXT NOT NULL, revision INTEGER NOT NULL);
                CREATE TABLE sync_files(path TEXT PRIMARY KEY, record TEXT NOT NULL);
                CREATE TABLE sync_pending(position INTEGER PRIMARY KEY, record TEXT NOT NULL);
                PRAGMA user_version=1; COMMIT;")?,
            1 => (), _ => return Err(invalid("unsupported sync journal version")),
        }
        Ok(Self { db, _lease: lease })
    }
    pub fn target(&self) -> Result<Option<Target>> {
        Ok(self.db.query_row("SELECT id,name FROM sync_target", [], |r| Ok(Target { version:1, id:r.get(0)?, name:r.get(1)? })).optional()?)
    }
    pub fn revision(&self) -> Result<u64> {
        let revision: i64 = self.db.query_row("SELECT revision FROM sync_target", [], |r| r.get(0)).optional()?.unwrap_or(0);
        u64::try_from(revision).map_err(|_| invalid("negative sync revision"))
    }
    pub(crate) fn bind(&mut self, target: &Target) -> Result<()> {
        target.validate()?;
        if let Some(previous) = self.target()? {
            if previous.id != target.id { return Err(conflict("this profile belongs to another storage")); }
        } else {
            self.db.execute("INSERT INTO sync_target(id,name,revision) VALUES (?1,?2,0)", params![target.id,target.name])?;
        }
        Ok(())
    }
    pub(crate) fn files(&self) -> Result<BTreeMap<RelativePath, Managed>> {
        let mut statement = self.db.prepare("SELECT path,record FROM sync_files ORDER BY path")?;
        let mut rows = statement.query([])?;
        let mut files = BTreeMap::new();
        let mut bytes = 0;
        while let Some(row) = rows.next()? {
            let path: String = row.get(0)?;
            let record: String = row.get(1)?;
            bytes += path.len() + record.len();
            if bytes > MAX_METADATA_BYTES || files.len() >= MAX_ASSETS { return Err(invalid("sync manifest exceeds its metadata budget")); }
            let managed: Managed = serde_json::from_str(&record).map_err(|_| invalid("damaged sync file record"))?;
            files.insert(RelativePath::new(path)?, managed);
        }
        Ok(files)
    }
    pub(crate) fn pending(&self) -> Result<Vec<(i64, Pending)>> {
        let mut statement = self.db.prepare("SELECT position,record FROM sync_pending ORDER BY position")?;
        let mut rows = statement.query([])?;
        let mut pending = Vec::new();
        let mut bytes = 0;
        while let Some(row) = rows.next()? {
            let record: String = row.get(1)?;
            bytes += record.len();
            if bytes > MAX_METADATA_BYTES * 2 || pending.len() >= MAX_ASSETS { return Err(invalid("sync journal exceeds its metadata budget")); }
            let item: Pending = serde_json::from_str(&record).map_err(|_| invalid("damaged pending sync operation"))?;
            let legacy = item.stage.as_str().starts_with(".chilpa-sync/staging/") && item.backup.as_str().starts_with(".chilpa-sync/backup/");
            let siblings = item.stage.parent() == item.operation.path.parent() && item.backup.parent() == item.operation.path.parent()
                && item.stage.filename().starts_with(".chilpa-sync-") && item.stage.filename().ends_with(".partial")
                && item.backup.filename().starts_with(".chilpa-sync-") && item.backup.filename().ends_with(".backup");
            if (!legacy && !siblings) || (!item.operation.path.as_str().starts_with("Music/") && !item.operation.path.as_str().starts_with("Playlists/")) {
                return Err(invalid("pending operation escapes sync staging"));
            }
            pending.push((row.get(0)?, item));
        }
        Ok(pending)
    }
    pub(crate) fn start(&mut self, operations: &[Operation]) -> Result<()> {
        if operations.len() > MAX_ASSETS { return Err(invalid("too many sync changes in one batch")); }
        if !self.pending()?.is_empty() { return Err(conflict("recover the interrupted sync before starting another")); }
        let tx = self.db.transaction()?;
        let run = crate::token();
        let mut bytes = 0;
        for (i, operation) in operations.iter().enumerate() {
            let parent = operation.path.parent().ok_or_else(|| invalid("sync media needs a parent folder"))?;
            let pending = Pending { operation:operation.clone(),
                // I need only one rename transaction per transition on MTP, with no cross-folder move.
                stage:RelativePath::new(format!("{parent}/.chilpa-sync-{run}-{i}.partial"))?,
                backup:RelativePath::new(format!("{parent}/.chilpa-sync-{run}-{i}.backup"))? };
            let record = serde_json::to_string(&pending).map_err(|e| invalid(e.to_string()))?;
            bytes += record.len();
            if bytes > MAX_METADATA_BYTES * 2 { return Err(invalid("sync journal exceeds its metadata budget")); }
            tx.execute("INSERT INTO sync_pending(position,record) VALUES (?1,?2)", params![i as i64, record])?;
        }
        tx.commit()?;
        Ok(())
    }
    /// I record a published fact before removing its backup or pending row.
    pub(crate) fn published(&mut self, operation: &Operation) -> Result<()> {
        let tx = self.db.transaction()?;
        if let Some(desired) = &operation.desired {
            let record = serde_json::to_string(desired).map_err(|e| invalid(e.to_string()))?;
            tx.execute("INSERT INTO sync_files(path,record) VALUES (?1,?2) ON CONFLICT(path) DO UPDATE SET record=excluded.record",
                params![operation.path.as_str(), record])?;
        } else { tx.execute("DELETE FROM sync_files WHERE path=?1", [operation.path.as_str()])?; }
        tx.execute("UPDATE sync_target SET revision=revision+1", [])?;
        tx.commit()?;
        Ok(())
    }
    pub(crate) fn finished(&self, position: i64) -> Result<()> {
        self.db.execute("DELETE FROM sync_pending WHERE position=?1", [position])?;
        Ok(())
    }
    pub fn managed_count(&self) -> Result<usize> {
        let count: i64 = self.db.query_row("SELECT count(*) FROM sync_files", [], |r| r.get(0))?;
        usize::try_from(count).map_err(|_| invalid("invalid sync file count"))
    }
    pub fn interrupted(&self) -> Result<bool> { Ok(self.db.query_row("SELECT EXISTS(SELECT 1 FROM sync_pending)", [], |r| r.get(0))?) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_oversized_batch_leaves_no_unreadable_recovery_journal() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("sync.db");
        let mut state = State::open(&path).unwrap();
        let operation = Operation { path:RelativePath::new("Music/fixture.flac").unwrap(), previous:None, desired:None, source:None };
        let error = state.start(&vec![operation; MAX_ASSETS + 1]).unwrap_err();
        assert!(matches!(error, crate::Error::Invalid(_)));
        assert!(!state.interrupted().unwrap());
        assert!(state.pending().unwrap().is_empty());
    }
}
