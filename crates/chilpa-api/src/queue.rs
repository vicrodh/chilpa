//! The queue: what plays, in what order and what comes next.
//!
//! Everything that sets a list going goes through `launch`, so that repeat,
//! shuffle and the position count are applied in one place.

use super::*;

impl Chilpa {
    /// Sets the queue already in `self.queue` going, from that position.
    ///
    /// Everything that plays a list goes through here: that way repeat,
    /// shuffle and the position count are applied in one place.
    pub(crate) fn spawn(&mut self, from: usize, device: &str, bit_perfect: bool) {
        let tracks: Vec<std::path::PathBuf> =
            self.queue.iter().skip(from).map(std::path::PathBuf::from).collect();
        if tracks.is_empty() {
            return;
        }
        self.base = from;
        self.removed = vec![false; self.queue.len()];
        let (shuffle, repeat, total) = (self.shuffle, self.repeat, self.queue.len());
        let tap = self.tap.clone();
        let player = self
            .player
            .get_or_insert_with(move || chilpa_core::spawn_player_with(tap));
        player.set_repeat(repeat);
        if shuffle {
            player.shuffle(tracks.len());
        } else {
            player.clear_order();
        }
        self.now = NowPlaying { playing: true, queue_len: total, ..NowPlaying::default() };
        player.play_all(&tracks, device, bit_perfect);
    }

    /// Starts what a search found playing, in that order.
    pub fn play_tracks(&mut self, paths: &[String], device: &str, bit_perfect: bool) -> Vec<String> {
        let tracks: Vec<std::path::PathBuf> =
            paths.iter().map(std::path::PathBuf::from).collect();
        if tracks.is_empty() {
            return Vec::new();
        }
        self.queue = tracks.iter().map(|p| p.display().to_string()).collect();
        self.spawn(0, device, bit_perfect);
        tracks.iter().map(|p| title_of(p)).collect()
    }

    /// The queue's titles, in the order they were queued.
    pub fn queue_titles(&self) -> Vec<String> {
        self.queue.iter().map(|r| title_of(std::path::Path::new(r))).collect()
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
        self.queue.clear();
        self.removed.clear();
        self.base = 0;
        self.now.queue_len = 0;
    }

    /// Adds tracks to the end of the queue. If nothing is playing, it starts.
    ///
    /// Queueing does not cut off what is playing: the queue grows behind it
    /// and what was added comes in when its turn arrives.
    pub fn enqueue(&mut self, paths: &[String], device: &str, bit_perfect: bool) -> Vec<String> {
        let tracks: Vec<std::path::PathBuf> =
            paths.iter().map(std::path::PathBuf::from).collect();
        if tracks.is_empty() {
            return Vec::new();
        }
        let playing = self.now.playing && self.player.is_some();
        if !playing {
            return self.play_tracks(paths, device, bit_perfect);
        }
        self.queue.extend(tracks.iter().map(|p| p.display().to_string()));
        self.removed.resize(self.queue.len(), false);
        if let Some(p) = &self.player {
            p.enqueue(&tracks);
        }
        self.now.queue_len = self.queue.len();
        tracks.iter().map(|p| title_of(p)).collect()
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
            self.now.error = "hace falta más cola para mezclar".into();
            return 0;
        }
        // Positions within what the player holds, not within the whole
        // queue.
        let paths: Vec<String> = self.queue.iter().skip(self.base).cloned().collect();
        let actual = self.now.index.saturating_sub(self.base);

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

        let con_tempo = bpms.iter().filter(|b| b.is_some()).count();
        if con_tempo < 3 {
            self.now.error = "casi nada de la cola tiene tempo analizado".into();
            return 0;
        }

        let order = order_by_tempo(&bpms, actual.min(paths.len().saturating_sub(1)));
        if let Some(p) = &self.player {
            p.set_order(order, paths.len());
        }
        // Mixing and shuffling are two forms of the same thing: they cannot
        // both be on.
        self.shuffle = false;
        con_tempo
    }

    /// Whether the queue plays shuffled.
    pub fn shuffle(&self) -> bool {
        self.shuffle
    }

    /// Shuffles the queue or returns it to its order. It takes effect when
    /// the playing track ends: shuffling does not cut the audio.
    pub fn set_shuffle(&mut self, shuffle: bool) {
        self.shuffle = shuffle;
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
                self.now.error = format!("no se pudo leer la lista: {e}");
                return Vec::new();
            }
        };
        if !list.missing.is_empty() {
            self.now.error = format!(
                "{} de las pistas de la lista ya no están",
                list.missing.len()
            );
        }
        if list.tracks.is_empty() {
            if self.now.error.is_empty() {
                self.now.error = "la lista no trae pistas".into();
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
            self.now.error = "no hay cola que guardar".into();
            return false;
        }
        let tracks: Vec<std::path::PathBuf> =
            self.queue.iter().map(std::path::PathBuf::from).collect();
        match chilpa_library::playlist::write_m3u(path, &tracks) {
            Ok(()) => true,
            Err(e) => {
                self.now.error = format!("no se pudo guardar la lista: {e}");
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
            self.now.error = "ese álbum no tiene pistas disponibles".into();
            return Vec::new();
        }
        let tracks: Vec<std::path::PathBuf> =
            available.iter().map(|t| std::path::PathBuf::from(&t.path)).collect();
        self.queue = tracks.iter().map(|p| p.display().to_string()).collect();
        self.spawn(0, device, bit_perfect);
        available.into_iter().map(|t| t.title).collect()
    }
}
