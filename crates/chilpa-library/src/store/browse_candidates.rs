//! I evaluate the extra filters once while preserving every column's cascade.

use super::{Filter, Library};
use rusqlite::{params_from_iter, types::ToSql};

impl Library {
    /// I share only source, quality and format eligibility across this read.
    /// TEMP tables also work on a connection whose main database is read-only.
    pub fn with_browse_candidates<T>(&self, filter: &Filter, read: impl FnOnce() -> T) -> rusqlite::Result<T> {
        if self.extra_candidates.borrow().is_some() { return Ok(read()); }
        let extra = Filter {
            excluded_sources: filter.excluded_sources.clone(), qualities: filter.qualities.clone(),
            formats: filter.formats.clone(), ..Default::default()
        };
        let (clause, args) = super::browse::clause(&extra);
        let before = self.db.total_changes();
        let prepared = (|| {
            self.db.execute_batch("CREATE TEMP TABLE IF NOT EXISTS browse_candidates(id INTEGER PRIMARY KEY);
            DELETE FROM browse_candidates;")?;
            self.db.execute(&format!("INSERT INTO browse_candidates SELECT t.id FROM tracks t NOT INDEXED {clause}"), params_from_iter(args.iter()))?;
            Ok::<_, rusqlite::Error>(())
        })();
        self.candidate_changes.set(self.candidate_changes.get() + self.db.total_changes() - before);
        prepared?;
        *self.extra_candidates.borrow_mut() = Some(extra);
        struct Reset<'a>(&'a Library);
        impl Drop for Reset<'_> {
            fn drop(&mut self) { *self.0.extra_candidates.borrow_mut() = None; }
        }
        let _reset = Reset(self);
        Ok(read())
    }

    /// I ignore TEMP writes and refuse transaction-local cache stamps.
    pub fn read_revision(&self) -> rusqlite::Result<(i64, u64)> {
        if !self.db.is_autocommit() { return Err(rusqlite::Error::InvalidQuery); }
        let external = self.db.query_row("PRAGMA data_version", [], |row| row.get(0))?;
        Ok((external, self.db.total_changes().saturating_sub(self.candidate_changes.get())))
    }

    pub(super) fn browse_clause(&self, filter: &Filter) -> (String, Vec<Box<dyn ToSql>>) {
        let prepared = self.extra_candidates.borrow();
        if prepared.as_ref().is_some_and(|extra| extra.excluded_sources == filter.excluded_sources
            && extra.qualities == filter.qualities && extra.formats == filter.formats) {
            let mut columns = filter.clone();
            columns.reset_extra();
            let (mut sql, args) = super::browse::clause(&columns);
            sql.push_str(" AND t.id IN (SELECT id FROM temp.browse_candidates)");
            (sql, args)
        } else {
            super::browse::clause(filter)
        }
    }
}
