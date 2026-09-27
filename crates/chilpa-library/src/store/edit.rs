//! What an edit of a record's metadata leaves in the library.
//!
//! The files or their sidecar are written first, by whoever edits. This is
//! the library catching up with them, by the same rules the scan follows:
//! after it, scanning those files again changes nothing.

use crate::store::Library;
use rusqlite::{params, OptionalExtension};

/// Whether a column is written.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Put<T> {
    Keep,
    Set(T),
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TrackEdit {
    pub id: i64,
    pub title: Put<String>,
    pub artist: Put<String>,
    pub track_number: Put<Option<u16>>,
    pub disc_number: Put<Option<u16>>,
    pub recording_mbid: Put<Option<String>>,
    pub genres: Put<Vec<String>>,
}

impl TrackEdit {
    /// Nothing changes on that track.
    pub fn none(id: i64) -> Self {
        Self {
            id,
            title: Put::Keep,
            artist: Put::Keep,
            track_number: Put::Keep,
            disc_number: Put::Keep,
            recording_mbid: Put::Keep,
            genres: Put::Keep,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AlbumEdit {
    pub album_id: i64,
    /// What the record is called and whose it is after the edit. Always
    /// given, changed or not.
    pub title: String,
    pub album_artist: String,
    pub year: Put<Option<i32>>,
    /// Where its cover is, on disk.
    pub artwork: Put<String>,
    /// The tracks the edit reached. A track of the record that is not here
    /// was not edited, and stays where it was.
    pub tracks: Vec<TrackEdit>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Edited {
    /// The record the tracks are on now.
    pub album_id: i64,
    /// It is not the one the edit started from.
    pub moved: bool,
}

/// A record's track as the editor starts from it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct EditTrack {
    pub id: i64,
    pub external_id: String,
    /// The file that holds it, when one does.
    pub container: Option<String>,
    pub title: String,
    pub artist: String,
    pub track_number: Option<u16>,
    pub disc_number: Option<u16>,
    pub recording_mbid: Option<String>,
    pub genres: Vec<String>,
}

impl Library {
    /// A record's present tracks, in the record's order.
    pub fn edit_tracks(&self, album_id: i64) -> rusqlite::Result<Vec<EditTrack>> {
        let mut tracks: Vec<EditTrack> = {
            let mut stmt = self.db.prepare(
                "SELECT t.id, t.external_id,
                        (SELECT s.container FROM scanned_files s WHERE s.track_id = t.id LIMIT 1),
                        t.title, t.artist, t.track_number, t.disc_number, t.recording_mbid
                 FROM tracks t
                 WHERE t.album_id = ?1 AND t.present = 1
                 ORDER BY t.disc_number, t.track_number, t.title COLLATE NOCASE",
            )?;
            let rows = stmt.query_map(params![album_id], |r| {
                Ok(EditTrack {
                    id: r.get(0)?,
                    external_id: r.get(1)?,
                    container: r.get(2)?,
                    title: r.get(3)?,
                    artist: r.get(4)?,
                    track_number: r.get::<_, Option<i64>>(5)?.and_then(|n| u16::try_from(n).ok()),
                    disc_number: r.get::<_, Option<i64>>(6)?.and_then(|n| u16::try_from(n).ok()),
                    recording_mbid: r.get(7)?,
                    genres: Vec::new(),
                })
            })?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        for track in &mut tracks {
            track.genres = self.track_genres(track.id)?;
        }
        Ok(tracks)
    }

    /// How the library already spells a genre, if it has it.
    pub fn genre_spelling(&self, name: &str) -> rusqlite::Result<Option<String>> {
        // The column is COLLATE NOCASE: "heavy metal" finds "Heavy Metal".
        self.db.query_row("SELECT name FROM genres WHERE name = ?1", params![name.trim()], |r| r.get(0)).optional()
    }

    /// Leaves in the library what an edit left on disk.
    ///
    /// One transaction. Nothing in it walks the library: it runs on the
    /// thread that draws the window.
    pub fn apply_edit(&self, edit: &AlbumEdit) -> rusqlite::Result<Edited> {
        let title = edit.title.trim();
        let artist = edit.album_artist.trim();
        if title.is_empty() || artist.is_empty() || edit.tracks.is_empty() {
            return Err(rusqlite::Error::InvalidQuery);
        }
        let mut ids: Vec<i64> = edit.tracks.iter().map(|t| t.id).collect();
        ids.sort_unstable();
        ids.dedup();
        if ids.len() != edit.tracks.len() {
            return Err(rusqlite::Error::InvalidQuery);
        }

        let from = edit.album_id;
        let tx = self.own_transaction()?;
        for id in &ids {
            let on: Option<i64> = tx
                .query_row("SELECT album_id FROM tracks WHERE id = ?1", params![id], |r| r.get::<_, Option<i64>>(0))
                .optional()?
                .flatten();
            if on != Some(from) {
                // Nothing was written yet, and the transaction goes as it came.
                return Err(rusqlite::Error::InvalidQuery);
            }
        }

        // Present or not: a track that is away is still the record's.
        let all: i64 = tx.query_row("SELECT count(*) FROM tracks WHERE album_id = ?1", params![from], |r| r.get(0))?;
        let whole = all as usize == ids.len();
        // The question the scan asks, so both find the same record.
        let there: Option<i64> = tx
            .query_row(
                "SELECT id FROM albums WHERE title = ?1 AND album_artist = ?2 AND folder = ''",
                params![title, artist],
                |r| r.get(0),
            )
            .optional()?;
        let to = match there {
            Some(id) => id,
            None if whole => {
                tx.execute(
                    "UPDATE albums SET title = ?2, album_artist = ?3, folder = '' WHERE id = ?1",
                    params![from, title, artist],
                )?;
                // The search keeps a copy of the record's title on each
                // track's row, refreshed when the track's record is set.
                tx.execute("UPDATE tracks SET album_id = album_id WHERE album_id = ?1", params![from])?;
                from
            }
            None => {
                tx.execute("INSERT INTO albums (title, album_artist) VALUES (?1, ?2)", params![title, artist])?;
                tx.last_insert_rowid()
            }
        };

        let moved = to != from;
        if moved {
            for id in &ids {
                tx.execute("UPDATE tracks SET album_id = ?2 WHERE id = ?1", params![id, to])?;
            }
            let left: i64 = tx.query_row("SELECT count(*) FROM tracks WHERE album_id = ?1", params![from], |r| r.get(0))?;
            if left == 0 {
                tx.execute(
                    "INSERT OR IGNORE INTO album_favorites (album_id, added_at)
                     SELECT ?2, added_at FROM album_favorites WHERE album_id = ?1",
                    params![from, to],
                )?;
                // What the record that goes knew and the one that stays did not.
                tx.execute(
                    "UPDATE albums SET
                       artwork_path = COALESCE(artwork_path, (SELECT artwork_path FROM albums WHERE id = ?2)),
                       year = COALESCE(year, (SELECT year FROM albums WHERE id = ?2))
                     WHERE id = ?1",
                    params![to, from],
                )?;
                tx.execute("DELETE FROM albums WHERE id = ?1", params![from])?;
            }
        }

        if let Put::Set(year) = &edit.year {
            tx.execute("UPDATE albums SET year = ?2 WHERE id = ?1", params![to, year])?;
        }
        if let Put::Set(path) = &edit.artwork {
            tx.execute("UPDATE albums SET artwork_path = ?2 WHERE id = ?1", params![to, path])?;
        }

        for track in &edit.tracks {
            if let Put::Set(v) = &track.title {
                tx.execute("UPDATE tracks SET title = ?2 WHERE id = ?1", params![track.id, v.trim()])?;
            }
            if let Put::Set(v) = &track.artist {
                tx.execute("UPDATE tracks SET artist = ?2 WHERE id = ?1", params![track.id, v.trim()])?;
            }
            if let Put::Set(v) = &track.track_number {
                tx.execute("UPDATE tracks SET track_number = ?2 WHERE id = ?1", params![track.id, v])?;
            }
            if let Put::Set(v) = &track.disc_number {
                tx.execute("UPDATE tracks SET disc_number = ?2 WHERE id = ?1", params![track.id, v])?;
            }
            if let Put::Set(v) = &track.recording_mbid {
                tx.execute("UPDATE tracks SET recording_mbid = ?2 WHERE id = ?1", params![track.id, v])?;
            }
            if let Put::Set(genres) = &track.genres {
                // Inside this transaction it takes no other of its own.
                self.set_track_genres(track.id, genres)?;
            }
        }

        if moved {
            // The plays follow the tracks. Counted for the two records that
            // changed, and for no other.
            tx.execute(
                "UPDATE album_rollup
                    SET plays = (SELECT count(*) FROM play_events p
                                   JOIN tracks t ON t.id = p.track_id
                                  WHERE t.album_id = album_rollup.album_id
                                    AND (p.duration_ms = 0 OR p.played_ms * 2 >= p.duration_ms)),
                        last_played = (SELECT max(p.started_at) FROM play_events p
                                         JOIN tracks t ON t.id = p.track_id
                                        WHERE t.album_id = album_rollup.album_id
                                          AND (p.duration_ms = 0 OR p.played_ms * 2 >= p.duration_ms))
                  WHERE album_id IN (?1, ?2)",
                params![from, to],
            )?;
        }
        tx.finish()?;
        Ok(Edited { album_id: to, moved })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AlbumSort;

    /// Two records by the same artist. "First" has three tracks, one of
    /// them away; "Second" has one. Track 1 has played twice, track 4 once.
    fn shelf() -> Library {
        let lib = Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        lib.db.execute("INSERT INTO albums (id, title, album_artist, year, artwork_path) VALUES (1, 'First', 'Someone', 1990, '/covers/first.jpg')", []).unwrap();
        lib.db.execute("INSERT INTO albums (id, title, album_artist) VALUES (2, 'Second', 'Someone')", []).unwrap();
        for (id, album, number, present) in [(1, 1, 1, 1), (2, 1, 2, 1), (3, 1, 3, 0), (4, 2, 1, 1)] {
            lib.db
                .execute(
                    "INSERT INTO tracks (id, source_id, external_id, title, artist, album_id, track_number,
                                         duration_ms, present, added_at)
                     VALUES (?1, ?2, ?3, ?4, 'Someone', ?5, ?6, 1000, ?7, ?1)",
                    params![id, source, format!("/m/{album}/{number:02}.flac"), format!("Track {id}"), album, number, present],
                )
                .unwrap();
        }
        lib.set_track_genres(1, &["Rock".to_string()]).unwrap();
        lib.set_album_favorite(1, true).unwrap();
        for (track, at) in [(1, 10), (1, 20), (4, 30)] {
            lib.log_play(Some(track), Some(source), at, 1000, 1000, false, None, None, true).unwrap();
        }
        lib
    }

    fn edit(album_id: i64, title: &str, tracks: &[i64]) -> AlbumEdit {
        AlbumEdit {
            album_id,
            title: title.to_string(),
            album_artist: "Someone".to_string(),
            year: Put::Keep,
            artwork: Put::Keep,
            tracks: tracks.iter().map(|id| TrackEdit::none(*id)).collect(),
        }
    }

    fn record(lib: &Library, id: i64) -> Option<(String, String, Option<i64>, Option<String>)> {
        lib.db
            .query_row("SELECT title, album_artist, year, artwork_path FROM albums WHERE id = ?1", params![id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })
            .optional()
            .unwrap()
    }

    fn on(lib: &Library, track: i64) -> i64 {
        lib.db.query_row("SELECT album_id FROM tracks WHERE id = ?1", params![track], |r| r.get(0)).unwrap()
    }

    fn plays(lib: &Library, album: i64) -> i64 {
        lib.db.query_row("SELECT plays FROM album_rollup WHERE album_id = ?1", params![album], |r| r.get(0)).unwrap()
    }

    #[test]
    fn renaming_a_record_keeps_its_id_its_favorite_and_its_plays() {
        let lib = shelf();
        let out = lib.apply_edit(&edit(1, "First, remastered", &[1, 2, 3])).unwrap();
        assert_eq!(out, Edited { album_id: 1, moved: false });
        assert_eq!(record(&lib, 1).unwrap().0, "First, remastered");
        assert!(lib.favorite_albums().unwrap().contains(&1));
        assert_eq!(plays(&lib, 1), 2);
    }

    #[test]
    fn the_search_finds_the_new_title_and_not_the_old() {
        let lib = shelf();
        lib.apply_edit(&edit(1, "Renamed", &[1, 2, 3])).unwrap();
        assert_eq!(lib.search("Renamed", 10).unwrap().len(), 2, "its two present tracks");
        assert!(lib.search("First", 10).unwrap().is_empty());
    }

    #[test]
    fn a_record_renamed_into_another_joins_it() {
        let lib = shelf();
        let out = lib.apply_edit(&edit(1, "Second", &[1, 2, 3])).unwrap();
        assert_eq!(out, Edited { album_id: 2, moved: true });
        assert!(record(&lib, 1).is_none(), "the one it came from is gone");
        assert_eq!([on(&lib, 1), on(&lib, 2), on(&lib, 3)], [2, 2, 2]);
        assert!(lib.favorite_albums().unwrap().contains(&2), "its favorite went with it");
        let kept = record(&lib, 2).unwrap();
        assert_eq!((kept.2, kept.3.as_deref()), (Some(1990), Some("/covers/first.jpg")), "and what it knew");
        assert_eq!(plays(&lib, 2), 3);
    }

    #[test]
    fn editing_part_of_a_record_splits_it() {
        let lib = shelf();
        // Tracks 1 and 2 are what is present. Track 3 is away and was not edited.
        let out = lib.apply_edit(&edit(1, "First, corrected", &[1, 2])).unwrap();
        assert!(out.moved);
        assert_ne!(out.album_id, 1);
        assert_eq!([on(&lib, 1), on(&lib, 2)], [out.album_id, out.album_id]);
        assert_eq!(on(&lib, 3), 1, "the track that is away is left where it was");
        assert_eq!(record(&lib, 1).unwrap().0, "First", "on the record as it was");
        assert!(lib.favorite_albums().unwrap().contains(&1), "which keeps its favorite");
        assert_eq!(plays(&lib, out.album_id), 2);
        assert_eq!(plays(&lib, 1), 0);
    }

    #[test]
    fn a_track_of_another_record_is_refused_and_nothing_is_written() {
        let lib = shelf();
        let mut wrong = edit(1, "Renamed", &[1, 4]);
        wrong.tracks[0].title = Put::Set("Changed".to_string());
        assert!(lib.apply_edit(&wrong).is_err());
        assert_eq!(record(&lib, 1).unwrap().0, "First");
        let title: String = lib.db.query_row("SELECT title FROM tracks WHERE id = 1", [], |r| r.get(0)).unwrap();
        assert_eq!(title, "Track 1");

        assert!(lib.apply_edit(&edit(1, "  ", &[1, 2, 3])).is_err(), "a record with no title");
        assert!(lib.apply_edit(&edit(1, "Renamed", &[1, 1])).is_err(), "the same track twice");
        assert!(lib.apply_edit(&edit(1, "Renamed", &[])).is_err(), "no track at all");
    }

    #[test]
    fn a_field_left_alone_keeps_its_value() {
        let lib = shelf();
        let mut e = edit(1, "First", &[1, 2, 3]);
        e.tracks[0].disc_number = Put::Set(Some(2));
        e.tracks[0].track_number = Put::Set(None);
        e.year = Put::Set(None);
        let out = lib.apply_edit(&e).unwrap();
        assert!(!out.moved);
        let (title, artist, track, disc): (String, String, Option<i64>, Option<i64>) = lib
            .db
            .query_row("SELECT title, artist, track_number, disc_number FROM tracks WHERE id = 1", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })
            .unwrap();
        assert_eq!((title.as_str(), artist.as_str()), ("Track 1", "Someone"));
        assert_eq!((track, disc), (None, Some(2)));
        let kept = record(&lib, 1).unwrap();
        assert_eq!(kept.2, None, "the year was cleared");
        assert_eq!(kept.3.as_deref(), Some("/covers/first.jpg"), "and the cover was left alone");
        assert_eq!(lib.track_genres(1).unwrap(), ["Rock"]);
    }

    #[test]
    fn the_genres_are_replaced_not_added_to() {
        let lib = shelf();
        let mut e = edit(1, "First", &[1, 2, 3]);
        e.tracks[0].genres = Put::Set(vec!["Blues".to_string(), "Soul".to_string()]);
        lib.apply_edit(&e).unwrap();
        assert_eq!(lib.track_genres(1).unwrap(), ["Blues", "Soul"]);
        assert_eq!(lib.search("Rock", 10).unwrap().len(), 0);
        assert_eq!(lib.search("Soul", 10).unwrap().len(), 1);
    }

    #[test]
    fn the_counts_of_both_records_add_up() {
        let lib = shelf();
        lib.apply_edit(&edit(1, "Second", &[1, 2, 3])).unwrap();
        let kept: Vec<(i64, i64, Option<i64>, i64)> = {
            let mut stmt = lib.db.prepare("SELECT album_id, plays, last_played, present_tracks FROM album_rollup ORDER BY album_id").unwrap();
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).unwrap();
            rows.map(|r| r.unwrap()).collect()
        };
        lib.rebuild_rollups().unwrap();
        let counted: Vec<(i64, i64, Option<i64>, i64)> = {
            let mut stmt = lib.db.prepare("SELECT album_id, plays, last_played, present_tracks FROM album_rollup ORDER BY album_id").unwrap();
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).unwrap();
            rows.map(|r| r.unwrap()).collect()
        };
        assert_eq!(kept, counted, "what was kept is what counting from the log gives");
        assert_eq!(lib.albums_sorted(AlbumSort::Title, 10, 0).unwrap().len(), 1);
        assert!(lib.check().unwrap().is_empty());
    }

    #[test]
    fn the_editor_starts_from_what_is_present_in_the_record_s_order() {
        let lib = shelf();
        let root = lib.root_id(1, std::path::Path::new("/m")).unwrap();
        lib.db
            .execute(
                "INSERT INTO scanned_files (root_id, path, mtime, size, track_id, scanned_at, container)
                 VALUES (?1, '/m/1/02.flac', 0, 0, 2, 0, '/m/1/disc.iso')",
                params![root],
            )
            .unwrap();
        let tracks = lib.edit_tracks(1).unwrap();
        assert_eq!(tracks.iter().map(|t| t.id).collect::<Vec<_>>(), [1, 2], "the one that is away is not there");
        assert_eq!(tracks[0].genres, ["Rock"]);
        assert_eq!(tracks[0].container, None);
        assert_eq!(tracks[1].container.as_deref(), Some("/m/1/disc.iso"));
    }

    #[test]
    fn a_genre_is_spelled_the_way_the_library_already_spells_it() {
        let lib = shelf();
        assert_eq!(lib.genre_spelling("rock").unwrap().as_deref(), Some("Rock"));
        assert_eq!(lib.genre_spelling("zydeco").unwrap(), None);
    }
}
