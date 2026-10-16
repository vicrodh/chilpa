//! Incremental DSD sidechain, run only by the existing visualizer worker.

use crate::dsd::{Decimator, OUTPUT_RATE};
use crate::format::SampleFormat;
use crate::tap::{Snapshot, Tap, FRAMES_KEPT};
use crate::tap_dsd::{Batch, Encoding};

const GAIN: f32 = 0.501_187_2; // The playback converter's default -6 dB headroom.

#[derive(Default)]
pub(super) struct Sidechain {
    generation: Option<u64>,
    encoding: Option<Encoding>,
    next: u64,
    decimator: Option<Decimator>,
    history: Vec<f32>,
}

pub(super) struct Update {
    pub active: bool,
    pub reset: bool,
    pub snapshot: Option<Snapshot>,
}

impl Sidechain {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn poll(&mut self, tap: &Tap) -> Update {
        self.process(tap.drain_dsd())
    }

    fn process(&mut self, batch: Batch) -> Update {
        let reset = self.generation != Some(batch.generation)
            || self.encoding != batch.encoding || self.next != batch.start;
        if reset {
            self.history.clear();
            self.decimator = batch.encoding.and_then(|encoding| {
                Decimator::new(encoding.rate()?, usize::from(encoding.format.channels).min(2))
            });
        }
        self.generation = Some(batch.generation);
        self.encoding = batch.encoding;
        self.next = batch.end;
        let mut update = Update { active: batch.encoding.is_some(), reset, snapshot: None };
        let (Some(encoding), Some(decimator)) = (batch.encoding, self.decimator.as_mut()) else {
            return update;
        };
        if batch.bytes.is_empty() { return update; }
        let channels = usize::from(encoding.format.channels).min(2);
        let planar = unpack(&batch.bytes, encoding, channels);
        let pcm = decimator.process(&planar);
        let frames = pcm.iter().map(Vec::len).min().unwrap_or(0);
        if frames == 0 { return update; }
        for frame in 0..frames {
            self.history.push(pcm[0][frame] * GAIN);
            self.history.push(pcm[channels - 1][frame] * GAIN);
        }
        let excess = self.history.len().saturating_sub(FRAMES_KEPT * 2);
        self.history.drain(..excess);
        update.snapshot = Some(Snapshot {
            samples: self.history.clone(),
            channels: 2,
            sample_rate_hz: OUTPUT_RATE,
            peaks: Vec::new(),
            rms: Vec::new(),
        });
        update
    }
}

/// I undo carrier word packing only. Source bytes are already MSB-first.
fn unpack(bytes: &[u8], encoding: Encoding, channels: usize) -> Vec<Vec<u8>> {
    let width = encoding.format.sample_format.bytes();
    let frame_bytes = encoding.format.frame_bytes();
    let mut planar = vec![Vec::new(); channels];
    for frame in bytes.chunks_exact(frame_bytes.max(1)) {
        for (channel, output) in planar.iter_mut().enumerate() {
            let word = &frame[channel * width..(channel + 1) * width];
            match (encoding.format.sample_format, encoding.dop) {
                (SampleFormat::DsdU32Be, false) => output.extend_from_slice(word),
                (SampleFormat::DsdU32Le, false) => output.extend(word.iter().rev().copied()),
                (SampleFormat::S24In32 | SampleFormat::S32, true) => output.extend([word[2], word[1]]),
                (SampleFormat::S24Packed, true) => output.extend([word[1], word[0]]),
                _ => unreachable!("only explicitly tagged DSD reaches the sidechain"),
            }
        }
    }
    planar
}

#[cfg(test)]
mod tests;
