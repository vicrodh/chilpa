//! What a track's samples go through on their way out, in QBZ's order
//! (`AnalyzerTap`, then `DynamicAmplify`): a copy for the analyzer as
//! decoded, then the normalization gain and the player bar's volume.

use super::analyzer::{Message, Track};
use super::cache::LoudnessCache;
use super::ramp::{scale_int, GainRamp};
use crate::queue_playback::Level;
use std::sync::atomic::AtomicU32;
use std::sync::mpsc::{SyncSender, TrySendError};
use std::sync::Arc;

/// What normalization is set to, read as each track starts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Normalize {
    pub target: f32,
    pub prevent_clipping: bool,
}

/// The track being leveled: its key in the cache, its own rate and
/// channels (before a mono track is spread), and its length when known.
pub struct Shape<'a> {
    pub key: &'a str,
    pub rate: u32,
    pub channels: u16,
    pub frames: Option<u64>,
}

pub struct Leveler {
    ramp: Option<GainRamp>,
    volume: Option<Level>,
    analyzer: Option<SyncSender<Message>>,
    /// The track's announcement, until the analyzer's channel takes it: a
    /// block sent before it would be measured as the previous track's.
    pending: Option<Track>,
    reset: bool,
    /// The start gain, and whether it was known.
    start: super::StartGain,
}

impl Leveler {
    /// The volume alone: normalization is off.
    pub fn volume_only(volume: Option<Level>) -> Self {
        Self { ramp: None, volume, analyzer: None, pending: None, reset: false, start: super::StartGain { gain: 1.0, known: true } }
    }

    /// Normalization on for one track: its gain worked out before its first
    /// sample (QBZ, `plan_start_gain`), and the analyzer told it starts.
    pub fn normalizing(
        shape: Shape<'_>,
        settings: Normalize,
        cache: &LoudnessCache,
        analyzer: SyncSender<Message>,
        said: Option<(f32, Option<f32>)>,
        tags: impl FnOnce() -> (Option<f32>, Option<f32>),
        volume: Option<Level>,
    ) -> Self {
        let start = super::start_gain(cache, shape.key, said, tags, settings.target, settings.prevent_clipping);
        chilpa_audio::diagnostics::trace(
            "loudness",
            "start_gain",
            format_args!("key={} gain={:.4} known={}", shape.key, start.gain, start.known),
        );
        let shared = Arc::new(AtomicU32::new(start.gain.to_bits()));
        let rate = shape.rate.max(1);
        let track = Track {
            key: shape.key.to_string(),
            rate,
            channels: shape.channels.max(1),
            duration_secs: shape.frames.map_or(0, |frames| frames / u64::from(rate)),
            target: settings.target,
            prevent_clipping: settings.prevent_clipping,
            gain: Arc::clone(&shared),
            known: start.known,
        };
        Self {
            ramp: Some(GainRamp::new(shared, start.gain, rate, shape.channels.max(1))),
            volume,
            analyzer: Some(analyzer),
            pending: Some(track),
            reset: false,
            start,
        }
    }

    /// Whether the samples change: a start gain that is not unity or not
    /// known yet, or a volume under full.
    pub fn touches(&self) -> bool {
        (self.ramp.is_some() && (!self.start.known || self.start.gain != 1.0))
            || self.volume.as_ref().is_some_and(|volume| volume.get() != 1.0)
    }

    /// A seek: the analyzer's window starts over, the gain stays.
    pub fn moved(&mut self) {
        self.reset = self.analyzer.is_some();
    }

    /// Hands the analyzer a copy of a block without ever waiting: a full
    /// channel drops it.
    fn tell(&mut self, copy: impl FnOnce() -> Vec<f32>) {
        let Some(sender) = &self.analyzer else { return };
        if let Some(track) = self.pending.take() {
            match sender.try_send(Message::Track(track)) {
                Ok(()) => {}
                Err(TrySendError::Full(Message::Track(track))) => {
                    self.pending = Some(track);
                    return;
                }
                Err(_) => {
                    self.analyzer = None;
                    return;
                }
            }
        }
        if self.reset {
            if sender.try_send(Message::Reset).is_err() {
                return;
            }
            self.reset = false;
        }
        let _ = sender.try_send(Message::Samples(copy()));
    }

    pub fn int(&mut self, frames: &mut [i32], bits: u8) {
        if self.analyzer.is_some() {
            let scale = 1.0 / (1u64 << (bits.clamp(2, 32) - 1)) as f32;
            self.tell(|| frames.iter().map(|v| *v as f32 * scale).collect());
        }
        let volume = self.volume.as_ref().map_or(1.0, Level::get);
        match &mut self.ramp {
            Some(ramp) => ramp.apply_int(frames, bits, volume),
            None if volume != 1.0 => scale_int(frames, bits, volume),
            None => {}
        }
    }

    pub fn float(&mut self, frames: &mut [f32]) {
        if self.analyzer.is_some() {
            self.tell(|| frames.to_vec());
        }
        let volume = self.volume.as_ref().map_or(1.0, Level::get);
        match &mut self.ramp {
            Some(ramp) => ramp.apply_float(frames, volume),
            None if volume != 1.0 => frames.iter_mut().for_each(|v| *v *= volume),
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::sync_channel;

    fn shape(key: &str) -> Shape<'_> {
        Shape { key, rate: 1000, channels: 1, frames: Some(60_000) }
    }

    const ON: Normalize = Normalize { target: -14.0, prevent_clipping: true };

    #[test]
    fn off_it_is_the_volume_and_nothing_else() {
        let mut level = Leveler::volume_only(Some(Level::new(0.5)));
        let mut floats = vec![1.0f32, -1.0];
        level.float(&mut floats);
        assert_eq!(floats, vec![0.5, -0.5]);
        let mut ints = vec![1000, -1000];
        level.int(&mut ints, 16);
        assert_eq!(ints, vec![500, -500]);
        assert!(level.touches());
        assert!(!Leveler::volume_only(Some(Level::new(1.0))).touches());
        assert!(!Leveler::volume_only(None).touches());
    }

    #[test]
    fn a_known_gain_is_on_the_first_sample_and_the_analyzer_hears_the_track_as_decoded() {
        let cache = LoudnessCache::in_memory();
        let (sender, receiver) = sync_channel(64);
        // ReplayGain -10 dB: -8 LUFS, six over a -14 target.
        let mut level = Leveler::normalizing(shape("a"), ON, &cache, sender, Some((-10.0, None)), || (None, None), None);
        assert!(level.touches());
        let mut ints = vec![16384];
        level.int(&mut ints, 16);
        assert_eq!(ints, vec![(16384.0 * super::super::db_to_linear(-6.0)).round() as i32]);
        let Ok(Message::Track(track)) = receiver.try_recv() else { panic!("the track comes first") };
        assert!(track.known && track.key == "a" && track.duration_secs == 60);
        let Ok(Message::Samples(copy)) = receiver.try_recv() else { panic!("then its samples") };
        assert_eq!(copy, vec![0.5], "before the gain, as a float");
    }

    #[test]
    fn an_unknown_gain_starts_at_unity_and_still_says_it_touches() {
        let cache = LoudnessCache::in_memory();
        let (sender, _receiver) = sync_channel(64);
        let mut level = Leveler::normalizing(shape("b"), ON, &cache, sender, None, || (None, None), None);
        assert!(level.touches(), "the analyzer may move it");
        let mut floats = vec![0.25f32];
        level.float(&mut floats);
        assert_eq!(floats, vec![0.25]);
    }

    #[test]
    fn a_full_channel_keeps_the_announcement_and_drops_the_block() {
        let cache = LoudnessCache::in_memory();
        let (sender, receiver) = sync_channel(1);
        sender.try_send(Message::Reset).unwrap();
        let mut level = Leveler::normalizing(shape("c"), ON, &cache, sender, None, || (None, None), None);
        level.float(&mut [0.1]);
        assert!(matches!(receiver.try_recv(), Ok(Message::Reset)));
        assert!(receiver.try_recv().is_err(), "the block went nowhere");
        level.moved();
        level.float(&mut [0.2]);
        assert!(matches!(receiver.try_recv(), Ok(Message::Track(_))), "the track before anything of it");
        assert!(receiver.try_recv().is_err());
        level.float(&mut [0.3]);
        assert!(matches!(receiver.try_recv(), Ok(Message::Reset)));
        level.float(&mut [0.4]);
        assert!(matches!(receiver.try_recv(), Ok(Message::Samples(_))));
    }
}
