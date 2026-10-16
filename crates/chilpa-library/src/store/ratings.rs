//! I store personal stars against library identities, not paths or normalized titles.
//! Source adapters resolve their (source instance, external id) to these ids.
//! A remote rating is separate metadata; I never imply a server write succeeded.

use super::Library;
use rusqlite::{params, Connection};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RatingTarget {
    Track(i64),
    Album(i64),
    Playlist(i64),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ratings {
    pub tracks: BTreeMap<i64, u8>,
    pub albums: BTreeMap<i64, u8>,
    pub playlists: BTreeMap<i64, u8>,
}

impl Ratings {
    pub fn get(&self, target: RatingTarget) -> u8 {
        match target {
            RatingTarget::Track(id) => self.tracks.get(&id),
            RatingTarget::Album(id) => self.albums.get(&id),
            RatingTarget::Playlist(id) => self.playlists.get(&id),
        }
        .copied()
        .unwrap_or(0)
    }
}

impl Library {
    pub fn ratings(&self) -> rusqlite::Result<Ratings> {
        fn read(db: &Connection, sql: &str) -> rusqlite::Result<BTreeMap<i64, u8>> {
            db.prepare(sql)?
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect()
        }
        Ok(Ratings {
            tracks: read(
                &self.db,
                "SELECT track_id, stars FROM track_ratings WHERE stars > 0",
            )?,
            albums: read(
                &self.db,
                "SELECT album_id, stars FROM album_ratings WHERE stars > 0",
            )?,
            playlists: read(
                &self.db,
                "SELECT playlist_id, stars FROM playlist_ratings WHERE stars > 0",
            )?,
        })
    }

    /// Zero is a persisted clear, so a merge cannot resurrect an older rating.
    pub fn rate(&self, target: RatingTarget, stars: u8) -> rusqlite::Result<()> {
        if stars > 5 {
            return Err(rusqlite::Error::InvalidParameterName(
                "stars must be between 0 and 5".into(),
            ));
        }
        let tx = self.own_transaction()?;
        let (table, key, id) = match target {
            RatingTarget::Track(id) => ("track_ratings", "track_id", id),
            RatingTarget::Album(id) => ("album_ratings", "album_id", id),
            RatingTarget::Playlist(id) => ("playlist_ratings", "playlist_id", id),
        };
        tx.execute(
            "UPDATE rating_clock SET revision = revision + 1 WHERE id = 1",
            [],
        )?;
        tx.execute(
            &format!(
                "INSERT INTO {table} ({key}, stars, revision, updated_at)
            VALUES (?1, ?2, (SELECT revision FROM rating_clock WHERE id = 1), unixepoch())
            ON CONFLICT({key}) DO UPDATE SET stars = excluded.stars,
                revision = excluded.revision, updated_at = excluded.updated_at"
            ),
            params![id, stars],
        )?;
        tx.finish()
    }
}

/// I resolve merge conflicts by the last explicit choice, including a clear.
pub(crate) fn merge_album_rating(db: &Connection, from: i64, to: i64) -> rusqlite::Result<usize> {
    db.execute(
        "INSERT INTO album_ratings(album_id, stars, revision, updated_at)
        SELECT ?2, stars, revision, updated_at FROM album_ratings WHERE album_id = ?1
        ON CONFLICT(album_id) DO UPDATE SET stars = excluded.stars,
            revision = excluded.revision, updated_at = excluded.updated_at
        WHERE excluded.revision > album_ratings.revision",
        params![from, to],
    )
}

#[cfg(test)]
mod tests;
