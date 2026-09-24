//! Decoding local files into native samples.
//!
//! The bit-perfect rule of the ADR starts here: if the file carries
//! integers, integers come out. Converting to `f32` and scaling back is what
//! moved an LSB in QBZ, so this crate hands over
//! [`chilpa_audio::Samples::Int`] whenever the codec allows it, and only
//! uses floats when the source really is float.

use chilpa_audio::format::{SampleFormat, StreamFormat};
use std::fs::File;
use std::path::Path;
use symphonia::core::audio::{AudioBufferRef, Signal};
use symphonia::core::codecs::{Decoder, DecoderOptions};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

#[derive(Debug)]
pub enum DecodeError {
    Open(std::io::Error),
    Unsupported(String),
    Corrupt(String),
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::Open(e) => {
                write!(f, "{}", chilpa_i18n::t_args("could not open: {}", &[&e.to_string()]))
            }
            DecodeError::Unsupported(s) => {
                write!(f, "{}", chilpa_i18n::t_args("format not supported: {}", &[s]))
            }
            DecodeError::Corrupt(s) => {
                write!(f, "{}", chilpa_i18n::t_args("damaged file: {}", &[s]))
            }
        }
    }
}

/// A decoded block, in the type it arrived as.
pub enum Block {
    /// Interleaved integers, right-justified, with `bits` useful ones.
    Int { data: Vec<i32>, bits: u8 },
    Float(Vec<f32>),
}

impl Block {
    pub fn frames(&self, channels: u16) -> usize {
        let len = match self {
            Block::Int { data, .. } => data.len(),
            Block::Float(data) => data.len(),
        };
        len / channels.max(1) as usize
    }
}

/// An open file, ready to hand over blocks.
pub struct Decoded {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    track_id: u32,
    stream: StreamFormat,
    delay: u64,
    padding: u32,
    frames: Option<u64>,
    /// The first block, when it had to be decoded to learn how many channels
    /// the file has and at what rate it plays.
    pending: Option<Block>,
    /// The useful bits the file declares. They win over the width of the
    /// buffer the decoder hands back: symphonia fits a 16-bit FLAC into a
    /// 32-bit buffer, and keeping the 32 would make the packing believe it
    /// has to truncate.
    declared_bits: Option<u8>,
}

impl Decoded {
    /// Opens a file and prepares its decoder.
    pub fn open(path: &Path) -> Result<Self, DecodeError> {
        let file = File::open(path).map_err(DecodeError::Open)?;
        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }
        Self::from_source(Box::new(file), hint)
    }

    /// Opens a network stream. It has no duration and cannot be seeked.
    ///
    /// The format comes from what the server announced: a radio URL almost
    /// never carries an extension, and guessing from the name fails.
    pub fn open_stream(stream: chilpa_stream::HttpStream) -> Result<Self, DecodeError> {
        let mut hint = Hint::new();
        if let Some(ext) = stream.info().extension_hint() {
            hint.with_extension(&ext);
        }
        Self::from_source(Box::new(stream), hint)
    }

    fn from_source(
        source: Box<dyn symphonia::core::io::MediaSource>,
        hint: Hint,
    ) -> Result<Self, DecodeError> {
        let mss = MediaSourceStream::new(source, Default::default());
        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
            .map_err(|e| DecodeError::Unsupported(e.to_string()))?;
        let format = probed.format;
        let track = format
            .default_track()
            .ok_or_else(|| DecodeError::Unsupported(chilpa_i18n::t("the file carries no tracks").into()))?;
        let track_id = track.id;
        let params = &track.codec_params;
        // An MP4 does not always declare channels or rate in the container:
        // the first decoded block carries them. So when they are missing, one
        // block is decoded up front and kept to hand over later.
        let declared = (params.sample_rate, params.channels.map(|c| c.count() as u16));
        // The file's bits win: 16 goes out as S16, 24 as S24In32.
        let sample_format = match params.bits_per_sample.unwrap_or(16) {
            0..=16 => SampleFormat::S16,
            17..=24 => SampleFormat::S24In32,
            _ => SampleFormat::S32,
        };
        let mut decoder = symphonia::default::get_codecs()
            .make(params, &DecoderOptions::default())
            .map_err(|e| DecodeError::Unsupported(e.to_string()))?;
        let delay = u64::from(params.delay.unwrap_or(0));
        let padding = params.padding.unwrap_or(0);
        let frames = params.n_frames;
        let declared_bits = params
            .bits_per_sample
            .map(|b| b.min(32) as u8)
            .or_else(|| alac_bit_depth(params));
        let mut format = format;
        let mut pending = None;
        let (sample_rate_hz, channels) = match declared {
            (Some(rate), Some(channels)) => (rate, channels),
            _ => {
                let (rate, channels, block) =
                    first_block(&mut *format, &mut *decoder, track_id, declared_bits)?;
                pending = Some(block);
                (rate, channels)
            }
        };
        Ok(Self {
            format,
            decoder,
            track_id,
            stream: StreamFormat::new(sample_format, sample_rate_hz, channels),
            delay,
            padding,
            frames,
            declared_bits,
            pending,
        })
    }

    /// Leading frames the encoder added that are not music.
    pub fn encoder_delay(&self) -> u64 {
        self.delay
    }

    /// Trailing frames the encoder added.
    pub fn encoder_padding(&self) -> u32 {
        self.padding
    }

    /// How many frames the track has, when the container says so.
    pub fn frame_count(&self) -> Option<u64> {
        self.frames
    }

    /// Moves the reader as close as it can to the requested frame and returns
    /// the plan to land exactly there: the container almost always lands
    /// earlier, and those frames are decoded and dropped.
    pub fn seek_to_frame(&mut self, frame: u64) -> Result<chilpa_audio::seek::SeekPlan, DecodeError> {
        self.pending = None;
        let to = SeekTo::TimeStamp { ts: frame, track_id: self.track_id };
        let landed = self
            .format
            .seek(SeekMode::Accurate, to)
            .map_err(|e| DecodeError::Corrupt(e.to_string()))?;
        // The decoder carries state from where it was.
        self.decoder.reset();
        Ok(chilpa_audio::seek::SeekPlan::after_landing(frame, landed.actual_ts))
    }

    /// The format this file wants the device opened at.
    pub fn stream_format(&self) -> StreamFormat {
        self.stream
    }

    /// The next block. `None` when the file is done.
    pub fn next_block(&mut self) -> Result<Option<Block>, DecodeError> {
        if let Some(block) = self.pending.take() {
            return Ok(Some(block));
        }
        loop {
            let packet = match self.format.next_packet() {
                Ok(p) => p,
                Err(SymphoniaError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    return Ok(None)
                }
                Err(SymphoniaError::ResetRequired) => return Ok(None),
                Err(e) => return Err(DecodeError::Corrupt(e.to_string())),
            };
            if packet.track_id() != self.track_id {
                continue;
            }
            match self.decoder.decode(&packet) {
                Ok(buffer) => return Ok(Some(interleave(&buffer, self.declared_bits))),
                // A damaged packet does not bring the track down: it is skipped.
                Err(SymphoniaError::DecodeError(_)) => continue,
                Err(e) => return Err(DecodeError::Corrupt(e.to_string())),
            }
        }
    }
}

/// ALAC does not declare its bits in the container: they live in its cookie
/// (ALACSpecificConfig), where byte 5 is the depth. Without this, a 16-bit
/// ALAC is taken for 32 and the packing believes it has to truncate.
fn alac_bit_depth(params: &symphonia::core::codecs::CodecParameters) -> Option<u8> {
    if params.codec != symphonia::core::codecs::CODEC_TYPE_ALAC {
        return None;
    }
    let cookie = params.extra_data.as_deref()?;
    let bits = *cookie.get(5)?;
    (1..=32).contains(&bits).then_some(bits)
}

/// Decodes up to the first block with audio and returns rate, channels and
/// that block, for containers that do not declare them.
fn first_block(
    format: &mut dyn FormatReader,
    decoder: &mut dyn Decoder,
    track_id: u32,
    declared_bits: Option<u8>,
) -> Result<(u32, u16, Block), DecodeError> {
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(e) => return Err(DecodeError::Unsupported(format!("sin audio legible: {e}"))),
        };
        if packet.track_id() != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(buffer) => {
                let spec = *buffer.spec();
                return Ok((
                    spec.rate,
                    spec.channels.count() as u16,
                    interleave(&buffer, declared_bits),
                ));
            }
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(e) => return Err(DecodeError::Corrupt(e.to_string())),
        }
    }
}

/// Turns a planar symphonia buffer into interleaved samples, keeping the
/// original type.
fn interleave(buffer: &AudioBufferRef<'_>, declared_bits: Option<u8>) -> Block {
    // The decoder lays the samples out left-justified inside the buffer: a
    // 16-bit FLAC arrives in a 32-bit buffer with the sample shifted 16
    // places. It is moved down to where it belongs, which is exact because
    // the low bits are zero, and that way the packing knows how many bits
    // it is dealing with.
    macro_rules! ints {
        ($buf:expr, $width:expr, $conv:expr) => {{
            let bits = declared_bits.unwrap_or($width).min($width);
            let offset = $width - bits;
            let frames = $buf.frames();
            let channels = $buf.spec().channels.count();
            let mut data = Vec::with_capacity(frames * channels);
            for f in 0..frames {
                for c in 0..channels {
                    data.push($conv($buf.chan(c)[f]) >> offset);
                }
            }
            Block::Int { data, bits }
        }};
    }
    match buffer {
        AudioBufferRef::S16(b) => ints!(b, 16, |v: i16| i32::from(v)),
        AudioBufferRef::S24(b) => ints!(b, 24, |v: symphonia::core::sample::i24| v.inner()),
        AudioBufferRef::S32(b) => ints!(b, 32, |v: i32| v),
        AudioBufferRef::U8(b) => ints!(b, 8, |v: u8| i32::from(v) - 128),
        AudioBufferRef::F32(b) => {
            let frames = b.frames();
            let channels = b.spec().channels.count();
            let mut data = Vec::with_capacity(frames * channels);
            for f in 0..frames {
                for c in 0..channels {
                    data.push(b.chan(c)[f]);
                }
            }
            Block::Float(data)
        }
        other => {
            // The remaining formats are converted to f32 once, and from there the
            // path no longer claims to be bit-perfect.
            let mut owned = other.make_equivalent::<f32>();
            other.convert(&mut owned);
            let frames = owned.frames();
            let channels = owned.spec().channels.count();
            let mut data = Vec::with_capacity(frames * channels);
            for f in 0..frames {
                for c in 0..channels {
                    data.push(owned.chan(c)[f]);
                }
            }
            Block::Float(data)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_that_does_not_exist_fails_to_open() {
        let r = Decoded::open(Path::new("/does/not/exist.flac"));
        assert!(matches!(r, Err(DecodeError::Open(_))));
    }

    #[test]
    fn frames_are_counted_per_channel() {
        let b = Block::Int { data: vec![0; 10], bits: 16 };
        assert_eq!(b.frames(2), 5);
    }
}
