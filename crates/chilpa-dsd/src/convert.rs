//! Streaming DSD-to-PCM conversion.
//!
//! The first FIR stage decimates each DSD byte into one PCM sample. A chain
//! of half-band filters then brings every supported DSD rate to 88.2 kHz,
//! before any general-purpose audio backend can resample its ultrasonic
//! noise into the audible band.

use chilpa_audio::dsd::Decimator;
use crate::{DsdError, DsdSource};

#[cfg(test)]
mod seek_tests;

/// PCM rate used for converted DSD on shared and converting outputs.
pub const OUTPUT_RATE: u32 = 88_200;
/// Headroom used because low-passed DSD may exceed PCM full scale.
pub const DEFAULT_GAIN_DB: f32 = -6.0;

/// Converts one open DSD source into interleaved stereo PCM blocks.
pub struct DsdPcmConverter {
    source: Box<dyn DsdSource>,
    source_channels: usize,
    decimator: Decimator,
    gain: f32,
    total_frames: u64,
    emitted_frames: u64,
    finished: bool,
    discard_bytes: usize,
    discard_frames: u64,
}

impl DsdPcmConverter {
    pub fn new(source: Box<dyn DsdSource>, gain_db: f32) -> Result<Self, DsdError> {
        let info = source.info();
        let ratio = info.dsd_rate_hz / OUTPUT_RATE;
        if info.dsd_rate_hz % OUTPUT_RATE != 0 || ratio < 16 || !(ratio / 8).is_power_of_two() {
            return Err(DsdError::Unsupported(format!(
                "{} Hz cannot be converted to {} Hz PCM",
                info.dsd_rate_hz, OUTPUT_RATE
            )));
        }
        if info.channels == 0 || info.channels > 6 {
            return Err(DsdError::Unsupported(format!(
                "{} channels cannot be converted to stereo PCM",
                info.channels
            )));
        }
        let source_channels = info.channels as usize;
        let decimator = Decimator::new(info.dsd_rate_hz, source_channels).expect("validated DSD rate");
        Ok(Self {
            total_frames: info.samples / u64::from(ratio),
            source,
            source_channels,
            decimator,
            gain: 10f32.powf(gain_db / 20.0),
            emitted_frames: 0,
            finished: false,
            discard_bytes: 0,
            discard_frames: 0,
        })
    }

    pub fn output_rate(&self) -> u32 {
        OUTPUT_RATE
    }

    pub fn channels(&self) -> u16 {
        2
    }

    pub fn total_frames(&self) -> u64 {
        self.total_frames
    }

    /// Moves in PCM frames, warming the existing FIR chain before the target.
    /// I retain 128 output frames of history: more than the 16-byte first
    /// stage and all 63-tap half-band stages need, at every supported rate.
    /// A source that cannot seek keeps its current filter state and position.
    pub fn seek_to_frame(&mut self, frame: u64) -> Result<Option<u64>, DsdError> {
        let frame = frame.min(self.total_frames);
        let rate = self.source.info().dsd_rate_hz;
        let ratio = u64::from(rate / OUTPUT_RATE);
        let start = frame.saturating_sub(128) * ratio;
        let Some(landed) = self.source.seek(start)? else { return Ok(None); };
        // Container alignment can land before the requested history. Keep
        // every decimation stage on the original absolute PCM frame grid.
        let aligned = landed.div_ceil(ratio) * ratio;
        self.discard_bytes = ((aligned - landed) / 8) as usize;
        self.emitted_frames = (aligned / ratio).min(self.total_frames);
        let target = frame.max(self.emitted_frames);
        self.discard_frames = target - self.emitted_frames;
        self.decimator = Decimator::new(rate, self.source_channels).expect("validated DSD rate");
        self.finished = false;
        Ok(Some(target))
    }

    pub fn next_block(&mut self) -> Result<Option<Vec<f32>>, DsdError> {
        loop {
            let Some(mut block) = self.convert_block()? else { return Ok(None); };
            let frames = block.len() / 2;
            let discard = self.discard_frames.min(frames as u64) as usize;
            self.discard_frames -= discard as u64;
            if discard == frames { continue; }
            if discard != 0 { block.drain(..discard * 2); }
            return Ok(Some(block));
        }
    }

    fn convert_block(&mut self) -> Result<Option<Vec<f32>>, DsdError> {
        if self.finished {
            return Ok(None);
        }
        let Some(mut planar) = self.source.next_block()? else {
            self.finished = true;
            let missing = self.total_frames.saturating_sub(self.emitted_frames);
            self.emitted_frames = self.total_frames;
            return if missing == 0 {
                Ok(None)
            } else {
                Ok(Some(vec![0.0; missing as usize * 2]))
            };
        };
        if planar.len() != self.source_channels {
            return Err(DsdError::Corrupt(
                "a DSD block changed its channel count".into(),
            ));
        }

        if self.discard_bytes != 0 {
            let discard = self.discard_bytes.min(planar.iter().map(Vec::len).min().unwrap_or(0));
            for channel in &mut planar { channel.drain(..discard); }
            self.discard_bytes -= discard;
        }

        let channels = self.decimator.process(&planar);

        let available = channels.iter().map(Vec::len).min().unwrap_or(0) as u64;
        let frames = available.min(self.total_frames.saturating_sub(self.emitted_frames)) as usize;
        if frames == 0 {
            return self.convert_block();
        }

        const MINUS_3_DB: f32 = std::f32::consts::FRAC_1_SQRT_2;
        let (center, surround_left, surround_right) = match self.source_channels {
            3 => (Some(2), None, None),
            4 => (None, Some(2), Some(3)),
            5 => (Some(2), Some(3), Some(4)),
            6 => (Some(2), Some(4), Some(5)),
            _ => (None, None, None),
        };
        let normalization = 1.0
            / (1.0
                + if center.is_some() { MINUS_3_DB } else { 0.0 }
                + if surround_left.is_some() {
                    MINUS_3_DB
                } else {
                    0.0
                });
        let mut output = Vec::with_capacity(frames * 2);
        for frame in 0..frames {
            let (left, right) = match self.source_channels {
                1 => (channels[0][frame], channels[0][frame]),
                2 => (channels[0][frame], channels[1][frame]),
                _ => {
                    let mut left = channels[0][frame];
                    let mut right = channels[1][frame];
                    if let Some(index) = center {
                        left += MINUS_3_DB * channels[index][frame];
                        right += MINUS_3_DB * channels[index][frame];
                    }
                    if let Some(index) = surround_left {
                        left += MINUS_3_DB * channels[index][frame];
                    }
                    if let Some(index) = surround_right {
                        right += MINUS_3_DB * channels[index][frame];
                    }
                    (left * normalization, right * normalization)
                }
            };
            output.push(left * self.gain);
            output.push(right * self.gain);
        }
        self.emitted_frames += frames as u64;
        if self.emitted_frames >= self.total_frames {
            self.finished = true;
        }
        Ok(Some(output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DsdInfo;

    struct IdleDsd {
        info: DsdInfo,
        blocks: Vec<Vec<Vec<u8>>>,
    }

    impl DsdSource for IdleDsd {
        fn info(&self) -> &DsdInfo {
            &self.info
        }

        fn next_block(&mut self) -> Result<Option<Vec<Vec<u8>>>, DsdError> {
            Ok(self.blocks.pop())
        }
    }

    #[test]
    fn dsd64_becomes_stereo_pcm_at_88_2_khz() {
        let bytes = 4096usize;
        let source = IdleDsd {
            info: DsdInfo {
                dsd_rate_hz: 2_822_400,
                channels: 2,
                samples: bytes as u64 * 8,
            },
            blocks: vec![vec![vec![0x69; bytes], vec![0x69; bytes]]],
        };
        let mut converter = DsdPcmConverter::new(Box::new(source), DEFAULT_GAIN_DB).unwrap();
        assert_eq!(converter.output_rate(), 88_200);
        assert_eq!(converter.channels(), 2);
        assert_eq!(converter.total_frames(), 1024);
        let output = converter.next_block().unwrap().unwrap();
        assert_eq!(output.len(), 2048);
        assert!(output[256..].iter().all(|sample| sample.abs() < 1e-3));
        assert!(converter.next_block().unwrap().is_none());
    }

    #[test]
    fn shared_kernel_matches_playback_gain_duration_and_odd_source_chunks() {
        for multiplier in [1, 2, 4, 8] {
            let rate = 2_822_400 * multiplier;
            let bytes = 1024 * (rate / OUTPUT_RATE / 8) as usize;
            let planar: Vec<Vec<u8>> = (0..2).map(|channel| {
                (0..bytes).map(|i| ((i * 73 + channel * 131) ^ (i >> 3)) as u8).collect()
            }).collect();
            let blocks: Vec<Vec<Vec<u8>>> = (0..bytes).step_by(251).map(|start| {
                planar.iter().map(|channel| channel[start..(start + 251).min(bytes)].to_vec()).collect()
            }).collect();
            let mut kernel = Decimator::new(rate, 2).unwrap();
            let mut expected = Vec::new();
            let gain = 10f32.powf(DEFAULT_GAIN_DB / 20.0);
            for block in &blocks {
                let channels = kernel.process(block);
                for frame in 0..channels[0].len() {
                    expected.extend([channels[0][frame] * gain, channels[1][frame] * gain]);
                }
            }
            let source = IdleDsd {
                info: DsdInfo { dsd_rate_hz: rate, channels: 2, samples: bytes as u64 * 8 },
                blocks: blocks.into_iter().rev().collect(),
            };
            let mut converter = DsdPcmConverter::new(Box::new(source), DEFAULT_GAIN_DB).unwrap();
            let mut actual = Vec::new();
            while let Some(block) = converter.next_block().unwrap() { actual.extend(block); }
            assert_eq!(actual, expected);
            assert_eq!(actual.len(), converter.total_frames() as usize * 2);
            let mut whole = Decimator::new(rate, 2).unwrap();
            let whole = whole.process(&planar);
            let whole: Vec<f32> = (0..whole[0].len()).flat_map(|i| [whole[0][i] * gain, whole[1][i] * gain]).collect();
            assert_eq!(actual, whole, "chunk boundaries preserve every FIR accumulation and phase");
        }
    }

    #[test]
    fn multichannel_downmix_and_gain_still_use_the_original_channel_layout() {
        for count in 1..=6 {
            let rate = 2_822_400;
            let planar: Vec<Vec<u8>> = (0..count).map(|channel| {
                (0..8192).map(|i| ((i * 53 + channel * 29) ^ (i >> 2)) as u8).collect()
            }).collect();
            let source = IdleDsd {
                info: DsdInfo { dsd_rate_hz: rate, channels: count as u16, samples: 8192 * 8 },
                blocks: vec![planar.clone()],
            };
            let mut converter = DsdPcmConverter::new(Box::new(source), -3.0).unwrap();
            let actual = converter.next_block().unwrap().unwrap();
            let channels = Decimator::new(rate, count).unwrap().process(&planar);
            let gain = 10f32.powf(-3.0 / 20.0);
            let scale = std::f32::consts::FRAC_1_SQRT_2;
            for (index, frame) in actual.chunks_exact(2).enumerate() {
                let (mut left, mut right) = (channels[0][index], channels[count.min(2) - 1][index]);
                let mut weights = 1.0;
                if matches!(count, 3 | 5 | 6) {
                    left += channels[2][index] * scale;
                    right += channels[2][index] * scale;
                    weights += scale;
                }
                if count >= 4 {
                    let surround = if count == 4 { 2 } else if count == 5 { 3 } else { 4 };
                    left += channels[surround][index] * scale;
                    right += channels[surround + 1][index] * scale;
                    weights += scale;
                }
                assert!((frame[0] - left / weights * gain).abs() < 1e-7);
                assert!((frame[1] - right / weights * gain).abs() < 1e-7);
            }
        }
    }
}
