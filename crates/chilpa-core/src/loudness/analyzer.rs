//! The live loudness analyzer (QBZ, `loudness_analyzer.rs`): one thread for
//! the whole process, asleep between tracks, that measures what plays
//! (BS.1770 integrated loudness and true peak) and sets the gain once when
//! nothing was known at the start.
//!
//! - The first measurement comes after ten seconds of audio; it moves the
//!   gain only when the start gain was unknown, and only once per track.
//! - Every five seconds after that a measurement refines the cache.
//! - One more measurement when the window reaches two seconds before the
//!   end: that one is the whole track's figure.
//! - A window under 30 seconds is not kept: a skip at twelve seconds is not
//!   the loudness of the track.
//!
//! The playback thread never waits for it: samples go with `try_send`, and
//! when the channel is full a block is dropped.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, OnceLock};

use ebur128::{EbuR128, Mode};

use super::cache::{LoudnessCache, Rank};

const MIN_KEPT_SECS: u64 = 30;
const END_WINDOW_SECS: u64 = 2;

/// What the analyzer is told about a track as it starts.
pub struct Track {
    pub key: String,
    pub rate: u32,
    pub channels: u16,
    /// Its length; 0 when unknown, and then there is no end measurement.
    pub duration_secs: u64,
    pub target: f32,
    pub prevent_clipping: bool,
    /// The gain the ramp reads, written here once when it was not known.
    pub gain: Arc<AtomicU32>,
    pub known: bool,
}

pub enum Message {
    Track(Track),
    /// Interleaved samples, as decoded.
    Samples(Vec<f32>),
    /// A seek: the window starts over, the gain stays.
    Reset,
}

/// Where to send what plays. The thread starts the first time it is asked
/// for.
pub fn sender() -> SyncSender<Message> {
    static SENDER: OnceLock<SyncSender<Message>> = OnceLock::new();
    SENDER
        .get_or_init(|| {
            let (sender, receiver) = sync_channel(64);
            let started = std::thread::Builder::new()
                .name("chilpa-loudness".into())
                .spawn(move || run(receiver, super::cache::shared()));
            if let Err(error) = started {
                eprintln!("loudness: the analyzer could not start: {error}");
            }
            sender
        })
        .clone()
}

/// Sends without waiting; a full channel drops the message.
pub fn send(sender: &SyncSender<Message>, message: Message) {
    if let Err(TrySendError::Disconnected(_)) = sender.try_send(message) {
        eprintln!("loudness: the analyzer is gone");
    }
}

fn run(receiver: Receiver<Message>, cache: &LoudnessCache) {
    let mut state: Option<State> = None;
    while let Ok(message) = receiver.recv() {
        match message {
            Message::Track(track) => state = State::new(track),
            Message::Samples(samples) => {
                if let Some(state) = &mut state {
                    state.feed(&samples, cache);
                }
            }
            Message::Reset => {
                if let Some(state) = &mut state {
                    state.reset();
                }
            }
        }
    }
}

struct State {
    track: Track,
    meter: EbuR128,
    /// Samples (all channels) fed since the window started.
    fed: u64,
    fed_at_last: u64,
    first_done: bool,
    /// The live gain was written for this track; a seek does not undo it.
    gain_written: bool,
    end_done: bool,
    first_after: u64,
    every: u64,
}

impl State {
    fn new(track: Track) -> Option<Self> {
        let meter = EbuR128::new(u32::from(track.channels), track.rate, Mode::I | Mode::TRUE_PEAK).ok()?;
        let per_second = u64::from(track.rate) * u64::from(track.channels);
        Some(Self {
            meter,
            fed: 0,
            fed_at_last: 0,
            first_done: false,
            gain_written: false,
            end_done: false,
            first_after: per_second * 10,
            every: per_second * 5,
            track,
        })
    }

    fn reset(&mut self) {
        if let Ok(meter) = EbuR128::new(u32::from(self.track.channels), self.track.rate, Mode::I | Mode::TRUE_PEAK) {
            self.meter = meter;
        }
        self.fed = 0;
        self.fed_at_last = 0;
        self.first_done = false;
        self.end_done = false;
    }

    fn seconds(&self) -> u64 {
        self.fed / (u64::from(self.track.rate) * u64::from(self.track.channels)).max(1)
    }

    fn feed(&mut self, samples: &[f32], cache: &LoudnessCache) {
        if samples.len() < usize::from(self.track.channels) || self.meter.add_frames_f32(samples).is_err() {
            return;
        }
        self.fed += samples.len() as u64;
        let near_end = self.track.duration_secs > 0 && !self.end_done && self.seconds() + END_WINDOW_SECS >= self.track.duration_secs;
        let due = near_end
            || if self.first_done { self.fed - self.fed_at_last >= self.every } else { self.fed >= self.first_after };
        if due {
            if near_end {
                self.end_done = true;
            }
            self.measure(cache);
        }
    }

    fn measure(&mut self, cache: &LoudnessCache) {
        let Ok(loudness) = self.meter.loudness_global() else { return };
        // Silence measures minus infinity: nothing to level.
        if !loudness.is_finite() {
            return;
        }
        let lufs = loudness as f32;
        let peak = (0..u32::from(self.track.channels)).filter_map(|c| self.meter.true_peak(c).ok()).fold(0.0f64, f64::max) as f32;
        let peak = (peak > 0.0).then_some(peak);
        let gain = super::gain_for(lufs, peak, self.track.target, self.track.prevent_clipping);
        let seconds = self.seconds();
        // The live gain moves at most once per track, and only when nothing
        // was known at the start: a later window must not level the song
        // again.
        if !self.track.known && !self.gain_written {
            self.track.gain.store(gain.to_bits(), Ordering::Relaxed);
            self.gain_written = true;
            eprintln!("loudness: {} measured {lufs:.1} LUFS after {seconds} s, gain {gain:.3}", self.track.key);
        }
        self.fed_at_last = self.fed;
        self.first_done = true;
        if self.track.duration_secs > 0 && seconds + END_WINDOW_SECS >= self.track.duration_secs {
            cache.store(&self.track.key, lufs, peak, Rank::Measured);
        } else if seconds >= MIN_KEPT_SECS {
            cache.store(&self.track.key, lufs, peak, Rank::Partial);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sine at `amplitude` on every channel, `seconds` long.
    fn tone(rate: u32, channels: u16, seconds: f32, amplitude: f32) -> Vec<f32> {
        let frames = (rate as f32 * seconds) as usize;
        let mut out = Vec::with_capacity(frames * usize::from(channels));
        for i in 0..frames {
            let v = amplitude * (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / rate as f32).sin();
            for _ in 0..channels {
                out.push(v);
            }
        }
        out
    }

    fn track(key: &str, duration_secs: u64, known: bool) -> (Track, Arc<AtomicU32>) {
        let gain = Arc::new(AtomicU32::new(1.0f32.to_bits()));
        (Track { key: key.into(), rate: 8000, channels: 2, duration_secs, target: -14.0, prevent_clipping: true, gain: gain.clone(), known }, gain)
    }

    fn feed_seconds(state: &mut State, cache: &LoudnessCache, seconds: u32) {
        for _ in 0..seconds {
            state.feed(&tone(8000, 2, 1.0, 0.5), cache);
        }
    }

    #[test]
    fn an_unknown_start_is_set_once_after_ten_seconds_and_kept_from_thirty() {
        let cache = LoudnessCache::in_memory();
        let (t, gain) = track("unknown", 40, false);
        let mut state = State::new(t).unwrap();
        feed_seconds(&mut state, &cache, 9);
        assert_eq!(f32::from_bits(gain.load(Ordering::Relaxed)), 1.0, "nothing before ten seconds");
        feed_seconds(&mut state, &cache, 2);
        let first = f32::from_bits(gain.load(Ordering::Relaxed));
        assert!(first != 1.0, "the first measurement sets the gain");
        assert!(cache.lookup("unknown").is_none(), "an eleven-second window is not kept");
        gain.store(0.123f32.to_bits(), Ordering::Relaxed);
        feed_seconds(&mut state, &cache, 20);
        assert_eq!(f32::from_bits(gain.load(Ordering::Relaxed)), 0.123, "later windows never move the gain");
        assert_eq!(cache.lookup("unknown").map(|r| r.rank), Some(Rank::Partial), "from thirty seconds on it is kept");
        feed_seconds(&mut state, &cache, 8);
        assert_eq!(cache.lookup("unknown").map(|r| r.rank), Some(Rank::Measured), "the last window is the whole track");
    }

    #[test]
    fn a_known_start_is_never_moved_but_still_kept() {
        let cache = LoudnessCache::in_memory();
        let (t, gain) = track("known", 0, true);
        let mut state = State::new(t).unwrap();
        feed_seconds(&mut state, &cache, 35);
        assert_eq!(f32::from_bits(gain.load(Ordering::Relaxed)), 1.0);
        assert_eq!(cache.lookup("known").map(|r| r.rank), Some(Rank::Partial));
    }

    #[test]
    fn a_seek_starts_the_window_over_and_keeps_the_gain() {
        let cache = LoudnessCache::in_memory();
        let (t, gain) = track("seek", 0, false);
        let mut state = State::new(t).unwrap();
        feed_seconds(&mut state, &cache, 11);
        let set = gain.load(Ordering::Relaxed);
        state.reset();
        assert_eq!(state.fed, 0);
        feed_seconds(&mut state, &cache, 11);
        assert_eq!(gain.load(Ordering::Relaxed), set, "the gain is written once, a seek or not");
    }

    #[test]
    fn a_sine_measures_where_the_standard_puts_it() {
        let mut meter = EbuR128::new(2, 48000, Mode::I | Mode::TRUE_PEAK).unwrap();
        meter.add_frames_f32(&tone(48000, 2, 5.0, 0.5)).unwrap();
        let lufs = meter.loudness_global().unwrap();
        assert!((-8.5..=-5.0).contains(&lufs), "{lufs}");
        let peak = meter.true_peak(0).unwrap();
        assert!((peak - 0.5).abs() < 0.03, "{peak}");
    }
}
