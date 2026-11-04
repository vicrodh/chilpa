//! A server's own playlists as the window shows and changes them (ADR-017,
//! as amended for them).
//!
//! The library keeps each one as the server's: in the server's order, under
//! the server's name and icon, and changed only through the server. A
//! change the window asks for is sent (`writing`) and the playlist stays as
//! it is until the server answers. Two servers of one kind are two sources,
//! told apart by the key the library's filters already use for their
//! tracks, `provider:instance`.

use super::writing::{Change, Write};
use crate::{Chilpa, PlaylistAddResult};
use std::collections::HashSet;

/// A server as its playlists are labelled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Origin {
    /// Its source's id.
    pub(crate) source_id: i64,
    /// What `Playlist::source` says for its playlists.
    pub(crate) key: String,
    pub(crate) name: String,
    /// Its glyph among the source icons.
    pub(crate) icon: String,
    pub(crate) enabled: bool,
}

/// The source a server's playlists carry: the key of its tracks in the
/// library's filters (`Library::facet_sources`).
pub(crate) fn source_key(provider: &str, instance: &str) -> String {
    format!("{provider}:{instance}")
}

impl Chilpa {
    /// Every server, as its playlists are labelled, in the order they were
    /// added.
    pub(crate) fn server_origins(&mut self) -> Vec<Origin> {
        let Some(lib) = self.library() else { return Vec::new() };
        lib.servers()
            .unwrap_or_default()
            .into_iter()
            .map(|s| {
                let kind = chilpa_servers::Kind::from_provider(&s.provider);
                Origin {
                    source_id: s.source_id,
                    key: source_key(&s.provider, &s.instance),
                    name: if s.label.is_empty() { kind.map(|k| k.name().to_string()).unwrap_or_default() } else { s.label.clone() },
                    icon: kind.map(|k| k.icon(&s.product)).unwrap_or("server").to_string(),
                    enabled: s.enabled,
                }
            })
            .collect()
    }

    /// Whether a playlist's source is a server: no plugin is asked for its
    /// tracks, the library holds them.
    pub(crate) fn is_server_source(&mut self, source: &str) -> bool {
        !source.is_empty() && self.server_origins().iter().any(|origin| origin.key == source)
    }

    /// The sources of the servers that are off. Their playlists stay in the
    /// library and are not shown anywhere until the server is on again.
    pub(crate) fn disabled_server_sources(&mut self) -> HashSet<i64> {
        self.server_origins().into_iter().filter(|origin| !origin.enabled).map(|origin| origin.source_id).collect()
    }

    /// One of the library's playlists, when it is a server's.
    fn server_playlist_of(&mut self, id: i64) -> Option<chilpa_library::StoredPlaylist> {
        if id <= 0 {
            return None;
        }
        self.library()?.playlists().ok()?.into_iter().find(|p| p.id == id && p.server)
    }

    /// Whether a change of that server playlist can be sent now: the
    /// account may change it, the network is on, the server answers and no
    /// other change of it is on its way.
    pub(crate) fn server_playlist_open(&mut self, playlist: &crate::Playlist) -> bool {
        if !playlist.writable || self.offline() || self.server_change_pending(playlist.id) {
            return false;
        }
        self.library().and_then(|lib| lib.server(playlist.source_id).ok().flatten()).is_some_and(|s| s.enabled && s.reachable)
    }

    /// What a playlist page shows of a server's playlist, as JSON: the
    /// server's name and icon, whether the account may change it, whether
    /// a change is on its way, and how many of its entries are not in the
    /// library. `{}` for any other playlist.
    pub fn server_playlist_json(&mut self, id: i64) -> String {
        let Some(stored) = self.server_playlist_of(id) else { return "{}".into() };
        let Some(origin) = self.server_origins().into_iter().find(|origin| origin.source_id == stored.source_id) else { return "{}".into() };
        let pending = self.server_change_pending(id);
        let describable = chilpa_servers::Kind::from_provider(&stored.provider).is_some_and(chilpa_servers::playlist_describable);
        serde_json::json!({"server": origin.name, "icon": origin.icon, "writable": stored.writable, "pending": pending, "left_out": stored.left_out, "describable": describable}).to_string()
    }

    /// The servers by the source their playlists carry, with their names
    /// and icons, as JSON: what the playlist manager names and draws.
    pub fn playlist_servers_json(&mut self) -> String {
        let servers: serde_json::Map<String, serde_json::Value> =
            self.server_origins().into_iter().map(|origin| (origin.key, serde_json::json!({"name": origin.name, "icon": origin.icon, "enabled": origin.enabled}))).collect();
        serde_json::Value::Object(servers).to_string()
    }

    /// Sends a change of one of the library's playlists to its server.
    /// `None` when it is not a server's; `Some(false)` when it could not
    /// go, and the window says why.
    pub(crate) fn change_server_playlist(&mut self, id: i64, change: Change) -> Option<bool> {
        let stored = self.server_playlist_of(id)?;
        if matches!(&change, Change::Remove { rows } if rows.is_empty()) {
            return Some(false);
        }
        if !stored.writable {
            self.now.error = chilpa_i18n::t("this user may not do that on the server");
            return Some(false);
        }
        match self.ask_server_change(Write { source: stored.source_id, playlist: id, request: String::new(), change }) {
            Ok(()) => Some(true),
            Err(why) => {
                self.now.error = why;
                Some(false)
            }
        }
    }

    /// Sends what the edit dialog changed of a server's playlist: its name
    /// and, where the server takes one, its description. Its folder here is
    /// the library's own and changes at once. `None` when it is not a
    /// server's.
    pub(crate) fn edit_server_playlist(&mut self, id: i64, name: &str, description: &str, folder_id: Option<i64>) -> Option<bool> {
        let stored = self.server_playlist_of(id)?;
        let (name, description) = (name.trim(), description.trim());
        if name.is_empty() {
            return Some(false);
        }
        let filed = stored.folder_id == folder_id || self.library().is_some_and(|lib| lib.move_playlist_to_folder(id, folder_id).is_ok());
        let change = Change::Edit { name: (name != stored.name).then(|| name.to_string()), description: (description != stored.description).then(|| description.to_string()) };
        if matches!(&change, Change::Edit { name: None, description: None }) {
            return Some(filed);
        }
        Some(self.change_server_playlist(id, change)? && filed)
    }

    /// The rows of a server's playlist that hold those library tracks.
    pub(crate) fn server_playlist_rows(&mut self, id: i64, track_ids: &[i64]) -> Vec<usize> {
        let entries = self.library().and_then(|lib| lib.playlist_entries(id).ok()).unwrap_or_default();
        entries.iter().enumerate().filter(|(_, e)| e.track_id.is_some_and(|t| track_ids.contains(&t))).map(|(row, _)| row).collect()
    }

    /// How many rows a server's playlist shows.
    pub(crate) fn server_playlist_len(&mut self, id: i64) -> usize {
        self.library().and_then(|lib| lib.playlist_entries(id).ok()).map_or(0, |entries| entries.len())
    }

    /// Adds library tracks to a server's playlist, at its end: each must be
    /// one of that server's. Without `repeats`, what it already holds is
    /// left out, as in one of ours. `None` when it is not a server's.
    pub(crate) fn add_to_server_playlist(&mut self, id: i64, track_ids: &[i64], repeats: bool) -> Option<PlaylistAddResult> {
        let stored = self.server_playlist_of(id)?;
        let rejected = Some(PlaylistAddResult { rejected: true, ..PlaylistAddResult::default() });
        let Some(entries) = self.mixed_playlist_entries(track_ids, false) else { return rejected };
        if !stored.writable || entries.len() != track_ids.len() || entries.iter().any(|e| e.source_id != Some(stored.source_id)) {
            return rejected;
        }
        let Some(lib) = self.library() else { return rejected };
        let membership = lib.playlist_membership(id, &entries).unwrap_or_default();
        let mut held: Vec<String> = lib.playlist_entries(id).unwrap_or_default().into_iter().map(|e| e.external_id).collect();
        let mut tracks = Vec::new();
        for entry in entries {
            if repeats || !held.contains(&entry.external_id) {
                held.push(entry.external_id.clone());
                tracks.push(entry.external_id);
            }
        }
        let added = tracks.len();
        if added > 0 {
            if let Err(why) = self.ask_server_change(Write { source: stored.source_id, playlist: id, request: String::new(), change: Change::Add { tracks } }) {
                self.now.error = why;
                return rejected;
            }
        }
        Some(PlaylistAddResult { added, exact: membership.exact, similar: membership.similar, rejected: false })
    }

    /// Creates a playlist on the server whose source key `source` is, with
    /// those library tracks, each one of that server's. The window waits on
    /// `request` (`server_write_state`). `None` when `source` is no server.
    pub(crate) fn create_server_playlist(&mut self, source: &str, name: &str, track_ids: &[i64], request: &str) -> Option<bool> {
        let origin = self.server_origins().into_iter().find(|origin| origin.key == source)?;
        let name = name.trim();
        let Some(entries) = self.mixed_playlist_entries(track_ids, false) else { return Some(false) };
        if name.is_empty() || entries.len() != track_ids.len() || entries.iter().any(|e| e.source_id != Some(origin.source_id)) {
            return Some(false);
        }
        let tracks = entries.into_iter().map(|e| e.external_id).collect();
        let write = Write { source: origin.source_id, playlist: 0, request: request.to_string(), change: Change::Create { name: name.to_string(), tracks } };
        Some(match self.ask_server_change(write) {
            Ok(()) => true,
            Err(why) => {
                self.now.error = why;
                false
            }
        })
    }

    /// How the last change the window waits on went at the server whose
    /// source key `source` is, as a source's write state reads. `None`
    /// when `source` is no server.
    pub(crate) fn server_write_state(&mut self, source: &str) -> Option<String> {
        let origin = self.server_origins().into_iter().find(|origin| origin.key == source)?;
        Some(self.server_change_answer(origin.source_id).cloned().unwrap_or(serde_json::Value::Null).to_string())
    }

    /// The servers a new playlist can be made on, as the picker's source
    /// chooser lists them: those that are on, and that take every one of the
    /// tracks, all of them theirs.
    pub(crate) fn server_playlist_sources(&mut self, track_ids: &[i64]) -> Vec<serde_json::Value> {
        let entries = self.mixed_playlist_entries(track_ids, false).unwrap_or_default();
        let complete = entries.len() == track_ids.len();
        let offline = self.offline();
        let reachable: HashSet<i64> = self.library().and_then(|lib| lib.servers().ok()).unwrap_or_default().into_iter().filter(|s| s.reachable).map(|s| s.source_id).collect();
        self.server_origins()
            .into_iter()
            .filter(|origin| origin.enabled)
            .map(|origin| {
                let theirs = complete && entries.iter().all(|e| e.source_id == Some(origin.source_id));
                serde_json::json!({"id": origin.key, "name": origin.name, "accepts": theirs && !offline && reachable.contains(&origin.source_id)})
            })
            .collect()
    }
}

/// The rows of a playlist of `len` after moving one from `from` to `to`.
pub(crate) fn moved(len: usize, from: usize, to: usize) -> Option<Vec<usize>> {
    if from >= len || to >= len || from == to {
        return None;
    }
    let mut rows: Vec<usize> = (0..len).collect();
    let row = rows.remove(from);
    rows.insert(to, row);
    Some(rows)
}

/// The rows of a playlist of `len` after a group of them is dropped at
/// `to`, as one block (`group_moves`).
pub(crate) fn moved_group(len: usize, group: &[usize], to: usize) -> Option<Vec<usize>> {
    if to >= len || group.iter().any(|&row| row >= len) {
        return None;
    }
    let mut rows: Vec<usize> = (0..len).collect();
    for (from, to) in crate::playlists::group_moves(group, to) {
        let row = rows.remove(from);
        rows.insert(to, row);
    }
    (rows.iter().enumerate().any(|(at, &row)| at != row)).then_some(rows)
}

#[cfg(test)]
mod tests;
