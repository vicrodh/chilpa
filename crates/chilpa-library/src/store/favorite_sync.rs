//! Favorites kept in step with the server that holds the item (schema 55),
//! by the same rules as ratings (`rating_sync`):
//!
//! - A change the listener made and that has not reached the server wins;
//!   once the server holds it, it is dropped.
//! - The first whole reading of a server is not a change: a server's
//!   favorite that Chilpa does not have comes in, and a heart Chilpa has and
//!   the server does not stays, without being sent.
//! - After that, only a value that differs from the last one read moves the
//!   heart here. A reading asked for before Chilpa's own write landed is not
//!   believed.
//! - Nothing here writes to a server but `queue_favorite`.

use super::favorites::FavoriteTarget;
use super::Library;
use rusqlite::{params, OptionalExtension};
use std::collections::HashSet;

/// Whether a server holds an item as a favorite, as one reading says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FavoriteRead {
    /// The server's own id.
    pub id: String,
    /// A track's queue entry, or a record's release id.
    pub local: String,
    pub on: bool,
    /// When the listing that said it was asked for, in unix milliseconds.
    pub read_ms: i64,
}

/// A change waiting to reach its server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingFavorite {
    pub origin: String,
    pub kind: String,
    pub id: String,
    pub on: bool,
    pub asked: i64,
    pub attempts: i64,
    pub error: String,
}

fn kind_column(kind: &str) -> Option<&'static str> {
    match kind {
        "track" => Some("tracks_at"),
        "album" => Some("albums_at"),
        _ => None,
    }
}

impl Library {
    /// Whether that origin's favorites go both ways. On unless turned off.
    pub fn favorite_sync_enabled(&self, origin: &str) -> rusqlite::Result<bool> {
        let on: Option<i64> = self.db.query_row("SELECT enabled FROM favorite_origins WHERE origin = ?1", [origin], |r| r.get(0)).optional()?;
        Ok(on.is_none_or(|on| on != 0))
    }

    /// Turned off, the changes on their way are dropped and what the server
    /// said is forgotten: turned on again, its favorites come in as new.
    pub fn set_favorite_sync(&self, origin: &str, on: bool) -> rusqlite::Result<()> {
        let tx = self.own_transaction()?;
        tx.execute(
            "INSERT INTO favorite_origins(origin, enabled) VALUES (?1, ?2) ON CONFLICT(origin) DO UPDATE SET enabled = excluded.enabled",
            params![origin, on],
        )?;
        if !on {
            tx.execute("UPDATE favorite_origins SET tracks_at = NULL, albums_at = NULL WHERE origin = ?1", [origin])?;
            tx.execute("DELETE FROM favorite_outbox WHERE origin = ?1", [origin])?;
            tx.execute("DELETE FROM remote_favorites WHERE origin = ?1", [origin])?;
        }
        tx.finish()
    }

    /// Settings turned favorite sync off for every server: what waits is
    /// dropped and what servers said is forgotten, so turned on again their
    /// favorites come in as new. Each server's own switch stays as it was.
    pub fn forget_favorite_changes(&self) -> rusqlite::Result<()> {
        let tx = self.own_transaction()?;
        tx.execute("DELETE FROM favorite_outbox", [])?;
        tx.execute("DELETE FROM remote_favorites", [])?;
        tx.execute("UPDATE favorite_origins SET tracks_at = NULL, albums_at = NULL", [])?;
        tx.finish()
    }

    /// The items of that origin the server held as favorites when last read.
    pub fn remote_hearts(&self, origin: &str) -> rusqlite::Result<HashSet<(String, String)>> {
        self.db
            .prepare("SELECT kind, external_id FROM remote_favorites WHERE origin = ?1 AND favorite = 1")?
            .query_map([origin], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect()
    }

    /// What the server said of one item last; `None` when never read.
    pub fn remote_favorite(&self, origin: &str, kind: &str, id: &str) -> rusqlite::Result<Option<bool>> {
        let on: Option<bool> = self.db
            .query_row("SELECT favorite FROM remote_favorites WHERE origin = ?1 AND kind = ?2 AND external_id = ?3", params![origin, kind, id], |r| r.get(0))
            .optional()?;
        if on.is_some() {
            return Ok(on);
        }
        Ok(self.favorites_read_whole(origin, kind)?.then_some(false))
    }

    fn favorites_read_whole(&self, origin: &str, kind: &str) -> rusqlite::Result<bool> {
        let Some(column) = kind_column(kind) else { return Ok(false) };
        let at: Option<Option<i64>> = self.db
            .query_row(&format!("SELECT {column} FROM favorite_origins WHERE origin = ?1"), [origin], |r| r.get(0))
            .optional()?;
        Ok(at.flatten().is_some())
    }

    /// Takes in what a server of the library (`source`) said of its items of
    /// one kind. `whole` says the reading covered all of them.
    pub fn apply_remote_favorites(&self, origin: &str, source: i64, kind: &str, reads: &[FavoriteRead], whole: bool) -> rusqlite::Result<(usize, usize)> {
        let (mut changed, mut settled) = (0, 0);
        let Some(column) = kind_column(kind) else { return Ok((0, 0)) };
        if !self.favorite_sync_enabled(origin)? {
            return Ok((0, 0));
        }
        let tx = self.own_transaction()?;
        let seen_whole = self.favorites_read_whole(origin, kind)?;
        for read in reads {
            let known: Option<(bool, i64)> = tx
                .query_row(
                    "SELECT favorite, sent_ms FROM remote_favorites WHERE origin = ?1 AND kind = ?2 AND external_id = ?3",
                    params![origin, kind, read.id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            if known.is_some_and(|(_, sent)| sent >= read.read_ms) {
                continue;
            }
            let Some(target) = self.favorite_target(source, kind, &read.local)? else { continue };
            let pending: Option<bool> = tx
                .query_row("SELECT favorite FROM favorite_outbox WHERE origin = ?1 AND kind = ?2 AND external_id = ?3", params![origin, kind, read.id], |r| r.get(0))
                .optional()?;
            match pending {
                Some(on) if on == read.on => {
                    tx.execute("DELETE FROM favorite_outbox WHERE origin = ?1 AND kind = ?2 AND external_id = ?3", params![origin, kind, read.id])?;
                    settled += 1;
                }
                Some(_) => {}
                None => {
                    let first = known.is_none() && !seen_whole;
                    let previous = known.is_some_and(|(on, _)| on);
                    let here = self.is_favorite(target)?;
                    let take = if first { !here && read.on } else { previous != read.on };
                    if take && here != read.on {
                        self.set_favorite(favorite_of(target), read.on)?;
                        changed += 1;
                    }
                }
            }
            if read.on || known.is_some() {
                tx.execute(
                    "INSERT INTO remote_favorites(origin, kind, external_id, favorite, read_at) VALUES (?1, ?2, ?3, ?4, unixepoch())
                     ON CONFLICT(origin, kind, external_id) DO UPDATE SET favorite = excluded.favorite, read_at = excluded.read_at",
                    params![origin, kind, read.id, read.on],
                )?;
            }
        }
        if whole {
            tx.execute(
                &format!("INSERT INTO favorite_origins(origin, {column}) VALUES (?1, unixepoch())
                          ON CONFLICT(origin) DO UPDATE SET {column} = COALESCE({column}, excluded.{column})"),
                [origin],
            )?;
        }
        tx.finish()?;
        Ok((changed, settled))
    }

    fn favorite_target(&self, source: i64, kind: &str, local: &str) -> rusqlite::Result<Option<Hearted>> {
        Ok(match kind {
            "track" => self.db
                .query_row("SELECT id FROM tracks WHERE source_id = ?1 AND external_id = ?2", params![source, local], |r| r.get(0))
                .optional()?
                .map(Hearted::Track),
            "album" => self.db
                .query_row("SELECT id FROM albums WHERE version_key = ?1", [format!("{source}:{local}")], |r| r.get(0))
                .optional()?
                .map(Hearted::Album),
            _ => None,
        })
    }

    /// Whether a track or record of the library carries the heart now.
    pub fn is_favorite(&self, target: Hearted) -> rusqlite::Result<bool> {
        let (table, key, id) = match target {
            Hearted::Track(id) => ("track_favorites", "track_id", id),
            Hearted::Album(id) => ("album_favorites", "album_id", id),
        };
        Ok(self.db.query_row(&format!("SELECT 1 FROM {table} WHERE {key} = ?1"), [id], |_| Ok(())).optional()?.is_some())
    }

    /// Queues a heart the listener changed for its server, replacing one on
    /// its way. `remote` is what the server holds, when known: a change to
    /// what it already holds sends nothing. True when queued.
    pub fn queue_favorite(&self, origin: &str, kind: &str, id: &str, on: bool, remote: Option<bool>) -> rusqlite::Result<bool> {
        if kind_column(kind).is_none() || id.is_empty() {
            return Err(rusqlite::Error::InvalidParameterName("invalid favorite change".into()));
        }
        let tx = self.own_transaction()?;
        if remote == Some(on) {
            tx.execute("DELETE FROM favorite_outbox WHERE origin = ?1 AND kind = ?2 AND external_id = ?3", params![origin, kind, id])?;
            tx.finish()?;
            return Ok(false);
        }
        tx.execute("UPDATE favorite_clock SET revision = revision + 1 WHERE id = 1", [])?;
        tx.execute(
            "INSERT INTO favorite_outbox(origin, kind, external_id, favorite, asked)
             VALUES (?1, ?2, ?3, ?4, (SELECT revision FROM favorite_clock WHERE id = 1))
             ON CONFLICT(origin, kind, external_id) DO UPDATE SET favorite = excluded.favorite, asked = excluded.asked,
                attempts = 0, next_at = 0, error = ''",
            params![origin, kind, id, on],
        )?;
        tx.finish()?;
        Ok(true)
    }

    /// The changes due by `now` (unix seconds), oldest first.
    pub fn due_favorites(&self, now: i64, limit: usize) -> rusqlite::Result<Vec<PendingFavorite>> {
        self.db
            .prepare(
                "SELECT origin, kind, external_id, favorite, asked, attempts, error FROM favorite_outbox
                 WHERE next_at <= ?1 ORDER BY next_at, asked LIMIT ?2",
            )?
            .query_map(params![now, limit as i64], |r| {
                Ok(PendingFavorite { origin: r.get(0)?, kind: r.get(1)?, id: r.get(2)?, on: r.get(3)?, asked: r.get(4)?, attempts: r.get(5)?, error: r.get(6)? })
            })?
            .collect()
    }

    pub fn next_favorite_due(&self) -> rusqlite::Result<Option<i64>> {
        self.db.query_row("SELECT min(next_at) FROM favorite_outbox", [], |r| r.get(0))
    }

    /// The server took the change: it goes, unless a newer one replaced it.
    pub fn favorite_sent(&self, change: &PendingFavorite, sent_ms: i64) -> rusqlite::Result<()> {
        let tx = self.own_transaction()?;
        tx.execute(
            "DELETE FROM favorite_outbox WHERE origin = ?1 AND kind = ?2 AND external_id = ?3 AND asked = ?4",
            params![change.origin, change.kind, change.id, change.asked],
        )?;
        tx.execute(
            "INSERT INTO remote_favorites(origin, kind, external_id, favorite, read_at, sent_ms) VALUES (?1, ?2, ?3, ?4, unixepoch(), ?5)
             ON CONFLICT(origin, kind, external_id) DO UPDATE SET favorite = excluded.favorite, sent_ms = excluded.sent_ms",
            params![change.origin, change.kind, change.id, change.on, sent_ms],
        )?;
        tx.finish()
    }

    pub fn favorite_failed(&self, change: &PendingFavorite, why: &str, next_at: i64) -> rusqlite::Result<()> {
        self.db.execute(
            "UPDATE favorite_outbox SET attempts = attempts + 1, next_at = ?5, error = ?6
             WHERE origin = ?1 AND kind = ?2 AND external_id = ?3 AND asked = ?4",
            params![change.origin, change.kind, change.id, change.asked, next_at, why],
        )?;
        Ok(())
    }

    pub fn drop_favorite(&self, change: &PendingFavorite) -> rusqlite::Result<()> {
        self.db.execute(
            "DELETE FROM favorite_outbox WHERE origin = ?1 AND kind = ?2 AND external_id = ?3 AND asked = ?4",
            params![change.origin, change.kind, change.id, change.asked],
        )?;
        Ok(())
    }

    /// Per origin, how many changes wait after failing, and why the last did.
    pub fn favorite_failures(&self) -> rusqlite::Result<Vec<(String, usize, String)>> {
        self.db
            .prepare(
                "SELECT origin, count(*), (SELECT error FROM favorite_outbox o WHERE o.origin = f.origin AND o.attempts > 0 ORDER BY o.asked DESC LIMIT 1)
                 FROM favorite_outbox f WHERE attempts > 0 GROUP BY origin ORDER BY origin",
            )?
            .query_map([], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as usize, r.get(2)?)))?
            .collect()
    }
}

/// A track or record of the library that a server may hold as a favorite.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hearted {
    Track(i64),
    Album(i64),
}

fn favorite_of(target: Hearted) -> FavoriteTarget<'static> {
    match target {
        Hearted::Track(id) => FavoriteTarget::Track(id),
        Hearted::Album(id) => FavoriteTarget::Album(id),
    }
}

#[cfg(test)]
mod tests;
