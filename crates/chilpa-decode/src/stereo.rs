//! A fixed horizontal-speaker matrix, allocated only for explicit stereo output.
//! An active matrix owns 256 bytes of weights plus one channel count. A
//! decoder otherwise retains only an optional box pointer. Converted blocks
//! own two samples per input frame. Drop releases both. No worker or
//! additional read-ahead is created. LFE is omitted from the stereo mix.
//! This generic PCM policy uses fixed speaker weights and headroom; encoded
//! downmix metadata and object-based renderers are outside its scope.
use super::{Block, DecodeError};
use symphonia::core::audio::Channels;

#[derive(Clone, Copy)]
pub(crate) struct Stereo {
    weights: [(f32, f32); 32],
    channels: usize,
}
impl Stereo {
    pub(crate) fn new(layout: Channels) -> Result<Self, DecodeError> {
        let mut weights = [(0.0, 0.0); 32];
        let channels = layout.count();
        if !(3..=32).contains(&channels) {
            return Err(DecodeError::Unsupported(
                "stereo: invalid channel count".into(),
            ));
        }
        let surround = std::f32::consts::FRAC_1_SQRT_2;
        for (index, speaker) in layout.iter().enumerate() {
            weights[index] = match speaker {
                Channels::FRONT_LEFT => (1.0, 0.0),
                Channels::FRONT_RIGHT => (0.0, 1.0),
                Channels::FRONT_CENTRE => (surround, surround),
                Channels::LFE1 | Channels::LFE2 => (0.0, 0.0),
                Channels::REAR_LEFT | Channels::SIDE_LEFT => (surround, 0.0),
                Channels::REAR_RIGHT | Channels::SIDE_RIGHT => (0.0, surround),
                Channels::REAR_CENTRE => (0.5, 0.5),
                _ => {
                    return Err(DecodeError::Unsupported(
                        "stereo: unsupported speaker position".into(),
                    ));
                }
            };
        }
        // One shared normalization preserves balance and reserves headroom
        // for correlated full-scale channels without a limiter or clipping.
        let left: f32 = weights.iter().map(|weight| weight.0).sum();
        let right: f32 = weights.iter().map(|weight| weight.1).sum();
        let gain = 1.0 / left.max(right).max(1.0);
        for weight in &mut weights {
            weight.0 *= gain;
            weight.1 *= gain;
        }
        Ok(Self { weights, channels })
    }
    pub(crate) fn convert(&self, block: Block) -> Block {
        let length = match &block {
            Block::Float(data) => data.len(),
            Block::Int { data, .. } => data.len(),
        };
        let frames = length / self.channels;
        let scale = match &block {
            Block::Int { bits, .. } => 2.0f64.powi(1 - i32::from((*bits).clamp(1, 32))),
            _ => 1.0,
        };
        let mut output = Vec::with_capacity(frames * 2);
        for frame in 0..frames {
            let mut left = 0.0f64;
            let mut right = 0.0f64;
            for channel in 0..self.channels {
                let index = frame * self.channels + channel;
                let sample = match &block {
                    Block::Float(data) => f64::from(data[index]),
                    Block::Int { data, .. } => f64::from(data[index]) * scale,
                };
                left += sample * f64::from(self.weights[channel].0);
                right += sample * f64::from(self.weights[channel].1);
            }
            output.extend([left as f32, right as f32]);
        }
        Block::Float(output)
    }
}
