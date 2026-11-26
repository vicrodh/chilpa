//! One read-only metadata lookup over a frozen selected group, using the
//! existing isolated adapter. Candidate bodies stay in the session manifest.
use super::{model::Row, worker::Session};
use chilpa_meta::beets::{self, Request, Track};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

pub(super) struct Permit(Arc<AtomicBool>);
impl Permit {
    pub fn acquire(flag: Arc<AtomicBool>) -> Result<Self, String> {
        flag.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| chilpa_i18n::t("Wait for the current beets operation to finish."))?;
        Ok(Self(flag))
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

pub(super) fn evidence(row: &Row) -> Result<String, String> {
    let body = serde_json::to_vec(&(row.proposed(), &row.stamp, row.duration_ms))
        .map_err(|error| error.to_string())?;
    Ok(format!("{:x}", Sha256::digest(body)))
}
pub(super) fn summary_text(value: &str) -> String {
    value.chars().take(256).collect()
}

impl Session {
    pub fn lookup_group(
        &self,
        key: &str,
        runtime: &Path,
        cancel: &AtomicBool,
    ) -> Result<(), String> {
        self.manifest
            .clear_documents("lookup")
            .map_err(|error| error.to_string())?;
        self.manifest
            .clear_documents("candidate")
            .map_err(|error| error.to_string())?;
        let group = self.group(key)?;
        let policy = chilpa_core::network::current();
        if policy.offline || !policy.musicbrainz {
            return Err(chilpa_core::network::blocked());
        }
        if chilpa_core::settings::load()
            .window
            .get("library.metaSrc")
            .map(String::as_str)
            == Some("embedded")
        {
            return Err(chilpa_i18n::t(
                "Looking up metadata is turned off in Settings.",
            ));
        }
        let mut request = Request::default();
        let mut albums = BTreeSet::new();
        let mut artists = BTreeSet::new();
        let mut releases = BTreeSet::new();
        let mut catalogs = BTreeSet::new();
        let mut evidence = serde_json::Map::new();
        let mut after = 0;
        let mut input_bytes = 8192;
        let mut dsd = false;
        while let Some(entry) = self
            .manifest
            .next_in_group(after, Some(&group))
            .map_err(|error| error.to_string())?
        {
            after = entry.id;
            if !entry.selected {
                continue;
            }
            if cancel.load(Ordering::Acquire) {
                return Err(chilpa_i18n::t("The metadata lookup was cancelled."));
            }
            if entry.state == "unsupported" {
                return Err(chilpa_i18n::t(
                    "Choose readable files for this metadata lookup.",
                ));
            }
            if request.tracks.len() >= beets::MAX_TRACKS {
                return Err(chilpa_i18n::t(
                    "Choose at most 1024 files for one metadata lookup.",
                ));
            }
            let row = Row::read(&entry)?;
            dsd |= crate::metadata::beets::is_dsd(&row.path);
            let values = row.proposed();
            let text = |name| {
                values
                    .get(name)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            };
            let number = |name| {
                values
                    .get(name)
                    .and_then(Value::as_u64)
                    .and_then(|number| u16::try_from(number).ok())
                    .unwrap_or(0)
            };
            for field in [
                "album",
                "album_artist",
                "artist_credit",
                "musicbrainz_release_id",
                "catalog_number",
                "title",
                "musicbrainz_recording_id",
            ] {
                if text(field).len() > 4096 {
                    return Err(chilpa_i18n::t(
                        "A metadata lookup field exceeds the 4096-byte input limit.",
                    ));
                }
            }
            albums.insert(text("album"));
            let album_artist = text("album_artist");
            let artist = if album_artist.is_empty() {
                text("artist_credit")
            } else {
                album_artist
            };
            if artists.len() < 2 {
                artists.insert(artist);
            }
            releases.insert(text("musicbrainz_release_id"));
            if catalogs.len() < 2 {
                catalogs.insert(text("catalog_number"));
            }
            if albums.len() > 1 || releases.iter().filter(|id| !id.is_empty()).count() > 1 {
                return Err(chilpa_i18n::t(
                    "Select files with consistent album and release tags for one lookup.",
                ));
            }
            evidence.insert(entry.id.to_string(), json!(self::evidence(&row)?));
            let track = Track {
                id: entry.id,
                title: text("title"),
                artist: text("artist_credit"),
                disc: number("disc_number"),
                number: number("track_number"),
                duration_ms: row.duration_ms,
                recording_id: text("musicbrainz_recording_id"),
            };
            input_bytes += serde_json::to_vec(&track)
                .map_err(|error| error.to_string())?
                .len()
                + 256;
            if input_bytes > beets::MAX_INPUT {
                return Err(chilpa_i18n::t(
                    "The selected lookup exceeds the 1 MiB input limit.",
                ));
            }
            request.tracks.push(track);
        }
        if request.tracks.is_empty() {
            return Err(chilpa_i18n::t(
                "Select files from this group before looking up metadata.",
            ));
        }
        if albums.len() > 1 || releases.iter().filter(|id| !id.is_empty()).count() > 1 {
            return Err(chilpa_i18n::t(
                "Select one album or prepare a consistent album proposal before looking up this group.",
            ));
        }
        request.album = albums.into_iter().next().unwrap_or_default();
        request.artist = if artists.len() == 1 {
            artists.into_iter().next().unwrap_or_default()
        } else {
            String::new()
        };
        request.release_id = releases
            .into_iter()
            .find(|id| !id.is_empty())
            .unwrap_or_default();
        request.catalog_number = if catalogs.len() == 1 {
            catalogs.into_iter().next().unwrap_or_default()
        } else {
            String::new()
        };
        if request.album.is_empty() && request.artist.is_empty() && request.release_id.is_empty() {
            return Err(chilpa_i18n::t(
                "Add an album or artist proposal before looking up files with empty tags.",
            ));
        }
        // As in the album editor: an SACD's editions lead, Chilpa's own
        // search is scored as well, and a chosen environment is told apart.
        request.media = if dsd { "SACD".into() } else { String::new() };
        request.strict = crate::beets_runtime::tested_only(&runtime.to_string_lossy());
        crate::metadata::beets::add_own_releases(&crate::metadata::worker::Network::new(), &mut request);
        if cancel.load(Ordering::Acquire) {
            return Err(chilpa_i18n::t("The metadata lookup was cancelled."));
        }
        let lookup_key = self
            .plan_key(&json!({"group":group,"evidence":evidence}))?
            .replace(":plan:", ":lookup:");
        let directory = tempfile::Builder::new()
            .prefix("lookup-")
            .tempdir_in(self.directory())
            .map_err(|error| error.to_string())?;
        let response = beets::lookup(runtime, directory.path(), &request, cancel)
            .map_err(crate::metadata::beets::said)?;
        let mut headers = Vec::new();
        let operation = || {
            self.manifest
                .clear_documents("candidate")
                .map_err(|error| error.to_string())?;
            for (index, candidate) in response.candidates.iter().enumerate() {
                let candidate_key = format!("{lookup_key}:{index}");
                let body = json!({"group":group,"evidence":evidence,"candidate":candidate});
                self.manifest
                    .put_document(
                        "candidate",
                        &candidate_key,
                        &serde_json::to_string(&body).map_err(|error| error.to_string())?,
                    )
                    .map_err(|error| error.to_string())?;
                let mut hit = serde_json::to_value(crate::metadata::beets::hit(candidate))
                    .map_err(|error| error.to_string())?;
                if let Some(object) = hit.as_object_mut() {
                    for value in object.values_mut() {
                        if let Some(text) = value.as_str() {
                            *value = json!(summary_text(text));
                        }
                    }
                }
                headers.push(json!({"key":candidate_key,"groupKey":key,"hit":hit,"matched":candidate.mappings.len(),
                    "unmatched":request.tracks.len()-candidate.mappings.len(),"conflicts":[]}));
            }
            self.put_document(
                "lookup",
                &json!({"id":lookup_key,"groupKey":key,"dataRevision":self.revision,"busy":false,
                "error":"","recommendation":response.recommendation,"candidates":headers}),
            )
        };
        self.manifest.transaction(operation)
    }
}
