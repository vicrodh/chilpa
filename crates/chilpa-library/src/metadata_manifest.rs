//! Disposable metadata selection, owned by one worker. No catalog migrations.
//! SQLite uses a 2 MiB page cache and at most 1 GiB of pages. The owner's
//! private temporary directory also owns the rollback journal and removes both.
use rusqlite::{Connection, OptionalExtension, params};
use std::path::Path;

pub struct MetadataManifest {
    db: Connection,
}

pub struct Entry {
    pub id: i64,
    pub path: String,
    pub artist: String,
    pub album_artist: String,
    pub album: String,
    pub group: String,
    pub release: String,
    pub selected: bool,
    pub state: String,
    pub dirty: bool,
    pub written: bool,
    pub body: String,
}

impl MetadataManifest {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let db = Connection::open(path)?;
        db.execute_batch("PRAGMA page_size=4096; PRAGMA max_page_count=262144;
            PRAGMA cache_size=-2048; PRAGMA temp_store=FILE; PRAGMA journal_mode=DELETE;
            CREATE TABLE entries(id INTEGER PRIMARY KEY,path TEXT UNIQUE NOT NULL,artist TEXT NOT NULL,
                album TEXT NOT NULL,group_key TEXT NOT NULL,release_id TEXT NOT NULL,selected INTEGER NOT NULL,
                state TEXT NOT NULL,dirty INTEGER NOT NULL,written INTEGER NOT NULL,body TEXT NOT NULL,album_artist TEXT NOT NULL);
            CREATE INDEX entry_group ON entries(group_key,id);
            CREATE TABLE directories(path TEXT PRIMARY KEY,visited INTEGER NOT NULL DEFAULT 0);
            CREATE TABLE homes(path TEXT PRIMARY KEY,evidence TEXT NOT NULL);
            CREATE TABLE documents(kind TEXT NOT NULL,key TEXT NOT NULL,body TEXT NOT NULL,PRIMARY KEY(kind,key));
            CREATE TABLE plan_entries(id INTEGER PRIMARY KEY,body TEXT NOT NULL);
            CREATE TABLE assignments(id INTEGER PRIMARY KEY,disc INTEGER NOT NULL,position INTEGER NOT NULL,path TEXT NOT NULL,ordinal INTEGER NOT NULL DEFAULT 0);
            CREATE INDEX assignment_order ON assignments(disc,position,path);")?;
        Ok(Self { db })
    }

    pub fn assign_disc(
        &self,
        id: i64,
        disc: usize,
        position: u64,
        path: &str,
    ) -> rusqlite::Result<()> {
        self.db.execute(
            "INSERT INTO assignments(id,disc,position,path) VALUES (?1,?2,?3,?4)",
            params![id, disc as i64, position.min(i64::MAX as u64) as i64, path],
        )?;
        Ok(())
    }

    pub fn assignment_counts(&self, disc: usize) -> rusqlite::Result<(i64, i64)> {
        self.db.query_row("SELECT count(*),coalesce(max(nullif(position,9223372036854775807)),0) FROM assignments WHERE disc=?1", [disc as i64], |row| Ok((row.get(0)?,row.get(1)?)))
    }

    pub fn order_assignments(&self) -> rusqlite::Result<()> {
        self.db.execute_batch("WITH ranked AS (SELECT id,row_number() OVER (PARTITION BY disc ORDER BY position,path) AS number FROM assignments)
            UPDATE assignments SET ordinal=(SELECT number FROM ranked WHERE ranked.id=assignments.id)")
    }

    pub fn next_assignment(&self, disc: usize, after: i64) -> rusqlite::Result<Option<(i64, u32)>> {
        self.db
            .query_row(
                "SELECT id,ordinal FROM assignments WHERE disc=?1 AND id>?2 ORDER BY id LIMIT 1",
                params![disc as i64, after],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
    }

    pub fn assigned_remaining_groups(&self) -> rusqlite::Result<Vec<serde_json::Value>> {
        self.db.prepare("SELECT group_key,substr(min(album),1,256),CASE WHEN count(DISTINCT album_artist)=1 THEN substr(min(album_artist),1,256) ELSE '' END,count(*) FROM entries
            WHERE group_key IN (SELECT e.group_key FROM entries e JOIN assignments a ON a.id=e.id)
            AND id NOT IN (SELECT id FROM assignments) GROUP BY group_key ORDER BY min(id) LIMIT 256")?
            .query_map([],|row|Ok(serde_json::json!({"key":row.get::<_,String>(0)?,"title":row.get::<_,String>(1)?,"artist":row.get::<_,String>(2)?,"tracks":row.get::<_,i64>(3)?})))?.collect()
    }

    pub fn document(&self, kind: &str, key: &str) -> rusqlite::Result<Option<String>> {
        self.db
            .query_row(
                "SELECT body FROM documents WHERE kind=?1 AND key=?2",
                params![kind, key],
                |row| row.get(0),
            )
            .optional()
    }

    pub fn put_document(&self, kind: &str, key: &str, body: &str) -> rusqlite::Result<()> {
        if body.len() > 8 * 1024 * 1024 {
            return Err(rusqlite::Error::InvalidParameterName(
                "metadata document exceeds 8 MiB".into(),
            ));
        }
        self.db.execute("INSERT INTO documents VALUES (?1,?2,?3) ON CONFLICT(kind,key) DO UPDATE SET body=excluded.body",params![kind,key,body])?;
        Ok(())
    }

    pub fn clear_documents(&self, kind: &str) -> rusqlite::Result<()> {
        self.db
            .execute("DELETE FROM documents WHERE kind=?1", [kind])?;
        Ok(())
    }

    pub fn clear_plan(&self) -> rusqlite::Result<()> {
        self.db
            .execute_batch("DELETE FROM assignments; DELETE FROM plan_entries; DELETE FROM documents WHERE kind='plan';")
    }

    pub fn put_plan_entry(&self, id: i64, body: &str) -> rusqlite::Result<()> {
        if body.len() > 256 * 1024 {
            return Err(rusqlite::Error::InvalidParameterName(
                "metadata proposal exceeds 256 KiB per file".into(),
            ));
        }
        self.db.execute("INSERT INTO plan_entries VALUES (?1,?2) ON CONFLICT(id) DO UPDATE SET body=excluded.body",params![id,body])?;
        Ok(())
    }

    pub fn next_plan_entry(&self, after: i64) -> rusqlite::Result<Option<(i64, String)>> {
        self.db
            .query_row(
                "SELECT id,body FROM plan_entries WHERE id>?1 ORDER BY id LIMIT 1",
                [after],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
    }

    pub fn plan_start(&self, offset: usize) -> rusqlite::Result<i64> {
        self.db
            .query_row(
                "SELECT id-1 FROM plan_entries ORDER BY id LIMIT 1 OFFSET ?1",
                [offset.min(i64::MAX as usize) as i64],
                |row| row.get(0),
            )
            .optional()
            .map(|id| id.unwrap_or(i64::MAX))
    }

    pub fn plan_count(&self) -> rusqlite::Result<i64> {
        self.db
            .query_row("SELECT count(*) FROM plan_entries", [], |row| row.get(0))
    }

    pub fn contains_path(&self, path: &str) -> rusqlite::Result<bool> {
        self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM entries WHERE path=?1)",
            [path],
            |row| row.get(0),
        )
    }

    pub fn insert(&self, row: &Entry) -> rusqlite::Result<i64> {
        self.db.execute("INSERT OR IGNORE INTO entries(path,artist,album,group_key,release_id,selected,state,dirty,written,body,album_artist)
            VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)", params![row.path,row.artist,row.album,row.group,row.release,
                row.selected,row.state,row.dirty,row.written,row.body,row.album_artist])?;
        self.db
            .query_row("SELECT id FROM entries WHERE path=?1", [&row.path], |r| {
                r.get(0)
            })
    }

    pub fn update(&self, row: &Entry) -> rusqlite::Result<()> {
        self.db.execute(
            "UPDATE entries SET artist=?2,album=?3,group_key=?4,release_id=?5,selected=?6,
            state=?7,dirty=?8,written=?9,body=?10,album_artist=?11 WHERE id=?1 AND body<>?10",
            params![
                row.id,
                row.artist,
                row.album,
                row.group,
                row.release,
                row.selected,
                row.state,
                row.dirty,
                row.written,
                row.body,
                row.album_artist
            ],
        )?;
        Ok(())
    }

    pub fn next(&self, after: i64) -> rusqlite::Result<Option<Entry>> {
        self.next_in_group(after, None)
    }

    pub fn next_in_group(
        &self,
        after: i64,
        group: Option<&str>,
    ) -> rusqlite::Result<Option<Entry>> {
        self.db
            .query_row(
                "SELECT id,path,artist,album,group_key,release_id,selected,state,dirty,written,body,album_artist
            FROM entries WHERE id>?1 AND (?2 IS NULL OR group_key=?2) ORDER BY id LIMIT 1",
                params![after, group],
                read_entry,
            )
            .optional()
    }

    pub fn entry(&self, id: i64) -> rusqlite::Result<Option<Entry>> {
        self.db
            .query_row(
                "SELECT id,path,artist,album,group_key,release_id,selected,state,dirty,written,body,album_artist
            FROM entries WHERE id=?1",
                [id],
                read_entry,
            )
            .optional()
    }

    pub fn group_exists(&self, group: &str) -> rusqlite::Result<bool> {
        self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM entries WHERE group_key=?1)",
            [group],
            |row| row.get(0),
        )
    }

    pub fn page_start_in_group(&self, offset: usize, group: Option<&str>) -> rusqlite::Result<i64> {
        self.db.query_row("SELECT id-1 FROM entries WHERE (?2 IS NULL OR group_key=?2) ORDER BY id LIMIT 1 OFFSET ?1",
            params![offset.min(i64::MAX as usize) as i64,group], |row| row.get(0)).optional().map(|id| id.unwrap_or(i64::MAX))
    }

    pub fn scope_summary(&self, group: Option<&str>) -> rusqlite::Result<serde_json::Value> {
        self.db.query_row("SELECT count(*),coalesce(sum(selected),0),coalesce(sum(selected AND dirty AND state<>'unsupported'),0)
            FROM entries WHERE (?1 IS NULL OR group_key=?1)",[group],|row| Ok(serde_json::json!({
                "files":row.get::<_,i64>(0)?,"selected":row.get::<_,i64>(1)?,"changed":row.get::<_,i64>(2)?})))
    }

    pub fn group_discs(&self, group: &str) -> rusqlite::Result<Vec<serde_json::Value>> {
        self.db
            .prepare(
                "SELECT coalesce(json_extract(body,'$.effective.disc_number'),0),count(*),
            max(json_extract(body,'$.effective.disc_total')) FROM entries WHERE group_key=?1
            GROUP BY coalesce(json_extract(body,'$.effective.disc_number'),0) ORDER BY 1 LIMIT 257",
            )?
            .query_map([group], |row| {
                Ok(serde_json::json!({"number":row.get::<_,i64>(0)?,
                "tracks":row.get::<_,i64>(1)?,"total":row.get::<_,Option<i64>>(2)?}))
            })?
            .collect()
    }

    pub fn groups(&self, offset: usize, limit: usize) -> rusqlite::Result<Vec<serde_json::Value>> {
        self.db.prepare("SELECT group_key,min(album),CASE WHEN count(DISTINCT album_artist)=1 THEN min(album_artist) ELSE '' END,
            min(release_id),count(*),sum(selected),sum(dirty AND selected),min(path),sum(state='unsupported')
            FROM entries GROUP BY group_key ORDER BY min(id) LIMIT ?1 OFFSET ?2")?
            .query_map(params![limit as i64,offset.min(i64::MAX as usize) as i64], |r| Ok(serde_json::json!({
                "key":r.get::<_,String>(0)?,"title":r.get::<_,String>(1)?,"artist":r.get::<_,String>(2)?,
                "releaseId":r.get::<_,String>(3)?,"tracks":r.get::<_,i64>(4)?,"selected":r.get::<_,i64>(5)?,"changed":r.get::<_,i64>(6)?,
                "path":r.get::<_,String>(7)?,"unsupported":r.get::<_,i64>(8)?,"conflicts":[]})))?.collect()
    }

    pub fn summary(&self) -> rusqlite::Result<serde_json::Value> {
        self.db.query_row("SELECT count(*),count(DISTINCT group_key),coalesce(sum(selected),0),
            coalesce(sum(selected AND dirty AND state<>'unsupported'),0),coalesce(sum(selected AND NOT dirty AND state='unchanged'),0),
            coalesce(sum(state='unsupported'),0),coalesce(sum(state IN ('failed','conflict')),0),coalesce(sum(state='saved'),0)
            FROM entries", [], |r| Ok(serde_json::json!({
                "counts":{"files":r.get::<_,i64>(0)?,"groups":r.get::<_,i64>(1)?,"selected":r.get::<_,i64>(2)?},
                "summary":{"changed":r.get::<_,i64>(3)?,"unchanged":r.get::<_,i64>(4)?,"unsupported":r.get::<_,i64>(5)?,
                    "failed":r.get::<_,i64>(6)?,"saved":r.get::<_,i64>(7)?}})))
    }

    /// Store visited directories on disk, including canonicalized cycle keys.
    pub fn visit_directory(&self, path: &str) -> rusqlite::Result<bool> {
        self.db
            .execute(
                "INSERT OR IGNORE INTO directories(path,visited) VALUES (?1,1)",
                [path],
            )
            .map(|changed| changed > 0)
    }

    pub fn transaction(&self, action: impl FnOnce() -> Result<(), String>) -> Result<(), String> {
        let tx = self
            .db
            .unchecked_transaction()
            .map_err(|error| error.to_string())?;
        action()?;
        tx.commit().map_err(|error| error.to_string())
    }

    pub fn home_evidence(&self, path: &str, initial: &str) -> rusqlite::Result<String> {
        self.db.execute(
            "INSERT OR IGNORE INTO homes(path,evidence) VALUES (?1,?2)",
            params![path, initial],
        )?;
        self.db
            .query_row("SELECT evidence FROM homes WHERE path=?1", [path], |r| {
                r.get(0)
            })
    }

    pub fn update_home(&self, path: &str, evidence: &str) -> rusqlite::Result<()> {
        self.db.execute(
            "UPDATE homes SET evidence=?2 WHERE path=?1",
            params![path, evidence],
        )?;
        Ok(())
    }
}

fn read_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<Entry> {
    Ok(Entry {
        id: row.get(0)?,
        path: row.get(1)?,
        artist: row.get(2)?,
        album: row.get(3)?,
        group: row.get(4)?,
        release: row.get(5)?,
        selected: row.get(6)?,
        state: row.get(7)?,
        dirty: row.get(8)?,
        written: row.get(9)?,
        body: row.get(10)?,
        album_artist: row.get(11)?,
    })
}
