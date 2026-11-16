//! One worker, one pending command, one replaceable response, one private manifest.
use super::{
    Selection,
    fields::Patch,
    files,
    model::{MAX_PAGE_BYTES, Row},
};
use super::progress::{MetadataBatchPhase as Phase, Progress};
use chilpa_library::{Library, metadata_manifest::MetadataManifest};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, SyncSender},
};
use std::thread::JoinHandle;

pub(super) enum Job {
    Select(Selection),
    Patch(Patch),
    Page {
        kind: String,
        offset: usize,
        limit: usize,
        request: u64,
    },
    GroupPage {
        group: String,
        offset: usize,
        limit: usize,
        request: u64,
    },
    Lookup {
        group: String,
        runtime: PathBuf,
        permit: super::lookup::Permit,
    },
    Candidate(super::candidate::Request),
    GroupPlan(super::grouping::Request),
    ApplyPlan(String),
    Save,
    Discard,
    Close,
}

pub(super) struct Worker {
    jobs: SyncSender<Job>,
    pub cancelled: Arc<AtomicBool>,
    closing: Arc<AtomicBool>,
    pub library_changed: Arc<AtomicBool>,
    response: Arc<Mutex<Option<Value>>>,
    thread: JoinHandle<()>,
    pub progress: Progress,
}

fn failed_view(session: u64, error: String, mut outcome: Value) -> Value {
    outcome["session"] = json!(session);
    outcome["state"] = json!("failed");
    outcome["busy"] = json!(false);
    outcome["dirty"] = json!(false);
    outcome["error"] = json!(error);
    outcome["notice"] = json!(if outcome["writesOccurred"].as_bool() == Some(true) {
        chilpa_i18n::t_args(
            "Files were changed before this session stopped. {} file saves completed. Reopen the selection to review the remaining work.",
            &[&outcome["completedFileSaves"]
                .as_u64()
                .unwrap_or(0)
                .to_string()],
        )
    } else {
        chilpa_i18n::t("The metadata session stopped. Reopen the selection to review the files.")
    });
    outcome
}

impl Worker {
    pub fn start(session: u64, paths: Vec<PathBuf>, library: PathBuf) -> Result<Self, String> {
        let (jobs, asked) = mpsc::sync_channel(1);
        let progress = Progress::default();
        progress.begin(session, Phase::Opening);
        let worker_progress = progress.clone();
        let cancelled = Arc::new(AtomicBool::new(false));
        let closing = Arc::new(AtomicBool::new(false));
        let response = Arc::new(Mutex::new(None));
        let library_changed = Arc::new(AtomicBool::new(false));
        let changes = library_changed.clone();
        let outcome = Arc::new(Mutex::new(json!({"completedFileSaves":0,"items":[]})));
        let (cancel, closed, mailbox) = (cancelled.clone(), closing.clone(), response.clone());
        let thread = std::thread::Builder::new()
            .name("chilpa-metadata-batch".into())
            .spawn(move || {
                let result = run(
                    session,
                    paths,
                    library,
                    asked,
                    &cancel,
                    &closed,
                    &mailbox,
                    changes,
                    outcome.clone(),
                    worker_progress.clone(),
                );
                if let Err(error) = result {
                    worker_progress.finish(cancel.load(Ordering::Acquire), true);
                    let held = outcome
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .clone();
                    *mailbox.lock().unwrap_or_else(|error| error.into_inner()) =
                        Some(failed_view(session, error, held));
                }
            })
            .map_err(|error| error.to_string())?;
        Ok(Self {
            jobs,
            cancelled,
            closing,
            library_changed,
            response,
            thread,
            progress,
        })
    }
    pub fn ask(&self, job: Job) -> bool {
        self.jobs.try_send(job).is_ok()
    }
    pub fn take(&self) -> Option<Value> {
        self.response
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take()
    }
    pub fn response_ready(&self) -> bool {
        self.response
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .is_some()
    }
    pub fn finished(&self) -> bool {
        self.thread.is_finished()
    }
    pub fn close(&self) {
        self.closing.store(true, Ordering::Release);
        self.cancelled.store(true, Ordering::Release);
        let _ = self.jobs.try_send(Job::Close);
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.close();
    }
}

pub(super) struct Session {
    pub manifest: MetadataManifest,
    // Drop the connection before deleting its directory, including on Windows.
    _directory: tempfile::TempDir,
    pub number: u64,
    library_path: PathBuf,
    pub(super) revision: u64,
    error: String,
    notice: String,
    recovery: std::cell::RefCell<Option<(i64, Row)>>,
    outcome: Arc<Mutex<Value>>,
    library_changed: Arc<AtomicBool>,
    pub(super) progress: Progress,
}

impl Session {
    pub fn new(number: u64, library_path: PathBuf, cache: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(cache).map_err(|error| error.to_string())?;
        let directory = tempfile::Builder::new()
            .prefix("session-")
            .tempdir_in(cache)
            .map_err(|error| error.to_string())?;
        let manifest = MetadataManifest::open(&directory.path().join("selection.sqlite"))
            .map_err(|error| error.to_string())?;
        Ok(Self {
            manifest,
            _directory: directory,
            number,
            library_path,
            revision: 1,
            error: String::new(),
            notice: String::new(),
            recovery: std::cell::RefCell::new(None),
            outcome: Arc::new(Mutex::new(json!({"completedFileSaves":0,"items":[]}))),
            library_changed: Arc::new(AtomicBool::new(false)),
            progress: Progress::default(),
        })
    }

    pub(super) fn directory(&self) -> &Path {
        self._directory.path()
    }

    fn put(&self, id: i64, row: &Row) -> Result<(), String> {
        self.manifest
            .update(&row.entry(id)?)
            .map_err(|error| error.to_string())
    }

    fn record_saved_row(&self, id: i64, row: Row) -> Result<(), String> {
        {
            let mut outcome = self
                .outcome
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let key = format!("{}:{id}", self.number);
            let repeated =
                outcome["items"][0]["key"] == key && outcome["items"][0]["status"] == "saved";
            if row.status == "saved" && !repeated {
                outcome["completedFileSaves"] =
                    json!(outcome["completedFileSaves"].as_u64().unwrap_or(0) + 1);
            }
            if row.file_written || row.overlay_written {
                outcome["writesOccurred"] = json!(true);
                outcome["lastWrittenFile"] = json!({"path":super::lookup::summary_text(&row.path),"pathTruncated":row.path.chars().count()>256,"phase":row.phase,"fileWritten":row.file_written,"sidecarWritten":row.overlay_written});
            }
            outcome["items"] = json!([{"key":key,"path":super::lookup::summary_text(&row.path),"pathTruncated":row.path.chars().count()>256,"title":super::lookup::summary_text(&row.text("title")),
                "status":row.status,"phase":row.phase,"fileWritten":row.file_written,"sidecarWritten":row.overlay_written,
                "error":super::lookup::summary_text(&row.error),"changes":[]}]);
        }
        match self.put(id, &row) {
            Ok(()) => Ok(()),
            Err(error) => {
                *self.recovery.borrow_mut() = Some((id, row));
                Err(chilpa_i18n::t_args(
                    "The file save progressed, but its session record could not be stored. Retry to record the completed phases: {}",
                    &[&error],
                ))
            }
        }
    }

    fn recover_record(&self) -> Result<(), String> {
        let pending = self.recovery.borrow_mut().take();
        if let Some((id, row)) = pending {
            self.record_saved_row(id, row)?;
        }
        Ok(())
    }

    fn add(&self, row: &Row) -> Result<(), String> {
        if self.manifest.contains_path(&row.path).map_err(|error| error.to_string())? { return Ok(()); }
        self.manifest
            .insert(&row.entry(0)?)
            .map_err(|error| error.to_string())?;
        self.progress.update(|p| p.completed += 1);
        Ok(())
    }

    pub fn open(&self, paths: &[PathBuf], cancel: &AtomicBool) -> Result<(), String> {
        let library = if self.library_path.exists() {
            Some(Library::open_read_only(&self.library_path).map_err(|error| error.to_string())?)
        } else {
            None
        };
        for requested in paths {
            if cancel.load(Ordering::Acquire) {
                break;
            }
            let root = match std::fs::canonicalize(requested) {
                Ok(root) => root,
                Err(error) => {
                    self.add(&files::unsupported(requested, error.to_string()))?;
                    continue;
                }
            };
            if root.is_file() {
                self.inspect(&root, requested, library.as_ref())?;
                continue;
            }
            let mut pending = vec![(root.clone(), requested.clone(), 0usize)];
            while let Some((directory, logical, depth)) = pending.pop() {
                if cancel.load(Ordering::Acquire) {
                    break;
                }
                if !self
                    .manifest
                    .visit_directory(&directory.to_string_lossy())
                    .map_err(|error| error.to_string())?
                {
                    continue;
                }
                let entries = match std::fs::read_dir(&directory) {
                    Ok(entries) => entries,
                    Err(error) => {
                        self.add(&files::unsupported(&directory, error.to_string()))?;
                        continue;
                    }
                };
                for entry in entries {
                    if cancel.load(Ordering::Acquire) {
                        break;
                    }
                    let entry = match entry {
                        Ok(entry) => entry,
                        Err(error) => {
                            self.add(&files::unsupported(&directory, error.to_string()))?;
                            continue;
                        }
                    };
                    let path = entry.path();
                    let logical_path = logical.join(entry.file_name());
                    let metadata = match std::fs::symlink_metadata(&path) {
                        Ok(metadata) => metadata,
                        Err(error) => {
                            self.add(&files::unsupported(&path, error.to_string()))?;
                            continue;
                        }
                    };
                    if metadata.file_type().is_symlink() {
                        match std::fs::canonicalize(&path) {
                            Ok(target) if target.is_file() && target.starts_with(&root) => self.inspect(&target,&logical_path,library.as_ref())?,
                            _ => self.add(&files::unsupported(&path,chilpa_i18n::t("Nested directory links and links outside this folder are skipped. Select their target explicitly to edit it.")))?,
                        }
                    } else if metadata.is_dir() {
                        if depth >= 255 || pending.len() >= 4096 {
                            return Err(chilpa_i18n::t(
                                "This folder tree exceeds the batch traversal limit. Select a smaller part.",
                            ));
                        }
                        pending.push((path, logical_path, depth + 1));
                    } else if metadata.is_file() {
                        self.inspect(&path, &logical_path, library.as_ref())?;
                    }
                }
            }
        }
        Ok(())
    }

    fn inspect(
        &self,
        path: &Path,
        logical: &Path,
        library: Option<&Library>,
    ) -> Result<(), String> {
        if !crate::folders::is_audio(path) {
            return Ok(());
        }
        if self
            .manifest
            .contains_path(&path.to_string_lossy())
            .map_err(|error| error.to_string())?
        {
            return Ok(());
        }
        if path.to_str().is_none() {
            return self.add(&files::unsupported(
                path,
                chilpa_i18n::t("This file path cannot be represented in the metadata editor."),
            ));
        }
        let row = files::inspect(path, logical, library, &self.manifest)
            .unwrap_or_else(|error| files::unsupported(path, error));
        self.add(&row)
    }

    pub fn select(&self, selection: &Selection) -> Result<(), String> {
        self.recover_record()?;
        let scope = selection
            .group_key
            .as_deref()
            .map(|key| self.group(key))
            .transpose()?;
        let groups = selection
            .groups
            .iter()
            .map(|key| self.group(key))
            .collect::<Result<Vec<_>, _>>()?;
        if selection.mode == "keys" {
            for key in &selection.keys {
                let id = self.item_id(key)?;
                let entry = self
                    .manifest
                    .entry(id)
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| {
                        chilpa_i18n::t("The selected file is no longer in the current selection.")
                    })?;
                if scope.as_ref().is_some_and(|group| &entry.group != group) {
                    return Err(chilpa_i18n::t(
                        "The selected file belongs to another metadata group.",
                    ));
                }
            }
        }
        let keys: std::collections::BTreeSet<_> = selection
            .keys
            .iter()
            .map(|key| self.item_id(key))
            .collect::<Result<_, _>>()?;
        let artists: std::collections::HashSet<_> = selection
            .artists
            .iter()
            .map(|name| name.trim().to_lowercase())
            .collect();
        self.manifest.transaction(|| {
            let mut after = 0;
            while let Some(entry) = self
                .manifest
                .next(after)
                .map_err(|error| error.to_string())?
            {
                after = entry.id;
                if scope.as_ref().is_some_and(|group| &entry.group != group) {
                    continue;
                }
                let mut row = Row::read(&entry)?;
                let matched = match selection.mode.as_str() {
                    "all" => true,
                    "groups" => groups.contains(&entry.group),
                    "keys" => keys.contains(&entry.id),
                    "artists" => {
                        ["artist_credit", "album_artist"].iter().any(|field| {
                            row.effective
                                .get(*field)
                                .and_then(Value::as_str)
                                .is_some_and(|value| artists.contains(&value.trim().to_lowercase()))
                        }) || ["artists", "album_artists"].iter().any(|field| {
                            row.effective
                                .get(*field)
                                .and_then(Value::as_array)
                                .is_some_and(|values| {
                                    values
                                        .iter()
                                        .filter_map(Value::as_str)
                                        .any(|value| artists.contains(&value.trim().to_lowercase()))
                                })
                        })
                    }
                    _ => false,
                };
                let previous = row.selected;
                if matched {
                    row.selected = selection.selected;
                } else if selection.replace {
                    row.selected = false;
                }
                if row.selected != previous {
                    self.put(entry.id, &row)?;
                }
            }
            Ok(())
        })
    }

    pub fn patch(&self, patch: &Patch) -> Result<(), String> {
        self.recover_record()?;
        patch.validate()?;
        let scope = patch
            .group_key
            .as_deref()
            .map(|key| self.group(key))
            .transpose()?;
        self.manifest.transaction(|| {
            let mut after = 0;
            while let Some(entry) = self
                .manifest
                .next(after)
                .map_err(|error| error.to_string())?
            {
                after = entry.id;
                if entry.selected && scope.as_ref().is_none_or(|group| &entry.group == group) {
                    let mut row = Row::read(&entry)?;
                    row.patch(patch)?;
                    self.put(entry.id, &row)?;
                }
            }
            Ok(())
        })
    }

    fn discard(&self) -> Result<(), String> {
        self.manifest.transaction(|| {
            let mut after = 0;
            while let Some(entry) = self
                .manifest
                .next(after)
                .map_err(|error| error.to_string())?
            {
                after = entry.id;
                let mut row = Row::read(&entry)?;
                if !row.file_written && !row.overlay_written {
                    row.patches.clear();
                    row.membership = None;
                    if row.status != "unsupported" {
                        row.status = "unchanged".into();
                        row.error.clear();
                    }
                    self.put(entry.id, &row)?;
                }
            }
            Ok(())
        })
    }

    pub fn page(
        &self,
        kind: &str,
        offset: usize,
        limit: usize,
        request: u64,
    ) -> Result<Value, String> {
        self.filtered_page(kind, None, offset, limit, request)
    }

    pub fn filtered_page(
        &self,
        kind: &str,
        group: Option<&str>,
        offset: usize,
        limit: usize,
        request: u64,
    ) -> Result<Value, String> {
        let mut view = self.manifest.summary().map_err(|error| error.to_string())?;
        view["error"] = json!(self.error);
        view["notice"] = json!(self.notice);
        view["lookup"] = self.document("lookup")?;
        let mut plan = self.document("plan")?;
        if plan.is_object() && plan["baseDataRevision"].as_u64() != Some(self.revision) {
            plan["canApply"] = json!(false);
            plan["error"] = json!(chilpa_i18n::t(
                "The metadata selection changed. Prepare this proposal again."
            ));
        }
        view["plan"] = plan;
        let limit = limit.clamp(1, 100);
        let mut items = Vec::new();
        let mut bytes = 16 * 1024
            + serde_json::to_vec(&view)
                .map_err(|error| error.to_string())?
                .len();
        if bytes > 256 * 1024 {
            return Err(chilpa_i18n::t(
                "The metadata proposal summary exceeds the page budget.",
            ));
        }
        if kind == "plan" {
            let mut after = self
                .manifest
                .plan_start(offset)
                .map_err(|error| error.to_string())?;
            while items.len() < limit {
                let Some((id, body)) = self
                    .manifest
                    .next_plan_entry(after)
                    .map_err(|error| error.to_string())?
                else {
                    break;
                };
                let proposal = serde_json::from_str(&body).map_err(|error| error.to_string())?;
                let value = self.proposal_row(id, &proposal)?;
                let size = serde_json::to_vec(&value)
                    .map_err(|error| error.to_string())?
                    .len();
                if bytes + size > MAX_PAGE_BYTES {
                    break;
                }
                bytes += size;
                items.push(value);
                after = id;
            }
        } else if kind == "groups" {
            for index in offset..offset.saturating_add(limit) {
                let Some(value) = self
                    .manifest
                    .groups(index, 1)
                    .map_err(|error| error.to_string())?
                    .pop()
                else {
                    break;
                };
                let value = self.group_view(value)?;
                let size = serde_json::to_vec(&value)
                    .map_err(|error| error.to_string())?
                    .len();
                if bytes + size > MAX_PAGE_BYTES {
                    break;
                }
                bytes += size;
                items.push(value);
            }
        } else {
            let mut after = self
                .manifest
                .page_start_in_group(offset, group)
                .map_err(|error| error.to_string())?;
            while items.len() < limit {
                let Some(entry) = self
                    .manifest
                    .next_in_group(after, group)
                    .map_err(|error| error.to_string())?
                else {
                    break;
                };
                let recovery = self.recovery.borrow();
                let value =
                    if let Some((id, row)) = recovery.as_ref().filter(|(id, _)| *id == entry.id) {
                        row.view(self.number, *id)
                    } else {
                        Row::read(&entry)?.view(self.number, entry.id)
                    };
                let size = serde_json::to_vec(&value)
                    .map_err(|error| error.to_string())?
                    .len();
                if bytes + size > MAX_PAGE_BYTES {
                    break;
                }
                bytes += size;
                items.push(value);
                after = entry.id;
            }
        }
        let scope = self
            .manifest
            .scope_summary(group)
            .map_err(|error| error.to_string())?;
        let total = if kind == "plan" {
            self.manifest
                .plan_count()
                .map_err(|error| error.to_string())? as usize
        } else if group.is_some() {
            scope["files"].as_u64().unwrap_or(0) as usize
        } else {
            view["counts"][if kind == "groups" { "groups" } else { "files" }]
                .as_u64()
                .unwrap_or(0) as usize
        };
        view["scope"] = scope;
        view["total"] = json!(total);
        let next = offset.saturating_add(items.len());
        view["session"] = json!(self.number);
        view["dataRevision"] = json!(self.revision);
        view["state"] = json!("ready");
        view["busy"] = json!(false);
        view["dirty"] = json!(
            self.recovery.borrow().is_some()
                || view["summary"]["changed"].as_u64().unwrap_or(0) > 0
        );
        view["recoveryPending"] = json!(self.recovery.borrow().is_some());
        view["requestId"] = json!(request);
        view["pageIdentity"] = json!({"kind":kind,"offset":offset,"limit":limit});
        if let Some(group) = group {
            view["pageIdentity"]["groupKey"] = json!(format!("{}:{group}", self.number));
        }
        view["nextOffset"] = json!(next);
        view["hasMore"] = json!(next < total);
        view["items"] = Value::Array(items);
        Ok(view)
    }

    pub fn save(&self, cancel: &AtomicBool) -> Result<(), String> {
        self.recover_record()?;
        let total = self.manifest.save_target_count().map_err(|error| error.to_string())?;
        self.progress.update(|p| { p.total = total; p.total_known = true; });
        let mut library = None;
        let mut after = 0;
        let mut reconciled = false;
        while let Some(entry) = self
            .manifest
            .next(after)
            .map_err(|error| error.to_string())?
        {
            after = entry.id;
            if cancel.load(Ordering::Acquire) {
                break;
            }
            if !entry.selected || !entry.dirty || entry.state == "unsupported" {
                continue;
            }
            let mut row = Row::read(&entry)?;
            let result = (|| {
                if row.phase != "reconcile" {
                    files::save(&mut row, &self.manifest, cancel)?;
                }
                if row.phase != "reconcile" {
                    return Ok(());
                }
                if let Some(id) = row.track_id {
                    if library.is_none() {
                        library = Some(
                            Library::open_existing(&self.library_path)
                                .map_err(|error| error.to_string())?,
                        );
                    }
                    chilpa_library::scan::reconcile_metadata_file(
                        library.as_ref().unwrap(),
                        id,
                        &row.library_path,
                        Path::new(&row.path),
                    )?;
                    self.library_changed.store(true, Ordering::Release);
                    reconciled = true;
                    row.error.clear();
                } else {
                    row.complete()?;
                }
                Ok::<_, String>(())
            })();
            if let Err(error) = result {
                if row.status != "conflict" {
                    row.status = "failed".into();
                }
                row.error = error;
            }
            self.progress.update(|p| {
                p.completed += 1;
                p.saved += u64::from(row.status == "saved");
                p.failed += u64::from(!row.error.is_empty());
            });
            self.record_saved_row(entry.id, row)?;
        }
        if reconciled {
            self.progress.update(|p| p.phase = Phase::Finalizing);
            let result = library
                .as_ref()
                .unwrap()
                .assemble_metadata_releases()
                .map_err(|error| error.to_string());
            let mut after = 0;
            while let Some(entry) = self
                .manifest
                .next(after)
                .map_err(|error| error.to_string())?
            {
                after = entry.id;
                let mut row = Row::read(&entry)?;
                if row.phase == "reconcile" && row.error.is_empty() {
                    match &result {
                        Ok(()) => {
                            if let Err(error) = row.complete() {
                                row.status = "failed".into();
                                row.error = error;
                            }
                        }
                        Err(error) => {
                            row.status = "failed".into();
                            row.error = error.clone();
                        }
                    }
                    self.progress.update(|p| {
                        p.saved += u64::from(row.status == "saved");
                        p.failed += u64::from(!row.error.is_empty());
                    });
                    self.record_saved_row(entry.id, row)?;
                }
            }
        }
        Ok(())
    }
}

fn run(
    number: u64,
    paths: Vec<PathBuf>,
    library: PathBuf,
    asked: Receiver<Job>,
    cancel: &AtomicBool,
    closed: &AtomicBool,
    mailbox: &Mutex<Option<Value>>,
    library_changed: Arc<AtomicBool>,
    outcome: Arc<Mutex<Value>>,
    progress: Progress,
) -> Result<(), String> {
    let mut session = Session::new(
        number,
        library,
        &chilpa_dirs::cache().join("metadata-batch"),
    )?;
    session.library_changed = library_changed;
    session.outcome = outcome;
    session.progress = progress;
    session.open(&paths, cancel)?;
    session.progress.finish(cancel.load(Ordering::Acquire), false);
    let publish = |value| {
        *mailbox.lock().unwrap_or_else(|error| error.into_inner()) = Some(value);
    };
    if closed.load(Ordering::Acquire) {
        return Ok(());
    }
    if cancel.load(Ordering::Acquire) {
        session.notice = chilpa_i18n::t(
            "Folder reading was cancelled. Only the files already captured are shown.",
        );
    }
    publish(session.page("items", 0, 100, 0)?);
    loop {
        if closed.load(Ordering::Acquire) {
            break;
        }
        let Ok(job) = asked.recv() else { break };
        if matches!(&job, Job::Close) {
            break;
        }
        let mut page = ("items".to_string(), 0, 100, 0);
        let user_work = !matches!(&job, Job::Page { .. } | Job::GroupPage { .. });
        let mutation = matches!(
            &job,
            Job::Select(_) | Job::Patch(_) | Job::Save | Job::Discard | Job::ApplyPlan(_)
        );
        let mut group_page = None;
        let saving = matches!(&job, Job::Save);
        if user_work {
            session.error.clear();
            session.notice.clear();
        }
        if user_work {
            session.progress.begin(number, if saving { Phase::Saving } else { Phase::Working });
        }
        let result = match job {
            Job::Select(selection) => session.select(&selection),
            Job::Patch(patch) => session.patch(&patch),
            Job::Save => session.save(cancel),
            Job::Lookup {
                group,
                runtime,
                permit,
            } => {
                let result = session.lookup_group(&group, &runtime, cancel);
                drop(permit);
                result
            }
            Job::Candidate(request) => session.candidate(&request),
            Job::GroupPlan(request) => session.group_plan(&request, cancel),
            Job::ApplyPlan(id) => session.apply_plan(&id),
            Job::Page {
                kind,
                offset,
                limit,
                request,
            } => {
                page = (kind, offset, limit, request);
                Ok(())
            }
            Job::GroupPage {
                group,
                offset,
                limit,
                request,
            } => {
                page = ("items".into(), offset, limit, request);
                match session.group(&group) {
                    Ok(group) => {
                        group_page = Some(group);
                        Ok(())
                    }
                    Err(error) => Err(error),
                }
            }
            Job::Discard => session.discard(),
            Job::Close => break,
        };
        if user_work { session.progress.finish(cancel.load(Ordering::Acquire), result.is_err()); }
        if mutation && (result.is_ok() || saving) {
            session.revision += 1;
        }
        if let Err(error) = result {
            session.error = error;
        }
        if closed.load(Ordering::Acquire) {
            break;
        }
        let mut view =
            session.filtered_page(&page.0, group_page.as_deref(), page.1, page.2, page.3)?;
        view["pageOnly"] = json!(!user_work);
        if cancel.load(Ordering::Acquire) && user_work {
            session.notice = chilpa_i18n::t("Pending work was cancelled.");
        }
        view["notice"] = json!(session.notice);
        publish(view);
    }
    Ok(())
}

#[cfg(test)]
mod failure_tests {
    use super::*;
    #[test]
    fn fatal_publication_keeps_known_physical_outcomes_after_manifest_failure() {
        let directory = tempfile::tempdir().unwrap();
        let session =
            Session::new(3, directory.path().join("none.sqlite"), directory.path()).unwrap();
        let row = Row {
            path: "/fixture/one.flac".into(),
            status: "failed".into(),
            phase: "overlay".into(),
            file_written: true,
            error: "x".repeat(super::super::model::MAX_ROW_BYTES),
            ..Default::default()
        };
        assert!(session.record_saved_row(1, row).is_err());
        let outcome = session.outcome.lock().unwrap().clone();
        let view = failed_view(3, "manifest unavailable".into(), outcome);
        assert_eq!(view["state"], "failed");
        assert_eq!(view["writesOccurred"], true);
        assert_eq!(view["lastWrittenFile"]["fileWritten"], true);
        assert_eq!(view["lastWrittenFile"]["phase"], "overlay");
        assert_eq!(view["completedFileSaves"], 0);
        assert!(serde_json::to_vec(&view).unwrap().len() < 64 * 1024);
    }
}

#[cfg(test)]
mod page_budget_tests {
    use super::*;
    #[test]
    fn error_and_cancellation_notice_are_counted_before_page_rows() {
        let directory = tempfile::tempdir().unwrap();
        let mut session =
            Session::new(4, directory.path().join("none.sqlite"), directory.path()).unwrap();
        for index in 0..12 {
            let row = Row {
                path: format!("/fixture/{index}.flac"),
                group: "fixture".into(),
                status: "unchanged".into(),
                effective: json!({"title":"x".repeat(100_000)})
                    .as_object()
                    .unwrap()
                    .clone(),
                ..Default::default()
            };
            session.manifest.insert(&row.entry(0).unwrap()).unwrap();
        }
        session.error = "e".repeat(128 * 1024);
        session.notice = "n".repeat(64 * 1024);
        let page = session.page("items", 0, 100, 1).unwrap();
        assert!(serde_json::to_vec(&page).unwrap().len() <= MAX_PAGE_BYTES);
        assert_eq!(page["error"], session.error);
        assert_eq!(page["notice"], session.notice);
        assert!(page["hasMore"].as_bool().unwrap());
        let next = page["nextOffset"].as_u64().unwrap() as usize;
        assert!(next > 0);
        let continuation = session.page("items", next, 100, 2).unwrap();
        assert_eq!(continuation["dataRevision"], page["dataRevision"]);
        assert_ne!(continuation["items"][0]["key"], page["items"][0]["key"]);
    }
}
