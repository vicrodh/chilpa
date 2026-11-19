//! A library older than this Chilpa is brought up on a thread of its own
//! while the startup cover says what is happening, and nothing in the window
//! reads or writes it before then. Until the upgrade ends the facade's
//! library is refused quietly, the services that read it have not started,
//! and the cover does not give up waiting.
//!
//! What it costs when nothing is pending: one read-only open and the file's
//! version, when the facade is built.

use chilpa_library::schema::upgrade::{self, Phase, Progress, Stop};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{mpsc, Arc, Mutex};

enum State {
    Current,
    Pending,
    Running,
    Failed { message: String, detail: String },
}

pub(crate) struct LibraryUpgrade {
    path: PathBuf,
    state: State,
    heard: Arc<Mutex<Option<Progress>>>,
    outcome: Option<mpsc::Receiver<Result<(), String>>>,
    stop: Arc<Stop>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl LibraryUpgrade {
    pub(crate) fn detect(path: PathBuf) -> Self {
        let state = if upgrade::pending(&path).is_some() { State::Pending } else { State::Current };
        Self { path, state, heard: Default::default(), outcome: None, stop: Default::default(), worker: None }
    }

    /// The library may not be opened: an upgrade waits, runs or failed.
    pub(crate) fn holds(&self) -> bool {
        !matches!(self.state, State::Current)
    }

    fn start(&mut self) {
        let (path, heard, stop) = (self.path.clone(), self.heard.clone(), self.stop.clone());
        *heard.lock().unwrap_or_else(|error| error.into_inner()) = None;
        let (tell, outcome) = mpsc::channel();
        let spawned = std::thread::Builder::new().name("chilpa-library-upgrade".into()).spawn(move || {
            let mut hear = |progress: &Progress| *heard.lock().unwrap_or_else(|error| error.into_inner()) = Some(*progress);
            let result = upgrade::run(&path, &mut hear, &stop);
            let _ = tell.send(result.map_err(|error| error.to_string()));
        });
        match spawned {
            Ok(worker) => {
                self.worker = Some(worker);
                self.outcome = Some(outcome);
                self.state = State::Running;
            }
            Err(error) => self.fail(error.to_string()),
        }
    }

    fn fail(&mut self, message: String) {
        let detail = match upgrade::unfinished_copy(&self.path) {
            Some(_) => chilpa_i18n::t_args(
                "Nothing was lost: the library is as it was before this step, and a copy from before the update is at {}.",
                &[&chilpa_library::schema::backup_path(&self.path).display().to_string()],
            ),
            None => chilpa_i18n::t("Nothing was lost: the library is as it was before this step."),
        };
        self.state = State::Failed { message, detail };
    }

    /// True once, when the upgrade has just ended well.
    pub(crate) fn advance(&mut self) -> bool {
        let Some(outcome) = &self.outcome else { return false };
        let result = match outcome.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return false,
            Err(mpsc::TryRecvError::Disconnected) => Err(String::new()),
        };
        self.outcome = None;
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        match result {
            Ok(()) => {
                self.state = State::Current;
                true
            }
            Err(message) => {
                self.fail(message);
                false
            }
        }
    }

    /// What the startup cover shows. Nothing when the library is current.
    pub(crate) fn view(&self) -> Value {
        match &self.state {
            State::Current => json!({ "active": false }),
            State::Failed { message, detail } => json!({
                "active": true,
                "failed": true,
                "title": chilpa_i18n::t("The library could not be updated"),
                "message": message,
                "detail": detail,
            }),
            State::Pending | State::Running => {
                let heard = *self.heard.lock().unwrap_or_else(|error| error.into_inner());
                let (text, progress) = said(heard.as_ref());
                json!({
                    "active": true,
                    "failed": false,
                    "text": text,
                    "progress": progress,
                    "note": chilpa_i18n::t("This happens once after an update and can take a few minutes with a large library. Chilpa opens by itself when it is done."),
                })
            }
        }
    }
}

/// The line under the bar and how full the bar is; below 0 while how far it
/// is cannot be told.
fn said(progress: Option<&Progress>) -> (String, f64) {
    let Some(progress) = progress else { return (chilpa_i18n::t("Updating the library…"), -1.0) };
    match progress.phase {
        Phase::WaitingForOther => (chilpa_i18n::t("Another Chilpa is updating this library. Waiting for it…"), -1.0),
        Phase::BackingUp => (chilpa_i18n::t("Saving a copy of the library first…"), progress.fraction),
        Phase::Step => {
            let (step, steps) = (progress.step.to_string(), progress.steps.to_string());
            (chilpa_i18n::t_args("Step {} of {}: {}", &[&step, &steps, &label(progress.version)]), progress.fraction)
        }
        Phase::Finishing => (chilpa_i18n::t("Checking the result…"), progress.fraction),
    }
}

/// What a step does, in words. Each one that takes long says it.
fn label(version: i64) -> String {
    match version {
        57 => chilpa_i18n::t("giving every track a number that is never used again"),
        _ => chilpa_i18n::t("changing how the library is stored"),
    }
}

impl Drop for LibraryUpgrade {
    /// A step is one transaction: asked to stop, the one under way rolls back
    /// and the library stays at the last version it completed.
    fn drop(&mut self) {
        self.stop.request();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl crate::Chilpa {
    /// Starts what reads the library: plugins, scrobbling, servers and the
    /// desktop's media controls. A library that has to be upgraded first is
    /// upgraded, and they start when it is done.
    pub fn start_services(&mut self) {
        if matches!(self.upgrade.state, State::Pending) {
            self.upgrade.start();
            return;
        }
        if self.upgrade.holds() { return; }
        self.start_plugins();
        self.start_scrobbling();
        // The listener's servers: their tracks play, sync and are watched (ADR-017).
        self.start_servers();
        self.start_media_controls();
    }

    /// The upgrade as the startup cover shows it, as JSON.
    pub fn library_upgrade_view(&mut self) -> String {
        self.advance_library_upgrade();
        self.upgrade.view().to_string()
    }

    /// Runs a failed upgrade again. Its copy from before is kept.
    pub fn retry_library_upgrade(&mut self) {
        if matches!(self.upgrade.state, State::Failed { .. }) {
            self.upgrade.start();
        }
    }

    /// Quits from the startup cover, without the window's question.
    pub fn leave_library_upgrade(&mut self) {
        if self.upgrade.holds() {
            self.desktop.requests.push_back(crate::WindowRequest::Quit);
        }
    }

    /// An upgrade that just ended well: what waited for the library starts,
    /// the tabs restored before it read their lists, and the window reads
    /// the library again.
    pub(crate) fn advance_library_upgrade(&mut self) {
        if !self.upgrade.advance() { return; }
        if let Some(tabs) = self.restored_tabs() {
            self.protect_tabs(&tabs);
        }
        self.start_services();
        self.mark_library_changed();
    }

    pub(crate) fn library_upgrading(&self) -> bool {
        self.upgrade.holds()
    }
}

#[cfg(test)]
pub(crate) mod tests;
