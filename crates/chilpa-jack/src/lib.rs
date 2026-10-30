//! The JACK graph, whoever is answering for it.
//!
//! On this machine there is no `jackd` and JACK works, because PipeWire
//! provides the client library. That is worth saying rather than leaving
//! somebody to believe they picked a shorter road, and `info` says who
//! replied.
//!
//! **This stack can never be bit-perfect, and the reason is what JACK is.**
//! A graph runs at one sample rate and every client runs at it. A client
//! does not choose; it is told, at the moment it connects. So a 44.1 track
//! on a graph running at 48 is resampled before it is anybody's decision,
//! and no setting here changes that. What JACK is for is routing audio
//! between programs, which is a different thing from delivering a file to a
//! DAC untouched.
//!
//! **The graph calls, it is not written to.** Every other stack here takes a
//! blocking write; JACK hands out a buffer on its own thread and expects it
//! filled before it returns. A ring between the two is what makes it look
//! like the others, and the ring is the only place the two threads meet.

use chilpa_audio::backend::{BackendInfo, BackendKind, BackendOutput, Fidelity, OutputBackend};
use chilpa_audio::format::{SampleFormat, StreamFormat};
use chilpa_audio::sink::{AudioSink, SinkError};
use jack::PortSpec as _;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

/// The JACK graph.
pub struct Jack;

/// What the client is called in the graph.
const CLIENT: &str = "Chilpa";

/// Asks the graph who it is, without joining it for longer than the answer
/// takes.
///
/// Opening a client is the only way to find out anything about JACK: there
/// is no file to read and no command that answers without one. It is only
/// ever called through [`ask`], which closes it where nobody waits, and it is
/// the one place here that connects without being asked to play.
fn hello() -> Option<(jack::Client, jack::ClientStatus)> {
    jack::Client::new(CLIENT, jack::ClientOptions::NO_START_SERVER).ok()
}

/// How long a question to the graph may take before it is given up on.
///
/// Two seconds, as the PipeWire stack gives `pactl`, and for the same reason:
/// the stack picker asks while somebody is looking at the window.
const PATIENCE: Duration = Duration::from_secs(2);

/// Why the graph gave no answer.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Silence {
    /// There is no graph to ask.
    Absent,
    /// There may be one, and it did not answer in time.
    Late,
    /// It was not asked: the previous question's client was still closing
    /// when the patience ran out. Not the same as absent, and never
    /// reported as it.
    Busy,
}

/// Threads of this crate still at work where nobody waits for them:
/// joining the graph, asking it something, or leaving it.
///
/// PipeWire's JACK library tears its loop down when the process exits, and a
/// thread still inside a close waits on that loop forever, so the exit never
/// finished: `pw_deinit` destroying a condition variable that
/// `jack_deactivate` was waiting on. Quitting the window right after playing
/// through JACK was enough. The process now waits for these threads, at most
/// [`PATIENCE`], before the library's teardown runs. A close takes about a
/// millisecond; one that never returns still hangs the exit, as it would
/// hang anything else.
static BUSY: AtomicUsize = AtomicUsize::new(0);

/// One of those threads, counted from before it starts until it is done.
struct Busy;

impl Busy {
    /// Counted before the thread is started, so an exit cannot slip in
    /// between the two.
    fn new() -> Self {
        static AT_EXIT: AtomicBool = AtomicBool::new(false);
        // Registered at run time, so on glibc it runs before the libraries'
        // own teardown, which the loader registered when the process
        // started. Elsewhere it is a wait of two seconds at most. A failed
        // registration is tried again by the next thread.
        if AT_EXIT.compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed).is_ok() {
            // SAFETY: `wait_for_busy` is a plain `extern "C" fn()` that
            // lives as long as the process and cannot unwind.
            if unsafe { libc::atexit(wait_for_busy) } != 0 {
                AT_EXIT.store(false, Ordering::Relaxed);
            }
        }
        BUSY.fetch_add(1, Ordering::Relaxed);
        Busy
    }
}

impl Drop for Busy {
    /// After the client is gone: what leaving did has to be seen by the
    /// teardown that waited for it.
    fn drop(&mut self) {
        BUSY.fetch_sub(1, Ordering::Release);
    }
}

extern "C" fn wait_for_busy() {
    let limit = std::time::Instant::now() + PATIENCE;
    while BUSY.load(Ordering::Acquire) > 0 && std::time::Instant::now() < limit {
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Runs `work` on a thread of its own and waits at most `patience` for what
/// it sends back.
///
/// **The thread is never joined.** Whatever it does after it has answered,
/// or instead of answering, nobody waits for, and that is the point. Under
/// PipeWire's JACK library, closing a client sometimes never returns
/// (`jack_client_close` waiting forever in `pw_thread_loop_stop`), and the
/// `jack` crate holds one lock over every client's creation and destruction
/// while it closes, so after one such close even opening a client waits
/// forever. Both hung whoever had asked: the player's thread, in one run for
/// six hours. Here a hang costs one idle thread.
fn within<T: Send + 'static>(
    patience: Duration,
    work: impl FnOnce(Sender<T>) + Send + 'static,
) -> Result<T, Silence> {
    let (answer, answered) = mpsc::channel();
    let busy = Busy::new();
    // A thread that cannot be started is a question that was not asked, and
    // of the two answers "late" is the nearer.
    std::thread::Builder::new()
        .name("chilpa-jack".into())
        .spawn(move || {
            work(answer);
            drop(busy);
        })
        .map_err(|_| Silence::Late)?;
    match answered.recv_timeout(patience) {
        Ok(value) => Ok(value),
        Err(RecvTimeoutError::Timeout) => Err(Silence::Late),
        // It gave up without answering, which only a failed connection does.
        Err(RecvTimeoutError::Disconnected) => Err(Silence::Absent),
    }
}

/// Whether a question to the graph is out: asked, and its thread not done
/// -- still connecting, still closing, or stuck closing for good. The
/// condition is told when it is done again.
static ASKING: Mutex<bool> = Mutex::new(false);
static ANSWERED: Condvar = Condvar::new();

/// The question that is out. Dropped when its thread is done, whichever way
/// it ends, and when no thread could be started for it.
struct Asking;

impl Drop for Asking {
    fn drop(&mut self) {
        *ASKING.lock().unwrap_or_else(|e| e.into_inner()) = false;
        ANSWERED.notify_all();
    }
}

/// Runs one question's `work` on a thread of its own, once the previous
/// question's thread is done: a question waits its turn, and the turn and
/// the answer together cost `patience` at most.
///
/// One at a time, because of what a stuck close does. After
/// `jack_client_close` hangs, every later client waits forever on the
/// `jack` crate's lock, so a question asked every few seconds -- the
/// outputs are listed on a timer -- would add a thread each time, waiting
/// behind the first one for as long as the process lives. A turn that never
/// comes costs the patience and no thread, and is said to be that
/// (`Busy`), not a graph that is absent.
///
/// Waited for and not refused, because the player asks twice in a row --
/// the stack's fidelity, then its capabilities -- and the first answer is
/// sent before its client closes: refused, the second question found the
/// close still under way a millisecond later and said no graph was running.
fn one_at_a_time<T: Send + 'static>(
    patience: Duration,
    work: impl FnOnce(Sender<T>) + Send + 'static,
) -> Result<T, Silence> {
    let deadline = std::time::Instant::now() + patience;
    let mut out = ASKING.lock().unwrap_or_else(|e| e.into_inner());
    while *out {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        if left.is_zero() {
            return Err(Silence::Busy);
        }
        out = ANSWERED.wait_timeout(out, left).unwrap_or_else(|e| e.into_inner()).0;
    }
    *out = true;
    drop(out);
    let asking = Asking;
    within(deadline.saturating_duration_since(std::time::Instant::now()), move |answer| {
        let _asking = asking;
        work(answer);
    })
}

/// Asks the graph one thing: connects, asks, answers, and only then closes,
/// on a thread nobody is waiting on. One question at a time, see
/// [`one_at_a_time`].
fn ask<T: Send + 'static>(
    question: impl FnOnce(&jack::Client) -> T + Send + 'static,
) -> Result<T, Silence> {
    one_at_a_time(PATIENCE, move |answer| {
        if let Some((client, _)) = hello() {
            let _ = answer.send(question(&client));
            drop(client);
        }
    })
}

/// Leaves the graph without waiting to have left. See [`within`].
fn let_go<C: Send + 'static>(client: C) {
    // If no thread can be started the closure is dropped here, on the
    // caller's thread. Wrapped, that leaks the client instead of closing it:
    // one left in the graph until Chilpa exits is better than a caller that
    // never comes back.
    let client = std::mem::ManuallyDrop::new(client);
    let busy = Busy::new();
    let _ = std::thread::Builder::new().name("chilpa-jack".into()).spawn(move || {
        drop(std::mem::ManuallyDrop::into_inner(client));
        drop(busy);
    });
}

/// What the graph accepts, which is one rate and nothing else.
///
/// Really measured: it is asked of the running graph, not assumed. Saying it
/// here rather than refusing inside `open` is what lets the machinery that
/// already exists do its job -- with bit-perfect on, a track at another rate
/// is refused and says why; with it off, it is converted to the graph's rate
/// and the window says converted. Neither of those is a decision this crate
/// has any business making on its own.
pub fn capabilities() -> Option<chilpa_audio::negotiate::DeviceCaps> {
    measured().ok()
}

/// The same, saying why there is no answer when there is none: a graph
/// that is absent and one that is still closing an earlier client are
/// different things, and only the first means nothing can play.
fn measured() -> Result<chilpa_audio::negotiate::DeviceCaps, Silence> {
    let rate = ask(|client| client.sample_rate() as u32)?;
    Ok(chilpa_audio::negotiate::DeviceCaps {
        sample_rates: vec![rate],
        // The graph deals in f32 and the conversion into it costs nothing;
        // what costs is the rate, and that is above.
        formats: vec![
            SampleFormat::S16,
            SampleFormat::S24Packed,
            SampleFormat::S24In32,
            SampleFormat::S32,
            SampleFormat::F32,
        ],
        channels: vec![1, 2, 4, 6, 8],
        probed: true,
    })
}

impl OutputBackend for Jack {
    fn inspect(&self, id: &str) -> chilpa_audio::evidence::Inspection {
        capabilities().map(|caps| chilpa_audio::evidence::Inspection::convertible("jack",id,format!("{:?}",caps.sample_rates),&caps))
            .unwrap_or_else(|| chilpa_audio::evidence::Inspection::unavailable("jack",id,String::new()))
    }
    fn kind(&self) -> BackendKind {
        BackendKind::Jack
    }

    fn info(&self) -> BackendInfo {
        let rate = match ask(|client| client.sample_rate()) {
            Ok(rate) => rate,
            Err(Silence::Absent) => {
                return BackendInfo::missing(
                    BackendKind::Jack,
                    chilpa_i18n::t(
                        "no JACK graph is running; start one, or install pipewire-jack to have \
                         PipeWire answer for it",
                    ),
                )
            }
            // Busy is late by another road: the previous question's client
            // was still closing when the patience ran out. Said as late,
            // never as absent: there may well be a graph.
            Err(Silence::Late | Silence::Busy) => {
                return BackendInfo::missing(
                    BackendKind::Jack,
                    chilpa_i18n::t("the JACK graph did not answer in time"),
                )
            }
        };
        // Nothing is claimed about who is answering. On most machines it
        // is PipeWire's replacement library rather than jack2, and it would
        // be worth saying -- but the only signals available are the shapes
        // of port names, and a guess dressed as a fact is worse than saying
        // nothing. What actually matters is in the caveat, and that is read
        // from the graph itself: the rate it runs at.
        let provider = String::new();
        BackendInfo {
            kind: BackendKind::Jack,
            present: true,
            missing: String::new(),
            provider,
            fidelity: Fidelity::Never,
            caveat: chilpa_i18n::t_args(
                "A JACK graph runs at one rate and every program in it runs at that rate -- this \
                 one is at {} Hz. A track recorded at any other rate is resampled before Chilpa \
                 has any say, and no setting here changes it. JACK is for routing audio between \
                 programs; for a file delivered to a DAC untouched, choose the card itself.",
                &[&rate.to_string()],
            ),
        }
    }

    /// The one server that can be measured without taking anything: the
    /// graph's rate is published and every client is told it.
    fn caps(&self, _id: &str) -> Result<chilpa_audio::negotiate::DeviceCaps, SinkError> {
        measured().map_err(|why| {
            SinkError::Backend(match why {
                Silence::Absent => chilpa_i18n::t("no JACK graph is running"),
                Silence::Late | Silence::Busy => chilpa_i18n::t("the JACK graph did not answer in time"),
            })
        })
    }

    fn outputs(&self) -> Vec<BackendOutput> {
        // The graph's physical inputs: the ports that are an actual way out
        // of the machine. Everything else is another program, and sending
        // music into one of those is not choosing an output.
        let Ok((ports, rate)) = ask(|client| {
            let ports = client.ports(
                None,
                Some(jack::AudioIn::default().jack_port_type()),
                jack::PortFlags::IS_INPUT | jack::PortFlags::IS_PHYSICAL,
            );
            (ports, client.sample_rate())
        }) else {
            return Vec::new();
        };

        // Ports come one per channel ("...:playback_1"), and what somebody
        // picks is a device, not a channel. They are grouped back by the
        // name in front of the colon.
        let mut devices: Vec<String> = Vec::new();
        for port in ports {
            let device = port.split_once(':').map(|(d, _)| d.to_string()).unwrap_or(port);
            if !devices.contains(&device) {
                devices.push(device);
            }
        }
        devices
            .into_iter()
            .map(|device| BackendOutput {
                detail: chilpa_i18n::t_args("the graph runs at {} Hz", &[&rate.to_string()]),
                label: device.clone(),
                id: device,
                // Never, and it is the graph and not the device: the same
                // DAC is exact through the card and is not through here.
                bit_perfect: false,
            })
            .collect()
    }

    fn open(
        &self,
        id: &str,
        format: StreamFormat,
        _exclusive: bool,
    ) -> Result<Box<dyn AudioSink + Send>, SinkError> {
        // There is nothing to ask for exclusively: a graph is shared by
        // definition, and pretending otherwise is how a player ends up
        // claiming bit-perfect over a resampler.
        JackSink::open(id, format).map(|s| Box::new(s) as Box<dyn AudioSink + Send>)
    }
}

/// What the writing side and the graph's thread share.
///
/// A plain buffer behind a lock rather than a lock-free ring: the graph's
/// callback must not block, and it does not -- it takes the lock or gives
/// up and plays silence for that period, which is one underrun and not a
/// stall. A ring that is never contended is not worth the code until this
/// one is measured and found wanting.
struct Shared {
    /// Samples waiting to go out, already interleaved as f32, which is what
    /// JACK deals in.
    waiting: Mutex<std::collections::VecDeque<f32>>,
    /// Set when the graph stopped calling: the server went away.
    lost: AtomicBool,
}

/// A stream into the graph.
pub struct JackSink {
    stream: StreamFormat,
    shared: Arc<Shared>,
    /// Kept so the client stays in the graph; dropping it leaves.
    _active: Option<jack::AsyncClient<Watch, Handler>>,
    /// How many samples may wait before the writer is asked to slow down.
    /// A quarter of a second, which is long enough to ride out a scheduling
    /// hiccup and short enough that stopping is not heard as a tail.
    ceiling: usize,
}

/// What the graph says when it stops.
///
/// Without this a server that goes away leaves the writer filling a buffer
/// nobody reads: every write succeeds, the position advances, and the track
/// plays out in silence. That is the shape of failure the sink's own
/// documentation was written against.
struct Watch {
    shared: Arc<Shared>,
}

impl jack::NotificationHandler for Watch {
    unsafe fn shutdown(&mut self, _: jack::ClientStatus, _: &str) {
        self.shared.lost.store(true, Ordering::Relaxed);
    }
}

/// What the graph calls.
struct Handler {
    shared: Arc<Shared>,
    ports: Vec<jack::Port<jack::AudioOut>>,
}

impl jack::ProcessHandler for Handler {
    fn process(&mut self, _: &jack::Client, scope: &jack::ProcessScope) -> jack::Control {
        let frames = scope.n_frames() as usize;
        // Taken once for the whole period. Failing to take it is an
        // underrun, not a reason to wait: the graph is on a deadline.
        let mut waiting = match self.shared.waiting.try_lock() {
            Ok(w) => w,
            Err(_) => {
                for port in &mut self.ports {
                    port.as_mut_slice(scope).fill(0.0);
                }
                return jack::Control::Continue;
            }
        };
        // Interleaved on the way in, one port per channel on the way out.
        // Running dry gives silence for the rest of the period, which is a
        // gap somebody hears once, not a stall the graph never recovers
        // from.
        for frame in 0..frames {
            for port in self.ports.iter_mut() {
                let sample = waiting.pop_front().unwrap_or(0.0);
                port.as_mut_slice(scope)[frame] = sample;
            }
        }
        jack::Control::Continue
    }
}

impl JackSink {
    /// Joins the graph, giving up after [`PATIENCE`].
    ///
    /// On a thread of its own like every question here: after a close that
    /// never returned, opening a client waits forever on the `jack` crate's
    /// lock, and this is called on the player's thread. A sink that is built
    /// after the wait was given up on is dropped over there, and leaves.
    fn open(device: &str, stream: StreamFormat) -> Result<Self, SinkError> {
        let device = device.to_string();
        within(PATIENCE, move |sink| {
            let _ = sink.send(JackSink::join(&device, stream));
        })
        .unwrap_or_else(|_| {
            Err(SinkError::Backend(chilpa_i18n::t("the JACK graph did not answer in time")))
        })
    }

    fn join(device: &str, stream: StreamFormat) -> Result<Self, SinkError> {
        let (client, _) = jack::Client::new(CLIENT, jack::ClientOptions::NO_START_SERVER)
            .map_err(|e| SinkError::Backend(e.to_string()))?;
        // The graph's rate is the graph's, not ours, and by here it has
        // already been agreed: `capabilities` publishes the one rate, so
        // whoever negotiated either matched it or converted to it on
        // purpose. This is the last check that nothing slipped past.
        if client.sample_rate() as u32 != stream.sample_rate_hz {
            return Err(SinkError::Backend(chilpa_i18n::t_args(
                "the JACK graph runs at {} Hz and this track is {} Hz; JACK cannot play it \
                 untouched",
                &[&client.sample_rate().to_string(), &stream.sample_rate_hz.to_string()],
            )));
        }
        let mut ports = Vec::new();
        for channel in 0..stream.channels {
            let port = client
                .register_port(&format!("out_{}", channel + 1), jack::AudioOut::default())
                .map_err(|e| SinkError::Backend(e.to_string()))?;
            ports.push(port);
        }
        let shared = Arc::new(Shared {
            waiting: Mutex::new(std::collections::VecDeque::new()),
            lost: AtomicBool::new(false),
        });
        let handler = Handler { shared: Arc::clone(&shared), ports };
        let active = client
            .activate_async(Watch { shared: Arc::clone(&shared) }, handler)
            .map_err(|e| SinkError::Backend(e.to_string()))?;

        // Connected to what was asked for, by name. A stream that is not
        // connected plays into nothing and sounds exactly like a broken
        // cable, so a connection that fails is a failure and not a warning.
        let wanted: Vec<String> = active
            .as_client()
            .ports(Some(&format!("^{device}:")), Some(jack::AudioIn::default().jack_port_type()),
                   jack::PortFlags::IS_INPUT);
        for (channel, target) in wanted.iter().take(stream.channels as usize).enumerate() {
            active
                .as_client()
                .connect_ports_by_name(&format!("{CLIENT}:out_{}", channel + 1), target)
                .map_err(|e| SinkError::Backend(e.to_string()))?;
        }

        Ok(JackSink {
            stream,
            shared,
            _active: Some(active),
            ceiling: (stream.sample_rate_hz as usize / 4) * stream.channels as usize,
        })
    }
}

impl AudioSink for JackSink {
    fn format(&self) -> StreamFormat {
        self.stream
    }

    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
        if self.shared.lost.load(Ordering::Relaxed) {
            return Err(SinkError::DeviceLost);
        }
        let samples = to_f32(frames, self.stream.sample_format);
        let mut waiting = self.shared.waiting.lock().map_err(|_| SinkError::Closed)?;
        if waiting.len() >= self.ceiling {
            // Full. Nothing was taken and the caller comes back, which is
            // what the sink's contract says a partial write means.
            return Ok(0);
        }
        waiting.extend(samples);
        // In frames, as the trait asks: the renderer multiplies by the size.
        Ok(frames.len() / self.stream.frame_bytes())
    }

    fn close(&mut self) -> Result<(), SinkError> {
        self.leave();
        Ok(())
    }

    fn flush(&mut self) -> Result<(), SinkError> {
        self.shared.waiting.lock().map_err(|_| SinkError::Closed)?.clear();
        Ok(())
    }

    fn held(&self) -> u64 {
        let samples = self.shared.waiting.lock().map(|w| w.len()).unwrap_or(0);
        (samples / usize::from(self.stream.channels.max(1))) as u64
    }

    /// Waits for the graph to take what is queued: a quarter of a second at
    /// most, and a second more before giving up on a graph that stopped.
    fn drain(&mut self, give_up: &dyn Fn() -> bool) -> Result<(), SinkError> {
        let limit = std::time::Instant::now() + Duration::from_millis(1_250);
        while std::time::Instant::now() < limit && !give_up() {
            if self.shared.lost.load(Ordering::Relaxed) {
                return Err(SinkError::DeviceLost);
            }
            if self.shared.waiting.lock().map_err(|_| SinkError::Closed)?.is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(())
    }
}

impl JackSink {
    /// Leaves the graph, and does not wait to have left: stopping never
    /// waits for a device. The comment here used to say that while the
    /// client was dropped on the player's thread, which is where closing it
    /// can hang.
    fn leave(&mut self) {
        if let Some(active) = self._active.take() {
            let_go(active);
        }
    }
}

impl Drop for JackSink {
    fn drop(&mut self) {
        self.leave();
    }
}

/// The samples as JACK wants them.
///
/// Converting here is not a loss of fidelity and would not be worth
/// mentioning except that it looks like one: JACK's own currency is f32, so
/// every program in a graph is already there. It is what the graph's rate
/// does to the track that costs, and that is said before anything opens.
fn to_f32(bytes: &[u8], format: SampleFormat) -> Vec<f32> {
    match format {
        SampleFormat::S16 => bytes
            .chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32_768.0)
            .collect(),
        SampleFormat::S24Packed => bytes
            .chunks_exact(3)
            .map(|c| {
                let v = i32::from_le_bytes([0, c[0], c[1], c[2]]) >> 8;
                v as f32 / 8_388_608.0
            })
            .collect(),
        SampleFormat::S24In32 | SampleFormat::S32 => bytes
            .chunks_exact(4)
            .map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]]) as f32 / 2_147_483_648.0)
            .collect(),
        SampleFormat::F32 => bytes
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect(),
        SampleFormat::DsdU32Be | SampleFormat::DsdU32Le => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    /// Runs `f`, and fails instead of hanging if it has not come back within
    /// `limit`. The bug here is a hang, and a test that hangs along with it
    /// tells nobody anything.
    fn comes_back_within<T: Send + 'static>(limit: Duration, f: impl FnOnce() -> T + Send + 'static) -> T {
        let (done, finished) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = done.send(f());
        });
        finished.recv_timeout(limit).expect("it never came back: the caller would have hung here")
    }

    /// What closing a client under PipeWire's JACK library sometimes does.
    fn forever() -> ! {
        loop {
            std::thread::park();
        }
    }

    #[test]
    fn an_answer_comes_back_even_when_closing_the_client_never_ends() {
        let got = comes_back_within(Duration::from_secs(5), || {
            within(Duration::from_secs(3), |answer: Sender<u32>| {
                let _ = answer.send(48_000);
                forever()
            })
        });
        assert_eq!(got, Ok(48_000));
    }

    #[test]
    fn a_graph_that_never_answers_costs_the_patience_and_no_more() {
        let asked = Instant::now();
        let got = comes_back_within(Duration::from_secs(5), || {
            within(Duration::from_millis(200), |_: Sender<u32>| forever())
        });
        assert_eq!(got, Err(Silence::Late));
        assert!(asked.elapsed() < Duration::from_secs(2), "{:?}", asked.elapsed());
    }

    #[test]
    fn no_graph_is_said_at_once() {
        let got = within(Duration::from_secs(5), |answer: Sender<u32>| drop(answer));
        assert_eq!(got, Err(Silence::Absent));
    }

    #[test]
    fn a_question_waits_its_turn_behind_one_still_closing_and_is_busy_past_its_patience() {
        // A question's thread stays until its client is closed, and a close
        // under PipeWire's JACK library can hang for good. The outputs are
        // listed on a timer, and each listing asked again: a thread per
        // question, every one waiting behind the stuck close. And the
        // player asks twice in a row, fidelity then capabilities, so a
        // close still under way a millisecond after the first answer must
        // not turn the second into "no graph".
        let (release, released) = mpsc::channel::<()>();
        let first = one_at_a_time(PATIENCE, move |answer: Sender<u32>| {
            let _ = answer.send(1);
            let _ = released.recv();
        });
        assert_eq!(first, Ok(1));
        // Answered, and still closing. The next question waits its turn
        // and, when its patience runs out first, is told busy: not absent.
        let asked = Instant::now();
        let second = one_at_a_time(Duration::from_millis(200), |answer: Sender<u32>| { let _ = answer.send(2); });
        assert_eq!(second, Err(Silence::Busy));
        assert!(asked.elapsed() >= Duration::from_millis(200) && asked.elapsed() < Duration::from_secs(2), "{:?}", asked.elapsed());
        // One with patience gets its turn once the first is really done.
        let closing = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            drop(release);
        });
        let asked = Instant::now();
        let third = one_at_a_time(PATIENCE, |answer: Sender<u32>| { let _ = answer.send(3); });
        assert_eq!(third, Ok(3), "the turn came and the question was not asked");
        assert!(asked.elapsed() >= Duration::from_millis(80), "it did not wait for the close: {:?}", asked.elapsed());
        closing.join().unwrap();
    }

    #[test]
    fn letting_go_of_a_client_does_not_wait_for_it_to_leave() {
        struct Stuck;
        impl Drop for Stuck {
            fn drop(&mut self) {
                forever()
            }
        }
        comes_back_within(Duration::from_secs(5), || let_go(Stuck));
    }

    #[test]
    fn this_stack_never_claims_to_be_exact() {
        let info = Jack.info();
        if info.present {
            assert_eq!(info.fidelity, Fidelity::Never);
            assert!(!info.caveat.is_empty());
            assert!(Jack.outputs().iter().all(|o| !o.bit_perfect));
        } else {
            assert!(!info.missing.is_empty());
        }
    }

    #[test]
    fn a_track_at_another_rate_is_refused_rather_than_resampled() {
        // The graph decides the rate and a client is told it. Playing
        // anyway means resampling, and doing that quietly is what every
        // other player does here.
        if Jack.info().present {
            let far_off = StreamFormat::new(SampleFormat::S16, 1, 2);
            assert!(Jack.open("nothing", far_off, false).is_err());
        }
    }

    #[test]
    fn a_write_answers_in_frames_and_not_in_bytes() {
        // The renderer multiplies the answer by the frame size. Answered in
        // bytes, every frame was counted four to eight times, and the
        // position ran ahead of the music by as much.
        let (Some(output), Ok(caps)) = (Jack.outputs().into_iter().next(), Jack.caps("")) else {
            eprintln!("no JACK graph: skipped");
            return;
        };
        let format = StreamFormat::new(caps.formats[0], caps.sample_rates[0], 2);
        let mut sink = match Jack.open(&output.id, format, false) {
            Ok(sink) => sink,
            Err(e) => {
                eprintln!("the graph would not take {}: skipped ({e:?})", output.id);
                return;
            }
        };
        // Silence, mixed in with whatever else the graph is playing.
        let silence = vec![0u8; 64 * format.frame_bytes()];
        assert_eq!(sink.write(&silence).unwrap(), 64);
        let _ = sink.close();
    }

    #[test]
    #[ignore = "plays two seconds of silence into the running graph; run it on purpose"]
    fn a_track_reaches_the_graph_whole_and_at_the_graph_s_pace() {
        // The graph takes a quarter of a second at a time and answers "full"
        // in between. The renderer took that as "on to the next block" and
        // raced through the track, dropping most of it.
        let (Some(output), Ok(caps)) = (Jack.outputs().into_iter().next(), Jack.caps("")) else {
            eprintln!("no JACK graph: skipped");
            return;
        };
        let format = StreamFormat::new(caps.formats[0], caps.sample_rates[0], 2);
        let mut sink = Jack.open(&output.id, format, false).expect("the graph takes a stream");

        struct Silence(u64, usize);
        impl chilpa_audio::render::FrameSource for Silence {
            fn next_block(&mut self) -> Result<Option<Vec<u8>>, String> {
                let frames = self.0.min(4096);
                self.0 -= frames;
                Ok((frames > 0).then(|| vec![0u8; frames as usize * self.1]))
            }
        }
        let total = 2 * u64::from(format.sample_rate_hz);
        let started = std::time::Instant::now();
        let out = chilpa_audio::render::render(
            &mut Silence(total, format.frame_bytes()),
            &mut sink,
            &chilpa_audio::sink::StopSignal::new(),
        );
        let took = started.elapsed();
        let _ = sink.close();
        eprintln!("smoke: {out:?} in {took:?}");
        assert_eq!(out, chilpa_audio::render::Outcome::Finished { frames: total }, "all of it went out");
        assert!(took > Duration::from_millis(1_500), "two seconds went out in {took:?}");
    }

    /// Run by the test below, in a process of its own: joins the graph,
    /// drops the sink and returns at once, so the process exits while the
    /// client is still leaving.
    #[test]
    #[ignore = "run by quitting_while_a_client_leaves_the_graph_does_not_hang"]
    fn leave_and_quit() {
        if std::env::var_os("CHILPA_JACK_LEAVE_AND_QUIT").is_none() {
            return;
        }
        let (Some(output), Ok(caps)) = (Jack.outputs().into_iter().next(), Jack.caps("")) else {
            return;
        };
        let format = StreamFormat::new(caps.formats[0], caps.sample_rates[0], 2);
        if let Ok(sink) = Jack.open(&output.id, format, false) {
            drop(sink);
        }
    }

    #[test]
    fn quitting_while_a_client_leaves_the_graph_does_not_hang() {
        // PipeWire's JACK library tears its loop down when the process exits,
        // and a client still leaving on its own thread waits on that loop
        // forever: the exit never finished. Seen as a test binary that had
        // passed and was still alive five minutes later.
        if Jack.outputs().is_empty() {
            eprintln!("no JACK graph: skipped");
            return;
        }
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "tests::leave_and_quit", "--ignored", "--test-threads=1"])
            .env("CHILPA_JACK_LEAVE_AND_QUIT", "1")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let limit = std::time::Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success(), "the process that left the graph failed: {status}");
                return;
            }
            if std::time::Instant::now() > limit {
                let _ = child.kill();
                let _ = child.wait();
                panic!("a process whose JACK client was still leaving could not exit");
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    #[test]
    fn the_samples_reach_jacks_own_currency_unharmed() {
        // Full scale, silence and the bottom, through each width.
        let s16: Vec<u8> = [0i16, 32_767, -32_768].iter().flat_map(|v| v.to_le_bytes()).collect();
        let out = to_f32(&s16, SampleFormat::S16);
        assert_eq!(out[0], 0.0);
        assert!((out[1] - 0.999_97).abs() < 0.001, "{}", out[1]);
        assert_eq!(out[2], -1.0);

        let s32: Vec<u8> = [0i32, i32::MAX, i32::MIN].iter().flat_map(|v| v.to_le_bytes()).collect();
        let out = to_f32(&s32, SampleFormat::S32);
        assert_eq!(out[0], 0.0);
        assert!((out[1] - 1.0).abs() < 0.001);
        assert_eq!(out[2], -1.0);

        let f32s: Vec<u8> = [0.0f32, 0.5, -1.0].iter().flat_map(|v| v.to_le_bytes()).collect();
        assert_eq!(to_f32(&f32s, SampleFormat::F32), vec![0.0, 0.5, -1.0]);
    }
}
