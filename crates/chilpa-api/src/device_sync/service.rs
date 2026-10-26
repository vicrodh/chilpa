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
use serde::Serialize;
use std::{
    collections::HashSet,
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
    Reopened(Vec<OptionRow>, library::Labels, Status),
    Browsed(Result<Browser, UiError>),
    Saved(Profile),
    Selected(OptionRow),
    Prepared(Ready),
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
    pending: Option<Receiver<Result<Outcome, UiError>>>,
    cancel: Cancellation,
    progress: Arc<Mutex<Option<chilpa_sync::Progress>>>,
    revision: u64,
    view_loaded: bool,
    incoming: Vec<OptionRow>,
    incoming_ids: Option<Vec<i64>>,
    labels: library::Labels,
    browser: Option<Browser>,
    browse_previous: Option<Status>,
    browse_source: String,
    browse_title: String,
    browser_error: Option<UiError>,
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
            pending: None,
            cancel: Cancellation::default(),
            progress: Arc::new(Mutex::new(None)),
            revision: 0,
            view_loaded: false,
            incoming: Vec::new(),
            incoming_ids: None,
            labels: library::Labels::new(),
            browser: None,
            browse_previous: None,
            browse_source: String::new(),
            browse_title: String::new(),
            browser_error: None,
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
    }
    fn launch(
        &mut self,
        phase: &str,
        job: impl FnOnce(Store, PathBuf, Cancellation) -> Result<Outcome, UiError> + Send + 'static,
    ) -> Result<(), UiError> {
        self.idle()?;
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
        self.poll();
        if self.pending.is_some() {
            return Ok(());
        }
        if self.incoming_ids.is_some() {
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
            Ok(Outcome::Reopened(options, labels, previous))
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
            let (destinations, error) = work::discover(&cancel);
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
            Ok(Outcome::Refreshed(
                catalog,
                vec![work::mounted()],
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
        row.checked = !row.checked;
        self.revision += 1;
        Ok(())
    }
    pub fn set_removals(&mut self, remove_unselected: bool) -> Result<(), UiError> {
        self.idle()?;
        if remove_unselected == self.status.remove_unselected {
            return Ok(());
        }
        self.invalidate();
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
        self.idle()?;
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
        self.incoming_ids = Some(ids);
        Ok(())
    }
    pub fn add_library_tracks(&mut self, ids: &[i64]) -> Result<(), UiError> {
        self.idle()?;
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
        self.launch("preparing", move |store, library, cancel| {
            let profile = work::save_selection(&store, &profile.id, picks, remove)?;
            Ok(Outcome::Prepared(work::prepare(
                &store,
                &library,
                profile,
                destination,
                recover,
                &cancel,
            )?))
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
        let ready = self.ready.take().unwrap();
        let progress = self.progress.clone();
        self.launch("transferring", move |store, _, cancel| {
            work::apply(&store, ready, approve_removals, &cancel, &progress)?;
            Ok(Outcome::Applied)
        })
    }
    pub fn cancel(&mut self) {
        self.cancel.cancel();
        if self.pending.is_some() {
            self.status.phase = "cancelling".into();
        }
    }
    pub fn show_error(&mut self, error: UiError) {
        self.status.error = Some(error);
        self.revision += 1;
    }
    /// Closing the surface releases row/query payloads; the bounded active task keeps running.
    pub fn release_view(&mut self) {
        self.close_browser();
        self.labels.clear();
        self.rows = Vec::new();
        self.options = Vec::new();
        self.available_options = Vec::new();
        self.view_loaded = false;
        self.incoming.clear();
        self.incoming_ids = None;
        if self.pending.is_none() {
            self.ready = None;
            self.status.plan.clear();
            if self.status.phase == "review" {
                self.status.phase = "idle".into();
            }
        }
        self.revision += 1;
    }
    fn align_options(&mut self) {
        if self
            .profile()
            .is_some_and(|profile| matches!(profile.root, Root::Mounted { .. }))
        {
            self.status.destination = "mounted".into();
        } else if self
            .profile()
            .is_some_and(|profile| matches!(profile.root, Root::Mtp { .. }))
            && self.status.destination == "mounted"
        {
            self.status.destination = self
                .destinations
                .iter()
                .find(|d| d.id != "mounted" && d.writable)
                .map(|d| d.id.clone())
                .unwrap_or_default();
        }
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
                    description: chilpa_i18n::t("Saved device selection"),
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
        for incoming in &self.incoming {
            self.options.retain(|row| row.id != incoming.id);
            self.options.insert(0, incoming.clone());
        }
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
        let mut resolve_incoming = false;
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
            self.pending = None;
            self.status.busy = false;
            self.status.error = None;
            match outcome {
                Ok(Outcome::Refreshed(catalog, destinations, options, labels, error)) => {
                    self.view_loaded = true;
                    resolve_incoming = true;
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
                        self.status.profile = self
                            .catalog
                            .profiles
                            .first()
                            .map(|p| p.id.clone())
                            .unwrap_or_default();
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
                Ok(Outcome::Reopened(options, labels, previous)) => {
                    self.view_loaded = true;
                    self.available_options = options;
                    self.labels = labels;
                    self.status = previous;
                    self.status.busy = false;
                    self.align_options();
                }
                Ok(Outcome::Added(profile)) => {
                    self.status.profile = profile.id.clone();
                    self.catalog.profiles.push(profile);
                    self.align_options();
                    self.status.phase = "idle".into();
                }
                Ok(Outcome::Saved(profile)) => {
                    self.incoming.clear();
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
                    let bytes: usize = self
                        .incoming
                        .iter()
                        .filter(|r| r.id != row.id)
                        .chain(std::iter::once(&row))
                        .map(|r| {
                            serde_json::to_vec(&r.pick)
                                .map(|b| b.len())
                                .unwrap_or(usize::MAX)
                        })
                        .fold(0, usize::saturating_add);
                    if bytes > 1024 * 1024
                        || self.incoming.len() >= 512
                            && !self.incoming.iter().any(|r| r.id == row.id)
                    {
                        self.status.phase = "error".into();
                        self.status.error = Some(failure(
                            chilpa_i18n::mark("Device profiles exceed 1 MiB."),
                            "",
                        ));
                    } else {
                        self.incoming.retain(|r| r.id != row.id);
                        self.incoming.push(row.clone());
                        self.options.retain(|o| o.id != row.id);
                        self.options.insert(0, row);
                        self.status.phase = "idle".into();
                    }
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
                Ok(Outcome::Prepared(ready)) => {
                    self.incoming.clear();
                    self.replace_profile(ready.profile.clone());
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
                    self.ready = Some(ready);
                    self.status.phase = "review".into();
                }
                Ok(Outcome::Applied) => {
                    self.status.completed = self.status.total;
                    self.status.phase = "done".into();
                    self.status.plan.clear();
                }
                Err(error) => {
                    self.status.plan.clear();
                    self.incoming_ids = None;
                    self.status.phase = if self.cancel.check().is_err() {
                        "cancelled"
                    } else {
                        "error"
                    }
                    .into();
                    self.status.error = Some(error);
                }
            }
            self.revision += 1;
            changed = true;
        }
        if resolve_incoming {
            if let Some(ids) = self.incoming_ids.take() {
                if let Err(error) = self.add_library_tracks(&ids) {
                    self.show_error(error);
                }
            }
        }
        changed
    }
}
