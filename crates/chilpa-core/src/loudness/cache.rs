//! The loudness each track was measured or said to have, kept between runs
//! (QBZ, `loudness_cache.rs`). A row holds the track's absolute integrated
//! loudness and its peak, so the start gain for any target is worked out
//! when it is read: changing the target never invalidates a row.
//!
//! Its own small database next to the library, `loudness.db`, as in QBZ:
//! the playback thread reads it and writes what ReplayGain says, the
//! analyzer writes what it measures, and it never waits on a scan of the
//! library.

use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::{Mutex, OnceLock};

/// Where a row came from, best last: a lower rank never overwrites a higher
/// one, and the same rank refreshes the row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rank {
    /// The analyzer's window: 30 seconds of the track or more, not all of it.
    Partial = 1,
    /// ReplayGain, the source's or the file's: a full-track figure.
    ReplayGain = 2,
    /// Measured here over the whole track: played to its end.
    Measured = 3,
}

impl Rank {
    fn as_str(self) -> &'static str {
        match self {
            Rank::Partial => "partial",
            Rank::ReplayGain => "replaygain",
            Rank::Measured => "measured",
        }
    }

    fn parse(text: &str) -> Self {
        match text {
            "measured" => Rank::Measured,
            "replaygain" => Rank::ReplayGain,
            _ => Rank::Partial,
        }
    }
}

/// One kept row.
#[derive(Clone, Debug, PartialEq)]
pub struct Stored {
    pub lufs: f32,
    /// Linear peak: true peak when measured here, sample peak from ReplayGain.
    pub peak: Option<f32>,
    pub rank: Rank,
}

/// The cache. Without a database (it could not be opened) every lookup
/// misses and every store is dropped: normalization still measures, it only
/// forgets.
pub struct LoudnessCache {
    connection: Option<Mutex<Connection>>,
}

const SCHEMA: &str = "PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
CREATE TABLE IF NOT EXISTS track_loudness (
    key TEXT PRIMARY KEY,
    lufs REAL NOT NULL,
    peak REAL NOT NULL DEFAULT 0,
    rank TEXT NOT NULL,
    measured_at INTEGER NOT NULL
);";

impl LoudnessCache {
    pub fn at(path: &Path) -> Self {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let opened = Connection::open(path).and_then(|connection| {
            connection.busy_timeout(std::time::Duration::from_secs(2))?;
            connection.execute_batch(SCHEMA)?;
            Ok(connection)
        });
        match opened {
            Ok(connection) => Self { connection: Some(Mutex::new(connection)) },
            Err(error) => {
                eprintln!("loudness: the cache could not be opened ({error}); measuring without keeping");
                Self { connection: None }
            }
        }
    }

    pub fn in_memory() -> Self {
        let connection = Connection::open_in_memory().and_then(|c| c.execute_batch(SCHEMA).map(|_| c)).ok();
        Self { connection: connection.map(Mutex::new) }
    }

    fn lock(&self) -> Option<std::sync::MutexGuard<'_, Connection>> {
        self.connection.as_ref()?.lock().ok()
    }

    /// Keeps a row unless a better-ranked one is there. `true` when it wrote.
    pub fn store(&self, key: &str, lufs: f32, peak: Option<f32>, rank: Rank) -> bool {
        if !lufs.is_finite() {
            return false;
        }
        let Some(connection) = self.lock() else { return false };
        let existing: Option<String> = connection
            .query_row("SELECT rank FROM track_loudness WHERE key = ?1", params![key], |row| row.get(0))
            .ok();
        if existing.is_some_and(|kept| Rank::parse(&kept) > rank) {
            return false;
        }
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
        connection
            .execute(
                "INSERT INTO track_loudness (key, lufs, peak, rank, measured_at) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(key) DO UPDATE SET lufs = excluded.lufs, peak = excluded.peak,
                    rank = excluded.rank, measured_at = excluded.measured_at",
                params![key, f64::from(lufs), f64::from(peak.unwrap_or(0.0)), rank.as_str(), now],
            )
            .map_err(|error| eprintln!("loudness: could not keep {key}: {error}"))
            .is_ok()
    }

    pub fn lookup(&self, key: &str) -> Option<Stored> {
        let connection = self.lock()?;
        connection
            .query_row("SELECT lufs, peak, rank FROM track_loudness WHERE key = ?1", params![key], |row| {
                Ok(Stored {
                    lufs: row.get::<_, f64>(0)? as f32,
                    peak: Some(row.get::<_, f64>(1)? as f32).filter(|p| *p > 0.0),
                    rank: Rank::parse(&row.get::<_, String>(2)?),
                })
            })
            .ok()
    }

    /// The start gain for `target`, or `None` for a track not kept.
    pub fn resolve_gain(&self, key: &str, target: f32, prevent_clipping: bool) -> Option<(f32, Rank)> {
        let row = self.lookup(key)?;
        Some((super::gain_for(row.lufs, row.peak, target, prevent_clipping), row.rank))
    }
}

/// The process's cache, opened the first time it is asked for.
pub fn shared() -> &'static LoudnessCache {
    static CACHE: OnceLock<LoudnessCache> = OnceLock::new();
    CACHE.get_or_init(|| LoudnessCache::at(&chilpa_dirs::data().join("loudness.db")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_keeps_absolute_loudness_and_resolves_any_target() {
        let cache = LoudnessCache::in_memory();
        assert!(cache.store("track", -12.0, Some(0.5), Rank::Measured));
        let (gain14, _) = cache.resolve_gain("track", -14.0, true).unwrap();
        let (gain18, _) = cache.resolve_gain("track", -18.0, true).unwrap();
        assert!((gain14 - super::super::db_to_linear(-2.0)).abs() < 0.001);
        assert!((gain18 - super::super::db_to_linear(-6.0)).abs() < 0.001);
        assert!(cache.resolve_gain("other", -14.0, true).is_none());
    }

    #[test]
    fn a_worse_source_never_overwrites_a_better_one() {
        let cache = LoudnessCache::in_memory();
        assert!(cache.store("t", -10.0, None, Rank::Measured));
        assert!(!cache.store("t", -20.0, None, Rank::ReplayGain), "ReplayGain does not replace a measurement");
        assert!(!cache.store("t", -20.0, None, Rank::Partial));
        assert_eq!(cache.lookup("t").unwrap().lufs, -10.0);
        assert!(cache.store("t", -11.0, Some(0.9), Rank::Measured), "the same rank refreshes the row");
        assert_eq!(cache.lookup("t").unwrap(), Stored { lufs: -11.0, peak: Some(0.9), rank: Rank::Measured });
        assert!(!cache.store("t", f32::NEG_INFINITY, None, Rank::Measured), "silence is not a loudness");
    }

    #[test]
    fn a_cache_on_disk_survives_and_one_that_cannot_open_forgets() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("deep").join("loudness.db");
        assert!(LoudnessCache::at(&path).store("t", -9.0, None, Rank::Partial));
        assert_eq!(LoudnessCache::at(&path).lookup("t").map(|r| r.rank), Some(Rank::Partial));
        let gone = LoudnessCache { connection: None };
        assert!(!gone.store("t", -9.0, None, Rank::Measured));
        assert!(gone.lookup("t").is_none());
    }
}
