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
        route: crate::output_evidence::Route,
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
    /// A queue entry was passed over and the queue went on: it would not
    /// open, or it broke partway. `generation` is the play it belongs to, as
    /// `play_from_index` returned it: whoever started another play since can
    /// tell it is not about the queue in front of them. `index` is its place
    /// in the queue the player was given; `message` says why, in text the
    /// interface can show. `played` is how many frames of it went out before
    /// it broke, and `None` when it never opened: a track heard partway is a
    /// listen cut short.
    Skipped { generation: u64, index: usize, path: PathBuf, message: String, played: Option<u64> },
    /// Something went wrong that did not end the play: an unreadable sector
    /// cut a track of a disc, and the disc goes on. `attempt` is the track's
    /// turn in the play, counted from one; `message` is text the interface
    /// can show.
    Notice { attempt: u64, message: String },
}

/// Where a play says what happens, each event under the play's generation:
/// `Player::poll_tagged` hands it over with the event, so whoever started
/// another play since can tell an event is not about the one in front of them.
#[derive(Clone)]
struct Events {
    to: Sender<(u64, PlayerEvent)>,
    generation: u64,
}

impl Events {
    fn send(&self, event: PlayerEvent) -> Result<(), ()> {
        self.to.send((self.generation, event)).map_err(|_| ())
    }
}

enum Order {
    /// Plays a launch: its queue, its controls and where it starts are its
    /// own, so a delayed order can never observe tracks from a newer click,
    /// and a start position cannot be taken by anything but the play it
    /// belongs to.
    Play(crate::launch::LaunchRun),
    Stop(u64),
    Quit,
}

/// Which launches the player has taken and not seen end, and whether it
/// still takes any.
struct Admission {
    /// False once the thread is gone: a launch asked for then ends at once.
    open: bool,
    unsettled: Vec<Arc<crate::launch::Launch>>,
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
    events: Receiver<(u64, PlayerEvent)>,
    /// Where a launch asked of a player that closed says it ended.
    events_tx: Sender<(u64, PlayerEvent)>,
    /// The settings every launch shares. Its stop, seek, Next, stop mark and
    /// start are not used: each launch has its own.
    controls: crate::queue_playback::Controls,
    /// The queue that is playing. It is shared with the thread so things
    /// can be added without cutting off what is playing.
    queue: Arc<Mutex<crate::queue_playback::SharedQueue>>,
    /// Counts play and stop requests, for `idle`: the worker says which stop
    /// it has passed.
    generation: Arc<AtomicU64>,
    admission: Arc<Mutex<Admission>>,
    thread: Option<JoinHandle<()>>,
    settled: Arc<AtomicU64>,
}

impl Player {
    pub fn gain_active(&self) -> bool { self.controls.gain_active.load(Ordering::Acquire) }
    pub fn dsp(&self) -> chilpa_audio::dsp::Control { self.controls.dsp.clone() }
    /// Starts the player thread.
    pub fn spawn(mut probe: ProbeDevice, mut open: OpenSink) -> Self {
        let (orders_tx, orders_rx) = channel::<Order>();
        let (events_tx, events_rx) = channel::<(u64, PlayerEvent)>();
        let controls = crate::queue_playback::Controls::new();
        let queue = Arc::new(Mutex::new(crate::queue_playback::SharedQueue::default()));
        let generation = Arc::new(AtomicU64::new(0));
        let settled = Arc::new(AtomicU64::new(0));
        let settled_thread = settled.clone();
        let admission = Arc::new(Mutex::new(Admission { open: true, unsettled: Vec::new() }));
        let admission_thread = Arc::clone(&admission);

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
                        Order::Play(run) => {
                            let launch = Arc::clone(&run.launch);
                            let generation = launch.id.get();
                            // Stopped or replaced while it waited: it does
                            // not start, and letting go of it says it ended.
                            if launch.called_off() {
                                chilpa_audio::diagnostics::trace("player", "order_discarded", format_args!("kind=play generation={generation}"));
                                continue;
                            }
                            chilpa_audio::diagnostics::trace(
                                "player",
                                "order_received",
                                format_args!(
                                    "kind=play generation={generation} device={:?} queue_len={} paused={}",
                                    run.device,
                                    launch.queue.len(),
                                    launch.controls.pause.is_paused()
                                ),
                            );
                            launch.controls.gain_active.store(false, Ordering::Release);
                            let started = std::time::Instant::now();
                            // What is this play's alone: its events and the
                            // remote opens it asks for. Its stop, its seek,
                            // its stop mark and where it starts came with it.
                            let events = Events { to: run.events(), generation };
                            crate::remote::opening_for(Some(generation));
                            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                play(&launch, &run.device, &mut probe, &mut open, &events)
                            }));
                            let result = match result {
                                Ok(result) => result,
                                // A panic ends this play as a failure and the
                                // player with it: what it was in the middle
                                // of is not known to be whole.
                                Err(panic) => {
                                    chilpa_audio::diagnostics::trace("player", "play_panicked", format_args!("after={:?}", started.elapsed()));
                                    run.settle(PlayerEvent::Failed { message: panic_text(panic.as_ref()) });
                                    break;
                                }
                            };
                            chilpa_audio::diagnostics::trace(
                                "player",
                                "play_returned",
                                format_args!("after={:?} result={result:?}", started.elapsed()),
                            );
                            let event = ending_event(launch.take_verdict(), result);
                            run.settle(event);
                        }
                    }
                }
                // Closing. Every launch still waiting ends here, in the order
                // it was asked for, and none is taken after: under the lock a
                // launch is sent under, so none slips in behind.
                let mut admission = admission_thread.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                while let Ok(order) = orders_rx.try_recv() {
                    if let Order::Stop(stamp) = order {
                        settled_thread.store(stamp, Ordering::Release);
                    }
                }
                admission.open = false;
                drop(admission);
                chilpa_audio::diagnostics::trace("player", "thread_stopped", format_args!(""));
            })
            .expect("the player thread starts");

        Self { orders: orders_tx, events: events_rx, events_tx, controls, queue, generation, admission, thread: Some(thread), settled }
    }

    fn admission(&self) -> std::sync::MutexGuard<'_, Admission> {
        self.admission.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The launch asked for last that has not ended: what a seek, a Next or
    /// a stop mark is for.
    fn newest(&self) -> Option<Arc<crate::launch::Launch>> {
        self.admission().unsettled.iter().rev().find(|launch| !launch.settled()).cloned()
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
    /// Returns the play's generation, which every `Skipped` of it carries.
    #[allow(clippy::too_many_arguments)]
    pub fn play_from_index(
        &self, paths: &[PathBuf], device: &str,
        start_at: Option<std::time::Duration>, paused: bool, start_index: Option<usize>,
        removed: &[bool], stop_after: Option<usize>,
    ) -> u64 {
        // The queue and the pause are set as the order is sent, not as the
        // thread takes it: what the caller does right after this call --
        // pause, play next, queue more -- comes after it, and the thread,
        // taking the order later, would have undone it. A disc image goes
        // in as its tracks, one entry each, like any album.
        let queue = crate::queue_playback::SharedQueue::new(crate::sacd_tracks::expand(paths));
        for (index, removed) in removed.iter().enumerate() { if *removed { queue.remove(index); } }
        if let Ok(mut current) = self.queue.lock() {
            *current = queue.clone();
        }
        self.generation.fetch_add(1, Ordering::AcqRel);
        // What is this play's alone, beside the settings every play shares.
        let controls = crate::queue_playback::Controls {
            stop: StopSignal::new(),
            seek: Default::default(),
            skip: Default::default(),
            start_index: Arc::new(std::sync::atomic::AtomicUsize::new(start_index.unwrap_or(usize::MAX))),
            start_at: Arc::new(Mutex::new(start_at)),
            stop_after: crate::queue_playback::StopAfter::new(),
            handoff: crate::queue_playback::Switch::new(false),
            ..self.controls.clone()
        };
        controls.stop_after.set(stop_after);
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
        let launch = crate::launch::Launch::new(queue, controls);
        let generation = launch.id.get();
        let run = crate::launch::LaunchRun::new(Arc::clone(&launch), device.to_string(), self.events_tx.clone());
        // A launch that is not taken ends as it is let go, once the lock is
        // released: the player closed, or its thread is gone.
        let not_taken = {
            let mut admission = self.admission();
            admission.unsettled.retain(|earlier| !earlier.settled());
            // An earlier one that has not started will not: this is the
            // newest request.
            for earlier in &admission.unsettled {
                earlier.supersede();
            }
            if admission.open {
                admission.unsettled.push(launch);
                self.orders.send(Order::Play(run)).err().map(|refused| refused.0)
            } else {
                Some(Order::Play(run))
            }
        };
        drop(not_taken);
        generation
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
        if let Some(launch) = self.newest() {
            launch.controls.skip.skip();
        }
    }

    /// Asks to seek to that point of the playing track. The seek is handled
    /// between blocks; asking does not block.
    pub fn seek(&self, position: std::time::Duration) {
        chilpa_audio::diagnostics::trace("player", "seek_requested", format_args!("position={position:?}"));
        if let Some(launch) = self.newest() {
            launch.controls.seek.request(position);
        }
    }

    /// Changes repeat. It takes effect when the playing track ends.
    pub fn set_repeat(&self, repeat: crate::queue_playback::Repeat) {
        self.controls.repeat.set(repeat);
    }

    /// Stops after this position in the player's queue, or clears the mark.
    pub fn set_stop_after(&self, index: Option<usize>) {
        if let Some(launch) = self.newest() {
            launch.controls.stop_after.set(index);
        }
    }

    /// Gapless on or off, from the next track on.
    pub fn set_gapless(&self, on: bool) {
        self.controls.gapless.set(on);
    }

    /// Normalizing on or off, from the next track on.
    pub fn set_normalize(&self, on: bool) {
        self.controls.normalize.set(on);
    }

    /// The loudness tracks are brought to, in LUFS, and whether a boost
    /// stops short of clipping; from the next track on, as in QBZ.
    pub fn set_normalization(&self, target: f32, prevent_clipping: bool) {
        self.controls.normalize_target.set(target);
        self.controls.prevent_clipping.set(prevent_clipping);
    }

    /// What the samples are multiplied by, heard as soon as what the device
    /// holds plays out. Raw DoP is never scaled.
    pub fn set_volume(&self, factor: f32) {
        self.controls.volume.set(factor);
    }

    /// Where the frames decoded for each track go, for the seek bar's
    /// waveform.
    pub fn set_waveform_feed(&self, feed: std::sync::Arc<dyn chilpa_audio::seek_waveform::Feed>) {
        if let Ok(mut slot) = self.controls.waveform.lock() {
            *slot = Some(feed);
        }
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
        self.call_off(false);
    }

    /// Stops because another playback request is about to replace this one.
    /// The ALSA sink may keep the reservation across that short hand-off.
    pub fn stop_for_handoff(&self) {
        chilpa_audio::diagnostics::trace("player", "handoff_requested", format_args!(""));
        self.call_off(true);
    }

    /// Stops every launch that has not ended, the ones still waiting too:
    /// each has its own stop, which is never lowered again.
    fn call_off(&self, handoff: bool) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        for launch in &self.admission().unsettled {
            launch.cancel(handoff);
        }
        let _ = self.orders.send(Order::Stop(self.generation.load(Ordering::Acquire)));
    }

    /// Whether the worker has acknowledged the latest stop barrier.
    pub fn idle(&self) -> bool {
        self.settled.load(Ordering::Acquire) == self.generation.load(Ordering::Acquire)
    }

    /// Whatever events there are, without blocking.
    pub fn poll(&self) -> Vec<PlayerEvent> {
        self.poll_tagged().into_iter().map(|(_, event)| event).collect()
    }

    /// The same, each with the generation of the play that said it, as
    /// `play_from_index` returned it.
    pub fn poll_tagged(&self) -> Vec<(u64, PlayerEvent)> {
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
        self.generation.fetch_add(1, Ordering::AcqRel);
        for launch in &self.admission().unsettled {
            launch.cancel(false);
        }
        let _ = self.orders.send(Order::Quit);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Player {
    fn drop(&mut self) { self.shutdown(); }
}

/// The one event a play ends with. An output that refused it said why
/// before it stopped, and that is how it ended, whatever the queue returned.
fn ending_event(verdict: Option<String>, result: Result<Outcome, PlayError>) -> PlayerEvent {
    match (verdict, result) {
        (Some(message), _) => PlayerEvent::Failed { message },
        (None, Ok(Outcome::Finished { frames })) => PlayerEvent::Finished { frames },
        (None, Ok(Outcome::Stopped { frames })) => PlayerEvent::Stopped { frames },
        (None, Ok(Outcome::Failed { frames, error })) => PlayerEvent::Failed {
            message: chilpa_i18n::t_args("the device failed after {} frames: {}", &[&frames.to_string(), &error.to_string()]),
        },
        (None, Ok(Outcome::Unreadable { reason, .. })) => PlayerEvent::Failed { message: reason },
        (None, Err(e)) => PlayerEvent::Failed { message: e.to_string() },
    }
}

/// What a panic said, in text the interface can show.
fn panic_text(panic: &(dyn std::any::Any + Send)) -> String {
    let said = panic.downcast_ref::<&str>().map(|said| said.to_string()).or_else(|| panic.downcast_ref::<String>().cloned());
    match said {
        Some(said) => format!("the player failed: {said}"),
        None => "the player failed".to_string(),
    }
}

fn play(
    launch: &crate::launch::Launch,
    device: &str,
    probe: &mut ProbeDevice,
    open: &mut OpenSink,
    events: &Events,
) -> Result<Outcome, PlayError> {
    let (queue, controls, generation) = (&launch.queue, &launch.controls, launch.id.get());
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
    // One-bit audio goes untouched on a card, as it always did, and on a
    // stack that converts on its own only with the output alone and against
    // what its wire takes untouched: converted, DoP is loud noise.
    let (direct_dsd, direct_caps) = match crate::backends::resolve(device) {
        Some((stack, _)) if stack.kind() == chilpa_audio::backend::BackendKind::Alsa => (true, None),
        Some((stack, bare)) if exclusive => match stack.untouched_caps(&bare) {
            Some(exact) => (true, Some(exact)),
            None => (false, None),
        },
        _ => (false, None),
    };
    controls.direct_dsd.set(direct_dsd);
    if let Ok(mut held) = controls.direct_caps.lock() {
        *held = direct_caps;
    }
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
            return play_stream(first, device, &caps, policy, exclusive, stop, pause, open, events, index, total, &controls.dsp);
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
            // The verdict, once: what Chilpa did to the samples, the road past
            // it, and the device as it opened.
            let bit_perfect = delivery.nothing_was_converted() && exact_path && crate::wire_keeps(device, delivery);
            chilpa_audio::diagnostics::trace(
                "player",
                "track_started",
                format_args!(
                    "index={index}/{total} path={path:?} requested={:?} opened={:?} transforms={:?} exact_path={exact_path} bit_perfect={bit_perfect}",
                    delivery.requested,
                    delivery.opened,
                    delivery.transforms
                ),
            );
            // One-bit audio in PCM words that the device, as it opened, does
            // not carry untouched reaches the DAC as loud noise. Nothing of it
            // is written: the queue stops here, before the first block, and
            // says why. Converting DSD to PCM is the choice that plays.
            let one_bit = (about.dsd.is_some() || crate::sacd_tracks::parse(path).is_some())
                && !delivery.transforms.contains(&chilpa_audio::negotiate::Transform::DsdToPcm);
            if one_bit && !bit_perfect {
                chilpa_audio::diagnostics::trace("player", "one_bit_refused", format_args!("path={path:?} opened={:?}", delivery.opened));
                // How this play ends, said once by the thread when it has
                // returned: it stops itself here.
                launch.refuse(chilpa_i18n::t(
                    "this output would not take DSD untouched, so none was sent; choose Convert to PCM for DSD playback",
                ));
                return;
            }
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
                    route: crate::output_evidence::route(device, delivery, exclusive),
                    channels: delivery.requested.channels,
                    bits: Some(1),
                    bit_perfect,
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
                let bits = about.bits;
                let (about, frames) = crate::remote::describe(path, &entry);
                let _ = events.send(PlayerEvent::Started {
                    path: path.to_path_buf(),
                    opened: delivery.opened,
                    route: crate::output_evidence::route(device, delivery, exclusive),
                    channels: delivery.requested.channels,
                    bits,
                    bit_perfect,
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
                    route: crate::output_evidence::route(device, delivery, exclusive),
                channels: delivery.requested.channels,
                bits: if dsd.is_some() { Some(1) } else { about.bits },
                bit_perfect,
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
        &mut |index, path, why, played| {
            chilpa_audio::diagnostics::trace("player", "track_passed_over", format_args!("index={index}/{total} path={path:?} played={played:?} why={why}"));
            let _ = events.send(PlayerEvent::Skipped { generation, index, path: path.to_path_buf(), message: why.to_string(), played });
        },
    )?;

    Ok(ending(results.last().map(|r| &r.outcome), controls.stop.is_stopped()))
}

/// How a queue that returned ended, from its last track and whether a stop
/// was asked for while it ran. A stop is how it ended, whatever the last
/// track said: whoever stopped it waits for `Stopped`, and a track passed
/// over or run out meanwhile is not the queue finishing. Otherwise a last
/// track that broke partway was passed over and said as it was: the queue
/// ran out after it. A device failure is said as it is.
fn ending(last: Option<&Outcome>, stopped: bool) -> Outcome {
    match last {
        Some(outcome @ (Outcome::Failed { .. } | Outcome::Stopped { .. })) => outcome.clone(),
        _ if stopped => Outcome::Stopped { frames: last.map_or(0, Outcome::frames) },
        Some(Outcome::Finished { frames } | Outcome::Unreadable { frames, .. }) => Outcome::Finished { frames: *frames },
        None => Outcome::Finished { frames: 0 },
    }
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
    events: &Events,
    index: usize,
    queue_len: usize,
    dsp: &chilpa_audio::dsp::Control,
) -> Result<Outcome, PlayError> {
    dsp.begin_stream();
    let exact_path = crate::path_is_exact(device, exclusive);
    let parts = parts_of(url);
    let address = parts.first().cloned().unwrap_or_default();
    let stream = chilpa_stream::open_parts(&parts)
        .map_err(|e| PlayError::Sink(SinkError::Backend(e.to_string())))?;
    let mut titles = stream.now_playing().changes();
    let station = stream.info().name.clone();

    let path = url.to_path_buf();
    let events = events.clone();
    let tag = chilpa_tags::Tags {
        title: station.clone().or_else(|| Some(address.clone())),
        album: Some("Radio".into()),
        ..chilpa_tags::Tags::default()
    };

    let played = crate::playback::play_stream_told(
        stream,
        caps,
        policy,
        stop,
        pause,
        Some(dsp.clone()),
        |format| open(device, format, exclusive),
        &mut |delivery, bits| {
            let _ = events.send(PlayerEvent::Started {
                path: path.clone(),
                opened: delivery.opened,
                    route: crate::output_evidence::route(device, delivery, exclusive),
                channels: delivery.requested.channels,
                bits,
                bit_perfect: delivery.nothing_was_converted() && exact_path && crate::wire_keeps(device, delivery) && !dsp.requested(),
                total_frames: None,
                tags: Some(tag.clone()),
                artwork: None,
                index,
                queue_len,
                dsd_multiple: None,
                dsd_passthrough: false,
            });
        },
        &mut |frames| publish_progress(&events, &mut titles, frames),
    )?;
    Ok(played.outcome)
}

/// Runs after every write to the output. The station title changes with every
/// song; it is looked at here, the only thing that travels often, and published
/// only when it changed: what listens to it goes to the network.
fn publish_progress(events: &Events, titles: &mut chilpa_stream::TitleChanges, frames: u64) {
    if let Some(title) = titles.next_change() {
        let _ = events.send(PlayerEvent::StreamTitle { title });
    }
    let _ = events.send(PlayerEvent::Progress { frames, played: frames });
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
    events: &Events,
) -> Result<Outcome, PlayError> {
    let node = cd_device_of(path)
        .ok_or_else(|| PlayError::Sink(SinkError::Backend(chilpa_i18n::t("the drive is missing"))))?;
    let mut unit = chilpa_cd::linux::LinuxDrive::open(&node)
        .map_err(|e| PlayError::Sink(SinkError::Backend(e.to_string())))?;
    play_cd_drive(&mut unit, &node, path, device, exclusive, caps, controls, open, events)
}

/// Plays the disc in a drive that is already open.
#[cfg(target_os = "linux")]
#[allow(clippy::too_many_arguments)]
fn play_cd_drive(
    unit: &mut dyn chilpa_cd::CdDevice,
    node: &Path,
    path: &Path,
    device: &str,
    exclusive: bool,
    caps: &chilpa_audio::negotiate::DeviceCaps,
    controls: &crate::queue_playback::Controls,
    open: &mut OpenSink,
    events: &Events,
) -> Result<Outcome, PlayError> {
    let toc = crate::cd_playback::read_toc(unit)
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
    let bit_perfect = crate::path_is_exact(device, exclusive) && !controls.dsp.requested();

    crate::cd_playback::play_disc(
        unit,
        &tracks,
        caps,
        controls,
        |format| open(device, format, exclusive),
        &mut |index, delivery| {
            let track = &tracks[index];
            let _ = events.send(PlayerEvent::Started {
                path: path.to_path_buf(),
                opened: delivery.opened,
                    route: crate::output_evidence::route(device, delivery, exclusive),
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
        // padding with silence. The disc goes on: this is no ending, and the
        // play still ends once.
        &mut |index, problem| {
            let _ = events.send(PlayerEvent::Notice {
                attempt: index as u64 + 1,
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
    _events: &Events,
) -> Result<Outcome, PlayError> {
    Err(PlayError::Sink(SinkError::Backend(
        chilpa_i18n::t("CDs are not read outside Linux yet"),
    )))
}

#[cfg(test)]
#[path = "player_tests.rs"]
mod tests;
