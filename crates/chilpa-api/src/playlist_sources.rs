//! Shared playlist identities across the local database and native catalogs.
use crate::{Chilpa, Playlist};
use serde_json::{json, Value};

pub(crate) fn execute(natives: &std::sync::Arc<crate::natives::Natives>, source: &str, mut input: Value) -> Result<Value, String> {
    let tracks = input.as_object_mut().and_then(|v| v.remove("initial_tracks"));
    let creating = input["action"] == "playlist.create";
    let mut result = natives.call(source, "page.action", input, std::time::Duration::from_secs(30))?;
    if creating {
        if let Some(tracks) = tracks.as_ref().and_then(Value::as_array).filter(|t| !t.is_empty()) {
            let reference = result["item"]["ref"].as_str().or_else(|| result["ref"].as_str()).map(str::to_owned);
            let mut completed = 0;
            if let Some(reference) = reference {
                for chunk in tracks.chunks(100) {
                    let request = json!({"action":"playlist.add","ref":reference,"tracks":chunk});
                    if natives.call(source, "page.action", request, std::time::Duration::from_secs(30)).is_err() { break; }
                    completed += chunk.len();
                }
            }
            if completed != tracks.len() {
                result["warning"] = json!(chilpa_i18n::t("The playlist was created, but some tracks could not be added. Open it before retrying."));
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_refresh_waits_for_a_busy_host_without_blocking_the_window() {
        let directory = tempfile::tempdir().unwrap();
        let host = std::sync::Arc::new(std::sync::Mutex::new(crate::plugins::Plugins::new(directory.path().into()).unwrap()));
        let source = "app.example.music";
        assert!(host.lock().unwrap().playlists_due(source));
        let mut api = Chilpa::new();
        api.plugins = Some(host.clone());
        api.library_changed();
        let held = host.lock().unwrap();
        let started = std::time::Instant::now();
        api.refresh_playlist_sources();
        assert!(started.elapsed() < std::time::Duration::from_millis(250));
        assert!(api.sources.playlist_refresh_pending);
        assert!(!api.library_changed());
        drop(held);
        api.advance_sources();
        assert!(!api.sources.playlist_refresh_pending);
        assert!(api.library_changed());
        assert!(host.lock().unwrap().playlists_due(source));
        assert!(!host.lock().unwrap().playlists_due(source));
    }
}

impl Chilpa {
    pub(crate) fn native_playlist_rows(&mut self) -> Vec<Playlist> {
        for plugin in self.natives.ids() {
            if self.natives.library_kinds(&plugin).iter().any(|kind| kind == "playlists") {
                self.catalog.shelf(&format!("{plugin} section:playlists"));
            }
        }
        let cards = self.catalog.saved_playlist_summaries();
        let Some(lib) = self.library() else { return Vec::new() };
        let mut stored = lib.playlists().unwrap_or_default();
        let mut inserted = false;
        for card in &cards {
            if stored.iter().any(|p| p.provider == crate::source_opener::PROVIDER && p.instance == card.plugin && p.external_id == card.identity) { continue; }
            if let Ok(source) = lib.source_id(crate::source_opener::PROVIDER, &card.plugin) {
                inserted |= lib.sync_source_playlist(source, &card.identity, &card.title, "", Some(&card.art)).is_ok();
            }
        }
        if inserted { stored = lib.playlists().unwrap_or_default(); }
        let mut rows = Vec::new();
        for card in cards {
            let plugin = card.plugin.clone();
            let name = self.catalog.name(&plugin).unwrap_or_else(|| plugin.clone());
            let writable = self.natives.edits_playlists(&plugin) && card.editable;
            let Some(stored) = stored.iter().find(|p| p.provider == crate::source_opener::PROVIDER && p.instance == plugin && p.external_id == card.identity).cloned() else { continue };
            let mut row = crate::playlists::playlist_of(stored);
            row.id = card.id;
            row.source = plugin;
            row.provider = name;
            row.name = card.title;
            row.description = card.description;
            row.artwork_path = card.art;
            row.track_count = card.track_count;
            row.updated_at = card.updated_at.unwrap_or(row.updated_at);
            row.writable = writable;
            rows.push(row);
        }
        rows
    }

    pub(crate) fn playlist_storage_id(&mut self, id: i64) -> Option<i64> {
        if id > 0 { return Some(id); }
        let row = self.native_playlist_rows().into_iter().find(|row| row.id == id)?;
        self.library()?.playlists().ok()?.into_iter()
            .find(|p| p.provider == crate::source_opener::PROVIDER && p.instance == row.source && p.external_id == row.external_id).map(|p| p.id)
    }

    pub(crate) fn mixed_playlist_entries(&mut self, ids: &[i64], persist: bool) -> Option<Vec<chilpa_library::PlaylistEntry>> {
        let mut entries = Vec::new();
        for id in ids {
            if *id >= 0 {
                let found = self.library()?.playlist_entries_for_tracks(&[*id]).ok()?;
                entries.push(found.into_iter().next()?);
            } else {
                let (track, art) = self.catalog.tracks_by_ids(&[*id]).into_iter().next()?;
                let lib = self.library()?;
                if !persist {
                    let known = lib.track_by_entry(&track.path).ok().flatten();
                    entries.push(chilpa_library::PlaylistEntry {
                        track_id: known.map(|(id, _)| id),
                        source_id: known.map(|(_, source)| source), external_id: track.path,
                    });
                    continue;
                }
                let source = lib.source_id(crate::source_opener::PROVIDER, &track.plugin).ok()?;
                let metadata = chilpa_library::PlaylistTrackMetadata {
                    external_id: track.path, title: track.title, artist: track.artist, album: track.album,
                    codec: track.format.as_ref().map(|f| f.codec.clone()),
                    sample_rate_hz: track.format.as_ref().and_then(|f| f.rate),
                    bit_depth: track.format.as_ref().and_then(|f| f.bits).map(u16::from),
                    duration_ms: track.duration_ms, artwork_path: (!art.is_empty()).then_some(art),
                    ..Default::default()
                };
                entries.push(lib.cache_playlist_tracks(source, &[metadata]).ok()?.into_iter().next()?);
            }
        }
        Some(entries)
    }

    fn playlist_refs(&mut self, ids: &[i64], plugin: &str) -> Option<Vec<String>> {
        let entries = self.mixed_playlist_entries(ids, false)?;
        entries.into_iter().map(|entry| {
            let (source, reference) = crate::source_opener::of_plugin(&entry.external_id)?;
            (source == plugin).then(|| reference.split('@').next().unwrap_or_default().to_string())
        }).collect()
    }

    pub fn playlist_sources_json(&mut self, ids: &[i64]) -> String {
        let mut rows = Vec::new();
        for plugin in self.natives.ids() {
            if !self.natives.edits_playlists(&plugin) { continue; }
            rows.push(json!({"id":plugin,"name":self.catalog.name(&plugin).unwrap_or_else(|| plugin.clone()),
                "accepts":self.playlist_refs(ids, &plugin).is_some()}));
        }
        rows.sort_by(|a,b| a["name"].as_str().cmp(&b["name"].as_str()));
        json!(rows).to_string()
    }

    /// Dispatches a source mutation on the existing asynchronous action worker.
    pub fn write_source_playlist(&mut self, plugin: &str, id: i64, title: &str, description: &str, ids: &[i64], request: &str) -> bool {
        if !self.natives.edits_playlists(plugin) { return false; }
        let Some(tracks) = self.playlist_refs(ids, plugin) else { return false };
        let input = if id == 0 {
            if title.trim().is_empty() { return false; }
            json!({"action":"playlist.create","title":title,"description":description,"public":false,"initial_tracks":tracks,"request_id":request})
        } else {
            let Some(card) = self.catalog.card(id).filter(|c| c.plugin == plugin && c.metadata["editable"] == true) else { return false };
            if tracks.is_empty() || tracks.len() > 100 { return false; }
            json!({"action":"playlist.add","ref":card.identity,"tracks":tracks,"request_id":request})
        };
        if self.sources.native_actions.get(plugin).is_some_and(|s| s["pending"] == true) { return false; }
        self.sources_command("native", plugin, &input.to_string());
        true
    }

    pub fn refresh_playlist_sources(&mut self) {
        self.sources.playlist_refresh_pending = true;
        self.advance_playlist_refresh();
        for id in self.natives.ids() {
            if self.natives.library_kinds(&id).iter().any(|kind| kind == "playlists") {
                self.catalog.retry(&format!("{id} section:playlists"));
            }
        }
    }

    /// I keep an explicit refresh pending while a compatibility call owns the host.
    pub(crate) fn advance_playlist_refresh(&mut self) {
        if !self.sources.playlist_refresh_pending { return; }
        let refreshed = self.plugins.as_ref().and_then(|host| host.try_lock().ok()).map(|mut host| {
            host.refresh_playlist_reads();
        }).is_some();
        if refreshed || (self.plugins.is_none() && !self.plugins_starting()) {
            self.sources.playlist_refresh_pending = false;
            self.mark_library_changed();
        }
    }

    pub fn playlist_write_state(&mut self, plugin: &str) -> String {
        self.advance_plugins();
        self.advance_sources();
        self.sources.native_actions.get(plugin).unwrap_or(&Value::Null).to_string()
    }

    pub fn order_playlist_folders(&mut self, ids: &[i64]) -> bool {
        self.library().is_some_and(|lib| lib.set_folder_order(ids).is_ok())
    }
}
