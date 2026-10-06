//! Lyrics already found, and songs already known to have none.
//!
//! A previous player kept lyrics under a bare track number. The number was
//! not unique across sources and the row did not say what song it was for,
//! so a lookup could come back with another song's lyrics. Here:
//!
//! - **The key says what song it is**: the ISRC when the track has one; the
//!   folded artist and title otherwise, next to the length in whole
//!   seconds; for lyrics a source gave for its own track, that source and
//!   the track's reference in it.
//! - **Every row keeps what it was found for**, and a read checks it again
//!   with [`crate::matching`]. A row that no longer fits is not returned.
//! - **"No lyrics" is remembered for a few days, and a failure never.** A
//!   provider that was down must be asked again; one that said "none" may
//!   have them next week.
//! - **What a provider sent is kept as it came**, with the schema it was
//!   read into, so a better reader can read it again without asking
//!   anyone.

use crate::matching::{self, Claim, Track};
use crate::model::{Document, SCHEMA};
use rusqlite::{params, Connection};
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// How long found lyrics are served before asking again in the background.
pub const FRESH: Duration = Duration::from_secs(60 * 60 * 24 * 90);
/// How long "none" is believed.
pub const NONE_FOR: Duration = Duration::from_secs(60 * 60 * 24 * 7);
/// Rows nobody read in this long are removed when the cache opens.
pub const FORGET_AFTER: Duration = Duration::from_secs(60 * 60 * 24 * 180);

/// Where a row came from, so the screen can say it.
#[derive(Clone, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub struct Origin {
    /// A plugin id, or `file` and `tags` for what came with the track.
    pub provider: String,
    /// What the screen shows.
    pub name: String,
}

/// What a read finds.
#[derive(Clone, PartialEq, Debug)]
pub enum Hit {
    /// Lyrics, and whether it is time to look for better ones.
    Found { document: Document, origin: Origin, stale: bool },
    /// Somebody looked recently and there were none.
    None,
}

/// Lyrics on disk, in SQLite.
pub struct Cache {
    db: Connection,
}

impl Cache {
    /// Opens the cache at a path, creating it.
    pub fn open(path: &Path) -> rusqlite::Result<Cache> {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let db = Connection::open(path)?;
        Cache::with(db)
    }

    /// A cache that lives in memory, for tests.
    pub fn in_memory() -> rusqlite::Result<Cache> {
        Cache::with(Connection::open_in_memory()?)
    }

    fn with(db: Connection) -> rusqlite::Result<Cache> {
        db.pragma_update(None, "journal_mode", "WAL")?;
        db.busy_timeout(Duration::from_secs(2))?;
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS lyrics (
                key TEXT NOT NULL,
                seconds INTEGER NOT NULL,
                found INTEGER NOT NULL,
                document TEXT,
                provider TEXT,
                name TEXT,
                raw TEXT,
                format TEXT,
                schema INTEGER,
                title TEXT,
                artist TEXT,
                album TEXT,
                duration_ms INTEGER,
                isrc TEXT,
                fetched_at INTEGER NOT NULL,
                used_at INTEGER NOT NULL,
                PRIMARY KEY (key, seconds)
            );",
        )?;
        let cache = Cache { db };
        cache.forget_unused()?;
        Ok(cache)
    }

    /// Looks up lyrics for a track by its identity.
    pub fn get(&self, track: &Track) -> Option<Hit> {
        self.read(&identity(track), track)
    }

    /// Looks up lyrics a source gave for its own track.
    pub fn get_from_source(&self, plugin: &str, reference: &str, track: &Track) -> Option<Hit> {
        self.read(&source_key(plugin, reference), track)
    }

    fn read(&self, key: &str, track: &Track) -> Option<Hit> {
        let seconds = seconds_of(track);
        let mut statement = self
            .db
            .prepare(
                "SELECT seconds, found, document, provider, name, title, artist, album, duration_ms, isrc, fetched_at
                 FROM lyrics WHERE key = ?1 AND abs(seconds - ?2) <= 2
                 ORDER BY abs(seconds - ?2) LIMIT 3",
            )
            .ok()?;
        let rows = statement
            .query_map(params![key, seconds], |r| {
                Ok(Row {
                    seconds: r.get(0)?,
                    found: r.get::<_, i64>(1)? != 0,
                    document: r.get(2)?,
                    provider: r.get(3)?,
                    name: r.get(4)?,
                    claim: Claim {
                        title: r.get(5)?,
                        artist: r.get(6)?,
                        album: r.get(7)?,
                        duration_ms: r.get::<_, Option<i64>>(8)?.map(|d| d as u32),
                        isrc: r.get(9)?,
                    },
                    fetched_at: r.get(10)?,
                })
            })
            .ok()?;
        let now = now();
        for row in rows.flatten() {
            let age = Duration::from_secs(now.saturating_sub(row.fetched_at).max(0) as u64);
            if !row.found {
                if age < NONE_FOR {
                    self.touch(key, row.seconds, now);
                    return Some(Hit::None);
                }
                continue;
            }
            let Some(document) = row.document.as_deref().and_then(|d| serde_json::from_str::<Document>(d).ok()) else {
                continue;
            };
            if document.schema != SCHEMA {
                continue;
            }
            // Lyrics a source gave for its own track are that track's; the
            // others must still fit what is playing.
            if !key.starts_with("source:") && !matching::judge(track, &row.claim, &document).accepted() {
                continue;
            }
            self.touch(key, row.seconds, now);
            let origin = Origin { provider: row.provider.unwrap_or_default(), name: row.name.unwrap_or_default() };
            return Some(Hit::Found { document, origin, stale: age >= FRESH });
        }
        None
    }

    /// Keeps lyrics found for a track by its identity.
    pub fn put(&self, track: &Track, claim: &Claim, document: &Document, origin: &Origin, raw: Option<(&str, &str)>) {
        self.write(&identity(track), track, Some((claim, document, origin)), raw);
    }

    /// Keeps lyrics a source gave for its own track.
    pub fn put_from_source(&self, plugin: &str, reference: &str, track: &Track, document: &Document, origin: &Origin, raw: Option<(&str, &str)>) {
        let claim = Claim {
            title: Some(track.title.clone()),
            artist: Some(track.artist.clone()),
            album: track.album.clone(),
            duration_ms: track.duration_ms,
            isrc: track.isrc.clone(),
        };
        self.write(&source_key(plugin, reference), track, Some((&claim, document, origin)), raw);
    }

    /// Remembers that nobody had lyrics for this track.
    pub fn put_none(&self, track: &Track) {
        self.write(&identity(track), track, None, None);
    }

    /// Remembers that the source had no lyrics for its own track.
    pub fn put_none_from_source(&self, plugin: &str, reference: &str, track: &Track) {
        self.write(&source_key(plugin, reference), track, None, None);
    }

    /// Forgets everything a plugin gave, for when it is removed or its
    /// account is closed.
    pub fn forget_provider(&self, plugin: &str) {
        let _ = self.db.execute("DELETE FROM lyrics WHERE provider = ?1 OR key LIKE ?2", params![plugin, format!("source:{plugin}:%")]);
    }

    fn write(&self, key: &str, track: &Track, found: Option<(&Claim, &Document, &Origin)>, raw: Option<(&str, &str)>) {
        let now = now();
        let document = found.and_then(|(_, d, _)| serde_json::to_string(d).ok());
        let claim = found.map(|(c, _, _)| c.clone()).unwrap_or_default();
        let _ = self.db.execute(
            "INSERT OR REPLACE INTO lyrics
             (key, seconds, found, document, provider, name, raw, format, schema, title, artist, album, duration_ms, isrc, fetched_at, used_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?15)",
            params![
                key,
                seconds_of(track),
                found.is_some() as i64,
                document,
                found.map(|(_, _, o)| o.provider.clone()),
                found.map(|(_, _, o)| o.name.clone()),
                raw.map(|(_, text)| text),
                raw.map(|(format, _)| format),
                SCHEMA as i64,
                claim.title,
                claim.artist,
                claim.album,
                claim.duration_ms.map(|d| d as i64),
                claim.isrc,
                now,
            ],
        );
    }

    fn touch(&self, key: &str, seconds: i64, now: i64) {
        let _ = self.db.execute("UPDATE lyrics SET used_at = ?3 WHERE key = ?1 AND seconds = ?2", params![key, seconds, now]);
    }

    fn forget_unused(&self) -> rusqlite::Result<()> {
        let before = now() - FORGET_AFTER.as_secs() as i64;
        self.db.execute("DELETE FROM lyrics WHERE used_at < ?1", params![before])?;
        Ok(())
    }

    /// How many rows there are, for tests.
    #[cfg(test)]
    fn rows(&self) -> i64 {
        self.db.query_row("SELECT count(*) FROM lyrics", [], |r| r.get(0)).unwrap()
    }

    /// Moves a row's dates into the past, for tests.
    #[cfg(test)]
    fn age(&self, by: Duration) {
        let by = by.as_secs() as i64;
        self.db.execute("UPDATE lyrics SET fetched_at = fetched_at - ?1, used_at = used_at - ?1", params![by]).unwrap();
    }
}

struct Row {
    seconds: i64,
    found: bool,
    document: Option<String>,
    provider: Option<String>,
    name: Option<String>,
    claim: Claim,
    fetched_at: i64,
}

/// The key for a track by what it is.
fn identity(track: &Track) -> String {
    if let Some(isrc) = track.isrc.as_deref().map(str::trim).filter(|i| !i.is_empty()) {
        return format!("isrc:{}", isrc.to_ascii_uppercase());
    }
    format!("song:{}\u{1f}{}", matching::fold(&track.artist), matching::fold(&track.title))
}

fn source_key(plugin: &str, reference: &str) -> String {
    format!("source:{plugin}:{reference}")
}

/// The length in whole seconds. An unknown length is far below zero, so rows
/// for unknown lengths only meet each other.
fn seconds_of(track: &Track) -> i64 {
    track.duration_ms.map(|d| ((d + 500) / 1_000) as i64).unwrap_or(-1_000)
}

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lrc;

    fn track() -> Track {
        Track { title: "Wasted Years".into(), artist: "Iron Maiden".into(), album: None, duration_ms: Some(306_000), isrc: None, mbid: None }
    }

    fn claim() -> Claim {
        Claim { title: Some("Wasted Years".into()), artist: Some("Iron Maiden".into()), album: None, duration_ms: Some(306_200), isrc: None }
    }

    fn origin() -> Origin {
        Origin { provider: "app.chilpa.lrclib".into(), name: "LRCLIB".into() }
    }

    #[test]
    fn found_lyrics_come_back() {
        let cache = Cache::in_memory().unwrap();
        let doc = lrc::parse("[00:10.00]From the coast of gold\n");
        cache.put(&track(), &claim(), &doc, &origin(), Some(("lrc", "[00:10.00]From the coast of gold")));
        match cache.get(&track()) {
            Some(Hit::Found { document, origin: o, stale }) => {
                assert_eq!(document, doc);
                assert_eq!(o, origin());
                assert!(!stale);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn another_length_is_another_row() {
        let cache = Cache::in_memory().unwrap();
        let doc = lrc::parse("[00:10.00]From the coast of gold\n");
        cache.put(&track(), &claim(), &doc, &origin(), None);
        let mut live = track();
        live.duration_ms = Some(420_000);
        assert!(cache.get(&live).is_none(), "the live take does not get the studio lyrics");
        let mut close = track();
        close.duration_ms = Some(307_400);
        assert!(cache.get(&close).is_some(), "a second off is the same recording");
    }

    #[test]
    fn a_row_that_no_longer_fits_is_not_served() {
        let cache = Cache::in_memory().unwrap();
        let doc = lrc::parse("[00:10.00]Words\n");
        let wrong = Claim { title: Some("Another Song".into()), ..claim() };
        cache.put(&track(), &wrong, &doc, &origin(), None);
        assert!(cache.get(&track()).is_none(), "what was found for another song is not this one's");
    }

    #[test]
    fn none_is_believed_for_a_while() {
        let cache = Cache::in_memory().unwrap();
        cache.put_none(&track());
        assert_eq!(cache.get(&track()), Some(Hit::None));
        cache.age(NONE_FOR + Duration::from_secs(1));
        assert_eq!(cache.get(&track()), None, "after a week it is asked again");
    }

    #[test]
    fn old_lyrics_are_served_and_marked_stale() {
        let cache = Cache::in_memory().unwrap();
        cache.put(&track(), &claim(), &lrc::parse("[00:10.00]Words\n"), &origin(), None);
        cache.age(FRESH + Duration::from_secs(1));
        assert!(matches!(cache.get(&track()), Some(Hit::Found { stale: true, .. })));
    }

    #[test]
    fn the_isrc_is_the_key_when_there_is_one() {
        let cache = Cache::in_memory().unwrap();
        let mut t = track();
        t.isrc = Some("gbaye8500152".into());
        let mut c = claim();
        c.isrc = Some("GBAYE8500152".into());
        cache.put(&t, &c, &lrc::parse("[00:10.00]Words\n"), &origin(), None);
        let mut renamed = t.clone();
        renamed.title = "Wasted Years (Remastered)".into();
        assert!(cache.get(&renamed).is_some(), "the same recording under another name");
    }

    #[test]
    fn what_a_source_gave_is_kept_apart() {
        let cache = Cache::in_memory().unwrap();
        let doc = lrc::parse("[00:10.00]Words\n");
        let from = Origin { provider: "example-source".into(), name: "Example".into() };
        cache.put_from_source("example-source", "track:42", &track(), &doc, &from, None);
        assert!(cache.get(&track()).is_none(), "it is not served to other tracks by name");
        assert!(cache.get_from_source("example-source", "track:42", &track()).is_some());
        cache.forget_provider("example-source");
        assert_eq!(cache.rows(), 0, "removing the plugin removes what it gave");
    }

    #[test]
    fn rows_nobody_reads_are_forgotten() {
        let dir = std::env::temp_dir().join(format!("chilpa-lyrics-cache-{}", std::process::id()));
        let path = dir.join("lyrics.db");
        let _ = std::fs::remove_dir_all(&dir);
        {
            let cache = Cache::open(&path).unwrap();
            cache.put_none(&track());
            cache.age(FORGET_AFTER + Duration::from_secs(1));
        }
        let cache = Cache::open(&path).unwrap();
        assert_eq!(cache.rows(), 0);
        let _ = std::fs::remove_dir_all(dir);
    }
}
