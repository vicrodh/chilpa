//! What the player asks when a track on a server comes up: where its bytes
//! are, and what to call it (`chilpa_core::remote`).
//!
//! It runs on the player's thread, with a connection to the library of its
//! own. The credential comes from `Keys`, already open: the player never
//! waits on the keyring and never holds a secret longer than one request.

use super::keys::Keys;
use chilpa_core::remote::{Described, Located, Media, RemoteOpener};
use chilpa_models::RemoteEntry;
use std::sync::{Arc, Mutex};

pub(crate) struct Opener {
    pub(crate) keys: Arc<Keys>,
    library: Mutex<Option<chilpa_library::Library>>,
    /// Sources whose track failed to open, for the facade to check.
    pub(crate) failures: Arc<Mutex<Vec<i64>>>,
    invalid:super::offline::InvalidCopies,
}

impl Opener {
    fn signal_failure(&self,source:i64) {
        let mut failures=self.failures.lock().unwrap_or_else(|error| error.into_inner());
        if failures.len()<64 && !failures.contains(&source) {failures.push(source);}
    }
    pub(crate) fn new(keys: Arc<Keys>, failures: Arc<Mutex<Vec<i64>>>,invalid:super::offline::InvalidCopies) -> Self {
        Self { keys, library: Mutex::new(None), failures,invalid }
    }

    /// An opener on a library of the test's own.
    #[cfg(test)]
    pub(crate) fn on(keys: Arc<Keys>, library: chilpa_library::Library, invalid: super::offline::InvalidCopies) -> Self {
        Self { keys, library: Mutex::new(Some(library)), failures: Arc::default(), invalid }
    }

    fn with_library<T>(&self, f: impl FnOnce(&chilpa_library::Library) -> Option<T>) -> Option<T> {
        let mut library = self.library.lock().unwrap_or_else(|e| e.into_inner());
        if library.is_none() {
            *library = chilpa_library::Library::open_read_only(&chilpa_library::Library::default_path()).ok();
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
            let copy=lib.offline_copy(source,&stored.account,&stored.identity,&written).ok().flatten();
            Some((stored, codec, revision, copy))
        });
        let Some((stored, codec, revision, copy)) = found else {
            return Err(chilpa_i18n::t("that track is no longer in the library"));
        };
        if !stored.enabled {
            return Err(chilpa_i18n::t("that server is disabled"));
        }
        let copy=copy.filter(|item| item.revision==super::offline::digest(revision.as_deref().unwrap_or("")));
        let checked=copy.as_ref().map(|item| super::offline::checked(item,|| false));
        if matches!(&checked,Some(Err(super::offline::CheckError::Invalid))) {
            if let Some(item)=&copy {
                let mut invalid=self.invalid.pending.lock().unwrap_or_else(|error| error.into_inner());
                let bytes=invalid.iter().map(chilpa_library::store::offline::Copy::retained_bytes).sum::<usize>();
                if let Some(queued)=invalid.iter().find(|copy| copy.key==item.key) {
                    if queued.checksum!=item.checksum || queued.path!=item.path || queued.bytes!=item.bytes {
                        self.invalid.scan.store(true,std::sync::atomic::Ordering::Release);
                    }
                } else if invalid.len()<64 && bytes.saturating_add(item.retained_bytes())<=chilpa_library::store::offline::MAX_SELECTION_BYTES {
                    invalid.push(item.clone());
                } else {self.invalid.scan.store(true,std::sync::atomic::Ordering::Release);}
            }
            self.signal_failure(stored.source_id);
        }
        if let Some(Ok((file,lease)))=checked {
            if !self.with_library(|lib| lib.server(stored.source_id).ok().flatten())
                .is_some_and(|current| current.enabled && current.account==stored.account && current.identity==stored.identity) {
                return Err(super::session::changed());
            }
            return Ok(Located {normalization:None,playback:None,media:Media::File {file,lease},extension:copy.as_ref().map(|item| item.extension.clone()),server:stored.label});
        }
        if chilpa_core::network::current().offline {
            return Err(chilpa_i18n::t("This track has no verified offline copy. Download it while connected."));
        }
        let _session=self.keys.session();
        // A connection edit may have completed during local verification.
        if !self.with_library(|lib| lib.server(stored.source_id).ok().flatten())
            .is_some_and(|current| super::session::Scope::of(&stored).matches(&current) && current.enabled) {
            return Err(super::session::changed());
        }
        let server = super::net::server(&stored).ok_or_else(|| chilpa_i18n::t("that track is no longer in the library"))?;
        let key = super::net::key(&self.keys, &stored).ok_or_else(|| chilpa_i18n::t_args("sign in to {} again", &[&stored.label]))?;
        let stream = chilpa_servers::stream(&server, &key, &entry.external_id, codec.as_deref(), revision.as_deref());
        Ok(Located {
            normalization: None,
            playback: None,
            media: chilpa_core::remote::Media::Remote(chilpa_core::RangedRequest { url: stream.url, headers: stream.headers }),
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
        Described { tags, artwork: artwork.map(std::path::PathBuf::from), ..Default::default() }
    }

    fn failed(&self, entry: &RemoteEntry, _why: &str, _play: Option<u64>) {
        let source = self.with_library(|lib| lib.find_source(&entry.source.provider, &entry.source.instance).ok().flatten());
        if let Some(source) = source {
            self.signal_failure(source);
        }
    }
}
