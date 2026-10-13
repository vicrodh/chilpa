//! The covers and pictures the listener chose: a record's, an artist's and a
//! record of a plugin's. The files are the facade's; here is only which one
//! goes where.
use super::favorites::folded;
use super::Library;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// The pictures chosen for artists, asked by name whatever its capitals.
#[derive(Clone, Debug, Default)]
pub struct ArtistPictures(HashMap<String, String>);

impl ArtistPictures {
    pub fn of(&self, name: &str) -> Option<&str> {
        self.0.get(&folded(name)).map(String::as_str)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Library {
    /// Cache files still named by persistent library rows. Pruning these
    /// without changing the rows would leave blank covers until a rescan.
    pub fn protected_cache_artwork(&self, root: &Path) -> rusqlite::Result<HashSet<PathBuf>> {
        let mut paths = HashSet::new();
        let mut stmt = self.db.prepare(
            "SELECT artwork_path FROM albums UNION SELECT release_artwork FROM tracks
             UNION SELECT artwork_path FROM playlists UNION SELECT path FROM album_artwork
             UNION SELECT path FROM artist_artwork UNION SELECT path FROM remote_artwork
             UNION SELECT artwork_path FROM play_context",
        )?;
        let rows = stmt.query_map([], |row| row.get::<_, Option<String>>(0))?;
        for path in rows {
            if let Some(path) = path? {
                let path = PathBuf::from(path);
                if path.starts_with(root) {
                    paths.insert(path);
                }
            }
        }
        Ok(paths)
    }

    /// The cover chosen for a record, if one was.
    pub fn album_artwork(&self, album_id: i64) -> rusqlite::Result<Option<String>> {
        self.db.query_row("SELECT path FROM album_artwork WHERE album_id = ?1", params![album_id], |r| r.get(0)).optional()
    }

    /// Gives the record that cover, or with `None` takes the chosen one away.
    /// Says whether anything changed.
    pub fn set_album_artwork(&self, album_id: i64, path: Option<&str>) -> rusqlite::Result<bool> {
        let changed = match path {
            Some(path) => self.db.execute(
                "INSERT INTO album_artwork(album_id, path) VALUES (?1, ?2)
                 ON CONFLICT(album_id) DO UPDATE SET path = excluded.path WHERE path IS NOT excluded.path",
                params![album_id, path],
            )?,
            None => self.db.execute("DELETE FROM album_artwork WHERE album_id = ?1", params![album_id])?,
        };
        Ok(changed > 0)
    }

    /// The pictures chosen for artists.
    pub fn artist_pictures(&self) -> rusqlite::Result<ArtistPictures> {
        let rows = self.db.prepare("SELECT folded, path FROM artist_artwork")?.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
        Ok(ArtistPictures(rows))
    }

    /// The picture chosen for that artist, if one was.
    pub fn artist_artwork(&self, name: &str) -> rusqlite::Result<Option<String>> {
        self.db.query_row("SELECT path FROM artist_artwork WHERE folded = ?1", params![folded(name)], |r| r.get(0)).optional()
    }

    /// Gives the artist that picture, or with `None` takes it away.
    pub fn set_artist_artwork(&self, name: &str, path: Option<&str>) -> rusqlite::Result<bool> {
        let key = folded(name);
        if key.is_empty() {
            return Ok(false);
        }
        let changed = match path {
            Some(path) => self.db.execute(
                "INSERT INTO artist_artwork(folded, path) VALUES (?1, ?2)
                 ON CONFLICT(folded) DO UPDATE SET path = excluded.path WHERE path IS NOT excluded.path",
                params![key, path],
            )?,
            None => self.db.execute("DELETE FROM artist_artwork WHERE folded = ?1", params![key])?,
        };
        Ok(changed > 0)
    }

    /// The covers chosen for records of a plugin, by plugin and identity.
    pub fn remote_artworks(&self) -> rusqlite::Result<HashMap<(String, String), String>> {
        self.db
            .prepare("SELECT origin, identity, path FROM remote_artwork")?
            .query_map([], |r| Ok(((r.get(0)?, r.get(1)?), r.get(2)?)))?
            .collect()
    }

    /// Gives that record of a plugin a cover, or with `None` takes it away.
    pub fn set_remote_artwork(&self, origin: &str, identity: &str, path: Option<&str>) -> rusqlite::Result<bool> {
        if origin.is_empty() || identity.is_empty() {
            return Ok(false);
        }
        let changed = match path {
            Some(path) => self.db.execute(
                "INSERT INTO remote_artwork(origin, identity, path) VALUES (?1, ?2, ?3)
                 ON CONFLICT(origin, identity) DO UPDATE SET path = excluded.path WHERE path IS NOT excluded.path",
                params![origin, identity, path],
            )?,
            None => self.db.execute("DELETE FROM remote_artwork WHERE origin = ?1 AND identity = ?2", params![origin, identity])?,
        };
        Ok(changed > 0)
    }

    /// A cover of the artist's records, the record with most of theirs,
    /// for where a picture of them is wanted and none was chosen.
    pub fn artist_cover(&self, name: &str) -> rusqlite::Result<Option<String>> {
        self.db
            .query_row(
                concat!("SELECT ", album_art!("a"), " FROM albums a JOIN album_artist_rollup m ON m.album_id = a.id
                 WHERE m.artist = ?1 COLLATE NOCASE AND ", album_art!("a"), " IS NOT NULL
                 ORDER BY m.tracks DESC LIMIT 1"),
                params![name],
                |r| r.get(0),
            )
            .optional()
    }

    /// Whether any choice still names that file, so the facade knows when
    /// the file can go.
    pub fn artwork_in_use(&self, path: &str) -> rusqlite::Result<bool> {
        self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM album_artwork WHERE path = ?1)
                 OR EXISTS(SELECT 1 FROM artist_artwork WHERE path = ?1)
                 OR EXISTS(SELECT 1 FROM remote_artwork WHERE path = ?1)",
            params![path],
            |r| r.get(0),
        )
    }
}

/// A record that is folded into another, or split off it, keeps the cover
/// chosen for it; the one the other already had wins.
pub(crate) fn carry_album_artwork(db: &Connection, from: i64, to: i64) -> rusqlite::Result<usize> {
    db.execute(
        "INSERT OR IGNORE INTO album_artwork(album_id, path) SELECT ?2, path FROM album_artwork WHERE album_id = ?1",
        params![from, to],
    )
}

#[cfg(test)]
mod tests {
    use crate::store::fixtures;

    fn cover(lib: &crate::Library, id: i64) -> Option<String> {
        lib.albums(10, 0).unwrap().into_iter().find(|a| a.id == id).and_then(|a| a.artwork_path)
    }

    #[test]
    fn a_chosen_cover_is_shown_and_taking_it_away_brings_the_original_back() {
        let lib = fixtures::sample_library();
        let id = 1;
        lib.db.execute("UPDATE albums SET artwork_path = '/bayou.jpg' WHERE id = 1", []).unwrap();
        let original = cover(&lib, id);
        assert_eq!(original.as_deref(), Some("/bayou.jpg"));
        assert!(lib.set_album_artwork(id, Some("/chosen.png")).unwrap());
        assert_eq!(cover(&lib, id).as_deref(), Some("/chosen.png"));
        assert_eq!(lib.album(id).unwrap().unwrap().0.artwork_path.as_deref(), Some("/chosen.png"));
        assert!(!lib.set_album_artwork(id, Some("/chosen.png")).unwrap(), "the same choice changes nothing");
        assert!(lib.artwork_in_use("/chosen.png").unwrap());
        assert!(lib.set_album_artwork(id, None).unwrap());
        assert_eq!(cover(&lib, id), original);
        assert!(!lib.artwork_in_use("/chosen.png").unwrap());
    }

    #[test]
    fn a_cover_goes_with_its_record() {
        let lib = fixtures::sample_library();
        let id = 2;
        lib.set_album_artwork(id, Some("/chosen.png")).unwrap();
        lib.db.execute("DELETE FROM tracks WHERE album_id = ?1", [id]).unwrap();
        lib.db.execute("DELETE FROM albums WHERE id = ?1", [id]).unwrap();
        assert!(lib.album_artwork(id).unwrap().is_none());
    }

    #[test]
    fn a_record_folded_into_another_keeps_its_cover_unless_the_other_has_one() {
        let lib = fixtures::sample_library();
        lib.set_album_artwork(1, Some("/one.png")).unwrap();
        super::carry_album_artwork(&lib.db, 1, 2).unwrap();
        assert_eq!(lib.album_artwork(2).unwrap().as_deref(), Some("/one.png"));
        lib.set_album_artwork(3, Some("/three.png")).unwrap();
        super::carry_album_artwork(&lib.db, 1, 3).unwrap();
        assert_eq!(lib.album_artwork(3).unwrap().as_deref(), Some("/three.png"));
    }

    #[test]
    fn an_artist_picture_is_by_name_whatever_the_capitals() {
        let lib = fixtures::sample_library();
        assert!(lib.set_artist_artwork("Ólafur Arnalds", Some("/olafur.jpg")).unwrap());
        assert_eq!(lib.artist_pictures().unwrap().of("ÓLAFUR arnalds"), Some("/olafur.jpg"));
        assert_eq!(lib.artist_artwork("ólafur arnalds").unwrap().as_deref(), Some("/olafur.jpg"));
        assert!(lib.set_artist_artwork("ÓLAFUR ARNALDS", None).unwrap());
        assert!(lib.artist_pictures().unwrap().is_empty());
        assert!(lib.set_remote_artwork("plugin", "record-1", Some("/r.webp")).unwrap());
        assert_eq!(lib.remote_artworks().unwrap().get(&("plugin".into(), "record-1".into())).map(String::as_str), Some("/r.webp"));
    }

    #[test]
    fn cache_cleanup_keeps_artwork_chosen_for_source_records() {
        let lib = fixtures::sample_library();
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path();
        let chosen = root.join("chosen.webp");
        lib.set_remote_artwork("plugin", "record-1", chosen.to_str()).unwrap();
        lib.set_artist_artwork("Other", Some("other.webp")).unwrap();
        let protected = lib.protected_cache_artwork(root).unwrap();
        assert!(protected.contains(&root.join("chosen.webp")));
        assert!(!protected.contains(std::path::Path::new("other.webp")));
    }
}
