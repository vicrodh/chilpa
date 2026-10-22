//! The library's artists, by what they are credited with.
//!
//! An album artist is who a record is credited to. Whoever plays on a
//! track of it is an artist of the library too, and on a compilation they
//! are the only ones who play: the record is credited to a stand-in.
//!
//! That stand-in is written many ways -- "Various Artists", "Various",
//! "VA", "V.A." -- and they are one entry here, under the name
//! the library gives a compilation. It is credited with records and plays
//! on none: a track tagged with it on somebody's record does not make that
//! record one of its own.

use super::{Library, StoredArtist};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::OnceLock;

/// How the stand-in for a compilation's artists is written.
const STAND_INS: &str = include_str!("../../data/various-artists.txt");

/// Which artists a list holds.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ArtistRole {
    /// Who a record is credited to.
    #[default]
    Album,
    /// Those, and whoever plays on a track.
    Any,
}

impl ArtistRole {
    /// From the name a setting keeps: "all" is everyone, anything else
    /// the album artists.
    pub fn from_name(name: &str) -> Self {
        if name == "all" { Self::Any } else { Self::Album }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Album => "album",
            Self::Any => "all",
        }
    }
}

/// Whether `name` is the stand-in for the artists of a compilation, in any
/// of its spellings.
pub fn is_various(name: &str) -> bool {
    static NAMES: OnceLock<HashSet<String>> = OnceLock::new();
    NAMES.get_or_init(|| STAND_INS.lines().map(folded).filter(|n| !n.is_empty()).collect()).contains(&folded(name))
}

fn folded(name: &str) -> String {
    name.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// One artist while its rows are read.
#[derive(Default)]
struct Entry {
    name: String,
    /// The tracks they have on each record.
    records: HashMap<i64, usize>,
    artwork: Option<String>,
}

impl Library {
    /// The artists with something present, by name.
    ///
    /// A record in several editions counts once while editions are grouped.
    pub fn artists_by_role(&self, role: ArtistRole) -> rusqlite::Result<Vec<StoredArtist>> {
        self.artists_by_role_within(role, None)
    }

    /// The same, from only the records in `within` when it is given: those a
    /// filter of sources, qualities or formats leaves.
    pub fn artists_by_role_within(&self, role: ArtistRole, within: Option<&HashSet<i64>>) -> rusqlite::Result<Vec<StoredArtist>> {
        let sql = match role {
            ArtistRole::Album => {
                concat!("SELECT a.id, coalesce(p.name, trim(a.album_artist)), r.present_tracks, ", album_art!("a"), ", a.album_artist
                 FROM albums a
                 JOIN album_rollup r ON r.album_id = a.id AND r.present_tracks > 0
                 LEFT JOIN credit_parts p ON p.credit = trim(a.album_artist)
                 WHERE trim(a.album_artist) <> ''
                   AND EXISTS (SELECT 1 FROM tracks t JOIN sources s ON s.id = t.source_id AND s.enabled = 1
                               WHERE t.album_id = a.id AND t.present = 1)")
            }
            ArtistRole::Any => {
                concat!("SELECT m.album_id, m.artist, m.tracks, ", album_art!("a"), ", a.album_artist
                 FROM album_artist_rollup m JOIN albums a ON a.id = m.album_id
                 WHERE EXISTS (SELECT 1 FROM tracks t JOIN sources s ON s.id = t.source_id AND s.enabled = 1
                               WHERE t.album_id = a.id AND t.present = 1
                                 AND (t.artist = m.artist COLLATE NOCASE OR t.album_credit = m.artist COLLATE NOCASE
                                      OR a.album_artist = m.artist COLLATE NOCASE
                                      OR EXISTS (SELECT 1 FROM credit_parts p WHERE p.name = m.artist
                                                 AND p.credit IN (trim(t.artist), trim(t.album_credit), trim(a.album_artist)))))")
            }
        };
        let mut found: BTreeMap<String, Entry> = BTreeMap::new();
        let mut stmt = self.db.prepare(sql)?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)? as usize, r.get::<_, Option<String>>(3)?, r.get::<_, String>(4)?)))?;
        for row in rows {
            let (album, name, tracks, artwork, credited) = row?;
            if within.is_some_and(|within| !within.contains(&album)) {
                continue;
            }
            let various = is_various(&name);
            if various && !is_various(&credited) {
                continue;
            }
            // The key is what SQLite's NOCASE compares, so an entry is the
            // artist its page asks for.
            let key = if various { crate::VARIOUS_ARTISTS.to_ascii_lowercase() } else { name.trim().to_ascii_lowercase() };
            let entry = found.entry(key).or_default();
            if various {
                entry.name = crate::VARIOUS_ARTISTS.to_string();
            } else if entry.name.is_empty() || name < entry.name {
                entry.name = name;
            }
            // Two spellings on one record are the same tracks, not more.
            let had = entry.records.entry(album).or_default();
            *had = (*had).max(tracks);
            if artwork > entry.artwork {
                entry.artwork = artwork;
            }
        }
        let catalog = if self.group_album_versions() { Some(self.version_catalog()?) } else { None };
        Ok(found
            .into_values()
            .map(|entry| {
                let mut ids: Vec<i64> = entry.records.keys().copied().collect();
                ids.sort_unstable();
                if let Some(catalog) = &catalog {
                    ids = catalog.collapse(&ids);
                }
                StoredArtist {
                    name: entry.name,
                    album_count: ids.len(),
                    track_count: ids.iter().filter_map(|id| entry.records.get(id)).sum(),
                    artwork_path: entry.artwork,
                }
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    /// A record of their own by Aimer with a track that names nobody, a
    /// compilation Aimer and YUI are on, and a soundtrack whose tracks name
    /// nobody.
    fn shelf() -> Library {
        let lib = Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        for (id, title, artist) in [(1, "Sun Dance", "Aimer"), (2, "OP&ED", "Various Artists"), (3, "Score", "VA")] {
            lib.db.execute("INSERT INTO albums (id, title, album_artist) VALUES (?1, ?2, ?3)", params![id, title, artist]).unwrap();
        }
        for (id, album, artist) in [(10, 1, "Aimer"), (20, 2, "aimer"), (21, 2, "YUI"), (22, 2, "YUI"), (30, 3, "Various Interprets"), (31, 3, "Artistes Divers"), (11, 1, "Various")] {
            lib.db
                .execute(
                    "INSERT INTO tracks (id, source_id, external_id, title, artist, album_id, duration_ms, added_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1000, ?1)",
                    params![id, source, format!("/{album}/{id}.flac"), format!("T{id}"), artist, album],
                )
                .unwrap();
        }
        lib
    }

    fn listed(lib: &Library, role: ArtistRole) -> Vec<(String, usize, usize)> {
        lib.artists_by_role(role).unwrap().into_iter().map(|a| (a.name, a.album_count, a.track_count)).collect()
    }

    #[test]
    fn a_filter_leaves_the_artists_of_the_records_it_lets_through() {
        let lib = shelf();
        let within: HashSet<i64> = [1].into();
        let names = |role| -> Vec<String> { lib.artists_by_role_within(role, Some(&within)).unwrap().into_iter().map(|a| a.name).collect() };
        assert_eq!(names(ArtistRole::Album), ["Aimer"], "the compilations were filtered out");
        assert_eq!(names(ArtistRole::Any), ["Aimer"]);
        assert!(lib.artists_by_role_within(ArtistRole::Album, Some(&HashSet::new())).unwrap().is_empty());
    }

    #[test]
    fn the_stand_in_is_known_in_its_spellings_and_a_name_that_holds_it_is_not() {
        for name in ["Various Artists", "various artists", " Various  Artists ", "VA", "v.a.", "Artistes Divers", "Various Interprets"] {
            assert!(is_various(name), "{name}");
        }
        for name in ["", "Various Cruelties", "Vari", "Eva", "The Various"] {
            assert!(!is_various(name), "{name}");
        }
    }

    #[test]
    fn who_plays_on_a_compilation_is_not_an_album_artist() {
        let lib = shelf();
        assert_eq!(
            listed(&lib, ArtistRole::Album),
            [("Aimer".to_string(), 1, 2), ("Various Artists".to_string(), 2, 5)],
            "YUI is credited with no record"
        );
    }

    #[test]
    fn everyone_is_an_artist_and_the_stand_in_is_one_of_them_once() {
        let lib = shelf();
        assert_eq!(
            listed(&lib, ArtistRole::Any),
            [("Aimer".to_string(), 2, 3), ("Various Artists".to_string(), 2, 5), ("YUI".to_string(), 1, 2)],
            "three spellings of the stand-in, one entry; Aimer's record is not one of its own"
        );
    }

    #[test]
    fn each_artist_of_a_joined_credit_is_an_album_artist() {
        let lib = shelf();
        lib.db.execute("INSERT INTO albums (id, title, album_artist) VALUES (4, 'Duets', 'Aimer, YUI')", []).unwrap();
        let source = lib.source_id("local", "default").unwrap();
        lib.db
            .execute(
                "INSERT INTO tracks (id, source_id, external_id, title, artist, album_id, duration_ms, added_at)
                 VALUES (40, ?1, '/4/40.flac', 'T40', 'Aimer, YUI', 4, 1000, 40)",
                params![source],
            )
            .unwrap();
        lib.match_credits().unwrap();
        assert_eq!(
            listed(&lib, ArtistRole::Album),
            [("Aimer".to_string(), 2, 3), ("Various Artists".to_string(), 2, 5), ("YUI".to_string(), 1, 1)]
        );
    }

    #[test]
    fn what_is_not_present_has_no_artist() {
        let lib = shelf();
        lib.db.execute("UPDATE tracks SET present = 0 WHERE album_id = 1", []).unwrap();
        lib.rebuild_rollups().unwrap();
        assert_eq!(listed(&lib, ArtistRole::Album), [("Various Artists".to_string(), 2, 5)]);
    }

    #[test]
    fn a_role_is_kept_by_name() {
        assert_eq!(ArtistRole::from_name("all"), ArtistRole::Any);
        assert_eq!(ArtistRole::from_name("album"), ArtistRole::Album);
        assert_eq!(ArtistRole::from_name(""), ArtistRole::Album);
        assert_eq!(ArtistRole::from_name(ArtistRole::Any.name()), ArtistRole::Any);
    }
}
