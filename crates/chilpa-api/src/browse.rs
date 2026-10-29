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
    /// Who wrote it, joined with commas; empty when nobody is named.
    pub composer: String,
    /// The disc's own title from its tag, ahead of any folder name.
    pub disc_subtitle: String,
    /// The work this track belongs to, when its source names one.
    pub work: String,
    /// A source artist route for the composer; empty for local files.
    pub composer_target: String,
    pub groups: Option<Box<crate::TrackGroups>>,
    pub explicit: Option<bool>,
    pub credits: Option<Box<crate::credits::TrackCredits>>,
}

/// One row in a grouped track list. Headers are explicit boundaries, so the
/// same list contract can represent discs, acts or scenes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrackEntry {
    Header(GroupHeader),
    Track(TrackRow),
}

impl TrackEntry {
    /// Whether refreshed metadata belongs to the same visual row.
    pub fn same_row(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Track(a), Self::Track(b)) => a.id == b.id && a.path == b.path,
            (Self::Header(a), Self::Header(b)) => a.label == b.label && a.record == b.record && a.work == b.work,
            _ => false,
        }
    }
    /// A header that only names its group.
    pub fn header(label: String, meta: String) -> Self {
        Self::Header(GroupHeader { label, meta, ..GroupHeader::default() })
    }
}

/// What a header says of the rows under it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GroupHeader {
    pub label: String,
    pub meta: String,
    /// The group's own cover; empty when it has none worth showing.
    pub art: String,
    /// Whose it is, for a header that stands for a record.
    pub artist: String,
    /// It stands for a record, or one disc of it, among several in a list:
    /// it is drawn with its cover, title and artist, not as a divider.
    pub record: bool,
    /// A work boundary, drawn as a title with an optional composer.
    pub work: bool,
    pub composer: String,
    pub composer_target: String,
}

pub(crate) fn row_of(t: chilpa_library::BrowseTrack) -> TrackRow {
    TrackRow {
        quality: Quality::of_track(&t.external_id, t.codec.as_deref(), t.bit_depth, t.sample_rate_hz),
        available: t.available,
        explicit: t.explicit,
        credits: None,
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
        composer: t.composers,
        disc_subtitle: t.disc_subtitle.unwrap_or_default(),
        work: t.work.unwrap_or_default(),
        composer_target: String::new(),
        groups: None,
    }
}

pub(crate) fn album_of((a, listens): (chilpa_library::StoredAlbum, usize)) -> Album {
    Album { id: a.id, title: a.title, artist: a.album_artist, year: a.year, track_count: a.track_count, artwork_path: a.artwork_path.unwrap_or_default(), play_count: listens, explicit: a.explicit }
}

/// The facet columns, each narrowed by the ones to its left.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Facets {
    pub sources: Vec<FacetEntry>,
    pub years: Vec<FacetEntry>,
    pub genres: Vec<FacetEntry>,
    pub labels: Vec<FacetEntry>,
    pub composers: Vec<FacetEntry>,
    pub ratings: Vec<FacetEntry>,
    pub artists: Vec<FacetEntry>,
    pub albums: Vec<FacetEntry>,
}

/// A rating column's entry named for the window: its stars, or none.
pub(crate) fn named_stars(mut entry: FacetEntry) -> FacetEntry {
    let stars: i64 = entry.key.parse().unwrap_or(0);
    entry.label = if stars == 0 {
        chilpa_i18n::t("Unrated")
    } else {
        chilpa_i18n::tf("{} star", "{} stars", stars, &[&stars.to_string()])
    };
    entry
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

/// A playlist's rows, given in its own order, in the order asked for: its
/// own (`custom`), by a field, the last to come in first (`added`), or the
/// most played first. `ascending` is each order's own way; the other way
/// round turns the whole list. A playlist keeps no date for an entry, so
/// when each came in is its place in the list, as QBZ reads it.
pub fn sort_playlist_rows(rows: &mut [TrackRow], order: &str, ascending: bool) {
    match order {
        "title" => rows.sort_by_key(|row| row.title.to_lowercase()),
        "artist" => rows.sort_by_key(|row| (row.artist.to_lowercase(), row.title.to_lowercase())),
        "album" => rows.sort_by_key(|row| (row.album.to_lowercase(), row.number, row.title.to_lowercase())),
        "duration" => rows.sort_by_key(|row| row.duration_ms),
        "plays" => rows.sort_by_key(|row| (std::cmp::Reverse(row.plays), row.title.to_lowercase())),
        "added" => rows.reverse(),
        _ => {}
    }
    if !ascending && order != "custom" {
        rows.reverse();
    }
}

/// An artist's row, with the picture the listener chose for them if there
/// is one, over the cover of a record of theirs.
pub(crate) fn artist_of(a: chilpa_library::StoredArtist, pictures: &chilpa_library::store::artwork::ArtistPictures) -> crate::Artist {
    let artwork_path = pictures.of(&a.name).map(str::to_string).or(a.artwork_path).unwrap_or_default();
    crate::Artist { name: a.name, album_count: a.album_count, track_count: a.track_count, artwork_path }
}

/// Folders side by side that each hold part of one record -- the discs of a
/// box set -- as one entry that is that record, where the first of them was.
/// A part alone stays a folder.
fn grouped(folders: Vec<chilpa_library::FolderEntry>) -> Vec<chilpa_library::FolderEntry> {
    let parts = |id: i64| folders.iter().filter(|f| f.part_of == Some(id)).count();
    let mut out: Vec<chilpa_library::FolderEntry> = Vec::new();
    for folder in &folders {
        let Some(id) = folder.part_of.filter(|&id| parts(id) > 1) else {
            out.push(folder.clone());
            continue;
        };
        match out.iter_mut().find(|f| f.album_id == Some(id)) {
            Some(record) => record.tracks += folder.tracks,
            None => out.push(chilpa_library::FolderEntry { album_id: Some(id), part_of: None, ..folder.clone() }),
        }
    }
    out
}

/// A folder as the Folders screen shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FolderView {
    pub folders: Vec<FolderItem>,
    pub tracks: Vec<TrackRow>,
    /// False for a folder outside the library, read from the disk as it is.
    pub in_library: bool,
    /// How many tracks it holds with everything under it.
    pub total: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FolderItem {
    pub name: String,
    pub path: String,
    /// Tracks under it, all the way down; zero when it is not known.
    pub tracks: usize,
    pub artwork_path: String,
    /// How many folders sit in it; zero when it is not known.
    pub folders: usize,
    /// The record it is, or zero: a folder whose files are all of one.
    pub album_id: i64,
    /// In the folder tree: how deep it is, whether it is open, and whether
    /// it is a file, which a tree lists under the folders beside it.
    pub depth: usize,
    pub open: bool,
    pub file: bool,
}

/// A row for a file the library does not have: its name, and what its
/// extension says.
pub(crate) fn file_row(path: &str) -> TrackRow {
    if let Some((image, _)) = chilpa_core::sacd_tracks::parse(std::path::Path::new(path)) {
        if let Some(row) = crate::folders::image_rows(&image).into_iter().find(|row| row.path == path) {
            return row;
        }
    }
    // Not in the library: a file opened from a folder plays if it is there;
    // a server's track the library no longer holds does not.
    let available = !chilpa_models::RemoteEntry::is_remote(path) && std::path::Path::new(path).exists();
    TrackRow { title: crate::title_of(std::path::Path::new(path)), quality: Quality::of_file(path, None, None), id: 0, path: path.to_string(), artist: String::new(), album: String::new(), album_id: 0, disc: 0, number: 0, duration_ms: 0, plays: 0, artwork_path: String::new(), available, composer: String::new(), disc_subtitle: String::new(), work: String::new(), composer_target: String::new(), groups: None, explicit: None, credits: None }
}

/// A record as its page shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AlbumDetail {
    pub album: Album,
    /// The genre most of its tracks carry; empty if none does.
    pub genre: String,
    /// The label most of its tracks name; empty if none does.
    pub label: String,
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
    pub(crate) lib: chilpa_library::Library,
    pub(crate) catalog: Option<std::sync::Arc<crate::source_catalog::Catalog>>,
    refresh_cache: std::cell::RefCell<RefreshCache>,
    /// What the listener integrated from the sources, while the catalog and
    /// the choice of what to integrate stay as they were.
    pub(crate) sources: std::cell::RefCell<Option<(crate::library_sources::Stamp, std::rc::Rc<crate::library_sources::SourceIndex>)>>,
}

/// The library's editions, whether editions are one card, and the copies
/// the listener chose: what a grid groups a record kept by several sources by.
pub(crate) type CopyGrouping = (std::sync::Arc<chilpa_library::store::versions::VersionCatalog>, bool, std::collections::HashSet<String>);

#[derive(Default)]
struct RefreshCache {
    revision: Option<(i64, u64)>,
    sources: crate::library_sources::Stamp,
    /// Grouping across sources, and the copies chosen, as preferences say.
    copies: (bool, Vec<String>),
    entries: std::collections::VecDeque<(Filter, TrackOrder, Facets, Snapshot)>,
}

/// The preferences a merged grid's grouping reads, as a cache compares them.
fn copy_preferences() -> (bool, Vec<String>) {
    let mut chosen: Vec<String> = crate::library_sources::chosen_copies().into_iter().collect();
    chosen.sort();
    (crate::library_sources::grouping_sources(), chosen)
}

/// I retain bounded snapshot pages; views can share them across navigation.
pub struct PageCache<T> {
    pages: std::collections::HashMap<usize, std::rc::Rc<Vec<T>>>,
    order: std::collections::VecDeque<usize>,
}
impl<T> Default for PageCache<T> {
    fn default() -> Self { Self { pages: Default::default(), order: Default::default() } }
}
impl<T> PageCache<T> {
    pub fn read(&mut self, page: usize, capacity: usize, load: impl FnOnce() -> Vec<T>) -> std::rc::Rc<Vec<T>> {
        if let Some(rows) = self.pages.get(&page) { return rows.clone(); }
        let rows = std::rc::Rc::new(load());
        self.pages.insert(page, rows.clone());
        self.order.push_back(page);
        if self.order.len() > capacity {
            if let Some(old) = self.order.pop_front() { self.pages.remove(&old); }
        }
        rows
    }
}

impl Reader {
    #[cfg(test)]
    pub(crate) fn of(lib: chilpa_library::Library) -> Self {
        Self { lib, catalog: None, refresh_cache: Default::default(), sources: Default::default() }
    }

    /// Forgets the answers kept for repeated filters, so a measurement reads.
    #[cfg(test)]
    pub(crate) fn forget_refreshes(&self) {
        self.refresh_cache.borrow_mut().entries.clear();
    }

    pub(crate) fn open(path: &std::path::Path) -> Result<Self, String> {
        chilpa_library::Library::open_read_only(path).map(|lib| Self { lib, catalog: None, refresh_cache: Default::default(), sources: Default::default() })
            .map_err(|e| format!("could not open library reader at {}: {e}", path.display()))
    }

    /// Every id the filter leaves, in that order, with the totals.
    pub fn snapshot(&self, filter: &Filter, order: TrackOrder) -> Snapshot {
        self.lib.grouped_snapshot(filter, order).unwrap_or_else(|e| {
            eprintln!("chilpa-queries: library snapshot failed: {e}");
            Snapshot::default()
        })
    }

    /// Where each letter starts among those tracks (`Library::letters`).
    pub fn letters(&self, ids: &[i64], by: &str) -> Vec<(String, usize)> {
        self.lib.letters(ids, by).unwrap_or_default()
    }

    /// The files behind those ids, in order: what a list queues when one
    /// of its rows is played. Read in chunks, so a snapshot of the whole
    /// library resolves without one statement holding thousands of
    /// parameters.
    pub fn paths(&self, ids: &[i64]) -> Vec<String> {
        let local: Vec<_> = ids.iter().copied().filter(|id| *id > 0).collect();
        let mut paths = self.lib.track_paths(&local).unwrap_or_default();
        if let Some(catalog) = &self.catalog { paths.extend(catalog.tracks_by_ids(ids).into_iter().map(|(t, _)| (t.id, t.path))); }
        ids.iter().filter_map(|id| paths.get(id).cloned()).collect()
    }

    pub fn album_paths(&self, ids: &[i64]) -> Vec<String> {
        ids.iter().flat_map(|id| {
            let filter = chilpa_library::Filter { album: Some(*id), ..Default::default() };
            self.lib.browse_ids(&filter, chilpa_library::TrackOrder::Disc).ok()
                .map(|snapshot| self.paths(&snapshot.ids)).unwrap_or_default()
        }).collect()
    }

    /// The artists with something present, by name, with their records,
    /// tracks and a cover of theirs. `role` is "all" for everyone who plays
    /// on a track; anything else leaves who a record is credited to.
    pub fn artists(&self, role: &str) -> Vec<crate::Artist> {
        let role = chilpa_library::ArtistRole::from_name(role);
        let pictures = self.lib.artist_pictures().unwrap_or_default();
        self.lib.artists_by_role(role).unwrap_or_default().into_iter().map(|a| artist_of(a, &pictures)).collect()
    }

    /// What the search box finds for `text`: artists, records and songs, up
    /// to `limit` of each, best first.
    pub fn find(&self, text: &str, limit: usize) -> Found {
        let found = self.lib.find(text, limit).unwrap_or_default();
        let pictures = self.lib.artist_pictures().unwrap_or_default();
        Found { artists: found.artists.into_iter().map(|a| artist_of(a, &pictures)).collect(), albums: self.lib.albums_by_ids(&found.albums).unwrap_or_default().into_iter().map(album_of).collect(), tracks: self.lib.tracks_by_ids(&found.tracks).unwrap_or_default().into_iter().map(row_of).collect() }
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
        crate::playlists::playlist_manager_entries(self.playlists(), self.playlist_folders(), query, filter, "", sort, ascending, folder_id)
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
        self.folder_with(dir, false)
    }

    /// The same, with the discs of a box set side by side standing as the
    /// box when `collections` asks for it (Folders' "Group collections").
    pub fn folder_with(&self, dir: &str, collections: bool) -> FolderView {
        if chilpa_core::sacd_tracks::is_image(std::path::Path::new(dir)) {
            let mut tracks = crate::folders::image_rows(std::path::Path::new(dir));
            let paths = tracks.iter().map(|row| row.path.clone()).collect::<Vec<_>>();
            let known = self.lib.tracks_by_paths(&paths).unwrap_or_default();
            let in_library = !tracks.is_empty() && known.iter().flatten().count() == tracks.len();
            for (row, saved) in tracks.iter_mut().zip(known) {
                if let Some(saved) = saved { *row = row_of(saved); }
            }
            return FolderView { total: tracks.len(), folders: Vec::new(), tracks, in_library };
        }
        let roots = self.lib.roots().unwrap_or_default();
        if dir.is_empty() {
            let folders = roots
                .iter()
                .map(|r| {
                    let path = r.path.to_string_lossy().into_owned();
                    // A root is called what its folder is called; where it
                    // is belongs to Settings.
                    let name = r.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.clone());
                    FolderItem { name, tracks: self.lib.folder_track_count(&path).unwrap_or(0), artwork_path: String::new(), path, ..FolderItem::default() }
                })
                .collect();
            let total = roots.iter().map(|r| self.lib.folder_track_count(&r.path.to_string_lossy()).unwrap_or(0)).sum();
            return FolderView { folders, tracks: Vec::new(), in_library: true, total };
        }
        let inside = roots.iter().any(|r| std::path::Path::new(dir).starts_with(&r.path));
        if inside {
            let (folders, tracks) = self.lib.folder_children(dir).unwrap_or_default();
            let folders = if collections { grouped(folders) } else { folders };
            return FolderView { folders: folders.into_iter().map(|f| FolderItem { name: f.name, path: f.path, tracks: f.tracks, artwork_path: f.artwork_path.unwrap_or_default(), folders: f.folders, album_id: f.album_id.unwrap_or(0), ..FolderItem::default() }).collect(), tracks: tracks.into_iter().map(row_of).collect(), in_library: true, total: self.lib.folder_track_count(dir).unwrap_or(0) };
        }
        crate::folders::disk_folder(std::path::Path::new(dir))
    }

    /// The folder tree as a list: the library's folders, and under each one
    /// that is open, its folders and then its files. `text` typed to find a
    /// folder makes it the folders that match, at one depth.
    pub fn folder_tree(&self, open: &std::collections::HashSet<String>, text: &str) -> FolderView {
        if !text.trim().is_empty() {
            return self.folder_matches(text);
        }
        let mut folders = Vec::new();
        self.branch("", 0, open, &mut folders);
        FolderView { folders, tracks: Vec::new(), in_library: true, total: 0 }
    }

    fn branch(&self, dir: &str, depth: usize, open: &std::collections::HashSet<String>, rows: &mut Vec<FolderItem>) {
        let view = self.folder(dir);
        for folder in view.folders {
            let path = folder.path.trim_end_matches('/').to_string();
            let is_open = open.contains(&path);
            rows.push(FolderItem { path: path.clone(), depth, open: is_open, ..folder });
            if is_open {
                self.branch(&path, depth + 1, open, rows);
            }
        }
        for track in view.tracks {
            let name = if chilpa_core::sacd_tracks::parse(std::path::Path::new(&track.path)).is_some() {
                format!("{:02}. {}", track.number, track.title)
            } else {
                std::path::Path::new(&track.path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or(track.title)
            };
            rows.push(FolderItem { name, path: track.path, depth, file: true, ..FolderItem::default() });
        }
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
                let physical = chilpa_core::sacd_tracks::parse(std::path::Path::new(&track))
                    .map(|(image, _)| image).unwrap_or_else(|| std::path::PathBuf::from(&track));
                let mut parent = physical.parent();
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
                words.iter().all(|word| lower.contains(word)).then_some(FolderItem { name: shown.clone(), path: shown, tracks: 0, ..FolderItem::default() })
            })
            .collect();
        FolderView { folders, tracks: Vec::new(), in_library: true, total: 0 }
    }

    /// Every playable file under a folder, in order: the library's for one
    /// inside it, the disk's for one outside.
    pub fn folder_files(&self, dir: &str) -> Vec<String> {
        if chilpa_core::sacd_tracks::is_image(std::path::Path::new(dir)) {
            return self.folder(dir).tracks.into_iter().filter(|row| row.available).map(|row| row.path).collect();
        }
        let roots = self.lib.roots().unwrap_or_default();
        if roots.iter().any(|r| std::path::Path::new(dir).starts_with(&r.path)) {
            return self.lib.folder_paths(dir).unwrap_or_default();
        }
        let files = crate::folders::audio_files_in(std::path::Path::new(dir));
        chilpa_core::sacd_tracks::expand(&files).iter().map(|p| p.display().to_string()).collect()
    }

    /// The names a query's words may be: the artists, composers, genres and
    /// labels of the library and of the integrated sources.
    fn known_names(&self, index: &crate::library_sources::SourceIndex) -> Vec<(chilpa_library::store::query::Field, Vec<String>)> {
        let mut known = crate::library_query::library_names(&self.lib);
        for item in &index.items {
            known[0].1.extend(item.artists.iter().cloned());
            known[1].1.extend(item.composers.iter().cloned());
            known[2].1.extend(item.genres.iter().cloned());
            if !item.label.is_empty() { known[3].1.push(item.label.clone()); }
        }
        for (_, names) in &mut known { names.sort(); names.dedup(); }
        known
    }

    /// I stop superseded work between queries, before it occupies the worker again.
    pub fn refresh(&self, filter: &Filter, order: TrackOrder, current: impl Fn() -> bool) -> Option<(Facets, Snapshot)> {
        if !current() { return None; }
        self.refresh_using(filter, order, current, self.source_index())
    }

    /// The same, with the integrated sources given.
    pub(crate) fn refresh_using(&self, filter: &Filter, order: TrackOrder, current: impl Fn() -> bool,
        (stamp, index): (crate::library_sources::Stamp, std::rc::Rc<crate::library_sources::SourceIndex>)) -> Option<(Facets, Snapshot)> {
        // A genre typed close to one there is reads as that one too, and
        // words that are a name the library has read as that name.
        let widened;
        let filter = match &filter.query {
            Some(query) if crate::library_query::names_genres(query) || crate::library_query::names_words(query) => {
                let mut query = query.clone();
                if crate::library_query::names_words(&query) {
                    query = crate::library_query::resolve_names(&query, &self.known_names(&index));
                }
                if crate::library_query::names_genres(&query) {
                    let mut known: Vec<String> = self.lib.genres().unwrap_or_default().into_iter().map(|(name, _)| name).collect();
                    known.extend(index.items.iter().flat_map(|item| item.genres.iter().cloned()));
                    known.sort();
                    known.dedup();
                    query = crate::library_query::widen_genres(&query, &known);
                }
                widened = Filter { query: Some(query), ..filter.clone() };
                &widened
            }
            _ => filter,
        };
        let before = self.lib.read_revision().ok();
        let shared = {
            let mut cache = self.refresh_cache.borrow_mut();
            let copies = if index.items.is_empty() { Default::default() } else { copy_preferences() };
            if cache.revision != before || cache.sources != stamp || cache.copies != copies {
                cache.entries.clear();
                (cache.revision, cache.sources, cache.copies) = (before, stamp, copies);
            }
            if before.is_some() {
                if let Some((_, _, facets, snapshot)) = cache.entries.iter().find(|(f, o, _, _)| f == filter && *o == order) {
                    let mut facets = facets.clone();
                    facets.ratings = facets.ratings.into_iter().map(named_stars).collect();
                    return Some((facets, snapshot.clone()));
                }
            }
            // I share the source list; every field count depends on the query.
            before.filter(|_| index.items.is_empty()).and_then(|_| cache.entries.front().map(|(_, _, facets, _)| facets.sources.clone()))
        };
        let read = || {
            let mut facets = self.facets_current(filter, &current, shared.as_ref())?;
            if !current() { return None; }
            let snapshot = if index.items.is_empty() { self.snapshot(filter, order) } else {
                let stars = crate::library_sources::Stars::read(&self.lib, &index);
                let held = self.copy_grouping();
                let grouping = held.as_ref().map(|(library, versions, chosen)| crate::library_sources::Grouping { library, versions: *versions, chosen });
                crate::library_sources::merge_facets(&mut facets, filter, &index, &stars, grouping);
                let (local, keys) = self.lib.keyed_snapshot(filter, order).unwrap_or_else(|e| {
                    eprintln!("chilpa-queries: library snapshot failed: {e}");
                    Default::default()
                });
                crate::library_sources::merge_snapshot(local, keys, filter, order, &index, &stars, crate::library_sources::Scope::Library, None, grouping).0
            };
            current().then_some((facets, snapshot))
        };
        let result = self.lib.with_browse_candidates(filter, read).unwrap_or_else(|error| {
            eprintln!("chilpa-queries: library eligibility failed: {error}");
            read()
        });
        if before.is_some() && self.lib.read_revision().ok() == before {
            if let Some((facets, snapshot)) = &result {
                let mut cache = self.refresh_cache.borrow_mut();
                if cache.entries.len() == 6 { cache.entries.pop_front(); }
                cache.entries.push_back((filter.clone(), order, facets.clone(), snapshot.clone()));
            }
        }
        result
    }

    /// What grids group a record kept by several sources by, when they do:
    /// the library's editions, whether editions are one card, and the
    /// copies the listener chose.
    pub(crate) fn copy_grouping(&self) -> Option<CopyGrouping> {
        if !crate::library_sources::grouping_sources() { return None; }
        let library = self.lib.version_catalog().ok()?;
        Some((library, self.lib.group_album_versions(), crate::library_sources::chosen_copies()))
    }

    /// Each column keeps the choices to its left, including the extra filters.
    pub fn facets(&self, filter: &Filter) -> Facets {
        self.facets_current(filter, &|| true, None).unwrap_or_default()
    }

    fn facets_current(&self, f: &Filter, current: &impl Fn() -> bool, shared: Option<&Vec<FacetEntry>>) -> Option<Facets> {
        macro_rules! read {
            ($query:expr) => {{ if !current() { return None; } $query.unwrap_or_default() }};
        }
        let mut left = crate::library_query::facet_filter(f, "artist");
        let album_filter = crate::library_query::facet_filter(f, "album");
        left.choose(5, &[]);
        left.queries[5..].fill(String::new());
        let left_albums = read!(self.lib.facet_albums(&left));
        let left_ids: Vec<i64> = left_albums.iter().filter_map(|a| a.key.parse().ok()).collect();
        let mut albums = if f.artist.is_none() && f.artists.is_empty() && f.queries[5].trim().is_empty() && left.query == album_filter.query {
            left_albums
        } else { read!(self.lib.facet_albums(&album_filter)) };
        albums = filter_facets(albums, &f.queries[6].trim().to_lowercase());
        let ids: Vec<i64> = albums.iter().filter_map(|a| a.key.parse().ok()).collect();
        let chosen: HashSet<i64> = self.lib.grouped_album_ids(&ids).unwrap_or(ids).into_iter().collect();
        albums.retain(|a| a.key.parse().is_ok_and(|id| chosen.contains(&id)));
        let mut artists = if left == Filter::default() {
            read!(self.lib.facet_record_artists(&left))
        } else { read!(self.lib.facet_artists_for_albums(&left_ids)) };
        if self.lib.group_album_versions() {
            if !current() { return None; }
            if let Ok(catalog) = self.lib.version_catalog() {
                let mut groups = std::collections::HashMap::<String, HashSet<String>>::new();
                for id in left_ids {
                    for (artist, _) in catalog.artists.get(&id).into_iter().flatten() {
                        if let Some(version) = catalog.versions.get(&id) {
                            groups.entry(artist.to_lowercase()).or_default().insert(version.group.clone());
                        }
                    }
                }
                for artist in &mut artists { artist.albums = groups.get(&artist.label.to_lowercase()).map_or(0, HashSet::len); }
            }
        }
        for loose in read!(self.lib.facet_albumless_artists(&left)) {
            if let Some(artist) = artists.iter_mut().find(|entry| entry.key.eq_ignore_ascii_case(&loose.key)) {
                artist.albums += loose.albums;
            } else { artists.push(loose); }
        }
        artists.sort_by(|a, b| chilpa_library::store::order_keys::nocase(&a.label, &b.label));
        let sources = read!(match shared { Some(sources) => Ok(sources.clone()), None => self.lib.facet_sources() });
        let years = read!(self.lib.facet_years_matching(&crate::library_query::facet_filter(f, "year")));
        Some(Facets {
            sources, years,
            genres: read!(self.lib.facet_genres(&crate::library_query::facet_filter(f, "genre"))), labels: read!(self.lib.facet_labels(&crate::library_query::facet_filter(f, "label"))),
            composers: read!(self.lib.facet_composers(&crate::library_query::facet_filter(f, "composer"))),
            ratings: read!(self.lib.facet_ratings(&crate::library_query::facet_filter(f, "rating"))).into_iter().map(named_stars).collect(), artists, albums,
        })
    }

}

impl Chilpa {
    /// The Library header counts real records and every eligible track.
    pub fn library_summary(&self, totals: Totals) -> String {
        let (albums, songs) = (totals.albums.to_string(), totals.tracks.to_string());
        format!("{} · {}",
            self.tf("{} album", "{} albums", totals.albums as i64, &[&albums]),
            self.tf("{} song", "{} songs", totals.tracks as i64, &[&songs]))
    }

    /// A second, read-only connection to an initialized library. Reports
    /// its own open errors: the main connection may still be healthy.
    pub fn reader(&self) -> Option<Reader> {
        Reader::open(&chilpa_library::Library::default_path()).map(|mut reader| { reader.catalog = Some(self.catalog.clone()); reader }).map_err(|e| eprintln!("chilpa-queries: {e}")).ok()
    }

    /// The rows of those ids, in that order. By primary key: the same cost
    /// anywhere in a snapshot.
    pub fn track_rows(&mut self, ids: &[i64]) -> Vec<TrackRow> {
        let local: Vec<_> = ids.iter().copied().filter(|id| *id >= 0).collect();
        let mut rows: std::collections::HashMap<_, _> = self.library().map(|lib| lib.tracks_by_ids(&local).unwrap_or_default()).unwrap_or_default().into_iter().map(row_of).map(|t| (t.id, t)).collect();
        rows.extend(self.catalog.tracks_by_ids(ids).into_iter().map(|(t, art)| (t.id, crate::source_catalog::view::row_of(&t, art))));
        ids.iter().filter_map(|id| rows.get(id).cloned()).collect()
    }

    /// Those albums, in the order given. By primary key.
    pub fn albums_by_ids(&mut self, ids: &[i64]) -> Vec<Album> {
        let local: Vec<_> = ids.iter().copied().filter(|id| *id >= 0).collect();
        let mut rows: std::collections::HashMap<_, _> = self.library().map(|lib| lib.albums_by_ids(&local).unwrap_or_default()).unwrap_or_default()
            .into_iter().map(album_of).map(|album| (album.id, album)).collect();
        for id in ids.iter().filter(|id| **id < 0) {
            if let Some(album) = self.source_album(*id) { rows.insert(*id, album); }
        }
        ids.iter().filter_map(|id| rows.remove(id)).collect()
    }

    /// The queue from what plays on, as rows: what plays first, then what
    /// comes next. A file the library does not have gets a row from its
    /// name, so anything queued shows.
    pub fn queue_rows(&mut self) -> Vec<TrackRow> {
        // In the order they will play, which shuffle and "play next" change.
        let paths: Vec<String> = self.queue_order().into_iter().map(|i| self.queue[i].clone()).collect();
        self.rows_for_paths(&paths)
    }

    pub(crate) fn rows_for_paths(&mut self, paths: &[String]) -> Vec<TrackRow> {
        let known = self.library().and_then(|lib| lib.tracks_by_paths(paths).ok())
            .unwrap_or_else(|| vec![None; paths.len()]);
        paths
            .into_iter()
            .zip(known)
            .map(|(path, row)| match row {
                Some(t) => row_of(t),
                // A source's track, then one of the media open right now,
                // which was read when it was opened; only then its name,
                // while the source is asked for the track.
                None => self.source_row(&path).or_else(|| self.media_row(&path)).unwrap_or_else(|| { self.catalog.want_track(&path); file_row(&path) }),
            })
            .collect()
    }

    /// A list's tracks, in its order. What the library does not have gets a
    /// row from its name, so a list survives a missing file.
    pub fn playlist_rows(&mut self, playlist_id: i64) -> Vec<TrackRow> {
        let active_sources: HashSet<_> = self.source_panels().into_iter().map(|panel| panel.id).collect();
        let Some(lib) = self.library() else {
            return Vec::new();
        };
        let entries = lib.playlist_entries(playlist_id).unwrap_or_default();
        let ids: Vec<i64> = entries.iter().filter_map(|e| e.track_id).collect();
        let known: std::collections::HashMap<i64, TrackRow> = lib.tracks_by_ids(&ids).unwrap_or_default().into_iter().map(|t| (t.id, row_of(t))).collect();
        let mut rows: Vec<TrackRow> = entries
            .into_iter()
            .map(|e| match e.track_id.and_then(|id| known.get(&id)) {
                Some(row) => row.clone(),
                None => file_row(&e.external_id),
            })
            .collect();
        self.catalog.protect_playlist_tracks(playlist_id, rows.iter().filter(|row| crate::source_opener::of_plugin(&row.path).is_some()).map(|row| row.path.clone()).collect());
        for row in &mut rows {
            if let Some((plugin, _)) = crate::source_opener::of_plugin(&row.path) {
                row.available = self.natives.is_native(&plugin) || active_sources.contains(&plugin);
                if let Some((track, art)) = self.catalog.track_by_path(&row.path) {
                    row.available &= track.playable;
                    row.album_id = track.album_id;
                    row.disc = track.disc;
                    row.number = track.number;
                    if art.is_empty() { self.catalog.want_art_of(&row.path); } else { row.artwork_path = art; }
                    if let Some(format) = track.format { row.quality = format.quality(); }
                } else {
                    // The library's row for a source's track (cached when a
                    // list was imported or synced) has a record of its own
                    // making, which is no page to open: the source is asked
                    // for the track, and its record comes with the answer.
                    row.album_id = 0;
                    self.catalog.want_track(&row.path);
                }
            }
        }
        rows
    }

    /// One playlist in the requested presentation order. `custom` is its
    /// persisted order and is the only order that may be edited.
    pub fn playlist_rows_sorted(&mut self, playlist_id: i64, order: &str, ascending: bool) -> Vec<TrackRow> {
        let mut rows = self.playlist_rows(playlist_id);
        sort_playlist_rows(&mut rows, order, ascending);
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
        if album_id < 0 && self.catalog.card(album_id).is_some_and(|card| card.kind != crate::source_catalog::store::CardKind::Album) { return Vec::new(); }
        let Some(album) = self.album(album_id) else { return Vec::new() };
        let local = self.artist_albums(&album.artist);
        let sources = self.natives.ids().into_iter().filter(|id| self.natives.searches(id)).collect();
        crate::album_shelves::same_artist(&album, local, Some(&self.catalog), sources, limit)
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
        ids.iter().flat_map(|id| {
            if *id < 0 {
                self.catalog.tracks(*id).unwrap_or_default().into_iter().filter(|track| track.playable).map(|track| track.path).collect()
            } else {
                self.album_track_rows(*id).into_iter().filter(|track| track.available).map(|track| track.path).collect::<Vec<_>>()
            }
        }).collect()
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
        if album_id < 0 {
            return self.source_album_rows(album_id);
        }
        let Some(lib) = self.library() else {
            return Vec::new();
        };
        let f = Filter { album: Some(album_id), ..Filter::default() };
        lib.browse_tracks(&f, TrackOrder::Disc, 10_000).unwrap_or_default().into_iter().map(row_of).collect()
    }

    /// Album tracks with a header for every disc when there is more than
    /// one: its number, and the name and the cover its own files give it.
    pub fn album_track_entries(&mut self, album_id: i64) -> Vec<TrackEntry> {
        let rows = self.album_track_rows(album_id);
        let said = if album_id > 0 && crate::discs::several(&rows) { self.library().and_then(|lib| lib.album_discs(album_id).ok()).unwrap_or_default() } else { Vec::new() };
        crate::album_page::entries(rows, said, |name| album_id > 0 && self.has_artist(name))
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

    /// The records played last, wherever they played: the library's, and a
    /// source's as the catalog's card for it, when the catalog has it.
    pub fn recently_played_everywhere(&mut self, limit: usize) -> Vec<Album> {
        let Some(lib) = self.library() else {
            return Vec::new();
        };
        // More than asked for: a source's record the catalog no longer has
        // is left out.
        let releases = lib.recent_releases(limit.saturating_mul(2)).unwrap_or_default();
        let local: Vec<i64> = releases.iter().filter_map(|r| r.album_id).collect();
        let kept: std::collections::HashMap<i64, Album> =
            lib.albums_by_ids(&local).unwrap_or_default().into_iter().map(album_of).map(|a| (a.id, a)).collect();
        releases.iter().filter_map(|release| match release.album_id {
            Some(id) => kept.get(&id).cloned(),
            None if release.provider == crate::source_opener::PROVIDER => self.catalog
                .card_by_identity(&release.instance, &release.album_external_id)
                .map(|card| crate::source_catalog::view::album_of(&card)),
            None => None,
        }).take(limit).collect()
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
        if album_id < 0 {
            return self.source_album_detail(album_id);
        }
        let album = self.album(album_id)?;
        let rows = self.album_track_rows(album_id);
        let genre = self.library()?.album_genre(album_id).ok().flatten().unwrap_or_default();
        let label = self.library()?.album_label(album_id).ok().flatten().unwrap_or_default();
        let mut discs: Vec<u16> = rows.iter().map(|r| r.disc.max(1)).collect();
        discs.dedup();
        let best = rows.iter().map(|r| &r.quality).max_by_key(|q| (q.tier.rank(), q.bits, q.rate_hz)).cloned().unwrap_or_default();
        Some(AlbumDetail { duration_ms: rows.iter().map(|r| r.duration_ms).sum(), discs: discs.len().max(1), quality: best, genre, label, album })
    }

    /// A badge reads stored metadata; drawing a card never resolves audio.
    pub fn album_card_quality(&mut self, id: i64) -> crate::Quality {
        if id < 0 {
            let Some(card) = self.catalog.card(id) else { return crate::Quality::default() };
            let known = self.catalog.tracks_by_ids(card.tracks.as_deref().unwrap_or_default()).into_iter().filter_map(|(t,_)| t.format).filter(|f| f.rate.is_some()).map(|f| f.quality()).max_by_key(|q| (q.tier.rank(),q.bits,q.rate_hz));
            return known.or_else(|| card.format.map(|f| f.quality())).unwrap_or_default();
        }
        self.album_track_rows(id).into_iter().map(|r| r.quality)
            .max_by_key(|q| (q.tier.rank(), q.bits, q.rate_hz)).unwrap_or_default()
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
    pub fn play_list(&mut self, paths: &[String], shuffled: bool) {
        let kept = self.playable(paths);
        if kept.is_empty() { return; }
        self.shuffle = shuffled;
        let first = self.replace_queue_from(kept, None);
        let (device, exact) = self.chosen_output();
        self.spawn(first, &device, exact);
    }

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
        self.replace_queue_from(kept, Some(start));
        self.spawn(start, &device, bit_perfect);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshot_pages_are_reused_until_the_bounded_cache_evicts_them() {
        let mut pages = PageCache::<i64>::default();
        let first = pages.read(0, 2, || vec![1]);
        assert!(std::rc::Rc::ptr_eq(&first, &pages.read(0, 2, || panic!("a cached page must not query again"))));
        pages.read(1, 2, || vec![2]);
        pages.read(2, 2, || vec![3]);
        assert_eq!(pages.pages.len(), 2);
        assert!(!std::rc::Rc::ptr_eq(&first, &pages.read(0, 2, || vec![4])));
    }

    #[test]
    fn refreshed_header_art_keeps_its_row_but_a_different_group_does_not() {
        let old = GroupHeader { label: "Disc 1".into(), record: true, ..Default::default() };
        let refreshed = GroupHeader { art: "cover.jpg".into(), ..old.clone() };
        assert!(TrackEntry::Header(old.clone()).same_row(&TrackEntry::Header(refreshed)));
        assert!(!TrackEntry::Header(old).same_row(&TrackEntry::header("Disc 2".into(), String::new())));
    }

    fn entry(label: &str) -> FacetEntry {
        FacetEntry { key: label.into(), label: label.into(), albums: 0, year: None, icon: String::new() }
    }

    fn folder(name: &str, tracks: usize, album_id: Option<i64>, part_of: Option<i64>) -> chilpa_library::FolderEntry {
        chilpa_library::FolderEntry { name: name.into(), path: format!("/m/{name}"), tracks, artwork_path: None, folders: 0, album_id, part_of }
    }

    #[test]
    fn the_discs_of_a_box_stand_as_the_box_where_the_first_one_was() {
        let found = grouped(vec![folder("Booklet", 2, None, None), folder("Disc 1", 18, None, Some(7)), folder("Disc 2", 16, None, Some(7)), folder("Single", 9, Some(3), None)]);
        let shown: Vec<(&str, usize, Option<i64>)> = found.iter().map(|f| (f.name.as_str(), f.tracks, f.album_id)).collect();
        assert_eq!(shown, [("Booklet", 2, None), ("Disc 1", 34, Some(7)), ("Single", 9, Some(3))]);
    }

    #[test]
    fn a_disc_alone_stays_a_folder() {
        let found = grouped(vec![folder("Disc 2", 16, None, Some(7))]);
        assert_eq!((found.len(), found[0].album_id, found[0].part_of), (1, None, Some(7)));
    }

    #[test]
    fn facet_queries_ignore_case_and_match_substrings() {
        let labels = vec![entry("Massive Attack"), entry("Björk")];
        assert_eq!(filter_facets(labels.clone(), "attack"), vec![labels[0].clone()]);
        assert_eq!(filter_facets(labels, "björk").len(), 1);
    }

    #[test]
    fn a_rating_entry_says_its_stars_or_that_it_has_none() {
        let label = |key: &str| named_stars(FacetEntry { key: key.into(), label: String::new(), albums: 1, year: None, icon: String::new() }).label;
        assert_eq!((label("5").as_str(), label("1").as_str(), label("0").as_str()), ("5 stars", "1 star", "Unrated"));
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
        let reader = Reader { lib, catalog: None, refresh_cache: Default::default(), sources: Default::default() };
        for artist in ["Singer A", "Singer B", "歌手 A", "歌手 B"] {
            let mut filter = Filter::default();
            filter.queries[5] = artist.into();
            let facets = reader.facets(&filter);
            assert_eq!(facets.artists.iter().find(|a| a.label == artist).unwrap().albums, 1);
            assert_eq!(facets.albums.len(), 1);
            assert_eq!(reader.snapshot(&filter, TrackOrder::Album).ids.len(), 2);
            filter.queries[5].clear();
            filter.artists = vec![artist.into()];
            assert_eq!(reader.snapshot(&filter, TrackOrder::Album).ids.len(), 2);
            assert_eq!(reader.artists("all").iter().find(|a| a.name == artist).unwrap().album_count, 1);
            assert_eq!(reader.artists("all").iter().find(|a| a.name == artist).unwrap().track_count, 1);
        }
        let credited: Vec<(String, usize, usize)> = reader.artists("album").into_iter().map(|a| (a.name, a.album_count, a.track_count)).collect();
        assert_eq!(credited, [("Various Artists".to_string(), 1, 2)], "who plays on a compilation is not credited with it");
    }

    #[test]
    fn long_queue_paths_preserve_order_duplicates_and_missing_ids_across_chunks() {
        let lib = chilpa_library::Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        lib.connection().execute("INSERT INTO albums(id,title,album_artist) VALUES (1,'Box','Band')", []).unwrap();
        lib.connection().execute_batch("BEGIN").unwrap();
        for id in 1..=1205 {
            lib.connection().execute("INSERT INTO tracks(id,source_id,external_id,title,album_id,disc_number,track_number,added_at)
                VALUES (?1,?2,?3,'Song',1,?4,?5,1)", rusqlite::params![id,source,format!("/{id}.flac"),3 - (id - 1) / 500,id]).unwrap();
        }
        lib.connection().execute_batch("COMMIT").unwrap();
        let reader = Reader { lib, catalog: None, refresh_cache: Default::default(), sources: Default::default() };
        let mut ids: Vec<i64> = (1..=1205).rev().collect();
        ids.insert(2, 9999); ids.push(1); ids.push(1205);
        let paths = reader.paths(&ids);
        let expected: Vec<_> = ids.iter().filter(|&&id| id != 9999).map(|id| format!("/{id}.flac")).collect();
        assert_eq!(paths, expected);
        let album = reader.album_paths(&[1]);
        assert_eq!(album.len(), 1205, "the whole album is resolved without a display-row limit");
        assert_eq!(&album[..3], &["/1001.flac", "/1002.flac", "/1003.flac"]);
        assert_eq!(reader.album_paths(&[1,1]).len(), 2410);
        assert!(reader.paths(&[]).is_empty());
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
        let found = Reader { lib, catalog: None, refresh_cache: Default::default(), sources: Default::default() }.folder_matches("rock");
        let paths: Vec<_> = found.folders.iter().map(|folder| folder.path.as_str()).collect();
        assert_eq!(paths, ["/music/Rock", "/music/Rock/Album"]);
    }
}

#[cfg(test)]
#[path = "browse_refresh_tests.rs"]
mod refresh_tests;
#[path = "widest.rs"]
mod widest;
