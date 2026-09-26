//! What the window's lists and screens read.
//!
//! Rows by id, a record's detail, the shelves of the home screen, and the
//! [`Reader`]: a second connection to the library that the window's worker
//! owns, for the queries that do not fit in a frame (the snapshot of an
//! order, the unfiltered genre column).

use crate::quality::Quality;
use crate::{Album, Chilpa};
pub use chilpa_library::{FacetEntry, Filter, Snapshot, Totals, TrackOrder};

/// A track as a list draws it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrackRow {
    pub id: i64,
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    /// Zero when the track belongs to no album.
    pub album_id: i64,
    /// Zero when the file does not say.
    pub disc: u16,
    pub number: u16,
    pub duration_ms: u64,
    pub plays: usize,
    pub quality: Quality,
    /// Its record's cover, empty if there is none.
    pub artwork_path: String,
}

fn row_of(t: chilpa_library::BrowseTrack) -> TrackRow {
    TrackRow {
        quality: Quality::of_file(&t.external_id, t.bit_depth, t.sample_rate_hz),
        id: t.id,
        path: t.external_id,
        title: t.title,
        artist: t.artist,
        album: t.album,
        album_id: t.album_id.unwrap_or(0),
        disc: t.disc_number.unwrap_or(0),
        number: t.track_number.unwrap_or(0),
        duration_ms: t.duration_ms,
        plays: t.plays,
        artwork_path: t.artwork_path.unwrap_or_default(),
    }
}

fn album_of((a, listens): (chilpa_library::StoredAlbum, usize)) -> Album {
    Album {
        id: a.id,
        title: a.title,
        artist: a.album_artist,
        year: a.year,
        track_count: a.track_count,
        artwork_path: a.artwork_path.unwrap_or_default(),
        play_count: listens,
    }
}

/// The four facet columns, each narrowed by the ones to its left.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Facets {
    pub years: Vec<FacetEntry>,
    pub genres: Vec<FacetEntry>,
    pub artists: Vec<FacetEntry>,
    pub albums: Vec<FacetEntry>,
}

/// What a search found, best first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Found {
    pub artists: Vec<crate::Artist>,
    pub albums: Vec<Album>,
    pub tracks: Vec<TrackRow>,
}

fn artist_of(a: chilpa_library::StoredArtist) -> crate::Artist {
    crate::Artist {
        name: a.name,
        album_count: a.album_count,
        track_count: a.track_count,
        artwork_path: a.artwork_path.unwrap_or_default(),
    }
}

/// A folder as the Folders screen shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FolderView {
    pub folders: Vec<FolderItem>,
    pub tracks: Vec<TrackRow>,
    /// False for a folder outside the library, read from the disk as it is.
    pub in_library: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FolderItem {
    pub name: String,
    pub path: String,
    /// Tracks under it, all the way down; zero when it is not known.
    pub tracks: usize,
    pub artwork_path: String,
}

/// A row for a file the library does not have: its name, and what its
/// extension says.
pub(crate) fn file_row(path: &str) -> TrackRow {
    TrackRow {
        title: crate::title_of(std::path::Path::new(path)),
        quality: Quality::of_file(path, None, None),
        id: 0,
        path: path.to_string(),
        artist: String::new(),
        album: String::new(),
        album_id: 0,
        disc: 0,
        number: 0,
        duration_ms: 0,
        plays: 0,
        artwork_path: String::new(),
    }
}

/// A record as its page shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AlbumDetail {
    pub album: Album,
    /// The genre most of its tracks carry; empty if none does.
    pub genre: String,
    pub duration_ms: u64,
    /// How many discs it spans; one when the files do not say.
    pub discs: usize,
    /// The best of its files, which is what the header states.
    pub quality: Quality,
}

/// A read-only connection of its own, for a thread of its own.
///
/// The library is in WAL mode, so this reads while the window's connection
/// or a scan writes.
pub struct Reader {
    lib: chilpa_library::Library,
}

impl Reader {
    /// Every id the filter leaves, in that order, with the totals.
    pub fn snapshot(&self, filter: &Filter, order: TrackOrder) -> Snapshot {
        self.lib.browse_ids(filter, order).unwrap_or_default()
    }

    /// The files behind those ids, in order: what a list queues when one
    /// of its rows is played. Read in chunks, so a snapshot of the whole
    /// library resolves without one statement holding thousands of
    /// parameters.
    pub fn paths(&self, ids: &[i64]) -> Vec<String> {
        ids.chunks(500)
            .flat_map(|chunk| self.lib.tracks_by_ids(chunk).unwrap_or_default())
            .map(|t| t.external_id)
            .collect()
    }

    /// Every artist with something present, by name, with their records,
    /// tracks and a cover of theirs.
    pub fn artists(&self) -> Vec<crate::Artist> {
        self.lib.artists(1_000_000, 0).unwrap_or_default().into_iter().map(artist_of).collect()
    }

    /// What the search box finds for `text`: artists, records and songs, up
    /// to `limit` of each, best first.
    pub fn find(&self, text: &str, limit: usize) -> Found {
        let found = self.lib.find(text, limit).unwrap_or_default();
        Found {
            artists: found.artists.into_iter().map(artist_of).collect(),
            albums: self.lib.albums_by_ids(&found.albums).unwrap_or_default().into_iter().map(album_of).collect(),
            tracks: self.lib.tracks_by_ids(&found.tracks).unwrap_or_default().into_iter().map(row_of).collect(),
        }
    }

    /// Every album with something present, in that order.
    pub fn album_ids(&self, order: chilpa_library::AlbumSort) -> Vec<i64> {
        self.lib.album_ids(order).unwrap_or_default()
    }

    /// A folder: the library's roots for an empty path, what the library
    /// has under a folder inside it, or what the disk has in one outside it.
    pub fn folder(&self, dir: &str) -> FolderView {
        let roots = self.lib.roots().unwrap_or_default();
        if dir.is_empty() {
            let folders = roots
                .iter()
                .map(|r| {
                    let path = r.path.to_string_lossy().into_owned();
                    FolderItem {
                        name: path.clone(),
                        tracks: self.lib.folder_track_count(&path).unwrap_or(0),
                        artwork_path: String::new(),
                        path,
                    }
                })
                .collect();
            return FolderView { folders, tracks: Vec::new(), in_library: true };
        }
        let inside = roots.iter().any(|r| std::path::Path::new(dir).starts_with(&r.path));
        if inside {
            let (folders, tracks) = self.lib.folder_children(dir).unwrap_or_default();
            return FolderView {
                folders: folders
                    .into_iter()
                    .map(|f| FolderItem {
                        name: f.name,
                        path: f.path,
                        tracks: f.tracks,
                        artwork_path: f.artwork_path.unwrap_or_default(),
                    })
                    .collect(),
                tracks: tracks.into_iter().map(row_of).collect(),
                in_library: true,
            };
        }
        crate::folders::disk_folder(std::path::Path::new(dir))
    }

    /// Every playable file under a folder, in order: the library's for one
    /// inside it, the disk's for one outside.
    pub fn folder_files(&self, dir: &str) -> Vec<String> {
        let roots = self.lib.roots().unwrap_or_default();
        if roots.iter().any(|r| std::path::Path::new(dir).starts_with(&r.path)) {
            return self.lib.folder_paths(dir).unwrap_or_default();
        }
        let files = crate::folders::audio_files_in(std::path::Path::new(dir));
        chilpa_core::sacd_tracks::expand(&files).iter().map(|p| p.display().to_string()).collect()
    }

    /// The four columns for this filter. Each column is narrowed only by
    /// the ones to its left, so choosing an artist does not empty the year
    /// column.
    pub fn facets(&self, f: &Filter) -> Facets {
        let genre = f.genre.as_deref();
        Facets {
            years: self.lib.facet_years().unwrap_or_default(),
            genres: self.lib.facet_genres(f.year).unwrap_or_default(),
            artists: self.lib.facet_artists(f.year, genre).unwrap_or_default(),
            albums: self.lib.facet_albums(f).unwrap_or_default(),
        }
    }
}

impl Chilpa {
    /// A second connection to the library, for the window's worker. `None`
    /// if the library cannot be opened, which the window's own connection
    /// has already reported.
    pub fn reader(&self) -> Option<Reader> {
        chilpa_core::open_library().ok().map(|lib| Reader { lib })
    }

    /// The rows of those ids, in that order. By primary key: the same cost
    /// anywhere in a snapshot.
    pub fn track_rows(&mut self, ids: &[i64]) -> Vec<TrackRow> {
        let Some(lib) = self.library() else { return Vec::new() };
        lib.tracks_by_ids(ids).unwrap_or_default().into_iter().map(row_of).collect()
    }

    /// Those albums, in the order given. By primary key.
    pub fn albums_by_ids(&mut self, ids: &[i64]) -> Vec<Album> {
        let Some(lib) = self.library() else { return Vec::new() };
        lib.albums_by_ids(ids).unwrap_or_default().into_iter().map(album_of).collect()
    }

    /// The queue from what plays on, as rows: what plays first, then what
    /// comes next. A file the library does not have gets a row from its
    /// name, so anything queued shows.
    pub fn queue_rows(&mut self) -> Vec<TrackRow> {
        let from = self.now.index.min(self.queue.len());
        let paths: Vec<String> = self.queue[from..].to_vec();
        let removed: Vec<bool> = (from..self.queue.len()).map(|i| self.removed.get(i).copied().unwrap_or(false)).collect();
        let Some(lib) = self.library() else { return Vec::new() };
        let known = lib.tracks_by_paths(&paths).unwrap_or_else(|_| vec![None; paths.len()]);
        paths
            .into_iter()
            .zip(known)
            .zip(removed)
            .filter(|(_, gone)| !gone)
            .map(|((path, row), _)| match row {
                Some(t) => row_of(t),
                None => file_row(&path),
            })
            .collect()
    }

    /// An artist's records, newest first.
    pub fn artist_albums(&mut self, name: &str) -> Vec<Album> {
        let Some(lib) = self.library() else { return Vec::new() };
        lib.albums_filtered(chilpa_library::AlbumSort::Year, Some(name), 10_000, 0)
            .unwrap_or_default()
            .into_iter()
            .map(album_of)
            .collect()
    }

    /// An artist's tracks, in that order, as many as asked for.
    pub fn artist_tracks(&mut self, name: &str, order: TrackOrder, limit: usize) -> Vec<TrackRow> {
        let Some(lib) = self.library() else { return Vec::new() };
        let f = Filter { artist: Some(name.to_string()), ..Filter::default() };
        lib.browse_tracks(&f, order, limit).unwrap_or_default().into_iter().map(row_of).collect()
    }

    /// A record's tracks, in disc order.
    pub fn album_track_rows(&mut self, album_id: i64) -> Vec<TrackRow> {
        let Some(lib) = self.library() else { return Vec::new() };
        let f = Filter { album: Some(album_id), ..Filter::default() };
        lib.browse_tracks(&f, TrackOrder::Disc, 10_000).unwrap_or_default().into_iter().map(row_of).collect()
    }

    /// The most played tracks. What never played is not "most played".
    pub fn most_played_rows(&mut self, limit: usize) -> Vec<TrackRow> {
        let Some(lib) = self.library() else { return Vec::new() };
        lib.browse_tracks(&Filter::default(), TrackOrder::Plays, limit)
            .unwrap_or_default()
            .into_iter()
            .filter(|t| t.plays > 0)
            .map(row_of)
            .collect()
    }

    /// The records played last. One nobody has played is not among them.
    pub fn recently_played_albums(&mut self, limit: usize) -> Vec<Album> {
        let mut albums = self.albums_by(chilpa_library::AlbumSort::LastPlayed, limit);
        albums.retain(|a| a.play_count > 0);
        albums
    }

    /// The records that came into the library last.
    pub fn recently_added_albums(&mut self, limit: usize) -> Vec<Album> {
        self.albums_by(chilpa_library::AlbumSort::Added, limit)
    }

    fn albums_by(&mut self, order: chilpa_library::AlbumSort, limit: usize) -> Vec<Album> {
        let Some(lib) = self.library() else { return Vec::new() };
        lib.albums_filtered(order, None, limit, 0).unwrap_or_default().into_iter().map(album_of).collect()
    }

    /// A record with what its header states.
    pub fn album_detail(&mut self, album_id: i64) -> Option<AlbumDetail> {
        let album = self.album(album_id)?;
        let rows = self.album_track_rows(album_id);
        let genre = self.library()?.album_genre(album_id).ok().flatten().unwrap_or_default();
        let mut discs: Vec<u16> = rows.iter().map(|r| r.disc.max(1)).collect();
        discs.dedup();
        let best = rows
            .iter()
            .map(|r| &r.quality)
            .max_by_key(|q| (q.tier.rank(), q.bits, q.rate_hz))
            .cloned()
            .unwrap_or_default();
        Some(AlbumDetail {
            duration_ms: rows.iter().map(|r| r.duration_ms).sum(),
            discs: discs.len().max(1),
            quality: best,
            genre,
            album,
        })
    }

    /// The output that is remembered, or the first one there is, and whether
    /// bit-perfect is asked for.
    pub fn chosen_output(&self) -> (String, bool) {
        let s = chilpa_core::settings::load();
        let output = if s.output.is_empty() { self.device_or_first() } else { s.output };
        (output, s.bit_perfect)
    }

    /// Plays those files as the queue, starting at `start`. What comes before
    /// it stays in the queue, so going back reaches it.
    pub fn play_from(&mut self, paths: &[String], start: usize) {
        if start >= paths.len() {
            return;
        }
        let (device, bit_perfect) = self.chosen_output();
        self.queue = paths.to_vec();
        self.spawn(start, &device, bit_perfect);
    }
}
