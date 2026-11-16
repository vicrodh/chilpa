//! File-scoped metadata editing. Idle sessions do no work. The worker owns
//! filesystem inspection and a private SQLite manifest (1 GiB plus journal).
//! RAM retains one row (256 KiB), one page (1 MiB), one command (1 MiB), a
//! 2 MiB SQLite cache and one file's tag parser/writer. Staging holds one media
//! copy and one sidecar (1 MiB). Traversal holds at most 4096 directory paths.
//! Release reconciliation adds one component (32768 tracks, 16 MiB text) and
//! indexed temporary SQLite tables (1 GiB). Its connection uses library pragmas.
mod candidate;
mod fields;
mod files;
mod grouping;
mod groups;
mod lookup;
mod model;
mod plans;
mod progress;
pub use progress::{MetadataBatchPhase, MetadataBatchProgress};
#[cfg(test)]
mod tests;
mod worker;
use crate::Chilpa;
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use worker::{Job, Worker};

const MAX_COMMAND: usize = 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Selection {
    mode: String,
    #[serde(default)]
    keys: Vec<String>,
    #[serde(default)]
    artists: Vec<String>,
    selected: bool,
    #[serde(default)]
    replace: bool,
    #[serde(default, rename = "groupKey")]
    group_key: Option<String>,
    #[serde(default)]
    groups: Vec<String>,
}

pub(crate) struct BatchEditor {
    session: u64,
    revision: u64,
    worker: Option<Worker>,
    view: Value,
    closing: bool,
    beets_view: Value,
    looking_up: bool,
    progress: MetadataBatchProgress,
}
impl Default for BatchEditor {
    fn default() -> Self {
        Self {
            session: 0,
            revision: 0,
            worker: None,
            closing: false,
            beets_view: Value::Null,
            looking_up: false,
            progress: MetadataBatchProgress::default(),
            view: json!({"session":0,"revision":0,"dataRevision":0,"state":"closed","busy":false,"dirty":false,"error":"",
            "counts":{"files":0,"groups":0,"selected":0},"summary":{"changed":0,"unchanged":0,"unsupported":0,"failed":0,"saved":0},"items":[]}),
        }
    }
}
impl BatchEditor {
    fn refuse(&mut self, message: String) -> bool {
        self.view["error"] = json!(message);
        self.changed();
        false
    }
    fn changed(&mut self) {
        self.revision += 1;
        self.view["revision"] = json!(self.revision);
    }
    fn busy(&self) -> bool {
        self.view["busy"].as_bool().unwrap_or(false)
    }
    fn ask(&mut self, job: Job) -> bool {
        if self.busy() || self.closing || self.view["state"] != "ready" {
            return self.refuse(chilpa_i18n::t(
                "Wait for the current metadata operation to finish.",
            ));
        }
        let Some(worker) = &self.worker else {
            return false;
        };
        worker.cancelled.store(false, Ordering::Release);
        let page = matches!(&job, Job::Page { .. } | Job::GroupPage { .. });
        if !worker.ask(job) {
            return self.refuse(chilpa_i18n::t("The metadata worker is not available."));
        }
        self.view["busy"] = json!(true);
        if !page {
            self.view["error"] = json!("");
        }
        self.changed();
        true
    }
    fn retire(&mut self) {
        if self.worker.as_ref().is_some_and(Worker::finished) {
            if let Some(value) = self.worker.as_ref().and_then(Worker::take) {
                self.view = value;
            }
            self.progress = self.worker.as_ref().unwrap().progress.read();
            self.worker.take();
            self.looking_up = false;
            self.view["busy"] = json!(false);
            if self.closing {
                self.view["state"] = json!("closed");
                self.view["items"] = json!([]);
                self.view["dirty"] = json!(false);
            }
            self.closing = false;
        }
    }
}

impl Chilpa {
    pub fn metadata_batch_progress(&self) -> MetadataBatchProgress {
        self.metadata_batch.worker.as_ref().map(|worker| worker.progress.read())
            .unwrap_or(self.metadata_batch.progress)
    }

    pub fn metadata_batch_session(&self) -> u64 {
        self.metadata_batch.session
    }

    pub fn metadata_batch_open(&mut self, paths_json: &str) -> bool {
        self.metadata_batch_poll();
        self.metadata_batch.retire();
        if self.metadata_batch.worker.is_some() {
            return self.metadata_batch.refuse(chilpa_i18n::t(
                "Close the current metadata selection before opening another.",
            ));
        }
        let paths = if paths_json.len() <= MAX_COMMAND {
            serde_json::from_str::<Vec<String>>(paths_json).ok()
        } else {
            None
        };
        let Some(paths) = paths.filter(|paths| !paths.is_empty() && paths.len() <= 256) else {
            return self
                .metadata_batch
                .refuse(chilpa_i18n::t("Choose between 1 and 256 folders or files."));
        };
        let paths: Option<Vec<PathBuf>> = paths
            .into_iter()
            .map(|path| {
                let path = if path.starts_with("file://") {
                    url::Url::parse(&path).ok()?.to_file_path().ok()?
                } else {
                    PathBuf::from(path)
                };
                (path.is_absolute() && path.as_os_str().len() <= 32768).then_some(path)
            })
            .collect();
        let Some(paths) = paths else {
            return self
                .metadata_batch
                .refuse(chilpa_i18n::t("Choose valid absolute file paths."));
        };
        let library = self
            .library
            .as_ref()
            .and_then(|library| library.connection().path())
            .map(PathBuf::from)
            .unwrap_or_else(chilpa_library::Library::default_path);
        let number = self.metadata_batch.session + 1;
        match Worker::start(number, paths, library) {
            Ok(worker) => {
                let editor = &mut self.metadata_batch;
                editor.session = number;
                editor.worker = Some(worker);
                editor.closing = false;
                editor.looking_up = false;
                editor.view = json!({"session":number,"dataRevision":0,"state":"opening","busy":true,"dirty":false,"error":"",
                    "counts":{"files":0,"groups":0,"selected":0},"summary":{"changed":0,"unchanged":0,"unsupported":0,"failed":0,"saved":0},"items":[]});
                editor.changed();
                self.metadata_batch_probe_beets();
                true
            }
            Err(error) => self.metadata_batch.refuse(error),
        }
    }

    pub fn metadata_batch_select(&mut self, selection_json: &str) -> bool {
        let selection = if selection_json.len() <= MAX_COMMAND {
            serde_json::from_str::<Selection>(selection_json).ok()
        } else {
            None
        };
        let prefix = format!("{}:", self.metadata_batch.session);
        let Some(selection) = selection.filter(|selection| {
            matches!(
                selection.mode.as_str(),
                "all" | "keys" | "artists" | "groups"
            ) && selection.groups.len() <= 1024
                && selection.groups.iter().all(|key| key.len() <= 256)
                && selection
                    .group_key
                    .as_ref()
                    .is_none_or(|key| key.len() <= 256)
                && selection.artists.len() <= 1024
                && selection.keys.len() <= 10000
                && selection.keys.iter().all(|key| {
                    key.strip_prefix(&prefix)
                        .is_some_and(|id| id.parse::<i64>().is_ok_and(|id| id > 0))
                })
        }) else {
            return self.metadata_batch.refuse(chilpa_i18n::t(
                "The metadata selection is invalid or belongs to another session.",
            ));
        };
        self.metadata_batch.ask(Job::Select(selection))
    }

    pub fn metadata_batch_patch(&mut self, patch_json: &str) -> bool {
        let patch = if patch_json.len() <= MAX_COMMAND {
            serde_json::from_str::<fields::Patch>(patch_json).ok()
        } else {
            None
        };
        let Some(patch) = patch else {
            return self
                .metadata_batch
                .refuse(chilpa_i18n::t("The metadata operation is invalid."));
        };
        if let Err(error) = patch.validate() {
            return self.metadata_batch.refuse(error);
        }
        self.metadata_batch.ask(Job::Patch(patch))
    }

    pub fn metadata_batch_page(
        &mut self,
        kind: &str,
        offset: usize,
        limit: usize,
        request_id: u64,
    ) -> bool {
        if !matches!(kind, "items" | "groups" | "plan") {
            return self
                .metadata_batch
                .refuse(chilpa_i18n::t("This metadata page is not supported."));
        }
        self.metadata_batch.ask(Job::Page {
            kind: kind.into(),
            offset,
            limit: limit.clamp(1, 100),
            request: request_id,
        })
    }

    pub fn metadata_batch_group_page(
        &mut self,
        group_key: &str,
        offset: usize,
        limit: usize,
        request_id: u64,
    ) -> bool {
        if group_key.len() > 256 {
            return self
                .metadata_batch
                .refuse(chilpa_i18n::t("This metadata group is invalid."));
        }
        self.metadata_batch.ask(Job::GroupPage {
            group: group_key.into(),
            offset,
            limit: limit.clamp(1, 100),
            request: request_id,
        })
    }

    pub fn metadata_batch_lookup_group(&mut self, group_key: &str) -> bool {
        let Some(runtime) = self.metadata_batch_beets_runtime() else {
            return self.metadata_batch.refuse(chilpa_i18n::t(
                "Check the beets environment in Settings before looking up metadata.",
            ));
        };
        if group_key.len() > 256 {
            return self
                .metadata_batch
                .refuse(chilpa_i18n::t("This metadata group is invalid."));
        }
        let permit = match lookup::Permit::acquire(self.metadata_beets_lease()) {
            Ok(permit) => permit,
            Err(error) => return self.metadata_batch.refuse(error),
        };
        let accepted = self.metadata_batch.ask(Job::Lookup {
            group: group_key.into(),
            runtime: runtime.into(),
            permit,
        });
        if accepted {
            self.metadata_batch.looking_up = true;
        }
        accepted
    }

    pub fn metadata_batch_candidate(&mut self, candidate_json: &str) -> bool {
        let request = if candidate_json.len() <= MAX_COMMAND {
            serde_json::from_str::<candidate::Request>(candidate_json).ok()
        } else {
            None
        };
        let Some(request) = request else {
            return self
                .metadata_batch
                .refuse(chilpa_i18n::t("This candidate proposal is invalid."));
        };
        self.metadata_batch.ask(Job::Candidate(request))
    }

    pub fn metadata_batch_group_plan(&mut self, plan_json: &str) -> bool {
        let request = if plan_json.len() <= MAX_COMMAND {
            serde_json::from_str::<grouping::Request>(plan_json).ok()
        } else {
            None
        };
        let Some(request) = request else {
            return self
                .metadata_batch
                .refuse(chilpa_i18n::t("This disc plan is invalid."));
        };
        self.metadata_batch.ask(Job::GroupPlan(request))
    }

    pub fn metadata_batch_apply_plan(&mut self, plan_id: &str) -> bool {
        if plan_id.len() > 256 {
            return self
                .metadata_batch
                .refuse(chilpa_i18n::t("This metadata proposal is invalid."));
        }
        self.metadata_batch.ask(Job::ApplyPlan(plan_id.into()))
    }

    pub(crate) fn metadata_batch_network_changed(&mut self) {
        if self.metadata_batch.looking_up {
            if let Some(worker) = &self.metadata_batch.worker {
                worker.cancelled.store(true, Ordering::Release);
            }
        }
    }

    pub fn metadata_batch_view(&self) -> Value {
        let mut view = self.metadata_batch.view.clone();
        view["fields"] = json!(fields::definitions());
        view["beets"] = self.metadata_batch_beets_status();
        view["busy"] = json!(self.metadata_batch.busy() || self.beets_runtime_busy());
        view["hardlinkPolicy"] = json!(chilpa_i18n::t(
            "Each selected hard-link path is replaced independently. Other hard-link paths keep their previous contents."
        ));
        view
    }

    pub fn metadata_batch_save(&mut self) -> bool {
        if !self.metadata_batch.view["dirty"].as_bool().unwrap_or(false) {
            return self
                .metadata_batch
                .refuse(chilpa_i18n::t("There are no selected changes to save."));
        }
        self.metadata_batch.ask(Job::Save)
    }
    pub fn metadata_batch_retry(&mut self) -> bool {
        self.metadata_batch_save()
    }

    pub fn metadata_batch_cancel(&mut self) {
        if self.metadata_batch.busy() {
            if let Some(worker) = &self.metadata_batch.worker {
                worker.cancelled.store(true, Ordering::Release);
            }
        } else {
            self.metadata_batch.ask(Job::Discard);
        }
    }
    pub fn metadata_batch_close(&mut self) {
        let editor = &mut self.metadata_batch;
        editor.looking_up = false;
        if let Some(worker) = &editor.worker {
            worker.close();
            editor.closing = true;
            editor.view["state"] = json!("closing");
            editor.view["busy"] = json!(true);
        }
        editor.changed();
    }
    pub(crate) fn advance_closed_metadata_batch(&mut self) {
        if self.metadata_batch.closing
            && self
                .metadata_batch
                .worker
                .as_ref()
                .is_some_and(Worker::finished)
        {
            self.metadata_batch_poll();
        }
    }

    pub fn metadata_batch_poll(&mut self) -> bool {
        self.advance_beets_runtime();
        let beets = self.metadata_batch_beets_status();
        let beets_changed = beets != self.metadata_batch.beets_view;
        self.metadata_batch.beets_view = beets;
        if beets_changed {
            self.metadata_batch.changed();
        }
        let finished_job = self
            .metadata_batch
            .worker
            .as_ref()
            .is_some_and(|worker| worker.finished() || worker.response_ready());
        let library_changed = finished_job
            && self
                .metadata_batch
                .worker
                .as_ref()
                .is_some_and(|worker| worker.library_changed.swap(false, Ordering::AcqRel));
        if library_changed {
            self.mark_library_changed();
        }
        let editor = &mut self.metadata_batch;
        let before = editor.revision;
        if let Some(mut view) = editor.worker.as_ref().and_then(Worker::take) {
            editor.looking_up = false;
            if view["pageOnly"].as_bool() == Some(true)
                && editor.view["error"]
                    .as_str()
                    .is_some_and(|error| !error.is_empty())
            {
                view["error"] = editor.view["error"].clone();
            }
            if !editor.closing && view["session"] == json!(editor.session) {
                editor.view = view;
                editor.changed();
            }
        }
        let held = editor.worker.is_some();
        editor.retire();
        if held && editor.worker.is_none() {
            editor.changed();
        }
        beets_changed || editor.revision != before
    }
}
