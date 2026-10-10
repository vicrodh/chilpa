//! Per-installation presentation preferences, kept outside replaceable packages.
use crate::Chilpa;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Preferences {
    pub separate_playlists: bool,
    pub integrate_albums: bool,
    pub integrate_tracks: bool,
    pub integrate_home: bool,
}

pub(crate) fn read(root: &Path, id: &str) -> Preferences {
    if id.is_empty() || id.contains("..") || !id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'-')) { return Preferences::default(); }
    std::fs::read(root.join(".integration").join(format!("{id}.json"))).ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
}

pub(crate) fn save(root: &Path, id: &str, field: &str, enabled: bool) -> Result<(), String> {
    if id.is_empty() || id.contains("..") || !id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'-'))
        || !root.join(id).join("manifest.json").is_file() { return Err("The plugin is not installed.".into()); }
    let mut preferences = read(root, id);
    match field {
        "separate_playlists" => preferences.separate_playlists = enabled,
        "integrate_albums" => preferences.integrate_albums = enabled,
        "integrate_tracks" => preferences.integrate_tracks = enabled,
        "integrate_home" => preferences.integrate_home = enabled,
        _ => return Err("Unknown integration preference.".into()),
    }
    let directory = root.join(".integration");
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let mut temporary = tempfile::NamedTempFile::new_in(&directory).map_err(|e| e.to_string())?;
    use std::io::Write;
    temporary.write_all(&serde_json::to_vec(&preferences).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    temporary.as_file().sync_all().map_err(|e| e.to_string())?;
    temporary.persist(directory.join(format!("{id}.json"))).map_err(|e| e.to_string())?;
    Ok(())
}

impl Chilpa {
    pub fn request_track_quality(&self, id: i64, path: &str) {
        if id < 0 { self.catalog.want_quality(id); }
        else if crate::source_opener::of_plugin(path).is_some() {
            if let Some((track, _)) = self.catalog.track_by_path(path) { self.catalog.want_quality(track.id); }
        }
    }
    /// Installed native libraries that can contribute saved releases to Albums.
    pub fn album_libraries(&self) -> String { self.integrated_libraries("albums") }
    pub fn track_libraries(&self) -> String { self.integrated_libraries("tracks") }
    pub fn home_libraries(&self) -> String {
        let root = Self::plugins_dir();
        let mut rows: Vec<_> = self.natives.ids().into_iter()
            .filter(|id| self.natives.has_pages(id) && read(&root, id).integrate_home)
            .map(|id| json!({"id":id,"name":self.catalog.name(&id).unwrap_or_else(|| id.clone())})).collect();
        rows.sort_by(|a,b| a["name"].as_str().cmp(&b["name"].as_str()));
        json!(rows).to_string()
    }
    fn integrated_libraries(&self, kind: &str) -> String {
        let root = Self::plugins_dir();
        let mut rows: Vec<_> = self.natives.ids().into_iter()
            .filter(|id| self.natives.library_kinds(id).iter().any(|k| k == kind))
            .map(|id| json!({"name":self.catalog.name(&id).unwrap_or_else(|| id.clone()),
                "enabled":if kind == "tracks" { read(&root, &id).integrate_tracks } else { read(&root, &id).integrate_albums },"icon":crate::plugin_icons::image_url(&root, &id),"id":id})).collect();
        rows.sort_by(|a,b| a["name"].as_str().cmp(&b["name"].as_str()));
        json!(rows).to_string()
    }

    pub(crate) fn show_album_library(&mut self, id: &str, enabled: bool) {
        if !self.natives.library_kinds(id).iter().any(|kind| kind == "albums") { return; }
        if let Err(error) = save(&Self::plugins_dir(), id, "integrate_albums", enabled) { self.now.error = error; return; }
        if enabled { self.catalog.retry(&format!("{id} section:albums")); }
        self.mark_library_changed();
    }
    pub(crate) fn show_track_library(&mut self, id: &str, enabled: bool) {
        if !self.natives.library_kinds(id).iter().any(|kind| kind == "tracks") { return; }
        if let Err(error) = save(&Self::plugins_dir(), id, "integrate_tracks", enabled) { self.now.error = error; return; }
        if enabled { self.catalog.retry(&format!("{id} section:tracks")); }
        self.mark_library_changed();
    }
    pub fn prepare_track_libraries(&self) {
        for id in self.natives.ids() {
            if read(&Self::plugins_dir(), &id).integrate_tracks && self.natives.library_kinds(&id).iter().any(|k| k == "tracks") {
                self.catalog.shelf(&format!("{id} section:tracks"));
            }
        }
    }
    pub(crate) fn forget_source_preferences(&mut self, id: &str) {
        let mut settings = chilpa_core::settings::load();
        let prefix = format!("ui.source.{id}");
        settings.window.retain(|key, _| !key.starts_with(&format!("{prefix} ")) && !key.starts_with(&format!("{prefix}.")));
        let _ = chilpa_core::settings::save(&settings);
    }

    /// Opted-in favorite albums share the global grid without importing track lists.
    pub fn integrated_albums(&mut self, text: &str, filter: &crate::Filter) -> Vec<crate::Album> {
        let root = Self::plugins_dir();
        let mut albums = Vec::new();
        for id in self.natives.ids() {
            if self.natives.library_kinds(&id).iter().any(|kind| kind == "albums") && read(&root, &id).integrate_albums {
                if filter.excluded_sources.contains(&format!("{}:{}", crate::source_opener::PROVIDER, id)) { continue; }
                albums.extend(self.source_shelf(&format!("{id} section:albums"), "", "", text).0.into_iter().filter(|a| {
                    let quality = self.catalog.card(a.id).and_then(|c| c.format).map(|f| f.quality()).unwrap_or_default();
                    let tier = match quality.tier { crate::quality::Tier::Lossy => "lossy", crate::quality::Tier::Lossless => "lossless", crate::quality::Tier::HiRes => "hires", crate::quality::Tier::Dsd => "dsd", _ => "unknown" };
                    (filter.qualities.is_empty() || filter.qualities.iter().any(|q| q == tier)) && (filter.formats.is_empty() || filter.formats.iter().any(|f| f.eq_ignore_ascii_case(&quality.format)))
                }));
            }
        }
        albums
    }
    /// Sidebar lists include native cached library cards without opening their tracks.
    pub fn sidebar_playlists(&mut self, query: &str) -> String {
        self.source_panels();
        let root = Self::plugins_dir();
        let mut playlists: Vec<Value> = self.playlists().into_iter().map(|p| {
            let mut row = serde_json::to_value(&p).unwrap_or_default();
            if let Some(card) = self.catalog.card(p.id) {
                row["contain_art"] = card.contain_art().into();
                row["native_ref"] = card.identity.into();
                row["native_folder"] = card.metadata["native_folder"].clone();
            }
            row["source_icon"] = crate::plugin_icons::image_url(&root, &p.source).into();
            row["separate"] = read(&root, &p.source).separate_playlists.into();
            row
        }).collect();
        let words: Vec<_> = query.to_lowercase().split_whitespace().map(str::to_owned).collect();
        playlists.retain(|p| p["visible_sidebar"] == true && words.iter().all(|word| p["name"].as_str().unwrap_or_default().to_lowercase().contains(word)));
        let mut native_folders = Vec::new(); let mut folder_sources = Vec::new();
        for source in self.natives.ids() {
            if let Some(shelf) = self.catalog.stored_shelf(&format!("{source} section:playlists")) {
                if let Some(folders) = shelf.document["folders"].as_array() {
                    let name = self.catalog.name(&source).unwrap_or_else(|| source.clone());
                    let icon = crate::plugin_icons::image_url(&root, &source);
                    let folder_icon = crate::plugin_icons::folder_image_url(&root,&source);
                    folder_sources.push(json!({"id":source,"name":name,"icon":icon}));
                    native_folders.extend(folders.iter().map(|folder| { let mut folder = folder.clone(); folder["source"] = source.clone().into(); folder["source_name"] = name.clone().into(); folder["source_icon"] = icon.clone().into(); folder["folder_icon"] = folder_icon.clone().into(); folder }));
                }
            }
        }
        json!({"playlists":playlists,"folders":self.playlist_folders(),"native_folders":native_folders,"folder_sources":folder_sources}).to_string()
    }
}
