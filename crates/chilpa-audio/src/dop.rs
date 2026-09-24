//! DoP: DSD over PCM, following the open DoP v1.1 standard.
//!
//! Each 24-bit word carries 16 bits of DSD under a marker that alternates
//! 0x05 and 0xFA between frames. The DAC spots that alternation and switches
//! to DSD mode; break it and it reads the stream as PCM and lets out loud
//! noise.
//!
//! That is where the rule the audit found broken in QBZ comes from: a
//! partial write left the remaining frames out and only logged a warning,
//! so the marker sequence drifted out of phase. Here partial writes resume
//! at the exact frame they stopped at, and what is sent during pauses or at
//! the end is DSD silence with valid markers, never PCM zeros.

/// The PCM carrier rate for a DSD bit rate: 16 DSD bits per frame per
/// channel.
pub const fn carrier_rate(dsd_rate_hz: u32) -> u32 {
    dsd_rate_hz / 16
}

/// The DSD silence payload. PCM zeros would knock the DAC out of DSD mode.
const SILENCE: i32 = 0x6969;

/// Packs DoP frames, keeping the marker phase across calls.
#[derive(Debug, Default)]
pub struct DopPacker {
    marker_fa: bool,
}

impl DopPacker {
    pub fn new() -> Self {
        Self::default()
    }

    fn marker(&mut self) -> i32 {
        let m = if self.marker_fa { 0xFA } else { 0x05 };
        self.marker_fa = !self.marker_fa;
        m
    }

    /// Packs planar DSD (2 bytes per channel per frame) into interleaved S32
    /// samples, left-justified and ready for the wire.
    pub fn pack(&mut self, planar: &[&[u8]], out: &mut Vec<i32>) {
        let channels = planar.len();
        let frames = planar.iter().map(|c| c.len()).min().unwrap_or(0) / 2;
        out.reserve(frames * channels);
        for f in 0..frames {
            let marker = self.marker();
            for channel in planar {
                let hi = i32::from(channel[f * 2]);
                let lo = i32::from(channel[f * 2 + 1]);
                out.push(((marker << 16) | (hi << 8) | lo) << 8);
            }
        }
    }

    /// DSD silence with valid markers, for pauses, seeks and final padding.
    pub fn silence(&mut self, frames: usize, channels: u16, out: &mut Vec<i32>) {
        out.reserve(frames * channels as usize);
        for _ in 0..frames {
            let marker = self.marker();
            for _ in 0..channels {
                out.push(((marker << 16) | SILENCE) << 8);
            }
        }
    }

    /// The current phase, to check continuity. `false` = 0x05 is next.
    pub fn next_is_fa(&self) -> bool {
        self.marker_fa
    }
}

/// What a device did with a write.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WriteOutcome {
    /// It wrote that many frames; there may be fewer than it was given.
    Wrote(usize),
    /// The device ran short of data and had to be recovered. Whatever did not
    /// make it out was lost.
    Xrun,
}

/// Delivers DoP words to a device without breaking the alternation.
///
/// The writer does not assume a write consumes everything: it retries the
/// rest from the exact frame it stopped at. After an xrun, the gap is
/// filled with DSD silence so the DAC never sees PCM.
pub struct DopWriter {
    channels: u16,
    /// Frames of silence sent after recovering from an xrun.
    pub recovery_frames: usize,
}

impl DopWriter {
    pub fn new(channels: u16) -> Self {
        Self { channels, recovery_frames: 1 }
    }

    /// Writes every frame of `words`, retrying the partial ones.
    ///
    /// `sink` receives the samples still owed and reports what it did with
    /// them. Returns how many frames were recovered with silence.
    pub fn write_all<S>(&self, packer: &mut DopPacker, words: &[i32], mut sink: S) -> usize
    where
        S: FnMut(&[i32]) -> WriteOutcome,
    {
        let channels = self.channels as usize;
        let mut done = 0;
        let mut recovered = 0;
        while done * channels < words.len() {
            match sink(&words[done * channels..]) {
                WriteOutcome::Wrote(0) => break,
                WriteOutcome::Wrote(frames) => done += frames,
                WriteOutcome::Xrun => {
                    // The silence advances the phase just like the frames that were
                    // lost, so the DAC keeps seeing the alternation.
                    let mut relleno = Vec::new();
                    packer.silence(self.recovery_frames, self.channels, &mut relleno);
                    sink(&relleno);
                    recovered += self.recovery_frames;
                }
            }
        }
        recovered
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dsd(bytes: &[u8]) -> Vec<u8> {
        bytes.to_vec()
    }

    #[test]
    fn the_dsd64_carrier_is_176_400() {
        assert_eq!(carrier_rate(2_822_400), 176_400);
        assert_eq!(carrier_rate(5_644_800), 352_800);
    }

    #[test]
    fn markers_alternate_and_both_channels_carry_the_same_one() {
        let left = dsd(&[0xAB, 0xCD, 0x12, 0x34]);
        let der = dsd(&[0x55, 0xAA, 0x9C, 0x63]);
        let mut out = Vec::new();
        DopPacker::new().pack(&[&left, &der], &mut out);
        assert_eq!(out[0], ((0x05 << 16) | 0xABCD) << 8);
        assert_eq!(out[1], ((0x05 << 16) | 0x55AA) << 8);
        assert_eq!(out[2], ((0xFA << 16) | 0x1234) << 8);
        assert_eq!(out[3], ((0xFA << 16) | 0x9C63) << 8);
    }

    #[test]
    fn the_marker_phase_survives_across_calls() {
        let mut packer = DopPacker::new();
        let mut out = Vec::new();
        packer.pack(&[&dsd(&[0, 0]), &dsd(&[0, 0])], &mut out);
        out.clear();
        packer.pack(&[&dsd(&[0, 0]), &dsd(&[0, 0])], &mut out);
        assert_eq!((out[0] >> 24) & 0xFF, 0xFA, "the second block carries on where the first left off");
    }

    #[test]
    fn silence_carries_a_dsd_payload_and_not_zeros() {
        let mut out = Vec::new();
        DopPacker::new().silence(1, 2, &mut out);
        assert_eq!(out[0], ((0x05 << 16) | 0x6969) << 8);
        assert_ne!(out[0], 0, "PCM zeros would take the DAC out of DSD mode");
    }

    #[test]
    fn a_partial_write_resumes_and_loses_no_frames() {
        // The audit's finding: QBZ only warned about the partial write and
        // threw the rest away, and that is where the alternation broke.
        let mut packer = DopPacker::new();
        let mut words = Vec::new();
        packer.pack(&[&dsd(&[1, 2, 3, 4, 5, 6]), &dsd(&[7, 8, 9, 10, 11, 12])], &mut words);
        let written = std::cell::RefCell::new(Vec::new());
        let writer = DopWriter::new(2);
        let mut first = true;
        writer.write_all(&mut packer, &words, |chunk| {
            let frames = if first { 1 } else { chunk.len() / 2 };
            written.borrow_mut().extend_from_slice(&chunk[..frames * 2]);
            if first {
                first = false;
            }
            WriteOutcome::Wrote(frames)
        });
        assert_eq!(written.borrow().len(), 6, "the three stereo frames came out in two writes");
    }

    #[test]
    fn after_an_xrun_it_fills_with_silence_and_the_phase_holds() {
        let mut packer = DopPacker::new();
        let mut words = Vec::new();
        packer.pack(&[&dsd(&[1, 2]), &dsd(&[3, 4])], &mut words);
        let mut vistas: Vec<i32> = Vec::new();
        let mut fail = true;
        let recovered = DopWriter::new(2).write_all(&mut packer, &words, |chunk| {
            if fail {
                fail = false;
                return WriteOutcome::Xrun;
            }
            vistas.extend_from_slice(chunk);
            WriteOutcome::Wrote(chunk.len() / 2)
        });
        assert_eq!(recovered, 1);
        assert_eq!(vistas[0], ((0xFA << 16) | 0x6969) << 8, "the padding carries on the alternation");
    }
}
