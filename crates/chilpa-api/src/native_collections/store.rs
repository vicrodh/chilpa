//! Cursor checkpoints and eight projected chunks share a 31 MiB database.
//! DELETE journals contain at most one old image per database page (<33 MiB).
use super::{CACHE_BYTES, CACHE_CHUNKS};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

pub(super) fn error(error: impl std::fmt::Display) -> String {
    error.to_string()
}
pub(super) const CHECKPOINT: usize = 8;

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Query {
    pub id: i64,
    pub generation: i64,
    pub plugin: String,
    pub params: Value,
    pub header: Value,
    pub revision: Option<String>,
    pub host: String,
    #[serde(default)]
    pub personal: bool,
    pub known: usize,
    pub pages: usize,
    pub next: Option<String>,
    pub total: Option<u64>,
    pub anchor: usize,
    #[serde(default)]
    pub restart: Option<String>,
}

pub(super) struct Page {
    pub ordinal: usize,
    pub first: usize,
    pub count: usize,
    pub reference: Option<String>,
    pub hash: String,
    pub next_hash: Option<String>,
}

pub(super) struct Store {
    pub db: Connection,
}
fn save(db: &Connection, query: &Query) -> rusqlite::Result<()> {
    let state = serde_json::to_string(query)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    db.execute(
        "UPDATE queries SET state=?2,touched=?3 WHERE id=?1",
        params![query.id, state, crate::now_unix()],
    )?;
    Ok(())
}
fn page(row: &rusqlite::Row<'_>) -> rusqlite::Result<Page> {
    Ok(Page {
        ordinal: row.get::<_, i64>(0)? as usize,
        first: row.get::<_, i64>(1)? as usize,
        count: row.get::<_, i64>(2)? as usize,
        reference: row.get(3)?,
        hash: row.get(4)?,
        next_hash: row.get(5)?,
    })
}

impl Store {
    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(error)?;
        }
        let db = Connection::open(path).map_err(error)?;
        db.busy_timeout(std::time::Duration::from_secs(2))
            .map_err(error)?;
        db.execute_batch("PRAGMA page_size=4096; PRAGMA max_page_count=7936; PRAGMA cache_size=-512;
            PRAGMA mmap_size=0; PRAGMA journal_mode=DELETE; PRAGMA synchronous=NORMAL;
            PRAGMA foreign_keys=ON; PRAGMA temp_store=MEMORY;
            CREATE TABLE IF NOT EXISTS queries(id INTEGER PRIMARY KEY AUTOINCREMENT,key TEXT UNIQUE NOT NULL,state TEXT NOT NULL,touched INTEGER NOT NULL);
            CREATE INDEX IF NOT EXISTS query_lru ON queries(touched);
            CREATE TABLE IF NOT EXISTS pages(query_id INTEGER REFERENCES queries(id) ON DELETE CASCADE,generation INTEGER,
                ordinal INTEGER,first_index INTEGER,item_count INTEGER,request_ref TEXT,request_hash TEXT NOT NULL,content_hash TEXT NOT NULL,next_hash TEXT,
                PRIMARY KEY(query_id,generation,ordinal));
            CREATE INDEX IF NOT EXISTS page_position ON pages(query_id,generation,first_index);
            CREATE INDEX IF NOT EXISTS page_cursor ON pages(query_id,generation,request_hash);
            CREATE TABLE IF NOT EXISTS projections(query_id INTEGER REFERENCES queries(id) ON DELETE CASCADE,generation INTEGER,
                first_index INTEGER,item_count INTEGER,body TEXT NOT NULL,bytes INTEGER NOT NULL,touched INTEGER NOT NULL,
                PRIMARY KEY(query_id,generation,first_index));
            CREATE TABLE IF NOT EXISTS projection_access(query_id INTEGER,generation INTEGER,first_index INTEGER,touched INTEGER NOT NULL,
                PRIMARY KEY(query_id,generation,first_index),
                FOREIGN KEY(query_id,generation,first_index) REFERENCES projections(query_id,generation,first_index) ON DELETE CASCADE);
            CREATE INDEX IF NOT EXISTS projection_access_lru ON projection_access(touched);
            INSERT OR IGNORE INTO projection_access SELECT query_id,generation,first_index,touched FROM projections;").map_err(error)?;
        let max: i64 = db
            .query_row("PRAGMA max_page_count", [], |row| row.get(0))
            .map_err(error)?;
        let size: i64 = db
            .query_row("PRAGMA page_size", [], |row| row.get(0))
            .map_err(error)?;
        if max > 7936 || size != 4096 {
            return Err("The collection cache exceeds its storage limit.".into());
        }
        Ok(Self { db })
    }

    fn room<T>(
        &self,
        active: i64,
        mut write: impl FnMut() -> rusqlite::Result<T>,
    ) -> Result<T, String> {
        loop {
            match write() {
                Ok(value) => return Ok(value),
                Err(rusqlite::Error::SqliteFailure(code, message))
                    if code.code == rusqlite::ErrorCode::DiskFull =>
                {
                    let oldest: Option<i64> = self
                        .db
                        .query_row(
                            "SELECT id FROM queries WHERE id<>?1 ORDER BY touched,id LIMIT 1",
                            [active],
                            |row| row.get(0),
                        )
                        .optional()
                        .map_err(error)?;
                    let Some(oldest) = oldest else {
                        return Err(rusqlite::Error::SqliteFailure(code, message).to_string());
                    };
                    // The failed transaction has rolled back before this LRU
                    // deletion. Free pages are reused without growing the file.
                    self.db
                        .execute("DELETE FROM queries WHERE id=?1", [oldest])
                        .map_err(error)?;
                }
                Err(failure) => return Err(failure.to_string()),
            }
        }
    }

    pub fn open_query(&self, key: &str, mut initial: Query) -> Result<Query, String> {
        let stored: Option<String> = self
            .db
            .query_row("SELECT state FROM queries WHERE key=?1", [key], |row| {
                row.get(0)
            })
            .optional()
            .map_err(error)?;
        if let Some(stored) = stored {
            return serde_json::from_str(&stored).map_err(error);
        }
        self.room(0,|| {
            let tx=self.db.unchecked_transaction()?;
            // The broker has one active query. Other descriptors are historical.
            tx.execute("DELETE FROM queries WHERE id IN (SELECT id FROM queries ORDER BY touched DESC,id DESC LIMIT -1 OFFSET 31)",[])?;
            tx.execute("INSERT INTO queries(key,state,touched) VALUES (?1,'{}',?2)",params![key,crate::now_unix()])?;
            initial.id=tx.last_insert_rowid();
            save(&tx,&initial)?;
            tx.commit()?;
            Ok(initial.clone())
        })
    }

    pub fn save(&self, query: &Query) -> Result<(), String> {
        self.room(query.id, || save(&self.db, query))
    }

    pub fn containing(&self, query: &Query, first: usize) -> Result<Option<Page>, String> {
        self.db.query_row("SELECT ordinal,first_index,item_count,request_ref,content_hash,next_hash FROM pages
            WHERE query_id=?1 AND generation=?2 AND first_index<=?3 AND first_index+item_count>?3 ORDER BY first_index DESC,ordinal DESC LIMIT 1",
            params![query.id,query.generation,first as i64],page).optional().map_err(error)
    }

    pub fn at(&self, query: &Query, ordinal: usize) -> Result<Option<Page>, String> {
        self.db.query_row("SELECT ordinal,first_index,item_count,request_ref,content_hash,next_hash FROM pages
            WHERE query_id=?1 AND generation=?2 AND ordinal=?3",params![query.id,query.generation,ordinal as i64],page).optional().map_err(error)
    }

    pub fn checkpoint(&self, query: &Query, ordinal: usize) -> Result<Page, String> {
        self.db.query_row("SELECT ordinal,first_index,item_count,request_ref,content_hash,next_hash FROM pages
            WHERE query_id=?1 AND generation=?2 AND ordinal<=?3 AND request_ref IS NOT NULL ORDER BY ordinal DESC LIMIT 1",
            params![query.id,query.generation,ordinal as i64],page).map_err(error)
    }

    pub fn append(&self, query: &Query, reference: &str, page: &Page) -> Result<(), String> {
        let request_hash = super::projection::hash(reference.as_bytes());
        if let Some(next) = &page.next_hash {
            let repeated:bool=self.db.query_row("SELECT EXISTS(SELECT 1 FROM pages WHERE query_id=?1 AND generation=?2 AND request_hash=?3)",
                params![query.id,query.generation,next],|row|row.get(0)).map_err(error)?;
            if repeated || *next == request_hash {
                return Err("The source repeated a collection cursor.".into());
            }
        }
        self.room(query.id, || {
            let tx = self.db.unchecked_transaction()?;
            // At most one full request cursor for every eight pages. The frontier
            // lives once in Query.next; replay checks every intermediate fingerprint.
            let checkpoint = (page.ordinal % CHECKPOINT == 0).then_some(reference);
            if page.ordinal == 0 {
                tx.execute("DELETE FROM pages WHERE query_id=?1", [query.id])?;
                tx.execute("DELETE FROM projections WHERE query_id=?1", [query.id])?;
            }
            tx.execute(
                "INSERT INTO pages VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![
                    query.id,
                    query.generation,
                    page.ordinal as i64,
                    page.first as i64,
                    page.count as i64,
                    checkpoint,
                    request_hash,
                    page.hash,
                    page.next_hash
                ],
            )?;
            save(&tx, query)?;
            tx.commit()
        })
    }

    pub fn cached(&self, query: &Query, first: usize) -> Result<Option<Value>, String> {
        let raw:Option<String>=self.db.query_row("SELECT body FROM projections WHERE query_id=?1 AND generation=?2 AND first_index<=?3
            AND first_index+item_count>?3 ORDER BY first_index DESC LIMIT 1",params![query.id,query.generation,first as i64],|row|row.get(0)).optional().map_err(error)?;
        let chunk: Option<Value> = raw
            .map(|raw| serde_json::from_str(&raw).map_err(error))
            .transpose()?;
        if let Some(chunk) = &chunk {
            // Recency lives separately: rewriting a row containing a large
            // overflow value can require a second copy of that value's pages.
            self.room(query.id,||self.db.execute("UPDATE projection_access SET touched=(SELECT coalesce(max(touched),0)+1 FROM projection_access)
                WHERE query_id=?1 AND generation=?2 AND first_index=?3",params![query.id,query.generation,chunk["first"].as_i64().unwrap_or(0)]))?;
        }
        Ok(chunk)
    }

    pub fn cache(&self, query: &Query, chunk: &Value) -> Result<(), String> {
        let body = serde_json::to_string(chunk).map_err(error)?;
        if body.len() > super::CHUNK_BYTES {
            return Err("The collection window exceeds its display limit.".into());
        }
        self.room(query.id,|| {
            let tx=self.db.unchecked_transaction()?;
            let touched:i64=tx.query_row("SELECT coalesce(max(touched),0)+1 FROM projection_access",[],|row|row.get(0))?;
            tx.execute("INSERT OR REPLACE INTO projections VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![query.id,query.generation,chunk["first"].as_i64().unwrap_or(0),chunk["count"].as_i64().unwrap_or(0),body,body.len() as i64,touched])?;
            tx.execute("INSERT INTO projection_access VALUES (?1,?2,?3,?4)",
                params![query.id,query.generation,chunk["first"].as_i64().unwrap_or(0),touched])?;
            loop {
                let (count,bytes):(i64,i64)=tx.query_row("SELECT count(*),coalesce(sum(bytes),0) FROM projections",[],|row|Ok((row.get(0)?,row.get(1)?)))?;
                if count<=CACHE_CHUNKS as i64 && bytes<=CACHE_BYTES as i64 { break; }
                tx.execute("DELETE FROM projections WHERE (query_id,generation,first_index)=(SELECT query_id,generation,first_index FROM projection_access ORDER BY touched LIMIT 1)",[])?;
            }
            tx.commit()
        })
    }
}
