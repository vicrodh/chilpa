//! Codec capability description.
//!
//! Each codec implementation registered with the codec registry attaches one
//! of these structs to declare what it can do, what its constraints are, and
//! how the registry should rank it against alternative implementations of
//! the same codec id.
//!
//! The flag layout is a 6-column capability string (one letter per
//! capability, `.` when absent):
//!
//! ```text
//!  D..... = Decoding supported
//!  .E.... = Encoding supported
//!  ..V... = Video codec       ..A... = Audio       ..S... = Subtitle
//!  ..D... = Data              ..T... = Attachment
//!  ...I.. = Intra-frame-only codec
//!  ....L. = Lossy compression
//!  .....S = Lossless compression
//! ```

use std::fmt;

use crate::blob::BlobKind;
use crate::format::{MediaType, PixelFormat, SampleFormat};

/// Default priority for software implementations. Lower numbers are preferred
/// at resolution time, so register hardware impls with a smaller value (e.g.
/// `10`) and software fallbacks with the default `100`.
///
/// The same value and the same "lower is preferred" convention serve as
/// the default *resolution* priority for every registry tie-break
/// (container probes, codec tag / payload-magic claims) — see the
/// crate-level "Resolution order" section.
pub const DEFAULT_PRIORITY: i32 = 100;

/// What an implementation can do plus how it ranks vs alternatives.
///
/// `#[non_exhaustive]`: build it with [`audio`](Self::audio),
/// [`video`](Self::video) or [`subtitle`](Self::subtitle) and the
/// `with_*` builders, then read the public fields. Fields are added
/// over time (0.1.38 added the accepted sample rates / formats) and a
/// struct literal in a downstream crate would stop compiling each time.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct CodecCapabilities {
    /// Decoding supported by this implementation.
    pub decode: bool,
    /// Encoding supported by this implementation.
    pub encode: bool,
    /// Media type this implementation handles (audio, video, ...).
    pub media_type: MediaType,
    /// Every coded unit is independently decodable (no inter-frame
    /// prediction).
    pub intra_only: bool,
    /// Supports lossy compression.
    pub lossy: bool,
    /// Supports lossless compression. `lossy` and `lossless` may both
    /// be set for codecs that offer both modes.
    pub lossless: bool,
    /// Hardware-accelerated implementation (VAAPI/NVENC/QSV/VideoToolbox/...).
    pub hardware_accelerated: bool,
    /// Short identifier for this implementation, e.g. "flac_sw", "h264_qsv".
    pub implementation: String,
    /// Restrictions — `None` means "no constraint".
    pub max_width: Option<u32>,
    /// Maximum supported frame height in pixels; `None` = unconstrained.
    pub max_height: Option<u32>,
    /// Maximum supported bit rate in bits per second; `None` =
    /// unconstrained.
    pub max_bitrate: Option<u64>,
    /// Maximum supported audio sample rate in Hz; `None` = unconstrained.
    pub max_sample_rate: Option<u32>,
    /// Maximum supported audio channel count; `None` = unconstrained.
    pub max_channels: Option<u16>,
    /// Lower numbers are preferred. HW impls should be ~10, SW impls ~100.
    pub priority: i32,
    /// Pixel formats this implementation accepts (video only). An empty
    /// `Vec` means "any format" — resolution won't filter on it. When
    /// populated, the registry can skip impls whose accepted set does not
    /// include the format requested by the caller.
    pub accepted_pixel_formats: Vec<PixelFormat>,
    /// Audio sample rates (Hz) this implementation accepts as encoder
    /// input (audio only). An empty `Vec` means "any rate". When
    /// populated, a pipeline feeding the encoder is expected to resample
    /// a stream whose rate is not listed to one of these rates (the
    /// listed order is not a preference — consumers pick the closest
    /// rate that does not discard bandwidth).
    pub accepted_sample_rates: Vec<u32>,
    /// Audio sample formats this implementation accepts as encoder input
    /// (audio only). An empty `Vec` means "any format". When populated,
    /// the first entry is the preferred conversion target for a stream
    /// whose format is not listed.
    pub accepted_sample_formats: Vec<SampleFormat>,
    /// Metadata blob kinds this implementation's **encoder** embeds in
    /// its output: the [`BlobKind`]s it reads from the input stream's
    /// [`CodecParameters::blobs`](crate::CodecParameters::blobs) and
    /// from frame records and writes into the coded output (an ICC
    /// profile into the codestream's colour box, an Exif block into
    /// its application segment, a format's own custom kind into its
    /// native chunk). Empty — the default — means "nothing declared":
    /// a caller cannot tell whether a blob it hands over survives, and
    /// a gateway reports that as pass-through. When populated it is a
    /// promise in both directions: every listed kind is embedded when
    /// present on the input, every unlisted kind is dropped. Lets a
    /// caller answer "will the ICC profile survive this save?" before
    /// encoding a pixel. Attached with
    /// [`with_embedded_blob_kind`](Self::with_embedded_blob_kind) /
    /// [`with_embedded_blob_kinds`](Self::with_embedded_blob_kinds),
    /// queried with [`embeds_blob`](Self::embeds_blob). Decoder-only
    /// registrations leave it empty.
    pub embedded_blob_kinds: Vec<BlobKind>,
}

impl CodecCapabilities {
    /// Construct a software audio decoder/encoder capability set with sensible
    /// defaults — adjust fields after creation.
    pub fn audio(implementation: impl Into<String>) -> Self {
        Self {
            decode: false,
            encode: false,
            media_type: MediaType::Audio,
            intra_only: true, // audio packets are independently decodable in most codecs
            lossy: false,
            lossless: false,
            hardware_accelerated: false,
            implementation: implementation.into(),
            max_width: None,
            max_height: None,
            max_bitrate: None,
            max_sample_rate: None,
            max_channels: None,
            priority: DEFAULT_PRIORITY,
            accepted_pixel_formats: Vec::new(),
            accepted_sample_rates: Vec::new(),
            accepted_sample_formats: Vec::new(),
            embedded_blob_kinds: Vec::new(),
        }
    }

    /// Construct a software video decoder/encoder capability set with
    /// sensible defaults — adjust fields after creation.
    pub fn video(implementation: impl Into<String>) -> Self {
        Self {
            decode: false,
            encode: false,
            media_type: MediaType::Video,
            intra_only: false,
            lossy: false,
            lossless: false,
            hardware_accelerated: false,
            implementation: implementation.into(),
            max_width: None,
            max_height: None,
            max_bitrate: None,
            max_sample_rate: None,
            max_channels: None,
            priority: DEFAULT_PRIORITY,
            accepted_pixel_formats: Vec::new(),
            accepted_sample_rates: Vec::new(),
            accepted_sample_formats: Vec::new(),
            embedded_blob_kinds: Vec::new(),
        }
    }

    /// Construct a software subtitle decoder/encoder capability set with
    /// sensible defaults (intra-only, lossless) — adjust with the
    /// `with_*` builders.
    pub fn subtitle(implementation: impl Into<String>) -> Self {
        let mut caps = Self::audio(implementation);
        caps.media_type = MediaType::Subtitle;
        caps.intra_only = true;
        caps.lossless = true;
        caps
    }

    /// Override the media type (for capability sets built from the
    /// closest constructor, e.g. data or attachment streams).
    pub fn with_media_type(mut self, media_type: MediaType) -> Self {
        self.media_type = media_type;
        self
    }

    /// 6-character capability flag string (see the module docs for the
    /// column layout). Useful for `oxideav list`-style
    /// output.
    pub fn flag_string(&self) -> String {
        let mut s = String::with_capacity(6);
        s.push(if self.decode { 'D' } else { '.' });
        s.push(if self.encode { 'E' } else { '.' });
        s.push(match self.media_type {
            MediaType::Video => 'V',
            MediaType::Audio => 'A',
            MediaType::Subtitle => 'S',
            MediaType::Data => 'D',
            MediaType::Unknown => '.',
        });
        s.push(if self.intra_only { 'I' } else { '.' });
        s.push(if self.lossy { 'L' } else { '.' });
        s.push(if self.lossless { 'S' } else { '.' });
        s
    }

    // Builder-style helpers so registrations stay compact.

    /// Mark this implementation as supporting decode.
    pub fn with_decode(mut self) -> Self {
        self.decode = true;
        self
    }
    /// Mark this implementation as supporting encode.
    pub fn with_encode(mut self) -> Self {
        self.encode = true;
        self
    }
    /// Set the intra-frame-only flag.
    pub fn with_intra_only(mut self, v: bool) -> Self {
        self.intra_only = v;
        self
    }
    /// Set the lossy-compression flag.
    pub fn with_lossy(mut self, v: bool) -> Self {
        self.lossy = v;
        self
    }
    /// Set the lossless-compression flag.
    pub fn with_lossless(mut self, v: bool) -> Self {
        self.lossless = v;
        self
    }
    /// Set the hardware-accelerated flag.
    pub fn with_hardware(mut self, v: bool) -> Self {
        self.hardware_accelerated = v;
        self
    }
    /// Set the registry ranking priority (lower is preferred; HW ~10,
    /// SW ~100).
    pub fn with_priority(mut self, p: i32) -> Self {
        self.priority = p;
        self
    }
    /// Constrain the maximum frame size to `w` × `h` pixels.
    pub fn with_max_size(mut self, w: u32, h: u32) -> Self {
        self.max_width = Some(w);
        self.max_height = Some(h);
        self
    }
    /// Constrain the maximum bit rate (bits per second).
    pub fn with_max_bitrate(mut self, br: u64) -> Self {
        self.max_bitrate = Some(br);
        self
    }
    /// Constrain the maximum audio sample rate (Hz).
    pub fn with_max_sample_rate(mut self, sr: u32) -> Self {
        self.max_sample_rate = Some(sr);
        self
    }
    /// Constrain the maximum audio channel count.
    pub fn with_max_channels(mut self, ch: u16) -> Self {
        self.max_channels = Some(ch);
        self
    }

    /// Add one accepted pixel format. Appends — call multiple times to
    /// list several.
    pub fn with_pixel_format(mut self, fmt: PixelFormat) -> Self {
        self.accepted_pixel_formats.push(fmt);
        self
    }

    /// Replace the accepted pixel-format set wholesale.
    pub fn with_pixel_formats(mut self, fmts: Vec<PixelFormat>) -> Self {
        self.accepted_pixel_formats = fmts;
        self
    }

    /// Replace the accepted encoder-input sample-rate set wholesale
    /// (see [`Self::accepted_sample_rates`]).
    pub fn with_sample_rates(mut self, rates: Vec<u32>) -> Self {
        self.accepted_sample_rates = rates;
        self
    }

    /// Replace the accepted encoder-input sample-format set wholesale
    /// (see [`Self::accepted_sample_formats`]; the first entry is the
    /// preferred conversion target).
    pub fn with_sample_formats(mut self, fmts: Vec<SampleFormat>) -> Self {
        self.accepted_sample_formats = fmts;
        self
    }

    /// Add one metadata blob kind the encoder embeds (see
    /// [`Self::embedded_blob_kinds`]). Appends — call several times to
    /// list several; a kind already listed is not added twice.
    pub fn with_embedded_blob_kind(mut self, kind: impl Into<BlobKind>) -> Self {
        let kind = kind.into();
        if !self.embedded_blob_kinds.contains(&kind) {
            self.embedded_blob_kinds.push(kind);
        }
        self
    }

    /// Replace the embedded blob-kind set wholesale (see
    /// [`Self::embedded_blob_kinds`]). Duplicates are removed, first
    /// occurrence kept.
    pub fn with_embedded_blob_kinds(mut self, kinds: Vec<BlobKind>) -> Self {
        self.embedded_blob_kinds.clear();
        for kind in kinds {
            if !self.embedded_blob_kinds.contains(&kind) {
                self.embedded_blob_kinds.push(kind);
            }
        }
        self
    }

    /// `true` when the encoder declares that it embeds blobs of `kind`
    /// (see [`Self::embedded_blob_kinds`]). `false` both for an
    /// unlisted kind and for an encoder that declares nothing — use
    /// [`declares_embedded_blobs`](Self::declares_embedded_blobs) to
    /// tell the two apart.
    pub fn embeds_blob(&self, kind: &BlobKind) -> bool {
        self.embedded_blob_kinds.contains(kind)
    }

    /// `true` when the encoder declares its embedded blob kinds at
    /// all, i.e. [`Self::embedded_blob_kinds`] is non-empty; `false`
    /// means every answer from [`embeds_blob`](Self::embeds_blob) is
    /// "unknown", not "no".
    pub fn declares_embedded_blobs(&self) -> bool {
        !self.embedded_blob_kinds.is_empty()
    }

    /// Pick the encoder-input sample rate for a stream at `rate` Hz:
    /// `rate` itself when it is accepted (or every rate is), otherwise
    /// the smallest accepted rate above it (no bandwidth is lost), and
    /// failing that the largest accepted rate.
    pub fn pick_sample_rate(&self, rate: u32) -> u32 {
        if self.accepted_sample_rates.is_empty() || self.accepted_sample_rates.contains(&rate) {
            return rate;
        }
        let above = self
            .accepted_sample_rates
            .iter()
            .copied()
            .filter(|&r| r > rate)
            .min();
        above.unwrap_or_else(|| {
            self.accepted_sample_rates
                .iter()
                .copied()
                .max()
                .unwrap_or(rate)
        })
    }
}

impl fmt::Display for CodecCapabilities {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.flag_string(), self.implementation)
    }
}

impl CodecCapabilities {
    /// Whether this implementation's max-* restrictions are compatible
    /// with the requested codec parameters. `for_encode` is reserved
    /// for restrictions that apply asymmetrically. Used by the
    /// registry's `make_decoder` / `make_encoder` walker and by
    /// out-of-tree selection layers (e.g. `oxideav-pipeline`'s
    /// `CodecPreferences` filter).
    pub fn fits_params(&self, p: &crate::CodecParameters, for_encode: bool) -> bool {
        let _ = for_encode;
        if let (Some(max), Some(w)) = (self.max_width, p.width)
            && w > max
        {
            return false;
        }
        if let (Some(max), Some(h)) = (self.max_height, p.height)
            && h > max
        {
            return false;
        }
        if let (Some(max), Some(br)) = (self.max_bitrate, p.bit_rate)
            && br > max
        {
            return false;
        }
        if let (Some(max), Some(sr)) = (self.max_sample_rate, p.sample_rate)
            && sr > max
        {
            return false;
        }
        if let (Some(max), Some(ch)) = (self.max_channels, p.channels)
            && ch > max
        {
            return false;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_sample_rate_prefers_exact_then_next_above_then_max() {
        let any = CodecCapabilities::audio("any");
        assert_eq!(any.pick_sample_rate(44_100), 44_100);
        let opus = CodecCapabilities::audio("opus")
            .with_sample_rates(vec![8_000, 12_000, 16_000, 24_000, 48_000]);
        assert_eq!(opus.pick_sample_rate(16_000), 16_000);
        assert_eq!(opus.pick_sample_rate(44_100), 48_000);
        assert_eq!(opus.pick_sample_rate(11_025), 12_000);
        assert_eq!(opus.pick_sample_rate(96_000), 48_000);
    }

    #[test]
    fn embedded_blob_kinds_default_empty_and_builders_dedupe() {
        for caps in [
            CodecCapabilities::audio("a"),
            CodecCapabilities::video("v"),
            CodecCapabilities::subtitle("s"),
        ] {
            assert!(caps.embedded_blob_kinds.is_empty(), "{caps}");
            assert!(!caps.declares_embedded_blobs(), "{caps}");
            assert!(!caps.embeds_blob(&BlobKind::ICC), "{caps}");
        }
        let c = CodecCapabilities::video("png")
            .with_encode()
            .with_embedded_blob_kind(BlobKind::ICC)
            .with_embedded_blob_kind(BlobKind::EXIF)
            .with_embedded_blob_kind("png-text")
            .with_embedded_blob_kind(BlobKind::ICC);
        assert_eq!(
            c.embedded_blob_kinds,
            vec![BlobKind::ICC, BlobKind::EXIF, BlobKind::custom("png-text")]
        );
        assert!(c.declares_embedded_blobs());
        assert!(c.embeds_blob(&BlobKind::ICC));
        assert!(c.embeds_blob(&BlobKind::custom("png-text")));
        assert!(!c.embeds_blob(&BlobKind::XMP));
        // Wholesale replacement dedupes and keeps first occurrences.
        let c = c.with_embedded_blob_kinds(vec![BlobKind::XMP, BlobKind::IPTC, BlobKind::XMP]);
        assert_eq!(c.embedded_blob_kinds, vec![BlobKind::XMP, BlobKind::IPTC]);
        assert!(!c.embeds_blob(&BlobKind::ICC));
        // Replacing with nothing returns to "undeclared".
        let c = c.with_embedded_blob_kinds(Vec::new());
        assert!(!c.declares_embedded_blobs());
        // The declaration is orthogonal to every other field and to
        // the flag string.
        assert_eq!(c.flag_string(), ".EV...");
    }

    #[test]
    fn sample_format_builder_replaces_set() {
        let c = CodecCapabilities::audio("x")
            .with_sample_formats(vec![SampleFormat::S16, SampleFormat::F32]);
        assert_eq!(
            c.accepted_sample_formats,
            vec![SampleFormat::S16, SampleFormat::F32]
        );
    }
}
