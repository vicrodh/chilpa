//! What the player asks when a track on a server comes up: where its bytes
//! are, and what to call it (`chilpa_core::remote`).
//!
//! It runs on the player's thread, with a connection to the library of its
//! own. The credential comes from `Keys`, already open: the player never
//! waits on the keyring and never holds a secret longer than one request.

use super::keys::Keys;
use chilpa_core::remote::{Described, Located, RemoteOpener};
use chilpa_models::RemoteEntry;
use std::sync::{Arc, Mutex};

pub(crate) struct Opener {
    pub(crate) keys: Arc<Keys>,
    library: Mutex<Option<chilpa_library::Library>>,
    /// Sources whose track failed to open, for the facade to check.
    pub(crate) failures: Arc<Mutex<Vec<i64>>>,
}

impl Opener {
    pub(crate) fn new(keys: Arc<Keys>, failures: Arc<Mutex<Vec<i64>>>) -> Self {
        Self { keys, library: Mutex::new(None), failures }
    }

    fn with_library<T>(&self, f: impl FnOnce(&chilpa_library::Library) -> Option<T>) -> Option<T> {
        let mut library = self.library.lock().unwrap_or_else(|e| e.into_inner());
        if library.is_none() {
            *library = chilpa_core::open_library().ok();
        }
        f(library.as_ref()?)
    }
}

impl RemoteOpener for Opener {
    fn locate(&self, entry: &RemoteEntry) -> Result<Located, String> {
        let written = entry.to_entry();
        let found = self.with_library(|lib| {
            let source = lib.find_source(&entry.source.provider, &entry.source.instance).ok()??;
            let stored = lib.server(source).ok()??;
            let (codec, revision) = lib.server_track(source, &written).ok()??;
            Some((stored, codec, revision))
        });
        let Some((stored, codec, revision)) = found else {
            return Err(chilpa_i18n::t("that track is no longer in the library"));
        };
        let server = super::net::server(&stored).ok_or_else(|| chilpa_i18n::t("that track is no longer in the library"))?;
        let key = self.keys.get(&stored.instance).ok_or_else(|| chilpa_i18n::t_args("sign in to {} again", &[&stored.label]))?;
        let stream = chilpa_servers::stream(&server, &key, &entry.external_id, codec.as_deref(), revision.as_deref());
        Ok(Located {
            playback: None,
            request: chilpa_core::RangedRequest { url: stream.url, headers: stream.headers },
            extension: stream.extension,
            server: stored.label.clone(),
        })
    }

    fn describe(&self, entry: &RemoteEntry) -> Described {
        let card = self.with_library(|lib| lib.server_track_card(&entry.to_entry()).ok().flatten());
        let Some((title, artist, album, album_artist, artwork)) = card else { return Described::default() };
        let tags = chilpa_tags::Tags {
            title: Some(title),
            artists: if artist.is_empty() { Vec::new() } else { vec![artist] },
            album: Some(album).filter(|a| !a.is_empty()),
            album_artist: Some(album_artist).filter(|a| !a.is_empty()),
            ..Default::default()
        };
        Described { tags, artwork: artwork.map(std::path::PathBuf::from) }
    }

    fn failed(&self, entry: &RemoteEntry, _why: &str) {
        let source = self.with_library(|lib| lib.find_source(&entry.source.provider, &entry.source.instance).ok().flatten());
        if let Some(source) = source {
            self.failures.lock().unwrap_or_else(|e| e.into_inner()).push(source);
        }
    }
}
