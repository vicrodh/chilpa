//! What a track row shows of its offline copy, wherever the row is: one
//! snapshot the window keeps between polls, cheap to clone (its sets are
//! shared) and read once per visible row by library track id, so a row
//! that scrolls away and comes back finds its state where it left it.
//!
//! Kept and failed copies come from the library (at most `MAX_LISTED` of
//! each) and are read again only when a copy changed or the window asks;
//! what waits, runs or failed this session comes from the worker's queue.

use super::worker::{ids, queue};
use crate::Chilpa;
use chilpa_library::store::offline::Marked;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(Clone, Debug, Default, PartialEq)]
struct Running {
    id: i64,
    phase: &'static str,
    bytes: u64,
    total: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct Line {
    kind: String,
    ids: Vec<i64>,
    total: usize,
    settled: usize,
    current: bool,
}

/// The offline state of every track a row can ask about.
#[derive(Clone, Debug, Default)]
pub struct OfflineMarks {
    kept: Arc<HashSet<i64>>,
    /// Failed copies in the library, with why.
    stored: Arc<HashMap<i64, String>>,
    /// Failures of this session, with why; they win over the library.
    failures: Arc<HashMap<i64, String>>,
    queued: Arc<HashSet<i64>>,
    running: Option<Running>,
    lines: Arc<Vec<Line>>,
    active: bool,
}

impl OfflineMarks {
    /// `queued`, `downloading`, `verifying`, `removing`, `failed`, `ready`,
    /// or empty when the track has nothing offline.
    pub fn state(&self, id: i64) -> &'static str {
        if id <= 0 {
            return "";
        }
        if let Some(running) = self.running.as_ref().filter(|running| running.id == id) {
            return running.phase;
        }
        if self.queued.contains(&id) {
            "queued"
        } else if self.failures.contains_key(&id) {
            "failed"
        } else if self.kept.contains(&id) {
            "ready"
        } else if self.stored.contains_key(&id) {
            "failed"
        } else {
            ""
        }
    }

    /// How much of the track's file has arrived, 0 to 1, or -1 while its
    /// size is unknown or nothing arrives.
    pub fn fraction(&self, id: i64) -> f64 {
        match &self.running {
            Some(running) if running.id == id && running.phase == "downloading" && running.total > 0 => {
                (running.bytes as f64 / running.total as f64).clamp(0.0, 1.0)
            }
            _ => -1.0,
        }
    }

    /// Why the track's copy failed, or empty.
    pub fn error(&self, id: i64) -> String {
        if self.state(id) != "failed" {
            return String::new();
        }
        self.failures.get(&id).or_else(|| self.stored.get(&id)).cloned().unwrap_or_default()
    }

    /// What the row's menu button says of the track besides "More", or
    /// empty when there is nothing offline to say.
    pub fn tip(&self, id: i64) -> String {
        match self.state(id) {
            "ready" => chilpa_i18n::t("Available offline"),
            "queued" => chilpa_i18n::t("Waiting in the offline queue"),
            "downloading" => chilpa_i18n::t("Downloading for offline listening"),
            "verifying" => chilpa_i18n::t("Verifying downloaded audio…"),
            "removing" => chilpa_i18n::t("Removing offline copy"),
            "failed" => Some(self.error(id)).filter(|error| !error.is_empty()).unwrap_or_else(|| chilpa_i18n::t("Offline download failed")),
            _ => String::new(),
        }
    }

    /// How far the last selection of `kind` that is still going has got,
    /// 0 to 1, or -1 when none is going. `selected` names it by its ids
    /// (`[17]` for a record); empty takes the last one of the kind.
    pub fn job(&self, kind: &str, selected: &str) -> f64 {
        let wanted = ids(selected);
        let Some(line) = self.lines.iter().rev().find(|line| line.kind == kind && (selected.is_empty() || line.ids == wanted)) else {
            return -1.0;
        };
        if line.settled >= line.total || line.total == 0 {
            return -1.0;
        }
        let part = match &self.running {
            Some(running) if line.current && running.total > 0 => (running.bytes as f64 / running.total as f64).clamp(0.0, 1.0),
            _ => 0.0,
        };
        ((line.settled as f64 + part) / line.total as f64).clamp(0.0, 1.0)
    }

    /// Whether anything waits or runs: the window polls only then.
    pub fn active(&self) -> bool {
        self.active
    }

    /// A row's menu: what the selection's tracks can do offline now.
    pub(super) fn entries(&self, tracks: &[i64]) -> Vec<Value> {
        if tracks.is_empty() {
            return Vec::new();
        }
        let count = |test: &dyn Fn(i64) -> bool| tracks.iter().filter(|id| test(**id)).count();
        let busy = count(&|id| matches!(self.state(id), "queued" | "downloading" | "verifying" | "removing"));
        if busy > 0 {
            return vec![json!({"text": chilpa_i18n::t("Cancel download"), "act": "offline:cancel_selection"})];
        }
        let kept = count(&|id| self.kept.contains(&id));
        let failed = count(&|id| self.failures.contains_key(&id) || self.stored.contains_key(&id));
        let stored = count(&|id| self.kept.contains(&id) || self.stored.contains_key(&id));
        let mut out = vec![if kept == tracks.len() {
            json!({"text": chilpa_i18n::t("Re-download"), "act": "offline:redownload"})
        } else if failed > 0 && kept == 0 {
            json!({"text": chilpa_i18n::t("Retry download"), "act": "offline:start"})
        } else {
            json!({"text": chilpa_i18n::t("Make available offline"), "act": "offline:start"})
        }];
        if stored > 0 {
            out.push(json!({"text": chilpa_i18n::tn("Remove offline copy", "Remove offline copies", stored as i64), "act": "offline:remove_now"}));
        }
        out
    }
}

/// The library's part of the marks, read again only when it changed.
#[derive(Default)]
pub(super) struct Cache {
    kept: Arc<HashSet<i64>>,
    stored: Arc<HashMap<i64, String>>,
    queued: (u64, Arc<HashSet<i64>>, Arc<HashMap<i64, String>>),
    read: bool,
}

pub(super) fn split(marks: Vec<Marked>) -> (HashSet<i64>, HashMap<i64, String>) {
    let mut kept = HashSet::new();
    let mut stored = HashMap::new();
    for mark in marks {
        if mark.ready {
            kept.insert(mark.track);
        } else {
            stored.insert(mark.track, mark.error);
        }
    }
    (kept, stored)
}

pub(super) fn snapshot(cache: &mut Cache, tracker: &queue::Tracker) -> OfflineMarks {
    if cache.queued.0 != tracker.revision || !cache.read {
        cache.queued = (tracker.revision, Arc::new(tracker.queued()), Arc::new(tracker.failures.clone()));
    }
    let running = tracker.current.as_ref().filter(|current| current.id > 0).map(|current| Running {
        id: current.id,
        phase: current.phase,
        bytes: current.bytes,
        total: current.total,
    });
    let seq = tracker.current.as_ref().map(|current| current.seq);
    let lines = tracker
        .jobs
        .iter()
        .filter(|count| count.settled < count.total)
        .map(|count| Line { kind: count.kind.clone(), ids: count.ids.clone(), total: count.total, settled: count.settled, current: seq == Some(count.seq) })
        .collect();
    OfflineMarks {
        kept: cache.kept.clone(),
        stored: cache.stored.clone(),
        failures: cache.queued.2.clone(),
        queued: cache.queued.1.clone(),
        running,
        lines: Arc::new(lines),
        active: tracker.busy(),
    }
}

impl Chilpa {
    /// The marks as they are now. `reread` reads the library's copies
    /// again, for a library that changed underneath.
    pub fn offline_marks(&mut self, reread: bool) -> OfflineMarks {
        self.poll_server_offline();
        let stale = std::mem::take(&mut self.servers.offline.stale) || reread || !self.servers.offline_page.marks.read;
        if stale {
            let marks = self.library().and_then(|lib| lib.offline_marks().ok()).unwrap_or_default();
            let (kept, stored) = split(marks);
            let cache = &mut self.servers.offline_page.marks;
            cache.kept = Arc::new(kept);
            cache.stored = Arc::new(stored);
            cache.read = true;
        }
        let tracker = queue::lock(&self.servers.offline.tracker);
        snapshot(&mut self.servers.offline_page.marks, &tracker)
    }

    /// A menu's offline entries for `kind` (`tracks`, `album`, `playlist`)
    /// and its ids, as JSON: `[{text, act}]`, empty for local music.
    pub(super) fn server_offline_menu(&mut self, kind: &str, selected: &str) -> String {
        let tracks: Vec<i64> = self
            .library()
            .and_then(|lib| lib.offline_selection(kind, &ids(selected)).ok())
            .unwrap_or_default()
            .iter()
            .map(|track| track.id)
            .collect();
        if tracks.is_empty() {
            return "[]".into();
        }
        Value::Array(self.offline_marks(false).entries(&tracks)).to_string()
    }
}

#[cfg(test)]
mod tests;
