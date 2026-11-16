//! The reference Opus decoder, with the player's trimming contract preserved.
//!
//! A decoder owns at most 1 MiB of native state and two 120 ms, eight-channel
//! float buffers (184,320 bytes each). No workers, timers or per-packet native
//! heap allocations are added by the adapter. Drop releases native state.
use std::ptr::NonNull;
use symphonia::core::{
    audio::{AsAudioBufferRef, AudioBuffer, AudioBufferRef, Signal, SignalSpec},
    codecs::{
        CODEC_TYPE_OPUS, CodecDescriptor, CodecParameters, Decoder, DecoderOptions, FinalizeResult,
    },
    errors::{Result, decode_error, unsupported_error},
    formats::Packet,
    support_codec,
};

// Eighty milliseconds is the format minimum. A bounded one-second warmup
// also settles decoder history before sample comparisons after a seek.
pub(crate) const SEEK_WINDOW: u64 = 48_000;
const RATE: u32 = 48_000;
const MAX_FRAMES: usize = 5_760;
const MAX_STATE: i32 = 1024 * 1024;
const MAX_PACKET: usize = 1024 * 1024;

// Opus mapping family 1 is in Vorbis order. Symphonia's planes follow the
// channel mask, so map each output plane back to its OpusHead channel.
const ORDER: [&[usize]; 8] = [
    &[0],
    &[0, 1],
    &[0, 2, 1],
    &[0, 1, 2, 3],
    &[0, 2, 1, 3, 4],
    &[0, 2, 1, 5, 3, 4],
    &[0, 2, 1, 6, 5, 3, 4],
    &[0, 2, 1, 7, 5, 6, 3, 4],
];

pub(crate) struct OpusDecoder {
    params: CodecParameters,
    state: NonNull<opusic_sys::OpusMSDecoder>,
    output: AudioBuffer<f32>,
    pcm: Vec<f32>,
    channels: usize,
    reset_failed: bool,
}

// SAFETY: the native state is owned by one decoder and only accessed through
// exclusive &mut self. Shared methods read Rust-owned parameters and samples.
unsafe impl Send for OpusDecoder {}
unsafe impl Sync for OpusDecoder {}

impl Drop for OpusDecoder {
    fn drop(&mut self) {
        // SAFETY: create returned this non-null owned state; it is freed once.
        unsafe { opusic_sys::opus_multistream_decoder_destroy(self.state.as_ptr()) };
    }
}

impl Decoder for OpusDecoder {
    fn try_new(params: &CodecParameters, _options: &DecoderOptions) -> Result<Self> {
        if params.codec != CODEC_TYPE_OPUS {
            return unsupported_error("opus: codec mismatch");
        }
        let Some(header) = params.extra_data.as_deref() else {
            return unsupported_error("opus: identification header is required");
        };
        if !(19..=256).contains(&header.len()) || &header[..8] != b"OpusHead" || header[8] > 15 {
            return decode_error("opus: invalid identification header");
        }
        let channels = usize::from(header[9]);
        if !(1..=8).contains(&channels) {
            return unsupported_error("opus: unsupported channel count");
        }
        let Some(mask) = params
            .channels
            .or_else(|| params.channel_layout.map(|layout| layout.into_channels()))
        else {
            return unsupported_error("opus: channel layout is required");
        };
        if mask.count() != channels {
            return decode_error("opus: inconsistent channel count");
        }
        let mut mapping = [0u8; 8];
        let (streams, coupled) = match header[18] {
            0 if channels <= 2 => {
                mapping[1] = 1;
                (1, channels - 1)
            }
            1 if header.len() >= 21 + channels => {
                let streams = usize::from(header[19]);
                let coupled = usize::from(header[20]);
                if streams == 0 || coupled > streams || streams + coupled > 8 {
                    return decode_error("opus: invalid stream mapping");
                }
                for (output, &input) in ORDER[channels - 1].iter().enumerate() {
                    let value = header[21 + input];
                    if value != 255 && usize::from(value) >= streams + coupled {
                        return decode_error("opus: invalid channel mapping");
                    }
                    mapping[output] = value;
                }
                (streams, coupled)
            }
            1 => return decode_error("opus: truncated channel mapping"),
            _ => return unsupported_error("opus: unsupported mapping family"),
        };
        // SAFETY: counts are positive and bounded above; this only computes size.
        let state_size = unsafe {
            opusic_sys::opus_multistream_decoder_get_size(streams as i32, coupled as i32)
        };
        if state_size <= 0 || state_size > MAX_STATE {
            return unsupported_error("opus: decoder state exceeds budget");
        }
        let mut error = opusic_sys::OPUS_OK;
        // SAFETY: mapping has channels initialized bytes, counts are validated,
        // and error points to writable storage. libopus copies the mapping.
        let state = unsafe {
            opusic_sys::opus_multistream_decoder_create(
                RATE as i32,
                channels as i32,
                streams as i32,
                coupled as i32,
                mapping.as_ptr(),
                &mut error,
            )
        };
        let Some(state) = NonNull::new(state) else {
            return decode_error("opus: decoder allocation failed");
        };
        let mut decoder = Self {
            params: params.clone(),
            state,
            output: AudioBuffer::new(MAX_FRAMES as u64, SignalSpec::new(RATE, mask)),
            pcm: vec![0.0; MAX_FRAMES * channels],
            channels,
            reset_failed: false,
        };
        if error != opusic_sys::OPUS_OK {
            return decode_error("opus: decoder initialization failed");
        }
        let gain = i32::from(i16::from_le_bytes([header[16], header[17]]));
        // SAFETY: state is live, and this request takes one promoted C int.
        let status = unsafe {
            opusic_sys::opus_multistream_decoder_ctl(
                decoder.state.as_ptr(),
                opusic_sys::OPUS_SET_GAIN_REQUEST,
                gain,
            )
        };
        if status != opusic_sys::OPUS_OK {
            return decode_error("opus: output gain could not be set");
        }
        decoder.params.with_sample_rate(RATE);
        Ok(decoder)
    }

    fn supported_codecs() -> &'static [CodecDescriptor] {
        &[support_codec!(CODEC_TYPE_OPUS, "opus", "Opus")]
    }

    fn reset(&mut self) {
        // SAFETY: state is live and exclusively borrowed; reset takes no arguments.
        let status = unsafe {
            opusic_sys::opus_multistream_decoder_ctl(
                self.state.as_ptr(),
                opusic_sys::OPUS_RESET_STATE,
            )
        };
        self.reset_failed = status != opusic_sys::OPUS_OK;
        self.output.clear();
    }

    fn codec_params(&self) -> &CodecParameters {
        &self.params
    }

    fn decode(&mut self, packet: &Packet) -> Result<AudioBufferRef<'_>> {
        self.output.clear();
        if self.reset_failed || packet.data.is_empty() || packet.data.len() > MAX_PACKET {
            return decode_error("opus: invalid packet or decoder state");
        }
        // SAFETY: the immutable packet length fits i32, pcm holds MAX_FRAMES
        // samples per initialized channel, and state is exclusively borrowed.
        let frames = unsafe {
            opusic_sys::opus_multistream_decode_float(
                self.state.as_ptr(),
                packet.data.as_ptr(),
                packet.data.len() as i32,
                self.pcm.as_mut_ptr(),
                MAX_FRAMES as i32,
                0,
            )
        };
        if frames <= 0 || frames as usize > MAX_FRAMES {
            return decode_error("opus: packet could not be decoded");
        }
        let frames = frames as usize;
        let start = packet.trim_start() as usize;
        let end = packet.trim_end() as usize;
        if start > frames || end > frames - start {
            return decode_error("opus: invalid packet trimming");
        }
        self.output.render_reserved(Some(frames));
        for channel in 0..self.channels {
            for (frame, sample) in self.output.chan_mut(channel).iter_mut().enumerate() {
                *sample = self.pcm[frame * self.channels + channel];
            }
        }
        // File playback's outer Trimmer owns encoder delay/padding. The offline
        // verifier enables demuxer trimming instead; never apply pre-skip twice.
        self.output.trim(start, end);
        Ok(self.output.as_audio_buffer_ref())
    }

    fn finalize(&mut self) -> FinalizeResult {
        FinalizeResult::default()
    }
    fn last_decoded(&self) -> AudioBufferRef<'_> {
        self.output.as_audio_buffer_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use symphonia::core::audio::Channels;

    fn header() -> Vec<u8> {
        let mut value = vec![0; 19];
        value[..8].copy_from_slice(b"OpusHead");
        value[8] = 1;
        value[9] = 2;
        value
    }

    fn accepts(header: Vec<u8>, channels: Channels) -> bool {
        let mut params = CodecParameters::new();
        params
            .for_codec(CODEC_TYPE_OPUS)
            .with_sample_rate(48_000)
            .with_channels(channels)
            .with_extra_data(header.into_boxed_slice());
        OpusDecoder::try_new(&params, &DecoderOptions::default()).is_ok()
    }

    #[test]
    fn failed_decode_clears_the_last_audio_buffer() {
        let mut params = CodecParameters::new();
        params
            .for_codec(CODEC_TYPE_OPUS)
            .with_sample_rate(48_000)
            .with_channels(Channels::FRONT_LEFT | Channels::FRONT_RIGHT)
            .with_extra_data(header().into_boxed_slice());
        let mut decoder = OpusDecoder::try_new(&params, &DecoderOptions::default()).unwrap();
        let silence = Packet::new_from_slice(0, 0, 960, &[0xf8, 0xff, 0xfe]);
        assert!(decoder.decode(&silence).unwrap().frames() > 0);
        assert!(
            decoder
                .decode(&Packet::new_from_slice(0, 960, 960, &[]))
                .is_err()
        );
        assert_eq!(decoder.last_decoded().frames(), 0);
    }

    #[test]
    fn malformed_headers_never_cross_the_native_decoder_boundary() {
        let stereo = Channels::FRONT_LEFT | Channels::FRONT_RIGHT;
        assert!(accepts(header(), stereo));
        for length in 0..19 {
            assert!(!accepts(header()[..length].to_vec(), stereo));
        }
        for (at, byte) in [(0, b'X'), (8, 16), (9, 0), (9, 9), (18, 255), (18, 1)] {
            let mut value = header();
            value[at] = byte;
            assert!(!accepts(value, stereo), "header field {at}={byte}");
        }
        let mut huge = header();
        huge.resize(257, 0);
        assert!(!accepts(huge, stereo));
        assert!(!accepts(header(), Channels::FRONT_LEFT));
        for (streams, coupled, mapping) in [(0, 0, 0), (1, 2, 0), (9, 0, 0), (1, 0, 2)] {
            let mut value = header();
            value[18] = 1;
            value.extend_from_slice(&[streams, coupled, mapping, mapping]);
            assert!(!accepts(value, stereo));
        }
    }
}
