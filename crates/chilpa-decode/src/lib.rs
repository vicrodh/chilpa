//! Decoding local files into native samples.
//!
//! The bit-perfect rule of the ADR starts here: if the file carries
//! integers, integers come out. Converting to `f32` and scaling back is what
//! moved an LSB in QBZ, so this crate hands over
//! [`chilpa_audio::Samples::Int`] whenever the codec allows it, and only
//! uses floats when the source really is float. Vorbis has a fixed signed
//! 32-bit PCM output reference; its conversion boundary is in `vorbis`.

mod vorbis;

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

/// How much of a stream may be read to find out what it is.
const PROBE_BUDGET: u64 = 1024 * 1024;

/// A stream that ends once its budget is spent, until the budget is lifted.
struct Budgeted {
    inner: chilpa_stream::HttpStream,
    left: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

impl std::io::Read for Budgeted {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        use std::sync::atomic::Ordering::Relaxed;
        let left = self.left.load(Relaxed);
        if left == u64::MAX {
            return self.inner.read(buf);
        }
        let room = buf.len().min(left as usize);
        if room == 0 {
            return Ok(0);
        }
        let n = self.inner.read(&mut buf[..room])?;
        self.left.store(left - n as u64, Relaxed);
        Ok(n)
    }
}

/// AAC frames as a station sends them, handed on as the decoder expects.
///
/// An ADTS frame header says whether it is MPEG-4 or MPEG-2. The audio is
/// the same either way, but the reader underneath only looks for the MPEG-4
/// header (`FF F1`) and never finds a frame in a stream of MPEG-2 ones
/// (`FF F9`). This walks the stream frame by frame and clears that one bit.
/// The payload goes through untouched.
struct AdtsFrames<R> {
    inner: R,
    frame: Vec<u8>,
    at: usize,
}

impl<R: std::io::Read> AdtsFrames<R> {
    fn new(inner: R) -> Self {
        Self { inner, frame: Vec::new(), at: 0 }
    }

    /// Reads the next whole frame. False at the end of the stream.
    fn next_frame(&mut self) -> std::io::Result<bool> {
        let mut header = [0u8; 7];
        if !filled(&mut self.inner, &mut header)? {
            return Ok(false);
        }
        loop {
            // Twelve bits of sync, a layer of zero, and no checksum: the one
            // shape the reader underneath takes.
            let synced = header[0] == 0xff && header[1] & 0xf7 == 0xf1;
            let length = (usize::from(header[3] & 0x03) << 11)
                | (usize::from(header[4]) << 3)
                | usize::from(header[5] >> 5);
            if synced && length >= header.len() {
                header[1] = 0xf1;
                self.frame.clear();
                self.frame.extend_from_slice(&header);
                self.frame.resize(length, 0);
                self.at = 0;
                return filled(&mut self.inner, &mut self.frame[7..]);
            }
            // Not at a frame: one byte further on, until one lines up.
            header.copy_within(1.., 0);
            if !filled(&mut self.inner, &mut header[6..])? {
                return Ok(false);
            }
        }
    }
}

/// Fills the whole buffer. False if the stream ended first.
fn filled(reader: &mut impl std::io::Read, buf: &mut [u8]) -> std::io::Result<bool> {
    match reader.read_exact(buf) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Ok(false),
        Err(e) => Err(e),
    }
}

impl<R: std::io::Read> std::io::Read for AdtsFrames<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.at >= self.frame.len() && !self.next_frame()? {
            return Ok(0);
        }
        let n = buf.len().min(self.frame.len() - self.at);
        buf[..n].copy_from_slice(&self.frame[self.at..self.at + n]);
        self.at += n;
        Ok(n)
    }
}

/// What the decoder reads a stream through: forward only, of no known
/// length.
struct Forward {
    inner: Box<dyn std::io::Read + Send + Sync>,
    position: u64,
}

impl std::io::Read for Forward {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.position += n as u64;
        Ok(n)
    }
}

impl std::io::Seek for Forward {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        match pos {
            std::io::SeekFrom::Current(0) => Ok(self.position),
            std::io::SeekFrom::Start(n) if n == self.position => Ok(self.position),
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                chilpa_i18n::t("a stream cannot be rewound"),
            )),
        }
    }
}

impl symphonia::core::io::MediaSource for Forward {
    fn is_seekable(&self) -> bool {
        false
    }

    fn byte_len(&self) -> Option<u64> {
        None
    }
}

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
    /// What the decoder reads, by the library's name for it: a container
    /// such as MP4 does not say on its own whether it holds FLAC or AAC.
    codec: &'static str,
}

/// The library's name for a codec, empty for one it does not name.
fn codec_name(codec: symphonia::core::codecs::CodecType) -> &'static str {
    use symphonia::core::codecs::*;
    match codec {
        CODEC_TYPE_FLAC => "flac",
        CODEC_TYPE_ALAC => "alac",
        CODEC_TYPE_AAC => "aac",
        CODEC_TYPE_MP3 => "mp3",
        CODEC_TYPE_VORBIS => "vorbis",
        CODEC_TYPE_OPUS => "opus",
        CODEC_TYPE_WAVPACK => "wavpack",
        other => match symphonia::default::get_codecs().get_codec(other) {
            Some(known) if known.short_name.starts_with("pcm") => "pcm",
            _ => "",
        },
    }
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
    ///
    /// A stream never ends, so nothing here may read until it finds what it
    /// is looking for: finding the format gets a budget of bytes, and a
    /// stream that spends it without being recognized is refused. Without
    /// the budget an unrecognized station kept the player reading forever,
    /// and everything asked of it afterwards waited behind that.
    pub fn open_stream(stream: chilpa_stream::HttpStream) -> Result<Self, DecodeError> {
        let extension = stream.info().extension_hint();
        let budget = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(PROBE_BUDGET));
        let source = Budgeted { inner: stream, left: budget.clone() };
        let aac = extension.as_deref() == Some("aac");
        let inner: Box<dyn std::io::Read + Send + Sync> =
            if aac { Box::new(AdtsFrames::new(source)) } else { Box::new(source) };
        let mss = MediaSourceStream::new(Box::new(Forward { inner, position: 0 }), Default::default());
        let format: Box<dyn FormatReader> = if aac {
            // The server said it is AAC, so it is read as ADTS without
            // guessing: there is nothing to guess, and a guess reads on.
            let reader = symphonia::default::formats::AdtsReader::try_new(mss, &FormatOptions::default())
                .map_err(|e| DecodeError::Unsupported(e.to_string()))?;
            Box::new(reader)
        } else {
            let mut hint = Hint::new();
            if let Some(ext) = &extension {
                hint.with_extension(ext);
            }
            symphonia::default::get_probe()
                .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
                .map_err(|e| DecodeError::Unsupported(e.to_string()))?
                .format
        };
        // The budget also covers the first block, which is decoded up front:
        // a stream whose packets never decode would be read on forever
        // looking for one that does. Once it is open it is read for as long
        // as it plays.
        let opened = Self::from_format(format);
        budget.store(u64::MAX, std::sync::atomic::Ordering::Relaxed);
        opened
    }

    /// Opens a file on a server of mine, read by windows (ADR-017). It is
    /// the original, so it seeks and has a duration like one on disk.
    ///
    /// `extension` is the file's own ("flac", "m4a"), which the server says
    /// and the address does not.
    pub fn open_remote(file: chilpa_stream::Ranged, extension: Option<&str>) -> Result<Self, DecodeError> {
        let mut hint = Hint::new();
        if let Some(ext) = extension.filter(|e| !e.is_empty()) {
            hint.with_extension(ext);
        }
        if let Some(kind) = file.content_type() {
            hint.mime_type(kind);
        }
        Self::from_source(Box::new(file), hint)
    }

    fn from_source(
        source: Box<dyn symphonia::core::io::MediaSource>,
        hint: Hint,
    ) -> Result<Self, DecodeError> {
        let mss = MediaSourceStream::new(source, Default::default());
        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
            .map_err(|e| DecodeError::Unsupported(e.to_string()))?;
        Self::from_format(probed.format)
    }

    fn from_format(format: Box<dyn FormatReader>) -> Result<Self, DecodeError> {
        let track = format
            .default_track()
            .ok_or_else(|| DecodeError::Unsupported(chilpa_i18n::t("the file carries no tracks").into()))?;
        let track_id = track.id;
        let params = &track.codec_params;
        // An MP4 does not always declare channels or rate in the container:
        // the first decoded block carries them. So when they are missing, one
        // block is decoded up front and kept to hand over later.
        let declared = (params.sample_rate, params.channels.map(|c| c.count() as u16));
        // What the file says it carries. `bits_per_sample` is empty for ALAC
        // -- symphonia leaves it unset and the depth lives in the magic
        // cookie -- which is the whole reason `alac_bit_depth` exists.
        //
        // It is read once, here, and used for both answers. It used to feed
        // only the packing, while the stream format fell through to
        // `unwrap_or(16)`: every ALAC file, whatever it held, was announced
        // as 16 bit. A DAC that takes no S16 refused the track outright,
        // which is how this was found; one that takes S16 would have played
        // a 24 bit album at 16 and the window would have called it
        // bit-perfect, because nothing in Chilpa had converted anything --
        // it had simply asked for the wrong thing.
        let codec = codec_name(params.codec);
        let declared_bits = params
            .bits_per_sample
            .map(|b| b.min(32) as u8)
            .or_else(|| alac_bit_depth(params));
        // The file's bits win: 16 goes out as S16, 24 as S24In32.
        let integers = match declared_bits.unwrap_or(16) {
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
        let mut format = format;
        // One block is decoded up front and kept to hand over later. It says
        // what the container may not -- an MP4 does not always declare rate
        // or channels -- and it says what the decoder produces, which the
        // container never does. A lossy codec declares no bit depth and
        // decodes to floats, and the fallback above announced it as 16 bit
        // integers: negotiation opened a 16 bit word with nothing to report,
        // the packing scaled the floats into it, and the verdict called an
        // MP3 bit-perfect.
        let first = first_block(&mut *format, &mut *decoder, track_id, declared_bits, codec);
        let (sample_rate_hz, channels, pending) = match (declared, first) {
            ((Some(rate), Some(channels)), Ok((_, _, block))) => (rate, channels, Some(block)),
            (_, Ok((rate, channels, block))) => (rate, channels, Some(block)),
            // Nothing decodable, but the container says what it would be: it
            // opens and plays nothing, as it always did.
            ((Some(rate), Some(channels)), Err(_)) => (rate, channels, None),
            (_, Err(e)) => return Err(e),
        };
        let sample_format = match &pending {
            Some(Block::Int { .. }) if codec == "vorbis" => SampleFormat::S32,
            Some(Block::Float(_)) => SampleFormat::F32,
            _ => integers,
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
            codec,
        })
    }

    /// The codec it decodes ("flac", "aac"), empty when it is not one the
    /// library names.
    pub fn codec(&self) -> &'static str {
        self.codec
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
        let ts = if self.codec == "vorbis" { frame.saturating_sub(vorbis::SEEK_WINDOW) } else { frame };
        let to = SeekTo::TimeStamp { ts, track_id: self.track_id };
        let landed = self
            .format
            .seek(SeekMode::Accurate, to)
            .map_err(|e| DecodeError::Corrupt(e.to_string()))?;
        // The decoder carries state from where it was.
        self.decoder.reset();
        let actual = if self.codec == "vorbis" {
            vorbis::prime_after_seek(&mut *self.format, &mut *self.decoder, self.track_id)?
        } else { landed.actual_ts };
        Ok(chilpa_audio::seek::SeekPlan::after_landing(frame, actual))
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
                Ok(buffer) => return Ok(Some(interleave(&buffer, self.declared_bits, self.codec))),
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
    codec: &str,
) -> Result<(u32, u16, Block), DecodeError> {
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(e) => return Err(DecodeError::Unsupported(format!("no readable audio: {e}"))),
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
                    interleave(&buffer, declared_bits, codec),
                ));
            }
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(e) => return Err(DecodeError::Corrupt(e.to_string())),
        }
    }
}

/// Turns a planar symphonia buffer into interleaved samples, keeping the
/// original type.
fn interleave(buffer: &AudioBufferRef<'_>, declared_bits: Option<u8>, codec: &str) -> Block {
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
        AudioBufferRef::F32(b) if codec == "vorbis" => vorbis::pcm_reference(b),
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
