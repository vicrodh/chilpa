mod source_cache;

use crate::{component, conflict, invalid, model::{fingerprint, hash_text, Source}, state::{Managed, Operation},
    transport::Transport, Cancellation, Input, Playlist, RelativePath, Result, State, Target,
    AssetKind, MAX_ASSETS, MAX_METADATA_BYTES, SPACE_RESERVE};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
pub struct ActionInfo { pub key: String, pub kind: AssetKind, pub title: String, pub artist: String, pub album: String }
impl ActionInfo {
    fn of(record: &Managed, path: &RelativePath) -> Self {
        Self { key:record.key.clone(),kind:record.kind,
            title:if record.title.is_empty() { path.filename().split("--").next().unwrap_or(path.filename()).to_owned() } else { record.title.clone() },
            artist:record.artist.clone(),album:record.album.clone() }
    }
}
#[derive(Clone, Debug)]
pub enum Action {
    Copy { path: RelativePath, bytes: u64, replaces: bool, info: ActionInfo },
    Remove { path: RelativePath, bytes: u64, info: ActionInfo },
}
/// I keep executable operations private; the UI can inspect but cannot edit an approved plan.
pub struct Plan {
    pub(crate) id: String,
    pub(crate) target: Target,
    pub(crate) revision: u64,
    pub(crate) operations: Vec<Operation>,
    actions: Vec<Action>,
    required_bytes: u64,
    unchanged: usize,
    source_bytes_hashed: u64,
    metadata_review: bool,
    pub(crate) source_guards: Vec<Source>,
}
impl Plan {
    pub fn id(&self) -> &str { &self.id }
    pub fn target(&self) -> &Target { &self.target }
    pub fn actions(&self) -> &[Action] { &self.actions }
    pub fn required_bytes(&self) -> u64 { self.required_bytes }
    pub fn unchanged_files(&self) -> usize { self.unchanged }
    /// Present files in a metadata review have not been cryptographically verified.
    pub fn is_metadata_review(&self) -> bool { self.metadata_review }
    pub fn source_bytes_hashed(&self) -> u64 { self.source_bytes_hashed }
    pub fn deletion_count(&self) -> usize { self.operations.iter().filter(|op| op.desired.is_none()).count() }
}

/// I resolve the whole selection before suggesting any deletion. A missing file or playlist
/// member fails the plan, rather than silently becoming an empty desired collection.
pub fn prepare(state: &State, transport: &mut dyn Transport, inputs: &[Input], playlists: &[Playlist],
    remove_unselected: bool, cancel: &Cancellation) -> Result<Plan> {
    prepare_inner(state, transport, inputs, playlists, remove_unselected, cancel, false, &mut |_| ())
}

/// I inspect destination metadata for a fast preview. Presence is not integrity verification.
/// Synchronization rechecks ownership and size; copied-byte verification is optional.
pub fn prepare_review(state: &State, transport: &mut dyn Transport, inputs: &[Input], playlists: &[Playlist],
    remove_unselected: bool, cancel: &Cancellation) -> Result<Plan> {
    prepare_review_with_progress(state, transport, inputs, playlists, remove_unselected, cancel, &mut |_| ())
}

pub fn prepare_review_with_progress(state: &State, transport: &mut dyn Transport, inputs: &[Input], playlists: &[Playlist],
    remove_unselected: bool, cancel: &Cancellation, progress: &mut (dyn FnMut(crate::ReviewProgress) + Send)) -> Result<Plan> {
    transport.set_cancellation(cancel);
    // The review snapshot is scoped to this call and never reaches execution or recovery.
    transport.begin_review();
    let result = prepare_inner(state, transport, inputs, playlists, remove_unselected, cancel, true, progress);
    transport.end_review();
    result
}

fn prepare_inner(state: &State, transport: &mut dyn Transport, inputs: &[Input], playlists: &[Playlist],
    remove_unselected: bool, cancel: &Cancellation, metadata_review: bool, progress: &mut (dyn FnMut(crate::ReviewProgress) + Send)) -> Result<Plan> {
    transport.set_cancellation(cancel);
    cancel.check()?;
    if state.interrupted()? { return Err(conflict("recover the interrupted sync before planning")); }
    let target = state.target()?.ok_or_else(|| invalid("pair a storage before planning"))?;
    crate::engine::check_target(transport, &target, cancel)?;
    if inputs.len().saturating_add(playlists.len()) > MAX_ASSETS { return Err(invalid("too many selected sync assets")); }
    let mut source_cache = source_cache::Cache::load(state)?;
    let known = state.files()?;
    let mut by_key: BTreeMap<&str, Vec<(&RelativePath, &Managed)>> = BTreeMap::new();
    for (path, record) in &known { if record.kind == AssetKind::Media { by_key.entry(&record.key).or_default().push((path, record)); } }
    let mut source_guards = Vec::new();
    let mut desired = BTreeMap::<RelativePath, (Managed, Source)>::new();
    let mut tracks = BTreeMap::new();
    let mut metadata_bytes = 0usize;
    let mut selected_bytes = 0u64;
    for (index, input) in inputs.iter().enumerate() {
        progress(crate::ReviewProgress { stage:"sources", completed:index, total:inputs.len(), current_path:input.source.display().to_string(), selected_bytes });
        cancel.check()?;
        if input.id.is_empty() || input.id.len() > 4096 || tracks.contains_key(&input.id) {
            return Err(invalid("sync track identities must be unique and nonempty"));
        }
        metadata_bytes = metadata_bytes.saturating_add(input.id.len() + input.source.as_os_str().len()
            + input.artist.len() + input.album.len() + input.title.len() + 512);
        if metadata_bytes > MAX_METADATA_BYTES { return Err(invalid("sync selection exceeds its metadata budget")); }
        let source = input.source.canonicalize()?;
        if !source.is_file() { return Err(invalid("selected sync source is not a regular file")); }
        let stamp = crate::source::Stamp::read(&source)?;
        selected_bytes = selected_bytes.checked_add(stamp.size).ok_or_else(|| invalid("sync size overflow"))?;
        let legacy = if by_key.get(input.id.as_str()).is_some_and(|rows| rows.iter().any(|(_, record)| record.source_stamp.is_none())) {
            source_cache.cached(&source)?
        } else { None };
        let previous = by_key.get(input.id.as_str()).and_then(|records| records.iter().copied().find(|(_, record)|
            (record.source_stamp.as_ref() == Some(&stamp) || (record.source_stamp.is_none() && legacy.is_some() && record.fingerprint == legacy))
            && record.title == input.title && record.artist == input.artist && record.album == input.album));
        let digest = if metadata_review { previous.and_then(|(_, record)| record.fingerprint.clone()) }
            else { Some(source_cache.fingerprint(&source, cancel, false)?) };
        stamp.check(&source)?;
        let checked = Source::CheckedFile { path:source, stamp:stamp.clone() };
        let extension = input.source.extension().and_then(|s| s.to_str()).filter(|s| !s.is_empty() && s.len() <= 16 && s.bytes().all(|b| b.is_ascii_alphanumeric()))
            .ok_or_else(|| invalid("selected media needs a portable file extension"))?.to_ascii_lowercase();
        let id = hash_text(&input.id);
        let version = digest.as_ref().map(|d| d.sha256.clone()).unwrap_or_else(|| format!("m{}", stamp.version()));
        let generated = RelativePath::new(format!("Music/{}/{}/{:02} - {}--{}-{}.{}", component(&input.artist),
            component(&input.album), input.track_number, component(&input.title), &id[..16], &version[..16], extension))?;
        let path = previous.filter(|(_, r)| metadata_review || r.fingerprint == digest).map_or(generated, |(path, _)| path.clone());
        tracks.insert(input.id.clone(), path.clone());
        if desired.insert(path, (Managed { fingerprint:digest, source_stamp:Some(stamp), kind:AssetKind::Media, key:input.id.clone(),
            title:input.title.clone(),artist:input.artist.clone(),album:input.album.clone() }, checked)).is_some() {
            return Err(conflict("two selected tracks produce the same device path"));
        }
    }
    progress(crate::ReviewProgress { stage:"sources", completed:inputs.len(), total:inputs.len(), current_path:String::new(), selected_bytes });
    let mut playlist_ids = BTreeSet::new();
    for (index, playlist) in playlists.iter().enumerate() {
        progress(crate::ReviewProgress { stage:"playlists", completed:index, total:playlists.len(), current_path:playlist.name.clone(), selected_bytes });
        cancel.check()?;
        if playlist.id.is_empty() || playlist.id.len() > 4096 || !playlist_ids.insert(&playlist.id) { return Err(invalid("sync playlist identities must be unique and nonempty")); }
        let mut bytes = b"#EXTM3U\n".to_vec();
        for id in &playlist.tracks {
            let path = tracks.get(id).ok_or_else(|| invalid("a playlist member is missing from the resolved sync selection"))?;
            bytes.extend_from_slice(b"../");
            bytes.extend_from_slice(path.as_str().as_bytes());
            bytes.push(b'\n');
            if bytes.len() > 1024 * 1024 { return Err(invalid("sync playlist exceeds 1 MiB")); }
        }
        metadata_bytes = metadata_bytes.saturating_add(bytes.len() * 2 + playlist.id.len() + playlist.name.len() + 512);
        if metadata_bytes > MAX_METADATA_BYTES { return Err(invalid("sync selection exceeds its metadata budget")); }
        let id = hash_text(&playlist.id);
        let path = RelativePath::new(format!("Playlists/{}--{}.m3u8", component(&playlist.name), &id[..16]))?;
        let digest = fingerprint(&mut bytes.as_slice(), cancel)?;
        desired.insert(path, (Managed { fingerprint:Some(digest), source_stamp:None, kind:AssetKind::Playlist, key:playlist.id.clone(),
            title:playlist.name.clone(),artist:String::new(),album:String::new() }, Source::Bytes(bytes)));
    }
    // Retained files still belong to this manifest when pruning is disabled.
    // I reject a batch before it can create a manifest that the next run cannot read.
    let retained = desired.iter().map(|(path, (record, _))| (path, record))
        .chain(known.iter().filter(|(path, record)| !remove_unselected && record.kind == AssetKind::Media && !desired.contains_key(*path)));
    let mut manifest_bytes = 0usize;
    for (index, (path, record)) in retained.enumerate() {
        let growth = record.digest_growth();
        let record = serde_json::to_vec(record).map_err(|e| invalid(e.to_string()))?;
        manifest_bytes = manifest_bytes.saturating_add(path.as_str().len()).saturating_add(record.len()).saturating_add(growth);
        if index >= MAX_ASSETS || manifest_bytes > MAX_METADATA_BYTES {
            return Err(invalid("sync manifest exceeds its metadata budget"));
        }
    }
    let mut operations = Vec::new();
    let mut actions = Vec::new();
    let mut required_bytes = 0u64;
    let mut unchanged = 0;
    // I publish all media before any playlist can reference it, then remove obsolete owned files.
    // A missing managed top-level folder proves every selected child is absent.
    // I never enumerate unrelated music outside the paired root.
    let missing_music = metadata_review && known.is_empty() && !inputs.is_empty()
        && transport.stat(&RelativePath::new("Music")?)?.is_none();
    let missing_playlists = metadata_review && known.is_empty() && !playlists.is_empty()
        && transport.stat(&RelativePath::new("Playlists")?)?.is_none();
    let destination_total = desired.len() + known.iter().filter(|(path, r)| !desired.contains_key(*path) && (remove_unselected || r.kind == AssetKind::Playlist)).count();
    let mut destination_completed = 0;
    for kind in [AssetKind::Media, AssetKind::Playlist] {
        for (path, (record, source)) in desired.iter().filter(|(_, (r, _))| r.kind == kind) {
            cancel.check()?;
            progress(crate::ReviewProgress { stage:"destination", completed:destination_completed, total:destination_total, current_path:path.to_string(), selected_bytes });
            let absent_parent = (record.kind == AssetKind::Media && missing_music) || (record.kind == AssetKind::Playlist && missing_playlists);
            let actual = if absent_parent { None } else { destination(transport, path, known.get(path), metadata_review, cancel)? };
            destination_completed += 1;
            let previous = known.get(path).and_then(|r| r.fingerprint.as_ref());
            if actual.is_some() && actual.as_ref() != previous { return Err(conflict(format!("unowned or externally changed file at {path}"))); }
            if actual.is_some() && actual == record.fingerprint { unchanged += 1; source_guards.push(source.clone()); continue; }
            required_bytes = required_bytes.checked_add(record.size()).ok_or_else(|| invalid("sync size overflow"))?;
            actions.push(Action::Copy { path:path.clone(), bytes:record.size(), replaces:actual.is_some(),info:ActionInfo::of(record,path) });
            operations.push(Operation { path:path.clone(), previous:actual, desired:Some(record.clone()), source:Some(source.clone()) });
        }
    }
    {
        for (path, record) in known.iter().filter(|(_, r)| r.kind == AssetKind::Playlist)
            .chain(known.iter().filter(|(_, r)| r.kind == AssetKind::Media)) {
            if desired.contains_key(path) || (!remove_unselected && record.kind == AssetKind::Media) { continue; }
            cancel.check()?;
            progress(crate::ReviewProgress { stage:"destination", completed:destination_completed, total:destination_total, current_path:path.to_string(), selected_bytes });
            let actual = destination(transport, path, known.get(path), metadata_review, cancel)?;
            destination_completed += 1;
            if actual.is_some() && actual.as_ref() != record.fingerprint.as_ref() { return Err(conflict(format!("externally changed file cannot be removed: {path}"))); }
            actions.push(Action::Remove { path:path.clone(), bytes:actual.as_ref().map_or(0, |f| f.size),info:ActionInfo::of(record,path) });
            operations.push(Operation { path:path.clone(), previous:actual, desired:None, source:None });
        }
    }
    if operations.len() > MAX_ASSETS { return Err(invalid("too many sync changes in one batch")); }
    if operations.iter().any(|op| op.desired.is_some()) {
        required_bytes = required_bytes.checked_add(SPACE_RESERVE).ok_or_else(|| invalid("sync size overflow"))?;
    }
    progress(crate::ReviewProgress { stage:"space", completed:destination_completed, total:destination_total, current_path:String::new(), selected_bytes });
    cancel.check()?;
    let available = transport.available_bytes()?;
    cancel.check()?;
    if !operations.is_empty() && available < required_bytes { return Err(crate::Error::Space { required:required_bytes, available }); }
    if !metadata_review { source_cache.save(state)?; }
    Ok(Plan { id:crate::token(), target, revision:state.revision()?, operations, actions, required_bytes, unchanged,
        source_bytes_hashed:source_cache.bytes_hashed, metadata_review, source_guards })
}

fn destination(transport: &mut dyn Transport, path: &RelativePath, known: Option<&Managed>,
    metadata_review: bool, cancel: &Cancellation) -> Result<Option<crate::Fingerprint>> {
    if !metadata_review { return transport.fingerprint(path, cancel); }
    let Some(info) = transport.stat(path)? else { return Ok(None) };
    let Some(known) = known else { return Err(conflict(format!("unowned file at {path}"))); };
    if info.directory || info.size != known.size() {
        return Err(conflict(format!("externally changed file at {path}")));
    }
    // I keep the intended digest for optional copy verification, without reading media here.
    Ok(Some(known.digest()?.clone()))
}
