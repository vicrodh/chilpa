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

use chilpa_library::store::offline::MAX_SELECTION_BYTES;

#[derive(Default)]
pub(super) struct Progress {
    phase: &'static str,
    removing: bool,
    total: usize,
    done: usize,
    failed: usize,
    bytes: u64,
    title: String,
    error: String,
    running: bool,
    cancelled: bool,
}

enum Update {
    Bytes(u64),
    Verifying,
    /// The file's size, when it is known before the transfer says it.
    Size(u64),
}

#[derive(Default)]
pub(crate) struct State {
    removal: Option<removal::Pending>,
    task: Option<std::thread::JoinHandle<()>>,
    pub(super) tracker: Arc<Mutex<queue::Tracker>>,
    changed: Arc<AtomicBool>,
    /// The library's kept and failed copies, read again when one changed.
    pub(super) stale: bool,
}
impl Drop for State {
    fn drop(&mut self) {
        queue::lock(&self.tracker).cancel_all();
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

    /// Queues the selection's downloads behind whatever is queued.
    pub fn server_offline_start(&mut self, kind: &str, selected: &str) -> String {
        self.server_offline_fetch(kind, selected, false)
    }

    /// `fresh` fetches a kept copy again instead of only checking it.
    pub(super) fn server_offline_fetch(&mut self, kind: &str, selected: &str, fresh: bool) -> String {
        self.poll_server_offline();
        let selected_ids = ids(selected);
        let tracks = self
            .library()
            .and_then(|lib| lib.offline_selection(kind, &selected_ids).ok())
            .unwrap_or_default();
        let chosen: Vec<i64> = tracks.iter().map(|track| track.id).collect();
        let refused = |this: &mut Self, error: String| {
            queue::lock(&this.servers.offline.tracker).refuse(&chosen, &error);
            json!({"error":error}).to_string()
        };
        if self.offline() {
            return refused(self, chilpa_core::network::blocked());
        }
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
            return refused(self, chilpa_i18n::t(
                "Choose between 1 and 10,000 server tracks for an offline download.",
            ));
        }
        let mut scopes = HashMap::new();
        let Some(lib) = self.library() else {
            return refused(self, chilpa_i18n::t("the library could not be opened"));
        };
        let mut changed = None;
        for track in &tracks {
            if scopes.contains_key(&track.source) {
                continue;
            }
            let Some(stored) = lib.server(track.source).ok().flatten() else {
                changed = Some(super::super::session::changed());
                break;
            };
            let scope = super::super::session::Scope::of(&stored);
            size = size.saturating_add(scope.retained_bytes());
            if size > MAX_SELECTION_BYTES {
                changed = Some(chilpa_i18n::t(
                    "Choose between 1 and 10,000 server tracks for an offline download.",
                ));
                break;
            }
            scopes.insert(track.source, Some(scope));
        }
        if let Some(error) = changed {
            return refused(self, error);
        }
        let root = match global_directory(self.preference(super::ROOT_PREFERENCE).as_deref()) {
            Ok(root) => root,
            Err(error) => return refused(self, error),
        };
        let collection = digest(&format!("{kind}:{selected}"));
        let limit = self.offline_limit();
        let work = tracks.into_iter().map(|track| (track.id, queue::Work::Fetch { track, fresh })).collect();
        self.offline_enqueue(kind, selected_ids, |seq| queue::Job { seq, scopes, collection, root, limit }, work)
    }

    /// Everything waiting is dropped and the file on its way stops.
    pub fn server_offline_cancel(&mut self) {
        queue::lock(&self.servers.offline.tracker).cancel_all();
    }

    /// Only the selection's tracks stop or leave the queue.
    pub(super) fn server_offline_cancel_selection(&mut self, kind: &str, selected: &str) {
        let chosen: std::collections::HashSet<i64> = if kind == "tracks" {
            ids(selected).into_iter().collect()
        } else {
            self.library()
                .and_then(|lib| lib.offline_selection(kind, &ids(selected)).ok())
                .unwrap_or_default()
                .into_iter()
                .map(|track| track.id)
                .collect()
        };
        queue::lock(&self.servers.offline.tracker).cancel(|id| chosen.contains(&id));
    }

    pub fn server_offline_status(&self) -> String {
        let tracker = queue::lock(&self.servers.offline.tracker);
        let progress = &tracker.progress;
        json!({"running":tracker.busy(),"phase":progress.phase,"removing":progress.removing,"total":progress.total,"done":progress.done,"failed":progress.failed,
            "bytes":progress.bytes,"title":progress.title,"error":progress.error,
            "cancelled":progress.cancelled}).to_string()
    }

    pub(crate) fn poll_server_offline(&mut self) {
        let state = &mut self.servers.offline;
        if state.changed.swap(false, Ordering::AcqRel) {
            self.servers.changed = true;
            state.stale = true;
        }
        if state.task.as_ref().is_some_and(|task| task.is_finished()) {
            if let Some(task) = state.task.take() {
                let _ = task.join();
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

/// A download that checks a kept copy instead of fetching it again.
#[cfg(test)]
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
    obtain(lib, root, keys, track, scope, collection, limit, cancelled, false, progress)
}

/// One track's copy. A kept copy is only checked, unless `fresh` asks to
/// fetch it again: then the old file stays in place until the new one is
/// verified, and comes back if the new one does not. A cancelled first
/// download leaves nothing behind, not even a failure.
fn obtain(
    lib: &Library,
    root: &Path,
    keys: &super::super::keys::Keys,
    track: &Track,
    scope: &super::super::session::Scope,
    collection: &str,
    limit: &Limit,
    cancelled: &AtomicBool,
    fresh: bool,
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
    if item.state == "complete" && !fresh {
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
    let previous = (fresh && item.state == "complete").then(|| item.clone());
    if let Some(previous) = &previous {
        progress(Update::Size(previous.bytes));
    }
    if item.state == "complete" {
        let retained = super::disk_bytes(&item);
        lib.invalidate_offline_copy(&item, "", retained)
            .map_err(|_| database())?;
    }
    let path = PathBuf::from(&item.path);
    let result = transfer(
        lib, keys, track, &stored, scope, &mut item, &path, limit, cancelled, fresh, progress,
    );
    if let Err(error) = &result {
        let restored = previous.as_ref().is_some_and(|previous| {
            super::checked(previous, || false).is_ok()
                && lib.complete_offline_copy(previous, stored.session_revision).unwrap_or(false)
        });
        if restored {
            return result;
        }
        let forgotten = previous.is_none()
            && cancelled.load(Ordering::Acquire)
            && lib
                .offline_copy_by_key(&item.key)
                .ok()
                .flatten()
                .is_some_and(|copy| copy.state != "complete" && removal::remove(lib, &copy, Some(scope)).is_ok());
        if !forgotten {
            let retained = super::disk_bytes(&item);
            lib.fail_offline_copy(&item.key, error, retained)
                .map_err(|_| database())?;
        }
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
    fresh: bool,
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
    // Revalidate it and confirm its registry without replacing any bytes,
    // unless the listener asked for the file again.
    if path.exists() && !fresh {
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

pub(super) mod queue;
mod recovery;
mod removal;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod playback_tests;
