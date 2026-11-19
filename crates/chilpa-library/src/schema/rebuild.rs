//! A step that rebuilds a table SQLite cannot change in place: a new
//! definition, the same rows with the same ids, and every index and trigger
//! the table had, read from the library itself rather than written out here
//! again, so a library with an index of its own keeps it.
//!
//! The table is copied into its new definition, the old one is dropped and
//! the new one takes its name, all in one transaction. Foreign keys are off
//! for the swap (with them on, dropping a table its children reference
//! fails) and `legacy_alter_table` is on (without it, the rename rewrites
//! the triggers of other tables that name the table, and fails on the first
//! one). Both are connection settings a transaction cannot change, so they
//! are set before it begins and put back as they were after it ends,
//! whatever happens.
//!
//! Before it commits it checks that the rows, the columns, the indexes and
//! the triggers are the ones there were, and that no foreign key that
//! pointed somewhere points nowhere now. Anything else rolls the step back.

use rusqlite::Connection;

pub(crate) struct Rebuild {
    pub(crate) table: &'static str,
    /// Every column the table has, in order, before and after.
    pub(crate) columns: &'static [&'static str],
    /// The new definition, of a table named `<table>_new`.
    pub(crate) create: &'static str,
}

/// The parts a rebuild reports: the copy, the swap, the indexes, and the
/// rest (triggers, ids, statistics and the checks).
pub(crate) const PARTS: usize = 4;

#[cfg(test)]
thread_local! {
    /// How many times a rebuild checked the foreign keys, and ran ANALYZE.
    pub(crate) static FOREIGN_KEY_CHECKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    pub(crate) static ANALYZED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
thread_local! {
    /// What a test makes go wrong inside the transaction, after the copy.
    pub(crate) static TROUBLE: std::cell::Cell<Option<&'static str>> = const { std::cell::Cell::new(None) };
}

/// The connection's settings as they were, put back on every way out.
struct Settings<'a> {
    db: &'a Connection,
    foreign_keys: i64,
    legacy: i64,
}

impl Drop for Settings<'_> {
    fn drop(&mut self) {
        let _ = self.db.execute_batch(&format!("PRAGMA legacy_alter_table = {}; PRAGMA foreign_keys = {};", self.legacy, self.foreign_keys));
    }
}

/// What the checks compare before and after.
#[derive(PartialEq, Debug)]
struct Shape {
    rows: i64,
    /// Name, type, whether it may be null, its default, its place in the key.
    columns: Vec<(String, String, i64, Option<String>, i64)>,
    objects: Vec<(String, String, String)>,
    broken: Vec<(String, i64, String, i64)>,
}

/// The indexes and triggers of `table`, in the order they were made.
fn objects(db: &Connection, table: &str) -> rusqlite::Result<Vec<(String, String, String)>> {
    db.prepare("SELECT type, name, sql FROM sqlite_master
        WHERE tbl_name = ?1 AND type IN ('index', 'trigger') AND sql IS NOT NULL ORDER BY rowid")?
        .query_map([table], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?.collect()
}

/// The tables with a foreign key to `table`, and the column that holds it.
fn children(db: &Connection, table: &str) -> rusqlite::Result<Vec<(String, String)>> {
    db.prepare("SELECT m.name, f.\"from\" FROM sqlite_master m JOIN pragma_foreign_key_list(m.name) f
        WHERE m.type = 'table' AND lower(f.\"table\") = lower(?1) ORDER BY m.name")?
        .query_map([table], |row| Ok((row.get(0)?, row.get(1)?)))?.collect()
}

/// `checked` asks for the foreign keys too; an empty table has nothing to
/// check, and a new library is mostly empty tables.
fn shape(db: &Connection, table: &str, checked: bool) -> rusqlite::Result<Shape> {
    let rows = db.query_row(&format!("SELECT count(*) FROM \"{table}\""), [], |row| row.get(0))?;
    let columns = db.prepare(&format!("PRAGMA table_info(\"{table}\")"))?
        .query_map([], |row| Ok((row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)))?.collect::<rusqlite::Result<_>>()?;
    let mut objects = objects(db, table)?;
    objects.sort();
    let mut broken = Vec::new();
    if !checked { return Ok(Shape { rows, columns, objects, broken }); }
    let mut tables: Vec<String> = children(db, table)?.into_iter().map(|(child, _)| child).collect();
    tables.push(table.to_string());
    tables.dedup();
    #[cfg(test)]
    FOREIGN_KEY_CHECKS.with(|cell| cell.set(cell.get() + 1));
    for name in tables {
        let mut check = db.prepare(&format!("PRAGMA foreign_key_check(\"{name}\")"))?;
        let rows = check.query_map([], |row| Ok((row.get(0)?, row.get::<_, Option<i64>>(1)?.unwrap_or(-1), row.get(2)?, row.get(3)?)))?;
        broken.extend(rows.collect::<rusqlite::Result<Vec<_>>>()?);
    }
    broken.sort();
    Ok(Shape { rows, columns, objects, broken })
}

fn refused(what: String) -> rusqlite::Error {
    rusqlite::Error::SqliteFailure(rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CONSTRAINT), Some(what))
}

/// Runs `rebuild` as the step to version `target`. `part` hears each part
/// after the first as it begins (the step's caller says the first), and an
/// error from it rolls the step back.
pub(crate) fn run(db: &Connection, target: i64, rebuild: &Rebuild, part: &mut dyn FnMut(usize) -> rusqlite::Result<()>) -> rusqlite::Result<()> {
    if !db.is_autocommit() {
        return Err(refused("a table is rebuilt only outside a transaction".into()));
    }
    let read = |pragma: &str| db.query_row(&format!("PRAGMA {pragma}"), [], |row| row.get::<_, i64>(0));
    let _settings = Settings { db, foreign_keys: read("foreign_keys")?, legacy: read("legacy_alter_table")? };
    db.execute_batch("PRAGMA foreign_keys = OFF; PRAGMA legacy_alter_table = ON;")?;
    if read("foreign_keys")? != 0 {
        return Err(refused("foreign keys could not be turned off to rebuild a table".into()));
    }
    let tx = rusqlite::Transaction::new_unchecked(db, rusqlite::TransactionBehavior::Immediate)?;
    let applied: i64 = tx.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if applied >= target {
        return tx.commit();
    }
    let table = rebuild.table;
    let filled = tx.query_row(&format!("SELECT EXISTS (SELECT 1 FROM \"{table}\")"), [], |row| row.get::<_, bool>(0))?;
    let before = shape(&tx, table, filled)?;
    if before.columns.iter().map(|column| column.0.as_str()).ne(rebuild.columns.iter().copied()) {
        return Err(refused(format!("{table} has the columns {:?}, not the ones this step rebuilds", before.columns)));
    }
    let made = objects(&tx, table)?;
    // Without its indexes the copy reuses their pages.
    for (kind, name, _) in &made {
        if kind == "index" {
            tx.execute_batch(&format!("DROP INDEX \"{name}\";"))?;
        }
    }
    let columns = rebuild.columns.iter().map(|column| format!("\"{column}\"")).collect::<Vec<_>>().join(", ");
    tx.execute_batch(rebuild.create)?;
    tx.execute_batch(&format!("INSERT INTO \"{table}_new\" ({columns}) SELECT {columns} FROM \"{table}\" ORDER BY rowid;"))?;
    #[cfg(test)]
    if let Some(copied) = std::env::var_os("CHILPA_TEST_REBUILD_PAUSE") {
        // A test kills the process here, between the copy and the commit.
        let _ = std::fs::write(copied, b"copied");
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
    part(1)?;
    tx.execute_batch(&format!("DROP TABLE \"{table}\"; ALTER TABLE \"{table}_new\" RENAME TO \"{table}\";"))?;
    #[cfg(test)]
    if let Some(trouble) = TROUBLE.with(|cell| cell.get()) {
        tx.execute_batch(trouble)?;
    }
    part(2)?;
    for (kind, _, sql) in &made {
        if kind == "index" {
            tx.execute_batch(sql)?;
        }
    }
    part(3)?;
    for (kind, _, sql) in &made {
        if kind == "trigger" {
            tx.execute_batch(sql)?;
        }
    }
    if rebuild.create.contains("AUTOINCREMENT") {
        keep_ids(&tx, table)?;
    }
    // Statistics of an empty table are what it already had.
    if filled {
        tx.execute_batch("ANALYZE;")?;
        #[cfg(test)]
        ANALYZED.with(|cell| cell.set(cell.get() + 1));
    }
    let after = shape(&tx, table, filled)?;
    if after != before {
        return Err(refused(format!("rebuilding {table} changed it: {before:?} became {after:?}")));
    }
    tx.pragma_update(None, "user_version", target)?;
    tx.commit()
}

/// The next id is past every id the table has and every one a row elsewhere
/// still names, even a row whose track is gone: a new row must never take
/// an id somebody else's history points at.
fn keep_ids(db: &Connection, table: &str) -> rusqlite::Result<()> {
    let mut highest: i64 = db.query_row(&format!("SELECT coalesce(max(rowid), 0) FROM \"{table}\""), [], |row| row.get(0))?;
    for (child, column) in children(db, table)? {
        let named: i64 = db.query_row(&format!("SELECT coalesce(max(\"{column}\"), 0) FROM \"{child}\""), [], |row| row.get(0))?;
        highest = highest.max(named);
    }
    db.execute("INSERT INTO sqlite_sequence(name, seq) SELECT ?1, 0 WHERE NOT EXISTS (SELECT 1 FROM sqlite_sequence WHERE name = ?1)", [table])?;
    db.execute("UPDATE sqlite_sequence SET seq = max(seq, ?2) WHERE name = ?1", rusqlite::params![table, highest])?;
    Ok(())
}
