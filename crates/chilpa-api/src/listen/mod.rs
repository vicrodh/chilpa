//! Named listening buffers share the one live queue and output.
mod storage;
mod view;
mod operations;

use crate::{Chilpa, NowPlaying, QueueGroup, QueuePlacement, Repeat};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Buffer {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub renamed: bool,
    pub epoch: u64,
    pub paths: Vec<String>,
    #[serde(default)]
    pub origins: Vec<Option<crate::queue_origin::QueueOrigin>>,
    pub removed: Vec<bool>,
    pub placement: Vec<QueuePlacement>,
    pub groups: Vec<QueueGroup>,
    pub order: Vec<usize>,
    pub cursor: usize,
    pub seconds: f64,
    pub shuffle: bool,
    pub repeat: u8,
    pub stop_after: Option<usize>,
}
impl Buffer {
    fn empty(id: u64) -> Self {
        Self { id, name: chilpa_i18n::t("Queue"), renamed: false, epoch: 1,
            paths: Vec::new(), origins: Vec::new(), removed: Vec::new(), placement: Vec::new(), groups: Vec::new(),
            order: Vec::new(), cursor: 0, seconds: 0.0, shuffle: false, repeat: 0, stop_after: None }
    }
    pub(crate) fn key(&self, index: usize) -> String { format!("{}:{}:{index}", self.id, self.epoch) }
    fn live(&self, index: usize) -> bool {
        index < self.paths.len() && !self.removed.get(index).copied().unwrap_or(false)
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Workspace {
    version: u8,
    pub buffers: Vec<Buffer>,
    pub active: u64,
    pub selected: u64,
    pub(crate) next: u64,
    #[serde(skip)]
    pub pending: Option<(u64, usize, f64)>,
    #[serde(skip)]
    pub(crate) undo: Option<Undo>,
    #[serde(skip)]
    pub saved_at: Option<std::time::Instant>,
    /// How long each track of a queue lasts, by its index, as its rows were
    /// last listed whole: what the Queues screen's footer adds up.
    #[serde(skip)]
    pub lengths: Option<(u64, std::collections::HashMap<usize, u64>)>,
}

#[derive(Clone)]
pub(crate) struct Undo {
    originals: Vec<(usize, Buffer)>,
    created: Vec<u64>,
    selected: u64,
}
impl Default for Workspace {
    fn default() -> Self {
        Self { version: 1, buffers: vec![Buffer::empty(1)], active: 1, selected: 1,
            next: 2, pending: None, undo: None, saved_at: None, lengths: None }
    }
}

impl Chilpa {
    pub(crate) fn full_queue_order(&self) -> Vec<usize> {
        let order = if self.casting.active() { self.casting.order.clone() }
            else if let Some(player) = self.player.as_ref().filter(|p| !p.idle()) {
                player.order().into_iter().map(|i| i + self.base).collect()
            } else {
                self.listen.buffers.iter().find(|b| b.id == self.listen.active)
                    .map(|b| b.order.clone()).unwrap_or_default()
            };
        if order.len() == self.queue.len() { order } else { (0..self.queue.len()).collect() }
    }
    fn buffer_snapshot(&self, id: u64) -> Option<Buffer> {
        let mut b = self.listen.buffers.iter().find(|b| b.id == id)?.clone();
        if id == self.listen.active {
            b.paths = self.queue.clone(); b.origins = self.queue_origins.clone(); b.removed = self.removed.clone();
            b.placement = self.queue_placement.clone(); b.groups = self.queue_groups.clone();
            b.order = self.full_queue_order(); b.cursor = self.now.index;
            b.seconds = self.now.seconds.max(0.0); b.shuffle = self.shuffle;
            b.repeat = self.repeat as u8; b.stop_after = self.stop_after;
        }
        Some(b)
    }
    fn capture_buffer(&mut self) {
        if let Some(b) = self.buffer_snapshot(self.listen.active) { self.store_buffer(b); }
    }
    fn store_buffer(&mut self, buffer: Buffer) {
        if let Some(at) = self.listen.buffers.iter_mut().find(|b| b.id == buffer.id) { *at = buffer; }
    }
    pub(crate) fn before_queue_replace(&mut self) {
        self.capture_buffer();
        let media = self.media.as_ref().map(|m| m.buffer);
        if self.listen.active == 0 || Some(self.listen.active) == media {
            self.listen.active = self.queue_for_playback(media);
            self.listen.selected = self.listen.active;
        }
        self.listen.pending = None;
        if let Some(at) = self.listen.buffers.iter().position(|b| b.id == self.listen.active) {
            let b = &mut self.listen.buffers[at];
            self.listen.undo = Some(Undo { originals: vec![(at, b.clone())], created: Vec::new(), selected: b.id });
            b.epoch = b.epoch.wrapping_add(1); b.order.clear(); b.paths.clear(); b.origins.clear();
        }
        if let Some(p) = &self.player { p.clear_order(); }
    }
    /// Where playback goes when nothing is playing, or when what plays is
    /// the opened media's queue: the queue in view, else the first that is
    /// not the media's. A queue is made only when the + asks for one, and
    /// here only when the media's is all there is.
    fn queue_for_playback(&mut self, media: Option<u64>) -> u64 {
        let usable = |id: u64| Some(id) != media;
        if usable(self.listen.selected) && self.listen.buffers.iter().any(|b| b.id == self.listen.selected) {
            return self.listen.selected;
        }
        match self.listen.buffers.iter().find(|b| usable(b.id)) {
            Some(b) => b.id,
            None => self.listen_create("", &[]),
        }
    }
    fn install_buffer(&mut self, b: &Buffer) {
        self.listen.active = b.id;
        self.queue = b.paths.clone(); self.queue_origins = b.origins.clone(); self.queue_origins.resize(self.queue.len(), None); self.removed = b.removed.clone();
        self.queue_placement = b.placement.clone(); self.queue_groups = b.groups.clone();
        self.next_queue_group = b.groups.iter().map(|g| g.key).max().unwrap_or(0) + 1;
        self.base = 0; self.shuffle = b.shuffle;
        self.repeat = match b.repeat { 1 => Repeat::All, 2 => Repeat::One, _ => Repeat::Off };
        self.stop_after = b.stop_after; self.mix = false;
        self.casting.order = b.order.clone();
        if let Some(p) = &self.player { p.set_order(b.order.clone(), b.paths.len()); }
        let cursor = b.cursor.min(b.paths.len().saturating_sub(1));
        let path = b.paths.get(cursor).cloned().unwrap_or_default();
        self.now = NowPlaying { path, index: cursor, seconds: b.seconds, queue_len: b.paths.len(),
            playing: b.live(cursor), paused: true, ..Default::default() };
        self.describe_waiting();
        self.restored_at = Some(b.seconds);
    }
    /// The chosen buffer becomes the single queue shown in the side panel.
    pub fn listen_play(&mut self, id: u64, key: &str) -> bool {
        self.capture_buffer();
        let Some(b) = self.buffer_snapshot(id) else { return false };
        let at = if key.is_empty() { b.cursor } else {
            let Some(at) = b.order.iter().copied().find(|&i| b.key(i) == key) else { return false };
            at
        };
        if !b.live(at) { return false; }
        let seconds = if key.is_empty() { b.seconds } else { 0.0 };
        self.listen.pending = Some((id, at, seconds));
        // I wait for the existing worker's stop barrier without blocking Qt.
        if let Some(p) = &self.player { p.stop_for_handoff(); }
        self.relaunching = None;
        self.finish_buffer_switch();
        true
    }
    pub(crate) fn finish_buffer_switch(&mut self) {
        if self.listen.pending.is_none() || self.casting.disconnecting { return; }
        if self.player.as_ref().is_some_and(|p| !p.idle()) { return; }
        let Some((id, index, seconds)) = self.listen.pending.take() else { return };
        if let Some(p) = &self.player { let _ = p.poll(); }
        self.log_play(true);
        self.capture_buffer();
        let Some(mut b) = self.buffer_snapshot(id) else { return };
        if !b.live(index) { return; }
        b.cursor = index; b.seconds = seconds;
        self.store_buffer(b.clone()); self.install_buffer(&b);
        self.restored_at = None;
        let (device, _) = self.chosen_output();
        self.spawn_from(index, &device, false, Some(std::time::Duration::from_secs_f64(seconds)), crate::Relaunch::Track);
        self.persist_buffers();
    }
    pub fn listen_create(&mut self, name: &str, paths: &[String]) -> u64 {
        let id = self.listen.next;
        self.listen.next += 1;
        let mut b = Buffer::empty(id);
        if !name.trim().is_empty() { b.name = name.trim().to_string(); b.renamed = true; }
        else if !self.listen.buffers.is_empty() {
            let mut number = 2;
            loop {
                b.name = chilpa_i18n::t_args("Queue {}", &[&number.to_string()]);
                if !self.listen.buffers.iter().any(|other| other.name == b.name) { break; }
                number += 1;
            }
        }
        b.paths = paths.iter().filter(|p| !p.is_empty()).cloned().collect();
        b.origins = vec![self.browse_origin.clone(); b.paths.len()];
        b.order = (0..b.paths.len()).collect();
        b.removed = vec![false; b.paths.len()];
        b.placement = vec![QueuePlacement::Original; b.paths.len()];
        self.listen.buffers.push(b); self.listen.selected = id;
        self.persist_buffers(); id
    }
    pub fn listen_select(&mut self, id: u64) {
        if self.listen.buffers.iter().any(|b| b.id == id) { self.listen.selected = id; self.persist_buffers(); }
    }
    pub fn listen_rename(&mut self, id: u64, name: &str) {
        if name.trim().is_empty() { return; }
        if let Some(b) = self.listen.buffers.iter_mut().find(|b| b.id == id) { b.name = name.trim().to_string(); b.renamed = true; }
        self.persist_buffers();
    }
    pub fn listen_close(&mut self, id: u64) {
        self.capture_buffer();
        let Some(at) = self.listen.buffers.iter().position(|b| b.id == id) else { return };
        if self.listen.pending.is_some_and(|p| p.0 == id) { self.listen.pending = None; }
        self.listen.undo = Some(Undo { originals: vec![(at, self.listen.buffers[at].clone())], created: Vec::new(), selected: self.listen.selected });
        if id == self.listen.active {
            self.stop(); self.queue.clear(); self.queue_origins.clear(); self.removed.clear(); self.queue_groups.clear();
            self.queue_placement.clear(); self.now = NowPlaying::default();
            // The output stays empty; another tab never starts by closing this one.
            self.listen.active = 0;
        }
        self.listen.buffers.remove(at);
        if self.media.as_ref().is_some_and(|m| m.buffer == id) { self.media = None; self.listen.undo = None; }
        if self.listen.buffers.is_empty() { self.listen_create("", &[]); }
        if self.listen.selected == id { self.listen.selected = self.listen.buffers[at.min(self.listen.buffers.len()-1)].id; }
        self.persist_buffers();
    }
    pub fn listen_replace(&mut self, id: u64, paths: &[String]) {
        self.capture_buffer();
        if self.listen.pending.is_some_and(|p| p.0 == id) { self.listen.pending = None; }
        let Some(mut b) = self.buffer_snapshot(id) else { return };
        let at = self.listen.buffers.iter().position(|item| item.id == id).unwrap();
        self.listen.undo = Some(Undo { originals: vec![(at, b.clone())], created: Vec::new(), selected: id });
        b.epoch += 1; b.paths = paths.to_vec(); b.origins = vec![self.browse_origin.clone(); paths.len()]; b.removed = vec![false; paths.len()];
        b.order = (0..paths.len()).collect(); b.groups.clear(); b.placement = vec![QueuePlacement::Original; paths.len()];
        b.cursor = 0; b.seconds = 0.0; b.stop_after = None;
        if self.listen.active == id {
            self.stop(); self.listen.active = 0; self.queue.clear(); self.queue_origins.clear(); self.now = NowPlaying::default();
        }
        self.store_buffer(b); self.persist_buffers();
    }
    pub fn listen_undo(&mut self) {
        let Some(undo) = self.listen.undo.take() else { return };
        let affected: Vec<_> = undo.originals.iter().map(|(_, b)| b.id).chain(undo.created).collect();
        if affected.contains(&self.listen.active) {
            self.stop(); self.listen.active = 0; self.queue.clear(); self.queue_origins.clear();
            self.removed.clear(); self.queue_placement.clear(); self.queue_groups.clear(); self.now = NowPlaying::default();
        }
        if self.listen.pending.is_some_and(|p| affected.contains(&p.0)) { self.listen.pending = None; }
        self.listen.buffers.retain(|b| !affected.contains(&b.id));
        for (at, b) in undo.originals { self.listen.buffers.insert(at.min(self.listen.buffers.len()), b); }
        self.listen.selected = if self.listen.buffers.iter().any(|b| b.id == undo.selected) { undo.selected } else { self.listen.buffers[0].id };
        self.listen.lengths = None;
        self.persist_buffers();
    }
    /// I append without starting the selected buffer, including when it is empty.
    pub fn listen_add(&mut self, id: u64, paths: &[String]) {
        if id == self.listen.active && self.now.playing { self.enqueue_here(paths); }
        else if let Some(mut b) = self.buffer_snapshot(id) {
            let start = b.paths.len(); b.origins.resize(start, None); b.paths.extend_from_slice(paths);
            b.origins.resize(b.paths.len(), self.browse_origin.clone());
            b.order.extend(start..b.paths.len()); b.removed.resize(b.paths.len(), false);
            b.placement.resize(b.paths.len(), QueuePlacement::Original);
            self.store_buffer(b.clone());
            if id == self.listen.active { self.install_buffer(&b); }
        }
        self.persist_buffers();
    }
    pub fn listen_save(&mut self, id: u64) -> Option<i64> { self.listen_save_named(id, "") }
    pub fn listen_save_named(&mut self, id: u64, name: &str) -> Option<i64> {
        let b = self.buffer_snapshot(id)?;
        let paths: Vec<_> = b.order.iter().filter(|&&i| b.live(i)).map(|&i| b.paths[i].clone()).collect();
        self.save_paths_as(if name.trim().is_empty() { &b.name } else { name.trim() }, &paths)
    }
    pub fn listen_selected(&self) -> u64 { self.listen.selected }
}
