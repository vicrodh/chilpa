//! The player: it plays on its own thread and reports what happens.
//!
//! The interface never blocks. It sends an order, goes on with its own
//! business and picks up events as they come. Stopping raises a flag that
//! the delivery loop checks between blocks, so it does not wait on the
//! device.

use chilpa_audio::negotiate::{DeviceCaps, Policy};
use chilpa_audio::render::Outcome;
use chilpa_audio::sink::{AudioSink, SinkError, StopSignal};
use chilpa_audio::format::StreamFormat;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use crate::playback::PlayError;

/// What the player tells whoever controls it.
#[derive(Clone, PartialEq, Debug)]
pub enum PlayerEvent {
    /// It started playing: what format the device was opened with and
    /// whether the delivery is bit-perfect.
    Started {
        path: PathBuf,
        opened: StreamFormat,
        /// The music's own channels. A mono track goes out in both channels
        /// of a stereo device, and `opened` then says two.
        channels: u16,
        /// How many bits carry the music, which is not always the width of
        /// the word `opened` names: see `negotiate::bits_carried`.
        bits: Option<u8>,
        bit_perfect: bool,
        total_frames: Option<u64>,
        /// What the file says about itself, when it could be read.
        tags: Option<chilpa_tags::Tags>,
        /// The cover already extracted to disk, when the file carries one.
        artwork: Option<PathBuf>,
        /// Its place in the queue, and out of how many.
        index: usize,
        queue_len: usize,
        /// For DSD: the multiple that names it (64, 128...). `None` on PCM.
        dsd_multiple: Option<u32>,
        /// True when that DSD leaves as one-bit data, either Native or DoP,
        /// rather than being converted to PCM.
        dsd_passthrough: bool,
    },
    /// Where the track is, in frames, and how many frames of it have gone
    /// out: after a seek the two differ, and the second is what was heard.
    Progress { frames: u64, played: u64 },
    /// What the station says is playing.
    StreamTitle { title: String },
    /// It finished naturally.
    Finished { frames: u64 },
    /// It stopped because someone asked.
    Stopped { frames: u64 },
    /// Something failed, in text the interface can show.
    Failed { message: String },
}

enum Order {
    /// Plays this request's queue. A queue belongs to one launch so a delayed
    /// order can never observe tracks from a newer click.
    Play {
        queue: crate::queue_playback::SharedQueue,
        generation: u64,
        device: String,
        /// Where the first track starts, when it is not at the beginning.
        ///
        /// It travels with the order instead of being asked for separately
        /// through `seek`, and that is the whole reason it exists. A caller
        /// that stops what is playing and then asks for a seek is racing the
        /// source it just told to die: the seek signal is one shared slot, and
        /// the old source reads it between blocks, so whichever got there
        /// first won. Carried here it cannot be taken by anything but the
        /// play it belongs to, because the thread arms it after the previous
        /// one has already returned.
        start_at: Option<std::time::Duration>,
        start_index: Option<usize>,
    },
    Stop(u64),
    Quit,
}

/// Opens the output device. It is injected so the tests use a fake one and
/// the application uses the real one.
/// Opens an output: its identifier, the format, and whether to ask for it
/// alone.
///
/// The last one is the user's bit-perfect switch reaching the device. A
/// player that always took the output exclusively would be right about
/// fidelity and wrong about everything else: turning bit-perfect off is
/// exactly what somebody does when they want to hear a video at the same
/// time, and it has to actually let go of the DAC.
pub type OpenSink =
    Box<dyn FnMut(&str, StreamFormat, bool) -> Result<Box<dyn AudioSink + Send>, SinkError> + Send>;

/// Measures a device by its name.
pub type ProbeDevice = Box<dyn FnMut(&str) -> Result<DeviceCaps, SinkError> + Send>;

/// The player, as the interface sees it.
pub struct Player {
    orders: Sender<Order>,
    events: Receiver<PlayerEvent>,
    controls: crate::queue_playback::Controls,
    /// The queue that is playing. It is shared with the thread so things
    /// can be added without cutting off what is playing.
    queue: Arc<Mutex<crate::queue_playback::SharedQueue>>,
    /// Monotonically identifies the newest play/stop request. Delayed play
    /// orders are discarded instead of replaying the newest queue twice.
    generation: Arc<AtomicU64>,
    thread: Option<JoinHandle<()>>,
    settled: Arc<AtomicU64>,
}

impl Player {
    /// Starts the player thread.
    pub fn spawn(mut probe: ProbeDevice, mut open: OpenSink) -> Self {
        let (orders_tx, orders_rx) = channel::<Order>();
        let (events_tx, events_rx) = channel::<PlayerEvent>();
        let controls = crate::queue_playback::Controls::new();
        let controls_thread = controls.clone();
        let queue = Arc::new(Mutex::new(crate::queue_playback::SharedQueue::default()));
        let generation = Arc::new(AtomicU64::new(0));
        let generation_thread = Arc::clone(&generation);
        let settled = Arc::new(AtomicU64::new(0));
        let settled_thread = settled.clone();
        let stop_thread = controls.stop.clone();
        let skip_thread = controls.skip.clone();
        let seek_thread = controls.seek.clone();

        let thread = std::thread::Builder::new()
            .name("chilpa-player".into())
            .spawn(move || {
                chilpa_audio::diagnostics::trace("player", "thread_started", format_args!(""));
                while let Ok(order) = orders_rx.recv() {
                    match order {
                        Order::Quit => {
                            chilpa_audio::diagnostics::trace("player", "order_received", format_args!("kind=quit"));
                            break;
                        }
                        Order::Stop(stamp) => {
                            settled_thread.store(stamp, Ordering::Release);
                            chilpa_audio::diagnostics::trace("player", "order_received", format_args!("kind=stop barrier=passed"));
                        }
                        Order::Play { queue, generation, device, start_at, start_index } => {
                            let latest = generation_thread.load(Ordering::Acquire);
                            if generation != latest {
                                chilpa_audio::diagnostics::trace(
                                    "player",
                                    "order_discarded",
                                    format_args!("kind=play generation={generation} latest={latest}"),
                                );
                                continue;
                            }
                            chilpa_audio::diagnostics::trace(
                                "player",
                                "order_received",
                                format_args!(
                                    "kind=play device={device:?} start_at={start_at:?} queue_len={} paused={}",
                                    queue.len(),
                                    controls_thread.pause.is_paused()
                                ),
                            );
                            // Every track starts with the flag clear: an old
                            // stop order does not kill the next one. The
                            // pause is not touched here: it was set when the
                            // order was sent (`play_all_from_paused`).
                            controls_thread.handoff.set(false);
                            stop_thread.reset();
                            skip_thread.take();
                            // Whatever seek was left pending belonged to the
                            // play that just ended, and serving it to the new
                            // one would drop somebody into the middle of a
                            // track they just chose from the start.
                            let _ = seek_thread.take();
                            if let Some(at) = start_at {
                                seek_thread.request(at);
                            }
                            controls_thread.start_index.store(start_index.unwrap_or(usize::MAX), Ordering::Release);
                            let started = std::time::Instant::now();
                            let result = play(
                                &queue,
                                &device,
                                &controls_thread,
                                &mut probe,
                                &mut open,
                                &events_tx,
                            );
                            chilpa_audio::diagnostics::trace(
                                "player",
                                "play_returned",
                                format_args!("after={:?} result={result:?}", started.elapsed()),
                            );
                            let event = match result {
                                Ok(Outcome::Finished { frames }) => PlayerEvent::Finished { frames },
                                Ok(Outcome::Stopped { frames }) => PlayerEvent::Stopped { frames },
                                Ok(Outcome::Failed { frames, error }) => PlayerEvent::Failed {
                                    message: chilpa_i18n::t_args(
                    "the device failed after {} frames: {}",
                    &[&frames.to_string(), &error.to_string()],
                ),
                                },
                                Ok(Outcome::Unreadable { reason, .. }) => PlayerEvent::Failed { message: reason },
                                Err(e) => PlayerEvent::Failed { message: e.to_string() },
                            };
                            let _ = events_tx.send(event);
                        }
                    }
                }
                chilpa_audio::diagnostics::trace("player", "thread_stopped", format_args!(""));
            })
            .expect("the player thread starts");

        Self { orders: orders_tx, events: events_rx, controls, queue, generation, thread: Some(thread), settled }
    }

    /// Asks to play a file. Returns immediately.
    pub fn play(&self, path: &Path, device: &str) {
        self.play_all(&[path.to_path_buf()], device);
    }

    /// Asks to play several tracks in order, one after another.
    pub fn play_all(&self, paths: &[PathBuf], device: &str) {
        self.play_all_from(paths, device, None);
    }

    /// The same, with the first track starting somewhere other than its
    /// beginning.
    ///
    /// This is what moving the music to another output is made of: the track
    /// is relaunched on the new device and picks up where it was. The caller
    /// still has to `stop` first -- orders are served in the order they
    /// arrive, and the one playing has to return before this one is read.
    pub fn play_all_from(
        &self,
        paths: &[PathBuf],
        device: &str,
        start_at: Option<std::time::Duration>,
    ) {
        self.play_all_from_paused(paths, device, start_at, false);
    }

    /// The same, starting paused when `paused` says so: moving a paused
    /// track to another output must not start it.
    pub fn play_all_from_paused(
        &self,
        paths: &[PathBuf],
        device: &str,
        start_at: Option<std::time::Duration>,
        paused: bool,
    ) {
        self.play_from_index(paths, device, start_at, paused, None, &[], None);
    }

    /// I launch the whole queue with an explicit cursor, keeping its earlier rows.
    #[allow(clippy::too_many_arguments)]
    pub fn play_from_index(
        &self, paths: &[PathBuf], device: &str,
        start_at: Option<std::time::Duration>, paused: bool, start_index: Option<usize>,
        removed: &[bool], stop_after: Option<usize>,
    ) {
        // The queue and the pause are set as the order is sent, not as the
        // thread takes it: what the caller does right after this call --
        // pause, play next, queue more -- comes after it, and the thread,
        // taking the order later, would have undone it. A disc image goes
        // in as its tracks, one entry each, like any album.
        self.controls.stop_after.set(stop_after);
        let queue = crate::queue_playback::SharedQueue::new(crate::sacd_tracks::expand(paths));
        for (index, removed) in removed.iter().enumerate() { if *removed { queue.remove(index); } }
        if let Ok(mut current) = self.queue.lock() {
            *current = queue.clone();
        }
        let generation = self.generation.fetch_add(1, Ordering::AcqRel).wrapping_add(1);
        self.controls.pause.set(paused);
        chilpa_audio::diagnostics::trace(
            "player",
            "order_sent",
            format_args!(
                "kind=play device={device:?} tracks={} first={:?} start_at={start_at:?} paused={paused}",
                queue.len(),
                queue.get(0)
            ),
        );
        let _ = self.orders.send(Order::Play {
            queue,
            generation,
            device: device.to_string(),
            start_at,
            start_index,
        });
    }

    /// Pauses or resumes. The track does not lose its place.
    pub fn set_paused(&self, paused: bool) {
        chilpa_audio::diagnostics::trace("player", "pause_requested", format_args!("paused={paused}"));
        self.controls.pause.set(paused);
    }

    pub fn is_paused(&self) -> bool {
        self.controls.pause.is_paused()
    }

    /// Skips to the next track in the queue. The current one ends as if it
    /// had finished, so the queue advances instead of stopping.
    pub fn next(&self) {
        chilpa_audio::diagnostics::trace("player", "skip_requested", format_args!(""));
        self.controls.skip.skip();
    }

    /// Asks to seek to that point of the playing track. The seek is handled
    /// between blocks; asking does not block.
    pub fn seek(&self, position: std::time::Duration) {
        chilpa_audio::diagnostics::trace("player", "seek_requested", format_args!("position={position:?}"));
        self.controls.seek.request(position);
    }

    /// Changes repeat. It takes effect when the playing track ends.
    pub fn set_repeat(&self, repeat: crate::queue_playback::Repeat) {
        self.controls.repeat.set(repeat);
    }

    /// Stops after this position in the player's queue, or clears the mark.
    pub fn set_stop_after(&self, index: Option<usize>) {
        self.controls.stop_after.set(index);
    }

    /// Gapless on or off, from the next track on.
    pub fn set_gapless(&self, on: bool) {
        self.controls.gapless.set(on);
    }

    /// Normalizing by ReplayGain on or off, from the next track on.
    pub fn set_normalize(&self, on: bool) {
        self.controls.normalize.set(on);
    }

    /// What the samples are multiplied by, heard as soon as what the device
    /// holds plays out. Raw DoP is never scaled.
    pub fn set_volume(&self, factor: f32) {
        self.controls.volume.set(factor);
    }

    pub fn volume(&self) -> f32 {
        self.controls.volume.get()
    }

    /// Whether the output is asked for alone, from the next time it opens.
    pub fn set_exclusive(&self, on: bool) {
        self.controls.exclusive.set(on);
    }

    /// Chooses explicitly how one-bit DSD leaves Chilpa.
    pub fn set_dsd_mode(&self, mode: crate::queue_playback::DsdMode) {
        self.controls.dsd_mode.set(mode);
    }

    pub fn repeat(&self) -> crate::queue_playback::Repeat {
        self.controls.repeat.get()
    }

    /// Adds tracks to the end of the playing queue, without cutting it off.
    ///
    /// Returns how many are left in the queue. If nothing is playing, the
    /// shared queue is empty and the caller decides whether to start it.
    pub fn enqueue(&self, paths: &[PathBuf]) -> usize {
        self.queue.lock().map(|queue| queue.extend(paths)).unwrap_or(0)
    }

    /// Removes a track from the playing queue, by its position within what
    /// was given to the player. The one already playing is not removed.
    pub fn remove_from_queue(&self, index: usize) -> bool {
        self.queue.lock().map(|queue| queue.remove(index)).unwrap_or(false)
    }

    /// How many tracks the playing queue carries.
    pub fn queue_len(&self) -> usize {
        self.queue.lock().map(|queue| queue.len()).unwrap_or(0)
    }

    /// Shuffles the playing queue. It takes effect when the track ends.
    pub fn shuffle(&self, len: usize) {
        self.controls.order.set(crate::queue_playback::shuffled_now(len), len);
    }

    /// Sets a traversal order for the playing queue.
    pub fn set_order(&self, order: Vec<usize>, len: usize) -> bool {
        self.controls.order.set(order, len)
    }

    /// The traversal order in use; empty when it is the queue's own.
    pub fn order(&self) -> Vec<usize> {
        self.controls.order.get()
    }

    /// Goes back to the order things were queued in.
    pub fn clear_order(&self) {
        self.controls.order.clear();
    }

    /// Asks to stop. It does not wait on the device.
    pub fn stop(&self) {
        chilpa_audio::diagnostics::trace("player", "stop_requested", format_args!(""));
        self.controls.handoff.set(false);
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.controls.stop.stop();
        let _ = self.orders.send(Order::Stop(self.generation.load(Ordering::Acquire)));
    }

    /// Stops because another playback request is about to replace this one.
    /// The ALSA sink may keep the reservation across that short hand-off.
    pub fn stop_for_handoff(&self) {
        chilpa_audio::diagnostics::trace("player", "handoff_requested", format_args!(""));
        self.controls.handoff.set(true);
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.controls.stop.stop();
        let _ = self.orders.send(Order::Stop(self.generation.load(Ordering::Acquire)));
    }

    /// Whether the worker has acknowledged the latest stop barrier.
    pub fn idle(&self) -> bool {
        self.settled.load(Ordering::Acquire) == self.generation.load(Ordering::Acquire)
    }

    /// Whatever events there are, without blocking.
    pub fn poll(&self) -> Vec<PlayerEvent> {
        let mut out = Vec::new();
        loop {
            match self.events.try_recv() {
                Ok(e) => out.push(e),
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            }
        }
        out
    }
    /// I stop and join while keeping the final delivery events available to poll.
    pub fn shutdown(&mut self) {
        if self.thread.is_none() { return; }
        self.controls.handoff.set(false);
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.controls.stop.stop();
        let _ = self.orders.send(Order::Quit);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Player {
    fn drop(&mut self) { self.shutdown(); }
}

fn play(
    queue: &crate::queue_playback::SharedQueue,
    device: &str,
    controls: &crate::queue_playback::Controls,
    probe: &mut ProbeDevice,
    open: &mut OpenSink,
    events: &Sender<PlayerEvent>,
) -> Result<Outcome, PlayError> {
    let exclusive = controls.exclusive.get();
    // What the samples meet after Chilpa hands them over. The delivery
    // cannot see that far, and a verdict that ignores it says bit-perfect
    // over a resampler.
    let exact_path = crate::path_is_exact(device, exclusive);
    let (stop, pause) = (&controls.stop, &controls.pause);
    let paths = queue.to_vec();
    let paths = paths.as_slice();
    chilpa_audio::diagnostics::trace(
        "player",
        "probe_started",
        format_args!("device={device:?} exclusive={exclusive} tracks={}", paths.len()),
    );
    let probe_started = std::time::Instant::now();
    let caps = match probe(device) {
        Ok(caps) => {
            chilpa_audio::diagnostics::trace(
                "player",
                "probe_finished",
                format_args!(
                    "after={:?} rates={:?} formats={:?} channels={:?} probed={}",
                    probe_started.elapsed(),
                    caps.sample_rates,
                    caps.formats,
                    caps.channels,
                    caps.probed
                ),
            );
            caps
        }
        Err(error) => {
            chilpa_audio::diagnostics::trace(
                "player",
                "probe_failed",
                format_args!("after={:?} error={error}", probe_started.elapsed()),
            );
            return Err(PlayError::Sink(error));
        }
    };
    // Fidelity is reported from the route and the transformations that
    // actually happened. It is not a promise that may prevent playback.
    let policy = Policy::AllowAnything;
    let direct_dsd = crate::backends::resolve(device)
        .is_some_and(|(stack, _)| stack.kind() == chilpa_audio::backend::BackendKind::Alsa);
    controls.direct_dsd.set(direct_dsd);
    chilpa_audio::diagnostics::trace(
        "player",
        "route_resolved",
        format_args!(
            "device={device:?} exact_path={exact_path} policy={policy:?} dsd_mode={:?} direct_dsd={direct_dsd}",
            controls.dsd_mode.get()
        ),
    );
    let total = paths.len();

    let index = controls.start_index.load(Ordering::Acquire).min(paths.len().saturating_sub(1));
    let index = if controls.start_index.load(Ordering::Acquire) == usize::MAX { 0 } else { index };
    if let Some(first) = paths.get(index) {
        if is_url(first) {
            return play_stream(first, device, &caps, policy, exclusive, stop, pause, open, events, index, total);
        }
        if is_cd(first) {
            return play_cd(first, device, exclusive, &caps, controls, open, events);
        }
    }
    let results = crate::queue_playback::play_queue_told(
        queue,
        &caps,
        policy,
        controls,
        |format| open(device, format, exclusive),
        &mut |index, path, delivery, about| {
            chilpa_audio::diagnostics::trace(
                "player",
                "track_started",
                format_args!(
                    "index={index}/{total} path={path:?} requested={:?} opened={:?} transforms={:?} exact_path={exact_path}",
                    delivery.requested,
                    delivery.opened,
                    delivery.transforms
                ),
            );
            // Tags and the cover are read as each track starts, not as it is
            // queued: a long queue must not read hundreds of files before the
            // first one plays.
            // A track of a disc image is named by the disc, and its cover is
            // the one beside the image.
            if let Some((image, number)) = crate::sacd_tracks::parse(path) {
                let about = crate::sacd_tracks::about(&image, number);
                let _ = events.send(PlayerEvent::Started {
                    path: path.to_path_buf(),
                    opened: delivery.opened,
                    channels: delivery.requested.channels,
                    bits: Some(1),
                    bit_perfect: delivery.nothing_was_converted() && exact_path,
                    // Counted in what opened: a disc played native runs
                    // half the frames of DoP, and was shown twice as long.
                    total_frames: about.as_ref().map(|(_, samples)| {
                        crate::dsd_playback::frames_opened(*samples, chilpa_sacd::DSD64_HZ, delivery.opened.sample_rate_hz)
                    }),
                    tags: about.map(|(tags, _)| tags),
                    artwork: chilpa_tags::artwork_for(&image, &crate::artwork_cache()),
                    index,
                    queue_len: total,
                    dsd_multiple: Some(64),
                    dsd_passthrough: !delivery
                        .transforms
                        .contains(&chilpa_audio::negotiate::Transform::DsdToPcm),
                });
                return;
            }
            if let Some(entry) = crate::remote::entry(path) {
                // Its tags are on the server; the library's copy of them
                // stands in, and the length comes from the decoder.
                let (about, frames) = crate::remote::describe(path, &entry);
                let _ = events.send(PlayerEvent::Started {
                    path: path.to_path_buf(),
                    opened: delivery.opened,
                    channels: delivery.requested.channels,
                    bits: delivery.bits_carried(),
                    bit_perfect: delivery.nothing_was_converted() && exact_path,
                    total_frames: frames,
                    tags: Some(about.tags),
                    artwork: about.artwork,
                    index,
                    queue_len: total,
                    dsd_multiple: None,
                    dsd_passthrough: false,
                });
                return;
            }
            // What the track says of itself was read as it was opened, and
            // ahead of its turn when it was opened ahead: nothing is read
            // from the file here, while the device waits for its first block.
            // A DSD track is named by its source rate. Its frame count follows
            // what actually opened: DoP carries sixteen bits per PCM frame;
            // converted DSD counts ordinary PCM frames.
            let dsd = &about.dsd;
            let total_frames = match dsd {
                Some(info) => Some(crate::dsd_playback::frames_opened(info.samples, info.dsd_rate_hz, delivery.opened.sample_rate_hz)),
                None => about.frames,
            };
            let _ = events.send(PlayerEvent::Started {
                path: path.to_path_buf(),
                opened: delivery.opened,
                channels: delivery.requested.channels,
                bits: if dsd.is_some() { Some(1) } else { delivery.bits_carried() },
                bit_perfect: delivery.nothing_was_converted() && exact_path,
                total_frames,
                tags: about.tags.clone(),
                artwork: about.artwork.clone(),
                index: index,
                queue_len: total,
                dsd_multiple: dsd.as_ref().map(|info| info.multiple()),
                dsd_passthrough: dsd.is_some()
                    && !delivery.transforms.contains(&chilpa_audio::negotiate::Transform::DsdToPcm),
            });
        },
        &mut |_, progress| {
            let _ = events.send(PlayerEvent::Progress { frames: progress.position, played: progress.delivered });
        },
    )?;

    // The end of the queue is the end of its last track.
    Ok(results
        .last()
        .map(|r| r.outcome.clone())
        .unwrap_or(Outcome::Finished { frames: 0 }))
}

/// True if what was asked for is a URL and not a file.
fn is_url(path: &Path) -> bool {
    path.to_string_lossy().starts_with("http://") || path.to_string_lossy().starts_with("https://")
}

/// The parts of a track that came in several URLs.
///
/// A queue entry is one string, and a track in several parts is still one
/// track, so its parts travel joined by a space. A space cannot appear in a
/// URL -- it has to be written `%20` -- so splitting on whitespace can never
/// cut a URL in half, and a single URL comes back as a list of one.
fn parts_of(url: &Path) -> Vec<String> {
    url.to_string_lossy().split_whitespace().map(str::to_string).collect()
}

/// Plays a radio, or any stream over HTTP.
#[allow(clippy::too_many_arguments)]
fn play_stream(
    url: &Path,
    device: &str,
    caps: &chilpa_audio::negotiate::DeviceCaps,
    policy: Policy,
    exclusive: bool,
    stop: &StopSignal,
    pause: &chilpa_audio::sink::PauseSignal,
    open: &mut OpenSink,
    events: &Sender<PlayerEvent>,
    index: usize,
    queue_len: usize,
) -> Result<Outcome, PlayError> {
    let exact_path = crate::path_is_exact(device, exclusive);
    let parts = parts_of(url);
    let address = parts.first().cloned().unwrap_or_default();
    let stream = chilpa_stream::open_parts(&parts)
        .map_err(|e| PlayError::Sink(SinkError::Backend(e.to_string())))?;
    let titles = stream.now_playing();
    let station = stream.info().name.clone();

    let path = url.to_path_buf();
    let events = events.clone();
    let tag = chilpa_tags::Tags {
        title: station.clone().or_else(|| Some(address.clone())),
        album: Some("Radio".into()),
        ..chilpa_tags::Tags::default()
    };

    let played = crate::playback::play_stream(
        stream,
        caps,
        policy,
        stop,
        pause,
        |format| open(device, format, exclusive),
        &mut |delivery| {
            let _ = events.send(PlayerEvent::Started {
                path: path.clone(),
                opened: delivery.opened,
                channels: delivery.requested.channels,
                bits: delivery.bits_carried(),
                bit_perfect: delivery.nothing_was_converted() && exact_path,
                total_frames: None,
                tags: Some(tag.clone()),
                artwork: None,
                index,
                queue_len,
                dsd_multiple: None,
                dsd_passthrough: false,
            });
        },
        &mut |frames| {
            // The station title changes with every song; it is published
            // with the progress, which is the only thing that travels often.
            if let Some(title) = titles.get() {
                let _ = events.send(PlayerEvent::StreamTitle { title: title });
            }
            let _ = events.send(PlayerEvent::Progress { frames, played: frames });
        },
    )?;
    Ok(played.outcome)
}

/// A CD is asked for as `cdda:/dev/sr0`. The prefix is not decoration: a
/// bare device node is indistinguishable from a file, and opening the drive
/// by mistake startles anyone.
fn is_cd(path: &Path) -> bool {
    path.to_string_lossy().starts_with("cdda:")
}

/// The drive a `cdda:` path asks for.
pub fn cd_device_of(path: &Path) -> Option<PathBuf> {
    let s = path.to_string_lossy();
    let node = s.strip_prefix("cdda:")?;
    (!node.is_empty()).then(|| PathBuf::from(node))
}

#[cfg(target_os = "linux")]
fn play_cd(
    path: &Path,
    device: &str,
    exclusive: bool,
    caps: &chilpa_audio::negotiate::DeviceCaps,
    controls: &crate::queue_playback::Controls,
    open: &mut OpenSink,
    events: &Sender<PlayerEvent>,
) -> Result<Outcome, PlayError> {
    let node = cd_device_of(path)
        .ok_or_else(|| PlayError::Sink(SinkError::Backend(chilpa_i18n::t("the drive is missing"))))?;
    let mut unit = chilpa_cd::linux::LinuxDrive::open(&node)
        .map_err(|e| PlayError::Sink(SinkError::Backend(e.to_string())))?;
    let toc = crate::cd_playback::read_toc(&mut unit)
        .map_err(|e| PlayError::Sink(SinkError::Backend(e.to_string())))?;
    let tracks: Vec<chilpa_cd::Track> = toc.audio_tracks().copied().collect();
    if tracks.is_empty() {
        return Err(PlayError::Sink(SinkError::Backend(
            chilpa_i18n::t("the disc carries no audio tracks"),
        )));
    }
    let total = tracks.len();
    let album = chilpa_i18n::t_args("CD in {}", &[&node.display().to_string()]);
    // A CD is not converted on the way out of Chilpa: it goes out as 16/44.1
    // or widened, which keeps every sample, and if the output takes neither
    // it is not opened at all. That is half the answer. The other half is
    // where it went, and it used to be missing: through `default` this said
    // bit-perfect while the samples went into a mixer.
    let bit_perfect = crate::path_is_exact(device, exclusive);

    crate::cd_playback::play_disc(
        &mut unit,
        &tracks,
        caps,
        controls,
        |format| open(device, format, exclusive),
        &mut |index, delivery| {
            let track = &tracks[index];
            let _ = events.send(PlayerEvent::Started {
                path: path.to_path_buf(),
                opened: delivery.opened,
                channels: chilpa_cd::CDDA_FORMAT.channels,
                bits: delivery.bits_carried(),
                bit_perfect,
                // A CD is 44,100 frames per second, and its sectors are
                // already PCM.
                total_frames: Some(u64::from(track.sectors) * chilpa_cd::FRAMES_PER_SECTOR as u64),
                tags: Some(chilpa_tags::Tags {
                    title: Some(chilpa_i18n::t_args("Track {}", &[&track.number.to_string()])),
                    album: Some(album.clone()),
                    ..chilpa_tags::Tags::default()
                }),
                artwork: None,
                index,
                queue_len: total,
                dsd_multiple: None,
                dsd_passthrough: false,
            });
        },
        &mut |_, progress| {
            let _ = events.send(PlayerEvent::Progress { frames: progress.position, played: progress.delivered });
        },
        // An unreadable sector cuts the track, and it is said instead of
        // padding with silence.
        &mut |index, problem| {
            let _ = events.send(PlayerEvent::Failed {
                message: chilpa_i18n::t_args("track {}: {}", &[&tracks[index].number.to_string(), &problem]),
            });
        },
    )
    .map_err(|e| PlayError::Sink(SinkError::Backend(e.to_string())))
}

#[cfg(not(target_os = "linux"))]
fn play_cd(
    _path: &Path,
    _device: &str,
    _exclusive: bool,
    _caps: &chilpa_audio::negotiate::DeviceCaps,
    _controls: &crate::queue_playback::Controls,
    _open: &mut OpenSink,
    _events: &Sender<PlayerEvent>,
) -> Result<Outcome, PlayError> {
    Err(PlayError::Sink(SinkError::Backend(
        chilpa_i18n::t("CDs are not read outside Linux yet"),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cd_path_is_recognized_by_its_prefix() {
        assert!(is_cd(Path::new("cdda:/dev/sr0")));
        // A bare device node is NOT a CD: opening the drive by mistake
        // startles anyone.
        assert!(!is_cd(Path::new("/dev/sr0")));
        assert!(!is_cd(Path::new("/music/one.flac")));
    }

    #[test]
    fn a_cd_path_yields_its_drive() {
        assert_eq!(cd_device_of(Path::new("cdda:/dev/sr0")), Some(PathBuf::from("/dev/sr0")));
        assert_eq!(cd_device_of(Path::new("cdda:")), None, "with no drive there is nothing to open");
        assert_eq!(cd_device_of(Path::new("/dev/sr0")), None);
    }

    use chilpa_audio::format::SampleFormat;
    use std::io::Write;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    struct NullSink {
        stream: StreamFormat,
        written: Arc<AtomicU64>,
    }

    impl AudioSink for NullSink {
        fn format(&self) -> StreamFormat {
            self.stream
        }
        fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
            let n = frames.len() / self.stream.frame_bytes();
            self.written.fetch_add(n as u64, Ordering::Relaxed);
            Ok(n)
        }
        fn close(&mut self) -> Result<(), SinkError> {
            Ok(())
        }
    }

    struct ClosingGateSink {
        stream: StreamFormat,
        closing: Arc<AtomicBool>,
        release: Arc<AtomicBool>,
    }

    impl AudioSink for ClosingGateSink {
        fn format(&self) -> StreamFormat {
            self.stream
        }
        fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
            Ok(frames.len() / self.stream.frame_bytes())
        }
        fn close(&mut self) -> Result<(), SinkError> {
            self.closing.store(true, Ordering::Release);
            while !self.release.load(Ordering::Acquire) {
                std::thread::yield_now();
            }
            Ok(())
        }
    }

    fn wav(name: &str, frames: u32) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("chilpa-player-{name}.wav"));
        let data = frames * 4;
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(b"RIFF").unwrap();
        f.write_all(&(36 + data).to_le_bytes()).unwrap();
        f.write_all(b"WAVEfmt ").unwrap();
        f.write_all(&16u32.to_le_bytes()).unwrap();
        f.write_all(&1u16.to_le_bytes()).unwrap();
        f.write_all(&2u16.to_le_bytes()).unwrap();
        f.write_all(&44_100u32.to_le_bytes()).unwrap();
        f.write_all(&176_400u32.to_le_bytes()).unwrap();
        f.write_all(&4u16.to_le_bytes()).unwrap();
        f.write_all(&16u16.to_le_bytes()).unwrap();
        f.write_all(b"data").unwrap();
        f.write_all(&data.to_le_bytes()).unwrap();
        for i in 0..frames {
            let v = (i % 1000) as i16;
            f.write_all(&v.to_le_bytes()).unwrap();
            f.write_all(&(-v).to_le_bytes()).unwrap();
        }
        p
    }

    fn player(written: Arc<AtomicU64>) -> Player {
        Player::spawn(
            Box::new(|_| {
                Ok(DeviceCaps {
                    sample_rates: vec![44_100],
                    formats: vec![SampleFormat::S16],
                    channels: vec![2],
                    probed: true,
                })
            }),
            Box::new(move |_, stream, _exclusive| {
                Ok(Box::new(NullSink { stream, written: Arc::clone(&written) }) as Box<dyn AudioSink + Send>)
            }),
        )
    }

    fn wait<F: Fn(&[PlayerEvent]) -> bool>(p: &Player, ready: F) -> Vec<PlayerEvent> {
        let limit = Instant::now() + Duration::from_secs(5);
        let mut all = Vec::new();
        while Instant::now() < limit {
            all.extend(p.poll());
            if ready(&all) {
                return all;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        all
    }

    #[test]
    fn playing_does_not_block_and_reports_progress_and_end() {
        let written = Arc::new(AtomicU64::new(0));
        let p = player(Arc::clone(&written));
        let file = wav("basic", 5_000);

        let started = Instant::now();
        p.play(&file, "fake");
        assert!(started.elapsed() < Duration::from_millis(100), "play() returns immediately");

        let events = wait(&p, |e| e.iter().any(|x| matches!(x, PlayerEvent::Finished { .. })));
        assert!(events.iter().any(|e| matches!(e, PlayerEvent::Started { .. })));
        assert!(events.iter().any(|e| matches!(e, PlayerEvent::Progress { .. })));
        assert!(matches!(events.last(), Some(PlayerEvent::Finished { frames: 5_000 })));
        assert_eq!(written.load(Ordering::Relaxed), 5_000);
        let _ = std::fs::remove_file(file);
    }

    #[test]
    fn a_track_moved_while_paused_stays_paused_there() {
        let written = Arc::new(AtomicU64::new(0));
        let p = player(Arc::clone(&written));
        let file = wav("moved-paused", 5_000);
        p.set_paused(true);
        p.play_all_from_paused(&[file.clone()], "fake", None, true);
        std::thread::sleep(Duration::from_millis(300));
        assert!(p.is_paused(), "the pause went with the track");
        assert_eq!(written.load(Ordering::Relaxed), 0, "nothing played while paused");
        p.set_paused(false);
        wait(&p, |e| e.iter().any(|x| matches!(x, PlayerEvent::Finished { .. })));
        assert_eq!(written.load(Ordering::Relaxed), 5_000);
        let _ = std::fs::remove_file(file);
    }

    #[test]
    fn a_new_play_does_not_inherit_an_old_pause() {
        let written = Arc::new(AtomicU64::new(0));
        let p = player(Arc::clone(&written));
        let file = wav("fresh-play", 2_000);
        p.set_paused(true);
        p.play(&file, "fake");
        let events = wait(&p, |e| e.iter().any(|x| matches!(x, PlayerEvent::Finished { .. })));
        assert!(matches!(events.last(), Some(PlayerEvent::Finished { frames: 2_000 })));
        let _ = std::fs::remove_file(file);
    }

    #[test]
    fn rapid_replacements_discard_stale_play_orders_and_their_queues() {
        let closing = Arc::new(AtomicBool::new(false));
        let release = Arc::new(AtomicBool::new(false));
        let close_seen = Arc::clone(&closing);
        let close_release = Arc::clone(&release);
        let p = Player::spawn(
            Box::new(|_| Ok(DeviceCaps { sample_rates: vec![44_100], formats: vec![SampleFormat::S16], channels: vec![2], probed: true })),
            Box::new(move |_, stream, _| Ok(Box::new(ClosingGateSink { stream, closing: Arc::clone(&close_seen), release: Arc::clone(&close_release) }) as Box<dyn AudioSink + Send>)),
        );
        let a = wav("replacement-a", 400_000);
        let stale = wav("replacement-stale", 1_000);
        let latest = wav("replacement-latest", 1_000);

        p.play(&a, "fake");
        let first = wait(&p, |events| events.iter().any(|event| matches!(event, PlayerEvent::Started { path, .. } if path == &a)));
        p.stop_for_handoff();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !closing.load(Ordering::Acquire) {
            assert!(Instant::now() < deadline, "the first stream never began closing");
            std::thread::yield_now();
        }
        p.play(&stale, "fake");
        p.stop_for_handoff();
        p.play(&latest, "fake");
        release.store(true, Ordering::Release);

        let mut events = first;
        events.extend(wait(&p, |events| events.iter().any(|event| matches!(event, PlayerEvent::Finished { .. }))));
        let starts: Vec<PathBuf> = events.iter().filter_map(|event| match event {
            PlayerEvent::Started { path, .. } => Some(path.clone()),
            _ => None,
        }).collect();
        assert_eq!(starts, vec![a.clone(), latest.clone()]);

        for path in [a, stale, latest] {
            let _ = std::fs::remove_file(path);
        }
    }

    #[test]
    fn the_exclusive_switch_asks_for_the_output_alone() {
        let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen = Arc::clone(&asked);
        let p = Player::spawn(
            Box::new(|_| {
                Ok(DeviceCaps { sample_rates: vec![44_100], formats: vec![SampleFormat::S16], channels: vec![2], probed: true })
            }),
            Box::new(move |_, stream, exclusive| {
                seen.lock().unwrap().push(exclusive);
                Ok(Box::new(NullSink { stream, written: Arc::new(AtomicU64::new(0)) }) as Box<dyn AudioSink + Send>)
            }),
        );
        let file = wav("exclusive", 1_000);
        p.play(&file, "fake");
        wait(&p, |e| e.iter().any(|x| matches!(x, PlayerEvent::Finished { .. })));
        p.set_exclusive(true);
        p.play(&file, "fake");
        wait(&p, |e| e.iter().any(|x| matches!(x, PlayerEvent::Finished { .. })));
        assert_eq!(*asked.lock().unwrap(), vec![false, true], "shared first, alone once asked");
        let _ = std::fs::remove_file(file);
    }

    #[test]
    fn a_queue_plays_in_order_and_reports_each_track_position() {
        let written = Arc::new(AtomicU64::new(0));
        let p = player(Arc::clone(&written));
        let a = wav("cola-a", 1_000);
        let b = wav("cola-b", 2_000);
        p.play_all(&[a.clone(), b.clone()], "fake");

        let events = wait(&p, |e| e.iter().any(|x| matches!(x, PlayerEvent::Finished { .. })));
        let startups: Vec<(usize, usize)> = events
            .iter()
            .filter_map(|e| match e {
                PlayerEvent::Started { index, queue_len, .. } => Some((*index, *queue_len)),
                _ => None,
            })
            .collect();
        assert_eq!(startups, vec![(0, 2), (1, 2)], "both tracks, in order");
        assert_eq!(written.load(Ordering::Relaxed), 3_000, "every frame went out");
        let _ = std::fs::remove_file(a);
        let _ = std::fs::remove_file(b);
    }

    #[test]
    fn stop_after_ends_the_queue_at_the_marked_track() {
        let written = Arc::new(AtomicU64::new(0));
        let p = player(Arc::clone(&written));
        let a = wav("stop-after-a", 2_000);
        let b = wav("stop-after-b", 3_000);
        p.play_all_from_paused(&[a.clone(), b.clone()], "fake", None, true);
        p.set_stop_after(Some(0));
        p.set_paused(false);

        let events = wait(&p, |e| e.iter().any(|x| matches!(x, PlayerEvent::Finished { .. })));
        let startups = events.iter().filter(|e| matches!(e, PlayerEvent::Started { .. })).count();
        assert_eq!(startups, 1, "the second track never starts");
        assert_eq!(written.load(Ordering::Relaxed), 2_000, "the marked track finishes cleanly");
        let _ = std::fs::remove_file(a);
        let _ = std::fs::remove_file(b);
    }

    #[test]
    fn skipping_advances_the_queue_instead_of_stopping_it() {
        let written = Arc::new(AtomicU64::new(0));
        let p = player(Arc::clone(&written));
        // Long tracks: without skipping, the test would take a while to reach the end.
        let a = wav("skip-a", 400_000);
        let b = wav("skip-b", 1_000);
        p.play_all(&[a.clone(), b.clone()], "fake");

        // As soon as the first one starts, a skip is asked for. The events
        // are gathered from both waits: each one consumes what it picked up.
        let mut events =
            wait(&p, |e| e.iter().any(|x| matches!(x, PlayerEvent::Started { .. })));
        p.next();
        events.extend(wait(&p, |e| {
            e.iter().any(|x| matches!(x, PlayerEvent::Finished { .. }))
        }));

        let startups = events
            .iter()
            .filter(|e| matches!(e, PlayerEvent::Started { .. }))
            .count();
        assert!(startups >= 2, "the second track started: {startups} startups");
        assert!(
            written.load(Ordering::Relaxed) < 401_000,
            "the first one did not play whole"
        );
        let _ = std::fs::remove_file(a);
        let _ = std::fs::remove_file(b);
    }

    #[test]
    fn a_missing_file_is_reported_and_the_player_stays_alive() {
        let p = player(Arc::new(AtomicU64::new(0)));
        p.play(Path::new("/does/not/exist.flac"), "fake");
        let events = wait(&p, |e| e.iter().any(|x| matches!(x, PlayerEvent::Failed { .. })));
        assert!(matches!(events.last(), Some(PlayerEvent::Failed { .. })));

        let file = wav("after-failing", 1_000);
        p.play(&file, "fake");
        let events = wait(&p, |e| e.iter().any(|x| matches!(x, PlayerEvent::Finished { .. })));
        assert!(events.iter().any(|e| matches!(e, PlayerEvent::Finished { .. })), "it still accepts orders");
        let _ = std::fs::remove_file(file);
    }
}
