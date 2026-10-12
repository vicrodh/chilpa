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

/// What the seven columns have chosen: year, genre, label, composer,
/// rating, artist and record. `None` is "All".
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filter {
    /// Header searches participate in the result, even with All selected.
    pub queries: [String; 7],
    pub year: Option<i32>,
    pub genre: Option<String>,
    /// An album credit or any artist contributing to the album.
    pub artist: Option<String>,
    pub album: Option<i64>,
    /// Multi-selection from Library Explorer. Singular fields remain for
    /// callers that open one artist or album directly.
    pub years: Vec<i32>,
    pub genres: Vec<String>,
    pub labels: Vec<String>,
    pub composers: Vec<String>,
    /// A record's own stars, 0 for none: the Library screen's column.
    pub ratings: Vec<u8>,
    pub artists: Vec<String>,
    pub albums: Vec<i64>,
    /// A track's own stars, 0 for none: the Tracks screen's filter.
    pub track_ratings: Vec<u8>,
    /// Source keys are `provider:instance`; exclusions keep empty as "all".
    pub excluded_sources: Vec<String>,
    pub qualities: Vec<String>,
    pub formats: Vec<String>,
    /// Who made the record or plays on one of its tracks: an artist's page,
    /// where a compilation they are on counts as theirs too.
    pub performer: Option<String>,
    /// What was typed to find tracks: every word of it has to be in the
    /// title, the artist or the record.
    pub text: String,
    /// What a list of tracks is grouped by: "album", "artist", "name" (the
    /// title's letter) or nothing. The groups come in their order, and the
    /// order asked for is the order inside each one.
    pub grouped_by: String,
}

impl Filter {
    /// Applies a Library Explorer column and invalidates every choice to its right.
    pub fn choose(&mut self, at: usize, keys: &[String]) {
        match at {
            0 => { self.year = None; self.years = keys.iter().filter_map(|key| key.parse().ok()).collect(); }
            1 => { self.genre = None; self.genres = keys.to_vec(); }
            2 => self.labels = keys.to_vec(),
            3 => self.composers = keys.to_vec(),
            4 => self.ratings = keys.iter().filter_map(|key| key.parse().ok()).collect(),
            5 => { self.artist = None; self.artists = keys.to_vec(); }
            _ => { self.album = None; self.albums = keys.iter().filter_map(|key| key.parse().ok()).collect(); }
        }
        if at < 1 { self.genre = None; self.genres.clear(); }
        if at < 2 { self.labels.clear(); }
        if at < 3 { self.composers.clear(); }
        if at < 4 { self.ratings.clear(); }
        if at < 5 { self.artist = None; self.artists.clear(); }
        if at < 6 { self.album = None; self.albums.clear(); }
    }

    pub fn toggle_extra(&mut self, kind: &str, key: &str) {
        let values = match kind {
            "source" => &mut self.excluded_sources,
            "quality" => &mut self.qualities,
            "format" => &mut self.formats,
            _ => return,
        };
        if let Some(at) = values.iter().position(|value| value == key) { values.remove(at); } else { values.push(key.into()); }
    }

    pub fn reset_extra(&mut self) {
        self.excluded_sources.clear();
        self.qualities.clear();
        self.formats.clear();
    }

    pub fn extra_filtered(&self) -> bool {
        !(self.excluded_sources.is_empty() && self.qualities.is_empty() && self.formats.is_empty())
    }
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
    /// The shortest first.
    Duration,
    /// The longest first.
    DurationDescending,
    /// Disc and track number: the order of one record.
    Disc,
    ArtistDescending,
    TitleDescending,
    /// The newest records first.
    YearDescending,
    /// Who plays each track, then the record, then the title: a list
    /// grouped by artist.
    Performer,
    /// The records from Z, each in its own order.
    AlbumDescending,
    /// The oldest arrivals first.
    AddedAscending,
    /// The least played first.
    PlaysAscending,
}

impl TrackOrder {
    pub fn from_name(name: &str) -> Self {
        match name {
            "album" => Self::Album, "year" => Self::Year, "added" => Self::Added,
            "title" => Self::Title, "plays" => Self::Plays, "duration" => Self::Duration,
            "duration-desc" => Self::DurationDescending,
            "disc" => Self::Disc, "artist-desc" => Self::ArtistDescending, "title-desc" => Self::TitleDescending,
            "year-desc" => Self::YearDescending, "performer" => Self::Performer,
            "album-desc" => Self::AlbumDescending, "added-asc" => Self::AddedAscending, "plays-asc" => Self::PlaysAscending,
            _ => Self::Artist,
        }
    }

    fn keys(self) -> &'static str {
        match self {
            TrackOrder::Artist => {
                "a.album_artist COLLATE NOCASE, a.year, a.title COLLATE NOCASE, coalesce(t.release_disc, t.disc_number), t.track_number"
            }
            TrackOrder::Album => "a.title COLLATE NOCASE, coalesce(t.release_disc, t.disc_number), t.track_number",
            TrackOrder::Year => "a.year, a.title COLLATE NOCASE, coalesce(t.release_disc, t.disc_number), t.track_number",
            TrackOrder::Added => "t.added_at DESC, coalesce(t.release_disc, t.disc_number), t.track_number",
            // Without the spaces a tag left before it: a title that starts
            // with one is not before the alphabet.
            TrackOrder::Title => "ltrim(t.title) COLLATE NOCASE",
            TrackOrder::Plays => "plays DESC, t.title COLLATE NOCASE",
            TrackOrder::Duration => "t.duration_ms, ltrim(t.title) COLLATE NOCASE",
            TrackOrder::DurationDescending => "t.duration_ms DESC, ltrim(t.title) COLLATE NOCASE",
            TrackOrder::Disc => "coalesce(t.release_disc, t.disc_number), t.track_number, t.title COLLATE NOCASE",
            TrackOrder::ArtistDescending => {
                "a.album_artist COLLATE NOCASE DESC, a.year, a.title COLLATE NOCASE, coalesce(t.release_disc, t.disc_number), t.track_number"
            }
            TrackOrder::TitleDescending => "ltrim(t.title) COLLATE NOCASE DESC",
            TrackOrder::YearDescending => "a.year DESC, a.title COLLATE NOCASE, coalesce(t.release_disc, t.disc_number), t.track_number",
            TrackOrder::Performer => "t.artist COLLATE NOCASE, a.title COLLATE NOCASE, coalesce(t.release_disc, t.disc_number), t.track_number, t.title COLLATE NOCASE",
            TrackOrder::AlbumDescending => "a.title COLLATE NOCASE DESC, coalesce(t.release_disc, t.disc_number), t.track_number",
            TrackOrder::AddedAscending => "t.added_at, coalesce(t.release_disc, t.disc_number), t.track_number",
            TrackOrder::PlaysAscending => "plays, t.title COLLATE NOCASE",
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
    /// Its cover: the one chosen for its record, else its own disc's, else
    /// its record's.
    pub artwork_path: Option<String>,
    /// In the library and reachable now. A row that is not is drawn dimmed
    /// and does not play (ADR-017).
    pub available: bool,
    /// The codec the file's source says it is, for a track whose name has no
    /// extension: a server's.
    pub codec: Option<String>,
    /// Who wrote it, joined with commas; empty when nobody is named.
    pub composers: String,
    pub disc_subtitle: Option<String>,
    pub work: Option<String>,
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
    /// A source's glyph (`sources.icon`); empty for the other columns.
    pub icon: String,
}

/// A folder inside the library, as the Folders screen lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FolderEntry {
    pub name: String,
    pub path: String,
    /// Tracks under it, all the way down.
    pub tracks: usize,
    /// The cover its own files gave -- a disc of a box set has its own --
    /// or else one of its records', if any has one.
    pub artwork_path: Option<String>,
    /// How many folders sit in it.
    pub folders: usize,
    /// The record it is, when its files sit in it and are the whole of one.
    /// A disc of a box set holds part of a record, and is a folder.
    pub album_id: Option<i64>,
    /// The record it holds part of: a disc of a box set.
    pub part_of: Option<i64>,
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

/// Folder membership follows the physical file, while playback keeps the
/// track's own address. Both arms use indexed path ranges.
fn folder_tracks() -> String {
    format!("WITH folder_tracks AS (
        SELECT t.id, t.external_id AS location FROM tracks t
        WHERE {LOCAL} AND t.present = 1 AND t.external_id >= ?1 AND t.external_id < ?2
        UNION
        SELECT t.id, sf.container AS location FROM scanned_files sf JOIN tracks t ON t.id = sf.track_id
        WHERE {LOCAL} AND t.present = 1 AND sf.container >= ?1 AND sf.container < ?2
    )")
}

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
    /// Records in the order their first track appears in `ids`.
    pub album_ids: Vec<i64>,
    pub totals: Totals,
}

const ROW: &str = concat!("SELECT t.id, t.external_id, t.title, t.artist, a.title, t.album_id, coalesce(t.release_disc, t.disc_number),
            t.track_number, t.duration_ms, t.sample_rate_hz, t.bit_depth,
            COALESCE(r.plays, 0) AS plays, ", track_art!("t", "a"), ", (t.present = 1 AND t.reachable = 1), t.codec,
            (SELECT group_concat(c.name, ', ') FROM track_composers tc JOIN composers c ON c.id = tc.composer_id WHERE tc.track_id = t.id),
            t.disc_subtitle, t.work");

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
        available: r.get::<_, i64>(13)? == 1,
        codec: r.get(14)?,
        composers: r.get::<_, Option<String>>(15)?.unwrap_or_default(),
        disc_subtitle: r.get(16)?,
        work: r.get(17)?,
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
    for (column, query) in f.queries.iter().enumerate() {
        if query.trim().is_empty() { continue; }
        let pattern = format!("%{}%", query.trim().replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_"));
        sql.push_str(match column {
            0 => " AND CAST(a.year AS TEXT) LIKE ? ESCAPE '\\'",
            1 => " AND EXISTS (SELECT 1 FROM track_genres tq JOIN genres gq ON gq.id = tq.genre_id WHERE tq.track_id = t.id AND gq.name LIKE ? ESCAPE '\\')",
            2 => " AND t.label LIKE ? ESCAPE '\\'",
            3 => " AND EXISTS (SELECT 1 FROM track_composers cq JOIN composers cc ON cc.id = cq.composer_id WHERE cq.track_id = t.id AND cc.name LIKE ? ESCAPE '\\')",
            // A rating's header search narrows its column, not the tracks.
            4 => continue,
            5 => " AND a.id IN (SELECT album_id FROM album_artist_rollup
                WHERE artist LIKE ? ESCAPE '\\')",
            _ => " AND a.title LIKE ? ESCAPE '\\'",
        });
        args.push(Box::new(pattern));
    }
    let marks = |n: usize| vec!["?"; n].join(",");
    let mut years = f.years.clone();
    if let Some(year) = f.year.filter(|year| !years.contains(year)) { years.push(year); }
    if !years.is_empty() {
        sql.push_str(&format!(" AND a.year IN ({})", marks(years.len())));
        args.extend(years.into_iter().map(|v| Box::new(v) as Box<dyn ToSql>));
    }
    let mut genres = f.genres.clone();
    if let Some(genre) = f.genre.as_ref().filter(|genre| !genres.contains(genre)) { genres.push(genre.clone()); }
    if !genres.is_empty() {
        sql.push_str(&format!(" AND EXISTS (SELECT 1 FROM track_genres tg JOIN genres g ON g.id = tg.genre_id WHERE tg.track_id = t.id AND g.name IN ({}))", marks(genres.len())));
        args.extend(genres.into_iter().map(|v| Box::new(v) as Box<dyn ToSql>));
    }
    if !f.labels.is_empty() {
        sql.push_str(&format!(" AND t.label COLLATE NOCASE IN ({})", marks(f.labels.len())));
        args.extend(f.labels.iter().cloned().map(|v| Box::new(v) as Box<dyn ToSql>));
    }
    if !f.composers.is_empty() {
        sql.push_str(&format!(" AND EXISTS (SELECT 1 FROM track_composers tcf JOIN composers cf ON cf.id = tcf.composer_id WHERE tcf.track_id = t.id AND cf.name IN ({}))", marks(f.composers.len())));
        args.extend(f.composers.iter().cloned().map(|v| Box::new(v) as Box<dyn ToSql>));
    }
    let stars = |list: &Vec<u8>| list.iter().map(|v| Box::new(*v) as Box<dyn ToSql>).collect::<Vec<_>>();
    if !f.ratings.is_empty() {
        sql.push_str(&format!(" AND COALESCE((SELECT stars FROM album_ratings WHERE album_id = a.id), 0) IN ({})", marks(f.ratings.len())));
        args.extend(stars(&f.ratings));
    }
    if !f.track_ratings.is_empty() {
        sql.push_str(&format!(" AND COALESCE((SELECT stars FROM track_ratings WHERE track_id = t.id), 0) IN ({})", marks(f.track_ratings.len())));
        args.extend(stars(&f.track_ratings));
    }
    let mut artists = f.artists.clone();
    if let Some(artist) = f.artist.as_ref().filter(|artist| !artists.iter().any(|v| v.eq_ignore_ascii_case(artist))) { artists.push(artist.clone()); }
    if !artists.is_empty() {
        sql.push_str(&format!(" AND a.id IN (SELECT album_id FROM album_artist_rollup
            WHERE artist COLLATE NOCASE IN ({}))", marks(artists.len())));
        args.extend(artists.into_iter().map(|v| Box::new(v) as Box<dyn ToSql>));
    }
    let mut albums = f.albums.clone();
    if let Some(album) = f.album.filter(|album| !albums.contains(album)) { albums.push(album); }
    if !albums.is_empty() {
        sql.push_str(&format!(" AND a.id IN ({})", marks(albums.len())));
        args.extend(albums.into_iter().map(|v| Box::new(v) as Box<dyn ToSql>));
    }
    if !f.excluded_sources.is_empty() {
        sql.push_str(&format!(" AND t.source_id NOT IN (SELECT id FROM sources WHERE provider || ':' || instance IN ({}))", marks(f.excluded_sources.len())));
        args.extend(f.excluded_sources.iter().cloned().map(|v| Box::new(v) as Box<dyn ToSql>));
    }
    let ext = |name: &str| format!("lower(t.external_id) LIKE '%.{name}'");
    let format_clause = |name: &str| match name {
        "flac" => Some(ext("flac")),
        "alac" => Some(format!("({} OR {}) AND t.bit_depth IS NOT NULL", ext("m4a"), ext("mp4"))),
        "aac" => Some(format!("({} OR {} OR {}) AND t.bit_depth IS NULL", ext("aac"), ext("m4a"), ext("mp4"))),
        "mp3" => Some(ext("mp3")), "ogg" => Some(format!("({} OR {})", ext("ogg"), ext("oga"))),
        "opus" => Some(ext("opus")), "wav" => Some(ext("wav")),
        "aiff" => Some(format!("({} OR {})", ext("aif"), ext("aiff"))),
        "wavpack" => Some(ext("wv")), "ape" => Some(ext("ape")),
        "dsd" => Some(format!("({} OR {} OR {} OR {} OR t.sample_rate_hz >= 2822400)", ext("dsf"), ext("dff"), ext("dsd"), ext("iso"))),
        _ => None,
    };
    let lossy = format!("({} OR {} OR {} OR {} OR {} OR (({} OR {}) AND t.bit_depth IS NULL))", ext("mp3"), ext("aac"), ext("ogg"), ext("oga"), ext("opus"), ext("m4a"), ext("mp4"));
    let dsd = format!("({} OR {} OR {} OR {} OR t.sample_rate_hz >= 2822400)", ext("dsf"), ext("dff"), ext("dsd"), ext("iso"));
    if !f.qualities.is_empty() {
        let tiers: Vec<String> = f.qualities.iter().filter_map(|tier| match tier.as_str() {
            "lossy" => Some(lossy.clone()),
            "lossless" => Some(format!("NOT {lossy} AND NOT {dsd} AND (t.bit_depth IS NOT NULL OR t.sample_rate_hz IS NOT NULL) AND COALESCE(t.bit_depth, 0) < 24 AND COALESCE(t.sample_rate_hz, 0) <= 48000")),
            "hires" => Some(format!("NOT {lossy} AND NOT {dsd} AND (t.bit_depth >= 24 OR t.sample_rate_hz > 48000)")),
            "dsd" => Some(dsd.clone()), _ => None,
        }).collect();
        if !tiers.is_empty() { sql.push_str(&format!(" AND ({})", tiers.join(" OR "))); }
    }
    if !f.formats.is_empty() {
        let formats: Vec<String> = f.formats.iter().filter_map(|name| format_clause(name)).collect();
        if !formats.is_empty() { sql.push_str(&format!(" AND ({})", formats.join(" OR "))); }
    }
    for word in super::find::patterns(&f.text) {
        sql.push_str(" AND (t.title LIKE ? ESCAPE '\\' OR t.artist LIKE ? ESCAPE '\\' OR a.title LIKE ? ESCAPE '\\')");
        for _ in 0..3 {
            args.push(Box::new(word.clone()));
        }
    }
    if let Some(name) = &f.performer {
        sql.push_str(" AND t.id IN (SELECT track_id FROM album_members
            WHERE artist = ? COLLATE NOCASE)");
        args.push(Box::new(name.clone()));
    }
    (sql, args)
}

impl Library {
    /// Sources that currently contribute tracks to Library Explorer.
    pub fn facet_sources(&self) -> rusqlite::Result<Vec<FacetEntry>> {
        let mut stmt = self.db.prepare(
            "SELECT s.provider, s.instance, count(*), s.label, s.icon FROM tracks t JOIN sources s ON s.id = t.source_id
             WHERE t.present = 1 GROUP BY s.id ORDER BY s.provider COLLATE NOCASE, s.instance COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], |r| {
            let provider: String = r.get(0)?;
            let instance: String = r.get(1)?;
            let label: String = r.get(3)?;
            Ok(FacetEntry {
                key: format!("{provider}:{instance}"),
                // A server has a name; its instance is an opaque id nobody reads.
                label: if !label.is_empty() { label } else if instance == "default" { provider } else { instance },
                albums: r.get::<_, i64>(2)? as usize,
                year: None,
                icon: r.get(4)?,
            })
        })?;
        rows.collect()
    }

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
                icon: String::new(),
            })
        })?;
        rows.collect()
    }

    /// The genres the chosen year leaves, by name.
    pub fn facet_genres(&self, f: &Filter) -> rusqlite::Result<Vec<FacetEntry>> {
        let mut left = f.clone();
        left.queries[1..].fill(String::new());
        left.genre = None;
        left.genres.clear();
        left.labels.clear();
        left.composers.clear();
        left.ratings.clear();
        left.artist = None;
        left.artists.clear();
        left.album = None;
        left.albums.clear();
        let (filter, args) = clause(&left);
        let sql = format!(
            "SELECT g.name, count(DISTINCT a.id) {FROM}
             JOIN track_genres tg ON tg.track_id = t.id
             JOIN genres g ON g.id = tg.genre_id
             {filter} GROUP BY g.name ORDER BY g.name COLLATE NOCASE"
        );
        self.named_facet(&sql, args)
    }

    /// The labels the year and genre leave, by name.
    pub fn facet_labels(&self, f: &Filter) -> rusqlite::Result<Vec<FacetEntry>> {
        let mut left = f.clone();
        left.choose(2, &[]);
        left.queries[2..].fill(String::new());
        let (filter, args) = clause(&left);
        let sql = format!(
            "SELECT min(t.label), count(DISTINCT a.id) {FROM}
             {filter} AND t.label IS NOT NULL AND t.label <> ''
             GROUP BY t.label COLLATE NOCASE ORDER BY 1 COLLATE NOCASE"
        );
        self.named_facet(&sql, args)
    }

    /// The composers the year, genre and label leave, by name.
    pub fn facet_composers(&self, f: &Filter) -> rusqlite::Result<Vec<FacetEntry>> {
        let mut left = f.clone();
        left.choose(3, &[]);
        left.queries[3..].fill(String::new());
        let (filter, args) = clause(&left);
        let sql = format!(
            "SELECT c.name, count(DISTINCT a.id) {FROM}
             JOIN track_composers tc ON tc.track_id = t.id
             JOIN composers c ON c.id = tc.composer_id
             {filter} GROUP BY c.name ORDER BY c.name COLLATE NOCASE"
        );
        self.named_facet(&sql, args)
    }

    /// The stars of the records the year, genre, label and composer leave,
    /// the most first and the records with none last. Its label is the
    /// facade's to give.
    pub fn facet_ratings(&self, f: &Filter) -> rusqlite::Result<Vec<FacetEntry>> {
        let mut left = f.clone();
        left.choose(4, &[]);
        left.queries[4..].fill(String::new());
        let (filter, args) = clause(&left);
        let sql = format!(
            "SELECT COALESCE(ra.stars, 0) AS stars, count(DISTINCT a.id) {FROM}
             LEFT JOIN album_ratings ra ON ra.album_id = a.id
             {filter} GROUP BY stars ORDER BY stars DESC"
        );
        let mut stmt = self.db.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(args.iter()), |r| {
            Ok(FacetEntry {
                key: r.get::<_, i64>(0)?.to_string(),
                label: String::new(),
                albums: r.get::<_, i64>(1)? as usize,
                year: None,
                icon: String::new(),
            })
        })?;
        rows.collect()
    }

    /// Album credits and contributing artists the columns to their left leave.
    pub fn facet_artists(&self, f: &Filter) -> rusqlite::Result<Vec<FacetEntry>> {
        let mut left = f.clone();
        left.queries[5..].fill(String::new());
        left.artist = None;
        left.artists.clear();
        left.album = None;
        left.albums.clear();
        if left == Filter::default() {
            return self.named_facet("SELECT min(artist), count(album_id)
                FROM album_artist_rollup GROUP BY artist COLLATE NOCASE ORDER BY 1 COLLATE NOCASE", Vec::new());
        }
        let albums: Vec<i64> = self.facet_albums(&left)?.iter().filter_map(|a| a.key.parse().ok()).collect();
        if albums.is_empty() { return Ok(Vec::new()); }
        let placeholders = vec!["?"; albums.len()].join(",");
        let sql = format!("SELECT min(artist), count(album_id) FROM album_artist_rollup
            WHERE album_id IN ({placeholders}) GROUP BY artist COLLATE NOCASE ORDER BY 1 COLLATE NOCASE");
        let args = albums.into_iter().map(|id| Box::new(id) as Box<dyn ToSql>).collect();
        self.named_facet(&sql, args)
    }

    /// The albums the other three leave, in shelf order, with their year.
    pub fn facet_albums(&self, f: &Filter) -> rusqlite::Result<Vec<FacetEntry>> {
        let mut left = Filter { album: None, albums: Vec::new(), ..f.clone() };
        left.queries[6].clear();
        let (filter, args) = clause(&left);
        let sql = format!(
            "SELECT a.id, a.title, a.year, count(t.id) {FROM} {filter}
             GROUP BY a.id ORDER BY a.title COLLATE NOCASE, a.album_artist COLLATE NOCASE, a.year"
        );
        let mut stmt = self.db.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(args.iter()), |r| {
            Ok(FacetEntry {
                key: r.get::<_, i64>(0)?.to_string(),
                label: r.get(1)?,
                albums: 1,
                year: r.get(2)?,
                icon: String::new(),
            })
        })?;
        rows.collect()
    }

    fn named_facet(&self, sql: &str, args: Vec<Box<dyn ToSql>>) -> rusqlite::Result<Vec<FacetEntry>> {
        let mut stmt = self.db.prepare(sql)?;
        let rows = stmt.query_map(params_from_iter(args.iter()), |r| {
            let name: String = r.get(0)?;
            Ok(FacetEntry { key: name.clone(), label: name, albums: r.get::<_, i64>(1)? as usize, year: None, icon: String::new() })
        })?;
        rows.collect()
    }

    /// The snapshot of an order: every id the filter leaves, sorted, and
    /// the totals, in one pass. The costly half of browsing; the window
    /// Where each letter starts in a list of tracks: the first track whose
    /// record ("album"), artist ("artist") or title (anything else) begins
    /// with it. What does not begin with a letter from A to Z goes under
    /// "#". The list has to be in the order of what is asked about.
    pub fn letters(&self, ids: &[i64], by: &str) -> rusqlite::Result<Vec<(String, usize)>> {
        let named = match by {
            "album" => "a.title",
            "artist" => "t.artist",
            _ => "t.title",
        };
        let mut initial: std::collections::HashMap<i64, String> = std::collections::HashMap::new();
        let mut stmt = self.db.prepare(&format!("SELECT t.id, substr(trim({named}), 1, 1) {FROM} WHERE t.present = 1"))?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?.unwrap_or_default())))?;
        for row in rows {
            let (id, first) = row?;
            initial.insert(id, letter_of(&first));
        }
        let mut out: Vec<(String, usize)> = Vec::new();
        for (at, id) in ids.iter().enumerate() {
            let letter = initial.get(id).cloned().unwrap_or_else(|| "#".into());
            if !out.iter().any(|(seen, _)| *seen == letter) {
                out.push((letter, at));
            }
        }
        Ok(out)
    }

    /// runs it off its own thread.
    pub fn browse_ids(&self, f: &Filter, order: TrackOrder) -> rusqlite::Result<Snapshot> {
        let (filter, args) = clause(f);
        // The groups follow an order about what they are named: Artist
        // from Z puts the artists from Z.
        let turned = |on: bool| if on { " DESC" } else { "" };
        let sql = format!(
            "SELECT t.id, t.album_id, t.duration_ms, COALESCE(r.plays, 0) AS plays
             {FROM} {filter} ORDER BY {}{}",
            match f.grouped_by.as_str() {
                "album" => format!("a.title COLLATE NOCASE{}, a.id, ", turned(order == TrackOrder::AlbumDescending)),
                "artist" => format!("t.artist COLLATE NOCASE{}, ", turned(order == TrackOrder::ArtistDescending)),
                "name" => format!("{}{}, ", letter_in_sql("t.title"), turned(order == TrackOrder::TitleDescending)),
                _ => String::new(),
            },
            order.keys()
        );
        let mut stmt = self.db.prepare(&sql)?;
        let mut rows = stmt.query(params_from_iter(args.iter()))?;
        let mut out = Snapshot::default();
        let mut albums = std::collections::HashSet::new();
        while let Some(r) = rows.next()? {
            out.ids.push(r.get(0)?);
            if let Some(album) = r.get::<_, Option<i64>>(1)? {
                if albums.insert(album) {
                    out.album_ids.push(album);
                }
            }
            out.totals.duration_ms += r.get::<_, i64>(2)? as u64;
        }
        out.totals.tracks = out.ids.len();
        out.totals.albums = albums.len();
        Ok(out)
    }

    /// Dates needed to merge a local snapshot with source library rows.
    pub fn track_dates(&self, ids: &[i64]) -> rusqlite::Result<std::collections::HashMap<i64, (i32, i64)>> {
        let mut dates = std::collections::HashMap::new();
        for chunk in ids.chunks(500) {
            let marks = vec!["?"; chunk.len()].join(",");
            let mut stmt = self.db.prepare(&format!("SELECT t.id, COALESCE(a.year, 0), t.added_at FROM tracks t LEFT JOIN albums a ON a.id=t.album_id WHERE t.id IN ({marks})"))?;
            for row in stmt.query_map(params_from_iter(chunk.iter()), |r| Ok((r.get(0)?, (r.get(1)?,r.get(2)?))))? { let (id, pair) = row?; dates.insert(id, pair); }
        }
        Ok(dates)
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
            concat!("{} SELECT substr(ft.location, ?3, instr(substr(ft.location, ?3), '/') - 1) AS name,
                    count(*), coalesce(max(nullif(t.release_artwork, '')), max(", album_art!("a"), ")),
                    count(DISTINCT CASE WHEN instr(substr(substr(ft.location, ?3), instr(substr(ft.location, ?3), '/') + 1), '/') > 0
                          THEN substr(substr(substr(ft.location, ?3), instr(substr(ft.location, ?3), '/') + 1), 1,
                               instr(substr(substr(ft.location, ?3), instr(substr(ft.location, ?3), '/') + 1), '/') - 1) END),
                    count(DISTINCT coalesce(t.album_id, -t.id)), min(t.album_id)
             FROM tracks t JOIN folder_tracks ft ON ft.id = t.id LEFT JOIN albums a ON a.id = t.album_id
             WHERE instr(substr(ft.location, ?3), '/') > 0
             GROUP BY name ORDER BY name COLLATE NOCASE"), folder_tracks()
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
                    folders: r.get::<_, i64>(3)? as usize,
                    album_id: match (r.get::<_, i64>(3)?, r.get::<_, i64>(4)?) {
                        (0, 1) => r.get(5)?,
                        _ => None,
                    },
                    part_of: None,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut folders = folders;
        let whole = self.album_track_counts(folders.iter().filter_map(|f| f.album_id))?;
        for folder in &mut folders {
            if folder.album_id.is_some_and(|id| whole.get(&id).is_some_and(|&all| all > folder.tracks)) {
                folder.part_of = folder.album_id.take();
            }
        }
        let sql = format!(
            "{} {ROW} {FROM} JOIN folder_tracks ft ON ft.id = t.id WHERE instr(substr(ft.location, ?3), '/') = 0
             ORDER BY coalesce(t.release_disc, t.disc_number), t.track_number, t.external_id", folder_tracks()
        );
        let mut stmt = self.db.prepare(&sql)?;
        let tracks = stmt.query_map(rusqlite::params![from, to, cut], row)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok((folders, tracks))
    }

    /// How many tracks of the library each of those records has.
    fn album_track_counts(&self, ids: impl Iterator<Item = i64>) -> rusqlite::Result<std::collections::HashMap<i64, usize>> {
        let ids: Vec<i64> = ids.collect();
        let mut counts = std::collections::HashMap::new();
        for chunk in ids.chunks(500) {
            let marks = vec!["?"; chunk.len()].join(",");
            let sql = format!("SELECT t.album_id, count(*) FROM tracks t WHERE {LOCAL} AND t.present = 1 AND t.album_id IN ({marks}) GROUP BY t.album_id");
            let mut stmt = self.db.prepare(&sql)?;
            for row in stmt.query_map(params_from_iter(chunk.iter()), |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)? as usize)))? {
                let (id, count) = row?;
                counts.insert(id, count);
            }
        }
        Ok(counts)
    }

    /// How many tracks there are under `dir`, all the way down.
    pub fn folder_track_count(&self, dir: &str) -> rusqlite::Result<usize> {
        let (from, to, _) = under(dir);
        let sql = format!(
            "{} SELECT count(*) FROM folder_tracks", folder_tracks()
        );
        self.db.query_row(&sql, rusqlite::params![from, to], |r| r.get::<_, i64>(0)).map(|n| n as usize)
    }

    /// Every file under `dir`, all the way down, in path order: what playing
    /// a folder queues.
    pub fn folder_paths(&self, dir: &str) -> rusqlite::Result<Vec<String>> {
        let (from, to, _) = under(dir);
        let sql = format!(
            "{} SELECT t.external_id FROM tracks t JOIN folder_tracks ft ON ft.id = t.id
             ORDER BY ft.location, coalesce(t.release_disc, t.disc_number), t.track_number, t.external_id", folder_tracks()
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
    /// The label most of a record's tracks name, if any does.
    /// The labels of these records, each once whatever its capitals, the
    /// one on most of their tracks first.
    pub fn albums_labels(&self, ids: &[i64]) -> rusqlite::Result<Vec<String>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let marks = vec!["?"; ids.len()].join(",");
        self.db
            .prepare(&format!(
                "SELECT min(label) FROM tracks
                 WHERE album_id IN ({marks}) AND present = 1 AND label IS NOT NULL AND label <> ''
                 GROUP BY label COLLATE NOCASE ORDER BY count(*) DESC, 1"
            ))?
            .query_map(rusqlite::params_from_iter(ids.iter()), |r| r.get(0))?
            .collect()
    }

    pub fn album_label(&self, album_id: i64) -> rusqlite::Result<Option<String>> {
        use rusqlite::OptionalExtension;
        self.db
            .query_row(
                "SELECT min(label) FROM tracks
                 WHERE album_id = ?1 AND present = 1 AND label IS NOT NULL AND label <> ''
                 GROUP BY label COLLATE NOCASE ORDER BY count(*) DESC, 1 LIMIT 1",
                [album_id],
                |r| r.get(0),
            )
            .optional()
    }

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
    fn an_artists_records_name_their_labels_once_the_commonest_first() {
        let lib = crate::store::fixtures::sample_library();
        lib.db.execute("UPDATE tracks SET label = 'Fantasy' WHERE album_id IN (1, 3)", []).unwrap();
        lib.db.execute("UPDATE tracks SET label = 'chrysalis' WHERE album_id = 2", []).unwrap();
        assert_eq!(lib.albums_labels(&[1, 2, 3]).unwrap(), vec!["Fantasy".to_string(), "chrysalis".to_string()]);
        assert!(lib.albums_labels(&[]).unwrap().is_empty());
    }

    #[test]
    fn each_column_offers_only_what_the_ones_to_its_left_leave() {
        let lib = shelf();
        assert_eq!(labels(lib.facet_years().unwrap()), ["2021", "2019", "2018"], "newest first");
        assert_eq!(labels(lib.facet_genres(&Filter::default()).unwrap()), ["Alternative", "Latin"]);
        assert_eq!(labels(lib.facet_genres(&Filter { years: vec![2021], ..Filter::default() }).unwrap()), ["Latin"]);
        assert_eq!(labels(lib.facet_artists(&Filter { genres: vec!["Alternative".into()], ..Filter::default() }).unwrap()), ["Mora Vale"]);
        let f = Filter { artist: Some("Mora Vale".into()), ..Filter::default() };
        assert_eq!(labels(lib.facet_albums(&f).unwrap()), ["Low Orbit", "Northbound"], "by name");
    }

    #[test]
    fn records_are_counted_and_chosen_by_their_stars_and_tracks_by_theirs() {
        use crate::store::ratings::RatingTarget;
        let lib = shelf();
        lib.rate(RatingTarget::Album(1), 5).unwrap();
        lib.rate(RatingTarget::Album(2), 3).unwrap();
        lib.rate(RatingTarget::Album(3), 0).unwrap();
        let counts = |f: &Filter| lib.facet_ratings(f).unwrap().into_iter().map(|e| (e.key, e.albums)).collect::<Vec<_>>();
        let pair = |stars: &str, albums| (stars.to_string(), albums);
        assert_eq!(counts(&Filter::default()), [pair("5", 1), pair("3", 1), pair("0", 1)], "a cleared rating is none");
        assert_eq!(counts(&Filter { years: vec![2021], ..Filter::default() }), [pair("3", 1)], "narrowed by the columns to its left");
        let mut chosen = Filter::default();
        chosen.choose(4, &["5".into(), "0".into()]);
        assert_eq!(lib.browse_ids(&chosen, TrackOrder::Artist).unwrap().totals.albums, 2);
        chosen.choose(5, &["Mora Vale".into()]);
        assert_eq!(chosen.ratings, [5, 0], "a column to its right keeps it");
        chosen.choose(1, &["Latin".into()]);
        assert!(chosen.ratings.is_empty(), "a column to its left clears it");
        lib.rate(RatingTarget::Track(10), 4).unwrap();
        let four = Filter { track_ratings: vec![4], ..Filter::default() };
        assert_eq!(lib.browse_ids(&four, TrackOrder::Artist).unwrap().totals.tracks, 1);
        let none = Filter { track_ratings: vec![0], ..Filter::default() };
        assert_eq!(lib.browse_ids(&none, TrackOrder::Artist).unwrap().totals.tracks, 4);
    }

    #[test]
    fn multi_selection_and_quality_filters_reach_the_snapshot() {
        let lib = shelf();
        let years = Filter { years: vec![2018, 2021], ..Filter::default() };
        assert_eq!(lib.browse_ids(&years, TrackOrder::Artist).unwrap().totals.tracks, 2);
        let hires = Filter { qualities: vec!["hires".into()], ..Filter::default() };
        assert_eq!(lib.browse_ids(&hires, TrackOrder::Artist).unwrap().totals.tracks, 3);
        let format_filter = Filter { formats: vec!["flac".into()], ..Filter::default() };
        assert_eq!(lib.browse_ids(&format_filter, TrackOrder::Artist).unwrap().totals.tracks, 5);
        let sources = lib.facet_sources().unwrap();
        assert_eq!((sources[0].key.as_str(), sources[0].albums), ("local:default", 5));
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
        let artists = lib.facet_artists(&Filter::default()).unwrap();
        assert_eq!(artists.iter().filter(|a| a.label.eq_ignore_ascii_case("mora vale")).count(), 1);
        let f = Filter { artist: Some("mora vale".into()), ..Filter::default() };
        assert_eq!(lib.facet_albums(&f).unwrap().len(), 3, "both spellings' records");
    }

    #[test]
    fn a_facet_counts_albums_not_tracks() {
        let lib = shelf();
        let genres = lib.facet_genres(&Filter::default()).unwrap();
        assert_eq!(genres[0].albums, 2, "Alternative is two records, four tracks");
    }

    #[test]
    fn a_snapshot_knows_the_size_before_any_row_is_read() {
        let lib = shelf();
        let all = lib.browse_ids(&Filter::default(), TrackOrder::Artist).unwrap();
        assert_eq!(all.totals, Totals { tracks: 5, albums: 3, duration_ms: 5000 });
        assert_eq!(all.ids.len(), 5);
        assert_eq!(all.album_ids, [3, 1, 2], "albums follow their first track in snapshot order");
        let one = lib.browse_ids(&Filter { album: Some(1), ..Filter::default() }, TrackOrder::Disc).unwrap();
        assert_eq!((one.totals.tracks, one.totals.albums), (3, 1));
        assert_eq!(one.ids, [10, 12, 11], "disc 1 first, then disc 2");
        assert_eq!(one.album_ids, [1]);
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
        assert_eq!((folders[0].folders, folders[0].album_id), (0, None), "a disc holds part of its record, and is a folder");
        let (top, _) = lib.folder_children("/m").unwrap();
        let box_folder = top.iter().find(|f| f.name == "Box").unwrap();
        assert_eq!((box_folder.folders, box_folder.album_id), (2, None), "a folder of folders is a folder");
        assert_eq!(tracks.iter().map(|t| t.id).collect::<Vec<_>>(), [4], "only what sits in it");
        assert_eq!(lib.folder_track_count("/m/Box").unwrap(), 4, "Box0 is not under Box");
        assert_eq!(lib.folder_paths("/m/Box/").unwrap().len(), 4, "a trailing slash is the same folder");
    }

    #[test]
    fn a_track_row_shows_its_own_disc_s_cover_unless_one_was_chosen_for_its_record() {
        let lib = Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        lib.db.execute("INSERT INTO albums (id, title, album_artist, artwork_path) VALUES (1, 'Box', 'X', '/covers/box.jpg')", []).unwrap();
        for (id, path, own) in [(1, "/m/Box/Disc 1/01.flac", "/covers/disc-1.jpg"), (2, "/m/Box/Disc 2/01.flac", "/covers/disc-2.jpg"), (3, "/m/Box/Disc 3/01.flac", "")] {
            lib.db
                .execute(
                    "INSERT INTO tracks (id, source_id, external_id, title, album_id, added_at, release_artwork) VALUES (?1, ?2, ?3, 'T', 1, 1, ?4)",
                    params![id, source, path, own],
                )
                .unwrap();
        }
        let covers = |lib: &Library| -> Vec<Option<String>> { lib.tracks_by_ids(&[1, 2, 3]).unwrap().into_iter().map(|t| t.artwork_path).collect() };
        assert_eq!(covers(&lib), [Some("/covers/disc-1.jpg".into()), Some("/covers/disc-2.jpg".into()), Some("/covers/box.jpg".into())],
            "each disc's own, and the box's for a disc with none");
        lib.db.execute("INSERT INTO album_artwork (album_id, path) VALUES (1, '/covers/chosen.jpg')", []).unwrap();
        assert!(covers(&lib).iter().all(|cover| cover.as_deref() == Some("/covers/chosen.jpg")), "a cover chosen for the record is the record's everywhere");
    }

    #[test]
    fn a_folder_that_holds_a_whole_record_is_that_record() {
        let lib = Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        lib.db.execute("INSERT INTO albums (id, title, album_artist) VALUES (1, 'Box', 'X'), (2, 'Single', 'Y')", []).unwrap();
        for (id, path, album) in [(1, "/m/Box/Disc 1/01.flac", 1), (2, "/m/Box/Disc 2/01.flac", 1), (3, "/m/Single/01.flac", 2), (4, "/m/Single/02.flac", 2)] {
            lib.db
                .execute(
                    "INSERT INTO tracks (id, source_id, external_id, title, album_id, added_at) VALUES (?1, ?2, ?3, 'T', ?4, 1)",
                    params![id, source, path, album],
                )
                .unwrap();
        }
        lib.db.execute("UPDATE albums SET artwork_path = '/covers/box.jpg' WHERE id = 1", []).unwrap();
        lib.db.execute("UPDATE tracks SET release_artwork = '/covers/disc-2.jpg' WHERE id = 2", []).unwrap();
        let (discs, _) = lib.folder_children("/m/Box").unwrap();
        assert_eq!(discs.iter().map(|f| f.album_id).collect::<Vec<_>>(), [None, None], "each disc is a folder of its own");
        assert_eq!(discs[1].artwork_path.as_deref(), Some("/covers/disc-2.jpg"), "a disc shows the cover its own files gave");
        assert_eq!(discs[0].artwork_path.as_deref(), Some("/covers/box.jpg"), "and without one, the record's");
        assert_eq!(discs.iter().map(|f| f.part_of).collect::<Vec<_>>(), [Some(1), Some(1)], "and part of the box");
        let (top, _) = lib.folder_children("/m").unwrap();
        let single = top.iter().find(|f| f.name == "Single").unwrap();
        assert_eq!(single.album_id, Some(2), "every track of it sits in it");
    }

    #[test]
    fn the_record_the_arrival_and_the_plays_turn_around_too() {
        let lib = shelf();
        let ids = |order: &str| lib.browse_tracks(&Filter::default(), TrackOrder::from_name(order), 10).unwrap().into_iter().map(|t| t.id).collect::<Vec<_>>();
        assert_eq!(ids("album-desc"), [30, 10, 12, 11, 20], "Northbound, Low Orbit in its own order, Afterimage");
        assert_eq!(ids("added-asc"), [10, 11, 12, 20, 30]);
        assert_eq!(ids("added"), [30, 20, 12, 11, 10]);
        assert_eq!(ids("plays-asc").last(), Some(&20), "the most played last");
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

/// The letters that are filed under another: an accent does not make a
/// letter of its own.
const UNDER: &[(char, &str)] = &[
    ('A', "ÁÀÂÄÃÅáàâäãå"),
    ('E', "ÉÈÊËéèêë"),
    ('I', "ÍÌÎÏíìîï"),
    ('O', "ÓÒÔÖÕóòôöõ"),
    ('U', "ÚÙÛÜúùûü"),
    ('N', "Ññ"),
    ('C', "Çç"),
];

/// The letter a name is filed under: its first, from A to Z, without its
/// accent, and "#" for anything else.
pub fn letter_of(name: &str) -> String {
    let Some(first) = name.trim().chars().next() else { return "#".to_string() };
    let plain = UNDER.iter().find(|(_, accented)| accented.contains(first)).map_or(first, |(letter, _)| *letter).to_ascii_uppercase();
    if plain.is_ascii_uppercase() { plain.to_string() } else { "#".to_string() }
}

/// The same letter, worked out by the database for the text of a column, so
/// a list can be ordered by it. "#" sorts before "A".
pub fn letter_in_sql(column: &str) -> String {
    let first = format!("substr(ltrim({column}), 1, 1)");
    let accents: String = UNDER
        .iter()
        .map(|(letter, accented)| {
            let list: Vec<String> = accented.chars().map(|c| format!("'{c}'")).collect();
            format!(" WHEN {first} IN ({}) THEN '{letter}'", list.join(", "))
        })
        .collect();
    format!("CASE{accents} WHEN upper({first}) BETWEEN 'A' AND 'Z' THEN upper({first}) ELSE '#' END")
}

#[cfg(test)]
mod tracks_view_tests {
    use super::*;
    use rusqlite::params;

    fn library() -> Library {
        let lib = Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        let rows = [(1, "  Zebra", "Alpha Band", "First"), (2, "apple", "beta Band", "Second"), (3, "\u{c1}rbol", "Alpha Band", "First"), (4, "99 Problems", "Gamma", "Third")];
        for (id, title, artist, album) in rows {
            lib.db.execute("INSERT OR IGNORE INTO albums (id, title, album_artist, folder) VALUES (?1, ?2, ?3, ?2)", params![id, album, artist]).unwrap();
            let album_id: i64 = lib.db.query_row("SELECT id FROM albums WHERE title = ?1", params![album], |r| r.get(0)).unwrap();
            lib.db
                .execute("INSERT INTO tracks (id, source_id, external_id, title, artist, album_id, added_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1)", params![id, source, format!("/{id}.flac"), title, artist, album_id])
                .unwrap();
        }
        lib
    }

    #[test]
    fn typed_words_each_have_to_be_in_the_title_the_artist_or_the_record() {
        let lib = library();
        let found = |text: &str| lib.browse_ids(&Filter { text: text.into(), ..Filter::default() }, TrackOrder::Title).unwrap().ids;
        assert_eq!(found("alpha"), [1, 3]);
        assert_eq!(found("alpha zeb"), [1], "a title with spaces before it is in its place");
        assert_eq!(found("second"), [2]);
        assert_eq!(found("100%"), Vec::<i64>::new(), "a percent sign is a character");
        assert_eq!(found("").len(), 4);
    }

    #[test]
    fn a_list_can_run_backwards_and_by_who_plays() {
        let lib = library();
        let ids = |order| lib.browse_ids(&Filter::default(), order).unwrap().ids;
        assert_eq!(ids(TrackOrder::TitleDescending), [3, 1, 2, 4]);
        assert_eq!(ids(TrackOrder::from_name("performer")), [1, 3, 2, 4]);
    }

    #[test]
    fn a_list_can_run_from_the_shortest_or_from_the_longest() {
        let lib = library();
        for (id, ms) in [(1, 3000), (2, 1000), (3, 1000), (4, 2000)] {
            lib.db.execute("UPDATE tracks SET duration_ms = ?2 WHERE id = ?1", params![id, ms]).unwrap();
        }
        let ids = |order| lib.browse_ids(&Filter::default(), TrackOrder::from_name(order)).unwrap().ids;
        assert_eq!(ids("duration"), [2, 3, 4, 1], "two of a length, by title");
        assert_eq!(ids("duration-desc"), [1, 4, 2, 3]);
    }

    #[test]
    fn each_letter_says_where_it_starts() {
        let lib = library();
        let ids = lib.browse_ids(&Filter::default(), TrackOrder::Title).unwrap().ids;
        // A letter with an accent sorts after the alphabet, as everywhere
        // in the library, and is still filed under its letter.
        assert_eq!(ids, [4, 2, 1, 3]);
        let letters = lib.letters(&ids, "title").unwrap();
        assert_eq!(letters, [("#".to_string(), 0), ("A".to_string(), 1), ("Z".to_string(), 2)]);
        assert_eq!(letter_of("\u{f1}u"), "N");
        assert_eq!(letter_of("zebra"), "Z");
        assert_eq!(letter_of("  "), "#");
    }

    #[test]
    fn a_grouped_list_keeps_the_order_asked_for_inside_each_group() {
        let lib = library();
        let ids = |by: &str, order| lib.browse_ids(&Filter { grouped_by: by.into(), ..Filter::default() }, order).unwrap().ids;
        // By artist: Alpha Band's two, then beta Band, then Gamma. Inside
        // Alpha Band, the titles forwards or backwards.
        assert_eq!(ids("artist", TrackOrder::Title), [1, 3, 2, 4]);
        assert_eq!(ids("artist", TrackOrder::TitleDescending), [3, 1, 2, 4]);
        // By letter: "#", then A with its accent, then Z.
        assert_eq!(ids("name", TrackOrder::Title), [4, 2, 3, 1]);
        // An order about what the groups are named turns the groups too:
        // Gamma, beta Band, then Alpha Band's two.
        assert_eq!(ids("artist", TrackOrder::ArtistDescending)[..2], [4, 2]);
        assert_eq!(ids("name", TrackOrder::TitleDescending)[0], 1, "Z first");
    }
}
