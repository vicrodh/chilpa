//! Mirroring a server into the library, on a thread of its own (ADR-017 §5).
//!
//! One server at a time, in the order they were asked for. Every page lands
//! whole or not at all; what the server no longer has is taken out only
//! when the whole catalogue was read and the count adds up. A sync that is
//! cut off, or that saw fewer tracks than the server announced, leaves the
//! library as it was.

use super::keys::Keys;
use super::net::{self, Network};
use chilpa_servers::{Failure, Net};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

/// Cover size asked for: the window's largest.
const COVER: u32 = 1200;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Event {
    Progress { source: i64, done: u64 },
    /// The catalogue is in. `gone` tracks were taken out.
    Finished { source: i64, gone: usize },
    /// The covers of the records that had none are in.
    Covered { source: i64 },
    Failed { source: i64, why: String, unreachable: bool },
}

pub(crate) struct Syncer {
    orders: Sender<i64>,
    events: Receiver<Event>,
    /// Asked to stop: the source being removed.
    cancel: Arc<std::sync::Mutex<HashSet<i64>>>,
    pub(crate) running: HashMap<i64, u64>,
    queued: HashSet<i64>,
}

impl Syncer {
    pub(crate) fn start(keys: Arc<Keys>) -> Self {
        let (orders, inbox) = channel::<i64>();
        let (outbox, events) = channel::<Event>();
        let cancel: Arc<std::sync::Mutex<HashSet<i64>>> = Arc::default();
        let stop = Arc::clone(&cancel);
        let _ = std::thread::Builder::new().name("chilpa-server-sync".into()).spawn(move || {
            while let Ok(source) = inbox.recv() {
                let stopped = AtomicBool::new(false);
                let asked_to_stop = || {
                    let asked = stop.lock().map(|s| s.contains(&source)).unwrap_or(false);
                    stopped.store(asked, Ordering::Relaxed);
                    asked
                };
                let Ok(lib) = chilpa_core::open_library() else {
                    let _ = outbox.send(Event::Failed { source, why: chilpa_i18n::t("the library could not be opened"), unreachable: false });
                    continue;
                };
                let event = run(&lib, &Network, &keys, source, &|done| {
                    let _ = outbox.send(Event::Progress { source, done });
                }, &asked_to_stop);
                let finished = matches!(event, Event::Finished { .. });
                let _ = outbox.send(event);
                if finished && !asked_to_stop() {
                    covers(&lib, &Network, &keys, source, &asked_to_stop);
                    let _ = outbox.send(Event::Covered { source });
                }
                if let Ok(mut s) = stop.lock() {
                    s.remove(&source);
                }
            }
        });
        Self { orders, events, cancel, running: HashMap::new(), queued: HashSet::new() }
    }

    /// Queues a sync of that server, unless one is queued or running.
    pub(crate) fn ask(&mut self, source: i64) {
        if self.queued.insert(source) {
            self.running.insert(source, 0);
            let _ = self.orders.send(source);
        }
    }

    /// Stops the sync of a server that is being removed, at the next page.
    pub(crate) fn cancel(&mut self, source: i64) {
        if !self.queued.contains(&source) { return; }
        if let Ok(mut s) = self.cancel.lock() {
            s.insert(source);
        }
    }

    pub(crate) fn poll(&mut self) -> Vec<Event> {
        let events: Vec<Event> = self.events.try_iter().collect();
        for event in &events {
            match event {
                Event::Progress { source, done } => {
                    self.running.insert(*source, *done);
                }
                Event::Finished { source, .. } | Event::Failed { source, .. } => {
                    self.running.remove(source);
                    self.queued.remove(source);
                }
                Event::Covered { .. } => {}
            }
        }
        events
    }
}

/// Reads the whole catalogue of one server into the library.
pub(crate) fn run(
    lib: &chilpa_library::Library,
    net: &dyn Net,
    keys: &Keys,
    source: i64,
    progress: &dyn Fn(u64),
    stopped: &dyn Fn() -> bool,
) -> Event {
    let failed = |failure: Failure| Event::Failed { source, unreachable: failure.is_unreachable(), why: failure.to_string() };
    let Some(stored) = lib.server(source).ok().flatten() else {
        return Event::Failed { source, why: chilpa_i18n::t("that server is no longer in the library"), unreachable: false };
    };
    if !stored.enabled {
        return Event::Failed { source, why: chilpa_i18n::t("that server is disabled"), unreachable: false };
    }
    let Some(server) = net::server(&stored) else {
        return Event::Failed { source, why: chilpa_i18n::t("that server is no longer in the library"), unreachable: false };
    };
    let Some(key) = keys.get(&stored.instance) else {
        return Event::Failed { source, why: chilpa_i18n::t_args("sign in to {} again", &[&stored.label]), unreachable: false };
    };
    let Ok(generation) = lib.begin_server_sync(source) else {
        return Event::Failed { source, why: chilpa_i18n::t("the library could not be written"), unreachable: false };
    };
    let known = lib.server_revisions(source).unwrap_or_default();
    let mut seen: HashSet<String> = HashSet::new();
    // What was kept or saved in this sync: only that counts as read.
    let mut sealed: usize = 0;
    // Something listed could not be read; nothing is taken out.
    let mut partial = false;
    let mut announced: HashMap<String, u64> = HashMap::new();
    let mut cursor: Option<String> = None;
    loop {
        if stopped() {
            return Event::Failed { source, why: chilpa_i18n::t("the sync was stopped"), unreachable: false };
        }
        let page = match chilpa_servers::page(net, &server, &key, cursor.as_deref()) {
            Ok(page) => page,
            Err(failure) => return failed(failure),
        };
        if let Some(total) = page.total {
            announced.insert(page.scope.clone(), total);
        }
        partial |= page.partial;
        let (mut keep, mut save, mut ask) = (Vec::new(), Vec::new(), Vec::new());
        for track in page.tracks {
            let entry = net::entry(&stored, &track.id);
            if !seen.insert(entry.clone()) {
                continue;
            }
            let unchanged = track.revision.is_some() && known.get(&entry).is_some_and(|r| r == &track.revision);
            if unchanged {
                keep.push(entry);
            } else if track.complete {
                save.push(net::track(&stored, track));
            } else {
                ask.push(track.id);
            }
        }
        for batch in ask.chunks(chilpa_servers::DETAIL_BATCH) {
            match chilpa_servers::details(net, &server, &key, batch) {
                Ok(found) => {
                    // A track listed and then not described was deleted in
                    // between, or the answer was cut: either way this sync
                    // cannot tell what is gone.
                    let described: HashSet<&str> = found.iter().map(|t| t.id.as_str()).collect();
                    partial |= batch.iter().any(|id| !described.contains(id.as_str()));
                    save.extend(found.into_iter().filter(|t| batch.contains(&t.id)).map(|t| net::track(&stored, t)));
                }
                Err(failure) => return failed(failure),
            }
        }
        if lib.keep_server_tracks(source, generation, &keep).is_err() || lib.save_server_tracks(source, generation, &save).is_err() {
            return Event::Failed { source, why: chilpa_i18n::t("the library could not be written"), unreachable: false };
        }
        sealed += keep.len() + save.len();
        progress(seen.len() as u64);
        match page.next {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }
    // Paging without a tie-breaker can repeat a track and skip another. A
    // sync that kept fewer than the server has, or could not read all it
    // listed, takes nothing out. Subsonic says its total only when asked,
    // after the last page, so a record skipped between pages shows.
    let mut expected: u64 = announced.values().sum();
    match chilpa_servers::census(net, &server, &key) {
        Ok(Some(total)) => expected = expected.max(total),
        Ok(None) => {}
        Err(failure) => return failed(failure),
    }
    if partial {
        return Event::Failed {
            source,
            why: chilpa_i18n::t("the server did not describe every track it listed; nothing was taken out, sync again"),
            unreachable: false,
        };
    }
    if (sealed as u64) < expected {
        let (saw, total) = ((sealed as u64).to_string(), expected.to_string());
        return Event::Failed {
            source,
            why: chilpa_i18n::t_args("the server listed {} of its {} tracks; nothing was taken out, sync again", &[&saw, &total]),
            unreachable: false,
        };
    }
    match lib.finish_server_sync(source, generation) {
        Ok(gone) => {
            let _ = lib.set_source_reachable(source, true);
            Event::Finished { source, gone }
        }
        Err(_) => Event::Failed { source, why: chilpa_i18n::t("the library could not be written"), unreachable: false },
    }
}

/// Where a server's covers are kept: removing the server removes the folder.
pub(crate) fn covers_dir(instance: &str) -> std::path::PathBuf {
    chilpa_dirs::cache().join("servers").join(instance)
}

/// Fetches the covers of the server's records that have none yet.
pub(crate) fn covers(lib: &chilpa_library::Library, net: &dyn Net, keys: &Keys, source: i64, stopped: &dyn Fn() -> bool) {
    let Some(stored) = lib.server(source).ok().flatten() else { return };
    let (Some(server), Some(key)) = (net::server(&stored), keys.get(&stored.instance)) else { return };
    let dir = covers_dir(&stored.instance);
    let _ = std::fs::create_dir_all(&dir);
    for (album, cover) in lib.pending_server_covers(source, 100_000).unwrap_or_default() {
        if stopped() {
            return;
        }
        let request = chilpa_servers::cover(&server, &key, &cover, COVER);
        let saved = match net.send(&request) {
            Ok(reply) if reply.status == 200 => {
                let kind = reply.headers.get("content-type").cloned().unwrap_or_default();
                let ext = if kind.contains("png") { "png" } else if kind.starts_with("image/") { "jpg" } else { "" };
                let path = dir.join(format!("{album}.{ext}"));
                (!ext.is_empty() && std::fs::write(&path, &reply.body).is_ok()).then(|| path.display().to_string())
            }
            // No cover, or the server does not answer: asked again next sync
            // only if it does not answer.
            Ok(_) => None,
            Err(_) => continue,
        };
        let _ = lib.set_server_cover(album, saved.as_deref());
    }
}

#[cfg(test)]
mod tests;
