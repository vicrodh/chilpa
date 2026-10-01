//! I own the renderer session here; the window only draws and dispatches.
mod asset;
mod end;
mod worker;
mod picker;
use crate::{Chilpa, NowPlaying, Repeat, VolumeView};
use chilpa_cast::{Device, State, Status};
use std::{sync::{Arc, atomic::{AtomicBool, Ordering}, mpsc::{self, Receiver}}, time::{Duration, Instant}};
use worker::{Action, Event, Order, Worker};

#[derive(Default)]
pub(crate) struct Casting {
    worker: Option<Worker>, generation: u64, selected: Option<Device>, connected: bool,
    discovery_running: Option<Arc<AtomicBool>>, devices: Vec<Device>, discovery: Option<Receiver<Vec<Device>>>, picker: bool, searched: Option<Instant>,
    pub(crate) disconnecting: bool,
    pub(crate) local_start: Option<(usize, String, Option<Duration>)>,
    local_output: (String, String),
    status: Status, error: String, pub(crate) order: Vec<usize>, loaded: bool, loading: bool,
}
impl Casting {
    pub(crate) fn active(&self) -> bool { self.selected.is_some() }
    fn send(&self, order: Order) { if let Some(worker) = &self.worker { worker.send(self.generation, order); } }
    fn advance(&mut self) { self.generation = self.generation.wrapping_add(1); }
}
impl Chilpa {
    pub fn cast_picker(&mut self, open: bool) {
        self.casting.picker = open;
        if open {
            let (output, _) = self.chosen_output();
            self.casting.local_output = self.output_names(&output);
            self.cast_discover();
        }
        else {
            if let Some(running) = self.casting.discovery_running.take() { running.store(false, Ordering::Release); }
            self.casting.discovery = None;
            self.casting.searched = None;
        }
    }
    pub fn cast_discover(&mut self) {
        if self.casting.discovery.is_some() { return; }
        let (tx, rx) = mpsc::channel(); self.casting.discovery = Some(rx); self.casting.searched = Some(Instant::now());
        let running = Arc::new(AtomicBool::new(true)); self.casting.discovery_running = Some(running.clone());
        std::thread::spawn(move || {
            let cast_running = running.clone();
            let cast = std::thread::spawn(move || chilpa_cast::discovery::chromecast_until(Duration::from_secs(5), &cast_running).unwrap_or_default());
            let mut devices = chilpa_cast::discovery::dlna_until(Duration::from_secs(3), &running).unwrap_or_default();
            devices.extend(cast.join().unwrap_or_default()); devices.sort_by(|a,b| a.name.cmp(&b.name));
            let _ = tx.send(devices);
        });
    }
    pub fn cast_connect(&mut self, id: &str) {
        if let Some(device) = self.casting.devices.iter().find(|d| d.id == id).cloned() { self.cast_connect_device(device); }
    }
    /// I also accept an explicitly addressed renderer for protocol tests.
    pub fn cast_connect_device(&mut self, device: Device) {
        if self.casting.selected.as_ref().is_some_and(|selected| selected.id == device.id && selected.kind == device.kind) && self.casting.error.is_empty() { return; }
        self.casting.disconnecting = false; self.casting.local_start = None;
        let transfer = self.now.playing && !self.now.paused;
        self.casting.advance();
        if self.casting.worker.is_none() { self.casting.worker = Some(Worker::new()); }
        if let Some(player) = &self.player { player.stop(); }
        self.casting.order = self.player.as_ref().map(|p| {
            let order = p.order();
            if order.len() == self.queue.len().saturating_sub(self.base) {
                (0..self.base).chain(order.into_iter().map(|i| i + self.base)).collect()
            } else { (0..self.queue.len()).collect() }
        }).unwrap_or_else(|| if self.casting.order.len() == self.queue.len() { self.casting.order.clone() } else { (0..self.queue.len()).collect() });
        self.base = 0;
        let player = self.player.take();
        self.log_play(false);
        self.casting.selected = Some(device.clone()); self.casting.connected = false;
        self.casting.status = Status::default(); self.casting.error.clear(); self.casting.loaded = false;
        self.now.bit_perfect = false;
        self.casting.loading = false;
        self.casting.send(Order::Connect(device, player));
        if transfer && !self.queue.is_empty() {
            self.cast_load(self.now.index.min(self.queue.len()-1), self.now.seconds, false);
        } else { self.now.playing = false; }
    }
    pub fn cast_disconnect(&mut self) {
        self.casting.disconnecting = self.casting.worker.is_some();
        self.casting.local_start = None;
        self.casting.advance(); self.casting.send(Order::Disconnect);
        self.casting.selected = None; self.casting.connected = false; self.casting.loaded = false;
        self.casting.loading = false; self.casting.error.clear();
        self.now.playing = false; self.now.paused = false; self.now.bit_perfect = false;
    }
    pub fn cast_destination(&self) -> Option<String> { self.casting.selected.as_ref().map(|d| d.name.clone()) }
    pub(crate) fn cast_load(&mut self, index: usize, position: f64, paused: bool) {
        let Some(path) = self.queue.get(index).cloned() else { return };
        self.casting.advance(); self.casting.error.clear(); self.casting.loaded = false;
        self.casting.loading = true;
        if self.casting.order.len() != self.queue.len() { self.cast_order(); }
        self.now = NowPlaying { path: path.clone(), index, queue_len: self.queue.len(), seconds: position.max(0.0), playing: true, paused, ..Default::default() };
        self.casting.send(Order::Load(path, position.max(0.0), paused));
    }
    pub(crate) fn cast_pause(&mut self, paused: bool) {
        self.casting.error.clear(); self.now.error.clear();
        if !paused && !self.casting.loaded && !self.casting.loading {
            self.cast_load(self.now.index, self.now.seconds, false);
        } else if self.casting.loaded || self.casting.loading {
            self.casting.send(Order::Action(Action::Pause(paused)));
        }
        self.now.paused = paused;
    }
    pub(crate) fn cast_seek(&mut self, position: f64) {
        if !position.is_finite() { return; }
        self.casting.error.clear(); self.now.error.clear();
        self.now.seconds = position.max(0.0);
        if self.casting.loaded || self.casting.loading { self.casting.send(Order::Action(Action::Seek(self.now.seconds))); }
    }
    pub(crate) fn cast_stop(&mut self) {
        self.casting.advance(); self.casting.loaded = false; self.casting.loading = false; self.now.playing = false; self.now.paused = false; self.now.seconds = 0.0;
        self.casting.send(Order::Action(Action::Stop));
    }
    pub(crate) fn cast_order(&mut self) {
        self.casting.order = if self.shuffle { chilpa_core::queue_playback::shuffled(self.queue.len(), crate::now_unix() as u64) } else { (0..self.queue.len()).collect() };
    }
    pub(crate) fn cast_upcoming(&self) -> Vec<usize> {
        let at = self.casting.order.iter().position(|i| *i == self.now.index).unwrap_or(0);
        self.casting.order.iter().skip(at).copied().filter(|i| !self.removed.get(*i).copied().unwrap_or(false)).collect()
    }
    pub(crate) fn cast_next(&mut self, natural: bool) {
        if natural && self.stop_after == Some(self.now.index) { self.stop_after = None; self.cast_stop(); return; }
        if natural && self.repeat == Repeat::One { self.cast_load(self.now.index, 0.0, false); return; }
        if self.casting.order.len() != self.queue.len() { self.cast_order(); }
        let upcoming = self.cast_upcoming();
        let next = upcoming.iter().copied().find(|i| *i != self.now.index).or_else(|| if self.repeat == Repeat::All { self.casting.order.iter().copied().find(|i| !self.removed.get(*i).copied().unwrap_or(false)) } else { None });
        match next { Some(index) => self.cast_load(index, 0.0, false), None => self.cast_stop() }
    }
    pub(crate) fn cast_previous(&mut self) {
        let at = self.casting.order.iter().position(|i| *i == self.now.index).unwrap_or(0);
        let previous = self.casting.order[..at].iter().rev().copied().find(|i| !self.removed.get(*i).copied().unwrap_or(false));
        if let Some(index) = previous { self.cast_load(index, 0.0, false); }
        else { self.cast_seek(0.0); }
    }
    pub(crate) fn cast_volume(&self) -> VolumeView {
        VolumeView { percent: (self.casting.status.volume.unwrap_or(1.0) * 100.0).round() as u8, muted: self.casting.status.muted.unwrap_or(false), hardware: true, locked: self.casting.status.volume.is_none(), why: chilpa_i18n::t("Receiver volume") }
    }
    pub(crate) fn cast_set_volume(&mut self, percent: u8) {
        let level = f64::from(percent.min(100)) / 100.0;
        self.casting.status.volume = Some(level);
        self.casting.send(Order::Action(Action::Volume(level)));
    }
    pub(crate) fn cast_set_muted(&mut self, muted: bool) {
        self.casting.status.muted = Some(muted);
        self.casting.send(Order::Action(Action::Mute(muted)));
    }
    pub(crate) fn poll_cast(&mut self) {
        if let Some(rx) = &self.casting.discovery {
            match rx.try_recv() {
                Ok(devices) => { self.casting.devices = devices; self.casting.discovery = None; }
                Err(mpsc::TryRecvError::Disconnected) => self.casting.discovery = None,
                _ => {}
            }
        }
        if self.casting.picker && self.casting.searched.is_none_or(|t| t.elapsed() > Duration::from_secs(10)) { self.cast_discover(); }
        let events = self.casting.worker.as_ref().map(|w| w.poll()).unwrap_or_default();
        for (generation, event) in events {
            if generation != self.casting.generation { continue; }
            match event {
                Event::Connected => self.casting.connected = true,
                Event::Loaded(metadata, mime, presentation) => {
                    self.casting.connected = true; self.casting.loaded = true; self.casting.loading = false; self.casting.error.clear();
                    self.now.title = metadata.title; self.now.artist = metadata.artist; self.now.album = metadata.album;
                    self.now.total_seconds = metadata.duration; self.now.format = chilpa_i18n::t("Original bytes · no transcoding by Chilpa · receiver processing unknown");
                    self.now.quality = crate::Quality::of_track(&self.now.path, Some(mime.trim_start_matches("audio/")), presentation.bits, presentation.rate);
                    self.now.artwork_path = presentation.artwork.map(|p| p.display().to_string()).unwrap_or_default();
                    let path = std::path::PathBuf::from(&self.now.path);
                    self.name_from_source(&path);
                    self.catalog_context();
                    self.load_lyrics(&path);
                }
                Event::Status(status) => {
                    if self.casting.loaded {
                        self.now.playing = !matches!(status.state, State::Stopped | State::Finished);
                        self.now.paused = status.state == State::Paused;
                        if let Some(position) = status.position.filter(|p| p.is_finite()) { self.now.seconds = position.max(0.0); self.now.lyrics_line = self.line_at(self.now.seconds); }
                        if let Some(duration) = status.duration.filter(|d| d.is_finite() && *d > 0.0) { self.now.total_seconds = duration; }
                    }
                    self.casting.status = status;
                }
                Event::Ended => self.cast_next(true),
                Event::Failed(error) => { self.casting.error = error.clone(); self.now.error = error; self.now.playing = false; self.now.paused = false; self.casting.loaded = false; self.casting.loading = false; }
                Event::ControlFailed(error) => { self.casting.error = error.clone(); self.now.error = error; },
                Event::Disconnected => {
                    self.casting.connected = false; self.casting.selected = None;
                    self.casting.loading = false; self.casting.loaded = false; self.casting.disconnecting = false;
                    self.now.playing = false; self.now.paused = false;
                    if let Some((index, device, position)) = self.casting.local_start.take() {
                        self.spawn_from(index, &device, false, position, crate::Relaunch::Track);
                    }
                },
            }
        }
    }
    /// Fills what is missing about the current track from the catalog.
    pub(crate) fn catalog_context(&mut self) {
        let path = self.now.path.clone();
        let track = self.library().and_then(|lib| lib.tracks_by_paths(&[path]).ok()).and_then(|mut rows| rows.pop().flatten());
        let Some(track) = track else { return };
        self.now.album_id = track.album_id.unwrap_or_default();
        if self.now.total_seconds <= 0.0 { self.now.total_seconds = track.duration_ms as f64 / 1000.0; }
        if self.now.artwork_path.is_empty() { self.now.artwork_path = track.artwork_path.unwrap_or_default(); }
        if self.now.title.is_empty() { self.now.title = track.title; }
        if self.now.artist.is_empty() { self.now.artist = track.artist; }
        if self.now.album.is_empty() { self.now.album = track.album; }
        // I retain measured file quality and fill missing source metadata from
        // the catalog, just as the reference player's cast presentation does.
        self.now.quality = crate::Quality::of_track(&self.now.path, track.codec.as_deref().or(Some(&self.now.quality.format)),
            self.now.quality.bits.or(track.bit_depth), self.now.quality.rate_hz.or(track.sample_rate_hz));
    }
}

impl Drop for Casting {
    fn drop(&mut self) {
        if let Some(running) = &self.discovery_running { running.store(false, Ordering::Release); }
    }
}
