//! Browsing: four facets in cascade and the tracks they leave.
//!
//! The window's Library screen narrows the library with four columns -- year,
//! genre, artist, album -- where each column only offers what the ones to its
//! left left standing. Below them, the tracks that pass all four.
//!
//! The tracks are read in two steps, and that is the design, not a detail.
//! First [`Library::browse_ids`] takes a snapshot of the order: every id the
//! filter leaves, sorted, and the totals, in one query. Sorting fifty
//! thousand tracks costs tens of milliseconds, so the window runs it off
//! its own thread. Then [`Library::tracks_by_ids`] reads the rows of the
//! ids scrolling into view, by primary key: page forty thousand costs what
//! the first page costs, which `LIMIT ... OFFSET` cannot do -- it walks
//! every row it skips, and at 40,000 that was 70 to 140 ms.
//!
//! A track row here carries what a list draws: its disc, the bits and rate
//! of the file and its plays. It is its own type so the rows search and the
//! analysis read do not grow columns they never use.

use crate::store::Library;
use rusqlite::types::ToSql;
use rusqlite::params_from_iter;

/// What the four columns have chosen. `None` is "All".
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filter {
    pub year: Option<i32>,
    pub genre: Option<String>,
    /// The album artist: what the record says about itself.
    pub artist: Option<String>,
    pub album: Option<i64>,
    /// Who made the record or plays on one of its tracks: an artist's page,
    /// where a compilation they are on counts as theirs too.
    pub performer: Option<String>,
}

/// How the tracks are ordered.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TrackOrder {
    /// Artist, year, album, then the record's own order: a shelf.
    #[default]
    Artist,
    Album,
    Year,
    /// The newest arrivals first.
    Added,
    Title,
    /// The most played first.
    Plays,
    Duration,
    /// Disc and track number: the order of one record.
    Disc,
}

impl TrackOrder {
    fn keys(self) -> &'static str {
        match self {
            TrackOrder::Artist => {
                "a.album_artist COLLATE NOCASE, a.year, a.title COLLATE NOCASE, t.disc_number, t.track_number"
            }
            TrackOrder::Album => "a.title COLLATE NOCASE, t.disc_number, t.track_number",
            TrackOrder::Year => "a.year, a.title COLLATE NOCASE, t.disc_number, t.track_number",
            TrackOrder::Added => "t.added_at DESC, t.disc_number, t.track_number",
            TrackOrder::Title => "t.title COLLATE NOCASE",
            TrackOrder::Plays => "plays DESC, t.title COLLATE NOCASE",
            TrackOrder::Duration => "t.duration_ms",
            TrackOrder::Disc => "t.disc_number, t.track_number, t.title COLLATE NOCASE",
        }
    }
}

/// A track as a list draws it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowseTrack {
    pub id: i64,
    pub external_id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_id: Option<i64>,
    pub disc_number: Option<u16>,
    pub track_number: Option<u16>,
    pub duration_ms: u64,
    /// The file's own, not what the device was opened with.
    pub sample_rate_hz: Option<u32>,
    pub bit_depth: Option<u8>,
    pub plays: usize,
    /// Its record's cover.
    pub artwork_path: Option<String>,
}

/// One entry of a facet column: what it is called, how many albums it
/// holds and, for an album, its year.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FacetEntry {
    /// What the filter takes: the year, the genre, the artist or the album id.
    pub key: String,
    pub label: String,
    pub albums: usize,
    pub year: Option<i32>,
}

/// A folder inside the library, as the Folders screen lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FolderEntry {
    pub name: String,
    pub path: String,
    /// Tracks under it, all the way down.
    pub tracks: usize,
    /// A cover from one of its records, if any has one.
    pub artwork_path: Option<String>,
}

/// The paths under `dir`, as a range: every path that starts with `dir/`
/// sorts between `dir/` and `dir0`, the character after `/`. A range keeps
/// the query to what is under the folder instead of every track.
fn under(dir: &str) -> (String, String, i64) {
    let dir = dir.trim_end_matches('/');
    // SQLite counts characters, not bytes, in substr and instr.
    (format!("{dir}/"), format!("{dir}0"), dir.chars().count() as i64 + 2)
}

const LOCAL: &str = "t.source_id = (SELECT id FROM sources WHERE provider = 'local' AND instance = 'default')";

/// How much a filter leaves.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Totals {
    pub tracks: usize,
    pub albums: usize,
    pub duration_ms: u64,
}

/// Every id a filter leaves, in order, with what they add up to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub ids: Vec<i64>,
    pub totals: Totals,
}

const ROW: &str = "SELECT t.id, t.external_id, t.title, t.artist, a.title, t.album_id, t.disc_number,
            t.track_number, t.duration_ms, t.sample_rate_hz, t.bit_depth,
            COALESCE(r.plays, 0) AS plays, a.artwork_path";

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<BrowseTrack> {
    Ok(BrowseTrack {
        id: r.get(0)?,
        external_id: r.get(1)?,
        title: r.get(2)?,
        artist: r.get(3)?,
        album: r.get(4)?,
        album_id: r.get(5)?,
        disc_number: r.get::<_, Option<i64>>(6)?.map(|n| n as u16),
        track_number: r.get::<_, Option<i64>>(7)?.map(|n| n as u16),
        duration_ms: r.get::<_, i64>(8)? as u64,
        sample_rate_hz: r.get::<_, Option<i64>>(9)?.map(|n| n as u32),
        bit_depth: r.get::<_, Option<i64>>(10)?.map(|n| n as u8),
        plays: r.get::<_, i64>(11)? as usize,
        artwork_path: r.get(12)?,
    })
}

/// The albums with something present, for the facets that only need album
/// columns: counting over `album_rollup` walks thousands of albums instead
/// of every track.
const ALBUMS: &str = "FROM albums a JOIN album_rollup ar ON ar.album_id = a.id WHERE ar.present_tracks > 0";

const FROM: &str = "FROM tracks t
     JOIN albums a ON a.id = t.album_id
     LEFT JOIN track_rollup r ON r.track_id = t.id";

/// The `WHERE` a filter makes, and its parameters in order.
fn clause(f: &Filter) -> (String, Vec<Box<dyn ToSql>>) {
    let mut sql = String::from("WHERE t.present = 1");
    let mut args: Vec<Box<dyn ToSql>> = Vec::new();
    if let Some(year) = f.year {
        sql.push_str(" AND a.year = ?");
        args.push(Box::new(year));
    }
    if let Some(genre) = &f.genre {
        sql.push_str(
            " AND EXISTS (SELECT 1 FROM track_genres tg JOIN genres g ON g.id = tg.genre_id
                          WHERE tg.track_id = t.id AND g.name = ?)",
        );
        args.push(Box::new(genre.clone()));
    }
    if let Some(artist) = &f.artist {
        // The same artist tagged "Alice in Chains" on one record and "Alice
        // In Chains" on another is one artist, as the column shows it.
        sql.push_str(" AND a.album_artist = ? COLLATE NOCASE");
        args.push(Box::new(artist.clone()));
    }
    if let Some(album) = f.album {
        sql.push_str(" AND a.id = ?");
        args.push(Box::new(album));
    }
    if let Some(name) = &f.performer {
        sql.push_str(" AND (a.album_artist = ? COLLATE NOCASE OR t.artist = ? COLLATE NOCASE)");
        args.push(Box::new(name.clone()));
        args.push(Box::new(name.clone()));
    }
    (sql, args)
}

impl Library {
    /// The years, newest first, with how many albums each holds. The first
    /// column has nothing to its left, so it always offers every year.
    pub fn facet_years(&self) -> rusqlite::Result<Vec<FacetEntry>> {
        let sql = format!(
            "SELECT a.year, count(*) {ALBUMS} AND a.year IS NOT NULL GROUP BY a.year ORDER BY a.year DESC"
        );
        let mut stmt = self.db.prepare(&sql)?;
        let rows = stmt.query_map([], |r| {
            let year: i32 = r.get(0)?;
            Ok(FacetEntry {
                key: year.to_string(),
                label: year.to_string(),
                albums: r.get::<_, i64>(1)? as usize,
                year: Some(year),
            })
        })?;
        rows.collect()
    }

    /// The genres the chosen year leaves, by name.
    pub fn facet_genres(&self, year: Option<i32>) -> rusqlite::Result<Vec<FacetEntry>> {
        let (filter, args) = clause(&Filter { year, ..Filter::default() });
        let sql = format!(
            "SELECT g.name, count(DISTINCT a.id) {FROM}
             JOIN track_genres tg ON tg.track_id = t.id
             JOIN genres g ON g.id = tg.genre_id
             {filter} GROUP BY g.name ORDER BY g.name COLLATE NOCASE"
        );
        self.named_facet(&sql, args)
    }

    /// The album artists the year and the genre leave.
    pub fn facet_artists(&self, year: Option<i32>, genre: Option<&str>) -> rusqlite::Result<Vec<FacetEntry>> {
        if genre.is_none() {
            let (sql, args): (String, Vec<Box<dyn ToSql>>) = match year {
                Some(y) => (
                    format!("SELECT min(a.album_artist), count(*) {ALBUMS} AND a.year = ? GROUP BY a.album_artist COLLATE NOCASE ORDER BY 1 COLLATE NOCASE"),
                    vec![Box::new(y)],
                ),
                None => (
                    format!("SELECT min(a.album_artist), count(*) {ALBUMS} GROUP BY a.album_artist COLLATE NOCASE ORDER BY 1 COLLATE NOCASE"),
                    Vec::new(),
                ),
            };
            return self.named_facet(&sql, args);
        }
        let f = Filter { year, genre: genre.map(str::to_string), ..Filter::default() };
        let (filter, args) = clause(&f);
        let sql = format!(
            "SELECT min(a.album_artist), count(DISTINCT a.id) {FROM} {filter}
             GROUP BY a.album_artist COLLATE NOCASE ORDER BY 1 COLLATE NOCASE"
        );
        self.named_facet(&sql, args)
    }

    /// The albums the other three leave, in shelf order, with their year.
    pub fn facet_albums(&self, f: &Filter) -> rusqlite::Result<Vec<FacetEntry>> {
        let (filter, args) = clause(&Filter { album: None, ..f.clone() });
        let sql = format!(
            "SELECT a.id, a.title, a.year, count(t.id) {FROM} {filter}
             GROUP BY a.id ORDER BY a.album_artist COLLATE NOCASE, a.year, a.title COLLATE NOCASE"
        );
        let mut stmt = self.db.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(args.iter()), |r| {
            Ok(FacetEntry {
                key: r.get::<_, i64>(0)?.to_string(),
                label: r.get(1)?,
                albums: 1,
                year: r.get(2)?,
            })
        })?;
        rows.collect()
    }

    fn named_facet(&self, sql: &str, args: Vec<Box<dyn ToSql>>) -> rusqlite::Result<Vec<FacetEntry>> {
        let mut stmt = self.db.prepare(sql)?;
        let rows = stmt.query_map(params_from_iter(args.iter()), |r| {
            let name: String = r.get(0)?;
            Ok(FacetEntry { key: name.clone(), label: name, albums: r.get::<_, i64>(1)? as usize, year: None })
        })?;
        rows.collect()
    }

    /// The snapshot of an order: every id the filter leaves, sorted, and
    /// the totals, in one pass. The costly half of browsing; the window
    /// runs it off its own thread.
    pub fn browse_ids(&self, f: &Filter, order: TrackOrder) -> rusqlite::Result<Snapshot> {
        let (filter, args) = clause(f);
        let sql = format!(
            "SELECT t.id, t.album_id, t.duration_ms, COALESCE(r.plays, 0) AS plays
             {FROM} {filter} ORDER BY {}",
            order.keys()
        );
        let mut stmt = self.db.prepare(&sql)?;
        let mut rows = stmt.query(params_from_iter(args.iter()))?;
        let mut out = Snapshot::default();
        let mut albums = std::collections::HashSet::new();
        while let Some(r) = rows.next()? {
            out.ids.push(r.get(0)?);
            albums.insert(r.get::<_, Option<i64>>(1)?);
            out.totals.duration_ms += r.get::<_, i64>(2)? as u64;
        }
        out.totals.tracks = out.ids.len();
        out.totals.albums = albums.len();
        Ok(out)
    }

    /// The rows of those ids, in the order given. By primary key, so it
    /// costs the same for any stretch of a snapshot.
    pub fn tracks_by_ids(&self, ids: &[i64]) -> rusqlite::Result<Vec<BrowseTrack>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let marks = vec!["?"; ids.len()].join(",");
        let sql = format!("{ROW} {FROM} WHERE t.id IN ({marks})");
        let mut stmt = self.db.prepare(&sql)?;
        let found: std::collections::HashMap<i64, BrowseTrack> = stmt
            .query_map(params_from_iter(ids.iter()), row)?
            .map(|r| r.map(|t| (t.id, t)))
            .collect::<rusqlite::Result<_>>()?;
        Ok(ids.iter().filter_map(|id| found.get(id).cloned()).collect())
    }

    /// The rows of those files, aligned with them: `None` for a file the
    /// library does not have. What a queue shows, where anything can be
    /// queued, library or not. In chunks, so a long queue never builds one
    /// statement with thousands of parameters.
    pub fn tracks_by_paths(&self, paths: &[String]) -> rusqlite::Result<Vec<Option<BrowseTrack>>> {
        let mut found = std::collections::HashMap::new();
        for chunk in paths.chunks(500) {
            let marks = vec!["?"; chunk.len()].join(",");
            let sql = format!("{ROW} {FROM} WHERE t.external_id IN ({marks})");
            let mut stmt = self.db.prepare(&sql)?;
            for row in stmt.query_map(params_from_iter(chunk.iter()), row)? {
                let t = row?;
                found.insert(t.external_id.clone(), t);
            }
        }
        Ok(paths.iter().map(|p| found.get(p).cloned()).collect())
    }

    /// What lies directly under `dir`: the subfolders that hold tracks, with
    /// how many each holds and a cover, and the tracks that sit in it.
    pub fn folder_children(&self, dir: &str) -> rusqlite::Result<(Vec<FolderEntry>, Vec<BrowseTrack>)> {
        let (from, to, cut) = under(dir);
        let base = from.clone();
        let sql = format!(
            "SELECT substr(t.external_id, ?3, instr(substr(t.external_id, ?3), '/') - 1) AS name,
                    count(*), max(a.artwork_path)
             FROM tracks t LEFT JOIN albums a ON a.id = t.album_id
             WHERE {LOCAL} AND t.present = 1 AND t.external_id >= ?1 AND t.external_id < ?2
               AND instr(substr(t.external_id, ?3), '/') > 0
             GROUP BY name ORDER BY name COLLATE NOCASE"
        );
        let mut stmt = self.db.prepare(&sql)?;
        let folders = stmt
            .query_map(rusqlite::params![from, to, cut], |r| {
                let name: String = r.get(0)?;
                Ok(FolderEntry {
                    path: format!("{base}{name}"),
                    name,
                    tracks: r.get::<_, i64>(1)? as usize,
                    artwork_path: r.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let sql = format!(
            "{ROW} {FROM} WHERE {LOCAL} AND t.present = 1 AND t.external_id >= ?1 AND t.external_id < ?2
               AND instr(substr(t.external_id, ?3), '/') = 0
             ORDER BY t.disc_number, t.track_number, t.external_id"
        );
        let mut stmt = self.db.prepare(&sql)?;
        let tracks = stmt.query_map(rusqlite::params![from, to, cut], row)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok((folders, tracks))
    }

    /// How many tracks there are under `dir`, all the way down.
    pub fn folder_track_count(&self, dir: &str) -> rusqlite::Result<usize> {
        let (from, to, _) = under(dir);
        let sql = format!(
            "SELECT count(*) FROM tracks t WHERE {LOCAL} AND t.present = 1 AND t.external_id >= ?1 AND t.external_id < ?2"
        );
        self.db.query_row(&sql, rusqlite::params![from, to], |r| r.get::<_, i64>(0)).map(|n| n as usize)
    }

    /// Every file under `dir`, all the way down, in path order: what playing
    /// a folder queues.
    pub fn folder_paths(&self, dir: &str) -> rusqlite::Result<Vec<String>> {
        let (from, to, _) = under(dir);
        let sql = format!(
            "SELECT t.external_id FROM tracks t WHERE {LOCAL} AND t.present = 1
               AND t.external_id >= ?1 AND t.external_id < ?2 ORDER BY t.external_id"
        );
        let mut stmt = self.db.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params![from, to], |r| r.get(0))?;
        rows.collect()
    }

    /// All the tracks a small filter leaves, in one go: one record, one
    /// shelf of a home screen. For the whole library, the snapshot.
    pub fn browse_tracks(&self, f: &Filter, order: TrackOrder, limit: usize) -> rusqlite::Result<Vec<BrowseTrack>> {
        let (filter, mut args) = clause(f);
        let sql = format!("{ROW} {FROM} {filter} ORDER BY {} LIMIT ?", order.keys());
        args.push(Box::new(limit as i64));
        let mut stmt = self.db.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(args.iter()), row)?;
        rows.collect()
    }

    /// The genre most of an album's tracks carry, if any carries one.
    pub fn album_genre(&self, album_id: i64) -> rusqlite::Result<Option<String>> {
        use rusqlite::OptionalExtension;
        self.db
            .query_row(
                "SELECT g.name FROM tracks t
                 JOIN track_genres tg ON tg.track_id = t.id
                 JOIN genres g ON g.id = tg.genre_id
                 WHERE t.album_id = ?1 AND t.present = 1
                 GROUP BY g.name ORDER BY count(*) DESC, g.name LIMIT 1",
                [album_id],
                |r| r.get(0),
            )
            .optional()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    /// Two artists, three albums, two genres; one track played twice.
    fn shelf() -> Library {
        let lib = Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        for (id, title, artist, year) in
            [(1, "Low Orbit", "Mora Vale", 2019), (2, "Afterimage", "Nils Okafor", 2021), (3, "Northbound", "Mora Vale", 2018)]
        {
            lib.db
                .execute(
                    "INSERT INTO albums (id, title, album_artist, year) VALUES (?1, ?2, ?3, ?4)",
                    params![id, title, artist, year],
                )
                .unwrap();
        }
        // (id, album, disc, number, bits, rate, genre)
        for (id, album, disc, n, bits, rate, genre) in [
            (10, 1, 1, 1, 24, 192_000, "Alternative"),
            (11, 1, 2, 1, 24, 192_000, "Alternative"),
            (12, 1, 1, 2, 24, 192_000, "Alternative"),
            (20, 2, 1, 1, 16, 44_100, "Latin"),
            (30, 3, 1, 1, 16, 44_100, "Alternative"),
        ] {
            lib.db
                .execute(
                    "INSERT INTO tracks (id, source_id, external_id, title, album_id, disc_number,
                                         track_number, duration_ms, bit_depth, sample_rate_hz, added_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1000, ?8, ?9, ?1)",
                    params![id, source, format!("/{id}.flac"), format!("T{id}"), album, disc, n, bits, rate],
                )
                .unwrap();
            lib.set_track_genres(id, &[genre.to_string()]).unwrap();
        }
        lib.log_play(Some(20), Some(source), 1, 1000, 1000, false, None, None, true).unwrap();
        lib.log_play(Some(20), Some(source), 2, 1000, 1000, false, None, None, true).unwrap();
        lib
    }

    fn labels(entries: Vec<FacetEntry>) -> Vec<String> {
        entries.into_iter().map(|e| e.label).collect()
    }

    #[test]
    fn each_column_offers_only_what_the_ones_to_its_left_leave() {
        let lib = shelf();
        assert_eq!(labels(lib.facet_years().unwrap()), ["2021", "2019", "2018"], "newest first");
        assert_eq!(labels(lib.facet_genres(None).unwrap()), ["Alternative", "Latin"]);
        assert_eq!(labels(lib.facet_genres(Some(2021)).unwrap()), ["Latin"]);
        assert_eq!(labels(lib.facet_artists(None, Some("Alternative")).unwrap()), ["Mora Vale"]);
        let f = Filter { artist: Some("Mora Vale".into()), ..Filter::default() };
        assert_eq!(labels(lib.facet_albums(&f).unwrap()), ["Northbound", "Low Orbit"], "by year");
    }

    #[test]
    fn one_artist_in_two_spellings_is_one_entry_and_one_filter() {
        let lib = shelf();
        lib.db.execute("INSERT INTO albums (id, title, album_artist, year) VALUES (4, 'Late', 'MORA VALE', 2020)", []).unwrap();
        let source = lib.source_id("local", "default").unwrap();
        lib.db
            .execute(
                "INSERT INTO tracks (id, source_id, external_id, title, album_id, duration_ms, added_at) VALUES (40, ?1, '/40.flac', 'T', 4, 1000, 1)",
                params![source],
            )
            .unwrap();
        let artists = lib.facet_artists(None, None).unwrap();
        assert_eq!(artists.iter().filter(|a| a.label.eq_ignore_ascii_case("mora vale")).count(), 1);
        let f = Filter { artist: Some("mora vale".into()), ..Filter::default() };
        assert_eq!(lib.facet_albums(&f).unwrap().len(), 3, "both spellings' records");
    }

    #[test]
    fn a_facet_counts_albums_not_tracks() {
        let lib = shelf();
        let genres = lib.facet_genres(None).unwrap();
        assert_eq!(genres[0].albums, 2, "Alternative is two records, four tracks");
    }

    #[test]
    fn a_snapshot_knows_the_size_before_any_row_is_read() {
        let lib = shelf();
        let all = lib.browse_ids(&Filter::default(), TrackOrder::Artist).unwrap();
        assert_eq!(all.totals, Totals { tracks: 5, albums: 3, duration_ms: 5000 });
        assert_eq!(all.ids.len(), 5);
        let one = lib.browse_ids(&Filter { album: Some(1), ..Filter::default() }, TrackOrder::Disc).unwrap();
        assert_eq!((one.totals.tracks, one.totals.albums), (3, 1));
        assert_eq!(one.ids, [10, 12, 11], "disc 1 first, then disc 2");
    }

    #[test]
    fn rows_come_back_in_the_order_they_were_asked_for() {
        let lib = shelf();
        let rows = lib.tracks_by_ids(&[30, 10, 20]).unwrap();
        let ids: Vec<i64> = rows.iter().map(|t| t.id).collect();
        assert_eq!(ids, [30, 10, 20]);
        assert!(lib.tracks_by_ids(&[]).unwrap().is_empty());
    }

    #[test]
    fn a_record_reads_in_disc_order_with_what_its_files_are() {
        let lib = shelf();
        let f = Filter { album: Some(1), ..Filter::default() };
        let rows = lib.browse_tracks(&f, TrackOrder::Disc, 50).unwrap();
        let order: Vec<(Option<u16>, Option<u16>)> = rows.iter().map(|t| (t.disc_number, t.track_number)).collect();
        assert_eq!(order, [(Some(1), Some(1)), (Some(1), Some(2)), (Some(2), Some(1))]);
        assert_eq!((rows[0].bit_depth, rows[0].sample_rate_hz), (Some(24), Some(192_000)));
    }

    #[test]
    fn a_queue_reads_its_files_and_leaves_a_hole_for_the_unknown() {
        let lib = shelf();
        let rows = lib.tracks_by_paths(&["/20.flac".into(), "/nowhere.flac".into(), "/10.flac".into()]).unwrap();
        let ids: Vec<Option<i64>> = rows.iter().map(|r| r.as_ref().map(|t| t.id)).collect();
        assert_eq!(ids, [Some(20), None, Some(10)]);
    }

    #[test]
    fn a_folder_lists_its_subfolders_and_its_own_tracks() {
        let lib = Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        lib.db.execute("INSERT INTO albums (id, title, album_artist) VALUES (1, 'A', 'X')", []).unwrap();
        for (id, path) in [(1, "/m/Box/Disc 1/01.flac"), (2, "/m/Box/Disc 1/02.flac"), (3, "/m/Box/Disc 2/01.flac"), (4, "/m/Box/bonus.flac"), (5, "/m/Other/01.flac"), (6, "/m/Box0/x.flac")] {
            lib.db
                .execute(
                    "INSERT INTO tracks (id, source_id, external_id, title, album_id, added_at) VALUES (?1, ?2, ?3, 'T', 1, 1)",
                    params![id, source, path],
                )
                .unwrap();
        }
        let (folders, tracks) = lib.folder_children("/m/Box").unwrap();
        let names: Vec<(String, usize)> = folders.iter().map(|f| (f.name.clone(), f.tracks)).collect();
        assert_eq!(names, [("Disc 1".to_string(), 2), ("Disc 2".to_string(), 1)]);
        assert_eq!(folders[0].path, "/m/Box/Disc 1");
        assert_eq!(tracks.iter().map(|t| t.id).collect::<Vec<_>>(), [4], "only what sits in it");
        assert_eq!(lib.folder_track_count("/m/Box").unwrap(), 4, "Box0 is not under Box");
        assert_eq!(lib.folder_paths("/m/Box/").unwrap().len(), 4, "a trailing slash is the same folder");
    }

    #[test]
    fn plays_come_with_the_row_and_order_it_when_asked() {
        let lib = shelf();
        let rows = lib.browse_tracks(&Filter::default(), TrackOrder::Plays, 1).unwrap();
        assert_eq!((rows[0].id, rows[0].plays), (20, 2));
    }

    #[test]
    fn an_album_takes_the_genre_most_of_it_carries() {
        let lib = shelf();
        assert_eq!(lib.album_genre(1).unwrap().as_deref(), Some("Alternative"));
        assert_eq!(lib.album_genre(99).unwrap(), None);
    }
}
