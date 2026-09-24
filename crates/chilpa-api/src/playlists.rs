//! The lists: saved, smart and by genre.
//!
//! A saved list has tracks; a smart one has rules and is resolved every time
//! it is looked at. Both come out the same door.

use super::*;
use chilpa_plugin::messages;

impl Chilpa {
    /// Starts a genre playing, shuffled.
    pub fn play_genre(&mut self, genre: &str, device: &str, bit_perfect: bool) -> Vec<String> {
        let list = chilpa_library::SmartList::new(
            genre,
            vec![chilpa_library::Rule::GenreIs(genre.to_string())],
            chilpa_library::SmartOrder::Random,
            300,
        );
        let Some(lib) = self.library() else { return Vec::new() };
        let paths: Vec<String> = lib
            .smart_tracks(&list)
            .unwrap_or_default()
            .into_iter()
            .map(|t| t.external_id)
            .collect();
        if paths.is_empty() {
            self.now.error = chilpa_i18n::t_args("there is nothing under {}", &[genre]);
            return Vec::new();
        }
        self.play_tracks(&paths, device, bit_perfect)
    }

    /// Starts a smart list playing, by its name.
    pub fn play_smart_list(
        &mut self,
        name: &str,
        device: &str,
        bit_perfect: bool,
    ) -> Vec<String> {
        let Some(list) = chilpa_library::smart_builtins().into_iter().find(|l| l.name == name)
        else {
            self.now.error =
                chilpa_i18n::t_args("I do not know the list \"{}\"", &[&chilpa_i18n::t(name)]);
            return Vec::new();
        };
        let Some(lib) = self.library() else { return Vec::new() };
        let paths: Vec<String> = lib
            .smart_tracks(&list)
            .unwrap_or_default()
            .into_iter()
            .map(|t| t.external_id)
            .collect();
        if paths.is_empty() {
            self.now.error =
                chilpa_i18n::t_args("\"{}\" brings nothing today", &[&chilpa_i18n::t(name)]);
            return Vec::new();
        }
        self.play_tracks(&paths, device, bit_perfect)
    }

    /// The saved lists, the ones from here and whatever a source brings.
    ///
    /// A source's lists are asked for here and not kept: they live on the
    /// source, and a copy in the library would be a second truth that goes
    /// stale without anyone noticing. A source that does not answer is left
    /// out and says nothing: its lists missing is not worth breaking the
    /// panel over.
    pub fn playlists(&mut self) -> Vec<Playlist> {
        let mut all: Vec<Playlist> = match self.library() {
            Some(lib) => lib
                .playlists()
                .unwrap_or_default()
                .into_iter()
                .map(|p| Playlist {
                    id: p.id,
                    name: p.name,
                    provider: p.provider,
                    track_count: p.track_count,
                    source: String::new(),
                    external_id: String::new(),
                })
                .collect(),
            None => Vec::new(),
        };
        for (id, name) in self.sources() {
            all.extend(self.source_playlists(&id, &name));
        }
        all
    }

    /// What one source says its lists are. Empty when it cannot answer.
    fn source_playlists(&mut self, id: &str, name: &str) -> Vec<Playlist> {
        let request = messages::write(&messages::Query::new("", messages::MAX_PER_REPLY));
        let Ok(bytes) = self.call_plugin_short(id, messages::op::PLAYLISTS, &request) else {
            return Vec::new();
        };
        let Ok(reply) = messages::read::<messages::Results>(&bytes) else { return Vec::new() };
        reply
            .playlists
            .into_iter()
            .map(|p| Playlist {
                id: 0,
                name: p.name,
                provider: name.to_string(),
                track_count: p.track_count,
                source: id.to_string(),
                external_id: p.external_id,
            })
            .collect()
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
        let names: std::collections::HashMap<String, String> =
            self.sources().into_iter().collect();
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
        let plays = self
            .plugins
            .as_ref()
            .and_then(|p| p.lock().ok())
            .is_none_or(|p| p.delivers_audio(source));
        let hits: Vec<search::Hit> = reply
            .tracks
            .into_iter()
            .map(|t| search::Hit::remote(t, source, &name, plays))
            .collect();
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

    /// Saves the queue as a list under that name.
    ///
    /// It returns the list's id. Whatever is not in the library is saved
    /// anyway, by its path: the list holds even with the scan still pending.
    pub fn save_queue_as(&mut self, name: &str) -> Option<i64> {
        let name = name.trim().to_string();
        if name.is_empty() {
            self.now.error = chilpa_i18n::t("a list needs a name");
            return None;
        }
        if self.queue.is_empty() {
            self.now.error = chilpa_i18n::t("there is no queue to save");
            return None;
        }
        let paths = self.queue.clone();
        let lib = self.library()?;
        let source = lib.source_id("local", "default").ok()?;
        let entries: Vec<chilpa_library::PlaylistEntry> = paths
            .iter()
            .map(|r| chilpa_library::PlaylistEntry {
                track_id: lib
                    .track_by_path(source, std::path::Path::new(r))
                    .ok()
                    .flatten(),
                external_id: r.clone(),
            })
            .collect();
        let id = lib.create_playlist(source, &name, "").ok()?;
        lib.set_playlist_entries(id, &entries).ok()?;
        Some(id)
    }

    /// Starts a saved list playing.
    pub fn play_saved_playlist(
        &mut self,
        playlist_id: i64,
        device: &str,
        bit_perfect: bool,
    ) -> Vec<String> {
        let Some(lib) = self.library() else { return Vec::new() };
        let paths: Vec<String> = lib
            .playlist_entries(playlist_id)
            .unwrap_or_default()
            .into_iter()
            .map(|e| e.external_id)
            .filter(|r| !r.is_empty())
            .collect();
        if paths.is_empty() {
            self.now.error = chilpa_i18n::t("that list is empty");
            return Vec::new();
        }
        self.play_tracks(&paths, device, bit_perfect)
    }

    /// Deletes a list. The tracks are not touched.
    pub fn delete_playlist(&mut self, playlist_id: i64) {
        if let Some(lib) = self.library() {
            let _ = lib.delete_playlist(playlist_id);
        }
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
}
