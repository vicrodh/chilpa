//! The queue: what plays, in what order and what comes next.
//!
//! Everything that sets a list going goes through `launch`, so that repeat,
//! shuffle and the position count are applied in one place.

use super::*;

/// Where the playback switches are kept.
const GAPLESS: &str = "playback.gapless";
const NORMALIZE: &str = "playback.normalize";
const EXCLUSIVE: &str = "audio.exclusive";

impl Chilpa {
    /// Sets the queue already in `self.queue` going, from that position.
    ///
    /// Everything that plays a list goes through here: that way repeat,
    /// shuffle and the position count are applied in one place.
    pub(crate) fn spawn(&mut self, from: usize, device: &str, bit_perfect: bool) {
        self.spawn_from(from, device, bit_perfect, None, Relaunch::Track);
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
        bit_perfect: bool,
        start_at: Option<std::time::Duration>,
        why: Relaunch,
    ) {
        let tracks: Vec<std::path::PathBuf> =
            self.queue.iter().skip(from).map(std::path::PathBuf::from).collect();
        if tracks.is_empty() {
            return;
        }
        if self.player.is_some() && self.now.playing {
            self.relaunching = Some(why);
            if why == Relaunch::Output {
                // The listen did not end and it was not skipped: it is the
                // same track, still going, coming out somewhere else. What
                // was played so far is dropped rather than written down as a
                // listen of its own, so one track logs one listen.
                self.in_progress = None;
            }
            self.stop();
        }
        self.base = from;
        self.removed = vec![false; self.queue.len()];
        // The player starts in the queue's own order or shuffled: a mix of
        // what played before does not carry over.
        self.mix = false;
        let (shuffle, repeat, total) = (self.shuffle, self.repeat, self.queue.len());
        let (gapless, normalize, exclusive) = (self.gapless(), self.normalize() && !bit_perfect, self.exclusive());
        let volume = self.software_gain();
        let tap = self.tap.clone();
        let player = self
            .player
            .get_or_insert_with(move || chilpa_core::spawn_player_with(tap));
        player.set_repeat(repeat);
        player.set_gapless(gapless);
        player.set_normalize(normalize);
        player.set_exclusive(exclusive);
        player.set_volume(volume);
        if shuffle {
            player.shuffle(tracks.len());
        } else {
            player.clear_order();
        }
        // Moving output is the same track going on somewhere else, so what
        // the window is showing stays up. Clearing it would blank the title,
        // the artwork and the position for the moment the new device takes to
        // open, and there is nothing to tell somebody in that blink. Every
        // other relaunch really is a different track and starts from nothing.
        self.now = if why == Relaunch::Output {
            NowPlaying { playing: true, queue_len: total, ..std::mem::take(&mut self.now) }
        } else {
            NowPlaying { playing: true, queue_len: total, ..NowPlaying::default() }
        };
        // Moved while paused, it stays paused where it went.
        let paused = why == Relaunch::Output && self.now.paused;
        player.play_all_from_paused(&tracks, device, bit_perfect, start_at, paused);
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
        let tracks: Vec<std::path::PathBuf> =
            paths.iter().map(std::path::PathBuf::from).collect();
        // A disc image goes in as its tracks, one entry each.
        let tracks = chilpa_core::sacd_tracks::expand(&tracks);
        if tracks.is_empty() {
            return Vec::new();
        }
        self.set_stop_after(None);
        self.queue = tracks.iter().map(|p| p.display().to_string()).collect();
        self.queue_placement = vec![QueuePlacement::Original; self.queue.len()];
        self.queue_groups.clear();
        self.spawn(0, device, bit_perfect);
        tracks.iter().map(|p| title_of(p)).collect()
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

    /// Stops and empties the queue.
    pub fn clear_queue(&mut self) {
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
        let tracks: Vec<std::path::PathBuf> =
            paths.iter().map(std::path::PathBuf::from).collect();
        // A disc image goes in as its tracks, one entry each.
        let tracks = chilpa_core::sacd_tracks::expand(&tracks);
        if tracks.is_empty() {
            return Vec::new();
        }
        let playing = self.now.playing && self.player.is_some();
        if !playing {
            return self.play_tracks(paths, device, bit_perfect);
        }
        self.queue.extend(tracks.iter().map(|p| p.display().to_string()));
        self.queue_placement.resize(self.queue.len(), QueuePlacement::Original);
        self.removed.resize(self.queue.len(), false);
        if let Some(p) = &self.player {
            p.enqueue(&tracks);
        }
        self.now.queue_len = self.queue.len();
        tracks.iter().map(|p| title_of(p)).collect()
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
        let tracks: Vec<std::path::PathBuf> = paths.iter().map(std::path::PathBuf::from).collect();
        let tracks = chilpa_core::sacd_tracks::expand(&tracks);
        if tracks.is_empty() {
            return;
        }
        let playing = self.now.playing && self.player.is_some();
        let Some(p) = self.player.as_ref().filter(|_| playing) else {
            self.play_tracks(paths, device, bit_perfect);
            return;
        };
        let before = p.queue_len();
        self.queue.extend(tracks.iter().map(|t| t.display().to_string()));
        self.queue_placement.resize(self.queue.len(), QueuePlacement::Next);
        self.removed.resize(self.queue.len(), false);
        p.enqueue(&tracks);
        let len = before + tracks.len();
        let mut order = p.order();
        if order.len() != before {
            order = (0..before).collect();
        }
        // The player counts from where it started; the view, from the start
        // of the queue.
        let current = self.now.index.saturating_sub(self.base);
        let mut at = order.iter().position(|&i| i == current).map_or(order.len(), |pos| pos + 1);
        while at < order.len()
            && self.queue_placement.get(self.base + order[at]) == Some(&QueuePlacement::Next)
        {
            at += 1;
        }
        order.splice(at..at, before..len);
        p.set_order(order, len);
        self.now.queue_len = self.queue.len();
    }

    /// Adds a named batch after everything explicitly queued next, without
    /// interrupting the track that is playing. Later groups keep the order
    /// in which they were added.
    pub fn play_later(&mut self, paths: &[String], label: &str, device: &str, bit_perfect: bool) -> Option<u64> {
        let tracks: Vec<std::path::PathBuf> = paths.iter().map(std::path::PathBuf::from).collect();
        let tracks = chilpa_core::sacd_tracks::expand(&tracks);
        if tracks.is_empty() {
            return None;
        }
        let playing = self.now.playing && self.player.is_some();
        let Some(player) = self.player.as_ref().filter(|_| playing) else {
            if self.play_tracks(paths, device, bit_perfect).is_empty() {
                return None;
            }
            self.queue_placement.fill(QueuePlacement::Later);
            let key = self.next_queue_group;
            self.next_queue_group = self.next_queue_group.wrapping_add(1).max(1);
            self.queue_groups.push(QueueGroup {
                key,
                label: label.to_string(),
                tracks: (0..self.queue.len()).collect(),
                collapsed: false,
            });
            return Some(key);
        };

        let first = self.queue.len();
        let before = player.queue_len();
        self.queue.extend(tracks.iter().map(|track| track.display().to_string()));
        self.queue_placement.resize(self.queue.len(), QueuePlacement::Later);
        self.removed.resize(self.queue.len(), false);
        player.enqueue(&tracks);

        let len = before + tracks.len();
        let mut order = player.order();
        if order.len() != before {
            order = (0..before).collect();
        }
        let current = self.now.index.saturating_sub(self.base);
        let mut at = order.iter().position(|&i| i == current).map_or(order.len(), |pos| pos + 1);
        while at < order.len()
            && matches!(
                self.queue_placement.get(self.base + order[at]),
                Some(QueuePlacement::Next | QueuePlacement::Later)
            )
        {
            at += 1;
        }
        order.splice(at..at, before..len);
        player.set_order(order, len);

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
        let Some(player) = &self.player else { return false };
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
        let mut order = player.order();
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
        player.set_order(order, own)
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
        let len = self.queue.len();
        let from = self.now.index.min(len);
        let order = self.player.as_ref().map(|p| p.order()).unwrap_or_default();
        let own = len.saturating_sub(self.base);
        let ahead: Vec<usize> = if own > 0 && order.len() == own {
            let current = from.saturating_sub(self.base);
            let at = order.iter().position(|&i| i == current).unwrap_or(0);
            order[at..].iter().map(|i| i + self.base).collect()
        } else {
            (from..len).collect()
        };
        ahead.into_iter().filter(|&i| !self.removed.get(i).copied().unwrap_or(false)).collect()
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

    /// Whether tracks are brought to one loudness by their ReplayGain. It is
    /// only applied with bit-perfect off: it changes the samples.
    pub fn normalize(&self) -> bool {
        self.preference(NORMALIZE).as_deref() == Some("true")
    }

    pub fn set_normalize(&mut self, on: bool) {
        self.remember_preference(NORMALIZE, &on.to_string());
        let (_, bit_perfect) = self.chosen_output();
        self.apply_playback_options(bit_perfect);
    }

    /// Whether the output is asked for alone with bit-perfect off too.
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
        let (output, bit_perfect) = self.chosen_output();
        if !bit_perfect {
            self.move_output(&output, bit_perfect);
        }
    }

    /// Hands the player the switches as they stand, for that policy.
    pub(crate) fn apply_playback_options(&mut self, bit_perfect: bool) {
        if let Some(p) = &self.player {
            p.set_gapless(self.gapless());
            p.set_normalize(self.normalize() && !bit_perfect);
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
        // Shuffling and the mix are two orders: one replaces the other.
        self.mix = false;
        let remaining = self.queue.len().saturating_sub(self.base);
        if let Some(p) = &self.player {
            if shuffle {
                p.shuffle(remaining);
            } else {
                p.clear_order();
            }
        }
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
        let available: Vec<LibraryTrack> =
            self.album_tracks(album_id).into_iter().filter(|t| t.present).collect();
        if available.is_empty() {
            self.now.error = chilpa_i18n::t("that album has no playable tracks");
            return Vec::new();
        }
        let tracks: Vec<std::path::PathBuf> =
            available.iter().map(|t| std::path::PathBuf::from(&t.path)).collect();
        self.queue = tracks.iter().map(|p| p.display().to_string()).collect();
        self.spawn(0, device, bit_perfect);
        available.into_iter().map(|t| t.title).collect()
    }
}
