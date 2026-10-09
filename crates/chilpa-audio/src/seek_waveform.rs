//! The seek bar's waveform: how far a track swings along its length, in
//! a fixed number of bins, from whatever decodes it. Each bin keeps its
//! peak, which is what gives the dense grain of a QuickLook player's
//! waveform (the owner, 2026-10-09; see audiowaveform, which keeps peaks
//! too).
//!
//! Frames come in with their absolute position in the track, so a pass
//! that skips, seeks or stops early leaves holes where it did not look
//! instead of shifting every later bin. A document still growing is scaled
//! against a fixed reference, so the bins already painted do not breathe
//! when a louder passage arrives; a finished one is scaled to its loudest
//! bin. The view reduces the bins to the columns it has room for with
//! [`columns`].

pub const BINS: usize = 2048;
/// The most frames looked at per bin; a bin with more is strided.
const FRAMES_PER_BIN_LIMIT: u64 = 4096;
/// What a growing document is scaled against: most music peaks near
/// full scale, so the painted bins seldom move when the whole is known.
const GROWING_REFERENCE: f32 = 0.9;
/// How the peaks are lifted before they are drawn: quiet passages stay
/// visible without the loud ones flattening.
const GAMMA: f32 = 0.72;

/// Where a player hands the frames it decodes, with their position, so a
/// track that cannot be read apart from playing it still gets its
/// document as it goes.
pub trait Feed: Send + Sync {
    /// A track starts: `total_frames` unknown means no document.
    fn begin(&self, path: &std::path::Path, channels: u16, total_frames: Option<u64>);
    fn feed_float(&self, start_frame: u64, samples: &[f32]);
    fn feed_int(&self, start_frame: u64, samples: &[i32], bits: u8);
    /// The track is over, finished or not.
    fn end(&self);
}

/// The document as the view reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    /// Moves on every change, so a reader copies only when it did.
    pub revision: u64,
    pub bins: [u8; BINS],
    /// How many bins have at least one frame.
    pub analyzed: u16,
    pub complete: bool,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self { revision: 0, bins: [0; BINS], analyzed: 0, complete: false }
    }
}

/// Gathers a track's bins from frames handed in with their position.
pub struct Accumulator {
    total_frames: u64,
    channels: usize,
    peaks: [f32; BINS],
    counts: [u32; BINS],
    analyzed: u16,
    complete: bool,
}

impl Accumulator {
    pub fn new(channels: u16, total_frames: u64) -> Self {
        Self {
            total_frames,
            channels: usize::from(channels.max(1)),
            peaks: [0.0; BINS],
            counts: [0; BINS],
            analyzed: 0,
            complete: false,
        }
    }

    pub fn analyzed(&self) -> u16 {
        self.analyzed
    }

    pub fn complete(&self) -> bool {
        self.complete
    }

    /// Interleaved floats, the first frame at `start_frame`.
    pub fn feed_float(&mut self, start_frame: u64, samples: &[f32]) {
        let channels = self.channels;
        self.feed_with(start_frame, samples.len() / channels, |frame| {
            let base = frame * channels;
            samples[base..base + channels].iter().fold(0.0f32, |peak, s| peak.max(s.abs()))
        });
    }

    /// Interleaved integers, right-justified, with `bits` useful ones.
    pub fn feed_int(&mut self, start_frame: u64, samples: &[i32], bits: u8) {
        let channels = self.channels;
        let full = (1u32 << (bits.clamp(2, 32) - 1)) as f32;
        self.feed_with(start_frame, samples.len() / channels, |frame| {
            let base = frame * channels;
            samples[base..base + channels].iter().fold(0.0f32, |peak, s| peak.max((*s as f32).abs() / full))
        });
    }

    fn feed_with(&mut self, start_frame: u64, frames: usize, swing: impl Fn(usize) -> f32) {
        if self.complete || self.total_frames == 0 || frames == 0 {
            return;
        }
        let frames_per_bin = self.total_frames.div_ceil(BINS as u64);
        let stride = frames_per_bin.div_ceil(FRAMES_PER_BIN_LIMIT).max(1);
        for frame in 0..frames {
            let absolute = start_frame.saturating_add(frame as u64);
            if absolute >= self.total_frames || absolute % stride != 0 {
                continue;
            }
            let bin = ((u128::from(absolute) * BINS as u128) / u128::from(self.total_frames)) as usize;
            self.peaks[bin] = self.peaks[bin].max(swing(frame));
            if self.counts[bin] == 0 {
                self.analyzed = self.analyzed.saturating_add(1);
            }
            self.counts[bin] = self.counts[bin].saturating_add(1);
        }
        if usize::from(self.analyzed) == BINS {
            self.complete = true;
        }
    }

    /// Nothing more comes: what is known is the whole document, holes and
    /// all.
    pub fn finish(&mut self) {
        self.complete = true;
    }

    /// The bins as they stand: scaled to the loudest once complete, to the
    /// fixed reference while growing.
    pub fn bins(&self) -> [u8; BINS] {
        let loudest = self.peaks.iter().copied().fold(0.0f32, f32::max);
        let reference = if self.complete && loudest > f32::EPSILON { loudest } else { GROWING_REFERENCE };
        self.peaks.map(|value| ((value / reference).clamp(0.0, 1.0) * 255.0).round() as u8)
    }

    pub fn snapshot(&self, revision: u64) -> Snapshot {
        Snapshot { revision, bins: self.bins(), analyzed: self.analyzed, complete: self.complete }
    }
}

/// The bins reduced to the `count` columns a view draws: the peak of the
/// bins each column covers, lifted by a gamma and scaled so the loudest
/// column fills the height. Nothing is smoothed: a column per pixel wants
/// the grain (the owner, 2026-10-09).
pub fn columns(bins: &[u8; BINS], count: usize) -> Vec<f32> {
    if count < 2 {
        return Vec::new();
    }
    let mut values: Vec<f32> = (0..count)
        .map(|column| {
            let begin = column * BINS / count;
            let end = ((column + 1) * BINS / count).max(begin + 1).min(BINS);
            let peak = bins[begin..end].iter().copied().max().unwrap_or(0);
            (f32::from(peak) / 255.0).powf(GAMMA)
        })
        .collect();
    let loudest = values.iter().copied().fold(0.0f32, f32::max);
    if loudest > 0.0005 {
        for value in &mut values {
            *value = (*value / loudest).clamp(0.0, 1.0);
        }
    }
    values
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_by_position_leave_holes_where_nothing_was_fed() {
        let mut accumulator = Accumulator::new(1, BINS as u64);
        let quarter = BINS / 4;
        accumulator.feed_float(0, &vec![0.5; quarter]);
        accumulator.feed_float((3 * quarter) as u64, &vec![0.5; quarter]);
        assert!(accumulator.counts[..quarter].iter().all(|c| *c > 0));
        assert!(accumulator.counts[quarter..3 * quarter].iter().all(|c| *c == 0));
        assert!(accumulator.counts[3 * quarter..].iter().all(|c| *c > 0));
        assert_eq!(accumulator.analyzed(), (BINS / 2) as u16);
        assert!(!accumulator.complete(), "a document with holes is not the whole track");
    }

    #[test]
    fn both_channels_count_and_opposite_phases_do_not_cancel() {
        let mut same = Accumulator::new(2, BINS as u64);
        let mut opposite = Accumulator::new(2, BINS as u64);
        let mut in_phase = Vec::new();
        let mut out_of_phase = Vec::new();
        for _ in 0..BINS {
            in_phase.extend_from_slice(&[0.5, 0.5]);
            out_of_phase.extend_from_slice(&[0.5, -0.5]);
        }
        same.feed_float(0, &in_phase);
        opposite.feed_float(0, &out_of_phase);
        assert!(same.peaks.iter().all(|v| (*v - 0.5).abs() < 1e-6));
        assert!(opposite.peaks.iter().all(|v| (*v - 0.5).abs() < 1e-6));
        assert!(same.complete(), "every bin saw a frame");
    }

    #[test]
    fn a_growing_document_keeps_its_earlier_bins_when_a_louder_passage_arrives() {
        let mut accumulator = Accumulator::new(1, BINS as u64);
        accumulator.feed_float(0, &[0.125; 128]);
        let before = accumulator.bins()[0];
        accumulator.feed_float(128, &[0.9; 128]);
        assert_eq!(accumulator.bins()[0], before);
    }

    #[test]
    fn a_finished_document_is_scaled_to_its_loudest_bin() {
        let mut accumulator = Accumulator::new(1, BINS as u64);
        accumulator.feed_float(0, &vec![0.1; BINS / 2]);
        accumulator.feed_float((BINS / 2) as u64, &vec![0.2; BINS / 2]);
        assert!(accumulator.complete());
        let bins = accumulator.bins();
        assert_eq!(bins[BINS - 1], 255);
        assert_eq!(bins[0], 128, "half as loud as the loudest: {}", bins[0]);
    }

    #[test]
    fn integers_are_read_against_their_full_scale() {
        let mut sixteen = Accumulator::new(1, BINS as u64);
        sixteen.feed_int(0, &vec![i32::from(i16::MAX); BINS], 16);
        let mut twenty_four = Accumulator::new(1, BINS as u64);
        twenty_four.feed_int(0, &vec![(1 << 23) - 1; BINS], 24);
        assert_eq!(sixteen.bins()[0], 255);
        assert_eq!(twenty_four.bins()[0], 255);
    }

    #[test]
    fn a_long_track_is_read_in_steps_and_still_fills_every_bin() {
        let total = 44_100 * 60 * 10;
        let mut accumulator = Accumulator::new(2, total);
        let block = vec![0.3f32; 2 * 4096];
        let mut at = 0;
        while at < total {
            accumulator.feed_float(at, &block);
            at += 4096;
        }
        assert!(accumulator.complete());
        assert!(accumulator.counts.iter().all(|c| *c > 0 && *c <= FRAMES_PER_BIN_LIMIT as u32 + 1));
    }

    #[test]
    fn columns_take_the_peak_of_their_bins_and_the_loudest_fills_the_height() {
        let mut bins = [0u8; BINS];
        bins[10] = 255;
        bins[300] = 60;
        let values = columns(&bins, 64);
        assert_eq!(values.len(), 64);
        let loudest = values.iter().copied().fold(0.0f32, f32::max);
        assert!((loudest - 1.0).abs() < 1e-6);
        assert!(values[0] > values[20], "the loud bin lifts its column and no other");
        assert_eq!(values[20], 0.0);
        assert!(columns(&bins, 1).is_empty());
    }
}
