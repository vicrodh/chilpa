//! Bounded selections and durable ownership of explicitly downloaded audio.

use super::Library;
use rusqlite::{params, OptionalExtension};
use sha2::{Digest, Sha256};

pub const MAX_TRACKS: usize = 10_000;
pub const MAX_SELECTION_BYTES: usize = 8 * 1024 * 1024;
/// The most copies the offline page lists at once.
pub const MAX_LISTED: usize = 20_000;

pub fn revision_digest(revision: Option<&str>) -> String {
    format!("{:x}", Sha256::digest(revision.unwrap_or("").as_bytes()))
}

pub(super) fn register(db: &rusqlite::Connection) -> rusqlite::Result<()> {
    let flags = rusqlite::functions::FunctionFlags::SQLITE_UTF8
        | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC;
    db.create_scalar_function("chilpa_revision", 1, flags, |context| {
        Ok(revision_digest(
            context.get::<Option<String>>(0)?.as_deref(),
        ))
    })
}

pub(super) const READY_COPY: &str = "EXISTS (SELECT 1 FROM offline_audio o JOIN servers v ON v.source_id=o.source_id
    WHERE o.source_id=tracks.source_id AND o.entry=tracks.external_id AND o.account=v.account
    AND o.identity=v.identity AND o.state='complete' AND o.revision=chilpa_revision(tracks.revision))";

#[derive(Clone, Debug)]
pub struct Track {
    pub source: i64,
    pub entry: String,
    pub revision: Option<String>,
    pub codec: Option<String>,
    pub title: String,
}

#[derive(Clone, Debug)]
pub struct Copy {
    pub key: String,
    pub source: i64,
    pub account: String,
    pub identity: String,
    pub entry: String,
    /// A digest of the catalogue revision, which may itself contain a URL.
    pub revision: String,
    pub path: String,
    pub bytes: u64,
    pub checksum: String,
    pub content_type: String,
    pub codec: String,
    pub extension: String,
    pub validator: String,
    pub state: String,
    pub error: String,
}

/// What one server keeps offline.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceUsage {
    pub source: i64,
    pub files: usize,
    pub bytes: u64,
}

/// A copy as the offline page lists it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Listed {
    pub key: String,
    pub source: i64,
    pub bytes: u64,
    pub state: String,
    pub error: String,
    pub changed_at: i64,
    /// The library's track, or 0 when it left the library.
    pub track: i64,
    pub title: String,
    pub artist: String,
    pub duration_ms: u64,
    pub disc: i64,
    pub number: i64,
    pub album: i64,
    pub album_title: String,
    pub album_artist: String,
    pub artwork: String,
}

impl Copy {
    pub fn retained_bytes(&self) -> usize {
        [
            &self.key,
            &self.account,
            &self.identity,
            &self.entry,
            &self.revision,
            &self.path,
            &self.checksum,
            &self.content_type,
            &self.validator,
            &self.state,
            &self.error,
            &self.codec,
            &self.extension,
        ]
        .iter()
        .map(|value| value.capacity())
        .sum::<usize>()
            + std::mem::size_of::<Self>() * 2
    }
}

fn copy(row: &rusqlite::Row<'_>) -> rusqlite::Result<Copy> {
    Ok(Copy {
        key: row.get(0)?,
        source: row.get(1)?,
        account: row.get(2)?,
        identity: row.get(3)?,
        entry: row.get(4)?,
        revision: row.get(5)?,
        path: row.get(6)?,
        bytes: row.get::<_, i64>(7)?.max(0) as u64,
        checksum: row.get(8)?,
        content_type: row.get(9)?,
        validator: row.get(10)?,
        state: row.get(11)?,
        error: row.get(12)?,
        codec: row.get(13)?,
        extension: row.get(14)?,
    })
}

const COLUMNS: &str = "key,source_id,account,identity,entry,revision,path,bytes,checksum,content_type,validator,state,error,codec,extension";

fn bounded_copies(
    rows: impl Iterator<Item = rusqlite::Result<Copy>>,
    page: bool,
) -> rusqlite::Result<Vec<Copy>> {
    let mut copies = Vec::new();
    let mut bytes = 0usize;
    for row in rows {
        let item = row?;
        bytes = bytes.saturating_add(item.retained_bytes());
        if bytes > MAX_SELECTION_BYTES {
            if page && !copies.is_empty() {
                break;
            }
            return Err(rusqlite::Error::InvalidQuery);
        }
        copies.push(item);
    }
    Ok(copies)
}

impl Library {
    /// Only server tracks participate. A mixed playlist keeps its original
    /// order and reports its eligible subset separately in the facade.
    pub fn offline_selection(&self, kind: &str, ids: &[i64]) -> rusqlite::Result<Vec<Track>> {
        if ids.is_empty() || ids.len() > MAX_TRACKS {
            return Ok(Vec::new());
        }
        let predicate = match kind {
            "tracks" => "t.id",
            "album" => "t.album_id",
            "playlist" => "t.id IN (SELECT track_id FROM playlist_tracks WHERE playlist_id",
            _ => return Ok(Vec::new()),
        };
        let marks = vec!["?"; ids.len()].join(",");
        let selection = if kind == "playlist" {
            format!("{predicate} IN ({marks}))")
        } else {
            format!("{predicate} IN ({marks})")
        };
        let sql = format!(
            "SELECT t.source_id,t.external_id,t.revision,t.codec,t.title FROM tracks t
            JOIN sources s ON s.id=t.source_id JOIN servers v ON v.source_id=t.source_id
            WHERE s.enabled=1 AND t.present=1 AND {selection}
            ORDER BY t.album_id,t.disc_number,t.track_number,t.id LIMIT {}",
            MAX_TRACKS + 1
        );
        let mut statement = self.db.prepare(&sql)?;
        let rows = statement.query_map(rusqlite::params_from_iter(ids), |row| {
            Ok(Track {
                source: row.get(0)?,
                entry: row.get(1)?,
                revision: row.get(2)?,
                codec: row.get(3)?,
                title: row.get(4)?,
            })
        })?;
        let mut tracks = Vec::new();
        let mut bytes = 0usize;
        for row in rows {
            let track = row?;
            bytes = bytes.saturating_add(
                track.entry.len()
                    + track.title.len()
                    + track.revision.as_ref().map_or(0, String::len)
                    + track.codec.as_ref().map_or(0, String::len),
            );
            if bytes > MAX_SELECTION_BYTES {
                return Err(rusqlite::Error::InvalidQuery);
            }
            tracks.push(track);
        }
        Ok(tracks)
    }

    pub fn offline_copy(
        &self,
        source: i64,
        account: &str,
        identity: &str,
        entry: &str,
    ) -> rusqlite::Result<Option<Copy>> {
        let sql = format!("SELECT {COLUMNS} FROM offline_audio o WHERE source_id=?1 AND account=?2 AND identity=?3
            AND entry=?4 AND profile='original' AND state='complete' AND EXISTS (SELECT 1 FROM tracks t
                WHERE t.source_id=o.source_id AND t.external_id=o.entry AND o.revision=chilpa_revision(t.revision))
            ORDER BY changed_at DESC,key LIMIT 1");
        self.db
            .query_row(&sql, params![source, account, identity, entry], copy)
            .optional()
    }

    pub fn offline_copy_by_key(&self, key: &str) -> rusqlite::Result<Option<Copy>> {
        self.db
            .query_row(
                &format!("SELECT {COLUMNS} FROM offline_audio WHERE key=?1"),
                [key],
                copy,
            )
            .optional()
    }

    pub fn offline_copies_for(
        &self,
        source: i64,
        account: &str,
        identity: &str,
        entry: &str,
    ) -> rusqlite::Result<Vec<Copy>> {
        let mut statement = self.db.prepare(&format!(
            "SELECT {COLUMNS} FROM offline_audio WHERE source_id=?1
            AND account=?2 AND identity=?3 AND entry=?4 LIMIT {}",
            MAX_TRACKS + 1
        ))?;
        let rows = statement.query_map(params![source, account, identity, entry], copy)?;
        bounded_copies(rows, false)
    }

    pub fn offline_all_copies(&self) -> rusqlite::Result<Vec<Copy>> {
        let mut statement = self.db.prepare(&format!(
            "SELECT {COLUMNS} FROM offline_audio ORDER BY key LIMIT {}",
            MAX_TRACKS + 1
        ))?;
        let rows = statement.query_map([], copy)?;
        bounded_copies(rows, false)
    }

    pub fn offline_copy_page(&self, after: &str) -> rusqlite::Result<Vec<Copy>> {
        let mut statement = self.db.prepare(&format!(
            "SELECT {COLUMNS} FROM offline_audio WHERE key>?1 ORDER BY key LIMIT 500"
        ))?;
        let rows = statement.query_map([after], copy)?;
        bounded_copies(rows, true)
    }

    pub fn offline_current_copy_page(&self, after: &str) -> rusqlite::Result<Vec<Copy>> {
        let columns = COLUMNS
            .split(',')
            .map(|column| format!("o.{column}"))
            .collect::<Vec<_>>()
            .join(",");
        let sql=format!("SELECT {columns} FROM offline_audio o JOIN servers v ON v.source_id=o.source_id
            AND v.account=o.account AND v.identity=o.identity JOIN sources s ON s.id=v.source_id AND s.enabled=1
            JOIN tracks t ON t.source_id=o.source_id AND t.external_id=o.entry AND t.present=1
            WHERE o.key>?1 AND o.state='complete' AND o.revision=chilpa_revision(t.revision) ORDER BY o.key LIMIT 500");
        let mut statement = self.db.prepare(&sql)?;
        let rows = statement.query_map([after], copy)?;
        bounded_copies(rows, true)
    }

    /// A deletion claims only the exact file shown by its confirmation.
    pub fn begin_offline_removal(&self, item: &Copy) -> rusqlite::Result<bool> {
        Ok(self.db.execute("UPDATE offline_audio SET state='removing' WHERE key=?1 AND path=?2 AND checksum=?3 AND bytes=?4",
            params![item.key,item.path,item.checksum,i64::try_from(item.bytes).map_err(|_| rusqlite::Error::InvalidQuery)?])?>0)
    }

    pub fn finish_offline_removal(&self, item: &Copy) -> rusqlite::Result<()> {
        let tx = self.own_transaction()?;
        tx.execute("DELETE FROM offline_collections WHERE key=?1", [&item.key])?;
        tx.execute(
            "DELETE FROM offline_audio WHERE key=?1 AND state='removing'",
            [&item.key],
        )?;
        tx.execute(
            &format!(
                "UPDATE tracks SET reachable=coalesce((SELECT s.enabled AND (s.reachable OR {})
            FROM sources s WHERE s.id=tracks.source_id),0) WHERE source_id=?1 AND external_id=?2",
                READY_COPY
            ),
            params![item.source, item.entry],
        )?;
        tx.finish()
    }

    pub fn offline_bytes(&self) -> rusqlite::Result<u64> {
        self.db
            .query_row(
                "SELECT coalesce(sum(bytes),0) FROM offline_audio",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|bytes| bytes.max(0) as u64)
    }

    pub fn offline_copy_count(&self) -> rusqlite::Result<usize> {
        self.db
            .query_row("SELECT count(*) FROM offline_audio", [], |row| {
                row.get::<_, i64>(0)
            })
            .map(|count| count.max(0) as usize)
    }

    /// An existing complete copy remains pinned and is never overwritten.
    /// Interrupted reservations retain their paths for explicit recovery.
    pub fn queue_offline_copy(&self, item: &Copy, collection: &str) -> rusqlite::Result<()> {
        let tx = self.own_transaction()?;
        tx.execute(
            "INSERT INTO offline_audio (key,source_id,account,identity,entry,revision,path)
            VALUES (?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(key) DO NOTHING",
            params![
                item.key,
                item.source,
                item.account,
                item.identity,
                item.entry,
                item.revision,
                item.path
            ],
        )?;
        tx.execute(
            "INSERT OR IGNORE INTO offline_collections (collection,key) VALUES (?1,?2)",
            params![collection, item.key],
        )?;
        tx.finish()
    }

    /// The bytes one transfer may write, at most `maximum`; zero refuses it.
    /// Under a quota the reservation is recorded, so final files and active
    /// transfers share it. Without one nothing is reserved and nothing is
    /// refused: the disk is the limit.
    pub fn reserve_offline_copy(
        &self,
        key: &str,
        maximum: u64,
        quota: Option<u64>,
    ) -> rusqlite::Result<u64> {
        let tx = self.own_transaction()?;
        let (state, previous): (String, i64) = tx.query_row(
            "SELECT state,bytes FROM offline_audio WHERE key=?1",
            [key],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if state == "complete" {
            return Ok(0);
        }
        let (budget, recorded) = match quota {
            Some(quota) => {
                let used: i64 = tx.query_row(
                    "SELECT coalesce(sum(bytes),0) FROM offline_audio",
                    [],
                    |row| row.get(0),
                )?;
                let reservation = quota
                    .saturating_sub(used.saturating_sub(previous).max(0) as u64)
                    .min(maximum);
                (reservation, reservation)
            }
            None => (maximum, 0),
        };
        tx.execute(
            "UPDATE offline_audio SET bytes=?2,state='downloading',error='' WHERE key=?1",
            params![
                key,
                i64::try_from(recorded).map_err(|_| rusqlite::Error::InvalidQuery)?
            ],
        )?;
        tx.finish()?;
        Ok(budget)
    }

    /// Files and bytes kept offline for each server, reservations included.
    pub fn offline_usage_by_source(&self) -> rusqlite::Result<Vec<SourceUsage>> {
        let mut statement = self.db.prepare(
            "SELECT source_id,count(*),coalesce(sum(bytes),0) FROM offline_audio
            GROUP BY source_id ORDER BY source_id LIMIT 1024",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(SourceUsage {
                source: row.get(0)?,
                files: row.get::<_, i64>(1)?.max(0) as usize,
                bytes: row.get::<_, i64>(2)?.max(0) as u64,
            })
        })?;
        rows.collect()
    }

    /// Every copy with the track and record it belongs to, for the offline
    /// page. A copy whose track left the library keeps its row, with an
    /// empty title and track id 0. At most `MAX_LISTED + 1` rows, so the
    /// caller can tell that more exist.
    pub fn offline_listing(&self) -> rusqlite::Result<Vec<Listed>> {
        let mut statement = self.db.prepare(&format!(
            "SELECT o.key,o.source_id,o.bytes,o.state,o.error,o.changed_at,coalesce(t.id,0),coalesce(t.title,''),
            coalesce(t.artist,''),coalesce(t.duration_ms,0),coalesce(t.disc_number,0),coalesce(t.track_number,0),
            coalesce(t.album_id,0),coalesce(a.title,''),coalesce(a.album_artist,''),coalesce(a.artwork_path,'')
            FROM offline_audio o LEFT JOIN tracks t ON t.source_id=o.source_id AND t.external_id=o.entry
            LEFT JOIN albums a ON a.id=t.album_id ORDER BY o.key LIMIT {}",
            MAX_LISTED + 1
        ))?;
        let rows = statement.query_map([], |row| {
            Ok(Listed {
                key: row.get(0)?,
                source: row.get(1)?,
                bytes: row.get::<_, i64>(2)?.max(0) as u64,
                state: row.get(3)?,
                error: row.get(4)?,
                changed_at: row.get(5)?,
                track: row.get(6)?,
                title: row.get(7)?,
                artist: row.get(8)?,
                duration_ms: row.get::<_, i64>(9)?.max(0) as u64,
                disc: row.get(10)?,
                number: row.get(11)?,
                album: row.get(12)?,
                album_title: row.get(13)?,
                album_artist: row.get(14)?,
                artwork: row.get(15)?,
            })
        })?;
        rows.collect()
    }

    /// Where a server's next copies go: a folder URL, or empty for the
    /// profile's offline folder. Copies already saved keep their paths.
    pub fn set_server_offline_folder(&self, source_id: i64, folder: &str) -> rusqlite::Result<bool> {
        Ok(self.db.execute(
            "UPDATE servers SET offline_folder=?2 WHERE source_id=?1",
            params![source_id, folder],
        )? > 0)
    }

    /// Called after file promotion. The account and track identity must
    /// still be configured, including a connection edit during transfer.
    pub fn complete_offline_copy(&self, item: &Copy, session: i64) -> rusqlite::Result<bool> {
        let tx = self.own_transaction()?;
        let changed = tx.execute(
            "UPDATE offline_audio SET bytes=?2,checksum=?3,content_type=?4,validator=?5,codec=?7,extension=?8,
            state='complete',error='',changed_at=unixepoch() WHERE key=?1 AND EXISTS (
                SELECT 1 FROM servers v JOIN sources s ON s.id=v.source_id
                WHERE v.source_id=offline_audio.source_id AND v.account=offline_audio.account
                AND v.identity=offline_audio.identity AND v.session_revision=?6 AND s.enabled=1)
                AND EXISTS (SELECT 1 FROM tracks t WHERE t.source_id=offline_audio.source_id
                    AND t.external_id=offline_audio.entry AND t.present=1 AND offline_audio.revision=chilpa_revision(t.revision))",
            params![
                item.key,
                i64::try_from(item.bytes).map_err(|_| rusqlite::Error::InvalidQuery)?,
                item.checksum,
                item.content_type,
                item.validator,
                session,
                item.codec,
                item.extension
            ],
        )?;
        if changed > 0 {
            tx.execute(
                "UPDATE tracks SET reachable=1 WHERE source_id=?1 AND external_id=?2 AND present=1",
                params![item.source, item.entry],
            )?;
        }
        tx.finish()?;
        Ok(changed > 0)
    }

    /// Retain the actual bytes when a final file exists but its promotion
    /// could not be committed. Recovery accounts for them before retrying.
    pub fn fail_offline_copy(&self, key: &str, error: &str, bytes: u64) -> rusqlite::Result<()> {
        self.db.execute("UPDATE offline_audio SET state='failed',error=?2,bytes=?3 WHERE key=?1 AND state<>'complete'",params![key,error,i64::try_from(bytes).map_err(|_| rusqlite::Error::InvalidQuery)?])?;
        Ok(())
    }

    pub fn invalidate_offline_copy(
        &self,
        item: &Copy,
        error: &str,
        bytes: u64,
    ) -> rusqlite::Result<()> {
        let tx = self.own_transaction()?;
        tx.execute(
            "UPDATE offline_audio SET state='failed',error=?2,bytes=?3 WHERE key=?1",
            params![
                item.key,
                error,
                i64::try_from(bytes).map_err(|_| rusqlite::Error::InvalidQuery)?
            ],
        )?;
        tx.execute(
            &format!(
                "UPDATE tracks SET reachable=coalesce((SELECT s.enabled AND (s.reachable OR {})
            FROM sources s WHERE s.id=tracks.source_id),0) WHERE source_id=?1 AND external_id=?2",
                READY_COPY
            ),
            params![item.source, item.entry],
        )?;
        tx.finish()
    }

    pub fn invalidate_checked_offline_copy(
        &self,
        item: &Copy,
        bytes: u64,
    ) -> rusqlite::Result<bool> {
        let tx = self.own_transaction()?;
        let changed=tx.execute("UPDATE offline_audio SET state='failed',bytes=?5 WHERE key=?1 AND path=?2 AND checksum=?3
            AND bytes=?4 AND state='complete'",params![item.key,item.path,item.checksum,i64::try_from(item.bytes).map_err(|_| rusqlite::Error::InvalidQuery)?,
                i64::try_from(bytes).map_err(|_| rusqlite::Error::InvalidQuery)?])?;
        if changed > 0 {
            tx.execute(&format!("UPDATE tracks SET reachable=coalesce((SELECT s.enabled AND (s.reachable OR {})
                FROM sources s WHERE s.id=tracks.source_id),0) WHERE source_id=?1 AND external_id=?2",READY_COPY),params![item.source,item.entry])?;
        }
        tx.finish()?;
        Ok(changed > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NewServer, ServerTrack};

    fn fixture() -> (Library, i64, Copy) {
        let lib = Library::in_memory().unwrap();
        let source = lib
            .add_server(&NewServer {
                provider: "subsonic".into(),
                instance: "fixture".into(),
                account: "first".into(),
                identity: "physical-server".into(),
                libraries: "[]".into(),
                details: "{}".into(),
                ..Default::default()
            })
            .unwrap();
        let generation = lib.begin_server_sync(source).unwrap();
        lib.save_server_tracks(
            source,
            generation,
            &[ServerTrack {
                external_id: "remote:subsonic/fixture/song".into(),
                title: "Song".into(),
                album: "Album".into(),
                album_key: "album".into(),
                ..Default::default()
            }],
        )
        .unwrap();
        lib.finish_server_sync(source, generation).unwrap();
        let item = Copy {
            key: "digest".into(),
            source,
            account: "first".into(),
            identity: "physical-server".into(),
            entry: "remote:subsonic/fixture/song".into(),
            revision: revision_digest(None),
            path: "managed.flac".into(),
            bytes: 4,
            checksum: "hash".into(),
            content_type: "audio/flac".into(),
            codec: "flac".into(),
            extension: "flac".into(),
            validator: String::new(),
            state: "queued".into(),
            error: String::new(),
        };
        (lib, source, item)
    }

    const QUOTA_BYTES: u64 = 2 * 1024 * 1024 * 1024;

    #[test]
    fn reservations_and_complete_files_share_one_quota_and_accounts_never_share_copies() {
        let (lib, source, item) = fixture();
        lib.queue_offline_copy(&item, "album/1").unwrap();
        assert_eq!(
            lib.reserve_offline_copy(&item.key, QUOTA_BYTES, Some(QUOTA_BYTES)).unwrap(),
            QUOTA_BYTES
        );
        assert_eq!(lib.offline_bytes().unwrap(), QUOTA_BYTES);
        let mut other = item.clone();
        other.key = "other".into();
        other.revision = "new".into();
        lib.queue_offline_copy(&other, "album/1").unwrap();
        assert_eq!(lib.reserve_offline_copy(&other.key, 1, Some(QUOTA_BYTES)).unwrap(), 0);
        assert!(!lib.complete_offline_copy(&item, 999).unwrap());
        let revision = lib.server(source).unwrap().unwrap().session_revision;
        assert!(lib.complete_offline_copy(&item, revision).unwrap());
        assert_eq!(lib.offline_bytes().unwrap(), 4);
        assert!(lib
            .offline_copy(source, "other", "physical-server", &item.entry)
            .unwrap()
            .is_none());
        assert!(lib
            .offline_copy(source, "first", "different-server", &item.entry)
            .unwrap()
            .is_none());
        assert_eq!(
            lib.offline_copy(source, "first", "physical-server", &item.entry)
                .unwrap()
                .unwrap()
                .checksum,
            "hash"
        );
        assert_eq!(lib.reserve_offline_copy(&item.key, 100, Some(QUOTA_BYTES)).unwrap(), 0);
    }

    #[test]
    fn a_quota_counts_every_step_and_no_quota_reserves_nothing_and_refuses_nothing() {
        let (lib, _, item) = fixture();
        let quota = 1000;
        lib.queue_offline_copy(&item, "album/1").unwrap();
        assert_eq!(lib.reserve_offline_copy(&item.key, 600, Some(quota)).unwrap(), 600);
        let mut second = item.clone();
        second.key = "second".into();
        second.revision = "second".into();
        lib.queue_offline_copy(&second, "album/1").unwrap();
        assert_eq!(lib.reserve_offline_copy(&second.key, 600, Some(quota)).unwrap(), 400, "what the first left");
        assert_eq!(lib.offline_bytes().unwrap(), 1000);
        let mut third = item.clone();
        third.key = "third".into();
        third.revision = "third".into();
        lib.queue_offline_copy(&third, "album/1").unwrap();
        assert_eq!(lib.reserve_offline_copy(&third.key, 600, Some(quota)).unwrap(), 0, "full");
        assert_eq!(lib.reserve_offline_copy(&third.key, 600, Some(500)).unwrap(), 0, "a quota lowered under the usage refuses");
        assert_eq!(lib.offline_bytes().unwrap(), 1000, "and removes nothing");
        assert_eq!(lib.reserve_offline_copy(&third.key, u64::MAX / 2, None).unwrap(), u64::MAX / 2, "no quota, the whole budget");
        assert_eq!(lib.offline_bytes().unwrap(), 1000, "and nothing recorded for it");
        assert_eq!(lib.offline_usage_by_source().unwrap(), [SourceUsage { source: item.source, files: 3, bytes: 1000 }]);
    }

    #[test]
    fn the_listing_names_the_track_and_record_and_keeps_copies_that_left_the_library() {
        let (lib, source, item) = fixture();
        lib.queue_offline_copy(&item, "album/1").unwrap();
        let mut gone = item.clone();
        gone.key = "gone".into();
        gone.entry = "remote:subsonic/fixture/removed".into();
        lib.queue_offline_copy(&gone, "album/1").unwrap();
        let listed = lib.offline_listing().unwrap();
        assert_eq!(listed.len(), 2);
        let found = listed.iter().find(|row| row.key == item.key).unwrap();
        assert!(found.track > 0 && found.album > 0);
        assert_eq!((found.title.as_str(), found.album_title.as_str(), found.state.as_str()), ("Song", "Album", "queued"));
        let left = listed.iter().find(|row| row.key == "gone").unwrap();
        assert_eq!((left.track, left.title.as_str(), left.source), (0, "", source));
    }

    #[test]
    fn a_server_folder_changes_alone() {
        let (lib, source, _) = fixture();
        let before = lib.server(source).unwrap().unwrap();
        assert!(lib.set_server_offline_folder(source, "file:///music/offline").unwrap());
        let after = lib.server(source).unwrap().unwrap();
        assert_eq!(after.offline_folder, "file:///music/offline");
        assert_eq!(after.session_revision, before.session_revision, "no job is invalidated");
        assert!(!lib.set_server_offline_folder(source + 100, "").unwrap());
    }

    #[test]
    fn downloaded_tracks_stay_playable_when_the_server_stops_answering_but_respect_disable() {
        let (lib, source, item) = fixture();
        lib.queue_offline_copy(&item, "track/1").unwrap();
        let revision = lib.server(source).unwrap().unwrap().session_revision;
        assert!(lib.complete_offline_copy(&item, revision).unwrap());
        lib.set_source_reachable(source, false).unwrap();
        assert!(lib
            .unplayable_entries(&[item.entry.clone()])
            .unwrap()
            .is_empty());
        lib.set_server_enabled(source, false).unwrap();
        assert!(lib
            .unplayable_entries(&[item.entry.clone()])
            .unwrap()
            .contains(&item.entry));
        lib.set_server_enabled(source, true).unwrap();
        assert!(lib
            .unplayable_entries(&[item.entry.clone()])
            .unwrap()
            .is_empty());
    }

    #[test]
    fn copies_are_selected_and_advertised_only_for_the_current_catalogue_revision() {
        let (lib, source, first) = fixture();
        let session = lib.server(source).unwrap().unwrap().session_revision;
        lib.queue_offline_copy(&first, "album/1").unwrap();
        assert!(lib.complete_offline_copy(&first, session).unwrap());
        lib.db
            .execute(
                "UPDATE tracks SET revision='new-revision' WHERE source_id=?1",
                [source],
            )
            .unwrap();
        lib.set_source_reachable(source, false).unwrap();
        assert!(lib
            .unplayable_entries(&[first.entry.clone()])
            .unwrap()
            .contains(&first.entry));
        assert!(lib
            .offline_copy(source, &first.account, &first.identity, &first.entry)
            .unwrap()
            .is_none());
        assert!(!lib.complete_offline_copy(&first, session).unwrap());
        let mut second = first.clone();
        second.key = "second".into();
        second.revision = revision_digest(Some("new-revision"));
        lib.queue_offline_copy(&second, "album/1").unwrap();
        assert!(lib.complete_offline_copy(&second, session).unwrap());
        lib.db
            .execute(
                "UPDATE offline_audio SET changed_at=changed_at+100 WHERE key='second'",
                [],
            )
            .unwrap();
        lib.db
            .execute(
                "UPDATE tracks SET revision=NULL WHERE source_id=?1",
                [source],
            )
            .unwrap();
        let found = lib
            .offline_copy(source, &first.account, &first.identity, &first.entry)
            .unwrap()
            .unwrap();
        assert_eq!(
            found.key, first.key,
            "revision matching happens before the latest-copy limit"
        );
        lib.set_source_reachable(source, false).unwrap();
        assert!(lib.unplayable_entries(&[first.entry]).unwrap().is_empty());
    }
}
