//! Changes to a server that write the library, away from the window: its
//! removal, and turning it off or on (ADR-017). A sync may be writing the
//! library at the time; the window never waits for it.
//!
//! A removal stops the server's sync first and waits until its thread lets
//! go, then takes the server's tracks out a part at a time, each part in a
//! short transaction, then the rest. A file marks it until it ends, so a
//! removal cut off by the app closing goes on at the next start. A write
//! that finds the library busy is tried again a few times, further apart
//! each time; one that still fails is said, with what to do.

use super::keys::Keys;
use super::sync;
use crate::Chilpa;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::time::{Duration, Instant};

/// Tracks taken out in one transaction.
const PART: usize = 500;

/// How long a removal waits for the server's sync to stop. Past it the
/// removal goes on, and its writes wait their turn at the library.
const STOP_WAIT: Duration = Duration::from_secs(30);

/// The waits between tries of a write that finds the library busy.
pub(crate) const WAITS: [Duration; 5] = [
    Duration::from_millis(500),
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(4),
    Duration::from_secs(8),
];

/// Why a change was not made, as the window says it: what failed, what to
/// do about it, and the library's own words, to copy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Refusal {
    pub(crate) title: String,
    pub(crate) advice: String,
    pub(crate) detail: String,
}

impl Refusal {
    /// What a library error says, under `title`: `busy` when another
    /// writer held the library (`chilpa_library::is_busy`).
    fn of(title: String, busy: bool, detail: String) -> Self {
        let advice = if busy {
            chilpa_i18n::t("The library stayed busy with another task. Nothing more was taken out; try again in a moment.")
        } else {
            chilpa_i18n::t("The library could not be changed. Try again; the detail below says what happened.")
        };
        Self { title, advice, detail }
    }

    /// `of`, for a library error.
    fn library(title: String, error: &impl std::fmt::Display, busy: bool) -> Self {
        Self::of(title, busy, error.to_string())
    }

    /// One line for a notice, when no dialog waits on it.
    fn said(&self) -> String {
        format!("{}. {}", self.title, self.advice)
    }
}

pub(crate) enum Stage {
    /// Its sync is asked to stop, since then.
    Stopping(Instant),
    /// Its rows are being taken out on a thread.
    Taking(Receiver<Result<(), Refusal>>),
    Done,
    Failed(Refusal),
}

/// A server being removed.
pub(crate) struct Removing {
    pub(crate) id: i64,
    pub(crate) label: String,
    instance: String,
    pub(crate) stage: Stage,
    /// Asked from the window, which shows how it goes. One resumed at
    /// startup says it in a notice.
    asked: bool,
}

impl Removing {
    /// Still under way: another removal waits for it.
    pub(crate) fn running(&self) -> bool {
        matches!(self.stage, Stage::Stopping(_) | Stage::Taking(_))
    }

    /// As the window reads it in the setup's state.
    pub(crate) fn json(&self) -> serde_json::Value {
        let (state, refusal) = match &self.stage {
            Stage::Stopping(_) | Stage::Taking(_) => ("removing", None),
            Stage::Done => ("done", None),
            Stage::Failed(refusal) => ("failed", Some(refusal)),
        };
        serde_json::json!({
            "id": self.id, "name": self.label, "state": state,
            "title": refusal.map(|r| r.title.clone()).unwrap_or_default(),
            "advice": refusal.map(|r| r.advice.clone()).unwrap_or_default(),
            "detail": refusal.map(|r| r.detail.clone()).unwrap_or_default(),
        })
    }
}

/// A server being turned off or on.
pub(crate) struct Toggling {
    id: i64,
    enabled: bool,
    done: Receiver<Result<bool, Refusal>>,
}

impl Chilpa {
    /// Removes a server and everything that came from it, away from the
    /// window, which follows it in the setup's state (`removing`). False
    /// when another removal is under way or the server is not there.
    pub fn remove_server(&mut self, id: i64) -> bool {
        if self.servers.removing.as_ref().is_some_and(Removing::running) {
            eprintln!("chilpa-servers: removal of source {id} not begun: another removal is under way");
            return false;
        }
        let Some(stored) = self.library().and_then(|lib| lib.server(id).ok().flatten()) else { return false };
        self.begin_removal(id, stored.label, stored.instance, stored.provider, true);
        true
    }

    /// The server being removed, while its removal is under way.
    pub(crate) fn removing_server(&self, id: i64) -> bool {
        self.servers.removing.as_ref().is_some_and(|r| r.id == id && r.running())
    }

    /// The window has read how the last removal went.
    pub(crate) fn forget_removal(&mut self) {
        if !self.servers.removing.as_ref().is_some_and(Removing::running) {
            self.servers.removing = None;
        }
    }

    fn begin_removal(&mut self, id: i64, label: String, instance: String, provider: String, asked: bool) {
        if let Some(syncer) = self.servers.syncer.as_mut() {
            syncer.cancel(id);
        }
        // Nothing of it plays from here on.
        let entry = chilpa_models::RemoteEntry::new(chilpa_models::SourceId::new(provider.clone(), instance.clone()), "x").to_entry();
        let prefix = entry.trim_end_matches('x').to_string();
        self.purge_queue(|path| path.starts_with(&prefix));
        if let Err(e) = mark(id, &label, &instance, &provider) {
            eprintln!("chilpa-servers: removal of source {id} is not marked to go on after a restart: {e}");
        }
        eprintln!("chilpa-servers: removing source {id} ({label}); its sync is stopped first");
        self.servers.removing = Some(Removing { id, label, instance, stage: Stage::Stopping(Instant::now()), asked });
    }

    /// Moves the removal on, and picks up a server turned off or on. `poll`
    /// calls it.
    pub(crate) fn poll_background(&mut self) {
        self.poll_toggling();
        let Some(mut removing) = self.servers.removing.take() else {
            // A removal cut off by the app closing goes on.
            if let Some((id, label, instance, provider)) = self.servers.resumed.pop() {
                self.begin_removal(id, label, instance, provider, false);
            }
            return;
        };
        // Taken out, so `removing` stays whole for what follows.
        let stage = std::mem::replace(&mut removing.stage, Stage::Done);
        removing.stage = match stage {
            Stage::Stopping(since) => {
                let syncing = self.servers.syncer.as_ref().is_some_and(|s| s.has(removing.id));
                if syncing && since.elapsed() < STOP_WAIT {
                    Stage::Stopping(since)
                } else {
                    if syncing {
                        eprintln!("chilpa-servers: the sync of source {} did not stop in {}s; its removal goes on", removing.id, STOP_WAIT.as_secs());
                    }
                    self.start_taking(&removing)
                }
            }
            Stage::Taking(done) => match done.try_recv() {
                Err(TryRecvError::Empty) => Stage::Taking(done),
                Ok(Ok(())) => {
                    self.removed(&removing);
                    Stage::Done
                }
                Ok(Err(refusal)) => Stage::Failed(refusal),
                Err(TryRecvError::Disconnected) => Stage::Failed(Refusal {
                    title: chilpa_i18n::t_args("{} could not be removed", &[&removing.label]),
                    advice: chilpa_i18n::t("The library could not be changed. Try again; the detail below says what happened."),
                    detail: chilpa_i18n::t("the removal stopped on an error"),
                }),
            },
            other => other,
        };
        if let Stage::Failed(refusal) = &removing.stage {
            eprintln!("chilpa-servers: removal of source {} failed: {} ({})", removing.id, refusal.title, refusal.detail);
            self.servers.changed = true;
        }
        // One nobody waits on says how it went, and is not kept.
        if !removing.asked && !removing.running() {
            if let Stage::Failed(refusal) = &removing.stage {
                self.now.error = refusal.said();
            }
            return;
        }
        self.servers.removing = Some(removing);
    }

    fn start_taking(&mut self, removing: &Removing) -> Stage {
        let (tx, done) = channel();
        let (path, keys) = (chilpa_library::Library::default_path(), self.servers.keys.clone());
        let (id, instance, label) = (removing.id, removing.instance.clone(), removing.label.clone());
        let spawned = std::thread::Builder::new().name("chilpa-server-remove".into()).spawn(move || {
            let _ = tx.send(take(&path, keys.as_deref(), id, &instance, &label, &WAITS));
        });
        match spawned {
            Ok(_) => Stage::Taking(done),
            Err(e) => Stage::Failed(Refusal {
                title: chilpa_i18n::t_args("{} could not be removed", &[&removing.label]),
                advice: chilpa_i18n::t("The library could not be changed. Try again; the detail below says what happened."),
                detail: e.to_string(),
            }),
        }
    }

    /// What follows a removal in the window.
    fn removed(&mut self, removing: &Removing) {
        let id = removing.id;
        self.servers.errors.remove(&id);
        self.servers.refused.remove(&id);
        self.servers.playlist_errors.remove(&id);
        self.servers.answers.remove(&id);
        self.servers.changed = true;
        self.persist_queue_session();
        self.servers.notice.0 += 1;
        self.servers.notice.1 = chilpa_i18n::t_args("{} was removed", &[&removing.label]);
        eprintln!("chilpa-servers: source {id} ({}) removed", removing.label);
    }

    /// Turns a server off, keeping its catalogue and sign-in, or on again,
    /// away from the window. False when it is not there or one is being
    /// turned already.
    pub fn set_server_enabled(&mut self, id: i64, enabled: bool) -> bool {
        if self.servers.toggling.is_some() || self.removing_server(id) {
            return false;
        }
        let Some(server) = self.library().and_then(|lib| lib.server(id).ok().flatten()) else { return false };
        if server.enabled == enabled {
            return false;
        }
        if !enabled {
            let playing = chilpa_models::RemoteEntry::parse(&self.now.path)
                .is_some_and(|entry| entry.source.provider == server.provider && entry.source.instance == server.instance);
            if playing {
                self.stop();
            }
            if let Some(syncer) = self.servers.syncer.as_mut() {
                syncer.cancel(id);
            }
        }
        let (tx, done) = channel();
        let (path, label) = (chilpa_library::Library::default_path(), server.label.clone());
        let spawned = std::thread::Builder::new().name("chilpa-server-toggle".into()).spawn(move || {
            let turned = chilpa_library::Library::open_existing(&path)
                .and_then(|lib| retried("turning a server off or on", &WAITS, chilpa_library::is_busy, || lib.set_server_enabled(id, enabled)));
            let title = if enabled {
                chilpa_i18n::t_args("{} could not be turned on", &[&label])
            } else {
                chilpa_i18n::t_args("{} could not be turned off", &[&label])
            };
            let _ = tx.send(turned.map_err(|e| Refusal::library(title, &e, chilpa_library::is_busy(&e))));
        });
        if spawned.is_err() {
            return false;
        }
        self.servers.toggling = Some(Toggling { id, enabled, done });
        true
    }

    fn poll_toggling(&mut self) {
        let Some(toggling) = self.servers.toggling.as_ref() else { return };
        let turned = match toggling.done.try_recv() {
            Ok(turned) => turned,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Ok(false),
        };
        let Some(toggling) = self.servers.toggling.take() else { return };
        match turned {
            Ok(true) => {
                if toggling.enabled {
                    if let Some(watcher) = &self.servers.watcher {
                        watcher.ask();
                    }
                }
                self.servers.errors.remove(&toggling.id);
                self.servers.refused.remove(&toggling.id);
                self.servers.playlist_errors.remove(&toggling.id);
                self.servers.changed = true;
            }
            Ok(false) => {}
            Err(refusal) => self.now.error = refusal.said(),
        }
    }
}

/// Takes a server out of the library, on its own thread: its credential
/// first (if it cannot be deleted nothing is touched), then it is turned off
/// so none of it shows, then its tracks a part at a time, then the rest,
/// then its covers. Each write that finds the library busy waits `waits`
/// between tries.
pub(crate) fn take(path: &Path, keys: Option<&Keys>, id: i64, instance: &str, label: &str, waits: &[Duration]) -> Result<(), Refusal> {
    let title = || chilpa_i18n::t_args("{} could not be removed", &[label]);
    let refused = |e: &_| Refusal::library(title(), e, chilpa_library::is_busy(e));
    let lib = chilpa_library::Library::open_existing(path).map_err(|e| refused(&e))?;
    if keys.is_some_and(|keys| !keys.forget(instance)) {
        unmark(id);
        return Err(Refusal {
            title: title(),
            advice: chilpa_i18n::t_args("the credential of {} could not be deleted; nothing was removed", &[label]),
            detail: String::new(),
        });
    }
    retried("turning a server off to remove it", waits, chilpa_library::is_busy, || lib.set_server_enabled(id, false)).map_err(|e| refused(&e))?;
    let mut taken = 0;
    loop {
        let part = retried("taking a server's tracks out", waits, chilpa_library::is_busy, || lib.remove_source_tracks(id, PART)).map_err(|e| refused(&e))?;
        if part == 0 {
            break;
        }
        taken += part;
    }
    let chosen = lib.overlay_covers_dir(id);
    retried("taking the rest of a server out", waits, chilpa_library::is_busy, || lib.remove_source(id)).map_err(|e| refused(&e))?;
    for covers in std::iter::once(sync::covers_dir(instance)).chain(chosen) {
        if let Err(e) = std::fs::remove_dir_all(&covers) {
            if e.kind() != std::io::ErrorKind::NotFound {
                eprintln!("chilpa-servers: the covers of source {id} could not be deleted from {}: {e}", covers.display());
            }
        }
    }
    unmark(id);
    eprintln!("chilpa-servers: source {id} taken out: {taken} tracks in parts of {PART}, then the rest");
    Ok(())
}

/// Runs a write, again after each of `waits` while `busy` says the library
/// is held by another writer (`chilpa_library::is_busy`). Any other error,
/// or still busy after the last, is returned, and written to the log.
pub(crate) fn retried<T, E: std::fmt::Display>(
    what: &str,
    waits: &[Duration],
    busy: impl Fn(&E) -> bool,
    mut write: impl FnMut() -> Result<T, E>,
) -> Result<T, E> {
    let mut tries = 0;
    loop {
        match write() {
            Ok(value) => return Ok(value),
            Err(e) if busy(&e) && tries < waits.len() => {
                eprintln!("chilpa-servers: {what}: the library is busy; tried again in {:?}", waits[tries]);
                std::thread::sleep(waits[tries]);
                tries += 1;
            }
            Err(e) => {
                eprintln!("chilpa-servers: {what} failed after {} tries: {e}", tries + 1);
                return Err(e);
            }
        }
    }
}

/// Where unfinished removals are marked, one file per source.
fn marks() -> PathBuf {
    chilpa_dirs::data().join("servers").join("removing")
}

fn mark(id: i64, label: &str, instance: &str, provider: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(marks())?;
    let said = serde_json::json!({"label": label, "instance": instance, "provider": provider});
    std::fs::write(marks().join(id.to_string()), said.to_string())
}

fn unmark(id: i64) {
    let _ = std::fs::remove_file(marks().join(id.to_string()));
}

/// The removals a closed app left unfinished: id, label, instance and
/// provider of each.
pub(crate) fn unfinished() -> Vec<(i64, String, String, String)> {
    let Ok(entries) = std::fs::read_dir(marks()) else { return Vec::new() };
    entries
        .flatten()
        .filter_map(|entry| {
            let id: i64 = entry.file_name().to_str()?.parse().ok()?;
            let said: serde_json::Value = serde_json::from_slice(&std::fs::read(entry.path()).ok()?).ok()?;
            let text = |key: &str| said.get(key).and_then(|v| v.as_str()).unwrap_or_default().to_string();
            Some((id, text("label"), text("instance"), text("provider")))
        })
        .collect()
}

#[cfg(test)]
mod tests;
