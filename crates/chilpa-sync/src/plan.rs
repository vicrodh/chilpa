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
    prepare_inner(state, transport, inputs, playlists, remove_unselected, cancel, false)
}

/// I inspect destination metadata for a fast preview. Presence is not integrity verification.
/// Synchronization verifies every affected destination against its recorded hash before writing.
pub fn prepare_review(state: &State, transport: &mut dyn Transport, inputs: &[Input], playlists: &[Playlist],
    remove_unselected: bool, cancel: &Cancellation) -> Result<Plan> {
    // The review snapshot is scoped to this call and never reaches execution or recovery.
    transport.begin_review();
    let result = prepare_inner(state, transport, inputs, playlists, remove_unselected, cancel, true);
    transport.end_review();
    result
}

fn prepare_inner(state: &State, transport: &mut dyn Transport, inputs: &[Input], playlists: &[Playlist],
    remove_unselected: bool, cancel: &Cancellation, metadata_review: bool) -> Result<Plan> {
    if state.interrupted()? { return Err(conflict("recover the interrupted sync before planning")); }
    let target = state.target()?.ok_or_else(|| invalid("pair a storage before planning"))?;
    crate::engine::check_target(transport, &target, cancel)?;
    if inputs.len().saturating_add(playlists.len()) > MAX_ASSETS { return Err(invalid("too many selected sync assets")); }
    let mut source_cache = source_cache::Cache::load(state)?;
    let mut desired = BTreeMap::<RelativePath, (Managed, Source)>::new();
    let mut tracks = BTreeMap::new();
    let mut metadata_bytes = 0usize;
    for input in inputs {
        cancel.check()?;
        if input.id.is_empty() || input.id.len() > 4096 || tracks.contains_key(&input.id) {
            return Err(invalid("sync track identities must be unique and nonempty"));
        }
        metadata_bytes = metadata_bytes.saturating_add(input.id.len() + input.source.as_os_str().len()
            + input.artist.len() + input.album.len() + input.title.len() + 512);
        if metadata_bytes > MAX_METADATA_BYTES { return Err(invalid("sync selection exceeds its metadata budget")); }
        let source = input.source.canonicalize()?;
        if !source.is_file() { return Err(invalid("selected sync source is not a regular file")); }
        let digest = source_cache.fingerprint(&source, cancel, metadata_review)?;
        let extension = source.extension().and_then(|s| s.to_str()).filter(|s| !s.is_empty() && s.len() <= 16 && s.bytes().all(|b| b.is_ascii_alphanumeric()))
            .ok_or_else(|| invalid("selected media needs a portable file extension"))?.to_ascii_lowercase();
        let id = hash_text(&input.id);
        let path = RelativePath::new(format!("Music/{}/{}/{:02} - {}--{}-{}.{}", component(&input.artist),
            component(&input.album), input.track_number, component(&input.title), &id[..16], &digest.sha256[..16], extension))?;
        tracks.insert(input.id.clone(), path.clone());
        if desired.insert(path, (Managed { fingerprint:digest, kind:AssetKind::Media, key:input.id.clone(),
            title:input.title.clone(),artist:input.artist.clone(),album:input.album.clone() }, Source::File(source))).is_some() {
            return Err(conflict("two selected tracks produce the same device path"));
        }
    }
    let mut playlist_ids = BTreeSet::new();
    for playlist in playlists {
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
        desired.insert(path, (Managed { fingerprint:digest, kind:AssetKind::Playlist, key:playlist.id.clone(),
            title:playlist.name.clone(),artist:String::new(),album:String::new() }, Source::Bytes(bytes)));
    }
    let known = state.files()?;
    // Retained files still belong to this manifest when pruning is disabled.
    // I reject a batch before it can create a manifest that the next run cannot read.
    let retained = desired.iter().map(|(path, (record, _))| (path, record))
        .chain(known.iter().filter(|(path, record)| !remove_unselected && record.kind == AssetKind::Media && !desired.contains_key(*path)));
    let mut manifest_bytes = 0usize;
    for (index, (path, record)) in retained.enumerate() {
        let record = serde_json::to_vec(record).map_err(|e| invalid(e.to_string()))?;
        manifest_bytes = manifest_bytes.saturating_add(path.as_str().len()).saturating_add(record.len());
        if index >= MAX_ASSETS || manifest_bytes > MAX_METADATA_BYTES {
            return Err(invalid("sync manifest exceeds its metadata budget"));
        }
    }
    let mut operations = Vec::new();
    let mut actions = Vec::new();
    let mut required_bytes = 0u64;
    let mut unchanged = 0;
    // I publish all media before any playlist can reference it, then remove obsolete owned files.
    for kind in [AssetKind::Media, AssetKind::Playlist] {
        for (path, (record, source)) in desired.iter().filter(|(_, (r, _))| r.kind == kind) {
            cancel.check()?;
            let actual = destination(transport, path, known.get(path), metadata_review, cancel)?;
            let previous = known.get(path).map(|r| &r.fingerprint);
            if actual.is_some() && actual.as_ref() != previous { return Err(conflict(format!("unowned or externally changed file at {path}"))); }
            if actual.as_ref() == Some(&record.fingerprint) { unchanged += 1; continue; }
            required_bytes = required_bytes.checked_add(record.fingerprint.size).ok_or_else(|| invalid("sync size overflow"))?;
            actions.push(Action::Copy { path:path.clone(), bytes:record.fingerprint.size, replaces:actual.is_some(),info:ActionInfo::of(record,path) });
            operations.push(Operation { path:path.clone(), previous:actual, desired:Some(record.clone()), source:Some(source.clone()) });
        }
    }
    {
        for (path, record) in known.iter().filter(|(_, r)| r.kind == AssetKind::Playlist)
            .chain(known.iter().filter(|(_, r)| r.kind == AssetKind::Media)) {
            if desired.contains_key(path) || (!remove_unselected && record.kind == AssetKind::Media) { continue; }
            cancel.check()?;
            let actual = destination(transport, path, known.get(path), metadata_review, cancel)?;
            if actual.is_some() && actual.as_ref() != Some(&record.fingerprint) { return Err(conflict(format!("externally changed file cannot be removed: {path}"))); }
            actions.push(Action::Remove { path:path.clone(), bytes:actual.as_ref().map_or(0, |f| f.size),info:ActionInfo::of(record,path) });
            operations.push(Operation { path:path.clone(), previous:actual, desired:None, source:None });
        }
    }
    if operations.len() > MAX_ASSETS { return Err(invalid("too many sync changes in one batch")); }
    if operations.iter().any(|op| op.desired.is_some()) {
        required_bytes = required_bytes.checked_add(SPACE_RESERVE).ok_or_else(|| invalid("sync size overflow"))?;
    }
    let available = transport.available_bytes()?;
    if !operations.is_empty() && available < required_bytes { return Err(crate::Error::Space { required:required_bytes, available }); }
    source_cache.save(state)?;
    Ok(Plan { id:crate::token(), target, revision:state.revision()?, operations, actions, required_bytes, unchanged,
        source_bytes_hashed:source_cache.bytes_hashed, metadata_review })
}

fn destination(transport: &mut dyn Transport, path: &RelativePath, known: Option<&Managed>,
    metadata_review: bool, cancel: &Cancellation) -> Result<Option<crate::Fingerprint>> {
    if !metadata_review { return transport.fingerprint(path, cancel); }
    let Some(info) = transport.stat(path)? else { return Ok(None) };
    let Some(known) = known else { return Err(conflict(format!("unowned file at {path}"))); };
    if info.directory || info.size != known.fingerprint.size {
        return Err(conflict(format!("externally changed file at {path}")));
    }
    // This expected fingerprint is checked against bytes before any mutation by synchronize.
    Ok(Some(known.fingerprint.clone()))
}
