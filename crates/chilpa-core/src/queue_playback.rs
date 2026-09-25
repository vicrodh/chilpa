//! Playing several tracks in a row.
//!
//! The queue chains with no gap as long as the stream format does not
//! change. If it changes, the device has to be reopened and the break is
//! unavoidable: it is reported, not hidden.
//!
//! Every track ends with its own [`Outcome`], and only a natural end lets
//! the next one through. A device failure stops the queue right there, which
//! is exactly what QBZ did not do: it skipped everything in silence.

use chilpa_audio::format::StreamFormat;
use chilpa_audio::gapless::{transition, Transition, Trimmer};
use chilpa_audio::negotiate::{negotiate, DeviceCaps, Delivery, Policy};
use chilpa_audio::pack::{duplicate_mono, pack, Samples};
use chilpa_audio::render::{FrameSource, Outcome};
use chilpa_audio::seek::SeekSignal;
use chilpa_audio::sink::{AudioSink, PauseSignal, SinkError, SkipSignal, StopSignal};
use chilpa_decode::{Block, Decoded};
use std::path::{Path, PathBuf};

use crate::dsd_playback::{dop_request, is_dsd_file, open_dsd_file, DopFrames};
use crate::playback::PlayError;
use chilpa_audio::dop::DopPacker;
use chilpa_dsd::DsdSource;

/// What a track is read from: the decoder for PCM, a DSD source for DoP.
enum Source {
    Pcm(Decoded),
    Dop(Box<dyn DsdSource>),
}

/// Opens a track by what it is, and negotiates what it asks for.
///
/// DSD used to be played outside the queue, on the first track only: a
/// queue of DSF files played its first file and stopped, because nothing
/// took the queue on from there. It goes through the queue like any track
/// now, always as bit-perfect, since converting DoP turns it into noise.
fn open_track(path: &Path, caps: &DeviceCaps, policy: Policy) -> Result<(Source, Delivery), PlayError> {
    let unreadable = |e: String| PlayError::Sink(SinkError::Backend(e));
    if let Some((image, number)) = crate::sacd_tracks::parse(path) {
        let reader = crate::sacd_tracks::open(&image, number).map_err(unreadable)?;
        return dop_track(Box::new(reader), caps);
    }
    if is_dsd_file(path) {
        let reader = open_dsd_file(path).map_err(|e| unreadable(e.to_string()))?;
        return dop_track(reader, caps);
    }
    let decoded = Decoded::open(path).map_err(PlayError::Decode)?;
    let delivery = negotiate(decoded.stream_format(), caps, policy).map_err(PlayError::Negotiation)?;
    Ok((Source::Pcm(decoded), delivery))
}

/// A DSD source, negotiated as bit-perfect: converting DoP turns it into
/// noise, so the policy is not asked.
fn dop_track(reader: Box<dyn DsdSource>, caps: &DeviceCaps) -> Result<(Source, Delivery), PlayError> {
    let request = dop_request(reader.info());
    let delivery = negotiate(request, caps, Policy::BitPerfect).map_err(|_| {
        PlayError::Sink(SinkError::Backend(chilpa_i18n::t_args(
            "the device does not accept {} Hz, which is what this DSD over DoP asks for",
            &[&request.sample_rate_hz.to_string()],
        )))
    })?;
    Ok((Source::Dop(reader), delivery))
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
#[derive(Clone, Default)]
pub struct Controls {
    pub stop: StopSignal,
    pub pause: PauseSignal,
    pub seek: SeekSignal,
    pub skip: SkipSignal,
    pub repeat: RepeatMode,
    pub order: Order,
}

impl Controls {
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
    /// What to add to the frames delivered to get the position in the file:
    /// where the last seek landed less what had been delivered by then. It
    /// used to be where the seek landed alone, and the renderer counts from
    /// the start of the track, so a seek to second 2 after one second had
    /// gone out reported second 3.
    base: std::sync::Arc<std::sync::atomic::AtomicI64>,
    /// Frames the device has taken of this track, as the renderer reports
    /// them.
    delivered: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

impl FrameSource for TrackFrames<'_> {
    fn next_block(&mut self) -> Result<Option<Vec<u8>>, String> {
        loop {
            if self.exhausted {
                return Ok(None);
            }
            if self.skip.take() {
                self.skipped = true;
                self.exhausted = true;
                self.finish_trim();
                return Ok(None);
            }
            if let Some(target) = self.seek.take() {
                let frame = chilpa_audio::seek::frames_at(target, self.opened.sample_rate_hz);
                match self.decoded.seek_to_frame(frame) {
                    Ok(plan) => {
                        self.discard = plan.discard;
                        // The point ASKED FOR, not where the container
                        // landed: the discarded frames never come out, so
                        // counting them separately would leave the position
                        // short.
                        let delivered = self.delivered.load(std::sync::atomic::Ordering::Relaxed);
                        self.base.store(
                            frame as i64 - delivered as i64,
                            std::sync::atomic::Ordering::Relaxed,
                        );
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
                    let frames = self.trimmer.push(&data);
                    if frames.is_empty() {
                        continue;
                    }
                    let frames = if self.spread { duplicate_mono(&frames) } else { frames };
                    let mut bytes = Vec::new();
                    let samples = Samples::Int { data: &frames, bits };
                    pack(&samples, self.opened.sample_format, &mut bytes)
                        .map_err(|e| PlayError::Pack(e).to_string())?;
                    bytes
                }
                Ok(Some(Block::Float(data))) => {
                    let channels = self.channels;
                    let data = self.drop_discard(data, channels);
                    if data.is_empty() {
                        continue;
                    }
                    let frames = self.float_trimmer.push(&data);
                    if frames.is_empty() {
                        continue;
                    }
                    let frames = if self.spread { duplicate_mono(&frames) } else { frames };
                    let mut bytes = Vec::new();
                    pack(&Samples::Float(&frames), self.opened.sample_format, &mut bytes)
                        .map_err(|e| PlayError::Pack(e).to_string())?;
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
    mut open_sink: F,
    on_track: &mut dyn FnMut(usize, &Path, &Delivery),
    on_progress: &mut dyn FnMut(usize, Progress),
) -> Result<Vec<TrackResult>, PlayError>
where
    F: FnMut(StreamFormat) -> Result<K, SinkError>,
    K: AudioSink,
{
    let Controls { stop, pause, seek, skip, repeat, order } = controls;
    let mut results = Vec::new();
    let mut opened: Option<(StreamFormat, K)> = None;
    let mut index = order.first(queue.len());
    // A file that delivers not one frame plus repeat turned on would be a
    // loop that spins forever. How many tracks in a row did not play is
    // counted, and it cuts off after one full lap.
    let mut consecutive_silent = 0usize;
    // The DoP marker of whatever is open. It carries on across tracks that
    // share an opening and starts over with a new one.
    let mut packer = DopPacker::new();
    // Whether the track before ended by itself: then what is still in the
    // device is its last part, and it is waited for before the device is
    // closed. Skipped or stopped, it is not.
    let mut ended_by_itself = false;
    // A stop, or a skip of what is still playing, ends the wait.
    let give_up = || stop.is_stopped() || skip.take();

    // The length is read again on every lap: the queue can grow while it
    // plays.
    loop {
        // A removed track is skipped without playing; if none are left, the
        // queue is over.
        let Some(path) = next_live(queue, order, repeat, &mut index) else { break };
        let path = &path;
        let (mut source, delivery) = open_track(path, caps, policy)?;

        let transition = match &opened {
            Some((actual, _)) => transition(*actual, delivery.opened),
            None => Transition::Seamless,
        };
        if !transition.is_seamless() || opened.is_none() {
            if let Some((_, mut old)) = opened.take() {
                if ended_by_itself {
                    old.drain(&give_up).map_err(PlayError::Sink)?;
                }
                old.close().map_err(PlayError::Sink)?;
            }
            let new = open_sink(delivery.opened).map_err(PlayError::Sink)?;
            opened = Some((delivery.opened, new));
            packer = DopPacker::new();
        }
        let (_, sink) = opened.as_mut().expect("freshly opened");

        on_track(index, path, &delivery);
        // Skipping a track ends the source just like a natural end, so it has
        // to be asked: with repeat-one, skipping advances.
        let (outcome, skipped) = match &mut source {
            Source::Pcm(decoded) => {
                let channels = decoded.stream_format().channels;
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
                    base: std::sync::Arc::new(std::sync::atomic::AtomicI64::new(0)),
                    delivered: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
                };
                let base = std::sync::Arc::clone(&frames.base);
                let delivered = std::sync::Arc::clone(&frames.delivered);
                let outcome =
                    chilpa_audio::render::render_paused(&mut frames, sink, stop, pause, &mut |f| {
                        delivered.store(f, std::sync::atomic::Ordering::Relaxed);
                        let at = f as i64 + base.load(std::sync::atomic::Ordering::Relaxed);
                        on_progress(index, Progress { position: at.max(0) as u64, delivered: f })
                    });
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
        };
        let finished_ok = outcome.may_advance_queue();
        ended_by_itself = finished_ok && !skipped;
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
        let mode = repeat.get();
        if mode == Repeat::One && !skipped {
            continue;
        }
        match order.next(index, queue.len(), mode == Repeat::All) {
            Some(n) => index = n,
            None => break,
        }
    }

    if let Some((_, mut sink)) = opened.take() {
        if ended_by_itself {
            sink.drain(&give_up).map_err(PlayError::Sink)?;
        }
        sink.close().map_err(PlayError::Sink)?;
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
