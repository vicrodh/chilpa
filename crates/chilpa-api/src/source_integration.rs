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
        for panel in self.source_panels() {
            if self.natives.is_native(&panel.id) && read(&root, &panel.id).integrate_albums {
                if filter.excluded_sources.contains(&format!("{}:{}", crate::source_opener::PROVIDER, panel.id)) { continue; }
                albums.extend(self.source_shelf(&format!("{} section:albums", panel.id), "", "", text).0.into_iter().filter(|a| {
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
        let panels = self.source_panels();
        let root = Self::plugins_dir();
        let mut playlists: Vec<Value> = self.playlists().into_iter().map(|p| {
            let mut row = serde_json::to_value(&p).unwrap_or_default();
            row["source_icon"] = crate::plugin_icons::image_url(&root, &p.source).into();
            row["separate"] = read(&root, &p.source).separate_playlists.into();
            row
        }).collect();
        for panel in panels.into_iter().filter(|p| self.natives.is_native(&p.id)) {
            let Some(shelf) = self.catalog.shelf(&format!("{} section:playlists", panel.id)) else { continue };
            let preferences = read(&root, &panel.id);
            for card in self.catalog.cards(&shelf.cards) {
                playlists.push(json!({"id":card.id,"name":card.title,"track_count":card.tracks.as_ref().map_or_else(|| card.metadata["tracks"].as_u64().unwrap_or_default() as usize, Vec::len),
                    "source":panel.id,"provider":panel.name,"mixed":false,"folder_id":null,"visible_sidebar":true,
                    "artwork_path":card.art,"covers":[],"source_icon":crate::plugin_icons::image_url(&root, &panel.id),
                    "separate":preferences.separate_playlists}));
            }
        }
        let words: Vec<_> = query.to_lowercase().split_whitespace().map(str::to_owned).collect();
        playlists.retain(|p| p["visible_sidebar"] == true && words.iter().all(|word| p["name"].as_str().unwrap_or_default().to_lowercase().contains(word)));
        json!({"playlists":playlists,"folders":self.playlist_folders()}).to_string()
    }
}
