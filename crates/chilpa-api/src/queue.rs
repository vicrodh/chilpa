//! The queue: what plays, in what order and what comes next.
//!
//! Everything that sets a list going goes through `launch`, so that repeat,
//! shuffle and the position count are applied in one place.

use super::*;

/// Where the playback switches are kept.
const GAPLESS: &str = "playback.gapless";
const NORMALIZE: &str = "playback.normalize";
const EXCLUSIVE: &str = "audio.exclusive";
const DSD_MODE: &str = "audio.dsdMode";
const KEEP_QUEUE: &str = "playback.keepQueue";

#[derive(serde::Serialize, serde::Deserialize)]
struct SavedQueue {
    entries: Vec<SavedEntry>,
    seconds: f64,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SavedEntry {
    path: String,
    placement: String,
    group: Option<SavedGroup>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct SavedGroup {
    key: u64,
    label: String,
    collapsed: bool,
}

fn queue_session_path() -> std::path::PathBuf {
    chilpa_dirs::data().join("queue.json")
}

impl Chilpa {
    pub(crate) fn replace_queue(&mut self, queue: Vec<String>) {
        self.before_queue_replace();
        self.set_stop_after(None);
        self.queue = queue;
        self.casting.order.clear();
        self.queue_placement = vec![QueuePlacement::Original; self.queue.len()];
        self.queue_groups.clear();
        self.removed = vec![false; self.queue.len()];
        self.restored_at = None;
        let order = if self.shuffle { chilpa_core::queue_playback::shuffled(self.queue.len(), crate::now_unix() as u64) } else { (0..self.queue.len()).collect() };
        if let Some(b) = self.listen.buffers.iter_mut().find(|b| b.id == self.listen.active) { b.order = order.clone(); }
        if let Some(p) = &self.player { p.set_order(order.clone(), self.queue.len()); }
        self.casting.order = order;
    }

    /// Queue persistence is on unless explicitly disabled in Settings.
    pub fn keep_queue(&self) -> bool {
        self.preference(KEEP_QUEUE).as_deref() != Some("false")
    }

    pub fn set_keep_queue(&mut self, on: bool) {
        self.remember_preference(KEEP_QUEUE, &on.to_string());
        if on {
            self.persist_queue_session();
        } else {
            let _ = std::fs::remove_file(queue_session_path());
            self.forget_buffers();
        }
    }

    pub(crate) fn persist_queue_session(&self) {
        self.persist_buffers();
    }

    pub(crate) fn restore_queue_session(&mut self) {
        if !self.keep_queue() {
            return;
        }
        let Ok(bytes) = std::fs::read(queue_session_path()) else { return };
        let Ok(saved) = serde_json::from_slice::<SavedQueue>(&bytes) else { return };
        if saved.entries.is_empty() || saved.entries.len() > 100_000 {
            return;
        }
        self.queue = saved.entries.iter().map(|entry| entry.path.clone()).collect();
        if self.queue.iter().any(|path| path.is_empty()) {
            self.queue.clear();
            return;
        }
        self.queue_placement = saved
            .entries
            .iter()
            .map(|entry| match entry.placement.as_str() {
                "next" => QueuePlacement::Next,
                "later" => QueuePlacement::Later,
                _ => QueuePlacement::Original,
            })
            .collect();
        self.removed = vec![false; self.queue.len()];
        let mut groups = std::collections::BTreeMap::<u64, QueueGroup>::new();
        for (index, entry) in saved.entries.iter().enumerate() {
            if let Some(group) = &entry.group {
                let restored = groups.entry(group.key).or_insert_with(|| QueueGroup {
                    key: group.key,
                    label: group.label.clone(),
                    tracks: Vec::new(),
                    collapsed: group.collapsed,
                });
                restored.tracks.push(index);
            }
        }
        self.queue_groups = groups.into_values().collect();
        self.next_queue_group = self.queue_groups.iter().map(|group| group.key).max().unwrap_or(0).wrapping_add(1).max(1);
        self.now = NowPlaying {
            path: self.queue[0].clone(),
            playing: true,
            paused: true,
            seconds: saved.seconds.max(0.0),
            index: 0,
            queue_len: self.queue.len(),
            ..NowPlaying::default()
        };
        self.describe_waiting();
        self.restored_at = Some(saved.seconds.max(0.0));
    }

    /// Says what the current track is before the player has opened it: a
    /// restored session waits paused, and the bar would otherwise show only
    /// the file name over an empty cover. The catalog answers without
    /// touching the file; a track outside it is read the way the player
    /// reads it when it starts.
    pub(crate) fn describe_waiting(&mut self) {
        if self.now.path.is_empty() { return; }
        let path = std::path::PathBuf::from(&self.now.path);
        self.catalog_context();
        if self.now.title.is_empty() && path.is_file() {
            if let Ok(info) = chilpa_tags::read(&path) {
                self.now.title = info.tags.title.clone().unwrap_or_default();
                self.now.artist = info.tags.artist_line();
                self.now.album = info.tags.album.clone().unwrap_or_default();
                self.now.total_seconds = info.duration.as_secs_f64();
                self.now.quality = Quality::of_track(&self.now.path, None, info.bit_depth, info.sample_rate_hz);
            }
        }
        if self.now.artwork_path.is_empty() {
            self.now.artwork_path = chilpa_tags::artwork_for(&path, &chilpa_core::artwork_cache())
                .map(|p| p.display().to_string()).unwrap_or_default();
        }
        if self.now.title.is_empty() { self.now.title = title_of(&path); }
        self.name_from_source(&path);
        self.now.live = self.name_from_entry(&path);
        self.name_from_heard();
        self.load_lyrics(&path, None);
    }

    /// Sets the queue already in `self.queue` going, from that position.
    ///
    /// Everything that plays a list goes through here: that way repeat,
    /// shuffle and the position count are applied in one place.
    pub(crate) fn spawn(&mut self, from: usize, device: &str, _bit_perfect: bool) {
        self.spawn_from(from, device, false, None, Relaunch::Track);
    }

    /// The same, with the first track starting somewhere other than its
    /// beginning, and saying why the old playback is being cut off.
    ///
    /// **The stop is what makes this work at all.** The player is one thread
    /// serving one order at a time, and an order to play blocks in it until
    /// the whole queue is done. Sending a second one without stopping first
    /// leaves it sitting in the channel: with repeat on, forever. That was
    /// the bug in going back to the previous track -- it did nothing until
    /// the queue ran out -- and it would have been the bug in changing output
    /// too.
    ///
    /// Stopping first is also what keeps the old device released before the
    /// new one is opened, which matters more here than anywhere: the orders
    /// are served in the order they arrive, so the play that is unwinding
    /// closes its sink before this one is even read.
    pub(crate) fn spawn_from(
        &mut self,
        from: usize,
        device: &str,
        _bit_perfect: bool,
        start_at: Option<std::time::Duration>,
        why: Relaunch,
    ) {
        // What is playing is about to be left: an episode keeps its place.
        self.keep_place(true);
        if self.casting.disconnecting {
            // I wait for the renderer's teardown before opening a local sink.
            self.casting.local_start = Some((from, device.to_string(), start_at));
            return;
        }
        if self.casting.active() {
            self.cast_load(from, start_at.map(|s| s.as_secs_f64()).unwrap_or(0.0), false);
            return;
        }
        chilpa_core::trace_playback(
            "facade",
            "launch_requested",
            format_args!(
                "from={from} device={device:?} start_at={start_at:?} reason={why:?} queue_len={} playing={} player_exists={}",
                self.queue.len(),
                self.now.playing,
                self.player.is_some()
            ),
        );
        let tracks: Vec<std::path::PathBuf> =
            self.queue.iter().map(std::path::PathBuf::from).collect();
        if tracks.is_empty() {
            return;
        }
        if self.player.is_some() && self.now.playing {
            self.relaunching = Some(why);
            if let Some(player) = &self.player {
                player.stop_for_handoff();
            }
            chilpa_core::trace_playback(
                "facade",
                "old_playback_stopped",
                format_args!("reason={why:?}"),
            );
        }
        let order = self.full_queue_order();
        self.base = 0;
        // I retain the complete order while changing only the cursor.
        self.mix = false;
        let (repeat, total) = (self.repeat, self.queue.len());
        let (gapless, normalize, exclusive) = (self.gapless(), self.normalize(), self.exclusive());
        let dsd_mode = self.dsd_mode();
        let volume = self.software_gain();
        let tap = self.tap.clone();
        let player = self
            .player
            .get_or_insert_with(move || chilpa_core::spawn_player_with(tap));
        player.set_repeat(repeat);
        player.set_gapless(gapless);
        player.set_normalize(normalize);
        player.set_exclusive(exclusive);
        player.set_dsd_mode(dsd_mode);
        player.set_volume(volume);
        player.set_order(order, tracks.len());
        // Moving output is the same track going on somewhere else, so what
        // the window is showing stays up. Clearing it would blank the title,
        // the artwork and the position for the moment the new device takes to
        // open, and there is nothing to tell somebody in that blink. Every
        // other relaunch really is a different track and starts from nothing.
        self.now = if why == Relaunch::Output {
            NowPlaying { playing: true, queue_len: total, ..std::mem::take(&mut self.now) }
        } else {
            NowPlaying { path: self.queue[from].clone(), index: from, seconds: start_at.map_or(0.0, |at| at.as_secs_f64()), playing: true, queue_len: total, ..NowPlaying::default() }
        };
        // Moved while paused, it stays paused where it went.
        let paused = why == Relaunch::Output && self.now.paused;
        chilpa_core::trace_playback(
            "facade",
            "launch_submitted",
            format_args!(
                "device={device:?} tracks={} first={:?} paused={paused} gapless={gapless} normalize={normalize} exclusive={exclusive}",
                tracks.len(),
                tracks.first()
            ),
        );
        player.play_from_index(&tracks, device, start_at, paused, Some(from), &self.removed, self.stop_after);
    }

    /// Moves what is playing to another output, keeping its place.
    ///
    /// Nothing happens if nothing is playing: the chosen output is already
    /// remembered, and the next track will start there.
    pub fn move_output(&mut self, device: &str, bit_perfect: bool) {
        if self.player.is_none() || !self.now.playing || self.queue.is_empty() {
            return;
        }
        let at = std::time::Duration::from_secs_f64(self.now.seconds.max(0.0));
        let index = self.now.index.min(self.queue.len() - 1);
        self.spawn_from(index, device, bit_perfect, Some(at), Relaunch::Output);
    }

    /// Starts what a search found playing, in that order.
    pub fn play_tracks(&mut self, paths: &[String], device: &str, bit_perfect: bool) -> Vec<String> {
        let paths = &self.playable(paths);
        chilpa_core::trace_playback(
            "facade",
            "play_tracks_called",
            format_args!("paths={} device={device:?} legacy_bit_perfect={bit_perfect}", paths.len()),
        );
        let tracks: Vec<std::path::PathBuf> =
            paths.iter().map(std::path::PathBuf::from).collect();
        // A disc image goes in as its tracks, one entry each.
        let tracks = chilpa_core::sacd_tracks::expand(&tracks);
        if tracks.is_empty() {
            return Vec::new();
        }
        self.replace_queue(tracks.iter().map(|p| p.display().to_string()).collect());
        self.spawn(0, device, bit_perfect);
        tracks.iter().map(|p| title_of(p)).collect()
    }

    /// What of those can be played now. A track whose folder is not mounted
    /// or whose server does not answer never reaches the queue (ADR-017);
    /// what the library does not know -- a file opened from a folder, a
    /// radio -- goes in as it is.
    pub(crate) fn playable(&mut self, paths: &[String]) -> Vec<String> {
        let refused = self.library().and_then(|lib| lib.unplayable_entries(paths).ok()).unwrap_or_default();
        if refused.is_empty() {
            return paths.to_vec();
        }
        let kept: Vec<String> = paths.iter().filter(|p| !refused.contains(*p)).cloned().collect();
        if kept.is_empty() {
            self.now.error = chilpa_i18n::t("none of that can be played now: its folder is not mounted or its server does not answer");
        }
        kept
    }

    /// The queue's titles, in the order they were queued.
    pub fn queue_titles(&self) -> Vec<String> {
        self.queue.iter().map(|r| title_of(std::path::Path::new(r))).collect()
    }

    /// The queue's files, whole and in the order they were queued.
    pub fn queue_paths(&self) -> &[String] {
        &self.queue
    }

    /// Which positions of the queue were removed.
    pub fn queue_removed(&self) -> &[bool] {
        &self.removed
    }

    /// Removes the track at that position from the queue.
    ///
    /// The playing one is not removed: that is what the next button is for.
    /// Nor is one that already went by, because it would change nothing.
    pub fn remove_from_queue(&mut self, index: usize) -> bool {
        if index <= self.now.index || index >= self.queue.len() {
            return false;
        }
        if self.casting.active() {
            if let Some(removed) = self.removed.get_mut(index) { *removed = true; return true; }
            return false;
        }
        let Some(p) = &self.player else { return false };
        // The player counts from where it started; the view, from the start
        // of the queue.
        if !p.remove_from_queue(index - self.base) {
            return false;
        }
        if let Some(q) = self.removed.get_mut(index) {
            *q = true;
        }
        true
    }

    /// Takes every entry `gone` names out of the queue, with a player or
    /// without one: removing a server leaves no track of it here, nor in
    /// the saved queue (ADR-017 §6). What is left is rebuilt from the
    /// current track on, with its groups; the history before it goes. It
    /// stays at the same second when the current track stays, and plays on
    /// if it was playing; when the current track goes, it waits paused.
    pub(crate) fn purge_queue(&mut self, gone: impl Fn(&str) -> bool) {
        if !self.queue.iter().any(|path| gone(path)) {
            return;
        }
        let playing = self.player.is_some() && !self.now.paused;
        let keeps_current = self.queue.get(self.now.index).is_some_and(|path| !gone(path));
        let seconds = if keeps_current { self.now.seconds.max(0.0) } else { 0.0 };
        let kept: Vec<(String, QueuePlacement, Option<QueueGroup>)> = self
            .queue_order()
            .into_iter()
            .filter(|&i| !gone(&self.queue[i]))
            .map(|i| (self.queue[i].clone(), self.queue_placement.get(i).copied().unwrap_or_default(), self.queue_group_at(i).cloned()))
            .collect();
        let next_group = self.next_queue_group;
        self.clear_queue();
        self.player = None;
        self.next_queue_group = next_group;
        if kept.is_empty() {
            self.now = NowPlaying::default();
            return;
        }
        let mut groups = std::collections::BTreeMap::<u64, QueueGroup>::new();
        for (index, (_, _, group)) in kept.iter().enumerate() {
            if let Some(group) = group {
                groups
                    .entry(group.key)
                    .or_insert_with(|| QueueGroup { key: group.key, label: group.label.clone(), tracks: Vec::new(), collapsed: group.collapsed })
                    .tracks
                    .push(index);
            }
        }
        self.queue_groups = groups.into_values().collect();
        self.queue_placement = kept.iter().map(|(_, placement, _)| *placement).collect();
        self.queue = kept.into_iter().map(|(path, _, _)| path).collect();
        self.removed = vec![false; self.queue.len()];
        self.now = NowPlaying {
            path: self.queue[0].clone(),
            playing: true,
            paused: true,
            seconds,
            index: 0,
            queue_len: self.queue.len(),
            ..NowPlaying::default()
        };
        self.restored_at = Some(seconds);
        if playing && keeps_current {
            self.set_paused(false);
        }
    }

    /// Stops and empties the queue.
    pub fn clear_queue(&mut self) {
        self.before_queue_replace();
        self.stop();
        self.set_stop_after(None);
        self.sleep_until = None;
        self.queue.clear();
        self.removed.clear();
        self.queue_placement.clear();
        self.queue_groups.clear();
        self.base = 0;
        self.now.queue_len = 0;
    }

    /// Marks a position in the whole queue as the last one to play. The
    /// current track is valid; history and removed rows are not.
    pub fn set_stop_after(&mut self, index: Option<usize>) -> bool {
        let valid = index.is_none_or(|at| {
            at >= self.now.index
                && at < self.queue.len()
                && !self.removed.get(at).copied().unwrap_or(false)
                && at >= self.base
        });
        if !valid {
            return false;
        }
        self.stop_after = index;
        if let Some(player) = &self.player {
            player.set_stop_after(index.map(|at| at - self.base));
        }
        true
    }

    pub fn stop_after(&self) -> Option<usize> {
        self.stop_after
    }

    /// Stops after the duration elapses. `None` turns the timer off. The
    /// deadline is checked by `poll`, which every frontend already drives.
    pub fn set_sleep_timer(&mut self, after: Option<std::time::Duration>) {
        self.sleep_until = after.and_then(|duration| std::time::Instant::now().checked_add(duration));
    }

    pub fn sleep_timer_remaining(&self) -> Option<std::time::Duration> {
        self.sleep_until.map(|deadline| deadline.saturating_duration_since(std::time::Instant::now()))
    }

    /// Adds tracks to the end of the queue. If nothing is playing, it starts.
    ///
    /// Queueing does not cut off what is playing: the queue grows behind it
    /// and what was added comes in when its turn arrives.
    pub fn enqueue(&mut self, paths: &[String], device: &str, bit_perfect: bool) -> Vec<String> {
        let paths = &self.playable(paths);
        let tracks: Vec<std::path::PathBuf> =
            paths.iter().map(std::path::PathBuf::from).collect();
        // A disc image goes in as its tracks, one entry each.
        let tracks = chilpa_core::sacd_tracks::expand(&tracks);
        if tracks.is_empty() {
            return Vec::new();
        }
        if !self.now.playing { return self.play_tracks(paths, device, bit_perfect); }
        self.insert_queued(&tracks, QueuePlacement::Original);
        tracks.iter().map(|p| title_of(p)).collect()
    }

    // Every output and a restored paused queue use the same insertion order.
    fn insert_queued(&mut self, tracks: &[std::path::PathBuf], placement: QueuePlacement) {
        let first = self.queue.len();
        let mut order = self.full_queue_order();
        let mut at = if placement == QueuePlacement::Original { order.len() }
            else { order.iter().position(|&i| i == self.now.index).map_or(order.len(), |i| i + 1) };
        while placement == QueuePlacement::Later && at < order.len()
            && matches!(self.queue_placement.get(order[at]), Some(QueuePlacement::Next | QueuePlacement::Later)) { at += 1; }
        self.queue.extend(tracks.iter().map(|p| p.display().to_string()));
        self.queue_placement.resize(self.queue.len(), placement);
        self.removed.resize(self.queue.len(), false);
        order.splice(at..at, first..self.queue.len());
        if let Some(p) = self.player.as_ref().filter(|p| !p.idle()) {
            p.enqueue(tracks); p.set_order(order.clone(), self.queue.len());
        }
        if let Some(b) = self.listen.buffers.iter_mut().find(|b| b.id == self.listen.active) { b.order = order.clone(); }
        self.casting.order = order;
        self.now.queue_len = self.queue.len();
    }

    /// `enqueue` on the output chosen in Settings, which is the one the
    /// window always means.
    pub fn enqueue_here(&mut self, paths: &[String]) {
        let (device, bit_perfect) = self.chosen_output();
        self.enqueue(paths, &device, bit_perfect);
    }

    /// `play_next` on the output chosen in Settings.
    pub fn play_next_here(&mut self, paths: &[String]) {
        let (device, bit_perfect) = self.chosen_output();
        self.play_next(paths, &device, bit_perfect);
    }

    /// Puts tracks right after the one playing, without cutting it off. If
    /// nothing is playing, they start.
    ///
    /// The player's queue never moves a position -- the track it is on is
    /// known by its place -- so they go at the end, and the order it walks
    /// the queue in puts them next: the same order shuffle and the mix use,
    /// read again each time a track ends. With shuffle on they go next in
    /// the shuffled order.
    pub fn play_next(&mut self, paths: &[String], device: &str, bit_perfect: bool) {
        let paths = &self.playable(paths);
        let tracks: Vec<std::path::PathBuf> = paths.iter().map(std::path::PathBuf::from).collect();
        let tracks = chilpa_core::sacd_tracks::expand(&tracks);
        if tracks.is_empty() {
            return;
        }
        if self.now.playing { self.insert_queued(&tracks, QueuePlacement::Next); }
        else { self.play_tracks(paths, device, bit_perfect); }
    }

    /// Adds a named batch after everything explicitly queued next, without
    /// interrupting the track that is playing. Later groups keep the order
    /// in which they were added.
    pub fn play_later(&mut self, paths: &[String], label: &str, device: &str, bit_perfect: bool) -> Option<u64> {
        let paths = &self.playable(paths);
        let tracks: Vec<std::path::PathBuf> = paths.iter().map(std::path::PathBuf::from).collect();
        let tracks = chilpa_core::sacd_tracks::expand(&tracks);
        if tracks.is_empty() {
            return None;
        }
        let first = if self.now.playing {
            let first = self.queue.len();
            self.insert_queued(&tracks, QueuePlacement::Later);
            first
        } else {
            if self.play_tracks(paths, device, bit_perfect).is_empty() { return None; }
            self.queue_placement.fill(QueuePlacement::Later);
            0
        };

        let key = self.next_queue_group;
        self.next_queue_group = self.next_queue_group.wrapping_add(1).max(1);
        self.queue_groups.push(QueueGroup {
            key,
            label: label.to_string(),
            tracks: (first..self.queue.len()).collect(),
            collapsed: false,
        });
        self.now.queue_len = self.queue.len();
        Some(key)
    }

    pub fn play_later_here(&mut self, paths: &[String], label: &str) -> Option<u64> {
        let (device, bit_perfect) = self.chosen_output();
        self.play_later(paths, label, &device, bit_perfect)
    }

    pub fn queue_groups(&self) -> &[QueueGroup] {
        &self.queue_groups
    }

    pub fn queue_group_at(&self, index: usize) -> Option<&QueueGroup> {
        self.queue_groups.iter().find(|group| group.tracks.contains(&index))
    }

    /// Moves rows in the visible, upcoming queue. `to` is a row boundary in
    /// that same view before the selected rows are lifted. The playing row
    /// cannot move; array positions stay fixed and only traversal changes.
    pub fn move_queued(&mut self, rows: &[usize], to: usize) -> bool {
        if self.player.is_none() && !self.casting.active() { return false; }
        let visible = self.queue_order();
        if rows.is_empty() || to > visible.len() {
            return false;
        }
        let mut selected_rows = rows.to_vec();
        selected_rows.sort_unstable();
        selected_rows.dedup();
        if selected_rows.iter().any(|&row| row == 0 || row >= visible.len()) {
            return false;
        }
        let selected: Vec<usize> = selected_rows.iter().map(|&row| visible[row]).collect();
        let mut moved: Vec<usize> = visible.iter().copied().filter(|index| !selected.contains(index)).collect();
        let destination = to
            .saturating_sub(selected_rows.iter().filter(|&&row| row < to).count())
            .min(moved.len());
        moved.splice(destination..destination, selected.iter().copied());
        if moved.first() != visible.first() {
            return false;
        }

        let own = self.queue.len().saturating_sub(self.base);
        let mut order = if self.casting.active() { self.casting.order.clone() } else { self.player.as_ref().map(|p| p.order()).unwrap_or_default() };
        if order.len() != own {
            order = (0..own).collect();
        }
        let live: std::collections::HashSet<usize> = visible.iter().copied().collect();
        let mut replacements = moved.into_iter().map(|index| index - self.base);
        for entry in &mut order {
            if live.contains(&(*entry + self.base)) {
                *entry = replacements.next().expect("one replacement for every visible queue row");
            }
        }
        if self.casting.active() { self.casting.order = order; true } else { self.player.as_ref().is_some_and(|p| p.set_order(order, own)) }
    }

    /// Folding is presentation state only; it never changes traversal.
    pub fn collapse_queue_group(&mut self, key: u64, collapsed: bool) -> bool {
        let Some(group) = self.queue_groups.iter_mut().find(|group| group.key == key) else { return false };
        group.collapsed = collapsed;
        true
    }

    pub fn remove_queue_group(&mut self, key: u64) -> bool {
        let Some(at) = self.queue_groups.iter().position(|group| group.key == key) else { return false };
        let tracks = self.queue_groups[at].tracks.clone();
        if tracks.iter().any(|&track| {
            track <= self.now.index || self.removed.get(track).copied().unwrap_or(true)
        }) {
            return false;
        }
        if !tracks.into_iter().all(|track| self.remove_from_queue(track)) {
            return false;
        }
        self.queue_groups.remove(at);
        true
    }

    /// The queue from the playing track on, as positions of the whole queue,
    /// in the order they will play and without the ones taken out: what the
    /// queue panel lists, row by row.
    pub fn queue_order(&self) -> Vec<usize> {
        let order = self.full_queue_order();
        let at = order.iter().position(|&i| i == self.now.index).unwrap_or(0);
        order[at..].iter().copied().filter(|&i| !self.removed.get(i).copied().unwrap_or(false)).collect()
    }

    /// Queues a whole album.
    pub fn enqueue_album(&mut self, album_id: i64, device: &str, bit_perfect: bool) -> Vec<String> {
        let paths: Vec<String> = self
            .album_tracks(album_id)
            .into_iter()
            .filter(|t| t.present)
            .map(|t| t.path)
            .collect();
        self.enqueue(&paths, device, bit_perfect)
    }

    /// Reorders the queue so the tempo changes little from track to track.
    ///
    /// It is what elsewhere is called automix: it starts at the playing one
    /// and each time takes the closest in BPM of those left. Whatever has
    /// no analysis goes to the end, in its own order: mixing blind is not
    /// mixing.
    ///
    /// It returns how many tracks went into the mix.
    pub fn automix(&mut self) -> usize {
        if self.queue.len() < 3 {
            self.now.error = chilpa_i18n::t("there is not enough queue to mix");
            return 0;
        }
        // Positions within what the player holds, not within the whole
        // queue.
        let paths: Vec<String> = self.queue.iter().skip(self.base).cloned().collect();
        let current = self.now.index.saturating_sub(self.base);

        let Some(lib) = self.library() else { return 0 };
        let source = match lib.source_id("local", "default") {
            Ok(s) => s,
            Err(_) => return 0,
        };
        let bpms: Vec<Option<f32>> = paths
            .iter()
            .map(|r| {
                let id = lib.track_by_path(source, std::path::Path::new(r)).ok().flatten()?;
                lib.track_bpm(id, chilpa_core::analysis_job::ANALYZER).ok().flatten()
            })
            .collect();

        let with_tempo = bpms.iter().filter(|b| b.is_some()).count();
        if with_tempo < 3 {
            self.now.error =
                chilpa_i18n::t("almost nothing in the queue has an analysed tempo");
            return 0;
        }

        let order = order_by_tempo(&bpms, current.min(paths.len().saturating_sub(1)));
        if self.casting.active() { self.casting.order = order.clone(); }
        if let Some(p) = &self.player {
            p.set_order(order, paths.len());
        }
        // Mixing and shuffling are two forms of the same thing: they cannot
        // both be on.
        self.shuffle = false;
        with_tempo
    }

    /// Whether one track runs straight into the next (on unless turned off).
    pub fn gapless(&self) -> bool {
        self.preference(GAPLESS).as_deref() != Some("false")
    }

    pub fn set_gapless(&mut self, on: bool) {
        self.remember_preference(GAPLESS, &on.to_string());
        if let Some(p) = &self.player {
            p.set_gapless(on);
        }
    }

    /// Whether tracks are brought to one loudness by their ReplayGain.
    pub fn normalize(&self) -> bool {
        self.preference(NORMALIZE).as_deref() == Some("true")
    }

    pub fn set_normalize(&mut self, on: bool) {
        self.remember_preference(NORMALIZE, &on.to_string());
        self.apply_playback_options(false);
    }

    /// Whether the output is asked for alone.
    pub fn exclusive(&self) -> bool {
        self.preference(EXCLUSIVE).as_deref() == Some("true")
    }

    /// Asking for the output alone or not happens when it opens, so what is
    /// playing is opened again where it is.
    pub fn set_exclusive(&mut self, on: bool) {
        self.remember_preference(EXCLUSIVE, &on.to_string());
        if let Some(p) = &self.player {
            p.set_exclusive(on);
        }
        let (output, _) = self.chosen_output();
        self.move_output(&output, false);
    }

    /// The explicit one-bit delivery selected for DSD tracks.
    pub fn dsd_mode(&self) -> chilpa_core::queue_playback::DsdMode {
        chilpa_core::queue_playback::DsdMode::from_key(
            // Existing installations played direct ALSA DSD as DoP before
            // this became an explicit three-way choice. Preserve that route
            // until the user chooses Native or PCM.
            self.preference(DSD_MODE).as_deref().unwrap_or("dop"),
        )
    }

    pub fn set_dsd_mode(&mut self, mode: &str) {
        let mode = chilpa_core::queue_playback::DsdMode::from_key(mode);
        self.remember_preference(DSD_MODE, mode.key());
        if let Some(player) = &self.player {
            player.set_dsd_mode(mode);
        }
        let (output, _) = self.chosen_output();
        self.move_output(&output, false);
    }

    /// Hands the player the switches as they stand.
    pub(crate) fn apply_playback_options(&mut self, _legacy_bit_perfect: bool) {
        let dsd_mode = self.dsd_mode();
        if let Some(p) = &self.player {
            p.set_gapless(self.gapless());
            p.set_normalize(self.normalize());
            p.set_dsd_mode(dsd_mode);
        }
        self.apply_volume();
    }

    /// Whether AutoMix has anything to order by: tracks whose tempo the
    /// analysis has read. Without them it can do nothing, so it is not
    /// offered.
    pub fn automix_available(&mut self) -> bool {
        use chilpa_core::analysis_job::{ANALYZER, VERSION};
        // Three, because a mix of fewer is not one (`automix`).
        self.library().and_then(|lib| lib.analyzed_count(ANALYZER, VERSION).ok()).is_some_and(|n| n >= 3)
    }

    /// Whether the queue is walked in the mix's order.
    pub fn automix_on(&self) -> bool {
        self.mix
    }

    /// Mixes the queue by tempo, or -- off -- walks it in the order it was
    /// queued again. Whatever reordered it, off puts it back.
    pub fn set_automix(&mut self, on: bool) {
        if on {
            self.mix = self.automix() > 0;
        } else {
            self.mix = false;
            if self.casting.active() { self.casting.order = (0..self.queue.len()).collect(); }
            if let Some(p) = &self.player {
                p.clear_order();
            }
        }
    }

    /// Whether the queue plays shuffled.
    pub fn shuffle(&self) -> bool {
        self.shuffle
    }

    /// Shuffles the queue or returns it to its order. It takes effect when
    /// the playing track ends: shuffling does not cut the audio.
    pub fn set_shuffle(&mut self, shuffle: bool) {
        self.shuffle = shuffle;
        if self.casting.active() { self.cast_order(); }
        // Shuffling and the mix are two orders: one replaces the other.
        self.mix = false;
        let order = if shuffle { chilpa_core::queue_playback::shuffled_now(self.queue.len()) }
            else { (0..self.queue.len()).collect() };
        if let Some(b) = self.listen.buffers.iter_mut().find(|b| b.id == self.listen.active) { b.order = order.clone(); }
        if let Some(p) = &self.player { p.set_order(order, self.queue.len()); }
    }

    pub fn repeat(&self) -> Repeat {
        self.repeat
    }

    /// Changes repeat. It also takes effect when the track ends.
    pub fn set_repeat(&mut self, repeat: Repeat) {
        self.repeat = repeat;
        if let Some(p) = &self.player {
            p.set_repeat(repeat);
        }
    }

    /// Opens an M3U list and starts it playing.
    ///
    /// It returns the queued titles; whatever the list names and is no
    /// longer there is reported in the error, instead of vanishing without
    /// a word.
    pub fn play_playlist(
        &mut self,
        path: &std::path::Path,
        device: &str,
        bit_perfect: bool,
    ) -> Vec<String> {
        let list = match chilpa_library::playlist::read(path) {
            Ok(l) => l,
            Err(e) => {
                self.now.error = chilpa_i18n::t_args("the list could not be read: {}", &[&e.to_string()]);
                return Vec::new();
            }
        };
        if !list.missing.is_empty() {
            self.now.error = chilpa_i18n::t_args(
                "{} of the list's tracks are gone",
                &[&list.missing.len().to_string()],
            );
        }
        if list.tracks.is_empty() {
            if self.now.error.is_empty() {
                self.now.error = chilpa_i18n::t("the list carries no tracks");
            }
            return Vec::new();
        }
        let paths: Vec<String> = list.tracks.iter().map(|p| p.display().to_string()).collect();
        let error = std::mem::take(&mut self.now.error);
        let titles = self.play_tracks(&paths, device, bit_perfect);
        self.now.error = error;
        titles
    }

    /// Saves the current queue as an M3U list.
    pub fn save_queue(&mut self, path: &std::path::Path) -> bool {
        if self.queue.is_empty() {
            self.now.error = chilpa_i18n::t("there is no queue to save");
            return false;
        }
        let tracks: Vec<std::path::PathBuf> =
            self.queue.iter().map(std::path::PathBuf::from).collect();
        match chilpa_library::playlist::write_m3u(path, &tracks) {
            Ok(()) => true,
            Err(e) => {
                self.now.error = chilpa_i18n::t_args("the list could not be saved: {}", &[&e.to_string()]);
                false
            }
        }
    }

    /// Starts a whole album playing, in the disc's order.
    ///
    /// It returns the queue's titles, not its paths: a queue showing file
    /// names tells nobody anything.
    pub fn play_album(&mut self, album_id: i64, device: &str, bit_perfect: bool) -> Vec<String> {
        let started = std::time::Instant::now();
        chilpa_core::trace_playback(
            "facade",
            "play_album_called",
            format_args!("album_id={album_id} device={device:?} legacy_bit_perfect={bit_perfect}"),
        );
        let available: Vec<LibraryTrack> =
            self.album_tracks(album_id).into_iter().filter(|t| t.present).collect();
        chilpa_core::trace_playback(
            "facade",
            "album_tracks_loaded",
            format_args!("after={:?} album_id={album_id} tracks={}", started.elapsed(), available.len()),
        );
        if available.is_empty() {
            self.now.error = chilpa_i18n::t("that album has no playable tracks");
            return Vec::new();
        }
        let tracks: Vec<std::path::PathBuf> =
            available.iter().map(|t| std::path::PathBuf::from(&t.path)).collect();
        self.replace_queue(tracks.iter().map(|p| p.display().to_string()).collect());
        self.spawn(0, device, bit_perfect);
        available.into_iter().map(|t| t.title).collect()
    }
}
