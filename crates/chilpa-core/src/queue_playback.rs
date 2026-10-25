//! Playing several tracks in a row.
//!
//! The queue chains with no gap as long as the stream format does not
//! change. If it changes, the device has to be reopened and the break is
//! unavoidable: it is reported, not hidden.
//!
//! Every track ends with its own [`Outcome`], and only a natural end lets
//! the next one through. A device failure stops the queue right there, which
//! is exactly what QBZ did not do: it skipped everything in silence.

use chilpa_audio::format::{SampleFormat, StreamFormat};
use chilpa_audio::gapless::{transition, Transition, Trimmer};
use chilpa_audio::negotiate::{negotiate, DeviceCaps, Delivery, Policy};
use chilpa_audio::pack::{duplicate_mono, pack, Samples};
use chilpa_audio::render::{FrameSource, Outcome};
use chilpa_audio::seek::SeekSignal;
use chilpa_audio::sink::{AudioSink, PauseSignal, SinkError, SkipSignal, StopSignal};
use chilpa_decode::{Block, Decoded};
use std::path::{Path, PathBuf};

use crate::dsd_playback::{dop_request, is_dsd_file, native_request, open_dsd_file, DopFrames, NativeFrames};
use crate::playback::PlayError;
use chilpa_audio::dop::DopPacker;
use chilpa_dsd::{DsdPcmConverter, DsdSource, DEFAULT_GAIN_DB};

mod dsd_pcm;
use dsd_pcm::DsdPcmFrames;

/// What a track is read from after its route has been resolved.
pub(crate) enum Source {
    Pcm(Decoded, Option<std::sync::Arc<dyn crate::playback_events::Observer>>, Option<crate::remote::Normalization>),
    Dop(Box<dyn DsdSource>),
    Native(Box<dyn DsdSource>),
    DsdPcm(DsdPcmConverter),
}

/// What a track says of itself, read once as it is opened and handed over
/// when it starts: its tags, its cover and how long it is. Read along with
/// the track, and ahead of it when the track is opened ahead, so that
/// nothing is read from the file while the device waits for its first
/// block. A track of a disc image or of a server says it another way, and
/// carries none of this.
#[derive(Clone, Debug, Default)]
pub struct TrackAbout {
    pub tags: Option<chilpa_tags::Tags>,
    pub artwork: Option<PathBuf>,
    /// How many frames the decoder counts, at the file's own rate.
    pub frames: Option<u64>,
    /// Encoded signal depth to announce, excluding a lossy PCM reference.
    pub bits: Option<u8>,
    /// What a DSD file is: its rate and its length in one-bit samples.
    pub dsd: Option<chilpa_dsd::DsdInfo>,
}

/// A track opened and ready to play.
pub(crate) struct Opened {
    pub source: Source,
    pub delivery: Delivery,
    pub about: TrackAbout,
}

/// Opens a track by what it is, and negotiates what it asks for.
///
/// DSD used to be played outside the queue, on the first track only: a
/// queue of DSF files played its first file and stopped, because nothing
/// took the queue on from there. It goes through the queue like any track
/// now, as DoP only on an eligible direct route and as PCM everywhere else.
pub(crate) fn open_track(
    path: &Path,
    caps: &DeviceCaps,
    policy: Policy,
    dsd_mode: DsdMode,
    direct_dsd: Option<&DeviceCaps>,
) -> Result<Opened, PlayError> {
    let unreadable = |e: String| PlayError::Sink(SinkError::Backend(e));
    if let Some((image, number)) = crate::sacd_tracks::parse(path) {
        let reader = crate::sacd_tracks::open(&image, number).map_err(unreadable)?;
        let (source, delivery) = dsd_track(Box::new(reader), caps, dsd_mode, direct_dsd)?;
        return Ok(Opened { source, delivery, about: TrackAbout::default() });
    }
    if is_dsd_file(path) {
        let reader = open_dsd_file(path).map_err(|e| unreadable(e.to_string()))?;
        let dsd = Some(reader.info().clone());
        let (tags, artwork) = said_by(path);
        let (source, delivery) = dsd_track(reader, caps, dsd_mode, direct_dsd)?;
        return Ok(Opened { source, delivery, about: TrackAbout { tags, artwork, frames: None, dsd, bits: Some(1) } });
    }
    // A track on a server: the original bytes, read by windows, decoded
    // here like a file on disk (ADR-017).
    let (decoded, observer, normalization, mut about) = match crate::remote::entry(path) {
        Some(entry) => {
            let (decoded, observer, normalization) = crate::remote::open(path, &entry).map_err(unreadable)?;
            (decoded, observer, normalization, TrackAbout::default())
        }
        None => {
            let decoded = Decoded::open(path).map_err(PlayError::Decode)?;
            let (tags, artwork) = said_by(path);
            let about = TrackAbout { tags, artwork, frames: decoded.frame_count(), dsd: None, bits: None };
            (decoded, None, None, about)
        }
    };
    let delivery = crate::playback::negotiate_decoded(&decoded, caps, policy).map_err(PlayError::Negotiation)?;
    about.bits = crate::playback::signal_bits(&decoded, &delivery);
    Ok(Opened { source: Source::Pcm(decoded, observer, normalization), delivery, about })
}

/// The tags a file carries and its cover, with the time they take.
fn said_by(path: &Path) -> (Option<chilpa_tags::Tags>, Option<PathBuf>) {
    let started = std::time::Instant::now();
    let tags = chilpa_tags::read(path).ok().map(|info| info.tags);
    let artwork = chilpa_tags::artwork_for(path, &crate::artwork_cache());
    chilpa_audio::diagnostics::trace("queue", "about_read", format_args!("after={:?} path={path:?}", started.elapsed()));
    (tags, artwork)
}

/// Sends DSD untouched only to a direct route that takes it as asked: DoP
/// in its carrier, or the kernel's native format. Every other path receives
/// ordinary PCM, which a sound server may safely mix, resample and scale.
///
/// Asking for DoP or native DSD is asking for the exact path where there is
/// one, not refusing the track where there is none. It used to refuse: with
/// either chosen, a DSD record did not play through a sound server or on a
/// card that takes neither, where converting is all it took. The delivery
/// says it was converted, so nothing calls it bit-perfect.
///
/// `direct` is what reaches the output's wire untouched, or `None` where
/// nothing does: DoP and native DSD are negotiated against it and nothing
/// else. On a card it is the card's own caps; on a stack that converts on
/// its own it is narrower than `caps`, and a carrier rate the device lacks
/// is not offered -- a resampled DoP stream is loud noise.
fn dsd_track(
    reader: Box<dyn DsdSource>,
    caps: &DeviceCaps,
    mode: DsdMode,
    direct: Option<&DeviceCaps>,
) -> Result<(Source, Delivery), PlayError> {
    let untouched = match (mode, direct) {
        (DsdMode::Dop, Some(exact)) => negotiate(dop_request(reader.info()), exact, Policy::BitPerfect).ok(),
        (DsdMode::Native, Some(exact)) => native_request(reader.info(), exact)
            .map(|request| Delivery { requested: request, opened: request, transforms: Vec::new() }),
        _ => None,
    };
    match (mode, untouched) {
        (DsdMode::Dop, Some(delivery)) => Ok((Source::Dop(reader), delivery)),
        (DsdMode::Native, Some(delivery)) => Ok((Source::Native(reader), delivery)),
        _ => pcm_track(reader, caps),
    }
}

fn pcm_track(reader: Box<dyn DsdSource>, caps: &DeviceCaps) -> Result<(Source, Delivery), PlayError> {
    let converter = DsdPcmConverter::new(reader, DEFAULT_GAIN_DB)
        .map_err(|e| PlayError::Sink(SinkError::Backend(e.to_string())))?;
    let request = StreamFormat::new(SampleFormat::F32, converter.output_rate(), converter.channels());
    let mut delivery = negotiate(request, caps, Policy::AllowAnything).map_err(PlayError::Negotiation)?;
    delivery.transforms.insert(0, chilpa_audio::negotiate::Transform::DsdToPcm);
    Ok((Source::DsdPcm(converter), delivery))
}

/// What to do on reaching the end of the queue.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Repeat {
    /// The queue ends and it stops.
    #[default]
    Off = 0,
    /// It starts over from the first one.
    All = 1,
    /// Repeats the playing track. Skipping does advance: repeating is not
    /// getting stuck.
    One = 2,
}

/// How one-bit DSD is delivered to the chosen output.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum DsdMode {
    /// Low-pass filter and decimate to ordinary PCM. Safe on every output.
    #[default]
    Pcm = 0,
    /// Carry untouched DSD bits inside marked 24-bit PCM words.
    Dop = 1,
    /// Use ALSA's DSD_U32_BE/LE formats without a PCM carrier.
    Native = 2,
}

impl DsdMode {
    pub fn key(self) -> &'static str {
        match self { Self::Pcm => "pcm", Self::Dop => "dop", Self::Native => "native" }
    }

    pub fn from_key(key: &str) -> Self {
        match key { "dop" => Self::Dop, "native" => Self::Native, _ => Self::Pcm }
    }
}

#[derive(Clone, Default)]
pub struct DsdModeChoice(std::sync::Arc<std::sync::atomic::AtomicU8>);

impl DsdModeChoice {
    pub fn set(&self, mode: DsdMode) {
        self.0.store(mode as u8, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn get(&self) -> DsdMode {
        match self.0.load(std::sync::atomic::Ordering::Relaxed) {
            1 => DsdMode::Dop,
            2 => DsdMode::Native,
            _ => DsdMode::Pcm,
        }
    }
}

/// Repeat, shared with whoever drives the queue.
///
/// It can be changed while something is playing and is handled when the
/// track ends: cutting the audio to change mode would make no sense.
#[derive(Clone, Default)]
pub struct RepeatMode(std::sync::Arc<std::sync::atomic::AtomicU8>);

impl RepeatMode {
    pub fn new(mode: Repeat) -> Self {
        let m = Self::default();
        m.set(mode);
        m
    }

    pub fn set(&self, mode: Repeat) {
        self.0.store(mode as u8, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn get(&self) -> Repeat {
        match self.0.load(std::sync::atomic::Ordering::Relaxed) {
            1 => Repeat::All,
            2 => Repeat::One,
            _ => Repeat::Off,
        }
    }
}

/// A queue position after which playback stops. `usize::MAX` means none.
/// It is consumed when reached, so repeating or starting another order does
/// not inherit an old stop point.
#[derive(Clone)]
pub struct StopAfter(std::sync::Arc<std::sync::atomic::AtomicUsize>);

impl Default for StopAfter {
    fn default() -> Self {
        Self(std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(Self::NONE)))
    }
}

impl StopAfter {
    const NONE: usize = usize::MAX;

    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&self, index: Option<usize>) {
        self.0.store(index.unwrap_or(Self::NONE), std::sync::atomic::Ordering::Relaxed);
    }

    pub fn take_at(&self, index: usize) -> bool {
        self.0
            .compare_exchange(index, Self::NONE, std::sync::atomic::Ordering::Relaxed, std::sync::atomic::Ordering::Relaxed)
            .is_ok()
    }

    pub fn clear(&self) {
        self.set(None);
    }
}

/// The list of tracks, shared with whoever drives the queue.
///
/// Things can be added while it plays: queueing something should not cut
/// off what is playing or force rebuilding the queue from scratch. The loop
/// reads the length again on every track, so what is added comes in by itself.
#[derive(Clone, Default)]
pub struct SharedQueue(std::sync::Arc<std::sync::Mutex<Vec<Entry>>>);

/// A track in the queue. Removing it does not take it out of the array: it
/// is marked, so the other positions do not move under the feet of whoever
/// is playing.
#[derive(Clone, PartialEq, Eq, Debug)]
struct Entry {
    path: PathBuf,
    removed: bool,
}

impl SharedQueue {
    pub fn new(paths: Vec<PathBuf>) -> Self {
        Self(std::sync::Arc::new(std::sync::Mutex::new(entries(paths))))
    }

    pub fn set(&self, paths: Vec<PathBuf>) {
        if let Ok(mut g) = self.0.lock() {
            *g = entries(paths);
        }
    }

    /// Adds at the end. Returns the new length.
    pub fn extend(&self, paths: &[PathBuf]) -> usize {
        match self.0.lock() {
            Ok(mut g) => {
                g.extend(entries(paths.to_vec()));
                g.len()
            }
            Err(_) => 0,
        }
    }

    /// How many positions it has, counting the removed ones.
    pub fn len(&self) -> usize {
        self.0.lock().map(|g| g.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Removes a track from the queue. The position is kept, empty.
    pub fn remove(&self, i: usize) -> bool {
        match self.0.lock() {
            Ok(mut g) => match g.get_mut(i) {
                Some(e) if !e.removed => {
                    e.removed = true;
                    true
                }
                _ => false,
            },
            Err(_) => false,
        }
    }

    /// True if that position has already been removed.
    pub fn is_removed(&self, i: usize) -> bool {
        self.0.lock().ok().and_then(|g| g.get(i).map(|e| e.removed)).unwrap_or(false)
    }

    /// The track at that position, or `None` if it does not exist or was removed.
    pub fn get(&self, i: usize) -> Option<PathBuf> {
        let g = self.0.lock().ok()?;
        let e = g.get(i)?;
        (!e.removed).then(|| e.path.clone())
    }

    /// Every path, including the removed ones, so whoever shows the queue
    /// knows which position it is talking about.
    pub fn to_vec(&self) -> Vec<PathBuf> {
        self.0.lock().map(|g| g.iter().map(|e| e.path.clone()).collect()).unwrap_or_default()
    }
}

fn entries(paths: Vec<PathBuf>) -> Vec<Entry> {
    paths.into_iter().map(|path| Entry { path, removed: false }).collect()
}

/// The order the queue is traversed in, shared.
///
/// It can be changed while something is playing: when the track ends, where
/// it landed in the new order is looked up and it goes on from there,
/// without cutting the audio. Empty means the array's own order.
#[derive(Clone, Default)]
pub struct Order(std::sync::Arc<std::sync::Mutex<Vec<usize>>>);

impl Order {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets an order. One that is not a permutation of `0..len` is ignored: a
    /// half order would leave tracks unplayed or playing twice.
    pub fn set(&self, order: Vec<usize>, len: usize) -> bool {
        if !is_permutation(&order, len) {
            return false;
        }
        if let Ok(mut g) = self.0.lock() {
            *g = order;
            return true;
        }
        false
    }

    /// Goes back to the array's own order.
    pub fn clear(&self) {
        if let Ok(mut g) = self.0.lock() {
            g.clear();
        }
    }

    pub fn get(&self) -> Vec<usize> {
        self.0.lock().map(|g| g.clone()).unwrap_or_default()
    }

    /// Which track comes after `current`, or `None` if it ends there.
    fn next(&self, actual: usize, len: usize, wrap: bool) -> Option<usize> {
        let order = self.get();
        if order.len() != len {
            // With no order of its own, the queue goes as it comes.
            let n = actual + 1;
            return if n < len {
                Some(n)
            } else if wrap {
                Some(0)
            } else {
                None
            };
        }
        // Where the current track landed is looked up: if the order changed
        // mid playback, it goes on from its new place.
        let pos = order.iter().position(|i| *i == actual)?;
        match order.get(pos + 1) {
            Some(n) => Some(*n),
            None if wrap => order.first().copied(),
            None => None,
        }
    }

    /// Where it starts.
    fn first(&self, len: usize) -> usize {
        let order = self.get();
        if order.len() == len { order.first().copied().unwrap_or(0) } else { 0 }
    }
}

/// A random permutation of `0..len`.
///
/// The generator is a three line xorshift: shuffling a queue needs nothing
/// better, and one more dependency is one more dependency.
pub fn shuffled(len: usize, seed: u64) -> Vec<usize> {
    let mut order: Vec<usize> = (0..len).collect();
    let mut x = seed | 1;
    // Fisher and Yates, from the end backwards: every ordering is as likely.
    for i in (1..len).rev() {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        order.swap(i, (x % (i as u64 + 1)) as usize);
    }
    order
}

/// The same, seeded from the clock.
pub fn shuffled_now(len: usize) -> Vec<usize> {
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x2545_F491_4F6C_DD1D);
    shuffled(len, seed)
}

fn is_permutation(order: &[usize], len: usize) -> bool {
    if order.len() != len {
        return false;
    }
    let mut seen = vec![false; len];
    for i in order {
        match seen.get_mut(*i) {
            Some(v) if !*v => *v = true,
            _ => return false,
        }
    }
    true
}

/// The signals a queue is driven with while it plays.
///
/// They travel together because they are always used together, and because
/// all of them are checked between blocks or between tracks: none of them
/// blocks waiting on the device.
#[derive(Clone)]
pub struct Controls {
    /// Validated DSP parameters, read by PCM renderers after read-ahead.
    pub dsp: chilpa_audio::dsp::Control,
    pub stop: StopSignal,
    pub pause: PauseSignal,
    pub seek: SeekSignal,
    pub skip: SkipSignal,
    pub repeat: RepeatMode,
    pub order: Order,
    /// I arm the first cursor only after the preceding request has stopped.
    pub start_index: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    /// Stops cleanly after this queue position finishes.
    pub stop_after: StopAfter,
    /// One track straight into the next, with the device kept open. Off,
    /// the device plays out and is opened again between tracks.
    pub gapless: Switch,
    /// Tracks brought to one loudness (`crate::loudness`). It changes the
    /// samples, under any policy, and a track it touches is not delivered
    /// bit-perfect.
    pub normalize: Switch,
    /// The loudness they are brought to, in LUFS, and whether a boost stops
    /// short of clipping. Read as each track starts.
    pub normalize_target: Level,
    pub prevent_clipping: Switch,
    /// The output asked for alone: nothing else is mixed in, while Chilpa may
    /// still convert. Read when the output opens.
    pub exclusive: Switch,
    /// What the samples are multiplied by, from the player bar's volume:
    /// read on every block, so a change is heard as soon as what the device
    /// holds plays out. Never applied to raw DoP.
    pub volume: Level,
    /// The user's explicit DSD delivery choice.
    pub dsd_mode: DsdModeChoice,
    /// Whether the selected route can take one-bit audio untouched.
    pub direct_dsd: Switch,
    /// What reaches that route's wire untouched, when it is narrower than
    /// what the route takes: DoP is negotiated against it. `None` on a card,
    /// whose own caps already are that.
    pub direct_caps: std::sync::Arc<std::sync::Mutex<Option<DeviceCaps>>>,
    /// A new playback request is waiting. The current sink hands its device
    /// lease to that request instead of returning the card to the sound
    /// server for a few milliseconds and taking it straight back.
    pub handoff: Switch,
    /// Where the frames decoded for a track go, with their position, for
    /// the seek bar's waveform; nobody listening, nothing is handed over.
    pub waveform: std::sync::Arc<std::sync::Mutex<Option<std::sync::Arc<dyn chilpa_audio::seek_waveform::Feed>>>>,
}

/// A factor shared with whoever drives the queue, read every block.
#[derive(Clone, Debug)]
pub struct Level(std::sync::Arc<std::sync::atomic::AtomicU32>);

impl Level {
    pub fn new(factor: f32) -> Self {
        Self(std::sync::Arc::new(std::sync::atomic::AtomicU32::new(factor.to_bits())))
    }

    pub fn set(&self, factor: f32) {
        self.0.store(factor.to_bits(), std::sync::atomic::Ordering::Relaxed);
    }

    pub fn get(&self) -> f32 {
        f32::from_bits(self.0.load(std::sync::atomic::Ordering::Relaxed))
    }
}

/// An on/off shared with whoever drives the queue, read each track.
#[derive(Clone, Debug)]
pub struct Switch(std::sync::Arc<std::sync::atomic::AtomicBool>);

impl Switch {
    pub fn new(on: bool) -> Self {
        Self(std::sync::Arc::new(std::sync::atomic::AtomicBool::new(on)))
    }

    pub fn set(&self, on: bool) {
        self.0.store(on, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn get(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl Default for Controls {
    fn default() -> Self {
        Self {
            dsp: chilpa_audio::dsp::Control::default(),
            stop: StopSignal::default(),
            pause: PauseSignal::default(),
            seek: SeekSignal::default(),
            skip: SkipSignal::default(),
            repeat: RepeatMode::default(),
            order: Order::default(),
            start_index: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(usize::MAX)),
            stop_after: StopAfter::new(),
            gapless: Switch::new(true),
            normalize: Switch::new(false),
            normalize_target: Level::new(crate::loudness::DEFAULT_TARGET),
            prevent_clipping: Switch::new(true),
            exclusive: Switch::new(false),
            volume: Level::new(1.0),
            dsd_mode: DsdModeChoice::default(),
            direct_dsd: Switch::new(false),
            direct_caps: std::sync::Arc::new(std::sync::Mutex::new(None)),
            handoff: Switch::new(false),
            waveform: std::sync::Arc::new(std::sync::Mutex::new(None)),
        }
    }
}

impl Controls {
    /// What one-bit audio may be negotiated against untouched on the route
    /// as it stands, or `None` where it is always converted.
    pub(crate) fn dsd_direct(&self, caps: &DeviceCaps) -> Option<DeviceCaps> {
        self.direct_dsd.get().then(|| {
            self.direct_caps.lock().ok().and_then(|exact| exact.clone()).unwrap_or_else(|| caps.clone())
        })
    }

    pub fn new() -> Self {
        Self::default()
    }
}

/// Where a track is, and how much of it has gone out.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Progress {
    /// The position in the track, in frames: what the bar shows.
    pub position: u64,
    /// Frames the device has taken since the track started: what was
    /// listened to. A seek moves the position and not this.
    pub delivered: u64,
}

/// How a track in the queue ended.
pub struct TrackResult {
    pub path: PathBuf,
    pub delivery: Delivery,
    pub outcome: Outcome,
    /// How this track was reached from the previous one.
    pub transition: Transition,
}

impl TrackResult {
    pub fn frames(&self) -> u64 {
        self.outcome.frames()
    }
}

/// A track's source, with its padding trim applied.
struct TrackFrames<'a> {
    decoded: &'a mut Decoded,
    opened: StreamFormat,
    /// The track's own channels. Trimming and seeking count its frames, and
    /// a mono track that opened in stereo has half the samples per frame
    /// the device has.
    channels: usize,
    /// A mono track going out in both channels.
    spread: bool,
    trimmer: Trimmer<i32>,
    /// The same trim for a track that decodes to floats. Only one of the
    /// two is ever fed; the floats used to go around the trim entirely, so
    /// every MP3 kept the frames its encoder added at both ends.
    float_trimmer: Trimmer<f32>,
    exhausted: bool,
    /// Where seeks are asked for. They are handled between blocks, never mid
    /// write.
    seek: SeekSignal,
    /// Where a track skip is asked for. It is handled by ending the source,
    /// which is what makes the queue advance instead of stopping.
    skip: SkipSignal,
    /// True if this track ended because someone skipped, not because it
    /// ran out.
    skipped: bool,
    /// Frames to throw away to land on the exact point asked for.
    discard: u64,
    /// Set by a seek that landed, until the renderer has heard of it.
    moved: bool,
    /// What to add to the frames delivered to get the position in the file:
    /// where the last seek landed less what had been delivered by then. It
    /// used to be where the seek landed alone, and the renderer counts from
    /// the start of the track, so a seek to second 2 after one second had
    /// gone out reported second 3.
    base: std::sync::Arc<std::sync::atomic::AtomicI64>,
    /// Frames the device has taken of this track, as the renderer reports
    /// them.
    delivered: std::sync::Arc<std::sync::atomic::AtomicU64>,
    /// Where to go back to after a long pause let the device go, in frames
    /// of the track: what was heard.
    rewind_to: Option<u64>,
    /// What had been delivered when the last seek was worked out.
    moved_at: u64,
    /// The normalization gain and the player bar's volume (`loudness`).
    level: crate::loudness::level::Leveler,
    gain_format: StreamFormat,
    /// Where the decoded frames go for the waveform, and the position in
    /// the track of the next frame the decoder hands out.
    feed: Option<std::sync::Arc<dyn chilpa_audio::seek_waveform::Feed>>,
    decoded_at: u64,
    /// How far ahead it is read: further for a track over the network.
    ahead: std::time::Duration,
}

impl FrameSource for TrackFrames<'_> {
    fn pcm_format(&self) -> Option<StreamFormat> { Some(self.gain_format) }
    fn ended_early(&self) -> bool {
        self.skipped
    }

    fn ahead(&self) -> std::time::Duration {
        self.ahead
    }

    fn moved(&mut self) -> bool {
        std::mem::take(&mut self.moved)
    }

    fn heard_at_move(&self) -> Option<u64> {
        Some(self.moved_at)
    }
    fn position_at_move(&self) -> Option<u64> {
        Some((self.base.load(std::sync::atomic::Ordering::Relaxed) + self.moved_at as i64).max(0) as u64)
    }

    /// What was heard is the renderer's last count plus where the last seek
    /// landed; going back there is a seek like any other.
    fn rewind(&mut self) -> bool {
        let heard = self.delivered.load(std::sync::atomic::Ordering::Relaxed) as i64
            + self.base.load(std::sync::atomic::Ordering::Relaxed);
        self.rewind_to = Some(heard.max(0) as u64);
        true
    }

    fn next_block(&mut self) -> Result<Option<Vec<u8>>, String> {
        loop {
            // Read ahead, the track ends before the device finishes it; a
            // seek, or a pause let go, in those last seconds goes back into
            // them.
            if (self.rewind_to.is_some() || self.seek.asked()) && !self.skipped {
                self.exhausted = false;
            }
            if self.exhausted {
                return Ok(None);
            }
            if self.skip.take() {
                self.skipped = true;
                self.exhausted = true;
                self.finish_trim();
                return Ok(None);
            }
            let asked = self
                .rewind_to
                .take()
                .or_else(|| self.seek.take().map(|t| chilpa_audio::seek::frames_at(t, self.opened.sample_rate_hz)));
            if let Some(frame) = asked {
                match self.decoded.seek_to_frame(frame) {
                    Ok(plan) => {
                        self.discard = plan.discard;
                        self.moved = true;
                        self.level.moved();
                        // The point ASKED FOR, not where the container
                        // landed: the discarded frames never come out, so
                        // counting them separately would leave the position
                        // short.
                        let delivered = self.delivered.load(std::sync::atomic::Ordering::Relaxed);
                        self.base.store(
                            frame as i64 - delivered as i64,
                            std::sync::atomic::Ordering::Relaxed,
                        );
                        self.moved_at = delivered;
                        self.decoded_at = frame;
                    }
                    // A container that cannot seek goes on playing from
                    // where it was, instead of dropping the track.
                    Err(_) => self.discard = 0,
                }
            }
            let clipped = match self.decoded.next_block() {
                Ok(Some(Block::Int { data, bits })) => {
                    let channels = self.channels;
                    let data = self.drop_discard(data, channels);
                    if data.is_empty() {
                        continue;
                    }
                    let mut frames = self.trimmer.push(&data);
                    if frames.is_empty() {
                        continue;
                    }
                    if let Some(feed) = &self.feed {
                        feed.feed_int(self.decoded_at, &frames, bits);
                    }
                    self.decoded_at += (frames.len() / channels) as u64;
                    self.level.int(&mut frames, bits);
                    let frames = if self.spread { duplicate_mono(&frames) } else { frames };
                    let mut bytes = Vec::new();
                    let samples = Samples::Int { data: &frames, bits };
                    crate::playback::pack_decoded(self.decoded, &samples, self.gain_format.sample_format, &mut bytes)
                        .map_err(|e| PlayError::Pack(e).to_string())?;
                    bytes
                }
                Ok(Some(Block::Float(data))) => {
                    let channels = self.channels;
                    let data = self.drop_discard(data, channels);
                    if data.is_empty() {
                        continue;
                    }
                    let mut frames = self.float_trimmer.push(&data);
                    if frames.is_empty() {
                        continue;
                    }
                    if let Some(feed) = &self.feed {
                        feed.feed_float(self.decoded_at, &frames);
                    }
                    self.decoded_at += (frames.len() / channels) as u64;
                    self.level.float(&mut frames);
                    let frames = if self.spread { duplicate_mono(&frames) } else { frames };
                    let mut bytes = Vec::with_capacity(frames.len() * self.gain_format.sample_format.bytes());
                    if self.gain_format.sample_format == SampleFormat::F32 {
                        bytes.extend(frames.iter().flat_map(|sample| sample.to_le_bytes()));
                    } else {
                        pack(&Samples::Float(&frames), self.gain_format.sample_format, &mut bytes)
                            .map_err(|e| PlayError::Pack(e).to_string())?;
                    }
                    bytes
                }
                Ok(None) => {
                    // What was held back was encoder padding.
                    self.exhausted = true;
                    self.finish_trim();
                    return Ok(None);
                }
                Err(e) => return Err(e.to_string()),
            };
            if !clipped.is_empty() {
                return Ok(Some(clipped));
            }
        }
    }
}

impl TrackFrames<'_> {
    /// Throws away the leftover frames of a seek, which can span several
    /// blocks.
    /// The track is over: what either trim held back was padding.
    fn finish_trim(&mut self) {
        self.trimmer.finish();
        self.float_trimmer.finish();
    }

    fn drop_discard<T: Clone>(&mut self, data: Vec<T>, channels: usize) -> Vec<T> {
        if self.discard == 0 {
            return data;
        }
        let frames = (data.len() / channels) as u64;
        if self.discard >= frames {
            self.discard -= frames;
            return Vec::new();
        }
        let cut = self.discard as usize * channels;
        self.discard = 0;
        data[cut..].to_vec()
    }
}

/// Plays the tracks in order.
///
/// `open_sink` opens the device with the format it is handed. It is called
/// once per renegotiation, not once per track.
pub fn play_queue<F, K>(
    paths: &[PathBuf],
    caps: &DeviceCaps,
    policy: Policy,
    stop: &StopSignal,
    open_sink: F,
) -> Result<Vec<TrackResult>, PlayError>
where
    F: FnMut(StreamFormat) -> Result<K, SinkError>,
    K: AudioSink,
{
    let controls = Controls { stop: stop.clone(), ..Controls::new() };
    play_queue_with(
        &SharedQueue::new(paths.to_vec()),
        caps,
        policy,
        &controls,
        open_sink,
        &mut |_, _, _| {},
        &mut |_, _| {},
    )
}

/// The same, reporting when each track starts and how many frames it has.
///
/// `on_track` receives the index, the path and how the delivery turned out;
/// `on_progress` the index and where that track is.
#[allow(clippy::too_many_arguments)]
pub fn play_queue_with<F, K>(
    queue: &SharedQueue,
    caps: &DeviceCaps,
    policy: Policy,
    controls: &Controls,
    open_sink: F,
    on_track: &mut dyn FnMut(usize, &Path, &Delivery),
    on_progress: &mut dyn FnMut(usize, Progress),
) -> Result<Vec<TrackResult>, PlayError>
where
    F: FnMut(StreamFormat) -> Result<K, SinkError>,
    K: AudioSink,
{
    play_queue_told(queue, caps, policy, controls, open_sink, &mut |index, path, delivery, _| on_track(index, path, delivery), on_progress)
}

/// The same, with what each track says of itself as it starts.
#[allow(clippy::too_many_arguments)]
pub fn play_queue_told<F, K>(
    queue: &SharedQueue,
    caps: &DeviceCaps,
    policy: Policy,
    controls: &Controls,
    mut open_sink: F,
    on_track: &mut dyn FnMut(usize, &Path, &Delivery, &TrackAbout),
    on_progress: &mut dyn FnMut(usize, Progress),
) -> Result<Vec<TrackResult>, PlayError>
where
    F: FnMut(StreamFormat) -> Result<K, SinkError>,
    K: AudioSink,
{
    // `exclusive` was read when the output was chosen, before the queue.
    let Controls { stop, pause, seek, skip, repeat, order, stop_after, gapless, normalize, normalize_target, prevent_clipping, volume, .. } = controls;
    let mut results = Vec::new();
    let mut opened: Option<(StreamFormat, crate::playback_events::Measured<K>)> = None;
    let requested = controls.start_index.swap(usize::MAX, std::sync::atomic::Ordering::AcqRel);
    let mut index = if requested < queue.len() { requested } else { order.first(queue.len()) };
    // A file that delivers not one frame plus repeat turned on would be a
    // loop that spins forever. How many tracks in a row did not play is
    // counted, and it cuts off after one full lap.
    let mut consecutive_silent = 0usize;
    // A server that stopped answering: its tracks are passed over and the
    // queue goes on (ADR-017). The last error is kept to say, when nothing
    // at all could play, why; and a full lap of them ends it.
    let mut preload = crate::preload::Preload::default();
    let mut unreachable_in_a_row = 0usize;
    let mut passed_over: Option<PlayError> = None;
    // The DoP marker of whatever is open. It carries on across tracks that
    // share an opening and starts over with a new one.
    let mut packer = DopPacker::new();
    let mut dsp = None;
    // Whether the track before ended by itself: then what is still in the
    // device is its last part, and it is waited for before the device is
    // closed. Skipped or stopped, it is not.
    let mut ended_by_itself = false;
    // Whether the track before was skipped: if the device carries on into
    // the next one, what it still holds is the rest of the skipped track.
    let mut skipped_before = false;
    // A stop, or a skip of what is still playing, ends the wait.
    let give_up = || stop.is_stopped() || skip.take();

    // The length is read again on every lap: the queue can grow while it
    // plays.
    loop {
        // A removed track is skipped without playing; if none are left, the
        // queue is over.
        let Some(path) = next_live(queue, order, repeat, &mut index) else { break };
        let path = &path;
        chilpa_audio::diagnostics::trace(
            "queue",
            "track_selected",
            format_args!(
                "index={index} queue_len={} path={path:?} policy={policy:?} dsd_mode={:?} direct_dsd={}",
                queue.len(),
                controls.dsd_mode.get(),
                controls.direct_dsd.get()
            ),
        );
        let prepared = preload.take(path, stop);
        if stop.is_stopped() { break; }
        let source_started = std::time::Instant::now();
        let ahead = prepared.is_some();
        let ready = prepared.unwrap_or_else(|| open_track(path, caps, policy, controls.dsd_mode.get(), controls.dsd_direct(caps).as_ref()));
        let Opened { mut source, mut delivery, about } = match ready {
            Ok(opened) => opened,
            Err(error) if crate::remote::entry(path).is_some() => {
                chilpa_audio::diagnostics::trace(
                    "queue",
                    "source_passed_over",
                    format_args!("after={:?} path={path:?} error={error}", source_started.elapsed()),
                );
                unreachable_in_a_row += 1;
                passed_over = Some(error);
                if unreachable_in_a_row > queue.len() || stop_after.take_at(index) {
                    break;
                }
                match order.next(index, queue.len(), repeat.get() == Repeat::All) {
                    Some(n) => index = n,
                    None => break,
                }
                continue;
            }
            Err(error) => {
                chilpa_audio::diagnostics::trace(
                    "queue",
                    "source_failed",
                    format_args!("after={:?} path={path:?} error={error}", source_started.elapsed()),
                );
                return Err(error);
            }
        };
        unreachable_in_a_row = 0;
        let source_kind = match &source {
            Source::Pcm(..) => "pcm",
            Source::Dop(_) => "dop",
            Source::Native(_) => "native-dsd",
            Source::DsdPcm(_) => "dsd-pcm",
        };
        chilpa_audio::diagnostics::trace(
            "queue",
            "source_opened",
            format_args!(
                "after={:?} ahead={ahead} kind={source_kind} requested={:?} opened={:?} transforms={:?}",
                source_started.elapsed(),
                delivery.requested,
                delivery.opened,
                delivery.transforms
            ),
        );

        let transition = match &opened {
            Some((actual, _)) => transition(*actual, delivery.opened),
            None => Transition::Seamless,
        };
        // Without gapless, one track does not run into the next: the device
        // plays out what it holds and is opened again.
        let keep_open = transition.is_seamless() && gapless.get();
        chilpa_audio::diagnostics::trace(
            "queue",
            "transition_decided",
            format_args!(
                "transition={transition:?} gapless={} keep_open={keep_open} sink_open={}",
                gapless.get(),
                opened.is_some()
            ),
        );
        if !keep_open || opened.is_none() {
            if let Some((old_format, mut old)) = opened.take() {
                if ended_by_itself {
                    let started = std::time::Instant::now();
                    chilpa_audio::diagnostics::trace(
                        "queue",
                        "sink_drain_started",
                        format_args!("format={old_format:?}"),
                    );
                    if let Err(error) = old.drain(&give_up) {
                        chilpa_audio::diagnostics::trace(
                            "queue",
                            "sink_drain_failed",
                            format_args!("after={:?} error={error}", started.elapsed()),
                        );
                        return Err(PlayError::Sink(error));
                    }
                    chilpa_audio::diagnostics::trace(
                        "queue",
                        "sink_drain_finished",
                        format_args!("after={:?}", started.elapsed()),
                    );
                }
                let started = std::time::Instant::now();
                if let Err(error) = old.handoff() {
                    chilpa_audio::diagnostics::trace(
                        "queue",
                        "sink_close_failed",
                        format_args!("after={:?} format={old_format:?} error={error}", started.elapsed()),
                    );
                    return Err(PlayError::Sink(error));
                }
                chilpa_audio::diagnostics::trace(
                    "queue",
                    "sink_closed",
                    format_args!("after={:?} format={old_format:?}", started.elapsed()),
                );
            }
            let started = std::time::Instant::now();
            chilpa_audio::diagnostics::trace(
                "queue",
                "sink_open_started",
                format_args!("format={:?}", delivery.opened),
            );
            let new = match open_sink(delivery.opened) {
                Ok(sink) => sink,
                Err(error) => {
                    chilpa_audio::diagnostics::trace(
                        "queue",
                        "sink_open_failed",
                        format_args!("after={:?} format={:?} error={error}", started.elapsed(), delivery.opened),
                    );
                    return Err(PlayError::Sink(error));
                }
            };
            chilpa_audio::diagnostics::trace(
                "queue",
                "sink_opened",
                format_args!("after={:?} format={:?}", started.elapsed(), delivery.opened),
            );
            opened = Some((delivery.opened, crate::playback_events::Measured::new(new)));
            packer = DopPacker::new();
        }
        let (_, sink) = opened.as_mut().expect("freshly opened");
        if skipped_before && keep_open {
            chilpa_audio::diagnostics::trace("queue", "sink_flush_started", format_args!("reason=skip"));
            sink.flush().map_err(PlayError::Sink)?;
            chilpa_audio::diagnostics::trace("queue", "sink_flush_finished", format_args!("reason=skip"));
        }

        // The volume touches the samples only off the bit-perfect policy;
        // normalization, when it is on, under any (QBZ: "This breaks strict
        // bit perfect"). Either one that changes anything is said in the
        // delivery, so the verdict for that track is not bit-perfect.
        let shape = match &source {
            Source::Pcm(decoded, ..) => Some((decoded.stream_format().sample_rate_hz, decoded.stream_format().channels, decoded.frame_count())),
            Source::DsdPcm(converter) => Some((converter.output_rate(), converter.channels(), Some(converter.total_frames()))),
            _ => None,
        };
        // Raw one-bit sources never enter a PCM gain stage. A saved PCM
        // volume must not mark them processed or make the player refuse them.
        let volume = (shape.is_some() && policy != Policy::BitPerfect).then(|| volume.clone());
        let mut level = match shape {
            Some((rate, channels, frames)) if normalize.get() => {
                let key = path.to_string_lossy();
                let said = match &source { Source::Pcm(_, _, data) => data.and_then(crate::remote::Normalization::valid).map(|d| (d.gain_db, d.peak)), _ => None };
                crate::loudness::level::Leveler::normalizing(
                    crate::loudness::level::Shape { key: &key, rate, channels, frames },
                    crate::loudness::level::Normalize { target: normalize_target.get(), prevent_clipping: prevent_clipping.get() },
                    crate::loudness::cache::shared(),
                    crate::loudness::analyzer::sender(),
                    said,
                    || about.tags.as_ref().map(|tags| tags.replaygain()).unwrap_or_default(),
                    volume.clone(),
                )
            }
            _ => crate::loudness::level::Leveler::volume_only(volume.clone()),
        };
        if level.touches() || volume.as_ref().is_some_and(|volume| volume.get() != 1.0) {
            delivery.transforms.push(chilpa_audio::negotiate::Transform::VolumeApplied);
        }
        if shape.is_some() && controls.dsp.requested() {
            delivery.transforms.push(chilpa_audio::negotiate::Transform::DspApplied);
        }
        let observer = match &source {
            Source::Pcm(_, observer, _) => observer.clone(),
            _ => None,
        };
        if shape.is_none() { dsp = None; }
        else if dsp.is_none() || !keep_open { dsp = Some(chilpa_audio::dsp::Processor::new(controls.dsp.clone(), delivery.opened)); }
        else if skipped_before { if let Some(dsp) = &mut dsp { dsp.reset(); } }
        sink.set_dop(matches!(&source, Source::Dop(_)));
        sink.begin(observer);
        // The next track, opened ahead for gapless: a file at once, a track
        // over the network once this one has played a while (`Preload::at`).
        if gapless.get() {
            let next = if repeat.get() == Repeat::One { Some(index) } else { order.next(index, queue.len(), repeat.get() == Repeat::All) };
            if let Some(mut next) = next {
                if let Some(path) = next_live(queue, order, repeat, &mut next) {
                    let length = match &source {
                        Source::Pcm(decoded, _, _) => decoded.frame_count().map(|frames| frames as f64 / f64::from(decoded.stream_format().sample_rate_hz.max(1))),
                        _ => None,
                    };
                    let at = crate::preload::at(crate::remote::entry(&path).is_some(), delivery.opened.sample_rate_hz, length);
                    let (caps, dsd_mode, direct) = (caps.clone(), controls.dsd_mode.get(), controls.dsd_direct(caps));
                    preload.request_at(path, at, move |path| open_track(path, &caps, policy, dsd_mode, direct.as_ref()));
                }
            }
        }
        let on_progress = &mut |index: usize, progress: Progress| {
            preload.reached(progress.delivered);
            on_progress(index, progress)
        };
        controls.dsp.begin_stream();
        on_track(index, path, &delivery, &about);
        chilpa_audio::diagnostics::trace(
            "queue",
            "render_started",
            format_args!("index={index} kind={source_kind} format={:?}", delivery.opened),
        );
        let render_started = std::time::Instant::now();
        // Skipping a track ends the source just like a natural end, so it has
        // to be asked: with repeat-one, skipping advances.
        let mut gain_format = delivery.opened;
        if matches!(&source, Source::DsdPcm(_)) || matches!(&source, Source::Pcm(decoded, ..) if decoded.stream_format().sample_format == SampleFormat::F32) {
            gain_format.sample_format = SampleFormat::F32;
        } else if let Source::Pcm(decoded, ..) = &source {
            let original = decoded.stream_format().sample_format;
            if original.is_integer() && (gain_format.sample_format == SampleFormat::F32
                || (decoded.codec() == "vorbis" && original.valid_bits() > gain_format.sample_format.valid_bits())) {
                gain_format.sample_format = original;
            }
        }
        let mut output_level = level.take_output();
        let mut gain = |bytes: &mut [u8]| output_level.packed(bytes, gain_format, delivery.duplicates_mono());
        let output_gain = (shape.is_some() && (volume.is_some() || normalize.get())).then_some(&mut gain as &mut dyn FnMut(&mut [u8]));
        let (outcome, skipped) = match &mut source {
            Source::Pcm(decoded, _, _) => {
                let channels = decoded.stream_format().channels;
                let feed = controls.waveform.lock().map(|f| f.clone()).unwrap_or_default();
                if let Some(feed) = &feed {
                    feed.begin(path, channels, decoded.frame_count());
                }
                let trimmer = Trimmer::new(decoded.encoder_delay(), decoded.encoder_padding(), channels);
                let float_trimmer =
                    Trimmer::new(decoded.encoder_delay(), decoded.encoder_padding(), channels);
                let mut frames = TrackFrames {
                    decoded,
                    opened: delivery.opened,
                    channels: usize::from(channels.max(1)),
                    spread: delivery.duplicates_mono(),
                    trimmer,
                    float_trimmer,
                    exhausted: false,
                    seek: seek.clone(),
                    skip: skip.clone(),
                    skipped: false,
                    discard: 0,
                    moved: false,
                    base: std::sync::Arc::new(std::sync::atomic::AtomicI64::new(0)),
                    delivered: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
                    rewind_to: None,
                    moved_at: 0,
                    level,
                    gain_format,
                    feed: feed.clone(),
                    decoded_at: 0,
                    ahead: if crate::remote::entry(path).is_some() { chilpa_audio::render::NETWORK_AHEAD } else { chilpa_audio::render::AHEAD },
                };
                let base = std::sync::Arc::clone(&frames.base);
                let delivered = std::sync::Arc::clone(&frames.delivered);
                let outcome =
                    chilpa_audio::render::render_with_volume(&mut frames, sink, stop, pause, dsp.as_mut(), output_gain, &mut |f| {
                        delivered.store(f, std::sync::atomic::Ordering::Relaxed);
                        let at = f as i64 + base.load(std::sync::atomic::Ordering::Relaxed);
                        on_progress(index, Progress { position: at.max(0) as u64, delivered: f })
                    });
                if let Some(feed) = &feed {
                    feed.end(matches!(outcome, chilpa_audio::render::Outcome::Finished { .. }) && !frames.skipped);
                }
                (outcome, frames.skipped)
            }
            Source::Dop(reader) => {
                let mut frames = DopFrames::new(reader.as_mut(), &mut packer, &delivery)
                    .controlled(skip.clone(), seek.clone());
                let base = std::sync::Arc::clone(&frames.base);
                let delivered = std::sync::Arc::clone(&frames.delivered);
                let outcome = chilpa_audio::render::render_paused(&mut frames, sink, stop, pause, &mut |f| {
                    delivered.store(f, std::sync::atomic::Ordering::Relaxed);
                    let at = f as i64 + base.load(std::sync::atomic::Ordering::Relaxed);
                    on_progress(index, Progress { position: at.max(0) as u64, delivered: f })
                });
                (outcome, frames.skipped)
            }
            Source::Native(reader) => {
                let mut frames = NativeFrames::new(reader.as_mut(), delivery.opened.sample_format)
                    .controlled(skip.clone(), seek.clone());
                let base = std::sync::Arc::clone(&frames.base);
                let delivered = std::sync::Arc::clone(&frames.delivered);
                let outcome = chilpa_audio::render::render_paused(&mut frames, sink, stop, pause, &mut |f| {
                    delivered.store(f, std::sync::atomic::Ordering::Relaxed);
                    let at = f as i64 + base.load(std::sync::atomic::Ordering::Relaxed);
                    on_progress(index, Progress { position: at.max(0) as u64, delivered: f })
                });
                (outcome, frames.skipped)
            }
            Source::DsdPcm(converter) => {
                let feed = controls.waveform.lock().map(|f| f.clone()).unwrap_or_default();
                if let Some(feed) = &feed {
                    feed.begin(path, converter.channels(), Some(converter.total_frames()));
                }
                let mut frames = DsdPcmFrames::new(converter, gain_format, skip.clone(), seek.clone(), level, feed.clone());
                let base = std::sync::Arc::clone(&frames.base);
                let delivered = std::sync::Arc::clone(&frames.delivered);
                let outcome = chilpa_audio::render::render_with_volume(
                    &mut frames,
                    sink,
                    stop,
                    pause,
                    dsp.as_mut(),
                    output_gain,
                    &mut |frames| {
                        delivered.store(frames, std::sync::atomic::Ordering::Relaxed);
                        let position = (frames as i64 + base.load(std::sync::atomic::Ordering::Relaxed)).max(0) as u64;
                        on_progress(index, Progress { position, delivered: frames })
                    },
                );
                if let Some(feed) = &feed {
                    feed.end(matches!(outcome, chilpa_audio::render::Outcome::Finished { .. }) && !frames.skipped);
                }
                (outcome, frames.skipped)
            }
        };
        chilpa_audio::diagnostics::trace(
            "queue",
            "render_finished",
            format_args!(
                "after={:?} index={index} kind={source_kind} skipped={skipped} outcome={outcome:?}",
                render_started.elapsed()
            ),
        );
        sink.end(if skipped { "skipped" } else {
            match &outcome {
                Outcome::Finished { .. } => "finished",
                Outcome::Stopped { .. } => "stopped",
                _ => "error",
            }
        });
        let finished_ok = outcome.may_advance_queue();
        ended_by_itself = finished_ok && !skipped;
        skipped_before = skipped;
        consecutive_silent = if outcome.frames() == 0 { consecutive_silent + 1 } else { 0 };
        results.push(TrackResult {
            path: path.clone(),
            delivery,
            outcome,
            transition: transition,
        });
        if !finished_ok || consecutive_silent > queue.len() {
            break;
        }
        if stop_after.take_at(index) {
            break;
        }
        let mode = repeat.get();
        if mode == Repeat::One && !skipped {
            continue;
        }
        match order.next(index, queue.len(), mode == Repeat::All) {
            Some(n) => index = n,
            None => break,
        }
    }

    if let Some((format, mut sink)) = opened.take() {
        if ended_by_itself {
            let started = std::time::Instant::now();
            chilpa_audio::diagnostics::trace("queue", "sink_drain_started", format_args!("format={format:?} final=true"));
            if let Err(error) = sink.drain(&give_up) {
                chilpa_audio::diagnostics::trace(
                    "queue",
                    "sink_drain_failed",
                    format_args!("after={:?} final=true error={error}", started.elapsed()),
                );
                return Err(PlayError::Sink(error));
            }
            chilpa_audio::diagnostics::trace(
                "queue",
                "sink_drain_finished",
                format_args!("after={:?} final=true", started.elapsed()),
            );
        }
        let started = std::time::Instant::now();
        let closing = if controls.handoff.get() { "handoff" } else { "close" };
        let result = if controls.handoff.get() { sink.handoff() } else { sink.close() };
        if let Err(error) = result {
            chilpa_audio::diagnostics::trace(
                "queue",
                "sink_close_failed",
                format_args!("after={:?} final=true mode={closing} error={error}", started.elapsed()),
            );
            return Err(PlayError::Sink(error));
        }
        chilpa_audio::diagnostics::trace(
            "queue",
            "sink_closed",
            format_args!("after={:?} format={format:?} final=true mode={closing}", started.elapsed()),
        );
    }
    // Every track it reached was on a server that did not answer: that is
    // what the window says, not that the queue ended.
    if let (true, Some(error)) = (results.is_empty(), passed_over) {
        return Err(error);
    }
    Ok(results)
}

/// The track at `index`, skipping the removed ones.
///
/// Advances `index` until it finds a live one. Returns `None` when none
/// are left, counting laps so as not to spin forever if the whole queue
/// was removed with repeat turned on.
fn next_live(
    queue: &SharedQueue,
    order: &Order,
    repeat: &RepeatMode,
    index: &mut usize,
) -> Option<PathBuf> {
    let wrap = repeat.get() == Repeat::All;
    for _ in 0..=queue.len() {
        if let Some(p) = queue.get(*index) {
            return Some(p);
        }
        if *index >= queue.len() {
            return None;
        }
        *index = order.next(*index, queue.len(), wrap)?;
    }
    None
}

/// How many times the device was reopened over an already played queue.
pub fn renegotiations(results: &[TrackResult]) -> usize {
    results.iter().filter(|r| !r.transition.is_seamless()).count()
}

/// The tracks that did play all the way through.
pub fn finished(results: &[TrackResult]) -> impl Iterator<Item = &Path> {
    results
        .iter()
        .filter(|r| r.outcome.may_advance_queue())
        .map(|r| r.path.as_path())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_an_order_of_its_own_the_queue_plays_as_it_came() {
        let o = Order::new();
        assert_eq!(o.first(3), 0);
        assert_eq!(o.next(0, 3, false), Some(1));
        assert_eq!(o.next(2, 3, false), None, "it ends there");
        assert_eq!(o.next(2, 3, true), Some(0), "with repeat, it goes back to the first");
    }

    #[test]
    fn an_explicit_order_wins() {
        let o = Order::new();
        assert!(o.set(vec![2, 0, 1], 3));
        assert_eq!(o.first(3), 2);
        assert_eq!(o.next(2, 3, false), Some(0));
        assert_eq!(o.next(1, 3, false), None, "1 is the last of the order");
        assert_eq!(o.next(1, 3, true), Some(2));
    }

    #[test]
    fn changing_the_order_mid_queue_continues_from_where_it_was() {
        let o = Order::new();
        assert!(o.set(vec![0, 1, 2, 3], 4));
        // Track 1 plays and the order changes: what comes next is whatever follows it now.
        assert!(o.set(vec![3, 1, 0, 2], 4));
        assert_eq!(o.next(1, 4, false), Some(0));
    }

    #[test]
    fn an_order_that_is_not_a_permutation_is_refused() {
        let o = Order::new();
        assert!(!o.set(vec![0, 0, 1], 3), "repeated");
        assert!(!o.set(vec![0, 1], 3), "incomplete");
        assert!(!o.set(vec![0, 1, 5], 3), "out of range");
        assert!(o.get().is_empty(), "none of that was kept");
    }

    #[test]
    fn shuffling_gives_a_permutation_and_not_always_the_same_one() {
        for seed in [1u64, 42, 99_999] {
            let o = shuffled(8, seed);
            assert!(is_permutation(&o, 8), "{o:?} is not a permutation");
        }
        assert_ne!(shuffled(16, 1), shuffled(16, 2), "different seeds, different shuffles");
        assert_eq!(shuffled(16, 7), shuffled(16, 7), "the same seed always gives the same thing");
        assert_eq!(shuffled(1, 3), vec![0], "a single track is not shuffled");
        assert!(shuffled(0, 3).is_empty());
    }

    #[test]
    fn clearing_restores_the_array_order() {
        let o = Order::new();
        assert!(o.set(vec![1, 0], 2));
        o.clear();
        assert_eq!(o.first(2), 0);
    }
}
