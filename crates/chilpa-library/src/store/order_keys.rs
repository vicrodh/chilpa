//! What a track is ordered by, read with the snapshot that orders it.
//!
//! The database orders the library's tracks; tracks that live outside it --
//! a source's saved ones -- are ordered here, by the same keys, and merged.
//! [`TrackOrder::compare`] is `TrackOrder::keys` written in Rust: NOCASE
//! folds only ASCII, as SQLite's does, and an absent value sorts first going
//! up and last going down, as NULL does.

use super::browse::{letter_of, TrackOrder};
use super::Library;
use std::cmp::Ordering;

/// The columns `OrderKey::read` takes, after id, album, duration and plays.
pub(super) const COLUMNS: &str = ", a.album_artist, a.title, a.year, coalesce(t.release_disc, t.disc_number),
    t.track_number, t.title, t.artist, t.added_at";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OrderKey {
    pub album_id: i64,
    pub album_artist: String,
    pub album: String,
    pub year: Option<i32>,
    pub disc: Option<i64>,
    pub number: Option<i64>,
    pub title: String,
    pub artist: String,
    pub added_at: i64,
    pub duration_ms: i64,
    pub plays: i64,
}

impl OrderKey {
    pub(super) fn read(r: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            album_id: r.get::<_, Option<i64>>(1)?.unwrap_or(0),
            duration_ms: r.get(2)?,
            plays: r.get(3)?,
            album_artist: r.get::<_, Option<String>>(4)?.unwrap_or_default(),
            album: r.get::<_, Option<String>>(5)?.unwrap_or_default(),
            year: r.get(6)?,
            disc: r.get(7)?,
            number: r.get(8)?,
            title: r.get(9)?,
            artist: r.get(10)?,
            added_at: r.get(11)?,
        })
    }
}

/// SQLite's NOCASE: ASCII letters fold, everything else compares as bytes.
pub fn nocase(a: &str, b: &str) -> Ordering {
    a.bytes().map(|c| c.to_ascii_lowercase()).cmp(b.bytes().map(|c| c.to_ascii_lowercase()))
}

/// `ltrim`, which takes spaces only.
fn trimmed(text: &str) -> &str {
    text.trim_start_matches(' ')
}

impl TrackOrder {
    /// The order the snapshot query gives, with `grouped_by` in front as it
    /// puts it there.
    pub fn compare(self, grouped_by: &str, a: &OrderKey, b: &OrderKey) -> Ordering {
        use TrackOrder::*;
        let turned = |on: bool, o: Ordering| if on { o.reverse() } else { o };
        let group = match grouped_by {
            "album" => turned(self == AlbumDescending, nocase(&a.album, &b.album)).then(a.album_id.cmp(&b.album_id)),
            "artist" => turned(self == ArtistDescending, nocase(&a.artist, &b.artist)),
            "name" => turned(self == TitleDescending, letter_of(trimmed(&a.title)).cmp(&letter_of(trimmed(&b.title)))),
            _ => Ordering::Equal,
        };
        let record = || nocase(&a.album, &b.album).then(a.disc.cmp(&b.disc)).then(a.number.cmp(&b.number));
        let place = || a.disc.cmp(&b.disc).then(a.number.cmp(&b.number));
        let title = || nocase(trimmed(&a.title), trimmed(&b.title));
        group.then_with(|| match self {
            Artist => nocase(&a.album_artist, &b.album_artist).then(a.year.cmp(&b.year)).then_with(record),
            ArtistDescending => nocase(&b.album_artist, &a.album_artist).then(a.year.cmp(&b.year)).then_with(record),
            Album => record(),
            AlbumDescending => nocase(&b.album, &a.album).then_with(place),
            Year => a.year.cmp(&b.year).then_with(record),
            YearDescending => b.year.cmp(&a.year).then_with(record),
            Added => b.added_at.cmp(&a.added_at).then_with(place),
            AddedAscending => a.added_at.cmp(&b.added_at).then_with(place),
            Title => title(),
            TitleDescending => title().reverse(),
            Plays => b.plays.cmp(&a.plays).then(nocase(&a.title, &b.title)),
            PlaysAscending => a.plays.cmp(&b.plays).then(nocase(&a.title, &b.title)),
            Duration => a.duration_ms.cmp(&b.duration_ms).then_with(title),
            DurationDescending => b.duration_ms.cmp(&a.duration_ms).then_with(title),
            Disc => place().then(nocase(&a.title, &b.title)),
            Performer => nocase(&a.artist, &b.artist).then_with(record).then(nocase(&a.title, &b.title)),
        })
    }
}

impl Library {
    /// The grouped snapshot (`grouped_snapshot`) with the keys of the ids it
    /// keeps.
    pub fn keyed_snapshot(&self, filter: &super::Filter, order: TrackOrder) -> rusqlite::Result<(super::Snapshot, Vec<OrderKey>)> {
        if !self.group_album_versions() { return self.browse_rows(filter, order, true); }
        let catalog = self.version_catalog()?;
        let mut filter = filter.clone();
        let mut albums = filter.albums.clone();
        albums.extend(filter.album);
        if !albums.is_empty() {
            filter.album = None;
            filter.albums = albums.iter().flat_map(|id| catalog.choices(*id)).map(|v| v.id).collect();
            if filter.albums.is_empty() { return Ok(Default::default()); }
        }
        let (mut snapshot, keys) = self.browse_rows(&filter, order, true)?;
        snapshot.album_ids = catalog.collapse(&snapshot.album_ids);
        let chosen: std::collections::HashSet<i64> = snapshot.album_ids.iter().copied().collect();
        let mut removed_ms = 0;
        let (mut ids, mut kept) = (Vec::with_capacity(snapshot.ids.len()), Vec::with_capacity(keys.len()));
        for (id, key) in snapshot.ids.into_iter().zip(keys) {
            match catalog.tracks.get(&id) {
                Some((album, duration)) if !chosen.contains(album) => removed_ms += duration,
                _ => { ids.push(id); kept.push(key); }
            }
        }
        snapshot.ids = ids;
        snapshot.totals.albums = chosen.len();
        snapshot.totals.tracks = snapshot.ids.len();
        snapshot.totals.duration_ms = snapshot.totals.duration_ms.saturating_sub(removed_ms);
        Ok((snapshot, kept))
    }
}

/// What a record is ordered by on a grid, as `AlbumSort::order_by` says.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AlbumKey {
    pub title: String,
    pub artist: String,
    pub year: Option<i32>,
    pub added_at: i64,
    pub plays: i64,
    pub last_played: Option<i64>,
}

impl super::AlbumSort {
    pub fn compare(self, a: &AlbumKey, b: &AlbumKey) -> Ordering {
        use super::AlbumSort::*;
        let title = || nocase(&a.title, &b.title);
        match self {
            Artist => nocase(&a.artist, &b.artist).then(a.year.cmp(&b.year)).then_with(title),
            Title => title(),
            Year => b.year.cmp(&a.year).then_with(title),
            Added => b.added_at.cmp(&a.added_at).then_with(title),
            Played => b.plays.cmp(&a.plays).then_with(title),
            LastPlayed => b.last_played.cmp(&a.last_played).then_with(title),
        }
    }
}

impl Library {
    /// The keys of those records, by id; by primary key, in pages of 500.
    pub fn album_keys(&self, ids: &[i64]) -> rusqlite::Result<std::collections::HashMap<i64, AlbumKey>> {
        let mut keys = std::collections::HashMap::with_capacity(ids.len());
        for chunk in ids.chunks(500) {
            let marks = vec!["?"; chunk.len()].join(",");
            let mut stmt = self.db.prepare(&format!("SELECT a.id, a.title, a.album_artist, a.year, coalesce(r.added_at, 0), coalesce(r.plays, 0), r.last_played
                FROM albums a LEFT JOIN album_rollup r ON r.album_id = a.id WHERE a.id IN ({marks})"))?;
            for row in stmt.query_map(rusqlite::params_from_iter(chunk.iter()), |r| Ok((r.get::<_, i64>(0)?, AlbumKey {
                title: r.get(1)?, artist: r.get(2)?, year: r.get(3)?, added_at: r.get(4)?, plays: r.get(5)?, last_played: r.get(6)?,
            })))? {
                let (id, key) = row?;
                keys.insert(id, key);
            }
        }
        Ok(keys)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Filter;
    use rusqlite::params;

    /// Titles, artists and records that NOCASE, accents, spaces and NULLs
    /// order in ways a naive comparison would not.
    fn library() -> Library {
        let lib = Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        for (id, title, artist, year) in [(1, "beta", "Zed", Some(1999)), (2, "Alpha", "zed", None), (3, "Ñandú", "Ana", Some(2001)), (4, "alpha", "Bob", Some(1999))] {
            lib.connection().execute("INSERT INTO albums(id, title, album_artist, year) VALUES (?1, ?2, ?3, ?4)", params![id, title, artist, year]).unwrap();
        }
        let tracks = [
            (1, 1, Some(1), Some(2), "  spaced", "Zed", 30, 3000),
            (2, 1, Some(1), Some(1), "Échelle", "zed", 10, 1000),
            (3, 2, None, Some(1), "apple", "Other", 20, 1000),
            (4, 3, Some(2), None, "Zulu", "Ana", 40, 2000),
            (5, 3, Some(1), Some(1), "zulu", "ana", 40, 5000),
            (6, 4, Some(1), Some(1), "Mango", "Bob", 5, 4000),
        ];
        for (id, album, disc, number, title, artist, added, ms) in tracks {
            lib.connection().execute("INSERT INTO tracks(id, source_id, external_id, title, artist, album_id, disc_number, track_number, added_at, duration_ms)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)", params![id, source, format!("/{id}.flac"), title, artist, album, disc, number, added, ms]).unwrap();
        }
        lib.log_play(Some(5), Some(source), 1, 1000, 1000, false, None, None, true).unwrap();
        lib
    }

    #[test]
    fn the_rust_order_is_the_order_the_database_gives() {
        let lib = library();
        let orders = ["artist", "album", "year", "added", "title", "plays", "duration", "duration-desc", "disc",
            "artist-desc", "title-desc", "year-desc", "performer", "album-desc", "added-asc", "plays-asc"];
        for group in ["", "album", "artist", "name"] {
            for name in orders {
                let order = TrackOrder::from_name(name);
                let filter = Filter { grouped_by: group.into(), ..Filter::default() };
                let (snapshot, keys) = lib.browse_rows(&filter, order, true).unwrap();
                assert_eq!(snapshot, lib.browse_ids(&filter, order).unwrap(), "{name} {group}: the keyed read is the same snapshot");
                let mut sorted: Vec<(i64, OrderKey)> = snapshot.ids.iter().copied().zip(keys).collect();
                sorted.sort_by(|a, b| order.compare(group, &a.1, &b.1));
                let ids: Vec<i64> = sorted.into_iter().map(|(id, _)| id).collect();
                assert_eq!(ids, snapshot.ids, "{name} grouped by '{group}'");
            }
        }
    }

    #[test]
    fn records_order_in_rust_as_the_grid_query_orders_them() {
        let lib = library();
        for order in crate::store::AlbumSort::all() {
            let ids = lib.album_ids(order).unwrap();
            let keys = lib.album_keys(&ids).unwrap();
            let mut sorted = ids.clone();
            sorted.sort_by(|a, b| order.compare(&keys[a], &keys[b]));
            assert_eq!(sorted, ids, "{order:?}");
        }
    }

    #[test]
    fn a_grouped_library_keeps_the_keys_of_the_tracks_it_keeps() {
        let lib = library();
        lib.set_group_album_versions(true).unwrap();
        let filter = Filter::default();
        let (snapshot, keys) = lib.keyed_snapshot(&filter, TrackOrder::Album).unwrap();
        assert_eq!(snapshot, lib.grouped_snapshot(&filter, TrackOrder::Album).unwrap());
        assert_eq!(keys.len(), snapshot.ids.len());
    }
}
