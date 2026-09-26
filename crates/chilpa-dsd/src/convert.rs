//! Streaming DSD-to-PCM conversion.
//!
//! The first FIR stage decimates each DSD byte into one PCM sample. A chain
//! of half-band filters then brings every supported DSD rate to 88.2 kHz,
//! before any general-purpose audio backend can resample its ultrasonic
//! noise into the audible band.

use crate::dsd2pcm::Dsd2Pcm;
use crate::{DsdError, DsdSource};
use std::sync::OnceLock;

/// PCM rate used for converted DSD on shared and converting outputs.
pub const OUTPUT_RATE: u32 = 88_200;
/// Headroom used because low-passed DSD may exceed PCM full scale.
pub const DEFAULT_GAIN_DB: f32 = -6.0;

const HALF_BAND_TAPS: usize = 63;

fn half_band_taps() -> &'static [f32; HALF_BAND_TAPS] {
    static TAPS: OnceLock<[f32; HALF_BAND_TAPS]> = OnceLock::new();
    TAPS.get_or_init(|| {
        let center = (HALF_BAND_TAPS - 1) as f64 / 2.0;
        let mut taps = [0.0; HALF_BAND_TAPS];
        let mut sum = 0.0;
        for (index, tap) in taps.iter_mut().enumerate() {
            let x = index as f64 - center;
            let sinc = if x == 0.0 {
                0.5
            } else {
                (std::f64::consts::PI * 0.5 * x).sin() / (std::f64::consts::PI * x)
            };
            let window = 0.42
                - 0.5
                    * (2.0 * std::f64::consts::PI * index as f64 / (HALF_BAND_TAPS - 1) as f64)
                        .cos()
                + 0.08
                    * (4.0 * std::f64::consts::PI * index as f64 / (HALF_BAND_TAPS - 1) as f64)
                        .cos();
            let value = sinc * window;
            *tap = value as f32;
            sum += value;
        }
        let scale = (1.0 / sum) as f32;
        for tap in &mut taps {
            *tap *= scale;
        }
        taps
    })
}

struct HalfBand {
    pending: Vec<f32>,
    next: usize,
}

impl HalfBand {
    fn new() -> Self {
        Self {
            pending: vec![0.0; HALF_BAND_TAPS - 1],
            next: HALF_BAND_TAPS - 1,
        }
    }

    fn process(&mut self, input: &[f32], output: &mut Vec<f32>) {
        let taps = half_band_taps();
        self.pending.extend_from_slice(input);
        let mut center = self.next;
        while center < self.pending.len() {
            let window = &self.pending[center + 1 - HALF_BAND_TAPS..=center];
            let mut sum = 0.0;
            for tap in (0..HALF_BAND_TAPS).step_by(2) {
                sum += taps[tap] * window[HALF_BAND_TAPS - 1 - tap];
            }
            sum += taps[HALF_BAND_TAPS / 2] * window[HALF_BAND_TAPS / 2];
            output.push(sum);
            center += 2;
        }
        let keep_from = self.pending.len().saturating_sub(HALF_BAND_TAPS - 1);
        let overshoot = center - self.pending.len();
        self.pending.drain(..keep_from);
        self.next = HALF_BAND_TAPS - 1 + overshoot;
    }
}

/// Converts one open DSD source into interleaved stereo PCM blocks.
pub struct DsdPcmConverter {
    source: Box<dyn DsdSource>,
    source_channels: usize,
    first_stage: Vec<Dsd2Pcm>,
    half_bands: Vec<Vec<HalfBand>>,
    gain: f32,
    total_frames: u64,
    emitted_frames: u64,
    finished: bool,
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
        let stage_count = (ratio / 8).trailing_zeros() as usize;
        Ok(Self {
            total_frames: info.samples / u64::from(ratio),
            source,
            source_channels,
            first_stage: (0..source_channels).map(|_| Dsd2Pcm::new()).collect(),
            half_bands: (0..stage_count)
                .map(|_| (0..source_channels).map(|_| HalfBand::new()).collect())
                .collect(),
            gain: 10f32.powf(gain_db / 20.0),
            emitted_frames: 0,
            finished: false,
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

    pub fn next_block(&mut self) -> Result<Option<Vec<f32>>, DsdError> {
        if self.finished {
            return Ok(None);
        }
        let Some(planar) = self.source.next_block()? else {
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

        let mut channels = Vec::with_capacity(self.source_channels);
        for channel in 0..self.source_channels {
            let mut samples = Vec::new();
            self.first_stage[channel].translate(&planar[channel], &mut samples);
            for stage in &mut self.half_bands {
                let mut downsampled = Vec::with_capacity(samples.len() / 2 + 1);
                stage[channel].process(&samples, &mut downsampled);
                samples = downsampled;
            }
            channels.push(samples);
        }

        let available = channels.iter().map(Vec::len).min().unwrap_or(0) as u64;
        let frames = available.min(self.total_frames.saturating_sub(self.emitted_frames)) as usize;
        if frames == 0 {
            return self.next_block();
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
}
