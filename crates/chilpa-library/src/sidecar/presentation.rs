//! Explicit local album presentation. Release identifiers remain embedded tags.
use super::Sidecar;
use chilpa_tags::edit::Snapshot;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, PartialEq)]
pub struct Presentation {
    pub key: String,
    pub title: String,
    pub artist: String,
    pub evidence: String,
}
impl Presentation {
    pub fn read(value: &Value) -> Option<Self> {
        let text = |field| value.get(field)?.as_str().map(str::to_string);
        let item = Self {
            key: text("key")?,
            title: text("title")?,
            artist: text("artist")?,
            evidence: text("evidence")?,
        };
        (item.key.starts_with("local:")
            && item.key.len() <= 128
            && !item.title.is_empty()
            && item.title.chars().count() <= 512
            && item.artist.chars().count() <= 512
            && item.evidence.len() == 64
            && item.evidence.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then_some(item)
    }
    pub fn value(&self) -> Value {
        json!({"key":self.key,"title":self.title,"artist":self.artist,"evidence":self.evidence})
    }
}

pub fn presentation_evidence(snapshot: &Snapshot) -> String {
    let value = json!([
        "chilpa-presentation-1",
        snapshot.album,
        snapshot.album_artist,
        snapshot.artist_credit,
        snapshot.disc_number,
        snapshot.disc_total,
        snapshot.track_number,
        snapshot.track_total,
        snapshot.musicbrainz_release_id,
        snapshot.musicbrainz_release_group_id,
        snapshot.musicbrainz_recording_id,
        snapshot.musicbrainz_track_id
    ]);
    format!("{:x}", Sha256::digest(value.to_string().as_bytes()))
}

pub fn presentation_evidence_after(snapshot: &Snapshot, changes: &Map<String, Value>) -> String {
    let mut next = snapshot.clone();
    for (field, value) in changes {
        let text = value
            .as_str()
            .filter(|text| !text.is_empty())
            .map(str::to_string);
        let number = value.as_u64().and_then(|number| u32::try_from(number).ok());
        match field.as_str() {
            "album" => next.album = text,
            "album_artist" => next.album_artist = text,
            "artist_credit" => next.artist_credit = text.into_iter().collect(),
            "disc_number" => next.disc_number = number,
            "disc_total" => next.disc_total = number,
            "track_number" => next.track_number = number,
            "track_total" => next.track_total = number,
            "musicbrainz_release_id" => next.musicbrainz_release_id = text,
            "musicbrainz_release_group_id" => next.musicbrainz_release_group_id = text,
            "musicbrainz_recording_id" => next.musicbrainz_recording_id = text,
            "musicbrainz_track_id" => next.musicbrainz_track_id = text,
            _ => {}
        }
    }
    presentation_evidence(&next)
}

impl Sidecar {
    pub fn presentation(&self, key: &str, snapshot: &Snapshot) -> Option<Presentation> {
        let value = self.tracks.get(key)?.unknown.get("presentation")?;
        let presentation = Presentation::read(value)?;
        (presentation.evidence == presentation_evidence(snapshot)).then_some(presentation)
    }
    pub fn put_presentation(&mut self, key: &str, presentation: &Presentation) {
        self.track_mut(key)
            .unknown
            .insert("presentation".into(), presentation.value());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn presentation_evidence_rejects_stale_membership_and_preserves_unknown_fields() {
        let mut snapshot = Snapshot::default();
        snapshot.album = Some("Album".into());
        snapshot.disc_number = Some(1);
        let mut sidecar = Sidecar::new(1);
        sidecar
            .track_mut("one.flac")
            .unknown
            .insert("future_field".into(), json!({"x":7}));
        let member = Presentation {
            key: "local:fixture".into(),
            title: "Combined".into(),
            artist: "Artist".into(),
            evidence: presentation_evidence(&snapshot),
        };
        sidecar.put_presentation("one.flac", &member);
        let roundtrip = Sidecar::from_json(&sidecar.to_json()).unwrap();
        assert_eq!(roundtrip.presentation("one.flac", &snapshot), Some(member));
        assert_eq!(roundtrip.tracks["one.flac"].unknown["future_field"]["x"], 7);
        snapshot.disc_number = Some(2);
        assert!(roundtrip.presentation("one.flac", &snapshot).is_none());
    }
}
