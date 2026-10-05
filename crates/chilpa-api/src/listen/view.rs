use super::*;
use crate::TrackEntry;
use serde_json::{json, Value};
use std::hash::{Hash, Hasher};

impl Chilpa {
    pub(crate) fn listen_key(&self, index: usize) -> String {
        self.listen.buffers.iter().find(|b| b.id == self.listen.active).map(|b| b.key(index)).unwrap_or_default()
    }
    pub fn listen_is_playing(&self, key: &str) -> bool {
        self.now.playing && self.listen_key(self.now.index) == key
    }
    /// Metadata only: track rows continue through the shared track model.
    pub fn listen_json(&self) -> String {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        let tabs: Vec<_> = self.listen.buffers.iter().filter_map(|item| {
            let mut b = self.buffer_snapshot(item.id)?;
            b.seconds = 0.0;
            serde_json::to_string(&b).unwrap_or_default().hash(&mut hash);
            Some(json!({"id": b.id.to_string(), "name": b.name,
                "count": b.order.iter().filter(|&&i| b.live(i)).count(),
                "playing": b.id == self.listen.active && self.now.playing,
                "paused": self.now.paused}))
        }).collect();
        self.now.playing.hash(&mut hash); self.now.paused.hash(&mut hash);
        let b = self.buffer_snapshot(self.listen.selected);
        json!({"tabs": tabs, "selected": self.listen.selected.to_string(),
            "active": self.listen.active.to_string(), "revision": hash.finish().to_string(),
            "name": b.as_ref().map(|b| &b.name), "undo": self.listen.undo.is_some(),
            "pending": self.listen.pending.is_some(), "error": self.now.error}).to_string()
    }
    pub fn listen_rows(&mut self, id: u64, query: &str, full: bool) -> (Vec<TrackEntry>, Vec<String>) {
        let id = if full { id } else { self.listen.active };
        let Some(b) = self.buffer_snapshot(id) else { return Default::default() };
        let current = b.order.iter().position(|&i| i == b.cursor).unwrap_or(0);
        let indices: Vec<_> = b.order.iter().enumerate().filter(|&(position, &i)| b.live(i) && (full || position >= current))
            .map(|(position, &i)| (position, i)).collect();
        let paths: Vec<_> = indices.iter().map(|&(_, i)| b.paths[i].clone()).collect();
        let tracks = self.rows_for_paths(&paths);
        let query = query.trim().to_lowercase();
        let (mut rows, mut keys) = (Vec::new(), Vec::new());
        let mut section = String::new();
        for ((position, index), track) in indices.into_iter().zip(tracks) {
            if !query.is_empty() && !format!("{} {} {}", track.title, track.artist, track.album).to_lowercase().contains(&query) { continue; }
            let phase = if position < current { chilpa_i18n::t("History") }
                else if position == current {
                    if id == self.listen.active && self.now.playing { chilpa_i18n::t("Now playing") }
                    else { chilpa_i18n::t("Saved position") }
                } else { chilpa_i18n::t("Up next") };
            if full && query.is_empty() && section != phase {
                rows.push(TrackEntry::Header { label: phase.clone(), meta: String::new() }); keys.push(String::new()); section = phase;
            }
            rows.push(TrackEntry::Track(track)); keys.push(b.key(index));
        }
        (rows, keys)
    }
    fn locate(&self, key: &str) -> Option<(Buffer, usize)> {
        let id = key.split(':').next()?.parse().ok()?;
        let b = self.buffer_snapshot(id)?;
        let index = b.order.iter().copied().find(|&i| b.key(i) == key && b.live(i))?;
        Some((b, index))
    }
    pub fn listen_row_action(&mut self, key: &str, action: &str, target: &str) -> bool {
        let Some((mut b, index)) = self.locate(key) else { return false };
        if action == "play" { return self.listen_play(b.id, key); }
        let active = b.id == self.listen.active;
        let current = active && index == self.now.index && self.now.playing;
        let at = b.order.iter().position(|&i| i == index).unwrap();
        match action {
            "remove" if !current => b.removed[index] = true,
            "after" => {
                for &i in &b.order[at + 1..] {
                    if active && i == self.now.index && self.now.playing { return false; }
                    b.removed[i] = true;
                }
            }
            "stop" => b.stop_after = if b.stop_after == Some(index) { None } else { Some(index) },
            "move" if !current => {
                let Some((other, to)) = self.locate(target) else { return false };
                if other.id != b.id || other.epoch != b.epoch { return false; }
                let destination = b.order.iter().position(|&i| i == to).unwrap();
                b.order.remove(at); b.order.insert(destination, index);
            }
            _ => return false,
        }
        if !b.live(b.cursor) {
            b.cursor = b.order[at..].iter().chain(b.order[..at].iter().rev()).copied().find(|&i| b.live(i)).unwrap_or(0);
            b.seconds = 0.0;
        }
        if b.stop_after.is_some_and(|i| !b.live(i)) { b.stop_after = None; }
        if active {
            self.removed = b.removed.clone(); self.stop_after = b.stop_after;
            self.casting.order = b.order.clone();
            if let Some(p) = &self.player {
                p.set_order(b.order.clone(), b.paths.len()); p.set_stop_after(b.stop_after);
                for (i, removed) in b.removed.iter().enumerate() { if *removed { p.remove_from_queue(i); } }
            }
        }
        self.store_buffer(b); self.persist_buffers(); true
    }
    pub fn listen_menu(&self, key: &str) -> Vec<Value> {
        let Some((b, index)) = self.locate(key) else { return Vec::new() };
        let item = |act, label| json!({"act":act,"text":chilpa_i18n::t(label)});
        let mut items = vec![item("play", chilpa_i18n::mark("Play now"))];
        if !(b.id == self.listen.active && index == self.now.index && self.now.playing) {
            items.push(item("listen:remove", chilpa_i18n::mark("Remove from queue")));
        }
        items.push(item("listen:stop", if b.stop_after == Some(index) { chilpa_i18n::mark("Cancel stop after this track") } else { chilpa_i18n::mark("Stop after this track") }));
        items.push(item("listen:after", chilpa_i18n::mark("Clear after this track")));
        items
    }
    pub fn collection_queue_menu(&self) -> Value {
        let mut entries: Vec<_> = [("queue", chilpa_i18n::mark("Add to queue")), ("next", chilpa_i18n::mark("Play next")), ("later", chilpa_i18n::mark("Play later"))]
            .into_iter().map(|(act, label)| json!({"act":act,"text":chilpa_i18n::t(label)})).collect();
        entries.extend(self.listen_targets());
        json!(entries)
    }
    pub fn listen_targets(&self) -> Vec<Value> {
        let mut items = vec![json!({"act":"buffer:new", "text":chilpa_i18n::t("Open in new queue")})];
        for b in &self.listen.buffers {
            items.push(json!({"act":format!("buffer:add:{}",b.id), "text":chilpa_i18n::t_args("Add to queue {}", &[&b.name])}));
            items.push(json!({"act":format!("buffer:replace:{}",b.id), "text":chilpa_i18n::t_args("Replace queue {}", &[&b.name])}));
        }
        vec![json!({"separator":true}), json!({"text":chilpa_i18n::t("Extra queues"), "submenu":true, "children":items})]
    }
    pub fn listen_paths_action(&mut self, action: &str, paths: &[String]) {
        let parts: Vec<_> = action.split(':').collect();
        match parts.as_slice() {
            ["buffer", "new"] => { self.listen_create("", paths); }
            ["buffer", "add", id] => if let Ok(id) = id.parse() { self.listen_add(id, paths); },
            ["buffer", "replace", id] => if let Ok(id) = id.parse() { self.listen_replace(id, paths); },
            _ => {}
        }
    }
    pub fn listen_command(&mut self, action: &str, id: &str, text: &str) {
        let id = id.parse().unwrap_or(self.listen.selected);
        match action {
            "select" => self.listen_select(id),
            "new" => { self.listen_create("", &[]); }
            "rename" => self.listen_rename(id, text),
            "play" => { self.listen_play(id, ""); }
            "close" => self.listen_close(id),
            "clear" => self.listen_replace(id, &[]),
            "save" => { self.listen_save(id); }
            "undo" => self.listen_undo(),
            _ => {}
        }
    }
}
