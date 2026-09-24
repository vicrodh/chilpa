//! The lists: saved, smart and by genre.
//!
//! A saved list has tracks; a smart one has rules and is resolved every time
//! it is looked at. Both come out the same door.

use super::*;

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
    pub fn playlists(&mut self) -> Vec<Playlist> {
        let Some(lib) = self.library() else { return Vec::new() };
        lib.playlists()
            .unwrap_or_default()
            .into_iter()
            .map(|p| Playlist {
                id: p.id,
                name: p.name,
                provider: p.provider,
                track_count: p.track_count,
            })
            .collect()
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
