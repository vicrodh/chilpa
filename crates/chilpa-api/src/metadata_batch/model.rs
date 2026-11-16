use super::fields::{self, Patch};
use chilpa_library::metadata_manifest::Entry;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::path::Path;

pub(super) const MAX_ROW_BYTES: usize = 256 * 1024;
pub(super) const MAX_PAGE_BYTES: usize = 1024 * 1024;

#[derive(Clone, Default, Deserialize, Serialize, PartialEq)]
pub(super) struct Stamp {
    length: u64,
    modified: u128,
    identity: Option<(u64, u64)>,
}
impl Stamp {
    pub fn read(path: &Path) -> Result<Self, String> {
        let metadata = std::fs::metadata(path).map_err(|error| error.to_string())?;
        let modified = metadata
            .modified()
            .map_err(|error| error.to_string())?
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        #[cfg(unix)]
        let identity = {
            use std::os::unix::fs::MetadataExt;
            Some((metadata.dev(), metadata.ino()))
        };
        #[cfg(not(unix))]
        let identity = None;
        Ok(Self {
            length: metadata.len(),
            modified,
            identity,
        })
    }
}

#[derive(Default, Deserialize, Serialize)]
pub(super) struct Row {
    pub path: String,
    pub library_path: String,
    pub track_id: Option<i64>,
    pub original: Map<String, Value>,
    pub effective: Map<String, Value>,
    pub patches: BTreeMap<String, Patch>,
    pub stamp: Stamp,
    pub placed: Vec<String>,
    pub selected: bool,
    pub status: String,
    pub phase: String,
    pub error: String,
    pub file_written: bool,
    pub overlay_written: bool,
    pub overlay_evidence: Option<String>,
    pub group: String,
    pub hardlinked: Option<bool>,
    pub id3_v23: bool,
    pub duration_ms: u64,
    pub membership: Option<Value>,
}

impl Row {
    pub fn read(entry: &Entry) -> Result<Self, String> {
        serde_json::from_str(&entry.body).map_err(|error| error.to_string())
    }

    pub fn changes(&self) -> Map<String, Value> {
        self.patches
            .iter()
            .filter_map(|(name, patch)| {
                let before = self.effective.get(name).unwrap_or(&Value::Null);
                let after = patch.result(before);
                (after != *before).then(|| (name.clone(), after))
            })
            .collect()
    }

    pub fn entry(&self, id: i64) -> Result<Entry, String> {
        let body = serde_json::to_string(self).map_err(|error| error.to_string())?;
        if body.len() > MAX_ROW_BYTES {
            return Err(chilpa_i18n::t(
                "This file's metadata exceeds the batch editor limit.",
            ));
        }
        Ok(Entry {
            id,
            path: self.path.clone(),
            artist: self.text("artist_credit"),
            album_artist: self.text("album_artist"),
            album: self.text("album"),
            group: self.group.clone(),
            release: self.text("musicbrainz_release_id"),
            selected: self.selected,
            state: self.status.clone(),
            dirty: self.membership.is_some()
                || !self.changes().is_empty()
                || matches!(self.phase.as_str(), "overlay" | "reconcile"),
            written: self.file_written,
            body,
        })
    }

    pub fn proposed(&self) -> Map<String, Value> {
        let mut values = self.effective.clone();
        values.extend(self.changes());
        values
    }

    pub fn text(&self, name: &str) -> String {
        self.effective
            .get(name)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    }

    pub fn view(&self, session: u64, id: i64) -> Value {
        let mut changes: Vec<_> = self.patches.iter().filter_map(|(field,patch)| {
            let before = self.effective.get(field).unwrap_or(&Value::Null);
            let after = patch.result(before);
            (after != *before).then(|| json!({"field":field,"label":fields::label(field),"before":before,"after":after,"operation":patch.operation}))
        }).collect();
        if let Some(member) = &self.membership {
            changes.push(json!({"field":"presentation","label":chilpa_i18n::t("Album grouping"),"before":self.text("album"),"after":member["title"],"operation":"set"}));
        }
        json!({"key":format!("{session}:{id}"),"path":self.path,"trackId":self.track_id,"title":self.text("title"),
            "artist":self.text("artist_credit"),"album":self.text("album"),"group":format!("{session}:{}",self.group),"selected":self.selected,
            "status":self.status,"statusLabel":self.status_label(),"phase":self.phase,"fileWritten":self.file_written,"sidecarWritten":self.overlay_written,"hardlinked":self.hardlinked,
            "error":self.error,"changes":changes,"values":self.effective})
    }

    fn status_label(&self) -> String {
        chilpa_i18n::t(match self.status.as_str() {
            "changed" => "Pending changes",
            "saved" => "Saved",
            "failed" => "Save incomplete",
            "conflict" => "Changed since preview",
            "unsupported" => "Unavailable",
            _ => "Unchanged",
        })
    }

    pub fn patch(&mut self, patch: &Patch) -> Result<(), String> {
        if self.status == "unsupported" {
            return Ok(());
        }
        if patch.operation != "keep" && !self.placed.contains(&patch.field) {
            return Err(chilpa_i18n::t_args(
                "The selected file cannot store this field: {}",
                &[&self.path],
            ));
        }
        if matches!(self.phase.as_str(), "overlay" | "reconcile") {
            return Err(chilpa_i18n::t(
                "Finish the pending save before changing this file's proposal.",
            ));
        }
        if patch.operation == "keep" {
            self.patches.remove(&patch.field);
        } else {
            self.patches.insert(patch.field.clone(), patch.clone());
        }
        if matches!(
            patch.field.as_str(),
            "album"
                | "album_artist"
                | "artist_credit"
                | "disc_number"
                | "disc_total"
                | "track_number"
                | "track_total"
                | "musicbrainz_release_id"
                | "musicbrainz_release_group_id"
                | "musicbrainz_recording_id"
                | "musicbrainz_track_id"
        ) {
            self.membership = None;
        }
        self.error.clear();
        self.file_written = false;
        self.overlay_written = false;
        self.overlay_evidence = None;
        self.phase = "tags".into();
        self.status = if self.changes().is_empty() {
            "unchanged"
        } else {
            "changed"
        }
        .into();
        Ok(())
    }

    pub fn complete(&mut self) -> Result<(), String> {
        let said = chilpa_tags::edit::snapshot(Path::new(&self.path))
            .map_err(|error| error.to_string())?;
        self.original = fields::values(&said);
        let (sidecar, _) = super::files::overlay(Path::new(&self.path))?;
        let home = chilpa_library::Home::of(&self.path, None);
        self.effective = fields::effective(&self.original, sidecar.as_ref(), &home.key(&self.path));
        self.group = super::files::group_identity(
            Path::new(&self.path),
            &self.effective,
            sidecar.as_ref(),
            &said,
        )?;
        self.stamp = Stamp::read(Path::new(&self.path))?;
        self.membership = None;
        self.patches.clear();
        self.phase = "done".into();
        self.status = "saved".into();
        self.error.clear();
        Ok(())
    }
}
