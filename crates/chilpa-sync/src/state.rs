use crate::{conflict, invalid, model::Source, AssetKind, Fingerprint, RelativePath, Result, Target, MAX_ASSETS, MAX_METADATA_BYTES};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::path::Path;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Managed {
    // A missing digest is permitted only in a pending copy, before staging completes.
    pub fingerprint: Option<Fingerprint>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub source_stamp: Option<crate::source::Stamp>,
    pub kind: AssetKind, pub key: String,
    #[serde(default)] pub title: String,
    #[serde(default)] pub artist: String,
    #[serde(default)] pub album: String,
}

impl Managed {
    pub fn digest(&self) -> Result<&Fingerprint> {
        self.fingerprint.as_ref().filter(|value| value.sha256.len() == 64 && value.sha256.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or_else(|| invalid("sync record has no committed content digest"))
    }
    pub fn digest_growth(&self) -> usize {
        if self.fingerprint.is_none() { crate::model::MAX_DIGEST_JSON_GROWTH } else { 0 }
    }
    pub fn size(&self) -> u64 {
        self.fingerprint.as_ref().map_or_else(|| self.source_stamp.as_ref().map_or(0, |s| s.size), |d| d.size)
    }
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
        db.execute_batch("CREATE TABLE IF NOT EXISTS sync_source_cache(id INTEGER PRIMARY KEY CHECK(id=1), record TEXT NOT NULL);")?;
        Ok(Self { db, _lease: lease })
    }
    pub(crate) fn source_cache(&self) -> Result<Option<String>> {
        // I bound the SQLite allocation before fetching the optional cache payload.
        let size: Option<i64> = self.db.query_row("SELECT length(CAST(record AS BLOB)) FROM sync_source_cache WHERE id=1", [], |r| r.get(0)).optional()?;
        if size.is_some_and(|size| size < 0 || size > MAX_METADATA_BYTES as i64) { return Ok(None); }
        Ok(self.db.query_row("SELECT record FROM sync_source_cache WHERE id=1", [], |r| r.get(0)).optional()?)
    }
    pub(crate) fn save_source_cache(&self, record: &str) -> Result<()> {
        if record.len() > MAX_METADATA_BYTES { return Err(invalid("source cache exceeds its metadata budget")); }
        self.db.execute("INSERT INTO sync_source_cache(id,record) VALUES (1,?1) ON CONFLICT(id) DO UPDATE SET record=excluded.record WHERE sync_source_cache.record<>excluded.record", [record])?;
        Ok(())
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
            managed.digest()?;
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
            bytes += record.len() + operation.desired.as_ref().map_or(0, Managed::digest_growth);
            if bytes > MAX_METADATA_BYTES * 2 { return Err(invalid("sync journal exceeds its metadata budget")); }
            tx.execute("INSERT INTO sync_pending(position,record) VALUES (?1,?2)", params![i as i64, record])?;
        }
        tx.commit()?;
        Ok(())
    }
    pub(crate) fn staged(&self, position: i64, pending: &Pending) -> Result<()> {
        pending.operation.desired.as_ref().ok_or_else(|| invalid("staged copy has no desired record"))?.digest()?;
        let record = serde_json::to_string(pending).map_err(|e| invalid(e.to_string()))?;
        let (previous_bytes, deferred): (i64, bool) = self.db.query_row(
            "SELECT length(CAST(record AS BLOB)), CASE WHEN json_type(record,'$.operation.desired.fingerprint')='null' THEN 1 ELSE 0 END FROM sync_pending WHERE position=?1",
            [position], |row| Ok((row.get(0)?, row.get(1)?)))?;
        let previous_bytes = usize::try_from(previous_bytes).map_err(|_| invalid("invalid journal record length"))?;
        let reserved = previous_bytes.saturating_add(if deferred { crate::model::MAX_DIGEST_JSON_GROWTH } else { 0 });
        // Every row stays within its share of the total reserved by start(). I avoid
        // rescanning the entire journal for every copied track, which would be quadratic.
        if record.len() > reserved || record.len() > MAX_METADATA_BYTES * 2 {
            return Err(invalid("staged digest exceeds its reserved journal budget"));
        }
        self.db.execute("UPDATE sync_pending SET record=?1 WHERE position=?2", params![record, position])?;
        Ok(())
    }
    /// I record a published fact before removing its backup or pending row.
    pub(crate) fn published(&mut self, operation: &Operation) -> Result<()> {
        let tx = self.db.transaction()?;
        if let Some(desired) = &operation.desired {
            desired.digest()?;
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

    fn deferred_record() -> Managed {
        Managed { fingerprint:None, source_stamp:None, kind:AssetKind::Media, key:"fixture".into(),
            title:"Fixture".into(), artist:String::new(), album:String::new() }
    }

    #[test]
    fn digest_reservation_covers_the_largest_serialized_fingerprint_exactly() {
        let mut record = deferred_record();
        let before = serde_json::to_vec(&record).unwrap().len();
        let reserved = record.digest_growth();
        record.fingerprint = Some(Fingerprint { size:u64::MAX, sha256:"f".repeat(64) });
        let after = serde_json::to_vec(&record).unwrap().len();
        assert_eq!(after - before, crate::model::MAX_DIGEST_JSON_GROWTH);
        assert_eq!(before + reserved, after);
        assert_eq!(record.digest_growth(), 0);
    }

    #[test]
    fn staged_materialization_cannot_exceed_its_reserved_share_of_the_journal() {
        let directory = tempfile::tempdir().unwrap();
        let mut state = State::open(&directory.path().join("sync.db")).unwrap();
        let operation = Operation { path:RelativePath::new("Music/fixture.flac").unwrap(), previous:None,
            desired:Some(deferred_record()), source:None };
        state.start(&[operation]).unwrap();
        let (position, mut pending) = state.pending().unwrap().remove(0);
        let before: String = state.db.query_row("SELECT record FROM sync_pending", [], |r| r.get(0)).unwrap();
        pending.operation.desired.as_mut().unwrap().fingerprint = Some(Fingerprint { size:u64::MAX, sha256:"f".repeat(64) });
        let mut oversized = pending.clone();
        oversized.operation.desired.as_mut().unwrap().title.push('x');
        assert!(matches!(state.staged(position, &oversized), Err(crate::Error::Invalid(_))));
        let retained: String = state.db.query_row("SELECT record FROM sync_pending", [], |r| r.get(0)).unwrap();
        assert_eq!(retained, before, "failed materialization must leave the durable journal unchanged");
        state.staged(position, &pending).unwrap();
        let committed: String = state.db.query_row("SELECT record FROM sync_pending", [], |r| r.get(0)).unwrap();
        assert_eq!(committed.len(), before.len() + crate::model::MAX_DIGEST_JSON_GROWTH);
        assert!(matches!(state.staged(position, &oversized), Err(crate::Error::Invalid(_))));
    }

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
