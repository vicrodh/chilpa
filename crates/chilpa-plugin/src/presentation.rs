//! Optional presentation metadata layered over the shared browse protocol.
//! Entry identities and operations remain opaque; no provider is recognized.

use crate::messages::Listing;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Entity { Track, Album, Artist, Playlist, Search, #[default] Folder, Notice }

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Action {
    pub key: String,
    pub enabled: bool,
    #[serde(default)] pub reason: String,
}

/// Technical hints supplied by the catalog. These never establish decoded
/// stream quality, device output format, or bit-perfect playback.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AdvertisedQuality {
    pub format: String,
    pub sample_rate_hz: Option<u32>,
    pub bit_depth: Option<u16>,
    pub channels: Option<u16>,
    pub bitrate_label: String,
}

impl AdvertisedQuality {
    pub fn label(&self) -> String {
        let mut parts = Vec::new();
        if !self.format.is_empty() { parts.push(self.format.clone()); }
        if let Some(depth) = self.bit_depth.filter(|value| *value > 0) { parts.push(format!("{depth} bit")); }
        if let Some(rate) = self.sample_rate_hz.filter(|value| *value > 0) { parts.push(format!("{} kHz", f64::from(rate) / 1000.0)); }
        if !self.bitrate_label.is_empty() { parts.push(self.bitrate_label.clone()); }
        parts.join(" · ")
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Presentation {
    pub entity: Entity,
    pub title: String,
    pub secondary: String,
    pub stable_id: String,
    pub sort_key: String,
    pub year: u32,
    pub track_number: u32,
    pub disc_number: u32,
    pub disc_count: u32,
    pub advertised_quality: AdvertisedQuality,
    pub metadata: BTreeMap<String, String>,
    pub actions: Vec<Action>,
    pub can_open: bool,
    pub can_preview: bool,
    pub play_all: bool,
}

impl Presentation {
    pub fn clean(&mut self) -> Result<(), &'static str> {
        let quality = &self.advertised_quality;
        if quality.format.len() > 128 || quality.bitrate_label.len() > 128
            || quality.sample_rate_hz.is_some_and(|rate| rate == 0 || rate > 100_000_000)
            || quality.bit_depth.is_some_and(|depth| depth == 0 || depth > 64)
            || quality.channels.is_some_and(|channels| channels == 0 || channels > 256) {
            return Err("Invalid advertised audio quality.");
        }
        if self.metadata.len() > 32 || self.actions.len() > 32
            || [&self.title, &self.secondary, &self.stable_id, &self.sort_key].into_iter()
                .chain(self.metadata.keys()).chain(self.metadata.values()).any(|text| text.len() > 32768)
            || self.actions.iter().any(|action| action.key.len() > 256 || action.reason.len() > 8192) {
            return Err("The presentation metadata is too large.");
        }
        // Command execution needs its own capability and adapter mapping.
        // Merely naming an action never enables an arbitrary guest command.
        for action in &mut self.actions { action.enabled = false; }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SourceDocument {
    #[serde(flatten)] pub listing: Listing,
    #[serde(default)] pub medium: String,
    #[serde(default)] pub attribution: String,
    #[serde(default)] pub facets: Vec<crate::messages::Column>,
    #[serde(default)] pub presentation: BTreeMap<String, Presentation>,
}

impl SourceDocument {
    pub fn clean(mut self, cursor: Option<&str>) -> Result<Self, &'static str> {
        if !["", "radio", "podcasts"].contains(&self.medium.as_str()) || self.attribution.len() > 512 { return Err("Invalid source medium."); }
        if self.presentation.len() > 1000 { return Err("Too many presentation entries."); }
        if self.facets.len() > 12 { return Err("Too many source facets."); }
        for facet in &mut self.facets {
            facet.entries.retain(|entry| entry.usable());
            facet.entries.truncate(1000);
            if !facet.usable() { return Err("Invalid source facet."); }
        }
        self.listing = self.listing.clean(cursor);
        let ids: BTreeSet<_> = self.listing.entries.iter().map(|entry| entry.id.as_str()).collect();
        self.presentation.retain(|id, _| ids.contains(id.as_str()));
        for value in self.presentation.values_mut() {
            value.clean()?;
        }
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn catalog_quality_is_optional_bounded_and_keeps_exact_sample_rates() {
        let mut presentation: Presentation = serde_json::from_value(json!({"entity":"album",
            "advertised_quality":{"format":"FLAC","bit_depth":24,"sample_rate_hz":88200}})).unwrap();
        presentation.clean().unwrap();
        assert_eq!(presentation.advertised_quality.label(), "FLAC · 24 bit · 88.2 kHz");
        assert!(Presentation::default().advertised_quality.label().is_empty());
        presentation.advertised_quality.bit_depth = Some(0);
        assert!(presentation.clean().is_err());
    }

    #[test]
    fn typed_metadata_survives_cleaning_but_orphans_and_unapproved_actions_do_not() {
        let document: SourceDocument = serde_json::from_value(json!({"title":"Collection","entries":[
            {"id":"album-one","title":"Record","kind":"folder"}],"presentation":{
            "album-one":{"entity":"album","metadata":{"artist":"Artist"},"year":2020,
                "actions":[{"key":"remove","enabled":true}]},
            "not-listed":{"entity":"artist"}}})).unwrap();
        let clean = document.clean(None).unwrap();
        assert_eq!(clean.presentation.len(), 1);
        let record = &clean.presentation["album-one"];
        assert_eq!(record.entity, Entity::Album);
        assert_eq!(record.metadata["artist"], "Artist");
        assert_eq!(record.year, 2020);
        assert!(!record.actions[0].enabled);
        assert_eq!(serde_json::to_value(&clean).unwrap()["presentation"]["album-one"]["entity"], "album");
    }

    #[test]
    fn existing_listings_need_no_presentation_extension() {
        let document: SourceDocument = serde_json::from_value(json!({"entries":[],"title":"Legacy"})).unwrap();
        assert!(document.clean(None).unwrap().presentation.is_empty());
    }
}
