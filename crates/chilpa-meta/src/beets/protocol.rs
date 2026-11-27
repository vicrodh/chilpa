//! Metadata-only messages. The schema has no input field for a music path.
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use super::Error;

pub const PROTOCOL: u8 = 1;
pub const BEETS_VERSION: &str = "2.14.1";
pub const MAX_TRACKS: usize = 1024;
pub const MAX_CANDIDATES: usize = 5;
/// Releases the host found on its own and asks beets to score as well.
pub const MAX_IDS: usize = 8;
pub const MAX_INPUT: usize = 1024 * 1024;
pub const MAX_OUTPUT: usize = 8 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source { #[default] MusicBrainz }

#[derive(Clone, Copy, Debug, Default, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Operation { #[default] Lookup, Probe }

#[derive(Clone, Debug, Default, Serialize)]
pub struct Track {
    pub id: i64,
    pub title: String,
    pub artist: String,
    pub disc: u16,
    pub number: u16,
    pub duration_ms: u64,
    pub recording_id: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Request {
    pub protocol: u8,
    pub operation: Operation,
    pub source: Source,
    pub artist: String,
    pub album: String,
    pub catalog_number: String,
    pub release_id: String,
    /// "SACD" when the files are DSD: its editions lead. Empty otherwise.
    pub media: String,
    pub year: u16,
    pub label: String,
    pub release_ids: Vec<String>,
    /// Only the version the bridge was tested with is accepted. A copy
    /// Chilpa installed asks for it; an environment the listener chose runs
    /// whatever beets it has, and is told it was not tested.
    pub strict: bool,
    pub tracks: Vec<Track>,
}
impl Default for Request {
    fn default() -> Self {
        Self { protocol:PROTOCOL, operation:Operation::Lookup, source:Source::MusicBrainz,
            artist:String::new(), album:String::new(), catalog_number:String::new(), release_id:String::new(),
            media:String::new(), year:0, label:String::new(), release_ids:Vec::new(), strict:true, tracks:Vec::new() }
    }
}
impl Request {
    pub fn probe() -> Self { Self { operation:Operation::Probe, ..Self::default() } }
    pub(crate) fn encode(&self) -> Result<Vec<u8>, Error> {
        if self.protocol != PROTOCOL { return Err(Error::InvalidRequest); }
        if matches!(self.operation, Operation::Lookup) {
            if self.tracks.is_empty() || self.tracks.len() > MAX_TRACKS { return Err(Error::TrackLimit); }
            let mut ids = HashSet::new();
            if self.tracks.iter().any(|track| !ids.insert(track.id)) { return Err(Error::InvalidRequest); }
        }
        if self.release_ids.len() > MAX_IDS || self.release_ids.iter().any(|id| id.len() > 64) { return Err(Error::InputLimit); }
        let strings = [&self.artist, &self.album, &self.catalog_number, &self.release_id, &self.media, &self.label].into_iter()
            .chain(self.tracks.iter().flat_map(|track| [&track.title, &track.artist, &track.recording_id]));
        if strings.into_iter().any(|value| value.len() > 4096) { return Err(Error::InputLimit); }
        let bytes = serde_json::to_vec(self).map_err(|_| Error::InvalidRequest)?;
        if bytes.len() > MAX_INPUT { return Err(Error::InputLimit); }
        Ok(bytes)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Credit { pub name:String, pub id:String, pub join:String }
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ReleaseTrack {
    pub disc:u16, pub number:u16, pub title:String, pub duration_ms:u32, pub credits:Vec<Credit>,
    pub recording_id:String, pub track_id:String,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Release {
    pub provider:Source, pub id:String, pub title:String, pub credits:Vec<Credit>, pub date:String,
    pub genres:Vec<String>, pub label:String, pub catalog_number:String, pub catalog_numbers:Vec<String>,
    pub country:String, pub barcode:String, pub discs:u16, pub media_titles:Vec<String>,
    pub release_group_id:String, pub url:String, pub tracks:Vec<ReleaseTrack>,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Mapping { pub track:i64, pub index:usize, pub composers:Vec<String>, pub work:String }
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Candidate {
    pub release:Release,
    /// How the files fit the discs they land on.
    pub score:u8,
    /// How they fit the whole release, every disc counted.
    #[serde(default)] pub whole:u8,
    /// The release's discs the files land on.
    #[serde(default)] pub discs:Vec<u16>,
    /// What those discs are, as the source names them: "Hybrid SACD (SACD layer, 2 channels)".
    #[serde(default)] pub format:String,
    pub mappings:Vec<Mapping>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Response {
    pub protocol:u8,
    #[serde(default)]
    pub beets_version:String,
    #[serde(default)]
    pub recommendation:String,
    #[serde(default)]
    pub candidates:Vec<Candidate>,
    pub error:Option<String>,
}
impl Response {
    pub(crate) fn decode(bytes: &[u8], request: &Request) -> Result<Self, Error> {
        if bytes.len() > MAX_OUTPUT { return Err(Error::OutputLimit); }
        let response: Self = serde_json::from_slice(bytes).map_err(|_| Error::InvalidResponse)?;
        if response.protocol != PROTOCOL { return Err(Error::IncompatibleVersion); }
        if let Some(error) = &response.error {
            return Err(match error.as_str() {
                "unavailable" => Error::Unavailable,
                "incompatible_version" => Error::IncompatibleVersion,
                "input_limit" => Error::InputLimit,
                "output_limit" => Error::OutputLimit,
                "track_limit" => Error::TrackLimit,
                "invalid_request" => Error::InvalidRequest,
                "invalid_mapping" | "invalid_score" => Error::InvalidResponse,
                _ => Error::LookupFailed,
            });
        }
        if request.strict && response.beets_version != BEETS_VERSION { return Err(Error::IncompatibleVersion); }
        if response.beets_version.is_empty() || response.beets_version.len() > 64 { return Err(Error::InvalidResponse); }
        if response.candidates.len() > MAX_CANDIDATES { return Err(Error::InvalidResponse); }
        let ids: HashSet<_> = request.tracks.iter().map(|track| track.id).collect();
        for candidate in &response.candidates {
            let release = &candidate.release;
            if candidate.score > 100 || candidate.whole > 100 || candidate.discs.len() > 64 || candidate.format.len() > 4096
                || release.id.is_empty() || release.tracks.len() > MAX_TRACKS
                || candidate.mappings.len() > request.tracks.len()
                || !(release.url.is_empty() || release.url.starts_with("https://") || release.url.starts_with("http://")) {
                return Err(Error::InvalidResponse);
            }
            let (mut mapped, mut targets) = (HashSet::new(), HashSet::new());
            for mapping in &candidate.mappings {
                if !ids.contains(&mapping.track) || mapping.index >= release.tracks.len()
                    || !mapped.insert(mapping.track) || !targets.insert(mapping.index) { return Err(Error::InvalidResponse); }
            }
        }
        Ok(response)
    }
}

#[cfg(test)]
mod tests;
