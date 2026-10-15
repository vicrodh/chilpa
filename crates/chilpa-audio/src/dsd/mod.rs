//! Shared streaming DSD analysis and playback filters.
//!
//! These filters never participate in native DSD or DoP device writes.

mod first_stage;
mod half_band;

use first_stage::Dsd2Pcm;
use half_band::HalfBand;

pub const OUTPUT_RATE: u32 = 88_200;

/// Persistent FIR history and decimation phase for planar, MSB-first DSD.
pub struct Decimator {
    first_stage: Vec<Dsd2Pcm>,
    half_bands: Vec<Vec<HalfBand>>,
}

impl Decimator {
    pub fn new(dsd_rate_hz: u32, channels: usize) -> Option<Self> {
        let ratio = dsd_rate_hz / OUTPUT_RATE;
        if channels == 0 || dsd_rate_hz % OUTPUT_RATE != 0 || ratio < 16
            || !(ratio / 8).is_power_of_two()
        {
            return None;
        }
        let stage_count = (ratio / 8).trailing_zeros() as usize;
        Some(Self {
            first_stage: (0..channels).map(|_| Dsd2Pcm::new()).collect(),
            half_bands: (0..stage_count)
                .map(|_| (0..channels).map(|_| HalfBand::new()).collect())
                .collect(),
        })
    }

    /// I retain all filter histories across calls, including odd block sizes.
    pub fn process(&mut self, planar: &[Vec<u8>]) -> Vec<Vec<f32>> {
        assert_eq!(planar.len(), self.first_stage.len());
        let mut channels = Vec::with_capacity(planar.len());
        for (channel, bytes) in planar.iter().enumerate() {
            let mut samples = Vec::new();
            self.first_stage[channel].translate(bytes, &mut samples);
            for stage in &mut self.half_bands {
                let mut downsampled = Vec::with_capacity(samples.len() / 2 + 1);
                stage[channel].process(&samples, &mut downsampled);
                samples = downsampled;
            }
            channels.push(samples);
        }
        channels
    }
}
