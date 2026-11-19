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
    /// The copy from before is being put back, on the worker thread.
    Restoring,
    /// `can_retry` is false while the library is missing, waiting for its
    /// copy: running the upgrade then would create an empty one.
    Failed { title: String, message: String, detail: String, can_restore: bool, can_retry: bool },
}

pub(crate) struct LibraryUpgrade {
    path: PathBuf,
    state: State,
    heard: Arc<Mutex<Option<Progress>>>,
    outcome: Option<mpsc::Receiver<Result<(), String>>>,
    stop: Arc<Stop>,
    worker: Option<std::thread::JoinHandle<()>>,
    /// The start's update check, asked for while the library was upgraded.
    pub(crate) update_check_waits: bool,
    /// The copy is back: the window quits.
    restored: bool,
}

impl LibraryUpgrade {
    pub(crate) fn detect(path: PathBuf) -> Self {
        let state = if upgrade::pending(&path).is_some() { State::Pending } else {
            upgrade::forget_finished(&path);
            State::Current
        };
        let mut upgrade = Self { path, state, heard: Default::default(), outcome: None, stop: Default::default(), worker: None, update_check_waits: false, restored: false };
        // Putting the copy back was cut off with the library moved aside: it
        // is finished from here, and nothing creates a library in its place.
        if upgrade::interrupted(&upgrade.path) {
            upgrade.state = State::Failed {
                title: chilpa_i18n::t("The library could not be updated"),
                message: chilpa_i18n::t("Putting the copy back did not finish."),
                detail: upgrade.copy_is_at(),
                can_restore: upgrade::restorable(&upgrade.path),
                can_retry: false,
            };
        }
        upgrade
    }

    fn copy_is_at(&self) -> String {
        chilpa_i18n::t_args("The copy from before the update is at {}.", &[&chilpa_library::schema::backup_path(&self.path).display().to_string()])
    }

    /// Putting the copy back failed. It stops before the library moves, so
    /// the library is as it was, unless it is missing.
    fn fail_restore(&mut self, message: String) {
        let present = self.path.exists();
        let detail = if present {
            chilpa_i18n::t_args("The library was not changed, and the copy from before the update is still at {}.",
                &[&chilpa_library::schema::backup_path(&self.path).display().to_string()])
        } else { self.copy_is_at() };
        self.state = State::Failed {
            title: chilpa_i18n::t("The library could not be updated"),
            message, detail, can_restore: upgrade::restorable(&self.path), can_retry: present,
        };
    }

    /// Puts the copy back on a thread of its own, as the upgrade runs: it
    /// copies a whole library and the window goes on drawing meanwhile.
    fn start_restore(&mut self) {
        let path = self.path.clone();
        let (tell, outcome) = mpsc::channel();
        let spawned = std::thread::Builder::new().name("chilpa-library-restore".into()).spawn(move || {
            let _ = tell.send(upgrade::restore(&path).map_err(|error| error.to_string()));
        });
        match spawned {
            Ok(worker) => {
                self.worker = Some(worker);
                self.outcome = Some(outcome);
                self.state = State::Restoring;
            }
            Err(error) => self.fail_restore(chilpa_i18n::t_args("The copy could not be put back: {}", &[&error.to_string()])),
        }
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
        // Every step committed and what failed came after: the open that
        // runs the repairs, or removing the marker. Trying again repeats
        // only that.
        if upgrade::current(&self.path) {
            self.state = State::Failed {
                title: chilpa_i18n::t("The library was updated, but the check after it failed"),
                message, detail: String::new(), can_restore: false, can_retry: true,
            };
            return;
        }
        let detail = match upgrade::unfinished_copy(&self.path) {
            Some(_) => chilpa_i18n::t_args(
                "Nothing was lost: the library is as it was before this step, and a copy from before the update is at {}.",
                &[&chilpa_library::schema::backup_path(&self.path).display().to_string()],
            ),
            None => chilpa_i18n::t("Nothing was lost: the library is as it was before this step."),
        };
        self.state = State::Failed {
            title: chilpa_i18n::t("The library could not be updated"),
            message, detail, can_restore: upgrade::restorable(&self.path), can_retry: true,
        };
    }

    /// True once, when the upgrade has just ended well.
    pub(crate) fn advance(&mut self) -> bool {
        let Some(outcome) = &self.outcome else { return false };
        let result = match outcome.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return false,
            // The thread ended without an answer: it panicked.
            Err(mpsc::TryRecvError::Disconnected) => Err(chilpa_i18n::t("The library update stopped unexpectedly.")),
        };
        self.outcome = None;
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        if matches!(self.state, State::Restoring) {
            match result {
                Ok(()) => self.restored = true,
                Err(message) => self.fail_restore(chilpa_i18n::t_args("The copy could not be put back: {}", &[&message])),
            }
            return false;
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
            State::Failed { title, message, detail, can_restore, can_retry } => json!({
                "active": true,
                "failed": true,
                "title": title,
                "message": message,
                "detail": detail,
                "canRestore": can_restore,
                "canRetry": can_retry,
            }),
            State::Restoring => json!({
                "active": true,
                "failed": false,
                "text": chilpa_i18n::t("Putting the copy back…"),
                "progress": -1.0,
                "note": "",
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
        // The copy says nothing of how far it is until it ends.
        Phase::BackingUp => (chilpa_i18n::t("Saving a copy of the library first…"), -1.0),
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
        if matches!(self.upgrade.state, State::Failed { can_retry: true, .. }) {
            self.upgrade.start();
        }
    }

    /// Puts the copy from before a failed upgrade back and quits. The next
    /// start upgrades the library again from that copy.
    pub fn restore_library_copy(&mut self) {
        if !matches!(self.upgrade.state, State::Failed { can_restore: true, .. }) { return; }
        self.upgrade.start_restore();
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
        let upgraded = self.upgrade.advance();
        if std::mem::take(&mut self.upgrade.restored) {
            self.desktop.requests.push_back(crate::WindowRequest::Quit);
        }
        if !upgraded { return; }
        if let Some(tabs) = self.restored_tabs() {
            self.protect_tabs(&tabs);
        }
        self.start_services();
        self.mark_library_changed();
        if std::mem::take(&mut self.upgrade.update_check_waits) {
            self.appimage_updates_check(true);
        }
    }

    pub(crate) fn library_upgrading(&self) -> bool {
        self.upgrade.holds()
    }
}

#[cfg(test)]
pub(crate) mod tests;
