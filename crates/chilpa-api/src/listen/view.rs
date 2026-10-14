use super::*;
use crate::TrackEntry;
use serde_json::{json, Value};
use std::hash::{Hash, Hasher};

impl Chilpa {
    fn buffer_name(&self, b: &Buffer) -> String {
        if self.listen.buffers.len() == 1 && !b.renamed { chilpa_i18n::t("Now playing") } else { b.name.clone() }
    }
    pub(crate) fn listen_key(&self, index: usize) -> String {
        self.listen.buffers.iter().find(|b| b.id == self.listen.active).map(|b| b.key(index)).unwrap_or_default()
    }
    pub fn listen_is_playing(&self, key: &str) -> bool {
        self.now.playing && self.listen_key(self.now.index) == key
    }
    pub fn listen_playing_key(&self) -> String {
        if self.now.playing { self.listen_key(self.now.index) } else { String::new() }
    }
    /// Metadata only: track rows continue through the shared track model.
    pub fn listen_json(&self) -> String {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        let tabs: Vec<_> = self.listen.buffers.iter().filter_map(|item| {
            let mut b = self.buffer_snapshot(item.id)?;
            b.seconds = 0.0;
            serde_json::to_string(&b).unwrap_or_default().hash(&mut hash);
            Some(json!({"id": b.id.to_string(), "name": self.buffer_name(&b),
                "count": b.order.iter().filter(|&&i| b.live(i)).count(),
                "playing": b.id == self.listen.active && self.now.playing,
                "paused": self.now.paused}))
        }).collect();
        self.now.playing.hash(&mut hash); self.now.paused.hash(&mut hash);
        let b = self.buffer_snapshot(self.listen.selected);
        json!({"tabs": tabs, "selected": self.listen.selected.to_string(),
            "active": self.listen.active.to_string(), "revision": hash.finish().to_string(),
            "name": b.as_ref().map(|b| self.buffer_name(b)), "undo": self.listen.undo.is_some(),
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
        if full {
            self.listen.lengths = Some((id, indices.iter().zip(&tracks).map(|(&(_, i), t)| (i, t.duration_ms)).collect()));
        }
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
                rows.push(TrackEntry::header(phase.clone(), String::new())); keys.push(String::new()); section = phase;
            }
            rows.push(TrackEntry::Track(track)); keys.push(b.key(index));
        }
        (rows, keys)
    }
    /// What the Queues screen says beside its search: how many songs the
    /// open queue holds and how long they last, and how long until it ends
    /// from where it stands, its cursor's track counted from its position.
    /// It adds up the lengths its rows were last listed with, so the tick
    /// that asks for it reads nothing.
    pub fn listen_footer(&self) -> String {
        let Some(b) = self.buffer_snapshot(self.listen.selected) else { return "{}".into() };
        let Some((_, lengths)) = self.listen.lengths.as_ref().filter(|(id, _)| *id == b.id) else { return "{}".into() };
        let order: Vec<usize> = b.order.iter().copied().filter(|&i| b.live(i)).collect();
        let (songs, total, left) = queue_left(&order, b.cursor, b.seconds, lengths);
        let n = chilpa_i18n::tf("{} song", "{} songs", songs as i64, &[&songs.to_string()]);
        json!({"summary": format!("{n} {}", hours_minutes(total)), "left_ms": left}).to_string()
    }

    /// What the active buffer played before the cursor, most recent first
    /// and without headers: the history tab of a queue view.
    pub fn listen_history(&mut self, query: &str) -> (Vec<TrackEntry>, Vec<String>) {
        let Some(b) = self.buffer_snapshot(self.listen.active) else { return Default::default() };
        let current = b.order.iter().position(|&i| i == b.cursor).unwrap_or(0);
        let indices: Vec<_> = b.order.iter().enumerate().filter(|&(position, &i)| b.live(i) && position < current).map(|(_, &i)| i).rev().collect();
        let paths: Vec<_> = indices.iter().map(|&i| b.paths[i].clone()).collect();
        let tracks = self.rows_for_paths(&paths);
        let query = query.trim().to_lowercase();
        let (mut rows, mut keys) = (Vec::new(), Vec::new());
        for (index, track) in indices.into_iter().zip(tracks) {
            if !query.is_empty() && !format!("{} {} {}", track.title, track.artist, track.album).to_lowercase().contains(&query) { continue; }
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
    /// Queue destinations follow the availability of the entire selection.
    pub fn track_queue_menu(&self, context: crate::TrackMenuContext) -> Vec<Value> {
        let available = context.count > 0 && !context.unavailable;
        let mut entries = crate::track_menu(context);
        if available { entries.extend(self.listen_targets()); }
        entries
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

/// A queue's songs, how long they last, and how long until it ends from its
/// cursor `seconds` into that track: `order` holds the live tracks in play
/// order and `lengths` each one's length.
fn queue_left(order: &[usize], cursor: usize, seconds: f64, lengths: &std::collections::HashMap<usize, u64>) -> (usize, u64, u64) {
    let length = |i: &usize| lengths.get(i).copied().unwrap_or(0);
    let total = order.iter().map(length).sum();
    let left = match order.iter().position(|&i| i == cursor) {
        Some(at) => length(&cursor).saturating_sub((seconds.max(0.0) * 1000.0) as u64) + order[at + 1..].iter().map(length).sum::<u64>(),
        None => total,
    };
    (order.len(), total, left)
}

/// "1 hr 23 min", "2 hrs 5 min", "45 min".
fn hours_minutes(ms: u64) -> String {
    let minutes = ms / 60_000;
    let (h, m) = (minutes / 60, minutes % 60);
    let mins = chilpa_i18n::t_args("{} min", &[&m.to_string()]);
    if h == 0 { mins } else { format!("{} {mins}", chilpa_i18n::tf("{} hr", "{} hrs", h as i64, &[&h.to_string()])) }
}

#[cfg(test)]
mod footer_tests {
    use super::*;

    #[test]
    fn a_queue_ends_when_its_cursor_track_and_the_ones_after_it_have_played() {
        let lengths = [(0, 60_000), (1, 120_000), (2, 180_000), (3, 240_000)].into_iter().collect();
        assert_eq!(queue_left(&[0, 2, 1, 3], 2, 30.0, &lengths), (4, 600_000, 150_000 + 120_000 + 240_000));
        assert_eq!(queue_left(&[0, 1], 9, 0.0, &lengths).2, 180_000, "no cursor among them: all of it is left");
        assert_eq!(hours_minutes(83 * 60_000), "1 hr 23 min");
        assert_eq!(hours_minutes(125 * 60_000), "2 hrs 5 min");
        assert_eq!(hours_minutes(45 * 60_000 + 59_000), "45 min");
    }
}
