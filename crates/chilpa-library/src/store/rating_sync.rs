//! Ratings kept in step with the source that holds the item (schema 54).
//!
//! The stars shown stay in their own tables (`ratings`, `external_ratings`);
//! here is what each source said last, raw on its own scale, and the
//! listener's changes on their way to it. The rules:
//!
//! - A change the listener made and that has not reached the source wins
//!   over anything the source says meanwhile.
//! - What a source says is taken in only when it is new: a value that
//!   differs from what it said last. The first whole reading of a source is
//!   not a change: it fills only what has no stars here.
//! - Nothing here writes to a source: what goes out is only what the
//!   listener changed (`queue_rating`).

use super::ratings::RatingTarget;
use super::Library;
use rusqlite::{params, OptionalExtension};
use std::collections::HashSet;

/// Where the items a source rates are kept here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Holder {
    /// A server of the library, by its source: its tracks by their queue
    /// entry, its records by their release id.
    Server(i64),
    /// A plugin, whose items keep their stars under its id.
    Plugin(String),
}

/// A rating as a source said it.
#[derive(Clone, Debug, PartialEq)]
pub struct Incoming {
    /// The source's own id.
    pub id: String,
    /// What the library knows it by: a server track's queue entry, a server
    /// record's release id, a plugin item's identity.
    pub local: String,
    /// On the source's scale; 0 is unrated.
    pub raw: f64,
    /// `raw` as whole stars.
    pub stars: u8,
    /// When the listing that said it was asked for, in unix milliseconds.
    pub read_ms: i64,
}

/// A change waiting to reach its source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pending {
    pub origin: String,
    pub kind: String,
    pub id: String,
    pub stars: u8,
    /// The rating clock's revision of the change: a newer one replaces it.
    pub asked: i64,
    pub attempts: i64,
    pub error: String,
}

/// What taking a source's ratings in did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Applied {
    /// Stars here that changed to what the source says.
    pub changed: usize,
    /// Changes on their way that the source already holds, dropped.
    pub settled: usize,
}

/// Where one item's stars are kept here.
enum Local {
    Rated(RatingTarget),
    External { source: String, kind: String, id: String },
}

fn kind_column(kind: &str) -> Option<&'static str> {
    match kind {
        "track" => Some("tracks_at"),
        "album" => Some("albums_at"),
        _ => None,
    }
}

impl Library {
    /// Whether that origin's ratings go both ways. On unless turned off.
    pub fn rating_sync_enabled(&self, origin: &str) -> rusqlite::Result<bool> {
        let on: Option<i64> = self.db.query_row("SELECT enabled FROM rating_origins WHERE origin = ?1", [origin], |r| r.get(0)).optional()?;
        Ok(on.is_none_or(|on| on != 0))
    }

    /// Turned off, the changes on their way are dropped and what the source
    /// said is forgotten: turned on again, its ratings come in as new.
    pub fn set_rating_sync(&self, origin: &str, on: bool) -> rusqlite::Result<()> {
        let tx = self.own_transaction()?;
        tx.execute(
            "INSERT INTO rating_origins(origin, enabled) VALUES (?1, ?2)
             ON CONFLICT(origin) DO UPDATE SET enabled = excluded.enabled",
            params![origin, on],
        )?;
        if !on {
            tx.execute("UPDATE rating_origins SET tracks_at = NULL, albums_at = NULL WHERE origin = ?1", [origin])?;
            tx.execute("DELETE FROM rating_outbox WHERE origin = ?1", [origin])?;
            tx.execute("DELETE FROM remote_ratings WHERE origin = ?1", [origin])?;
        }
        tx.finish()
    }

    /// The items of that origin the source rated when it was last read, as
    /// (kind, id): an unrated reading of anything else changes nothing.
    pub fn remote_rated(&self, origin: &str) -> rusqlite::Result<HashSet<(String, String)>> {
        self.db
            .prepare("SELECT kind, external_id FROM remote_ratings WHERE origin = ?1 AND raw > 0")?
            .query_map([origin], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect()
    }

    /// What the source said of one item last: its raw value, 0 for unrated,
    /// `None` when it was never read.
    pub fn remote_rating(&self, origin: &str, kind: &str, id: &str) -> rusqlite::Result<Option<f64>> {
        let raw: Option<f64> = self.db
            .query_row("SELECT raw FROM remote_ratings WHERE origin = ?1 AND kind = ?2 AND external_id = ?3", params![origin, kind, id], |r| r.get(0))
            .optional()?;
        if raw.is_some() {
            return Ok(raw);
        }
        Ok(self.read_whole(origin, kind)?.then_some(0.0))
    }

    fn read_whole(&self, origin: &str, kind: &str) -> rusqlite::Result<bool> {
        let Some(column) = kind_column(kind) else { return Ok(false) };
        let at: Option<Option<i64>> = self.db
            .query_row(&format!("SELECT {column} FROM rating_origins WHERE origin = ?1"), [origin], |r| r.get(0))
            .optional()?;
        Ok(at.flatten().is_some())
    }

    /// Takes in what a source said of its items of one kind. `whole` says
    /// the reading covered every item of that kind it has: from then on, an
    /// item without a rating row was read unrated. A plugin's lists are never
    /// whole, and keep a row for what they read unrated.
    pub fn apply_remote_ratings(&self, origin: &str, holder: &Holder, kind: &str, reads: &[Incoming], whole: bool) -> rusqlite::Result<Applied> {
        let mut applied = Applied::default();
        let Some(column) = kind_column(kind) else { return Ok(applied) };
        if !self.rating_sync_enabled(origin)? {
            return Ok(applied);
        }
        let tx = self.own_transaction()?;
        let seen_whole = self.read_whole(origin, kind)?;
        let keeps_unrated = matches!(holder, Holder::Plugin(_));
        for read in reads {
            if read.stars > 5 || !read.raw.is_finite() || read.raw < 0.0 {
                continue;
            }
            let known: Option<(f64, i64)> = tx
                .query_row(
                    "SELECT raw, sent_ms FROM remote_ratings WHERE origin = ?1 AND kind = ?2 AND external_id = ?3",
                    params![origin, kind, read.id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            // Asked for before Chilpa's own write reached the source: it may
            // say what was there before it.
            if known.is_some_and(|(_, sent)| sent >= read.read_ms) {
                continue;
            }
            // Not in the library yet: read again at the next listing.
            let Some(local) = self.local_of(holder, kind, &read.local)? else { continue };
            let pending: Option<u8> = tx
                .query_row("SELECT stars FROM rating_outbox WHERE origin = ?1 AND kind = ?2 AND external_id = ?3", params![origin, kind, read.id], |r| r.get(0))
                .optional()?;
            match pending {
                Some(stars) if stars == read.stars => {
                    tx.execute("DELETE FROM rating_outbox WHERE origin = ?1 AND kind = ?2 AND external_id = ?3", params![origin, kind, read.id])?;
                    applied.settled += 1;
                }
                Some(_) => {}
                None => {
                    let first = known.is_none() && !seen_whole;
                    let previous = known.map_or(0.0, |(raw, _)| raw);
                    let here = self.local_stars(&local)?;
                    let take = if first { here == 0 && read.stars > 0 } else { previous != read.raw };
                    if take && here != read.stars {
                        self.set_local_stars(&local, read.stars)?;
                        applied.changed += 1;
                    }
                }
            }
            if read.raw > 0.0 || known.is_some() || keeps_unrated {
                tx.execute(
                    "INSERT INTO remote_ratings(origin, kind, external_id, raw, read_at) VALUES (?1, ?2, ?3, ?4, unixepoch())
                     ON CONFLICT(origin, kind, external_id) DO UPDATE SET raw = excluded.raw, read_at = excluded.read_at",
                    params![origin, kind, read.id, read.raw],
                )?;
            }
        }
        if whole && !keeps_unrated {
            tx.execute(
                &format!("INSERT INTO rating_origins(origin, {column}) VALUES (?1, unixepoch())
                          ON CONFLICT(origin) DO UPDATE SET {column} = COALESCE({column}, excluded.{column})"),
                [origin],
            )?;
        }
        tx.finish()?;
        Ok(applied)
    }

    fn local_of(&self, holder: &Holder, kind: &str, local: &str) -> rusqlite::Result<Option<Local>> {
        Ok(match (holder, kind) {
            (Holder::Server(source), "track") => self.db
                .query_row("SELECT id FROM tracks WHERE source_id = ?1 AND external_id = ?2", params![source, local], |r| r.get(0))
                .optional()?
                .map(|id| Local::Rated(RatingTarget::Track(id))),
            (Holder::Server(source), "album") => self.db
                .query_row("SELECT id FROM albums WHERE version_key = ?1", [format!("{source}:{local}")], |r| r.get(0))
                .optional()?
                .map(|id| Local::Rated(RatingTarget::Album(id))),
            (Holder::Plugin(source), "track" | "album") => Some(Local::External { source: source.clone(), kind: kind.into(), id: local.into() }),
            _ => None,
        })
    }

    fn local_stars(&self, local: &Local) -> rusqlite::Result<u8> {
        let stars: Option<u8> = match local {
            Local::Rated(target) => {
                let (table, key, id) = match *target {
                    RatingTarget::Track(id) => ("track_ratings", "track_id", id),
                    RatingTarget::Album(id) => ("album_ratings", "album_id", id),
                    RatingTarget::Playlist(id) => ("playlist_ratings", "playlist_id", id),
                };
                self.db.query_row(&format!("SELECT stars FROM {table} WHERE {key} = ?1"), [id], |r| r.get(0)).optional()?
            }
            Local::External { source, kind, id } => self.db
                .query_row("SELECT stars FROM external_ratings WHERE source = ?1 AND kind = ?2 AND external_id = ?3", params![source, kind, id], |r| r.get(0))
                .optional()?,
        };
        Ok(stars.unwrap_or(0))
    }

    fn set_local_stars(&self, local: &Local, stars: u8) -> rusqlite::Result<()> {
        match local {
            Local::Rated(target) => self.rate(*target, stars),
            Local::External { source, kind, id } => self.rate_external(source, kind, id, stars),
        }
    }

    /// The server a track or record of the library rates on: its source and
    /// what the server knows it by (a track's queue entry, a record's release
    /// id). `None` for anything else, including a record whose release id the
    /// server did not give.
    pub fn rated_on_server(&self, target: RatingTarget) -> rusqlite::Result<Option<(i64, String)>> {
        match target {
            RatingTarget::Track(id) => self.db
                .query_row(
                    "SELECT t.source_id, t.external_id FROM tracks t JOIN sources s ON s.id = t.source_id
                     WHERE t.id = ?1 AND s.kind = 'server'",
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional(),
            RatingTarget::Album(id) => {
                let key: Option<String> = self.db.query_row("SELECT version_key FROM albums WHERE id = ?1", [id], |r| r.get(0)).optional()?.flatten();
                let Some((source, release)) = key.as_deref().and_then(|k| k.split_once(':')) else { return Ok(None) };
                let Ok(source) = source.parse::<i64>() else { return Ok(None) };
                let server: bool = self.db
                    .query_row("SELECT kind = 'server' FROM sources WHERE id = ?1", [source], |r| r.get(0))
                    .optional()?
                    .unwrap_or(false);
                Ok((server && !release.is_empty() && !release.contains('\u{1f}')).then(|| (source, release.to_string())))
            }
            RatingTarget::Playlist(_) => Ok(None),
        }
    }

    /// The stars an item has here now, before a change.
    pub fn stars_of(&self, target: RatingTarget) -> rusqlite::Result<u8> {
        self.local_stars(&Local::Rated(target))
    }

    /// The stars a plugin's item has here now.
    pub fn external_stars(&self, source: &str, kind: &str, id: &str) -> rusqlite::Result<u8> {
        self.local_stars(&Local::External { source: source.into(), kind: kind.into(), id: id.into() })
    }

    /// Queues a change the listener made for its source, replacing one on
    /// its way. `remote` is what the source holds, as stars, when known:
    /// a change to what it already holds sends nothing. True when queued.
    pub fn queue_rating(&self, origin: &str, kind: &str, id: &str, stars: u8, remote: Option<u8>) -> rusqlite::Result<bool> {
        if stars > 5 || kind_column(kind).is_none() || id.is_empty() {
            return Err(rusqlite::Error::InvalidParameterName("invalid rating change".into()));
        }
        let tx = self.own_transaction()?;
        if remote == Some(stars) {
            tx.execute("DELETE FROM rating_outbox WHERE origin = ?1 AND kind = ?2 AND external_id = ?3", params![origin, kind, id])?;
            tx.finish()?;
            return Ok(false);
        }
        tx.execute(
            "INSERT INTO rating_outbox(origin, kind, external_id, stars, asked)
             VALUES (?1, ?2, ?3, ?4, (SELECT revision FROM rating_clock WHERE id = 1))
             ON CONFLICT(origin, kind, external_id) DO UPDATE SET stars = excluded.stars, asked = excluded.asked,
                attempts = 0, next_at = 0, error = ''",
            params![origin, kind, id, stars],
        )?;
        tx.finish()?;
        Ok(true)
    }

    /// The changes due by `now` (unix seconds), oldest first.
    pub fn due_ratings(&self, now: i64, limit: usize) -> rusqlite::Result<Vec<Pending>> {
        self.db
            .prepare(
                "SELECT origin, kind, external_id, stars, asked, attempts, error FROM rating_outbox
                 WHERE next_at <= ?1 ORDER BY next_at, asked LIMIT ?2",
            )?
            .query_map(params![now, limit as i64], |r| {
                Ok(Pending { origin: r.get(0)?, kind: r.get(1)?, id: r.get(2)?, stars: r.get(3)?, asked: r.get(4)?, attempts: r.get(5)?, error: r.get(6)? })
            })?
            .collect()
    }

    /// When the next change is due, in unix seconds; `None` when none waits.
    pub fn next_rating_due(&self) -> rusqlite::Result<Option<i64>> {
        self.db.query_row("SELECT min(next_at) FROM rating_outbox", [], |r| r.get(0))
    }

    /// The source took the change: it goes, unless a newer one replaced it,
    /// and what the source holds now is what was sent.
    pub fn rating_sent(&self, change: &Pending, raw: f64, sent_ms: i64) -> rusqlite::Result<()> {
        let tx = self.own_transaction()?;
        tx.execute(
            "DELETE FROM rating_outbox WHERE origin = ?1 AND kind = ?2 AND external_id = ?3 AND asked = ?4",
            params![change.origin, change.kind, change.id, change.asked],
        )?;
        tx.execute(
            "INSERT INTO remote_ratings(origin, kind, external_id, raw, read_at, sent_ms) VALUES (?1, ?2, ?3, ?4, unixepoch(), ?5)
             ON CONFLICT(origin, kind, external_id) DO UPDATE SET raw = excluded.raw, sent_ms = excluded.sent_ms",
            params![change.origin, change.kind, change.id, raw, sent_ms],
        )?;
        tx.finish()
    }

    /// The source did not take the change: it is tried again at `next_at`.
    pub fn rating_failed(&self, change: &Pending, why: &str, next_at: i64) -> rusqlite::Result<()> {
        self.db.execute(
            "UPDATE rating_outbox SET attempts = attempts + 1, next_at = ?5, error = ?6
             WHERE origin = ?1 AND kind = ?2 AND external_id = ?3 AND asked = ?4",
            params![change.origin, change.kind, change.id, change.asked, next_at, why],
        )?;
        Ok(())
    }

    /// A change that can never reach its source: the item or the source is
    /// gone. A newer change of the same item stays.
    pub fn drop_rating(&self, change: &Pending) -> rusqlite::Result<()> {
        self.db.execute(
            "DELETE FROM rating_outbox WHERE origin = ?1 AND kind = ?2 AND external_id = ?3 AND asked = ?4",
            params![change.origin, change.kind, change.id, change.asked],
        )?;
        Ok(())
    }

    /// Per origin, how many changes wait after failing at least once, and
    /// why the last one failed.
    pub fn rating_failures(&self) -> rusqlite::Result<Vec<(String, usize, String)>> {
        self.db
            .prepare(
                "SELECT origin, count(*), (SELECT error FROM rating_outbox o WHERE o.origin = f.origin AND o.attempts > 0 ORDER BY o.asked DESC LIMIT 1)
                 FROM rating_outbox f WHERE attempts > 0 GROUP BY origin ORDER BY origin",
            )?
            .query_map([], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as usize, r.get(2)?)))?
            .collect()
    }
}

#[cfg(test)]
mod tests;
