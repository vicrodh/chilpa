//! What the window's lists and screens read.
//!
//! Rows by id, a record's detail, the shelves of the home screen, and the
//! [`Reader`]: a second connection to the library that the window's worker
//! owns, for the queries that do not fit in a frame (the snapshot of an
//! order, the unfiltered genre column).

use crate::quality::Quality;
use crate::{Album, Chilpa};
pub use chilpa_library::{FacetEntry, Filter, Snapshot, Totals, TrackOrder};
use std::collections::HashSet;

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
    /// It can be played now. A row that cannot is dimmed, does not play,
    /// and offers only to be taken out of an editable playlist (ADR-017).
    pub available: bool,
}

/// One row in a grouped track list. Headers are explicit boundaries, so the
/// same list contract can represent discs, acts or scenes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrackEntry {
    Header { label: String, meta: String },
    Track(TrackRow),
}

fn row_of(t: chilpa_library::BrowseTrack) -> TrackRow {
    TrackRow {
        quality: Quality::of_track(&t.external_id, t.codec.as_deref(), t.bit_depth, t.sample_rate_hz),
        available: t.available,
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

pub(crate) fn album_of((a, listens): (chilpa_library::StoredAlbum, usize)) -> Album {
    Album { id: a.id, title: a.title, artist: a.album_artist, year: a.year, track_count: a.track_count, artwork_path: a.artwork_path.unwrap_or_default(), play_count: listens }
}

/// The four facet columns, each narrowed by the ones to its left.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Facets {
    pub sources: Vec<FacetEntry>,
    pub years: Vec<FacetEntry>,
    pub genres: Vec<FacetEntry>,
    pub artists: Vec<FacetEntry>,
    pub albums: Vec<FacetEntry>,
}

/// Narrows the choices shown by one facet header without changing the
/// library filter itself. The caller already lowercases and trims the query.
pub fn filter_facets(mut entries: Vec<FacetEntry>, query: &str) -> Vec<FacetEntry> {
    if !query.is_empty() {
        entries.retain(|entry| entry.label.to_lowercase().contains(query));
    }
    entries
}

/// Excel-style selection for a facet: plain replaces, Ctrl/Cmd toggles and
/// Shift spans from the anchor; Ctrl+Shift adds that span.
pub fn select_facets(entries: &[FacetEntry], current: &[String], anchor: Option<usize>, row: usize, modifiers: i32) -> (Vec<String>, Option<usize>) {
    if row == 0 || row > entries.len() {
        return (Vec::new(), None);
    }
    let at = row - 1;
    let (ctrl, shift) = (modifiers & 1 != 0, modifiers & 2 != 0);
    let mut chosen = current.to_vec();
    if shift {
        let from = anchor.unwrap_or(at);
        let keys = entries[from.min(at)..=from.max(at)].iter().map(|entry| entry.key.clone());
        if ctrl {
            for key in keys {
                if !chosen.contains(&key) {
                    chosen.push(key);
                }
            }
        } else {
            chosen = keys.collect();
        }
        (chosen, anchor)
    } else {
        let key = entries[at].key.clone();
        if ctrl {
            if let Some(i) = chosen.iter().position(|value| *value == key) {
                chosen.remove(i);
            } else {
                chosen.push(key);
            }
        } else {
            chosen = vec![key];
        }
        (chosen, Some(at))
    }
}

/// Excel-style track selection. The caller supplies row ids because a view
/// may interleave tracks with headers or page them instead of loading all.
pub fn select_tracks(
    current: &HashSet<i64>,
    anchor: Option<usize>,
    row: usize,
    modifiers: i32,
    id_at: impl Fn(usize) -> Option<i64>,
) -> (HashSet<i64>, Option<usize>) {
    let Some(id) = id_at(row) else {
        return (current.clone(), anchor);
    };
    if modifiers & 4 != 0 && current.contains(&id) {
        return (current.clone(), anchor);
    }
    if modifiers & 2 != 0 && anchor.is_some() {
        let from = anchor.unwrap_or(row);
        let selected = (from.min(row)..=from.max(row)).filter_map(id_at).collect();
        return (selected, anchor);
    }
    let mut selected = current.clone();
    if modifiers & 1 != 0 {
        if !selected.remove(&id) {
            selected.insert(id);
        }
    } else {
        selected = [id].into_iter().collect();
    }
    (selected, Some(row))
}

/// What a search found, best first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Found {
    pub artists: Vec<crate::Artist>,
    pub albums: Vec<Album>,
    pub tracks: Vec<TrackRow>,
}

fn artist_of(a: chilpa_library::StoredArtist) -> crate::Artist {
    crate::Artist { name: a.name, album_count: a.album_count, track_count: a.track_count, artwork_path: a.artwork_path.unwrap_or_default() }
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
    // Not in the library: a file opened from a folder plays if it is there;
    // a server's track the library no longer holds does not.
    let available = !chilpa_models::RemoteEntry::is_remote(path) && std::path::Path::new(path).exists();
    TrackRow { title: crate::title_of(std::path::Path::new(path)), quality: Quality::of_file(path, None, None), id: 0, path: path.to_string(), artist: String::new(), album: String::new(), album_id: 0, disc: 0, number: 0, duration_ms: 0, plays: 0, artwork_path: String::new(), available }
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
        self.lib.grouped_snapshot(filter, order).unwrap_or_default()
    }

    /// The files behind those ids, in order: what a list queues when one
    /// of its rows is played. Read in chunks, so a snapshot of the whole
    /// library resolves without one statement holding thousands of
    /// parameters.
    pub fn paths(&self, ids: &[i64]) -> Vec<String> {
        ids.chunks(500).flat_map(|chunk| self.lib.tracks_by_ids(chunk).unwrap_or_default()).map(|t| t.external_id).collect()
    }

    /// Every artist with something present, by name, with their records,
    /// tracks and a cover of theirs.
    pub fn artists(&self) -> Vec<crate::Artist> {
        self.lib.presented_artists(1_000_000, 0).unwrap_or_default().into_iter().map(artist_of).collect()
    }

    /// What the search box finds for `text`: artists, records and songs, up
    /// to `limit` of each, best first.
    pub fn find(&self, text: &str, limit: usize) -> Found {
        let found = self.lib.find(text, limit).unwrap_or_default();
        Found { artists: found.artists.into_iter().map(artist_of).collect(), albums: self.lib.albums_by_ids(&found.albums).unwrap_or_default().into_iter().map(album_of).collect(), tracks: self.lib.tracks_by_ids(&found.tracks).unwrap_or_default().into_iter().map(row_of).collect() }
    }

    /// Our own lists, newest first, for the sidebar. A source's lists are
    /// the facade's to ask for: they need its plugins.
    pub fn local_playlists(&self) -> Vec<crate::Playlist> {
        self.lib.playlists().unwrap_or_default().into_iter().filter(|p| p.provider == "local").map(crate::playlists::playlist_of).collect()
    }

    /// Every cached playlist. This does not call a source from the reader.
    pub fn playlists(&self) -> Vec<crate::Playlist> {
        let mut playlists: Vec<crate::Playlist> = self.lib.playlists().unwrap_or_default().into_iter().map(crate::playlists::playlist_of).collect();
        for playlist in &mut playlists {
            playlist.covers = self.lib.playlist_covers(playlist.id, 4).unwrap_or_default();
        }
        playlists
    }

    pub fn playlist_folders(&self) -> Vec<crate::PlaylistFolder> {
        self.lib.playlist_folders().unwrap_or_default().into_iter().map(|folder| crate::PlaylistFolder { id: folder.id, name: folder.name, color: folder.color, position: folder.position, playlist_count: folder.playlist_count }).collect()
    }

    pub fn playlist_manager(&self, query: &str, filter: &str, sort: &str, ascending: bool, folder_id: Option<i64>) -> Vec<crate::PlaylistManagerEntry> {
        crate::playlists::playlist_manager_entries(self.playlists(), self.playlist_folders(), query, filter, sort, ascending, folder_id)
    }

    /// Every album with something present, in that order.
    pub fn album_ids(&self, order: chilpa_library::AlbumSort) -> Vec<i64> {
        self.lib.album_ids(order).unwrap_or_default()
    }

    /// The same, keeping the records whose title or artist holds every word
    /// of `text`.
    pub fn album_ids_matching(&self, order: chilpa_library::AlbumSort, text: &str) -> Vec<i64> {
        let ids = self.lib.album_ids_matching(order, text).unwrap_or_default();
        self.lib.grouped_album_ids(&ids).unwrap_or(ids)
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
                    FolderItem { name: path.clone(), tracks: self.lib.folder_track_count(&path).unwrap_or(0), artwork_path: String::new(), path }
                })
                .collect();
            return FolderView { folders, tracks: Vec::new(), in_library: true };
        }
        let inside = roots.iter().any(|r| std::path::Path::new(dir).starts_with(&r.path));
        if inside {
            let (folders, tracks) = self.lib.folder_children(dir).unwrap_or_default();
            return FolderView { folders: folders.into_iter().map(|f| FolderItem { name: f.name, path: f.path, tracks: f.tracks, artwork_path: f.artwork_path.unwrap_or_default() }).collect(), tracks: tracks.into_iter().map(row_of).collect(), in_library: true };
        }
        crate::folders::disk_folder(std::path::Path::new(dir))
    }

    /// Library folders whose path contains every word in `text`. Search is
    /// flat on purpose: it jumps into a large tree without opening every
    /// branch and issuing one query per directory.
    pub fn folder_matches(&self, text: &str) -> FolderView {
        let words: Vec<String> = text.to_lowercase().split_whitespace().map(String::from).collect();
        if words.is_empty() {
            return self.folder("");
        }
        let mut paths = std::collections::BTreeSet::new();
        for root in self.lib.roots().unwrap_or_default() {
            let root = root.path;
            paths.insert(root.clone());
            for track in self.lib.folder_paths(&root.to_string_lossy()).unwrap_or_default() {
                let mut parent = std::path::Path::new(&track).parent();
                while let Some(dir) = parent.filter(|dir| dir.starts_with(&root)) {
                    paths.insert(dir.to_path_buf());
                    if dir == root {
                        break;
                    }
                    parent = dir.parent();
                }
            }
        }
        let folders = paths
            .into_iter()
            .filter_map(|path| {
                let shown = path.to_string_lossy().into_owned();
                let lower = shown.to_lowercase();
                words.iter().all(|word| lower.contains(word)).then_some(FolderItem { name: shown.clone(), path: shown, tracks: 0, artwork_path: String::new() })
            })
            .collect();
        FolderView { folders, tracks: Vec::new(), in_library: true }
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
        let mut albums = filter_facets(self.lib.facet_albums(f).unwrap_or_default(), &f.queries[3].trim().to_lowercase());
        let ids: Vec<i64> = albums.iter().filter_map(|a| a.key.parse().ok()).collect();
        let chosen: HashSet<i64> = self.lib.grouped_album_ids(&ids).unwrap_or(ids).into_iter().collect();
        albums.retain(|a| a.key.parse().is_ok_and(|id| chosen.contains(&id)));
        let mut artists = self.lib.facet_artists(f).unwrap_or_default();
        if self.lib.group_album_versions() {
            let mut left = f.clone();
            left.choose(2, &[]);
            left.queries[2..].fill(String::new());
            let ids = self.lib.facet_albums(&left).unwrap_or_default().iter().filter_map(|a| a.key.parse().ok()).collect::<Vec<_>>();
            if let Ok(catalog) = self.lib.version_catalog() {
                let mut groups = std::collections::HashMap::<String, HashSet<String>>::new();
                for id in ids {
                    for (artist, _) in catalog.artists.get(&id).into_iter().flatten() {
                        if let Some(version) = catalog.versions.get(&id) {
                            groups.entry(artist.to_lowercase()).or_default().insert(version.group.clone());
                        }
                    }
                }
                for artist in &mut artists { artist.albums = groups.get(&artist.label.to_lowercase()).map_or(0, HashSet::len); }
            }
        }
        Facets { sources: self.lib.facet_sources().unwrap_or_default(), years: self.lib.facet_years().unwrap_or_default(), genres: self.lib.facet_genres(f).unwrap_or_default(), artists, albums }
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
        let Some(lib) = self.library() else {
            return Vec::new();
        };
        lib.tracks_by_ids(ids).unwrap_or_default().into_iter().map(row_of).collect()
    }

    /// Those albums, in the order given. By primary key.
    pub fn albums_by_ids(&mut self, ids: &[i64]) -> Vec<Album> {
        let Some(lib) = self.library() else {
            return Vec::new();
        };
        lib.albums_by_ids(ids).unwrap_or_default().into_iter().map(album_of).collect()
    }

    /// The queue from what plays on, as rows: what plays first, then what
    /// comes next. A file the library does not have gets a row from its
    /// name, so anything queued shows.
    pub fn queue_rows(&mut self) -> Vec<TrackRow> {
        // In the order they will play, which shuffle and "play next" change.
        let paths: Vec<String> = self.queue_order().into_iter().map(|i| self.queue[i].clone()).collect();
        let Some(lib) = self.library() else {
            return Vec::new();
        };
        let known = lib.tracks_by_paths(&paths).unwrap_or_else(|_| vec![None; paths.len()]);
        paths
            .into_iter()
            .zip(known)
            .map(|(path, row)| match row {
                Some(t) => row_of(t),
                None => file_row(&path),
            })
            .collect()
    }

    /// A list's tracks, in its order. What the library does not have gets a
    /// row from its name, so a list survives a missing file.
    pub fn playlist_rows(&mut self, playlist_id: i64) -> Vec<TrackRow> {
        let Some(lib) = self.library() else {
            return Vec::new();
        };
        let entries = lib.playlist_entries(playlist_id).unwrap_or_default();
        let ids: Vec<i64> = entries.iter().filter_map(|e| e.track_id).collect();
        let known: std::collections::HashMap<i64, TrackRow> = lib.tracks_by_ids(&ids).unwrap_or_default().into_iter().map(|t| (t.id, row_of(t))).collect();
        entries
            .into_iter()
            .map(|e| match e.track_id.and_then(|id| known.get(&id)) {
                Some(row) => row.clone(),
                None => file_row(&e.external_id),
            })
            .collect()
    }

    /// One playlist in the requested presentation order. `custom` is its
    /// persisted order and is the only order that may be edited.
    pub fn playlist_rows_sorted(&mut self, playlist_id: i64, order: &str, ascending: bool) -> Vec<TrackRow> {
        let mut rows = self.playlist_rows(playlist_id);
        match order {
            "title" => rows.sort_by_key(|row| row.title.to_lowercase()),
            "artist" => rows.sort_by_key(|row| (row.artist.to_lowercase(), row.title.to_lowercase())),
            "album" => rows.sort_by_key(|row| (row.album.to_lowercase(), row.number, row.title.to_lowercase())),
            "duration" => rows.sort_by_key(|row| row.duration_ms),
            "plays" => rows.sort_by_key(|row| (std::cmp::Reverse(row.plays), row.title.to_lowercase())),
            _ => {}
        }
        if !ascending && order != "custom" {
            rows.reverse();
        }
        rows
    }

    /// An artist's records, newest first.
    pub fn artist_albums(&mut self, name: &str) -> Vec<Album> {
        let Some(lib) = self.library() else {
            return Vec::new();
        };
        let cards = lib.albums_filtered(chilpa_library::AlbumSort::Year, Some(name), 10_000, 0).unwrap_or_default().into_iter().map(album_of).collect();
        self.group_album_cards(cards)
    }

    /// Eight records by the same artist, excluding this one and in a stable
    /// shuffled order so the shelf is not merely the discography order.
    pub fn same_artist_albums(&mut self, album_id: i64, limit: usize) -> Vec<Album> {
        let Some(album) = self.album(album_id) else { return Vec::new() };
        let mut albums = self.artist_albums(&album.artist);
        albums.retain(|candidate| candidate.id != album_id);
        albums.sort_by_key(|candidate| candidate.id.wrapping_mul(1_103_515_245).wrapping_add(album_id) & 0x7fff_ffff);
        albums.truncate(limit);
        albums
    }

    /// Metadata-near records from the same source. Tempo and rhythmic
    /// similarity can extend the library rank once analysis exposes them.
    pub fn related_albums(&mut self, album_id: i64, limit: usize) -> Vec<Album> {
        let Some(lib) = self.library() else { return Vec::new() };
        let cards = lib.related_albums(album_id, limit).unwrap_or_default().into_iter().map(album_of).collect();
        self.group_album_cards(cards)
    }

    /// The records marked as favorites.
    pub fn favorite_albums(&mut self) -> std::collections::HashSet<i64> {
        self.library().and_then(|lib| lib.favorite_albums().ok()).unwrap_or_default()
    }

    pub fn set_album_favorite(&mut self, album_id: i64, on: bool) {
        if let Some(lib) = self.library() {
            let _ = lib.set_album_favorite(album_id, on);
        }
    }

    /// Every file of those records, one record after another, each in disc
    /// order.
    pub fn album_paths(&mut self, ids: &[i64]) -> Vec<String> {
        ids.iter().flat_map(|id| self.album_track_rows(*id)).map(|t| t.path).collect()
    }

    /// The records by others an artist plays on: the compilations.
    pub fn artist_appearances(&mut self, name: &str) -> Vec<Album> {
        let Some(lib) = self.library() else {
            return Vec::new();
        };
        let cards = lib.albums_appearing(name).unwrap_or_default().into_iter().map(album_of).collect();
        self.group_album_cards(cards)
    }

    /// An artist's tracks, in that order, as many as asked for: on their own
    /// records and on the compilations they are on.
    pub fn artist_tracks(&mut self, name: &str, order: TrackOrder, limit: usize) -> Vec<TrackRow> {
        let Some(lib) = self.library() else {
            return Vec::new();
        };
        let f = Filter { performer: Some(name.to_string()), ..Filter::default() };
        lib.browse_tracks(&f, order, limit).unwrap_or_default().into_iter().map(row_of).collect()
    }

    /// A record's tracks, in disc order.
    pub fn album_track_rows(&mut self, album_id: i64) -> Vec<TrackRow> {
        let Some(lib) = self.library() else {
            return Vec::new();
        };
        let f = Filter { album: Some(album_id), ..Filter::default() };
        lib.browse_tracks(&f, TrackOrder::Disc, 10_000).unwrap_or_default().into_iter().map(row_of).collect()
    }

    /// Album tracks with a header for every disc when there is more than one.
    pub fn album_track_entries(&mut self, album_id: i64) -> Vec<TrackEntry> {
        let rows = self.album_track_rows(album_id);
        let mut discs: Vec<u16> = rows.iter().map(|track| track.disc.max(1)).collect();
        discs.dedup();
        if discs.len() < 2 {
            return rows.into_iter().map(TrackEntry::Track).collect();
        }
        let mut entries = Vec::with_capacity(rows.len() + discs.len());
        let mut current = None;
        for (index, track) in rows.iter().enumerate() {
            let disc = track.disc.max(1);
            if current != Some(disc) {
                current = Some(disc);
                let songs = rows[index..].iter().take_while(|row| row.disc.max(1) == disc).count();
                let count = songs.to_string();
                entries.push(TrackEntry::Header {
                    label: self.t_args("Disc {}", &[&disc.to_string()]),
                    meta: self.tf("{} song", "{} songs", songs as i64, &[&count]),
                });
            }
            entries.push(TrackEntry::Track(track.clone()));
        }
        entries
    }

    /// The most played tracks. What never played is not "most played".
    pub fn most_played_rows(&mut self, limit: usize) -> Vec<TrackRow> {
        let Some(lib) = self.library() else {
            return Vec::new();
        };
        lib.browse_tracks(&Filter::default(), TrackOrder::Plays, limit).unwrap_or_default().into_iter().filter(|t| t.plays > 0).map(row_of).collect()
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
        let Some(lib) = self.library() else {
            return Vec::new();
        };
        let ids = lib.album_ids(order).unwrap_or_default();
        let ids = lib.grouped_album_ids(&ids).unwrap_or(ids);
        lib.albums_by_ids(&ids[..ids.len().min(limit)]).unwrap_or_default().into_iter().map(album_of).collect()
    }

    /// A record with what its header states.
    pub fn album_detail(&mut self, album_id: i64) -> Option<AlbumDetail> {
        let album = self.album(album_id)?;
        let rows = self.album_track_rows(album_id);
        let genre = self.library()?.album_genre(album_id).ok().flatten().unwrap_or_default();
        let mut discs: Vec<u16> = rows.iter().map(|r| r.disc.max(1)).collect();
        discs.dedup();
        let best = rows.iter().map(|r| &r.quality).max_by_key(|q| (q.tier.rank(), q.bits, q.rate_hz)).cloned().unwrap_or_default();
        Some(AlbumDetail { duration_ms: rows.iter().map(|r| r.duration_ms).sum(), discs: discs.len().max(1), quality: best, genre, album })
    }

    /// The output that is remembered, or the first one there is. The boolean
    /// is retained at this facade boundary until its callers migrate; it is
    /// always false because bit-perfect is a result, not a requested mode.
    pub fn chosen_output(&self) -> (String, bool) {
        let s = chilpa_core::settings::load();
        let output = if s.output.is_empty() { self.device_or_first() } else { s.output };
        (output, false)
    }

    /// Plays those files as the queue, starting at `start`. What comes before
    /// it stays in the queue, so going back reaches it.
    pub fn play_from(&mut self, paths: &[String], start: usize) {
        if start >= paths.len() {
            return;
        }
        // What cannot play now stays out, and the start moves with it: the
        // track asked for, or the next one that can play.
        let chosen = &paths[start];
        let kept = self.playable(paths);
        let Some(start) = kept.iter().position(|p| p == chosen).or_else(|| {
            let before = paths[..start].iter().filter(|p| kept.contains(p)).count();
            (before < kept.len()).then_some(before)
        }) else {
            return;
        };
        let (device, bit_perfect) = self.chosen_output();
        self.replace_queue(kept);
        self.spawn(start, &device, bit_perfect);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(label: &str) -> FacetEntry {
        FacetEntry { key: label.into(), label: label.into(), albums: 0, year: None, icon: String::new() }
    }

    #[test]
    fn facet_queries_ignore_case_and_match_substrings() {
        let labels = vec![entry("Massive Attack"), entry("Björk")];
        assert_eq!(filter_facets(labels.clone(), "attack"), vec![labels[0].clone()]);
        assert_eq!(filter_facets(labels, "björk").len(), 1);
    }

    #[test]
    fn compilation_members_keep_group_counts_and_find_the_complete_album() {
        let lib = chilpa_library::Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        for (release, bits) in [("original", 16), ("remaster", 24)] {
            let id = lib.source_edition(source, release, "Reference CD", "Various Artists", Some(2007)).unwrap();
            for (number, artist, credit) in [(1, "歌手 A", "Singer A"), (2, "歌手 B", "Singer B")] {
                lib.connection().execute("INSERT INTO tracks(source_id, external_id, title, artist, album_credit,
                    album_id, bit_depth, duration_ms, added_at) VALUES (?1, ?2, 'Song', ?3, ?4, ?5, ?6, 1000, 1)",
                    rusqlite::params![source, format!("/{release}/{number}.flac"), artist, credit, id, bits]).unwrap();
            }
        }
        let reader = Reader { lib };
        for artist in ["Singer A", "Singer B", "歌手 A", "歌手 B"] {
            let mut filter = Filter::default();
            filter.queries[2] = artist.into();
            let facets = reader.facets(&filter);
            assert_eq!(facets.artists.iter().find(|a| a.label == artist).unwrap().albums, 1);
            assert_eq!(facets.albums.len(), 1);
            assert_eq!(reader.snapshot(&filter, TrackOrder::Album).ids.len(), 2);
            filter.queries[2].clear();
            filter.artists = vec![artist.into()];
            assert_eq!(reader.snapshot(&filter, TrackOrder::Album).ids.len(), 2);
            assert_eq!(reader.artists().iter().find(|a| a.name == artist).unwrap().album_count, 1);
            assert_eq!(reader.artists().iter().find(|a| a.name == artist).unwrap().track_count, 1);
        }
    }

    #[test]
    fn facet_selection_replaces_toggles_and_spans_from_its_anchor() {
        let entries = [entry("A"), entry("B"), entry("C"), entry("D")];
        let (one, anchor) = select_facets(&entries, &[], None, 2, 0);
        assert_eq!((one.as_slice(), anchor), (&["B".into()][..], Some(1)));
        let (span, anchor) = select_facets(&entries, &one, anchor, 4, 2);
        assert_eq!(span, ["B", "C", "D"]);
        let (more, _) = select_facets(&entries, &span, anchor, 1, 1);
        assert_eq!(more, ["B", "C", "D", "A"]);
    }

    #[test]
    fn track_selection_skips_headers_while_spanning_rows() {
        let ids = [Some(10), None, Some(20), Some(30)];
        let (one, anchor) = select_tracks(&HashSet::new(), None, 0, 0, |row| ids[row]);
        let (span, anchor) = select_tracks(&one, anchor, 3, 2, |row| ids[row]);
        assert_eq!(span, HashSet::from([10, 20, 30]));
        let (toggled, _) = select_tracks(&span, anchor, 2, 1, |row| ids[row]);
        assert_eq!(toggled, HashSet::from([10, 30]));
    }

    #[test]
    fn folder_search_returns_matching_directories_in_one_flat_result() {
        let lib = chilpa_library::Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        lib.root_id(source, std::path::Path::new("/music")).unwrap();
        lib.connection()
            .execute(
                "INSERT INTO tracks (source_id, external_id, title, added_at) \
                 VALUES (?1, '/music/Rock/Album/song.flac', 'Song', 1)",
                [source],
            )
            .unwrap();
        let found = Reader { lib }.folder_matches("rock");
        let paths: Vec<_> = found.folders.iter().map(|folder| folder.path.as_str()).collect();
        assert_eq!(paths, ["/music/Rock", "/music/Rock/Album"]);
    }
}
