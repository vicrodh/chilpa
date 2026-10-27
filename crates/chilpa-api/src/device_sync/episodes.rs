//! I stage explicit podcast downloads independently of streaming music and the Library.
use super::{catalog::Pick, failure, selection::OptionRow, UiError};
use chilpa_sync::{Cancellation, Input};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
impl std::fmt::Debug for Episode {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("Episode")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Episode {
    pub id: String,
    pub title: String,
    pub show: String,
    pub publisher: String,
    pub url: String,
    pub format: String,
}
impl Episode {
    pub(crate) fn valid(&self) -> bool {
        !self.id.is_empty()
            && self.id.len() <= 256
            && !self.title.is_empty()
            && [&self.title, &self.show, &self.publisher]
                .iter()
                .all(|s| s.len() <= 1024)
            && Self::public_audio(&self.url, &self.format)
    }
    pub(crate) fn public_audio(url: &str, format: &str) -> bool {
        url.len() <= 8192 && chilpa_core::download::allowed(url)
            && ["mp3", "m4a", "aac", "ogg", "opus", "flac", "wav"].contains(&format)
    }
    pub(super) fn row(self) -> OptionRow {
        let id = self.id.clone();
        OptionRow {
            id: id.clone(),
            name: self.title.clone(),
            description: self.show.clone(),
            checked: true,
            pick: Pick::Episode {
                id,
                name: self.title.clone(),
                episode: self,
            },
        }
    }
    pub(super) fn track(&self) -> chilpa_library::ExportTrack {
        chilpa_library::ExportTrack {
            id: 0,
            key: self.id.clone(),
            path: None,
            title: self.title.clone(),
            artist: self.publisher.clone(),
            album: self.show.clone(),
            number: 0,
            disc: 1,
            available: true,
        }
    }
}

/// I keep at most 2 GiB of task-owned temporary audio, 512 MiB per episode, until review/apply ends.
pub(super) fn resolve(
    picks: &[Pick],
    inputs: &mut Vec<Input>,
    cancel: &Cancellation,
) -> Result<Option<tempfile::TempDir>, UiError> {
    let episodes: Vec<_> = picks
        .iter()
        .filter_map(|pick| {
            if let Pick::Episode { episode, .. } = pick {
                Some(episode)
            } else {
                None
            }
        })
        .collect();
    if episodes.is_empty() {
        return Ok(None);
    }
    let directory = tempfile::tempdir().map_err(|_| {
        failure(
            chilpa_i18n::mark("Temporary episode storage is unavailable."),
            "",
        )
    })?;
    let mut remaining = 2u64 * 1024 * 1024 * 1024;
    for episode in episodes {
        cancel.check().map_err(UiError::from)?;
        if !episode.valid() {
            return Err(failure(
                chilpa_i18n::mark("This episode does not have a supported public audio file."),
                "",
            ));
        }
        if inputs.iter().any(|input| input.id == episode.id) {
            continue;
        }
        let path = directory.path().join(format!(
            "{:x}.{}",
            Sha256::digest(&episode.id),
            episode.format
        ));
        let mut output = std::fs::File::create(&path).map_err(|_| {
            failure(
                chilpa_i18n::mark("Temporary episode storage is unavailable."),
                "",
            )
        })?;
        let bytes = chilpa_core::download::download(
            &episode.url,
            &mut output,
            remaining.min(512 * 1024 * 1024),
            || cancel.check().is_err(),
        )
        .map_err(|message| failure(message, ""))?;
        remaining -= bytes;
        inputs.push(Input {
            id: episode.id.clone(),
            source: path,
            artist: episode.publisher.clone(),
            album: episode.show.clone(),
            title: episode.title.clone(),
            track_number: 0,
        });
        if inputs.len() > chilpa_sync::MAX_ASSETS {
            return Err(failure(
                chilpa_i18n::mark(
                    "A device selection can contain at most 50,000 files and playlists.",
                ),
                "",
            ));
        }
    }
    Ok(Some(directory))
}
