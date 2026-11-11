//! One lazy download worker, at most 10,000 selected identities and 64 KiB
//! transfer memory. All copies are explicitly pinned within the offline
//! limit, or within the disk when there is none (`limit`).

use super::limit::{Limit, CEILING_FILE_BYTES};
#[cfg(test)]
use super::limit::LIMITED_FILE_BYTES;
use super::{destination, digest, global_directory};
use crate::Chilpa;
use chilpa_library::{
    store::offline::{Copy, Track, MAX_TRACKS},
    Library,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    io::{Read, Seek, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

const MAX_SELECTION_BYTES: usize = 8 * 1024 * 1024;

#[derive(Default)]
struct Progress {
    phase: &'static str,
    removing: bool,
    total: usize,
    done: usize,
    failed: usize,
    bytes: u64,
    title: String,
    error: String,
    running: bool,
}

enum Update {
    Bytes(u64),
    Verifying,
}

#[derive(Default)]
pub(crate) struct State {
    removal: Option<removal::Pending>,
    task: Option<std::thread::JoinHandle<()>>,
    cancelled: Arc<AtomicBool>,
    progress: Arc<Mutex<Progress>>,
    changed: Arc<AtomicBool>,
}
impl Drop for State {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

pub(super) fn ids(value: &str) -> Vec<i64> {
    if value.len() > MAX_TRACKS * 24 {
        return Vec::new();
    }
    serde_json::from_str::<Vec<i64>>(value)
        .ok()
        .filter(|ids| ids.len() <= MAX_TRACKS)
        .unwrap_or_default()
}

impl Chilpa {
    /// Opening an action reads only the catalogue, never disk or network.
    pub fn server_offline_info(&mut self, kind: &str, selected: &str) -> String {
        if kind == "downloads" {
            let (count, bytes) = self
                .library()
                .map(|lib| {
                    (
                        lib.offline_copy_count().unwrap_or(0),
                        lib.offline_bytes().unwrap_or(0),
                    )
                })
                .unwrap_or_default();
            return json!({"count":0,"stored_count":count,"stored_bytes":bytes}).to_string();
        }
        let ids = ids(selected);
        let tracks = self
            .library()
            .and_then(|lib| lib.offline_selection(kind, &ids).ok())
            .unwrap_or_default();
        json!({"count":tracks.len().min(MAX_TRACKS),"too_many":tracks.len()>MAX_TRACKS}).to_string()
    }

    pub fn server_offline_start(&mut self, kind: &str, selected: &str) -> String {
        self.poll_server_offline();
        let refused = |error: String| json!({"error":error}).to_string();
        if self.servers.offline.task.is_some() {
            return self.server_offline_status();
        }
        if self.offline() {
            return refused(chilpa_core::network::blocked());
        }
        let Some(keys) = self.servers.keys.clone() else {
            return refused(chilpa_i18n::t("servers are not started"));
        };
        let selected_ids = ids(selected);
        let tracks = self
            .library()
            .and_then(|lib| lib.offline_selection(kind, &selected_ids).ok())
            .unwrap_or_default();
        let mut size: usize = tracks
            .iter()
            .map(|track| {
                track.entry.len()
                    + track.title.len()
                    + track.revision.as_ref().map_or(0, String::len)
                    + track.codec.as_ref().map_or(0, String::len)
            })
            .sum::<usize>()
            + tracks.capacity() * std::mem::size_of::<Track>();
        if tracks.is_empty() || tracks.len() > MAX_TRACKS || size > MAX_SELECTION_BYTES {
            return refused(chilpa_i18n::t(
                "Choose between 1 and 10,000 server tracks for an offline download.",
            ));
        }
        let mut scopes = HashMap::new();
        let Some(lib) = self.library() else {
            return refused(chilpa_i18n::t("the library could not be opened"));
        };
        for track in &tracks {
            if scopes.contains_key(&track.source) {
                continue;
            }
            let Some(stored) = lib.server(track.source).ok().flatten() else {
                return refused(super::super::session::changed());
            };
            let scope = super::super::session::Scope::of(&stored);
            size = size.saturating_add(scope.retained_bytes());
            if size > MAX_SELECTION_BYTES {
                return refused(chilpa_i18n::t(
                    "Choose between 1 and 10,000 server tracks for an offline download.",
                ));
            }
            scopes.insert(track.source, scope);
        }
        let root = match global_directory(self.preference(super::ROOT_PREFERENCE).as_deref()) {
            Ok(root) => root,
            Err(error) => return refused(error),
        };
        let collection = digest(&format!("{kind}:{selected}"));
        let limit = self.offline_limit();
        let progress = Arc::new(Mutex::new(Progress {
            total: tracks.len(),
            running: true,
            ..Default::default()
        }));
        let cancelled = Arc::new(AtomicBool::new(false));
        let changed = Arc::new(AtomicBool::new(false));
        let (worker_progress, worker_cancelled, worker_changed) =
            (progress.clone(), cancelled.clone(), changed.clone());
        let path = Library::default_path();
        let task = std::thread::Builder::new()
            .name("chilpa-offline-download".into())
            .spawn(move || {
                run(
                    &path,
                    &root,
                    &keys,
                    &tracks,
                    &scopes,
                    &collection,
                    &limit,
                    &worker_progress,
                    &worker_cancelled,
                    &worker_changed,
                );
                worker_progress
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .running = false;
            });
        match task {
            Ok(task) => {
                self.servers.offline = State {
                    removal: None,
                    task: Some(task),
                    progress,
                    cancelled,
                    changed,
                }
            }
            Err(_) => return refused(chilpa_i18n::t("The download worker could not be started.")),
        }
        self.server_offline_status()
    }

    pub fn server_offline_cancel(&mut self) {
        self.servers
            .offline
            .cancelled
            .store(true, Ordering::Release);
    }

    pub fn server_offline_status(&self) -> String {
        let state = &self.servers.offline;
        let progress = state
            .progress
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        json!({"running":progress.running,"phase":progress.phase,"removing":progress.removing,"total":progress.total,"done":progress.done,"failed":progress.failed,
            "bytes":progress.bytes,"title":progress.title,"error":progress.error,
            "cancelled":state.cancelled.load(Ordering::Acquire)}).to_string()
    }

    pub(crate) fn poll_server_offline(&mut self) {
        let state = &mut self.servers.offline;
        if state.changed.swap(false, Ordering::AcqRel) {
            self.servers.changed = true;
        }
        if state.task.as_ref().is_some_and(|task| task.is_finished()) {
            if let Some(task) = state.task.take() {
                let _ = task.join();
            }
            state
                .progress
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .running = false;
        }
    }
}

fn run(
    path: &Path,
    root: &Path,
    keys: &super::super::keys::Keys,
    tracks: &[Track],
    scopes: &HashMap<i64, super::super::session::Scope>,
    collection: &str,
    limit: &Limit,
    progress: &Mutex<Progress>,
    cancelled: &AtomicBool,
    changed: &AtomicBool,
) {
    let lib = match Library::open_existing(path) {
        Ok(lib) => lib,
        Err(_) => {
            progress
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .error = chilpa_i18n::t("the library could not be opened");
            return;
        }
    };
    for track in tracks {
        if cancelled.load(Ordering::Acquire) || chilpa_core::network::current().offline {
            cancelled.store(true, Ordering::Release);
            break;
        }
        {
            let mut state = progress.lock().unwrap_or_else(|error| error.into_inner());
            state.title = track.title.chars().take(512).collect();
            state.bytes = 0;
            state.phase = "downloading";
        }
        let result = scopes
            .get(&track.source)
            .ok_or_else(super::super::session::changed)
            .and_then(|scope| {
                download(
                    &lib,
                    root,
                    keys,
                    track,
                    scope,
                    collection,
                    limit,
                    cancelled,
                    |update| {
                        let mut state = progress.lock().unwrap_or_else(|error| error.into_inner());
                        match update {
                            Update::Bytes(bytes) => {
                                state.bytes = bytes;
                                state.phase = "downloading";
                            }
                            Update::Verifying => state.phase = "verifying",
                        }
                    },
                )
            });
        let mut state = progress.lock().unwrap_or_else(|error| error.into_inner());
        match result {
            Ok(()) => {
                state.done += 1;
                changed.store(true, Ordering::Release);
            }
            Err(error) => {
                state.failed += 1;
                state.error = error;
            }
        }
    }
}

fn extension(codec: Option<&str>) -> &str {
    match codec.unwrap_or("") {
        "flac" => "flac",
        "mp3" => "mp3",
        "aac" => "aac",
        "alac" | "m4a" | "mp4" => "m4a",
        "wav" | "pcm" => "wav",
        "ogg" | "vorbis" => "ogg",
        "opus" => "opus",
        "aiff" | "aif" => "aiff",
        "ape" => "ape",
        "dsf" => "dsf",
        "dff" => "dff",
        _ => "audio",
    }
}

fn draft(
    stored: &chilpa_library::StoredServer,
    track: &Track,
    root: &Path,
) -> Result<Copy, String> {
    let kind = chilpa_servers::Kind::from_provider(&stored.provider)
        .ok_or_else(super::super::session::changed)?;
    let folder = destination(
        kind,
        &stored.product,
        &stored.instance,
        &stored.account,
        &stored.offline_folder,
        root,
    )?;
    let revision = digest(track.revision.as_deref().unwrap_or(""));
    let key = digest(
        &serde_json::json!([
            stored.provider,
            stored.instance,
            stored.account,
            stored.identity,
            track.entry,
            revision,
            "original"
        ])
        .to_string(),
    );
    let path = folder.join(format!("{key}.{}", extension(track.codec.as_deref())));
    Ok(Copy {
        key,
        source: stored.source_id,
        account: stored.account.clone(),
        identity: stored.identity.clone(),
        entry: track.entry.clone(),
        revision,
        path: path
            .to_str()
            .ok_or_else(|| {
                chilpa_i18n::t("The offline folder cannot be represented as a local path.")
            })?
            .to_string(),
        bytes: 0,
        checksum: String::new(),
        content_type: String::new(),
        codec: String::new(),
        extension: String::new(),
        validator: String::new(),
        state: "queued".into(),
        error: String::new(),
    })
}

fn download(
    lib: &Library,
    root: &Path,
    keys: &super::super::keys::Keys,
    track: &Track,
    scope: &super::super::session::Scope,
    collection: &str,
    limit: &Limit,
    cancelled: &AtomicBool,
    progress: impl Fn(Update),
) -> Result<(), String> {
    let stored = lib
        .server(track.source)
        .ok()
        .flatten()
        .filter(|stored| stored.enabled && scope.matches(stored))
        .ok_or_else(super::super::session::changed)?;
    let mut item = draft(&stored, track, root)?;
    let database = || chilpa_i18n::t("the library could not be written");
    lib.queue_offline_copy(&item, collection)
        .map_err(|_| database())?;
    item = lib
        .offline_copy_by_key(&item.key)
        .map_err(|_| database())?
        .ok_or_else(database)?;
    progress(Update::Verifying);
    if item.state == "complete" {
        match super::checked(&item, || cancelled.load(Ordering::Acquire)) {
            Ok(_) => return Ok(()),
            Err(super::CheckError::Cancelled) => return Err(chilpa_i18n::t("Download cancelled.")),
            Err(super::CheckError::Busy) => {
                return Err(chilpa_i18n::t(
                    "This offline file is in use. Stop playback before removing it.",
                ))
            }
            Err(super::CheckError::Invalid) => {}
        }
    }
    if item.state == "complete" {
        let retained = super::disk_bytes(&item);
        lib.invalidate_offline_copy(&item, "", retained)
            .map_err(|_| database())?;
    }
    let path = PathBuf::from(&item.path);
    let result = transfer(
        lib, keys, track, &stored, scope, &mut item, &path, limit, cancelled, progress,
    );
    if let Err(error) = &result {
        let retained = super::disk_bytes(&item);
        lib.fail_offline_copy(&item.key, error, retained)
            .map_err(|_| database())?;
    }
    result
}

fn transfer(
    lib: &Library,
    keys: &super::super::keys::Keys,
    track: &Track,
    stored: &chilpa_library::StoredServer,
    scope: &super::super::session::Scope,
    item: &mut Copy,
    path: &Path,
    limit: &Limit,
    cancelled: &AtomicBool,
    progress: impl Fn(Update),
) -> Result<(), String> {
    let failed = || chilpa_i18n::t("The offline file could not be written.");
    let folder = path.parent().ok_or_else(failed)?;
    std::fs::create_dir_all(folder).map_err(|_| failed())?;
    // Without a limit the disk is the budget: a file larger than its free
    // space is refused before a byte is written.
    let maximum = match limit.quota {
        None if !path.exists() => chilpa_devices::transport::available(folder)
            .map_or(limit.file_bytes(), |free| free.min(limit.file_bytes())),
        _ => limit.file_bytes(),
    };
    let full = || {
        if limit.quota.is_some() {
            chilpa_i18n::t("The offline storage limit has been reached.")
        } else {
            chilpa_i18n::t("There is not enough free space on the disk for this file.")
        }
    };
    let reservation = lib
        .reserve_offline_copy(&item.key, maximum, limit.quota)
        .map_err(|_| failed())?;
    item.bytes = reservation;
    if reservation == 0 {
        return Err(full());
    }
    let temporary = path.with_extension("part");
    // A prior final file can be the atomic promotion left by a crash.
    // Revalidate it and confirm its registry without replacing any bytes.
    if path.exists() {
        progress(Update::Verifying);
        let (bytes, checksum, codec, container) =
            inspect(path, Some(extension(track.codec.as_deref())), cancelled)?;
        if !item.checksum.is_empty() && item.checksum != checksum {
            return Err(chilpa_i18n::t(
                "The offline copy is damaged. Remove the offline copy before downloading it again.",
            ));
        }
        if bytes > reservation {
            return Err(full());
        }
        item.bytes = bytes;
        item.checksum = checksum;
        item.codec = codec;
        item.extension = container;
    } else {
        let request = {
            let _session = keys.session();
            let current = lib
                .server(stored.source_id)
                .ok()
                .flatten()
                .filter(|server| scope.matches(server))
                .ok_or_else(super::super::session::changed)?;
            let server =
                super::super::net::server(&current).ok_or_else(super::super::session::changed)?;
            let key = super::super::net::key(keys, &current)
                .ok_or_else(|| chilpa_i18n::t_args("sign in to {} again", &[&current.label]))?;
            let remote = chilpa_models::RemoteEntry::parse(&track.entry)
                .ok_or_else(super::super::session::changed)?;
            if remote.source.provider != stored.provider
                || remote.source.instance != stored.instance
            {
                return Err(super::super::session::changed());
            }
            let stream = chilpa_servers::stream(
                &server,
                &key,
                &remote.external_id,
                track.codec.as_deref(),
                track.revision.as_deref(),
            );
            chilpa_core::RangedRequest {
                url: stream.url,
                headers: stream.headers,
            }
        };
        // A managed partial has no playback eligibility. Retry starts from
        // zero instead of appending bytes without a proven validator.
        if std::fs::symlink_metadata(&temporary).is_ok_and(|metadata| !metadata.is_file()) {
            return Err(failed());
        }
        let mut output = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&temporary)
            .map_err(|_| failed())?;
        let received = chilpa_core::download::server::download(
            &request,
            &mut output,
            reservation,
            || cancelled.load(Ordering::Acquire),
            |bytes| progress(Update::Bytes(bytes)),
        );
        let result = (|| {
            // The transfer says the limit was reached when the file is larger
            // than its budget; without a limit that budget is the disk.
            let limited = chilpa_i18n::t("The offline storage limit has been reached.");
            let received = received.map_err(|error| if error == limited { full() } else { error })?;
            output.flush().map_err(|_| failed())?;
            output.sync_all().map_err(|_| failed())?;
            drop(output);
            progress(Update::Verifying);
            let (bytes, checksum, codec, container) = inspect(
                &temporary,
                Some(extension(track.codec.as_deref())),
                cancelled,
            )?;
            if bytes != received.bytes {
                return Err(chilpa_i18n::t("The download is incomplete. Try again."));
            }
            if cancelled.load(Ordering::Acquire) {
                return Err(chilpa_i18n::t("Download cancelled."));
            }
            item.bytes = bytes;
            item.checksum = checksum;
            item.codec = codec;
            item.extension = container;
            item.content_type = received.content_type;
            item.validator = received.validator;
            if !lib
                .server(stored.source_id)
                .ok()
                .flatten()
                .is_some_and(|server| scope.matches(&server) && server.enabled)
            {
                return Err(super::super::session::changed());
            }
            std::fs::rename(&temporary, path).map_err(|_| failed())?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        result?;
    }
    if !lib
        .complete_offline_copy(item, stored.session_revision)
        .map_err(|_| failed())?
    {
        return Err(super::super::session::changed());
    }
    Ok(())
}

fn inspect(
    path: &Path,
    extension: Option<&str>,
    cancelled: &AtomicBool,
) -> Result<(u64, String, String, String), String> {
    let invalid = || chilpa_i18n::t("The downloaded file could not be verified as complete audio.");
    let mut file = std::fs::File::open(path).map_err(|_| invalid())?;
    let bytes = file.metadata().map_err(|_| invalid())?.len();
    if bytes == 0 || bytes > CEILING_FILE_BYTES {
        return Err(invalid());
    }
    let mut buffer = [0u8; 64 * 1024];
    let mut hash = Sha256::new();
    loop {
        if cancelled.load(Ordering::Acquire) {
            return Err(chilpa_i18n::t("Download cancelled."));
        }
        let count = file.read(&mut buffer).map_err(|_| invalid())?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    file.rewind().map_err(|_| invalid())?;
    let count = file.read(&mut buffer).map_err(|_| invalid())?;
    let header = buffer[..count].to_vec();
    file.rewind().map_err(|_| invalid())?;
    let codec =
        chilpa_core::remote::validate_copy(file, extension, || cancelled.load(Ordering::Acquire))
            .map_err(|_| invalid())?;
    let container = container(&header, &codec, extension).to_string();
    Ok((bytes, format!("{:x}", hash.finalize()), codec, container))
}

fn container<'a>(header: &[u8], codec: &'a str, hint: Option<&'a str>) -> &'a str {
    if header.starts_with(b"fLaC") {
        return "flac";
    }
    if header.starts_with(b"OggS") {
        return "ogg";
    }
    if header.starts_with(b"RIFF") && header.get(8..12) == Some(b"WAVE") {
        return "wav";
    }
    if header.starts_with(b"FORM") && matches!(header.get(8..12), Some(b"AIFF" | b"AIFC")) {
        return "aiff";
    }
    let mut offset = 0usize;
    while let Some(box_header) = header.get(offset..offset.saturating_add(8)) {
        if matches!(&box_header[4..8], b"ftyp" | b"moov" | b"mdat") {
            return "m4a";
        }
        let length = u32::from_be_bytes(box_header[..4].try_into().unwrap()) as usize;
        if length < 8 {
            break;
        }
        offset = offset.saturating_add(length);
    }
    match codec {
        "vorbis" | "opus" => "ogg",
        "alac" => "m4a",
        "pcm" => hint.unwrap_or("wav"),
        "mp3" => "mp3",
        "aac" => "aac",
        "flac" => "flac",
        _ => hint.unwrap_or(codec),
    }
}

mod recovery;
mod removal;
#[cfg(test)]
mod tests;
