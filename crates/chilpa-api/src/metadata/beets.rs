//! One optional proposal job per editor, using the existing metadata worker.
//! Closing cancels the child and clears a synchronized response slot, including
//! results that arrive after close. Only a completed, open session retains candidates.
use super::{draft::Draft, fields, Editor, State as EditorState, worker::Job};
use crate::Chilpa;
use chilpa_meta::beets::{self as engine, Candidate, Error, Request, Response};
use chilpa_meta::{Credit, Hit, Provider, Release, ReleaseTrack};
use std::path::PathBuf;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};

#[derive(Default)]
pub(crate) struct BeetsState {
    pub enabled: bool,
    pub runtime: String,
    pub version: String,
    pub recommendation: String,
    pub probing: bool,
    pub candidates: Vec<Candidate>,
    pub selected: Option<usize>,
    busy: Arc<AtomicBool>,
    active: Option<Arc<Slot>>,
}

#[derive(Default)]
pub(crate) struct Slot {
    cancelled: AtomicBool,
    response: Mutex<Option<Result<Response, Error>>>,
}
impl Slot {
    fn cancel(&self) {
        let mut response = self.response.lock().unwrap_or_else(|error| error.into_inner());
        self.cancelled.store(true, Ordering::Release);
        response.take();
    }
    pub fn take(&self) -> Option<Result<Response, Error>> {
        self.response.lock().unwrap_or_else(|error| error.into_inner()).take()
    }
    fn finish(&self, result: Result<Response, Error>) {
        let mut response = self.response.lock().unwrap_or_else(|error| error.into_inner());
        if !self.cancelled.load(Ordering::Acquire) { *response = Some(result); }
    }
}

struct Permit(Arc<AtomicBool>);
impl Drop for Permit { fn drop(&mut self) { self.0.store(false, Ordering::Release); } }

pub(crate) struct Task {
    runtime: PathBuf,
    request: Request,
    slot: Arc<Slot>,
    _permit: Permit,
}
impl Task {
    pub fn run(self) -> Arc<Slot> {
        let result = (|| {
            if self.slot.cancelled.load(Ordering::Acquire) { return Err(Error::Cancelled); }
            let policy = chilpa_core::network::current();
            if matches!(self.request.operation, engine::Operation::Lookup) && (policy.offline || !policy.musicbrainz) { return Err(Error::Cancelled); }
            let root = chilpa_dirs::cache().join("beets");
            std::fs::create_dir_all(&root).map_err(|_| Error::LookupFailed)?;
            let workspace = tempfile::Builder::new().prefix("lookup-").tempdir_in(root).map_err(|_| Error::LookupFailed)?;
            engine::lookup(&self.runtime, workspace.path(), &self.request, &self.slot.cancelled)
        })();
        self.slot.finish(result);
        self.slot.clone()
    }
}

impl BeetsState {
    pub fn busy(&self) -> bool { self.busy.load(Ordering::Acquire) }
    pub(crate) fn busy_flag(&self) -> Arc<AtomicBool> { self.busy.clone() }
    pub fn cancel(&mut self) {
        if let Some(slot) = self.active.take() { slot.cancel(); }
        self.candidates.clear(); self.selected = None; self.recommendation.clear(); self.probing = false;
    }
    fn task(&mut self, request: Request) -> Option<Task> {
        if self.busy.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_err() { return None; }
        self.cancel();
        self.probing = matches!(request.operation, engine::Operation::Probe);
        let slot = Arc::new(Slot::default());
        self.active = Some(slot.clone());
        Some(Task { runtime:PathBuf::from(&self.runtime), request, slot, _permit:Permit(self.busy.clone()) })
    }
    pub fn accept(&mut self, response: Response) {
        self.active = None;
        self.version = response.beets_version;
        self.recommendation = response.recommendation;
        self.candidates = response.candidates;
    }
    pub fn selected_candidate(&self) -> Option<&Candidate> { self.enabled.then(|| self.selected.and_then(|at| self.candidates.get(at))).flatten() }
    pub fn pairs(&self, draft: &Draft, release: &Release) -> Option<Vec<Option<usize>>> {
        if !self.enabled { return None; }
        let mut pairs = vec![None; draft.rows.len()];
        if let Some(candidate) = self.selected_candidate() {
            for mapping in &candidate.mappings {
                if mapping.index < release.tracks.len() {
                    if let Some(row) = draft.row(mapping.track) { pairs[row] = Some(mapping.index); }
                }
            }
        }
        Some(pairs)
    }
}

fn credit(credit: &engine::Credit) -> Credit { Credit { name:credit.name.clone(), id:credit.id.clone(), join:credit.join.clone() } }
pub(crate) fn release(candidate: &Candidate) -> Release {
    let from = &candidate.release;
    Release { provider:match from.provider { engine::Source::MusicBrainz => Provider::MusicBrainz },
        id:from.id.clone(), title:from.title.clone(), credits:from.credits.iter().map(credit).collect(), date:from.date.clone(),
        genres:from.genres.clone(), label:from.label.clone(), catalog_number:from.catalog_number.clone(), catalog_numbers:from.catalog_numbers.clone(),
        country:from.country.clone(), barcode:from.barcode.clone(), discs:from.discs, media_titles:from.media_titles.clone(),
        release_group_id:from.release_group_id.clone(), url:from.url.clone(), tracks:from.tracks.iter().map(|track| ReleaseTrack {
            disc:track.disc, number:track.number, title:track.title.clone(), duration_ms:track.duration_ms,
            credits:track.credits.iter().map(credit).collect(), recording_id:track.recording_id.clone(), track_id:track.track_id.clone(),
        }).collect() }
}

pub(crate) fn hit(candidate: &Candidate) -> Hit {
    let from = &candidate.release;
    Hit { provider:Provider::MusicBrainz, id:from.id.clone(), title:from.title.clone(),
        artist:chilpa_meta::display(&from.credits.iter().map(credit).collect::<Vec<_>>()), date:from.date.clone(),
        tracks:from.tracks.len() as u16, country:from.country.clone(), label:from.label.clone(), catalog_number:from.catalog_number.clone(),
        format:String::new(), score:candidate.score, release_group_id:from.release_group_id.clone() }
}

fn request(draft: &Draft, durations: &HashMap<i64,u64>) -> Request {
    let value = |row:usize, name:&str| fields::track(name).map(|at| draft.rows[row].cells[at].now.clone()).unwrap_or_default();
    Request { artist:draft.album_cell("album_artist").now.clone(), album:draft.album_cell("title").now.clone(),
        catalog_number:draft.album_cell("catalog_number").now.clone(), release_id:draft.album_cell("musicbrainz_release_id").now.clone(),
        tracks:draft.rows.iter().enumerate().map(|(at,row)| engine::Track { id:row.track.id,
            title:value(at,"title"), artist:value(at,"artist_credit"), disc:value(at,"disc_number").parse().unwrap_or(0),
            number:value(at,"track_number").parse().unwrap_or(0), duration_ms:durations.get(&row.track.id).copied().unwrap_or(0), recording_id:value(at,"musicbrainz_recording_id") }).collect(),
        ..Request::default() }
}

pub(crate) fn said(error: Error) -> String {
    match error {
        Error::Unavailable => chilpa_i18n::t("The selected Python environment could not start beets."),
        Error::IncompatibleVersion => chilpa_i18n::t_args("Install beets {} in the selected Python environment.", &[engine::BEETS_VERSION]),
        Error::TrackLimit => chilpa_i18n::t_args("beets accepts up to {} tracks per lookup.", &[&engine::MAX_TRACKS.to_string()]),
        Error::Timeout => chilpa_i18n::t("The beets lookup timed out. Try again."),
        Error::Cancelled => chilpa_i18n::t("Lookup cancelled."),
        Error::InputLimit | Error::OutputLimit => chilpa_i18n::t("The beets lookup exceeded its metadata size limit."),
        Error::InvalidRequest | Error::InvalidResponse | Error::LookupFailed => chilpa_i18n::t("beets could not return a valid metadata proposal."),
    }
}

impl Editor {
    pub(crate) fn start_beets(&mut self, probe: bool, durations: &HashMap<i64,u64>) -> bool {
        if self.state != EditorState::Ready { return false; }
        if self.beets.runtime.trim().is_empty() { return self.refuse(chilpa_i18n::t("Choose the Python environment containing beets.")); }
        let Some(draft) = self.draft.as_ref() else { return false };
        let request = if probe { Request::probe() } else { request(draft, durations) };
        let Some(task) = self.beets.task(request) else { return self.refuse(chilpa_i18n::t("The previous beets lookup is still stopping.")); };
        self.lookup_turn += 1; self.searching = true; self.loading = false;
        self.release = None; self.hits.clear(); self.changes.clear(); self.preview_matched = 0; self.error.clear(); self.notice.clear();
        let sent = self.ask(Job::Beets { session:self.session, turn:self.lookup_turn, task });
        if !sent { self.searching = false; self.error = said(Error::Unavailable); }
        self.refresh(); sent
    }
}

impl Chilpa {
    pub(crate) fn metadata_beets_lease(&self) -> Arc<AtomicBool> { self.metadata.beets.busy_flag() }
    pub(crate) fn metadata_beets_network_changed(&mut self) {
        if self.metadata.beets.enabled && self.metadata.beets.busy() && !self.metadata.beets.probing { self.metadata_cancel_lookup(); }
    }
    pub(crate) fn metadata_preference_changed(&mut self, key: &str) {
        if key == "library.metaSrc" && self.metadata_lookups_off() { self.metadata_beets_network_changed(); }
        if key == crate::beets_runtime::ENGINE {
            let enabled = self.preference(key).as_deref() == Some("beets");
            if enabled != self.metadata.beets.enabled {
                self.metadata_cancel_lookup();
                self.metadata.beets.enabled = enabled;
                if enabled { self.metadata.provider = Provider::MusicBrainz; }
                self.metadata.refresh();
            }
        }
        if key == crate::beets_runtime::PYTHON {
            let runtime = self.beets_python();
            if runtime != self.metadata.beets.runtime {
                self.metadata_cancel_lookup();
                self.metadata.beets.runtime = runtime;
                self.metadata.beets.version.clear();
                self.metadata.refresh();
            }
        }
    }
    pub fn metadata_set_engine(&mut self, engine: &str) -> bool {
        if !matches!(engine,"native"|"beets") || self.metadata.saving() || self.beets_runtime_busy() { return false; }
        if self.preference(crate::beets_runtime::ENGINE).as_deref() != Some(engine) {
            if let Err(error) = self.remember_preference_checked(crate::beets_runtime::ENGINE, engine) { return self.metadata.refuse(error); }
        } else { self.metadata_preference_changed(crate::beets_runtime::ENGINE); }
        true
    }
    pub fn metadata_set_beets_runtime(&mut self, runtime: &str) -> bool {
        if runtime.len() > 4096 || runtime.contains(['\0','\n','\r']) || self.metadata.saving() || self.beets_runtime_busy() { return false; }
        if self.preference(crate::beets_runtime::PYTHON).as_deref() != Some(runtime) {
            if let Err(error) = self.remember_preference_checked(crate::beets_runtime::PYTHON, runtime) { return self.metadata.refuse(error); }
        } else { self.metadata_preference_changed(crate::beets_runtime::PYTHON); }
        true
    }
    pub fn metadata_probe_beets(&mut self) -> bool {
        !self.beets_runtime_busy() && self.metadata.start_beets(true, &HashMap::new())
    }
    pub(crate) fn metadata_beets_search(&mut self) -> bool {
        if self.beets_runtime_busy() { return self.metadata.refuse(chilpa_i18n::t("Wait for the beets installation or check to finish in Settings.")); }
        if self.metadata_lookups_off() { return self.metadata.refuse(chilpa_i18n::t("looking records up is turned off in Settings")); }
        let policy = chilpa_core::network::current();
        if policy.offline || !policy.musicbrainz || !self.musicbrainz_enabled() { return self.metadata.refuse(chilpa_core::network::blocked()); }
        let Some(draft) = self.metadata.draft.as_ref().filter(|_| self.metadata.state == EditorState::Ready) else { return false };
        if draft.rows.len() > engine::MAX_TRACKS { return self.metadata.refuse(said(Error::TrackLimit)); }
        let ids: Vec<_> = draft.rows.iter().map(|row| row.track.id).collect();
        let durations = match self.library().map(|library| library.track_durations(&ids)) {
            Some(Ok(durations)) => durations.into_iter().collect(),
            _ => return self.metadata.refuse(chilpa_i18n::t("Could not read track durations from the library.")),
        };
        self.metadata.start_beets(false, &durations)
    }
    pub fn metadata_cancel_lookup(&mut self) {
        self.metadata.beets.cancel(); self.metadata.lookup_turn += 1;
        self.metadata.searching = false; self.metadata.loading = false; self.metadata.release = None;
        self.metadata.hits.clear(); self.metadata.changes.clear(); self.metadata.preview_matched = 0; self.metadata.selected_disc = 0;
        self.metadata.refresh();
    }
}

#[cfg(test)]
mod tests;
