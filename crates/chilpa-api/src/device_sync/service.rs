//! I give one requested job the session; idle services hold no worker or transport.
use super::{browser::Browser, library};
use super::{
    catalog::{Catalog, Pick, Profile, Root, Store},
    failure,
    selection::{self, OptionRow},
    work::{self, ChangeRow, Destination, Ready},
    UiError,
};
use chilpa_sync::Cancellation;
mod browsing;
mod connections;
use serde::Serialize;
use std::{
    collections::{HashSet, VecDeque},
    path::PathBuf,
    sync::{
        mpsc::{sync_channel, Receiver, TryRecvError},
        Arc, Mutex,
    },
};

#[derive(Clone, Debug, Default, Serialize)]
pub struct Status {
    pub phase: String,
    pub busy: bool,
    pub profile: String,
    pub destination: String,
    pub error: Option<UiError>,
    pub copies: usize,
    pub updates: usize,
    pub removals: usize,
    pub unchanged: usize,
    pub tracks: usize,
    pub remove_unselected: bool,
    pub required: String,
    pub plan: String,
    pub completed: usize,
    pub total: usize,
    pub current: String,
    pub transferred: String,
    pub metadata_review: bool,
}
enum Outcome {
    Refreshed(
        Catalog,
        Vec<Destination>,
        Vec<OptionRow>,
        library::Labels,
        Option<UiError>,
    ),
    Added(Profile),
    Connections(Vec<Destination>, Option<UiError>, Status),
    Reopened(Vec<OptionRow>, library::Labels, Vec<Destination>, Option<UiError>, Status),
    Browsed(Result<Browser, UiError>),
    PlaylistRead(Option<UiError>),
    Saved(Profile),
    Selected(OptionRow),
    Prepared(Profile),
    Applied,
}
pub struct Service {
    store: Store,
    library: PathBuf,
    catalog: Catalog,
    destinations: Vec<Destination>,
    options: Vec<OptionRow>,
    available_options: Vec<OptionRow>,
    rows: Vec<ChangeRow>,
    status: Status,
    ready: Option<Ready>,
    review_result: Arc<Mutex<(bool, Option<Ready>)>>,
    pending: Option<Receiver<Result<Outcome, UiError>>>,
    cancel: Cancellation,
    progress: Arc<Mutex<Option<chilpa_sync::Progress>>>,
    revision: u64,
    view_loaded: bool,
    drafts: super::drafts::Drafts,
    incoming_ids: VecDeque<Vec<i64>>,
    labels: library::Labels,
    browser: Option<Browser>,
    browse_previous: Option<Status>,
    browse_source: String,
    browse_title: String,
    browser_error: Option<UiError>,
    reading_playlist: bool,
    playlist_result: Arc<Mutex<Option<Browser>>>,
}
impl Default for Service {
    fn default() -> Self {
        Self::new(
            work::default_library(),
            chilpa_dirs::data().join("device-sync"),
        )
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        self.cancel.cancel();
        let mut result = self.review_result.lock().unwrap_or_else(|e| e.into_inner());
        *result = (false, None);
    }
}
impl Service {
    pub fn new(library: PathBuf, directory: PathBuf) -> Self {
        Self {
            store: Store { directory },
            library,
            catalog: Catalog::default(),
            destinations: Vec::new(),
            options: Vec::new(),
            available_options: Vec::new(),
            rows: Vec::new(),
            status: Status {
                phase: "idle".into(),
                ..Default::default()
            },
            ready: None,
            review_result: Arc::new(Mutex::new((false, None))),
            pending: None,
            cancel: Cancellation::default(),
            progress: Arc::new(Mutex::new(None)),
            revision: 0,
            view_loaded: false,
            drafts: Default::default(),
            incoming_ids: VecDeque::new(),
            labels: library::Labels::new(),
            browser: None,
            browse_previous: None,
            browse_source: String::new(),
            browse_title: String::new(),
            browser_error: None,
            reading_playlist: false,
            playlist_result: Arc::new(Mutex::new(None)),
        }
    }
    pub fn status(&self) -> &Status {
        &self.status
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn destinations(&self) -> &[Destination] {
        &self.destinations
    }
    pub fn options(&self) -> &[OptionRow] {
        &self.options
    }
    pub fn profiles(&self) -> &[Profile] {
        &self.catalog.profiles
    }
    pub fn profile(&self) -> Option<&Profile> {
        self.catalog
            .profiles
            .iter()
            .find(|p| p.id == self.status.profile)
    }
    pub fn change_page(&self, offset: usize, limit: usize) -> &[ChangeRow] {
        let start = offset.min(self.rows.len());
        let end = start.saturating_add(limit.min(200)).min(self.rows.len());
        &self.rows[start..end]
    }
    pub fn change_count(&self) -> usize {
        self.rows.len()
    }
    fn idle(&mut self) -> Result<(), UiError> {
        self.poll();
        if self.pending.is_some() {
            Err(failure(
                chilpa_i18n::mark("A device operation is still running."),
                "",
            ))
        } else {
            Ok(())
        }
    }
    fn invalidate(&mut self) {
        self.close_browser();
        self.ready = None;
        self.rows.clear();
        self.status.plan.clear();
        self.status.phase = "idle".into();
        self.status.error = None;
        self.status.copies = 0;
        self.status.updates = 0;
        self.status.removals = 0;
        self.status.unchanged = 0;
        self.status.tracks = 0;
        self.status.required.clear();
        self.status.metadata_review = false;
    }
    fn launch(
        &mut self,
        phase: &str,
        job: impl FnOnce(Store, PathBuf, Cancellation) -> Result<Outcome, UiError> + Send + 'static,
    ) -> Result<(), UiError> {
        if self.pending.is_some() { return Err(failure(chilpa_i18n::mark("A device operation is still running."), "")); }
        self.cancel = Cancellation::default();
        self.status.phase = phase.into();
        self.status.busy = true;
        self.status.error = None;
        self.status.completed = 0;
        self.status.total = 0;
        self.status.current.clear();
        self.status.transferred.clear();
        if let Ok(mut slot) = self.progress.lock() {
            *slot = None;
        }
        let (store, library, cancel) = (
            self.store.clone(),
            self.library.clone(),
            self.cancel.clone(),
        );
        let (sender, receiver) = sync_channel(1);
        std::thread::Builder::new()
            .name("chilpa-device-sync".into())
            .spawn(move || {
                let _ = sender.send(job(store, library, cancel));
            })
            .map_err(|e| {
                self.status.busy = false;
                failure(chilpa_i18n::mark("I could not start the device task."), e)
            })?;
        self.pending = Some(receiver);
        Ok(())
    }
    /// I restore the surface on demand while keeping a completed task's summary.
    pub fn open_view(&mut self) -> Result<(), UiError> {
        self.review_result.lock().unwrap_or_else(|e| e.into_inner()).0 = true;
        self.poll();
        if self.pending.is_some() {
            return Ok(());
        }
        if !self.incoming_ids.is_empty() {
            return self.refresh();
        }
        if self.view_loaded {
            return Ok(());
        }
        if self.catalog.profiles.is_empty() {
            return self.refresh();
        }
        let previous = self.status.clone();
        let catalog = self.catalog.clone();
        self.launch("loading", move |_, library, cancel| {
            let options = selection::options(&library)?;
            let labels = library::labels(&library, &catalog, &cancel)?;
            cancel.check().map_err(UiError::from)?;
            let (destinations, error) = work::discover(&catalog, &cancel);
            Ok(Outcome::Reopened(options, labels, destinations, error, previous))
        })
    }
    pub fn refresh(&mut self) -> Result<(), UiError> {
        self.idle()?;
        self.invalidate();
        self.launch("discovering", |store, library, cancel| {
            let catalog = store.read()?;
            let options = if library.is_file() {
                selection::options(&library)?
            } else {
                Vec::new()
            };
            let (destinations, error) = work::discover(&catalog, &cancel);
            let labels = library::labels(&library, &catalog, &cancel)?;
            Ok(Outcome::Refreshed(
                catalog,
                destinations,
                options,
                labels,
                error,
            ))
        })
    }
    /// I refresh connection metadata only; Library rows and the current selection stay resident.
    pub fn connections_changed(&mut self) -> Result<(), UiError> {
        self.idle()?;
        let previous = self.status.clone();
        let catalog = self.catalog.clone();
        self.launch("reconnecting", move |_, _, cancel| {
            let (destinations, error) = work::discover(&catalog, &cancel);
            Ok(Outcome::Connections(destinations, error, previous))
        })
    }
    /// I load saved profiles and selections without claiming a USB interface.
    pub fn load(&mut self) -> Result<(), UiError> {
        self.idle()?;
        self.invalidate();
        self.launch("loading", |store, library, cancel| {
            let catalog = store.read()?;
            let options = if library.is_file() {
                selection::options(&library)?
            } else {
                Vec::new()
            };
            let labels = library::labels(&library, &catalog, &cancel)?;
            let destination = work::mounted_for(&catalog);
            Ok(Outcome::Refreshed(
                catalog,
                vec![destination],
                options,
                labels,
                None,
            ))
        })
    }
    pub fn select_profile(&mut self, id: &str) -> Result<(), UiError> {
        self.idle()?;
        if id == self.status.profile {
            return Ok(());
        }
        if !self.catalog.profiles.iter().any(|p| p.id == id) {
            return Err(failure(
                chilpa_i18n::mark("Choose a saved device profile."),
                "",
            ));
        }
        self.store.remember_profile(id)?;
        self.invalidate();
        self.status.profile = id.into();
        self.align_options();
        self.revision += 1;
        Ok(())
    }
    pub fn select_destination(&mut self, id: &str) -> Result<(), UiError> {
        self.idle()?;
        if id == self.status.destination {
            return Ok(());
        }
        if !self.destinations.iter().any(|d| d.id == id && d.writable) {
            return Err(failure(
                chilpa_i18n::mark("Choose an available writable storage."),
                "",
            ));
        }
        self.invalidate();
        self.status.destination = id.into();
        self.revision += 1;
        Ok(())
    }
    pub fn add_profile(&mut self, name: &str, folder: &str) -> Result<(), UiError> {
        let (destination, name, folder) = (
            self.status.destination.clone(),
            name.to_owned(),
            folder.to_owned(),
        );
        if destination.is_empty() {
            return Err(failure(
                chilpa_i18n::mark("Choose the connected player and its storage first."),
                "",
            ));
        }
        self.idle()?;
        self.invalidate();
        self.launch("pairing", move |store, _, cancel| {
            Ok(Outcome::Added(work::add(
                &store,
                &destination,
                &name,
                &folder,
                &cancel,
            )?))
        })
    }
    pub fn toggle(&mut self, id: &str) -> Result<(), UiError> {
        self.idle()?;
        self.invalidate();
        let row = self
            .options
            .iter_mut()
            .find(|o| o.id == id)
            .ok_or_else(|| {
                failure(
                    chilpa_i18n::mark("This Library selection is no longer available."),
                    "",
                )
            })?;
        let mut edited = row.clone();
        edited.checked = !edited.checked;
        self.drafts.put(&self.status.profile, edited.clone())?;
        *row = edited;
        self.revision += 1;
        Ok(())
    }
    pub fn set_removals(&mut self, remove_unselected: bool) -> Result<(), UiError> {
        self.idle()?;
        if remove_unselected == self.status.remove_unselected {
            return Ok(());
        }
        self.invalidate();
        self.drafts.removals(&self.status.profile, remove_unselected);
        self.status.remove_unselected = remove_unselected;
        self.revision += 1;
        Ok(())
    }
    pub fn save_selection(&mut self) -> Result<(), UiError> {
        self.idle()?;
        let id = self.status.profile.clone();
        let picks = self.selected();
        let remove = self.status.remove_unselected;
        self.invalidate();
        self.launch("saving", move |store, _, _| {
            Ok(Outcome::Saved(work::save_selection(
                &store, &id, picks, remove,
            )?))
        })
    }
    fn selected(&self) -> Vec<Pick> {
        self.options
            .iter()
            .filter(|o| o.checked)
            .map(|o| o.pick.clone())
            .collect()
    }
    /// I stage a menu request until its acknowledged device surface opens.
    pub fn stage_library_tracks(&mut self, raw: &str) -> Result<(), UiError> {
        self.poll();
        if raw.len() > 1024 * 1024 {
            return Err(failure(
                chilpa_i18n::mark("Choose between 1 and 50,000 Library tracks."),
                "",
            ));
        }
        let ids: Vec<i64> = serde_json::from_str(raw).map_err(|e| {
            failure(
                chilpa_i18n::mark("I could not read the selected Library tracks."),
                e,
            )
        })?;
        if ids.is_empty() || ids.len() > chilpa_sync::MAX_ASSETS || ids.iter().any(|id| *id <= 0) {
            return Err(failure(
                chilpa_i18n::mark("Choose between 1 and 50,000 Library tracks."),
                "",
            ));
        }
        let queued = self.incoming_ids.iter().map(Vec::len).sum::<usize>();
        if queued.saturating_add(ids.len()) > chilpa_sync::MAX_ASSETS || self.incoming_ids.len() >= 512 {
            return Err(failure(chilpa_i18n::mark("Choose between 1 and 50,000 Library tracks."), ""));
        }
        self.incoming_ids.push_back(ids);
        Ok(())
    }
    pub fn add_library_tracks(&mut self, ids: &[i64]) -> Result<(), UiError> {
        self.idle()?;
        self.start_library_tracks(ids)
    }
    fn start_library_tracks(&mut self, ids: &[i64]) -> Result<(), UiError> {
        if ids.is_empty() || ids.len() > chilpa_sync::MAX_ASSETS {
            return Err(failure(
                chilpa_i18n::mark("Choose between 1 and 50,000 Library tracks."),
                "",
            ));
        }
        let ids = ids.to_vec();
        self.invalidate();
        self.launch("selecting", move |_, path, cancel| {
            cancel.check().map_err(UiError::from)?;
            let library = chilpa_library::Library::open_read_only(&path).map_err(|e| {
                failure(
                    chilpa_i18n::mark("I could not read the selected Library tracks."),
                    e,
                )
            })?;
            let tracks = library.export_tracks(&ids).map_err(|e| {
                failure(
                    chilpa_i18n::mark(
                        "Some selected Library tracks are missing. I kept the device unchanged.",
                    ),
                    e,
                )
            })?;
            Ok(Outcome::Selected(library::batch(tracks)))
        })
    }
    pub fn stage_episode(&mut self, episode: super::Episode) -> Result<(), UiError> {
        self.idle()?;
        if !episode.valid() { return Err(failure(chilpa_i18n::mark("This episode does not have a supported public audio file."), "")); }
        self.invalidate();
        let row = episode.row();
        self.drafts.put(&self.status.profile, row.clone())?;
        if self.view_loaded { self.options.retain(|r| r.id != row.id); self.options.insert(0, row); }
        self.revision += 1;
        Ok(())
    }
    pub fn preview(&mut self, recover: bool) -> Result<(), UiError> {
        self.idle()?;
        let profile = self
            .profile()
            .cloned()
            .ok_or_else(|| failure(chilpa_i18n::mark("Choose a saved device profile."), ""))?;
        let destination = self.status.destination.clone();
        let picks = self.selected();
        let remove = self.status.remove_unselected;
        self.invalidate();
        let phase = if picks.iter().any(|pick| matches!(pick, Pick::Episode { .. })) { "downloading" } else { "preparing" };
        let result = self.review_result.clone();
        result.lock().unwrap_or_else(|e| e.into_inner()).0 = true;
        self.launch(phase, move |store, library, cancel| {
            let profile = work::save_selection(&store, &profile.id, picks, remove)?;
            let ready = work::prepare(&store, &library, profile, destination, recover, &cancel)?;
            let profile = ready.profile.clone();
            let mut slot = result.lock().unwrap_or_else(|e| e.into_inner());
            if slot.0 { slot.1 = Some(ready); }
            Ok(Outcome::Prepared(profile))
        })
    }
    pub fn apply(&mut self, token: &str, approve_removals: bool) -> Result<(), UiError> {
        self.idle()?;
        let ready = self.ready.as_ref().ok_or_else(|| {
            failure(
                chilpa_i18n::mark("Review the device changes before syncing."),
                "",
            )
        })?;
        if ready.plan.id() != token || (ready.plan.deletion_count() > 0 && !approve_removals) {
            return Err(failure(
                chilpa_i18n::mark("Confirm this preview and its removals before syncing."),
                "",
            ));
        }
        self.close_browser();
        let ready = self.ready.take().unwrap();
        let progress = self.progress.clone();
        self.launch("transferring", move |store, _, cancel| {
            work::apply(&store, ready, approve_removals, &cancel, &progress)?;
            Ok(Outcome::Applied)
        })
    }
    pub fn cancel(&mut self) {
        self.incoming_ids.clear();
        self.cancel.cancel();
        if self.pending.is_some() {
            self.status.phase = "cancelling".into();
        }
    }
    pub fn show_error(&mut self, error: UiError) {
        self.status.error = Some(error);
        self.revision += 1;
    }
    /// I release table/browser payloads; bounded profile drafts and the active task survive navigation.
    pub fn release_view(&mut self) {
        *self.review_result.lock().unwrap_or_else(|e| e.into_inner()) = (false, None);
        self.close_browser();
        self.labels = library::Labels::new();
        self.rows = Vec::new();
        self.options = Vec::new();
        self.available_options = Vec::new();
        self.view_loaded = false;
        if self.pending.is_none() || self.reading_playlist {
            self.ready = None;
            self.status.plan.clear();
            if let Some(previous) = &mut self.browse_previous {
                previous.plan.clear();
                if previous.phase == "review" { previous.phase = "idle".into(); }
            }
            if self.status.phase == "review" {
                self.status.phase = "idle".into();
            }
        }
        self.revision += 1;
    }
    fn align_options(&mut self) {
        self.align_destination();
        if !self.status.profile.is_empty() { self.drafts.attach(&self.status.profile); }
        let picks = self.profile().map(|p| p.picks.clone()).unwrap_or_default();
        self.status.remove_unselected = self.profile().is_some_and(|p| p.remove_unselected);
        self.options = self.available_options.clone();
        let ids: HashSet<_> = picks.iter().map(Pick::id).collect();
        for row in &mut self.options {
            row.checked = ids.contains(row.id.as_str());
        }
        for pick in picks {
            if let Some(row) = self.options.iter_mut().find(|r| r.id == pick.id()) {
                row.name = pick.name().into();
                if let Pick::Query { expression, .. } = &pick {
                    row.description = expression.clone();
                }
                row.pick = pick;
            } else {
                self.options.push(OptionRow {
                    id: pick.id().into(),
                    name: pick.name().into(),
                    description: if let Pick::Episode { episode, .. } = &pick { episode.show.clone() } else { chilpa_i18n::t("Saved device selection") },
                    checked: true,
                    pick,
                });
            }
        }
        for row in &mut self.options {
            if let Some((name, description)) = self
                .labels
                .get(&(self.status.profile.clone(), row.id.clone()))
            {
                row.name = name.clone();
                row.description = description.clone();
            }
        }
        self.drafts.apply(&self.status.profile, &mut self.options, &mut self.status.remove_unselected);
    }
    fn replace_profile(&mut self, profile: Profile) {
        if let Some(saved) = self
            .catalog
            .profiles
            .iter_mut()
            .find(|p| p.id == profile.id)
        {
            *saved = profile;
        }
    }
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        if let Ok(mut slot) = self.progress.lock() {
            if let Some(p) = slot.take() {
                self.status.completed = p.completed_files;
                self.status.total = p.total_files;
                self.status.transferred = super::bytes(p.transferred_bytes);
                self.status.current = self
                    .rows
                    .get(p.completed_files)
                    .filter(|r| r.path == p.current_path)
                    .or_else(|| {
                        self.rows
                            .get(p.completed_files.saturating_sub(1))
                            .filter(|r| r.path == p.current_path)
                    })
                    .map(|r| r.title.clone())
                    .unwrap_or_default();
                changed = true;
            }
        }
        let result = self.pending.as_ref().map(Receiver::try_recv);
        let outcome = match result {
            Some(Ok(value)) => Some(value),
            Some(Err(TryRecvError::Disconnected)) => Some(Err(failure(
                chilpa_i18n::mark("The device task stopped. Its progress is saved for recovery."),
                "",
            ))),
            _ => None,
        };
        if let Some(outcome) = outcome {
            let mut catalog_changed = true;
            self.pending = None;
            self.status.busy = false;
            self.status.error = None;
            match outcome {
                Ok(Outcome::Refreshed(catalog, destinations, options, labels, error)) => {
                    self.view_loaded = true;
                    self.catalog = catalog;
                    self.destinations = destinations;
                    self.available_options = options;
                    self.labels = labels;
                    if !self
                        .catalog
                        .profiles
                        .iter()
                        .any(|p| p.id == self.status.profile)
                    {
                        self.status.profile = self.store.active_profile().filter(|id| self.catalog.profiles.iter().any(|p| p.id == *id))
                            .or_else(|| self.catalog.profiles.first().map(|p| p.id.clone())).unwrap_or_default();
                    }
                    if !self
                        .destinations
                        .iter()
                        .any(|d| d.id == self.status.destination)
                    {
                        self.status.destination = self
                            .destinations
                            .first()
                            .map(|d| d.id.clone())
                            .unwrap_or_default();
                    }
                    self.align_options();
                    self.status.phase = "idle".into();
                    self.status.error = error;
                }
                Ok(Outcome::Connections(destinations, error, previous)) => {
                    let before = self.status.destination.clone();
                    catalog_changed = self.destinations.len() != destinations.len()
                        || self.destinations.iter().any(|row| !destinations.contains(row));
                    self.status = previous;
                    self.status.busy = false;
                    if catalog_changed {
                        self.destinations = destinations;
                        self.align_destination();
                        if self.status.destination != before { self.invalidate(); }
                    }
                    if error.is_some() { self.status.error = error; }
                }
                Ok(Outcome::Reopened(options, labels, destinations, error, previous)) => {
                    self.view_loaded = true;
                    self.available_options = options;
                    self.labels = labels;
                    self.destinations = destinations;
                    self.status = previous;
                    self.status.busy = false;
                    let before = self.status.destination.clone();
                    self.align_options();
                    if self.status.destination != before { self.invalidate(); }
                    if error.is_some() { self.status.error = error; }
                }
                Ok(Outcome::Added(profile)) => {
                    self.drafts.attach(&profile.id);
                    self.status.profile = profile.id.clone();
                    if let Some(destination) = self.destinations.iter_mut().find(|d| d.id == self.status.destination) {
                        destination.profiles.push(profile.id.clone());
                    }
                    self.catalog.profiles.push(profile);
                    self.align_options();
                    self.status.phase = "idle".into();
                }
                Ok(Outcome::Saved(profile)) => {
                    self.drafts.clear(&profile.id);
                    self.replace_profile(profile);
                    self.status.phase = "idle".into();
                }
                Ok(Outcome::Selected(mut row)) => {
                    if let Some(existing) =
                        self.options
                            .iter()
                            .find(|saved| match (&saved.pick, &row.pick) {
                                (Pick::Tracks { keys: a, .. }, Pick::Tracks { keys: b, .. }) => {
                                    a == b
                                }
                                _ => false,
                            })
                    {
                        row.id = existing.id.clone();
                        if let Pick::Tracks { id, .. } = &mut row.pick {
                            *id = row.id.clone();
                        }
                    }
                    match self.drafts.put(&self.status.profile, row.clone()) {
                        Err(error) => { self.status.phase = "error".into(); self.status.error = Some(error); }
                        Ok(()) => {
                            if self.view_loaded {
                                self.options.retain(|o| o.id != row.id);
                                self.options.insert(0, row);
                            }
                            self.status.phase = "idle".into();
                        }
                    }
                }
                Ok(Outcome::PlaylistRead(error)) => {
                    self.reading_playlist = false;
                    if let Some(previous) = self.browse_previous.take() {
                        self.status = previous;
                    }
                    let view = self.playlist_result.lock().unwrap_or_else(|e| e.into_inner()).take();
                    if self.view_loaded && !self.browse_source.is_empty() { self.browser = view; self.browser_error = error; }
                }
                Ok(Outcome::Browsed(result)) => {
                    if let Some(previous) = self.browse_previous.take() {
                        self.status = previous;
                    }
                    match result {
                        Ok(view) if self.view_loaded => self.browser = Some(view),
                        Err(_) if self.cancel.check().is_err() => {
                            self.browser_error = Some(failure(
                                chilpa_i18n::mark("Loading music was cancelled."),
                                "",
                            ))
                        }
                        Err(error) => self.browser_error = Some(error),
                        _ => (),
                    }
                }
                Ok(Outcome::Prepared(profile)) => {
                    self.drafts.clear(&profile.id);
                    self.replace_profile(profile);
                    let ready = self.review_result.lock().unwrap_or_else(|e| e.into_inner()).1.take();
                    let Some(ready) = ready else {
                        self.status.phase = "idle".into();
                        self.status.plan.clear();
                        self.revision += 1;
                        return true;
                    };
                    self.rows = work::rows(&ready.plan);
                    self.status.plan = ready.plan.id().into();
                    self.status.copies = ready
                        .plan
                        .actions()
                        .iter()
                        .filter(|a| {
                            matches!(
                                a,
                                chilpa_sync::Action::Copy {
                                    replaces: false,
                                    ..
                                }
                            )
                        })
                        .count();
                    self.status.updates = ready
                        .plan
                        .actions()
                        .iter()
                        .filter(|a| matches!(a, chilpa_sync::Action::Copy { replaces: true, .. }))
                        .count();
                    self.status.removals = ready.plan.deletion_count();
                    self.status.unchanged = ready.plan.unchanged_files();
                    self.status.required = super::bytes(ready.plan.required_bytes());
                    self.status.tracks = ready.tracks;
                    self.status.metadata_review = ready.plan.is_metadata_review();
                    if self.view_loaded {
                        self.ready = Some(ready);
                        self.status.phase = "review".into();
                    } else {
                        self.rows = Vec::new();
                        self.status.plan.clear();
                        self.status.phase = "idle".into();
                    }
                }
                Ok(Outcome::Applied) => {
                    self.status.completed = self.status.total;
                    self.status.phase = "done".into();
                    self.status.plan.clear();
                }
                Err(error) => {
                    self.reading_playlist = false;
                    self.browse_previous = None;
                    self.playlist_result.lock().unwrap_or_else(|e| e.into_inner()).take();
                    self.status.plan.clear();
                    self.status.phase = if self.cancel.check().is_err() {
                        "cancelled"
                    } else {
                        "error"
                    }
                    .into();
                    self.status.error = Some(error);
                }
            }
            if catalog_changed { self.revision += 1; }
            changed = true;
        }
        if self.pending.is_none() && self.view_loaded && self.status.error.is_none() {
            if let Some(ids) = self.incoming_ids.pop_front() {
                if let Err(error) = self.start_library_tracks(&ids) {
                    self.show_error(error);
                }
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stopped_playlist_worker_cannot_restore_a_busy_session_on_close() {
        let directory = tempfile::tempdir().unwrap();
        let mut service = Service::new(directory.path().join("library"), directory.path().join("profiles"));
        let (sender, receiver) = sync_channel(1);
        drop(sender);
        service.pending = Some(receiver);
        service.status.busy = true;
        service.status.phase = "browsing".into();
        service.browse_previous = Some(Status::default());
        service.reading_playlist = true;
        service.poll();
        assert!(service.status.error.is_some());
        service.close_browser();
        assert!(!service.status.busy, "a stopped metadata worker must release the session slot");
        assert!(service.idle().is_ok());
    }
}
