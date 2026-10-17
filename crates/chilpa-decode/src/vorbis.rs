//! The signed PCM reference produced by the Vorbis decoder.
//!
//! Vorbis has no encoded integer bit depth. Its decoder output is defined
//! here as signed 32-bit PCM, using Symphonia's full-scale conversion once.
//! Bit-perfect delivery compares against these decoded words, not against
//! the encoder's original master. The codec remains lossy.

use crate::{Block, DecodeError};
use symphonia::core::{
    audio::{AudioBuffer, Signal},
    codecs::Decoder,
    conv::FromSample,
    errors::Error,
    formats::FormatReader,
};

// Vorbis I permits windows up to 8192 frames. Starting one maximum window
// earlier leaves a complete predecessor packet to restore overlap state.
// https://xiph.org/vorbis/doc/Vorbis_I_spec.html
pub(super) const SEEK_WINDOW: u64 = 8192;

pub(super) fn prime_after_seek(
    format: &mut dyn FormatReader,
    decoder: &mut dyn Decoder,
    track_id: u32,
) -> Result<u64, DecodeError> {
    loop {
        let packet = format
            .next_packet()
            .map_err(|e| DecodeError::Corrupt(e.to_string()))?;
        if packet.track_id() != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            // After reset, the first packet primes the overlap window and
            // produces no samples. Its duration is absent from the next
            // output; the discard count must start at the following packet.
            Ok(buffer) => {
                debug_assert_eq!(buffer.frames(), 0);
                return Ok(packet.ts.saturating_add(packet.dur));
            }
            Err(Error::DecodeError(_)) => continue,
            Err(e) => return Err(DecodeError::Corrupt(e.to_string())),
        }
    }
}

pub(super) fn pcm_reference(buffer: &AudioBuffer<f32>) -> Block {
    let channels = buffer.spec().channels.count();
    let frames = buffer.frames();
    let mut data = Vec::with_capacity(frames * channels);
    for frame in 0..frames {
        for channel in 0..channels {
            data.push(i32::from_sample(buffer.chan(channel)[frame]));
        }
    }
    Block::Int { data, bits: 32 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use symphonia::core::audio::{Channels, SignalSpec};

    #[test]
    fn the_reference_uses_full_scale_without_the_maximum_integer_gain_error() {
        let mut buffer = AudioBuffer::<f32>::new(9, SignalSpec::new(44_100, Channels::FRONT_LEFT));
        buffer.render_reserved(Some(9));
        buffer.chan_mut(0).copy_from_slice(&[
            -1.0,
            -0.5,
            -1.0 / 2_147_483_648.0,
            -0.0,
            0.0,
            1.0 / 2_147_483_648.0,
            0.5,
            f32::from_bits(1.0f32.to_bits() - 1),
            1.0,
        ]);
        let Block::Int { data, bits } = pcm_reference(&buffer) else {
            panic!("integer reference")
        };
        assert_eq!(bits, 32);
        assert_eq!(
            data,
            [
                i32::MIN,
                -1_073_741_824,
                -1,
                0,
                0,
                1,
                1_073_741_824,
                2_147_483_520,
                i32::MAX
            ]
        );
    }

    #[test]
    fn other_float_codecs_retain_their_original_samples_and_format() {
        let mut buffer = AudioBuffer::<f32>::new(3, SignalSpec::new(48_000, Channels::FRONT_LEFT));
        buffer.render_reserved(Some(3));
        buffer.chan_mut(0).copy_from_slice(&[-0.5, 0.0, 0.5]);
        for codec in ["aac", "mp3", "pcm", "opus", ""] {
            let audio =
                symphonia::core::audio::AudioBufferRef::F32(std::borrow::Cow::Borrowed(&buffer));
            let Block::Float(data) = crate::interleave(&audio, None, codec) else {
                panic!("unrelated codec changed to integer")
            };
            assert_eq!(data, [-0.5, 0.0, 0.5]);
        }
    }
}
