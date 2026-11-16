//! One disposable album query. The caller owns its private directory and drops
//! this connection before removing it. No library table is written.
//! At most 64 MiB of pages, a 512 KiB cache, no journal or retained worker.
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
pub use rusqlite::Error;

pub const MAX_TAG_BYTES: usize = 1024 * 1024;

pub fn validate_tag(tag: &str) -> rusqlite::Result<()> {
    if tag.len() > MAX_TAG_BYTES {
        return Err(Error::InvalidParameterName("Genre text exceeds the 1 MiB query limit.".into()));
    }
    Ok(())
}

pub struct GenreIndex { db: Connection }

pub struct GenreEntry {
    pub token: String,
    pub name: String,
    pub truncated: bool,
    pub chars: usize,
    pub originals: usize,
}

impl GenreIndex {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let db = Connection::open(path)?;
        db.execute_batch("PRAGMA page_size=4096; PRAGMA max_page_count=16384;
            PRAGMA cache_size=-512; PRAGMA temp_store=FILE; PRAGMA journal_mode=OFF;
            PRAGMA synchronous=OFF;
            CREATE TABLE originals(token TEXT NOT NULL,name TEXT NOT NULL,PRIMARY KEY(token,name)) WITHOUT ROWID;
            CREATE TABLE groups(token TEXT PRIMARY KEY,name TEXT NOT NULL,canonical TEXT NOT NULL,
                sort TEXT NOT NULL,originals INTEGER NOT NULL,matched INTEGER NOT NULL);
            CREATE INDEX genre_order ON groups(sort,name,token);
            CREATE TABLE sources(key TEXT PRIMARY KEY) WITHOUT ROWID;
            BEGIN;")?;
        Ok(Self { db })
    }

    pub fn insert(&self, token: &str, original: &str, canonical: &str, matched: bool) -> rusqlite::Result<()> {
        validate_tag(original)?;
        validate_tag(canonical)?;
        if self.db.execute("INSERT OR IGNORE INTO originals VALUES (?1,?2)", params![token,original])? == 0 { return Ok(()); }
        let sort = original.to_lowercase();
        self.db.execute("INSERT INTO groups VALUES (?1,?2,?3,?4,1,?5)
            ON CONFLICT(token) DO UPDATE SET originals=originals+1,matched=max(matched,excluded.matched)",
            params![token,original,canonical,sort,matched])?;
        self.db.execute("UPDATE groups SET name=?2,canonical=?3,sort=?4 WHERE token=?1 AND (sort,name)>(?4,?2)",
            params![token,original,canonical,sort])?;
        Ok(())
    }

    pub fn source(&self, key: &str) -> rusqlite::Result<()> {
        self.db.execute("INSERT OR IGNORE INTO sources VALUES (?1)",[key])?;
        Ok(())
    }

    pub fn has_source(&self, key: &str) -> rusqlite::Result<bool> {
        self.db.query_row("SELECT EXISTS(SELECT 1 FROM sources WHERE key=?1)",[key],|row|row.get(0))
    }

    pub fn canonical(&self, token: &str) -> rusqlite::Result<Option<String>> {
        self.db.query_row("SELECT canonical FROM groups WHERE token=?1",[token],|row|row.get(0)).optional()
    }

    pub fn page(&self, offset: usize, limit: usize, descending: bool) -> rusqlite::Result<(Vec<GenreEntry>, usize)> {
        let total = self.db.query_row("SELECT count(*) FROM groups WHERE matched=1",[],|row|row.get::<_,i64>(0))?;
        let order = if descending { "sort DESC,name DESC,token DESC" } else { "sort,name,token" };
        // Match the API presentation projection without loading full labels.
        let sql = format!("SELECT token,CASE WHEN length(name)>128 THEN substr(name,1,95)||'…'||substr(name,-32) ELSE substr(name,1,128) END,
            length(name)>128,originals,length(name) FROM groups WHERE matched=1 ORDER BY {order} LIMIT ?1 OFFSET ?2");
        let rows = self.db.prepare(&sql)?.query_map(params![limit.min(60) as i64,offset.min(i64::MAX as usize) as i64],|row|Ok(GenreEntry {
            token:row.get(0)?,name:row.get(1)?,truncated:row.get(2)?,originals:row.get::<_,i64>(3)? as usize,chars:row.get::<_,i64>(4)? as usize,
        }))?.collect::<rusqlite::Result<_>>()?;
        Ok((rows,total as usize))
    }
}
