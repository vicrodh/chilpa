//! The lists: saved, smart and by genre.
//!
//! A saved list has tracks; a smart one has rules and is resolved every time
//! it is looked at. Both come out the same door.

use super::*;
use chilpa_plugin::messages;

/// What the add-to-playlist picker says about one target.
#[derive(Clone, PartialEq, Eq, Debug, serde::Serialize)]
pub struct PlaylistTarget {
    pub playlist: Playlist,
    pub exact: usize,
    pub similar: usize,
    pub accepts: bool,
    /// It belongs where the tracks come from: one of ours for the library's
    /// tracks, or for a mix of sources; a source's own for that source's.
    pub same_source: bool,
    /// What marks it: [`OURS`] for one of ours, the source's own image for
    /// a source's.
    pub icon: String,
}

/// The glyph of Chilpa's own playlists: the tray's mark.
pub const OURS: &str = "chilpa";

/// Where a carried track comes from: a source of the library, or a plugin.
#[derive(Clone, PartialEq, Eq, Debug)]
enum Home {
    Source(i64),
    Plugin(String),
}

/// Where each of the carried tracks comes from, once each.
fn homes_of(entries: &[chilpa_library::PlaylistEntry]) -> Vec<Home> {
    let mut homes = Vec::new();
    for entry in entries {
        let home = match crate::source_opener::of_plugin(&entry.external_id) {
            Some((plugin, _)) => Some(Home::Plugin(plugin)),
            None => entry.source_id.map(Home::Source),
        };
        if let Some(home) = home.filter(|home| !homes.contains(home)) {
            homes.push(home);
        }
    }
    homes
}

pub(crate) fn playlist_of(p: chilpa_library::StoredPlaylist) -> Playlist {
    let local = p.provider == "local";
    Playlist {
        id: p.id,
        source_id: p.source_id,
        name: p.name,
        description: p.description,
        artwork_path: p.artwork_path.unwrap_or_default(),
        covers: Vec::new(),
        tags: p.tags,
        provider: if p.source_label.is_empty() { p.provider.clone() } else { p.source_label },
        instance: p.instance,
        track_count: p.track_count,
        play_count: p.play_count,
        last_played_at: p.last_played_at,
        folder_id: p.folder_id,
        custom_position: p.custom_position,
        visible_sidebar: p.visible_sidebar,
        updated_at: p.updated_at,
        // Source writes are closed until the plugin API exposes the declared
        // capability and operation. Read-only is the safe default.
        writable: p.writable,
        mixed: local,
        source: if local { String::new() } else { p.provider },
        external_id: p.external_id,
    }
}

/// Applies the playlist manager's search, scope and ordering in Rust so every
/// frontend presents the same rows.
pub fn playlist_manager_entries(mut playlists: Vec<Playlist>, mut folders: Vec<PlaylistFolder>, query: &str, filter: &str, sort: &str, ascending: bool, folder_id: Option<i64>) -> Vec<PlaylistManagerEntry> {
    let words: Vec<String> = query.to_lowercase().split_whitespace().map(str::to_string).collect();
    let matches = |text: String| words.iter().all(|word| text.contains(word));
    folders.retain(|folder| folder_id.is_none() && matches(folder.name.to_lowercase()) && (filter != "empty" || folder.playlist_count == 0));
    playlists.retain(|playlist| {
        let in_folder = !words.is_empty() || playlist.folder_id == folder_id;
        let text = format!("{} {} {} {}", playlist.name, playlist.description, playlist.provider, playlist.tags.join(" ")).to_lowercase();
        in_folder && matches(text)
    });
    match sort {
        "recent" => playlists.sort_by_key(|playlist| std::cmp::Reverse(playlist.updated_at)),
        "tracks" => playlists.sort_by_key(|playlist| (std::cmp::Reverse(playlist.track_count), playlist.name.to_lowercase())),
        "plays" => playlists.sort_by_key(|playlist| (std::cmp::Reverse(playlist.play_count), playlist.name.to_lowercase())),
        "custom" => playlists.sort_by_key(|playlist| playlist.custom_position),
        _ => playlists.sort_by_key(|playlist| playlist.name.to_lowercase()),
    }
    if sort == "custom" {
        folders.sort_by_key(|folder| folder.position);
    } else {
        folders.sort_by_key(|folder| folder.name.to_lowercase());
    }
    if !ascending {
        playlists.reverse();
        folders.reverse();
    }
    let mut rows = Vec::new();
    if filter != "playlists" {
        rows.extend(folders.into_iter().map(|folder| PlaylistManagerEntry {
            kind: PlaylistManagerKind::Folder,
            id: folder.id,
            name: folder.name,
            detail: String::new(),
            source: String::new(),
            source_name: String::new(),
            artwork_path: String::new(),
            covers: Vec::new(),
            color: folder.color,
            track_count: folder.playlist_count,
            play_count: 0,
            folder_id: None,
            local: true,
            visible_sidebar: true,
        }));
    }
    if !matches!(filter, "folders" | "empty") {
        rows.extend(playlists.into_iter().map(|playlist| PlaylistManagerEntry {
            kind: PlaylistManagerKind::Playlist,
            id: playlist.id,
            name: playlist.name,
            detail: playlist.description,
            source: playlist.source,
            source_name: playlist.provider,
            artwork_path: playlist.artwork_path,
            covers: playlist.covers,
            color: String::new(),
            track_count: playlist.track_count,
            play_count: playlist.play_count,
            folder_id: playlist.folder_id,
            local: playlist.mixed,
            visible_sidebar: playlist.visible_sidebar,
        }));
    }
    rows
}

/// Where each playlist's chosen track order is kept: one preference, a map
/// from the playlist to its order.
const TRACK_ORDERS: &str = "playlists.trackOrders";
const TRACK_ORDER_NAMES: [&str; 7] = ["custom", "title", "artist", "album", "duration", "added", "plays"];
/// How many playlists keep a chosen order; past it, one is forgotten.
const TRACK_ORDERS_KEPT: usize = 512;

impl Chilpa {
    /// A manager projection ready for thin frontends.
    pub fn managed_playlists(&mut self, query: &str, filter: &str, sort: &str, ascending: bool, folder_id: Option<i64>) -> Vec<PlaylistManagerEntry> {
        let playlists = self.playlists();
        let folders = self.playlist_folders();
        playlist_manager_entries(playlists, folders, query, filter, sort, ascending, folder_id)
    }

    /// Starts a genre playing, shuffled.
    pub fn play_genre(&mut self, genre: &str, device: &str, bit_perfect: bool) -> Vec<String> {
        let list = chilpa_library::SmartList::new(genre, vec![chilpa_library::Rule::GenreIs(genre.to_string())], chilpa_library::SmartOrder::Random, 300);
        let Some(lib) = self.library() else {
            return Vec::new();
        };
        let paths: Vec<String> = lib.smart_tracks(&list).unwrap_or_default().into_iter().map(|t| t.external_id).collect();
        if paths.is_empty() {
            self.now.error = chilpa_i18n::t_args("there is nothing under {}", &[genre]);
            return Vec::new();
        }
        self.play_tracks(&paths, device, bit_perfect)
    }

    /// Starts a smart list playing, by its name.
    pub fn play_smart_list(&mut self, name: &str, device: &str, bit_perfect: bool) -> Vec<String> {
        let Some(list) = chilpa_library::smart_builtins().into_iter().find(|l| l.name == name) else {
            self.now.error = chilpa_i18n::t_args("I do not know the list \"{}\"", &[&chilpa_i18n::t(name)]);
            return Vec::new();
        };
        let Some(lib) = self.library() else {
            return Vec::new();
        };
        let paths: Vec<String> = lib.smart_tracks(&list).unwrap_or_default().into_iter().map(|t| t.external_id).collect();
        if paths.is_empty() {
            self.now.error = chilpa_i18n::t_args("\"{}\" brings nothing today", &[&chilpa_i18n::t(name)]);
            return Vec::new();
        }
        self.play_tracks(&paths, device, bit_perfect)
    }

    /// The saved lists, the ones from here and whatever a source brings.
    ///
    /// A source's lists refresh their cached metadata. A source that does not
    /// answer leaves the last cache intact: absence never means deletion.
    pub fn playlists(&mut self) -> Vec<Playlist> {
        // The window asks for these, so they never wait behind a plugin call
        // in flight (a source's page can take seconds): a busy host leaves
        // the cached lists, and the next ask refreshes them.
        self.library();
        let mut host = self.plugins.as_ref().and_then(|p| p.try_lock().ok());
        if let Some(host) = host.as_mut() {
            for (id, name) in host.searchers() {
                if host.natives.is_native(&id) { continue; }
                refresh_source_playlists(self.library.as_ref(), host, &id, &name);
            }
        }
        drop(host);
        let Some(lib) = self.library() else {
            return Vec::new();
        };
        let mut playlists: Vec<Playlist> = lib.playlists().unwrap_or_default().into_iter().filter(|p| p.provider != crate::source_opener::PROVIDER).map(playlist_of).collect();
        for playlist in &mut playlists {
            playlist.covers = lib.playlist_covers(playlist.id, 4).unwrap_or_default();
        }
        playlists.extend(self.native_playlist_rows());
        playlists
    }

    /// Opens a source's list: its tracks become the results.
    ///
    /// It does not play. A source's track has to be resolved one at a time,
    /// and the queue holds resolved addresses, so putting a whole list in it
    /// would mean asking the source for every signed URL up front. Showing
    /// the tracks means playing one is the same click as playing a search
    /// result, down the same path.
    ///
    /// Returns how many tracks came back.
    pub fn open_source_playlist(&mut self, source: &str, external_id: &str) -> usize {
        let names: std::collections::HashMap<String, String> = self.sources().into_iter().collect();
        let name = names.get(source).cloned().unwrap_or_else(|| source.to_string());
        let request = messages::write(&messages::Resolve::new(external_id));
        let bytes = match self.call_plugin_short(source, messages::op::PLAYLIST_TRACKS, &request) {
            Ok(b) => b,
            Err(e) => {
                self.now.error = format!("{name}: {e}");
                return 0;
            }
        };
        let reply: messages::Results = match messages::read(&bytes) {
            Ok(r) => r,
            Err(e) => {
                self.now.error = format!("{name}: {e}");
                return 0;
            }
        };
        let mut refreshed = None;
        if let Some(lib) = self.library() {
            if let Ok(source_id) = lib.source_id(source, "default") {
                let playlist_id = lib.playlists().unwrap_or_default().into_iter().find(|playlist| playlist.source_id == source_id && playlist.external_id == external_id).map(|playlist| playlist.id).or_else(|| lib.create_playlist(source_id, external_id, external_id).ok());
                if let Some(playlist_id) = playlist_id {
                    let tracks: Vec<chilpa_library::PlaylistTrackMetadata> = reply
                        .tracks
                        .iter()
                        .map(|track| chilpa_library::PlaylistTrackMetadata {
                            external_id: track.external_id.clone(),
                            title: track.title.clone(),
                            artist: track.artist.clone(),
                            album: track.album.clone(),
                            duration_ms: track.duration_ms,
                            sample_rate_hz: track.quality.sample_rate_hz,
                            bit_depth: track.quality.bit_depth.map(u16::from),
                            channels: track.quality.channels,
                            codec: track.quality.codec.clone(),
                            isrc: track.isrc.clone(),
                            recording_mbid: track.musicbrainz_recording_id.clone(),
                            artwork_path: track.artwork_url.clone(),
                        })
                        .collect();
                    let _ = lib.sync_source_playlist_tracks(source_id, playlist_id, &tracks);
                    refreshed = Some(playlist_id);
                }
            }
        }
        if let Some(id) = refreshed { self.refresh_playlist_roots(id); }
        let plays = self.plugins.as_ref().and_then(|p| p.lock().ok()).is_none_or(|p| p.delivers_audio(source));
        let hits: Vec<search::Hit> = reply.tracks.into_iter().map(|t| search::Hit::remote(t, source, &name, plays)).collect();
        let found = hits.len();
        self.search.text = String::new();
        self.search.replies = Vec::new();
        self.search.local = Vec::new();
        self.search.hits = hits;
        self.search.changed = true;
        if found == 0 {
            self.now.error = chilpa_i18n::t_args("\"{}\" brings nothing today", &[&name]);
        }
        found
    }

    /// Saves the queue as a list under that name: what plays now and what
    /// comes next, in the order it will play (design/04 7). What already
    /// played stays out.
    ///
    /// It returns the list's id. Whatever is not in the library is saved
    /// anyway, by its path: the list holds even with the scan still pending.
    pub fn save_queue_as(&mut self, name: &str) -> Option<i64> {
        let paths: Vec<String> = self.queue_order().into_iter().map(|i| self.queue[i].clone()).collect();
        self.save_paths_as(name, &paths)
    }

    pub(crate) fn save_paths_as(&mut self, name: &str, paths: &[String]) -> Option<i64> {
        let name = name.trim();
        if name.is_empty() { self.now.error = chilpa_i18n::t("a list needs a name"); return None; }
        if paths.is_empty() { self.now.error = chilpa_i18n::t("the queue is empty"); return None; }
        for path in paths {
            if let Some((track, _)) = self.catalog.track_by_path(path) {
                self.mixed_playlist_entries(&[track.id], true)?;
            }
        }
        let lib = self.library()?;
        let source = lib.source_id("local", "default").ok()?;
        let entries: Vec<chilpa_library::PlaylistEntry> = paths.iter().map(|r| entry_of(lib, source, r)).collect();
        let id = lib.create_playlist(source, &name, "").ok()?;
        lib.set_playlist_entries(id, &entries).ok()?;
        Some(id)
    }

    /// Starts a saved list playing.
    pub fn play_saved_playlist(&mut self, playlist_id: i64, device: &str, bit_perfect: bool) -> Vec<String> {
        let Some(lib) = self.library() else {
            return Vec::new();
        };
        let _ = lib;
        let paths: Vec<String> = self.playlist_rows(playlist_id).into_iter().filter(|row| row.available).map(|row| row.path).collect();
        if paths.is_empty() {
            self.now.error = chilpa_i18n::t("that list is empty");
            return Vec::new();
        }
        if let Some(lib) = self.library() { let _ = lib.record_playlist_play(playlist_id); }
        self.play_tracks(&paths, device, bit_perfect)
    }

    /// Makes an empty list under that name. Returns its id.
    pub fn create_playlist(&mut self, name: &str) -> Option<i64> {
        let name = name.trim().to_string();
        if name.is_empty() {
            self.now.error = chilpa_i18n::t("a list needs a name");
            return None;
        }
        let lib = self.library()?;
        let source = lib.source_id("local", "default").ok()?;
        lib.create_playlist(source, &name, "").ok()
    }

    /// Makes a local playlist with its complete editable metadata.
    pub fn create_playlist_with(&mut self, name: &str, description: &str, artwork_path: Option<&str>, tags: &[String], folder_id: Option<i64>) -> Option<i64> {
        let name = name.trim();
        if name.is_empty() {
            self.now.error = chilpa_i18n::t("a list needs a name");
            return None;
        }
        let lib = self.library()?;
        let source = lib.source_id("local", "default").ok()?;
        lib.create_playlist_with(source, &chilpa_library::PlaylistEdit { name: name.to_string(), description: description.trim().to_string(), artwork_path: artwork_path.filter(|path| !path.is_empty()).map(str::to_string), tags: tags.to_vec(), folder_id }).ok()
    }

    /// Adds files to the end of a list, leaving out what it already
    /// carries. Returns how many went in.
    pub fn add_to_playlist(&mut self, playlist_id: i64, paths: &[String]) -> usize {
        let Some(lib) = self.library() else { return 0 };
        let Ok(source) = lib.source_id("local", "default") else {
            return 0;
        };
        let entries: Vec<chilpa_library::PlaylistEntry> = paths.iter().map(|r| entry_of(lib, source, r)).collect();
        let added = lib.append_to_playlist(playlist_id, &entries).unwrap_or(0);
        if added > 0 { self.refresh_playlist_roots(playlist_id); }
        added
    }

    /// Picker targets for a library selection, including exact and fuzzy
    /// presence counts. Fuzzy presence only warns; it never blocks adding.
    pub fn playlist_targets(&mut self, track_ids: &[i64], query: &str) -> Vec<PlaylistTarget> {
        let Some(lib) = self.library() else {
            return Vec::new();
        };
        let _ = lib;
        let entries = self.mixed_playlist_entries(track_ids, false).unwrap_or_default();
        let native = self.native_playlist_rows();
        let Some(lib) = self.library() else { return Vec::new() };
        let local = lib.source_id("local", "default").ok();
        let homes = homes_of(&entries);
        let plugins = Self::plugins_dir();
        let words: Vec<String> = query.to_lowercase().split_whitespace().map(str::to_string).collect();
        let mut targets: Vec<_> = lib.playlists()
            .unwrap_or_default()
            .into_iter()
            .map(playlist_of)
            .filter(|playlist| {
                let name = playlist.name.to_lowercase();
                words.iter().all(|word| name.contains(word))
            })
            .map(|playlist| {
                let membership = lib.playlist_membership(playlist.id, &entries).unwrap_or_default();
                let accepts = playlist.writable && (playlist.mixed || entries.iter().all(|entry| entry.source_id == Some(playlist.source_id)));
                // Ours are the library's home, and the home of a mix.
                let home = if playlist.mixed { local.map(Home::Source) } else { Some(Home::Source(playlist.source_id)) };
                let same_source = home.is_some_and(|home| homes.iter().all(|h| *h == home) || (playlist.mixed && homes.len() > 1));
                let icon = if playlist.mixed { OURS.to_string() } else { String::new() };
                PlaylistTarget { playlist, exact: membership.exact, similar: membership.similar, accepts, same_source, icon }
            })
            .collect();
        let complete = entries.len() == track_ids.len();
        for playlist in native {
            let accepts = complete && playlist.writable && entries.iter().all(|entry| crate::source_opener::of_plugin(&entry.external_id).is_some_and(|(plugin, _)| plugin == playlist.source));
            if words.iter().all(|word| playlist.name.to_lowercase().contains(word)) {
                let same_source = !homes.is_empty() && homes.iter().all(|h| *h == Home::Plugin(playlist.source.clone()));
                let icon = crate::plugin_icons::image_url(&plugins, &playlist.source);
                targets.push(PlaylistTarget { playlist, exact: 0, similar: 0, accepts, same_source, icon });
            }
        }
        targets.retain(|target| target.playlist.source != crate::source_opener::PROVIDER);
        if !complete { for target in &mut targets { target.accepts = false; } }
        targets.sort_by_key(|target| target.playlist.name.to_lowercase());
        targets
    }

    pub fn add_track_ids_to_playlist(&mut self, playlist_id: i64, track_ids: &[i64]) -> PlaylistAddResult {
        self.add_track_ids(playlist_id, track_ids, false)
    }

    /// The same, adding again what the playlist already carries: the
    /// picker's "Add all tracks".
    pub fn add_track_ids_again(&mut self, playlist_id: i64, track_ids: &[i64]) -> PlaylistAddResult {
        self.add_track_ids(playlist_id, track_ids, true)
    }

    fn add_track_ids(&mut self, playlist_id: i64, track_ids: &[i64], repeats: bool) -> PlaylistAddResult {
        let Some(lib) = self.library() else {
            return PlaylistAddResult { rejected: true, ..PlaylistAddResult::default() };
        };
        let Some(target) = lib.playlists().ok().and_then(|lists| lists.into_iter().find(|playlist| playlist.id == playlist_id)) else {
            return PlaylistAddResult { rejected: true, ..PlaylistAddResult::default() };
        };
        let playlist = playlist_of(target);
        let _ = lib;
        let Some(entries) = self.mixed_playlist_entries(track_ids, true) else {
            return PlaylistAddResult { rejected: true, ..PlaylistAddResult::default() };
        };
        if entries.len() != track_ids.len() {
            return PlaylistAddResult { rejected: true, ..PlaylistAddResult::default() };
        }
        let accepts = playlist.writable && (playlist.mixed || entries.iter().all(|entry| entry.source_id == Some(playlist.source_id)));
        if !accepts {
            return PlaylistAddResult { rejected: true, ..PlaylistAddResult::default() };
        }
        let Some(lib) = self.library() else { return PlaylistAddResult { rejected: true, ..Default::default() }; };
        let Ok(membership) = lib.playlist_membership(playlist_id, &entries) else {
            return PlaylistAddResult { rejected: true, ..PlaylistAddResult::default() };
        };
        let appended = if repeats { lib.append_again_to_playlist(playlist_id, &entries) } else { lib.append_to_playlist(playlist_id, &entries) };
        let Ok(added) = appended else {
            return PlaylistAddResult { rejected: true, ..PlaylistAddResult::default() };
        };
        if added > 0 { self.refresh_playlist_roots(playlist_id); }
        PlaylistAddResult { added, exact: membership.exact, similar: membership.similar, rejected: false }
    }

    pub fn remove_tracks_from_playlist(&mut self, playlist_id: i64, track_ids: &[i64]) -> usize {
        if !self.playlist(playlist_id).is_some_and(|playlist| playlist.mixed) {
            return 0;
        }
        let removed = self.library().and_then(|lib| lib.remove_tracks_from_playlist(playlist_id, track_ids).ok()).unwrap_or(0);
        if removed > 0 { self.refresh_playlist_roots(playlist_id); }
        removed
    }

    pub fn set_tracks_favorite(&mut self, track_ids: &[i64], favorite: bool) -> bool {
        let (remote, local): (Vec<i64>, Vec<i64>) = track_ids.iter().partition(|id| **id < 0);
        if !remote.is_empty() {
            self.set_source_tracks_favorite(&remote, favorite);
        }
        if local.is_empty() {
            return !remote.is_empty();
        }
        let track_ids = &local[..];
        self.library().is_some_and(|lib| lib.set_track_favorites(track_ids, favorite).is_ok())
    }

    /// Moves a track of a list from one position to another.
    pub fn move_in_playlist(&mut self, playlist_id: i64, from: usize, to: usize) -> bool {
        if !self.playlist(playlist_id).is_some_and(|playlist| playlist.mixed) {
            return false;
        }
        self.library().is_some_and(|lib| lib.move_in_playlist(playlist_id, from, to).unwrap_or(false))
    }

    /// Moves several tracks of a list, dragged together, to where one was
    /// dropped. True if any of them moved.
    pub fn move_group_in_playlist(&mut self, playlist_id: i64, rows: &[usize], to: usize) -> bool {
        let mut moved = false;
        for (from, to) in group_moves(rows, to) {
            moved |= self.move_in_playlist(playlist_id, from, to);
        }
        moved
    }

    /// Takes the track at that position out of a list.
    pub fn remove_from_playlist(&mut self, playlist_id: i64, position: usize) -> bool {
        if !self.playlist(playlist_id).is_some_and(|playlist| playlist.mixed) {
            return false;
        }
        let removed = self.library().is_some_and(|lib| lib.remove_from_playlist(playlist_id, position).unwrap_or(false));
        if removed { self.refresh_playlist_roots(playlist_id); }
        removed
    }

    /// Our own lists, newest first, without asking any source for theirs.
    pub fn local_playlists(&mut self) -> Vec<Playlist> {
        let Some(lib) = self.library() else {
            return Vec::new();
        };
        lib.playlists().unwrap_or_default().into_iter().filter(|p| p.provider == "local").map(playlist_of).collect()
    }

    /// Every cached playlist, without touching a source. Used by the manager
    /// and the picker after the facade's refresh path has run.
    pub fn cached_playlists(&mut self) -> Vec<Playlist> {
        let Some(lib) = self.library() else {
            return Vec::new();
        };
        let mut playlists: Vec<Playlist> = lib.playlists().unwrap_or_default().into_iter().filter(|p| p.provider != crate::source_opener::PROVIDER).map(playlist_of).collect();
        for playlist in &mut playlists {
            playlist.covers = lib.playlist_covers(playlist.id, 4).unwrap_or_default();
        }
        playlists.extend(self.native_playlist_rows());
        playlists
    }

    /// One of our lists, by its id. From the library alone: asking the
    /// sources for theirs to find one of ours would be a network round.
    pub fn playlist(&mut self, playlist_id: i64) -> Option<Playlist> {
        if playlist_id < 0 { return self.native_playlist_rows().into_iter().find(|p| p.id == playlist_id); }
        let p = self.library()?.playlists().ok()?.into_iter().find(|p| p.id == playlist_id)?;
        Some(playlist_of(p))
    }

    /// The covers of a list's first records, up to four: its mosaic.
    pub fn playlist_covers(&mut self, playlist_id: i64) -> Vec<String> {
        self.library().map(|lib| lib.playlist_covers(playlist_id, 4).unwrap_or_default()).unwrap_or_default()
    }

    /// Deletes a list. The tracks are not touched.
    pub fn delete_playlist(&mut self, playlist_id: i64) {
        if let Some(lib) = self.library() {
            let _ = lib.delete_playlist(playlist_id);
        }
        self.refresh_playlist_roots(playlist_id);
    }

    /// Renames a list.
    pub fn rename_playlist(&mut self, playlist_id: i64, name: &str) {
        let name = name.trim().to_string();
        if name.is_empty() {
            self.now.error = chilpa_i18n::t("a list needs a name");
            return;
        }
        if let Some(lib) = self.library() {
            let _ = lib.rename_playlist(playlist_id, &name);
        }
    }

    /// Changes a local playlist's metadata. Source-owned metadata belongs to
    /// its source and is therefore left alone.
    pub fn update_playlist(&mut self, playlist_id: i64, name: &str, description: &str, artwork_path: Option<&str>, tags: &[String], folder_id: Option<i64>) -> bool {
        let Some(playlist) = self.playlist(playlist_id) else {
            return false;
        };
        if !playlist.mixed || name.trim().is_empty() {
            return false;
        }
        self.library()
            .is_some_and(|lib| lib.update_playlist(playlist_id, &chilpa_library::PlaylistEdit { name: name.trim().to_string(), description: description.trim().to_string(), artwork_path: artwork_path.filter(|path| !path.is_empty()).map(str::to_string), tags: tags.to_vec(), folder_id }).is_ok())
    }

    pub fn playlist_folders(&mut self) -> Vec<PlaylistFolder> {
        self.library().and_then(|lib| lib.playlist_folders().ok()).unwrap_or_default().into_iter().map(|folder| PlaylistFolder { id: folder.id, name: folder.name, color: folder.color, position: folder.position, playlist_count: folder.playlist_count }).collect()
    }

    pub fn create_playlist_folder(&mut self, name: &str, color: &str) -> Option<i64> {
        let name = name.trim();
        if name.is_empty() {
            return None;
        }
        self.library()?.create_playlist_folder(name, color).ok()
    }

    pub fn update_playlist_folder(&mut self, id: i64, name: &str, color: &str) -> bool {
        !name.trim().is_empty() && self.library().is_some_and(|lib| lib.update_playlist_folder(id, name.trim(), color).is_ok())
    }

    pub fn delete_playlist_folder(&mut self, id: i64) -> bool {
        self.library().is_some_and(|lib| lib.delete_playlist_folder(id).is_ok())
    }

    pub fn move_playlist_to_folder(&mut self, id: i64, folder: Option<i64>) -> bool {
        let Some(id) = self.playlist_storage_id(id) else { return false };
        self.library().is_some_and(|lib| lib.move_playlist_to_folder(id, folder).is_ok())
    }

    pub fn set_playlist_visible(&mut self, id: i64, visible: bool) -> bool {
        let Some(id) = self.playlist_storage_id(id) else { return false };
        self.library().is_some_and(|lib| lib.set_playlist_visible(id, visible).is_ok())
    }

    pub fn set_playlist_custom_order(&mut self, ids: &[i64]) -> bool {
        let ids: Vec<_> = ids.iter().filter_map(|id| self.playlist_storage_id(*id)).collect();
        self.library().is_some_and(|lib| lib.set_playlist_custom_order(&ids).is_ok())
    }

    /// A playlist's key across sessions: a source's by its identity there,
    /// since a catalog id lasts a session; one of the library's by its id.
    fn playlist_order_key(&self, id: i64) -> Option<String> {
        if id < 0 {
            return self.catalog.identity_of(id).map(|(plugin, identity)| format!("{plugin} {identity}"));
        }
        (id > 0).then(|| format!("library {id}"))
    }

    fn kept_track_orders(&self) -> serde_json::Map<String, serde_json::Value> {
        self.preference(TRACK_ORDERS).and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
    }

    /// The order a playlist's tracks were last shown in, as the sort button
    /// names it ("title", "added-asc"): its own order until one is chosen.
    pub fn playlist_track_order(&self, id: i64) -> String {
        let kept = self.kept_track_orders();
        self.playlist_order_key(id).and_then(|key| kept.get(&key)?.as_str().map(str::to_string)).unwrap_or_else(|| "custom".into())
    }

    /// Keeps the order chosen for a playlist's tracks, for the next time it opens.
    pub fn set_playlist_track_order(&mut self, id: i64, order: &str) {
        let named = order.strip_suffix("-desc").or_else(|| order.strip_suffix("-asc")).unwrap_or(order);
        if !TRACK_ORDER_NAMES.contains(&named) { return; }
        let Some(key) = self.playlist_order_key(id) else { return };
        let mut kept = self.kept_track_orders();
        kept.remove(&key);
        if order != "custom" {
            while kept.len() >= TRACK_ORDERS_KEPT {
                let Some(first) = kept.keys().next().cloned() else { break };
                kept.remove(&first);
            }
            kept.insert(key, serde_json::json!(order));
        }
        self.remember_preference(TRACK_ORDERS, &serde_json::Value::Object(kept).to_string());
    }

    /// A playlist that starts playing from its own list, a source's as well
    /// as ours: Home's recently listened playlists take it.
    pub fn playlist_started(&mut self, id: i64) {
        if id < 0 && self.catalog.card(id).is_some_and(|card| card.kind == crate::source_catalog::store::CardKind::Playlist) {
            self.catalog.record_playlist_play(id);
        }
        self.count_playlist_play(id);
    }

    pub fn count_playlist_play(&mut self, id: i64) {
        if let Some(id) = self.playlist_storage_id(id) {
            if let Some(lib) = self.library() { let _ = lib.record_playlist_play(id); }
        }
        self.home.changed();
    }

    /// Lists that actually started playing, newest first. The local view
    /// includes mixed lists; the all-sources view also includes source lists.
    pub fn recently_listened_playlists(&mut self, all_sources: bool, limit: usize) -> String {
        let disabled: std::collections::HashSet<i64> = self.library().and_then(|lib| lib.servers().ok())
            .unwrap_or_default().into_iter().filter(|server| !server.enabled).map(|server| server.source_id).collect();
        let mut rows: Vec<serde_json::Value> = self.playlists().into_iter()
            .filter(|playlist| playlist.last_played_at.is_some() && (all_sources || playlist.mixed)
                && !disabled.contains(&playlist.source_id))
            .filter_map(|playlist| serde_json::to_value(playlist).ok()).collect();
        if all_sources {
            for card in self.catalog.played_playlist_cards().into_iter().filter(|card| self.natives.is_native(&card.plugin)) {
                if rows.iter().any(|row| row["id"] == card.id) { continue; }
                let name = self.catalog.name(&card.plugin).unwrap_or_else(|| card.plugin.clone());
                rows.push(serde_json::json!({"id":card.id,"name":card.title,"provider":name,"source":card.plugin,
                    "artwork_path":card.art,"covers":[],"mixed":false,"last_played_at":card.last_played_at}));
            }
        }
        rows.sort_by_key(|playlist| std::cmp::Reverse(playlist["last_played_at"].as_i64().unwrap_or_default()));
        serde_json::to_string(&rows.into_iter().take(limit).collect::<Vec<_>>()).unwrap_or_else(|_| "[]".into())
    }
}

/// A queue entry as a playlist keeps it: a server's track with its server,
/// anything else as a local file, found in the library or not.
fn entry_of(lib: &chilpa_library::Library, local: i64, path: &str) -> chilpa_library::PlaylistEntry {
    if let Some((track, source)) = lib.track_by_entry(path).ok().flatten() {
        return chilpa_library::PlaylistEntry { track_id: Some(track), source_id: Some(source), external_id: path.to_string() };
    }
    chilpa_library::PlaylistEntry { track_id: lib.track_by_path(local, std::path::Path::new(path)).ok().flatten(), source_id: Some(local), external_id: path.to_string() }
}

/// The single moves that take the rows of a group to where it was dropped,
/// as one block and in the order they had.
///
/// The rows above the target go down to it, the last one first, and the
/// rows below come up after them: a move above the target never shifts a
/// row below it, so the positions stay true from one move to the next.
pub fn group_moves(rows: &[usize], to: usize) -> Vec<(usize, usize)> {
    let mut rows = rows.to_vec();
    rows.sort_unstable();
    rows.dedup();
    let on_target = rows.contains(&to);
    let above: Vec<usize> = rows.iter().copied().filter(|&row| row < to).collect();
    let below: Vec<usize> = rows.iter().copied().filter(|&row| row > to).collect();
    let mut moves = Vec::new();
    let last = if on_target { to.saturating_sub(1) } else { to };
    for (step, &row) in above.iter().rev().enumerate() {
        moves.push((row, last - step));
    }
    let first = if on_target || !above.is_empty() { to + 1 } else { to };
    for (step, &row) in below.iter().enumerate() {
        moves.push((row, first + step));
    }
    moves.retain(|(from, to)| from != to);
    moves
}

/// Refreshes what one source says its lists are, on a host already taken.
/// A failed refresh changes nothing in the cache.
fn refresh_source_playlists(library: Option<&chilpa_library::Library>, host: &mut crate::plugins::Plugins, id: &str, name: &str) {
    if !host.playlists_due(id) { return; }
    let request = messages::write(&messages::Query::new("", messages::MAX_PER_REPLY));
    let Ok(bytes) = host.call_short(id, messages::op::PLAYLISTS, &request) else {
        return;
    };
    let Ok(reply) = messages::read::<messages::Results>(&bytes) else {
        return;
    };
    let Some(lib) = library else { return };
    let Ok(source_id) = lib.source_id(id, "default") else {
        return;
    };
    let _ = lib.set_source_label(source_id, name);
    for playlist in reply.playlists {
        let _ = lib.sync_source_playlist(source_id, &playlist.external_id, &playlist.name, &playlist.description, playlist.artwork_url.as_deref());
    }
}

#[cfg(test)]
mod group_tests {
    use super::*;

    #[test]
    fn each_playlist_keeps_the_order_chosen_for_it() {
        let mut api = crate::Chilpa::new();
        assert_eq!(api.playlist_track_order(7), "custom", "its own order until one is chosen");
        api.set_playlist_track_order(7, "added-asc");
        api.set_playlist_track_order(8, "title");
        assert_eq!((api.playlist_track_order(7).as_str(), api.playlist_track_order(8).as_str()), ("added-asc", "title"));
        api.set_playlist_track_order(7, "custom");
        assert_eq!(api.playlist_track_order(7), "custom");
        api.set_playlist_track_order(8, "colour");
        assert_eq!(api.playlist_track_order(8), "title", "an order nobody offers is not kept");
    }

    #[test]
    fn date_added_puts_the_last_to_come_in_first() {
        let row = |title: &str| { let mut row = crate::browse::file_row("/music/a.flac"); row.title = title.into(); row };
        let mut rows = vec![row("first"), row("second"), row("third")];
        crate::browse::sort_playlist_rows(&mut rows, "added", true);
        assert_eq!(rows.iter().map(|r| r.title.as_str()).collect::<Vec<_>>(), ["third", "second", "first"]);
        let mut rows = vec![row("first"), row("second"), row("third")];
        crate::browse::sort_playlist_rows(&mut rows, "added", false);
        assert_eq!(rows[0].title, "first", "the other way round, the first to come in first");
        let entries = vec![crate::TrackEntry::Track(row("b")), crate::TrackEntry::Track(row("a"))];
        let sorted = crate::ordered_entries(entries, "title", true);
        assert!(matches!(&sorted[0], crate::TrackEntry::Track(t) if t.title == "a"));
    }

    #[test]
    fn a_playlist_started_from_its_own_list_is_recently_listened() {
        let lib = chilpa_library::Library::in_memory().unwrap();
        let local = lib.source_id("local", "default").unwrap();
        let id = lib.create_playlist(local, "Local list", "").unwrap();
        let mut api = crate::Chilpa::new();
        api.library = Some(lib);
        api.playlist_started(id);
        let local: serde_json::Value = serde_json::from_str(&api.recently_listened_playlists(false, 10)).unwrap();
        assert_eq!(local[0]["name"], "Local list");
        assert_eq!(local[0]["play_count"], 1);
    }

    #[test]
        fn recent_playlists_keep_local_and_all_sources_separate() {
        let lib = chilpa_library::Library::in_memory().unwrap();
        let local = lib.source_id("local", "default").unwrap();
        let remote = lib.source_id("fixture", "account").unwrap();
        let local_id = lib.create_playlist(local, "Local list", "").unwrap();
        let remote_id = lib.create_playlist(remote, "Source list", "").unwrap();
        let mut api = crate::Chilpa::new();
        api.library = Some(lib);
        assert_eq!(api.recently_listened_playlists(true, 10), "[]");
        api.count_playlist_play(local_id);
        api.count_playlist_play(remote_id);
        let local: serde_json::Value = serde_json::from_str(&api.recently_listened_playlists(false, 10)).unwrap();
        let all: serde_json::Value = serde_json::from_str(&api.recently_listened_playlists(true, 10)).unwrap();
        assert_eq!(local.as_array().unwrap().len(), 1);
        assert_eq!(local[0]["name"], "Local list");
        assert_eq!(all.as_array().unwrap().len(), 2);
        assert!(all.as_array().unwrap().iter().any(|row| row["name"] == "Source list"));
    }

    fn dropped(rows: &[usize], to: usize) -> String {
        let mut list: Vec<char> = "abcdef".chars().collect();
        for (from, to) in group_moves(rows, to) {
            let track = list.remove(from);
            list.insert(to, track);
        }
        list.into_iter().collect()
    }

    #[test]
    fn a_group_lands_as_one_block_in_its_order() {
        assert_eq!(dropped(&[1, 4], 2), "acbedf", "one from above and one from below meet at the target");
        assert_eq!(dropped(&[4, 5], 1), "aefbcd", "a group from below lands where it was dropped");
        assert_eq!(dropped(&[0, 1], 3), "cdabef", "a group from above keeps its order");
        assert_eq!(dropped(&[1, 3, 5], 3), "acbdfe", "dropped on one of its own, the group closes around it");
        assert_eq!(dropped(&[2, 3], 2), "abcdef", "a block dropped on itself does not move");
        assert_eq!(dropped(&[4], 1), "aebcdf", "one row moves as it always did");
    }

    #[test]
    fn the_lists_never_wait_for_a_plugin_call_in_flight() {
        let mut api = crate::Chilpa::new();
        api.library = Some(chilpa_library::Library::in_memory().unwrap());
        let dir = std::env::temp_dir().join(format!("chilpa-playlists-busy-{}", std::process::id()));
        let host = std::sync::Arc::new(std::sync::Mutex::new(crate::plugins::Plugins::new(dir).unwrap()));
        api.plugins = Some(host.clone());
        // A call in flight holds the host for as long as the source takes.
        let (taken, wait) = std::sync::mpsc::channel();
        let busy = std::thread::spawn(move || {
            let _held = host.lock().unwrap();
            taken.send(()).unwrap();
            std::thread::sleep(std::time::Duration::from_secs(5));
        });
        wait.recv().unwrap();
        let started = std::time::Instant::now();
        api.playlists();
        assert!(started.elapsed() < std::time::Duration::from_secs(1), "the window waited {:?} for the host", started.elapsed());
        busy.join().unwrap();
    }
}
