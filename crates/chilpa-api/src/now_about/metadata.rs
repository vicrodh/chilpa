//! I share successful MusicBrainz reads between track and album panels.

use chilpa_library::Library;
use chilpa_meta::{contributions::{self, CreditedRelease}, musicbrainz, text::is_uuid, Failure, Net};
use chilpa_models::credits::Contribution;
use chilpa_meta::contributions::Contribution as ExternalContribution;
use serde_json::{json, Value};

#[derive(Clone, Debug, Default)]
pub(super) struct Hint {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub catalog_number: String,
    pub release_id: String,
    pub recording_id: String,
    pub isrc: String,
    pub duration_ms: u64,
    pub path: String,
}

#[cfg(test)]
#[path = "metadata_tests.rs"]
mod tests;

pub(super) fn file_hint(path: &str, hint: &mut Hint) {
    if path.is_empty() || crate::source_opener::of_plugin(path).is_some() { return; }
    let container = chilpa_core::sacd_tracks::parse(std::path::Path::new(path)).map(|(file, _)| file.display().to_string());
    let home = chilpa_library::sidecar::Home::of(path, container.as_deref());
    if container.is_none() {
        if let Ok(snapshot) = chilpa_tags::edit::snapshot(std::path::Path::new(path)) {
            if hint.release_id.is_empty() { hint.release_id = snapshot.musicbrainz_release_id.unwrap_or_default(); }
            if hint.catalog_number.is_empty() { hint.catalog_number = snapshot.catalog_number.unwrap_or_default(); }
            if let Some(artist) = snapshot.album_artist { hint.album_artist = artist; }
        }
    }
    if let Ok(Some(sidecar)) = chilpa_library::sidecar::load(&home, path) {
        use chilpa_library::sidecar::Override;
        for (value, target) in [(&sidecar.album.musicbrainz_release_id, &mut hint.release_id),
            (&sidecar.album.catalog_number, &mut hint.catalog_number), (&sidecar.album.album_artist, &mut hint.album_artist)] {
            match value { Override::Absent => {}, Override::Cleared => target.clear(), Override::Value(value) => *target = value.clone() }
        }
    }
}

pub(super) fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs().min(i64::MAX as u64) as i64
}

pub(super) struct Reader<'a> {
    pub library: Option<&'a Library>,
    pub net: &'a Net<'a>,
    pub online: bool,
    pub now: i64,
}

impl Reader<'_> {
    fn read(&self, key: &str, fetch: impl FnOnce() -> Result<(Value, Vec<Contribution>), Failure>) -> Result<Value, Failure> {
        let cached = self.library.and_then(|library| library.metadata_cached(key).ok().flatten());
        if let Some(kept) = &cached {
            if kept.fresh(self.now) || !self.online { return Ok(kept.document.clone()); }
        }
        if !self.online { return Err(Failure::Unreachable("offline".into())); }
        if self.library.is_none() { return Err(Failure::Unexpected("metadata storage unavailable".into())); }
        match fetch() {
            Ok((value, credits)) => {
                if let Some(library) = self.library {
                    library.cache_metadata(key, self.now, &value, &credits).map_err(|error| Failure::Unexpected(error.to_string()))?;
                }
                Ok(value)
            }
            Err(error) => cached.map(|entry| entry.document).ok_or(error),
        }
    }

    pub fn release(&self, hint: &Hint) -> Result<Option<CreditedRelease>, Failure> { self.release_read(hint, false) }

    pub fn refresh_release(&self, hint: &Hint) -> Result<Option<CreditedRelease>, Failure> {
        if !self.online { return Err(Failure::Unreachable("offline".into())); }
        if self.library.is_none() { return Err(Failure::Unexpected("metadata storage unavailable".into())); }
        self.release_read(hint, true)
    }

    fn release_read(&self, hint: &Hint, refresh: bool) -> Result<Option<CreditedRelease>, Failure> {
        let id = if is_uuid(&hint.release_id) { hint.release_id.clone() } else {
            if hint.album.trim().is_empty() || hint.album_artist.trim().is_empty() { return Ok(None); }
            let key = format!("mb.about.v1.search:{}", json!([hint.album, hint.album_artist, hint.catalog_number]));
            let lookup = || {
                let hits = musicbrainz::search(self.net, &hint.album, &hint.album_artist, &hint.catalog_number)?;
                let fold = |s: &str| s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect::<String>();
                let best = hits.iter().filter(|hit| hit.score >= 95 && fold(&hit.title) == fold(&hint.album)
                    && fold(&hit.artist) == fold(&hint.album_artist)).max_by_key(|hit| hit.score);
                Ok((json!(best.map(|hit| &hit.id)), Vec::new()))
            };
            let cached = self.library.and_then(|library| library.metadata_cached(&key).ok().flatten());
            let value = if refresh && cached.as_ref().is_none_or(|entry| entry.document.is_null()) {
                let (value, credits) = lookup()?;
                if let Some(library) = self.library {
                    library.cache_metadata(&key, self.now, &value, &credits).map_err(|error| Failure::Unexpected(error.to_string()))?;
                }
                value
            } else { self.read(&key, lookup)? };
            let Some(id) = value.as_str().filter(|id| is_uuid(id)) else { return Ok(None); };
            id.to_string()
        };
        let key = format!("mb.about.v1.release:{id}");
        if refresh {
            let release = contributions::release_complete(self.net, &id)?;
            let all = release.contributions.iter().chain(release.tracks.iter().flat_map(|track| &track.contributions))
                .map(super::contributors::stored).collect();
            let mut entries = vec![(key, serde_json::to_value(&release).unwrap(), all)];
            let mut recordings = std::collections::BTreeMap::<String, Vec<ExternalContribution>>::new();
            for track in &release.tracks {
                if let Some(previous) = recordings.get(&track.recording_id) {
                    if previous.len() != track.contributions.len() || !previous.iter().all(|credit| track.contributions.contains(credit)) {
                        return Err(Failure::Unexpected("conflicting recording relationships".into()));
                    }
                } else { recordings.insert(track.recording_id.clone(), track.contributions.clone()); }
            }
            for (id, credits) in recordings {
                entries.push((format!("mb.about.v1.recording:{id}"), serde_json::to_value(&credits).unwrap(),
                    credits.iter().map(super::contributors::stored).collect()));
            }
            self.library.ok_or_else(|| Failure::Unexpected("metadata storage unavailable".into()))?
                .store_metadata_batch(self.now, &entries).map_err(|error| Failure::Unexpected(error.to_string()))?;
            return Ok(Some(release));
        }
        let value = self.read(&key, || {
            let release = contributions::release(self.net, &id)?;
            let credits = release.contributions.iter().chain(release.tracks.iter().flat_map(|t| &t.contributions)).map(super::contributors::stored).collect();
            Ok((serde_json::to_value(release).unwrap(), credits))
        })?;
        serde_json::from_value(value).map(Some).map_err(|e| Failure::Unexpected(e.to_string()))
    }

    pub fn track(&self, hint: &Hint) -> Result<(Vec<ExternalContribution>, String), Failure> {
        let mut id = hint.recording_id.clone();
        let mut failure = None;
        if !is_uuid(&id) {
            if let Some(isrc) = musicbrainz::isrc_key(&hint.isrc) {
                let value = self.read(&format!("mb.about.v1.isrc:{isrc}"), || {
                    let ids = contributions::recordings_by_isrc(self.net, &isrc)?;
                    Ok((json!(if ids.len() == 1 { ids.first() } else { None }), Vec::new()))
                });
                match value {
                    Ok(value) => id = value.as_str().unwrap_or_default().to_string(),
                    Err(error) => failure = Some(error),
                }
            }
        }
        if is_uuid(&id) {
            let key = format!("mb.about.v1.recording:{id}");
            let value = self.read(&key, || {
                let credits = contributions::recording(self.net, &id)?;
                Ok((serde_json::to_value(&credits).unwrap(), credits.iter().map(super::contributors::stored).collect()))
            })?;
            let credits = serde_json::from_value(value).map_err(|e| Failure::Unexpected(e.to_string()))?;
            return Ok((credits, format!("https://musicbrainz.org/recording/{id}")));
        }
        if let Some(release) = self.release(hint)? {
            if let Some(track) = contributions::matching_track(&release, &hint.title, &hint.artist, hint.duration_ms) {
                return Ok((track.contributions.clone(), format!("https://musicbrainz.org/recording/{}", track.recording_id)));
            }
        }
        match failure {
            Some(error) => Err(error),
            None => Ok((Vec::new(), String::new())),
        }
    }
}
