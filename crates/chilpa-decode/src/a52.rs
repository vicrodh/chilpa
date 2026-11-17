//! Bounded A/52 channel decoding. JOC object rendering is not requested.
//!
//! One decoder retains one 64 KiB lookahead packet in its caller, one
//! 1536-frame/eight-channel scratch buffer and one planar output buffer.
//! Codec state follows the bounded A/52 channel/substream fields. No worker,
//! timer or output device is created; dropping the decoder releases it.
use eac3::chanmap::ChannelLocation as Location;
use oxideav_ac3::{audblk, bsi, decoder::verify_packet_crc, eac3, syncinfo};
use symphonia::core::{
    audio::{AsAudioBufferRef, AudioBuffer, AudioBufferRef, Channels, Signal, SignalSpec},
    codecs::CodecParameters,
    errors::{Result, decode_error, unsupported_error},
    formats::Packet,
};

pub(crate) const MAX_PACKET: usize = 65_536;
pub(crate) const SEEK_WINDOW: u64 = 1_536;
const MAX_FRAMES: usize = 1_536;
const MAX_CHANNELS: usize = 8;

pub(crate) struct A52Decoder {
    base: bool,
    ac3: Box<audblk::Ac3State>,
    eac3: Box<eac3::decoder::Eac3DecoderState>,
    scratch: Vec<f32>,
    output: AudioBuffer<f32>,
    spec: Option<SignalSpec>,
}
impl A52Decoder {
    pub(crate) fn new(params: &CodecParameters) -> Result<Self> {
        let Some(config) = params.extra_data.as_deref() else {
            return unsupported_error("a52: MP4 configuration is required");
        };
        let base = match config.get(..4) {
            Some(b"dac3") if config.len() == 7 => true,
            Some(b"dec3") if (9..=4100).contains(&config.len()) => false,
            _ => return decode_error("a52: invalid MP4 configuration"),
        };
        let mut state = eac3::decoder::Eac3DecoderState::default();
        state.set_s16_output(false);
        state.set_crc_check(true);
        Ok(Self {
            base,
            ac3: Box::new(audblk::Ac3State::new()),
            eac3: Box::new(state),
            scratch: Vec::with_capacity(MAX_FRAMES * MAX_CHANNELS),
            output: AudioBuffer::new(
                MAX_FRAMES as u64,
                SignalSpec::new(48_000, Channels::FRONT_LEFT),
            ),
            spec: None,
        })
    }
    pub(crate) fn name(&self) -> &'static str {
        if self.base { "ac3" } else { "eac3" }
    }
    pub(crate) fn reset(&mut self) {
        *self.ac3 = audblk::Ac3State::new();
        *self.eac3 = eac3::decoder::Eac3DecoderState::default();
        self.eac3.set_s16_output(false);
        self.eac3.set_crc_check(true);
        self.scratch.clear();
        self.output.clear();
    }
    pub(crate) fn decode(
        &mut self,
        packet: &Packet,
        next: Option<&Packet>,
    ) -> Result<AudioBufferRef<'_>> {
        self.output.clear();
        if let Err(error) = self.decode_into(&packet.data, next.map(|packet| packet.data.as_ref()))
        {
            self.reset();
            return Err(error);
        }
        Ok(self.output.as_audio_buffer_ref())
    }
    fn decode_into(&mut self, bytes: &[u8], next: Option<&[u8]>) -> Result<()> {
        validate(bytes, self.base)?;
        if self.base {
            let header = syncinfo::parse(bytes).map_err(|_| {
                symphonia::core::errors::Error::DecodeError("a52: invalid sync header")
            })?;
            let info = bsi::parse(&bytes[5..]).map_err(|_| {
                symphonia::core::errors::Error::DecodeError("a52: invalid bitstream information")
            })?;
            let channels = usize::from(info.nchans);
            if !(1..=MAX_CHANNELS).contains(&channels) {
                return unsupported_error("a52: unsupported channel count");
            }
            self.scratch.resize(MAX_FRAMES * channels, 0.0);
            audblk::decode_frame_strict(&mut self.ac3, &header, &info, bytes, &mut self.scratch)
                .map_err(|_| {
                    symphonia::core::errors::Error::Unsupported("a52: unsupported audio block")
                })?;
            let locations =
                eac3::decoder::Eac3DecoderState::default_dep_locations(info.acmod, info.lfeon);
            self.render(header.sample_rate, MAX_FRAMES, channels, &locations, false)
        } else {
            let before = self.eac3.frames_zero_filled;
            let next = next.filter(|bytes| validate(bytes, false).is_ok());
            let frame = eac3::decoder::decode_eac3_packet_with_next(&mut self.eac3, bytes, next)
                .map_err(|_| {
                    symphonia::core::errors::Error::DecodeError("a52: invalid enhanced frame")
                })?;
            if self.eac3.frames_zero_filled != before || self.eac3.last_error.is_some() {
                return unsupported_error("a52: unsupported enhanced audio block");
            }
            let mut locations =
                eac3::decoder::Eac3DecoderState::default_dep_locations(frame.acmod, frame.lfeon);
            for location in frame.dep_locations {
                if !locations.contains(&location) {
                    locations.push(location);
                }
            }
            self.render(
                frame.sample_rate,
                frame.samples as usize,
                frame.channels as usize,
                &locations,
                true,
            )
        }
    }
    fn render(
        &mut self,
        rate: u32,
        frames: usize,
        channels: usize,
        locations: &[Location],
        enhanced: bool,
    ) -> Result<()> {
        if !(8_000..=48_000).contains(&rate)
            || !(1..=MAX_FRAMES).contains(&frames)
            || !(1..=MAX_CHANNELS).contains(&channels)
            || locations.len() != channels
        {
            return unsupported_error("a52: unsupported PCM shape");
        }
        let mut order = [(u32::MAX, 0usize); MAX_CHANNELS];
        let mut mask = Channels::empty();
        for (index, &location) in locations.iter().enumerate() {
            let channel = channel(location)?;
            if mask.contains(channel) {
                return decode_error("a52: duplicate speaker");
            }
            mask |= channel;
            order[index] = (channel.bits(), index);
        }
        order.sort_unstable_by_key(|entry| entry.0);
        let spec = SignalSpec::new(rate, mask);
        if let Some(previous) = self.spec {
            if previous != spec {
                return unsupported_error("a52: PCM layout changed inside a track");
            }
        } else {
            self.output = AudioBuffer::new(MAX_FRAMES as u64, spec);
            self.spec = Some(spec);
        }
        let pcm = if enhanced {
            self.eac3.indep_pcm_f32()
        } else {
            &self.scratch
        };
        if pcm.len() != frames * channels || pcm.iter().any(|sample| !sample.is_finite()) {
            return unsupported_error("a52: invalid decoded samples");
        }
        self.output.render_reserved(Some(frames));
        for (plane, &(_, source)) in order.iter().take(channels).enumerate() {
            for (frame, output) in self.output.chan_mut(plane).iter_mut().enumerate() {
                *output = pcm[frame * channels + source];
            }
        }
        Ok(())
    }
}
fn channel(location: Location) -> Result<Channels> {
    Ok(match location {
        Location::Left => Channels::FRONT_LEFT,
        Location::Right => Channels::FRONT_RIGHT,
        Location::Center => Channels::FRONT_CENTRE,
        Location::Lfe => Channels::LFE1,
        Location::LeftSurround => Channels::SIDE_LEFT,
        Location::RightSurround => Channels::SIDE_RIGHT,
        Location::LeftRearSurround => Channels::REAR_LEFT,
        Location::RightRearSurround => Channels::REAR_RIGHT,
        Location::CenterSurround => Channels::REAR_CENTRE,
        _ => return unsupported_error("a52: unsupported speaker position"),
    })
}
fn validate(bytes: &[u8], base: bool) -> Result<()> {
    if !(6..=MAX_PACKET).contains(&bytes.len()) {
        return decode_error("a52: packet exceeds its bounds");
    }
    if base {
        let header = syncinfo::parse(bytes)
            .map_err(|_| symphonia::core::errors::Error::DecodeError("a52: invalid sync header"))?;
        if header.frame_length as usize != bytes.len() || bytes[5] >> 3 > 10 {
            return decode_error("a52: invalid base frame");
        }
        // Legacy half/quarter-rate profiles also alter bit allocation. Until
        // those profiles have reference coverage, do not play them at 2x/4x.
        if matches!(bytes[5] >> 3, 9 | 10) {
            return unsupported_error("a52: unsupported half-rate profile");
        }
        if !verify_packet_crc(bytes)
            .map_err(|_| {
                symphonia::core::errors::Error::DecodeError("a52: invalid checksum header")
            })?
            .all_ok()
        {
            return decode_error("a52: checksum failed");
        }
        return Ok(());
    }
    let mut offset = 0;
    let mut units = 0;
    while offset < bytes.len() {
        let rest = &bytes[offset..];
        if rest.len() < 6 || &rest[..2] != b"\x0b\x77" || !(11..=16).contains(&(rest[5] >> 3)) {
            return decode_error("a52: invalid enhanced header");
        }
        let length = ((((usize::from(rest[2]) & 7) << 8) | usize::from(rest[3])) + 1) * 2;
        if length < 6 || length > rest.len() {
            return decode_error("a52: truncated enhanced frame");
        }
        let stream_type = rest[2] >> 6;
        if units == 0 {
            if !matches!(stream_type, 0 | 2) || (rest[2] >> 3) & 7 != 0 {
                return unsupported_error("a52: unsupported program");
            }
        } else if stream_type != 1 {
            return unsupported_error("a52: multiple programs in one packet");
        }
        units += 1;
        if units > 9 {
            return decode_error("a52: too many dependent substreams");
        }
        if !verify_packet_crc(&rest[..length])
            .map_err(|_| {
                symphonia::core::errors::Error::DecodeError("a52: invalid checksum header")
            })?
            .all_ok()
        {
            return decode_error("a52: checksum failed");
        }
        offset += length;
    }
    Ok(())
}

/// MP4 timestamps are not necessarily measured at the decoded sample rate.
/// Only the A/52 path uses this clock; other codec timelines stay unchanged.
#[derive(Clone, Copy)]
pub(crate) struct Clock {
    scale: u64,
    denominator: u32,
}
impl Clock {
    pub(crate) fn new(rate: u32, base: Option<symphonia::core::units::TimeBase>) -> Result<Self> {
        let base = base.unwrap_or(symphonia::core::units::TimeBase::new(1, rate));
        if rate == 0 || base.numer == 0 || base.denom == 0 {
            return decode_error("a52: invalid time base");
        }
        Ok(Self {
            scale: u64::from(rate) * u64::from(base.numer),
            denominator: base.denom,
        })
    }
    pub(crate) fn to_ticks(self, samples: u64) -> u64 {
        (u128::from(samples) * u128::from(self.denominator) / u128::from(self.scale))
            .min(u128::from(u64::MAX)) as u64
    }
    pub(crate) fn to_samples(self, ticks: u64) -> u64 {
        (u128::from(ticks) * u128::from(self.scale) / u128::from(self.denominator))
            .min(u128::from(u64::MAX)) as u64
    }
}
