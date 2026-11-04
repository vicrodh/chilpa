//! Edition identifiers for artwork lookup, read on the existing query worker.
use crate::{Filter, Reader, TrackOrder};
use serde_json::{json, Value};

impl Reader {
    /// Reads one file header and its sidecar, never decodes or opens an output.
    pub fn album_artwork_hint(&self, album: i64) -> String {
        if album < 0 {
            let value = self.catalog.as_ref().and_then(|catalog| catalog.card(album)).map(|card| card.metadata).unwrap_or(Value::Null);
            return json!({"release_id":value["musicbrainz_release_id"].as_str().unwrap_or("")}).to_string();
        }
        let track = self.lib.browse_tracks(&Filter { album:Some(album), ..Default::default() }, TrackOrder::Disc, 1).ok().and_then(|rows| rows.into_iter().next());
        let Some(track) = track else { return "{}".into() };
        let path = std::path::Path::new(&track.external_id);
        if !track.available || !path.is_absolute() || !path.is_file() { return "{}".into(); }
        let mut release = chilpa_tags::edit::snapshot(path).ok().and_then(|tags| tags.musicbrainz_release_id).unwrap_or_default();
        let home = chilpa_library::sidecar::Home::of(&track.external_id, None);
        if let Some(sidecar) = chilpa_library::sidecar::load(&home, &track.external_id).ok().flatten() {
            use chilpa_library::sidecar::Override;
            match sidecar.album.musicbrainz_release_id { Override::Absent => {}, Override::Cleared => release.clear(), Override::Value(value) => release = value }
        }
        json!({"release_id":release}).to_string()
    }
}
