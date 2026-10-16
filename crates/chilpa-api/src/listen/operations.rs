use super::*;
use std::collections::{HashMap, HashSet};

impl Buffer {
    fn indices(&self) -> Vec<usize> { self.order.iter().copied().filter(|&i| self.live(i)).collect() }

    fn append_from(&mut self, source: &Buffer, indices: &[usize]) -> Vec<usize> {
        let offset = self.paths.len();
        let mapping: HashMap<_, _> = indices.iter().copied().zip(offset..).collect();
        for &i in indices {
            self.paths.push(source.paths[i].clone());
            self.origins.push(source.origins.get(i).cloned().flatten());
            self.placement.push(source.placement.get(i).copied().unwrap_or_default());
            self.removed.push(false);
        }
        for group in &source.groups {
            let tracks: Vec<_> = group.tracks.iter().filter_map(|i| mapping.get(i).copied()).collect();
            if tracks.is_empty() { continue; }
            self.groups.push(QueueGroup { key: self.groups.iter().map(|g| g.key).max().unwrap_or(0) + 1,
                label: group.label.clone(), tracks, collapsed: group.collapsed });
        }
        (offset..self.paths.len()).collect()
    }

    fn subset(&self, id: u64, indices: &[usize]) -> Buffer {
        let mut b = Buffer::empty(id);
        b.order = b.append_from(self, indices);
        b.shuffle = self.shuffle; b.repeat = self.repeat;
        if let Some(cursor) = indices.iter().position(|&i| i == self.cursor) { b.cursor = cursor; b.seconds = self.seconds; }
        b.stop_after = self.stop_after.and_then(|i| indices.iter().position(|&v| v == i));
        b
    }

    fn retain_indices(&mut self, indices: &[usize]) {
        let keep: HashSet<_> = indices.iter().copied().collect();
        for (i, removed) in self.removed.iter_mut().enumerate() { *removed |= !keep.contains(&i); }
        self.groups.retain_mut(|g| { g.tracks.retain(|i| keep.contains(i)); !g.tracks.is_empty() });
        if self.stop_after.is_some_and(|i| !self.live(i)) { self.stop_after = None; }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitting_a_shuffled_batch_preserves_occurrences_provenance_and_stop_marker() {
        let mut source = Buffer::empty(8);
        source.paths = vec!["a".into(), "b".into(), "a".into(), "c".into()];
        source.origins = (0..4).map(|id| Some(crate::queue_origin::QueueOrigin { id, ..Default::default() })).collect();
        source.removed = vec![false, true, false, false]; source.order = vec![2, 1, 0, 3];
        source.placement = vec![QueuePlacement::Later; 4]; source.shuffle = true; source.repeat = 1;
        source.cursor = 0; source.seconds = 12.5; source.stop_after = Some(3);
        source.groups = vec![QueueGroup { key: 4, label: "Batch".into(), tracks: vec![0, 1, 2, 3], collapsed: true }];
        let mut tail = source.subset(9, &[0, 3]);
        assert_eq!(tail.paths, ["a", "c"]);
        assert_eq!(tail.cursor, 0); assert_eq!(tail.seconds, 12.5); assert_eq!(tail.stop_after, Some(1));
        assert_eq!(tail.origins[1].as_ref().unwrap().id, 3);
        assert_eq!(tail.groups[0].tracks, [0, 1]); assert!(tail.groups[0].collapsed);
        assert!(tail.shuffle); assert_eq!(tail.repeat, 1);
        assert_eq!(tail.append_from(&source, &[2]), [2]);
        assert_eq!(tail.paths, ["a", "c", "a"]);
        assert_eq!(tail.groups[1].tracks, [2]); assert_ne!(tail.groups[0].key, tail.groups[1].key);
        assert_eq!(tail.origins[2].as_ref().unwrap().id, 2);
    }
}

impl Chilpa {
    pub fn listen_step(&mut self, by: i32) {
        let at = self.listen.buffers.iter().position(|b| b.id == self.listen.selected).unwrap_or(0);
        let next = (at as i64 + i64::from(by)).rem_euclid(self.listen.buffers.len() as i64) as usize;
        self.listen_select(self.listen.buffers[next].id);
    }

    fn remember_queue_edit(&mut self, ids: &[u64], created: Vec<u64>) {
        self.capture_buffer();
        let originals = self.listen.buffers.iter().enumerate().filter(|(_, b)| ids.contains(&b.id))
            .map(|(at, b)| (at, b.clone())).collect();
        self.listen.undo = Some(Undo { originals, created, selected: self.listen.selected });
    }

    // I keep the decoder's indices intact while editing the visible sequence.
    fn apply_live_buffer(&mut self, b: &Buffer) {
        let added: Vec<_> = b.paths[self.queue.len()..].iter().map(std::path::PathBuf::from).collect();
        if let Some(p) = self.player.as_ref().filter(|p| !p.idle()) {
            if !added.is_empty() { p.enqueue(&added); }
            let order = b.order.iter().copied().filter(|&i| i >= self.base).map(|i| i - self.base).collect();
            p.set_order(order, b.paths.len().saturating_sub(self.base));
            p.set_stop_after(b.stop_after.map(|i| i.saturating_sub(self.base)));
            for (i, removed) in b.removed.iter().enumerate().skip(self.base) { if *removed { p.remove_from_queue(i - self.base); } }
        }
        self.listen.active = b.id;
        self.queue = b.paths.clone(); self.queue_origins = b.origins.clone(); self.removed = b.removed.clone();
        self.queue_placement = b.placement.clone(); self.queue_groups = b.groups.clone();
        self.next_queue_group = b.groups.iter().map(|g| g.key).max().unwrap_or(0) + 1;
        self.casting.order = b.order.clone(); self.stop_after = b.stop_after; self.now.queue_len = b.paths.len();
    }

    pub fn listen_split(&mut self, key: &str) -> Option<u64> {
        let (source, index) = self.locate(key)?;
        if !self.can_split(&source, index) { return None; }
        let indices = source.indices();
        let at = indices.iter().position(|&i| i == index)?;
        if at == 0 { return None; }
        let id = self.listen.next;
        self.remember_queue_edit(&[source.id], vec![id]);
        self.listen.next += 1;
        let (before, after) = indices.split_at(at);
        let active = source.id == self.listen.active;
        let mut first = source.subset(source.id, before);
        let mut second = source.subset(id, after);
        if active {
            if before.contains(&source.cursor) {
                first = source.clone(); first.retain_indices(before); self.apply_live_buffer(&first);
            } else {
                second = source.clone(); second.id = id; second.epoch = 1;
                second.retain_indices(after); self.apply_live_buffer(&second);
            }
        }
        if !(active && before.contains(&source.cursor)) { first.epoch = source.epoch.wrapping_add(1); }
        first.name = source.name.clone(); first.renamed = source.renamed;
        second.name = chilpa_i18n::t_args("{} (split)", &[&source.name]); second.renamed = true;
        let position = self.listen.buffers.iter().position(|b| b.id == source.id)?;
        self.listen.buffers[position] = first;
        self.listen.buffers.insert(position + 1, second);
        self.listen.selected = id; self.listen.lengths = None;
        self.persist_buffers(); Some(id)
    }

    pub(super) fn can_split(&self, source: &Buffer, index: usize) -> bool {
        if self.listen.pending.is_some() || self.media.as_ref().is_some_and(|m| m.buffer == source.id) { return false; }
        let indices = source.indices();
        let Some(at) = indices.iter().position(|&i| i == index).filter(|&at| at > 0) else { return false; };
        let total = source.paths.len() + self.listen.buffers.iter().filter(|b| b.id != source.id).map(|b| b.paths.len()).sum::<usize>();
        let extra = if source.id != self.listen.active { 0 }
            else if indices[..at].contains(&source.cursor) { indices.len() - at } else { at };
        total + extra <= 100_000
    }

    pub fn listen_merge(&mut self, ids: &[u64], name: &str, keep: bool) -> Result<u64, String> {
        if self.listen.pending.is_some() { return Err(chilpa_i18n::t("Wait for the queue to finish switching.")); }
        if ids.len() < 2 || ids.iter().copied().collect::<HashSet<_>>().len() != ids.len() {
            return Err(chilpa_i18n::t("Choose at least two different queues."));
        }
        let sources: Vec<_> = ids.iter().map(|&id| self.buffer_snapshot(id)).collect::<Option<_>>()
            .ok_or_else(|| chilpa_i18n::t("A selected queue is no longer available."))?;
        if self.media.as_ref().is_some_and(|m| ids.contains(&m.buffer)) {
            return Err(chilpa_i18n::t("The temporary media queue cannot be combined."));
        }
        if sources.iter().map(|b| b.indices().len()).sum::<usize>() == 0 {
            return Err(chilpa_i18n::t("The selected queues are empty."));
        }
        let active = (!keep).then(|| sources.iter().find(|b| b.id == self.listen.active)).flatten();
        let id = active.map_or(self.listen.next, |b| b.id);
        let mut merged = active.cloned().unwrap_or_else(|| Buffer::empty(id));
        let original_len = merged.paths.len();
        merged.origins.resize(original_len, None);
        let mut order = Vec::new();
        for source in &sources {
            let indices = source.indices();
            if source.id == id { order.extend(indices); }
            else { order.extend(merged.append_from(source, &indices)); }
        }
        // Removed occurrences still occupy decoder indices and the persisted permutation.
        order.extend((0..original_len).filter(|&i| !merged.live(i)));
        merged.order = order;
        let total: usize = self.listen.buffers.iter().filter(|b| keep || !ids.contains(&b.id)).map(|b| b.paths.len()).sum();
        if total + merged.paths.len() > 100_000 { return Err(chilpa_i18n::t("The queues exceed the session limit of 100,000 tracks.")); }
        merged.name = if name.trim().is_empty() { chilpa_i18n::t("Combined queue") } else { name.trim().into() };
        merged.renamed = true;
        self.remember_queue_edit(if keep { &[] } else { ids }, if active.is_some() { Vec::new() } else { vec![id] });
        if active.is_some() { self.apply_live_buffer(&merged); }
        else { self.listen.next += 1; }
        let at = if keep { self.listen.buffers.len() } else { self.listen.buffers.iter().position(|b| ids.contains(&b.id)).unwrap() };
        if !keep { self.listen.buffers.retain(|b| !ids.contains(&b.id)); }
        self.listen.buffers.insert(at, merged);
        self.listen.selected = id; self.listen.lengths = None;
        self.persist_buffers(); Ok(id)
    }
}
