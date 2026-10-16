//! The listener's favorites: tracks, records, playlists and artists.
//!
//! A favorite is a mark of mine, not a fact about what it marks, and it goes
//! away with it. An artist is marked by name, folded to lower case: the
//! pages show one artist for a name written with other capitals.

use super::{now_unix, Library};
use rusqlite::params;
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FavoriteTarget<'a> {
    Track(i64),
    Album(i64),
    Playlist(i64),
    Artist(&'a str),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Favorites {
    pub tracks: HashSet<i64>,
    pub albums: HashSet<i64>,
    pub playlists: HashSet<i64>,
    /// Names folded to lower case.
    pub artists: HashSet<String>,
}

impl Favorites {
    pub fn has(&self, target: FavoriteTarget) -> bool {
        match target {
            FavoriteTarget::Track(id) => self.tracks.contains(&id),
            FavoriteTarget::Album(id) => self.albums.contains(&id),
            FavoriteTarget::Playlist(id) => self.playlists.contains(&id),
            FavoriteTarget::Artist(name) => self.artists.contains(&folded(name)),
        }
    }
}

pub(crate) fn folded(name: &str) -> String {
    name.trim().to_lowercase()
}

impl Library {
    pub fn favorites(&self) -> rusqlite::Result<Favorites> {
        let ids = |sql: &str| -> rusqlite::Result<HashSet<i64>> {
            self.db.prepare(sql)?.query_map([], |r| r.get(0))?.collect()
        };
        Ok(Favorites {
            tracks: ids("SELECT track_id FROM track_favorites")?,
            albums: ids("SELECT album_id FROM album_favorites")?,
            playlists: ids("SELECT playlist_id FROM playlist_favorites")?,
            artists: self
                .db
                .prepare("SELECT folded FROM artist_favorites")?
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?,
        })
    }

    /// Marks it, or takes the mark away. Marking twice is marking once.
    pub fn set_favorite(&self, target: FavoriteTarget, on: bool) -> rusqlite::Result<()> {
        let (table, key, id) = match target {
            FavoriteTarget::Track(id) => ("track_favorites", "track_id", id),
            FavoriteTarget::Album(id) => ("album_favorites", "album_id", id),
            FavoriteTarget::Playlist(id) => ("playlist_favorites", "playlist_id", id),
            FavoriteTarget::Artist(name) => {
                let folded = folded(name);
                if folded.is_empty() {
                    return Ok(());
                }
                if on {
                    self.db.execute(
                        "INSERT OR IGNORE INTO artist_favorites (folded, name, added_at) VALUES (?1, ?2, ?3)",
                        params![folded, name.trim(), now_unix()],
                    )?;
                } else {
                    self.db.execute("DELETE FROM artist_favorites WHERE folded = ?1", params![folded])?;
                }
                return Ok(());
            }
        };
        if on {
            self.db.execute(
                &format!("INSERT OR IGNORE INTO {table} ({key}, added_at) VALUES (?1, ?2)"),
                params![id, now_unix()],
            )?;
        } else {
            self.db.execute(&format!("DELETE FROM {table} WHERE {key} = ?1"), params![id])?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn library() -> Library {
        let lib = Library::in_memory().unwrap();
        lib.db
            .execute_batch(
                "INSERT OR IGNORE INTO sources(id, provider, instance) VALUES (1, 'local', 'default');
                 INSERT INTO albums(id, title, album_artist, version_key) VALUES (1, 'Set', 'Band', '1:first');
                 INSERT INTO tracks(id, source_id, external_id, title, album_id, added_at,
                     album_title, album_credit, disc_number, disc_total)
                     VALUES (7, 1, '/set/a.flac', 'One', 1, 0, 'Set', 'Band', 1, 1);
                 INSERT INTO playlists(id, source_id, name, created_at, updated_at) VALUES (3, 1, 'Mine', 0, 0);",
            )
            .unwrap();
        lib
    }

    #[test]
    fn each_kind_keeps_its_own_mark() {
        let lib = library();
        for target in [FavoriteTarget::Track(7), FavoriteTarget::Album(1), FavoriteTarget::Playlist(3), FavoriteTarget::Artist("Cuca")] {
            assert!(!lib.favorites().unwrap().has(target));
            lib.set_favorite(target, true).unwrap();
            lib.set_favorite(target, true).unwrap();
            assert!(lib.favorites().unwrap().has(target), "{target:?} is marked");
        }
        lib.set_favorite(FavoriteTarget::Playlist(3), false).unwrap();
        let favorites = lib.favorites().unwrap();
        assert!(!favorites.has(FavoriteTarget::Playlist(3)));
        assert!(favorites.has(FavoriteTarget::Track(7)), "the others stay");
    }

    #[test]
    fn an_artist_is_the_same_whatever_its_capitals() {
        let lib = library();
        lib.set_favorite(FavoriteTarget::Artist("La Castañeda"), true).unwrap();
        assert!(lib.favorites().unwrap().has(FavoriteTarget::Artist(" la castañeda ")));
        lib.set_favorite(FavoriteTarget::Artist("LA CASTAÑEDA"), false).unwrap();
        assert!(lib.favorites().unwrap().artists.is_empty());
    }

    #[test]
    fn a_playlist_takes_its_marks_with_it() {
        let lib = library();
        lib.set_favorite(FavoriteTarget::Playlist(3), true).unwrap();
        lib.rate(crate::store::ratings::RatingTarget::Playlist(3), 4).unwrap();
        assert_eq!(lib.ratings().unwrap().playlists.get(&3), Some(&4));
        lib.db.execute("DELETE FROM playlists WHERE id = 3", []).unwrap();
        assert!(lib.favorites().unwrap().playlists.is_empty());
        assert!(lib.ratings().unwrap().playlists.is_empty());
    }
}
