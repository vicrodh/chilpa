//! Stream metadata shared between containers and codecs.

use crate::blob::{BlobKind, MetadataBlob};
use crate::format::{ChannelLayout, MediaType, PixelFormat, SampleFormat};
use crate::layer::LayerInfo;
use crate::limits::DecoderLimits;
use crate::options::CodecOptions;
use crate::rational::Rational;
use crate::signal::{ColorRange, ColorSignal};
use crate::time::TimeBase;

/// A stable identifier for a codec. Codec crates register a `CodecId` so the
/// codec registry can look them up by name.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CodecId(pub String);

impl CodecId {
    /// Build a `CodecId` from any string-like codec name (e.g. `"h264"`).
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// The codec name as a borrowed string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for CodecId {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

impl std::fmt::Display for CodecId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A codec identifier scoped to a container format — the thing a
/// demuxer reads out of the file to name a codec. Resolved to a
/// [`CodecId`] by the codec registry.
///
/// Centralising these in the registry (instead of each container
/// hand-rolling its own FourCC → CodecId table) lets:
///
/// * a codec crate declare its own tag claims in `register()`, keeping
///   ownership co-located with the decoder;
/// * multiple codecs claim the same tag with priority ordering;
/// * optional per-claim probes disambiguate the tag-collision cases
///   that happen everywhere in the wild (DIV3 that's actually MPEG-4
///   Part 2, XVID that's actually MS-MPEG4v3, audio wFormatTag=0x0055
///   that could be MP3 or — very rarely — something else, etc.).
///
/// **Payload magics are intentionally absent** from this enum: some
/// carriage formats have no codec tag at all — the codec is announced
/// by a magic byte prefix on the payload itself (an Ogg logical
/// stream's first packet is the canonical case), which is
/// prefix-matched rather than looked up as an exact key. Such claims
/// are declared via
/// [`CodecInfo::payload_magic`](crate::registry::CodecInfo::payload_magic)
/// and resolved via [`CodecResolver::resolve_payload_magic`].
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum CodecTag {
    /// Four-character code used by AVI's `bmih.biCompression`, MP4 /
    /// QuickTime sample-entry type, Matroska V_/A_ tags built around
    /// FourCC, and many others. Always stored with alphabetic bytes
    /// upper-cased so lookups are case-insensitive; non-alphabetic
    /// bytes are preserved as-is.
    Fourcc([u8; 4]),

    /// AVI / WAV `WAVEFORMATEX::wFormatTag` (e.g. 0x0001 = PCM,
    /// 0x0055 = MP3, 0x00FF = "raw" AAC, 0x1610 = AAC ADTS).
    WaveFormat(u16),

    /// MP4 ObjectTypeIndication (ISO/IEC 14496-1 Table 5 / the values
    /// in an MP4 `esds` `DecoderConfigDescriptor`). e.g. 0x40 = MPEG-4
    /// AAC, 0x20 = MPEG-4 Visual, 0x69 = MP3.
    Mp4ObjectType(u8),

    /// Matroska `CodecID` element (full string, e.g.
    /// `"V_MPEG4/ISO/AVC"`, `"A_AAC"`, `"A_VORBIS"`).
    Matroska(String),
}

impl CodecTag {
    /// Build a FourCC tag, upper-casing alphabetic bytes.
    pub fn fourcc(raw: &[u8; 4]) -> Self {
        let mut out = [0u8; 4];
        for i in 0..4 {
            out[i] = raw[i].to_ascii_uppercase();
        }
        Self::Fourcc(out)
    }

    /// Build a [`CodecTag::WaveFormat`] tag from a `wFormatTag` value.
    pub fn wave_format(tag: u16) -> Self {
        Self::WaveFormat(tag)
    }

    /// Build a [`CodecTag::Mp4ObjectType`] tag from an MP4
    /// ObjectTypeIndication byte.
    pub fn mp4_object_type(oti: u8) -> Self {
        Self::Mp4ObjectType(oti)
    }

    /// Build a [`CodecTag::Matroska`] tag from a full Matroska
    /// `CodecID` string (e.g. `"A_VORBIS"`).
    pub fn matroska(id: impl Into<String>) -> Self {
        Self::Matroska(id.into())
    }
}

impl std::fmt::Display for CodecTag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fourcc(fcc) => {
                // Print as bytes when ASCII-printable, else as hex.
                if fcc.iter().all(|b| b.is_ascii_graphic() || *b == b' ') {
                    write!(f, "fourcc({})", std::str::from_utf8(fcc).unwrap_or("????"))
                } else {
                    write!(
                        f,
                        "fourcc(0x{:02X}{:02X}{:02X}{:02X})",
                        fcc[0], fcc[1], fcc[2], fcc[3]
                    )
                }
            }
            Self::WaveFormat(t) => write!(f, "wFormatTag(0x{t:04X})"),
            Self::Mp4ObjectType(o) => write!(f, "mp4_oti(0x{o:02X})"),
            Self::Matroska(s) => write!(f, "matroska({s})"),
        }
    }
}

/// Context passed to a codec's probe function during tag resolution.
///
/// Built by the demuxer from whatever it has already parsed (stream
/// format block, a peek at the first packet, numeric hints like
/// `bits_per_sample`). Probes read fields directly; the struct is
/// `#[non_exhaustive]` so additional hints can be added later without
/// breaking codec crates that match on it.
///
/// The canonical construction pattern, for a demuxer:
///
/// ```
/// # use oxideav_core::{CodecTag, ProbeContext};
/// let tag = CodecTag::wave_format(0x0001);
/// let ctx = ProbeContext::new(&tag)
///     .bits(24)
///     .channels(2)
///     .sample_rate(48_000);
/// # let _ = ctx;
/// ```
///
/// Codec authors read fields like `ctx.bits_per_sample` / `ctx.tag`
/// directly — `#[non_exhaustive]` forbids struct-literal construction
/// from outside this crate but does not restrict field access.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct ProbeContext<'a> {
    /// The tag being resolved — always set.
    pub tag: &'a CodecTag,
    /// Raw container-level stream-format blob if available
    /// (e.g. WAVEFORMATEX, BITMAPINFOHEADER, MP4 sample-entry bytes,
    /// Matroska `CodecPrivate`). Format is container-specific.
    pub header: Option<&'a [u8]>,
    /// First packet bytes if the demuxer has already read one.
    /// Most demuxers resolve tags at stream-discovery time before any
    /// packet exists; this is `None` in that case.
    pub packet: Option<&'a [u8]>,
    /// Audio: bits per sample (from WAVEFORMATEX, MP4 sample entry,
    /// Matroska `BitDepth`, etc.).
    pub bits_per_sample: Option<u16>,
    /// Audio: channel count from the container's stream header.
    pub channels: Option<u16>,
    /// Audio: sample rate in Hz from the container's stream header.
    pub sample_rate: Option<u32>,
    /// Video: coded frame width in pixels from the container's stream
    /// header.
    pub width: Option<u32>,
    /// Video: coded frame height in pixels from the container's stream
    /// header.
    pub height: Option<u32>,
}

impl<'a> ProbeContext<'a> {
    /// Start building a context for `tag` with every hint field empty.
    pub fn new(tag: &'a CodecTag) -> Self {
        Self {
            tag,
            header: None,
            packet: None,
            bits_per_sample: None,
            channels: None,
            sample_rate: None,
            width: None,
            height: None,
        }
    }

    /// Builder method: attach the raw container-level stream-format
    /// blob (WAVEFORMATEX, BITMAPINFOHEADER, MP4 sample-entry bytes,
    /// Matroska `CodecPrivate`, ...).
    pub fn header(mut self, h: &'a [u8]) -> Self {
        self.header = Some(h);
        self
    }

    /// Builder method: attach the first packet's bytes, when the
    /// demuxer has already read one.
    pub fn packet(mut self, p: &'a [u8]) -> Self {
        self.packet = Some(p);
        self
    }

    /// Builder method: set the audio bits-per-sample hint.
    pub fn bits(mut self, n: u16) -> Self {
        self.bits_per_sample = Some(n);
        self
    }

    /// Builder method: set the audio channel-count hint.
    pub fn channels(mut self, n: u16) -> Self {
        self.channels = Some(n);
        self
    }

    /// Builder method: set the audio sample-rate hint (Hz).
    pub fn sample_rate(mut self, n: u32) -> Self {
        self.sample_rate = Some(n);
        self
    }

    /// Builder method: set the video frame-width hint (pixels).
    pub fn width(mut self, n: u32) -> Self {
        self.width = Some(n);
        self
    }

    /// Builder method: set the video frame-height hint (pixels).
    pub fn height(mut self, n: u32) -> Self {
        self.height = Some(n);
        self
    }
}

/// Confidence value returned by a probe. `1.0` means "certainly me",
/// `0.0` means "not me", values in between mean "partial evidence — if
/// no higher-confidence claim exists, this should win". The registry
/// picks the claim with the highest returned confidence and skips any
/// that return `0.0`.
pub type Confidence = f32;

/// A probe function a codec attaches to its registration to
/// disambiguate tag collisions. Called once per candidate
/// registration during `resolve_tag`.
pub type ProbeFn = fn(&ProbeContext) -> Confidence;

/// Resolve a [`CodecTag`] (FourCC / WAVEFORMATEX / Matroska id / …) to a
/// [`CodecId`]. The [`oxideav-codec`](https://crates.io/crates/oxideav-codec)
/// registry implements this, but defining the trait here lets
/// containers consume tag resolution via `&dyn CodecResolver` without
/// pulling in the codec crate as a direct dependency.
///
/// **Inverse direction** (codec_id → wire tag) is intentionally NOT a
/// method on this trait. Wire tags are per-stream state: different
/// `mpeg4video` streams correctly identify as `DIVX` / `XVID` /
/// `MP4V` / `FMP4`, different `h264` streams as `H264` vs `AVC1`,
/// and so on. The stream's [`CodecParameters::tag`] field is the
/// canonical home for that data — set by the demuxer when reading
/// existing media and by the encoder via its `output_params()` at
/// configure-time. A registry-level "give me the canonical tag for
/// this codec_id" lookup walks registration order and returns
/// whichever tag was declared first, which is arbitrary and breaks
/// round-trip preservation.
pub trait CodecResolver: Sync {
    /// Resolve the tag in `ctx.tag` to a codec id. Implementations walk
    /// every registration whose tag set contains the tag, call each
    /// probe (treating `None` as "always 1.0"), and return the id with
    /// the highest resulting confidence. Ties are broken by
    /// registration order.
    fn resolve_tag(&self, ctx: &ProbeContext) -> Option<CodecId>;

    /// Resolve a codec from a stream's leading payload bytes, for
    /// carriage formats that announce the codec in the payload itself
    /// rather than through a container tag.
    ///
    /// The canonical case is Ogg, which has no numeric codec tag at
    /// all: a logical stream announces its codec purely through a
    /// magic byte prefix at the start of the first packet
    /// (`\x01vorbis`, `OpusHead`, `\x80theora`, `\x7fFLAC`,
    /// `Speex   `, …); raw elementary streams identified by a file
    /// head are the same shape. That identification model is
    /// prefix-shaped rather than exact-key-shaped, so it gets its own
    /// resolution entry point instead of a [`CodecTag`] form: codec
    /// crates declare the magic prefixes they answer to at
    /// registration time, and the caller hands the stream's leading
    /// payload bytes (an Ogg demuxer: the first packet of a logical
    /// stream; a raw-stream prober: the file head — or however much of
    /// it is available) to this method. Implementations return the
    /// codec whose declared magic is a prefix of `first_bytes`,
    /// preferring the **longest**
    /// matching magic (most specific claim) and breaking remaining
    /// ties by registration order.
    ///
    /// The default implementation resolves nothing, so existing
    /// resolver implementations (and [`NullCodecResolver`]) are
    /// unaffected.
    fn resolve_payload_magic(&self, first_bytes: &[u8]) -> Option<CodecId> {
        let _ = first_bytes;
        None
    }
}

/// Null resolver that resolves nothing — useful as a default when a
/// caller doesn't have a real registry handy (e.g. unit tests, or
/// legacy callers of the tag-free `open()` APIs).
#[derive(Default, Clone, Copy)]
pub struct NullCodecResolver;

impl CodecResolver for NullCodecResolver {
    fn resolve_tag(&self, _ctx: &ProbeContext) -> Option<CodecId> {
        None
    }
}

/// Codec-level parameters shared between demuxer/muxer and en/decoder.
///
/// **Marked `#[non_exhaustive]`** — construction via struct-literal
/// syntax (including functional-update `CodecParameters { ..base }`)
/// is not possible outside this crate. Use the [`audio`](Self::audio) /
/// [`video`](Self::video) / [`subtitle`](Self::subtitle) /
/// [`data`](Self::data) constructors, the `with_*` builders and direct
/// field assignment, so new fields can be added without another semver
/// break.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct CodecParameters {
    /// Registry identifier of the codec this stream is encoded with.
    pub codec_id: CodecId,
    /// Whether this stream is audio, video, subtitle, or data. Set by
    /// the constructor ([`audio`](Self::audio), [`video`](Self::video),
    /// [`subtitle`](Self::subtitle), [`data`](Self::data)).
    pub media_type: MediaType,

    // Audio-specific
    /// Audio: sample rate in Hz. `None` for non-audio streams.
    pub sample_rate: Option<u32>,
    /// Audio: number of channels. See [`Self::resolved_channels`] for
    /// the layout-aware accessor.
    pub channels: Option<u16>,
    /// Audio: sample format of the decoded output (or encoder input).
    pub sample_format: Option<SampleFormat>,
    /// Speaker layout for the audio stream. **This is the canonical
    /// answer to "what layout does this stream have?"** — layout is a
    /// stream-level property and is intentionally *not* duplicated on
    /// individual [`AudioFrame`](crate::AudioFrame)s.
    ///
    /// Optional and additive alongside [`channels`](Self::channels): a
    /// codec/container that only knows the count can leave this `None`
    /// and consumers will fall back to [`ChannelLayout::from_count`]
    /// via [`Self::resolved_layout`]. When both are set, they must
    /// agree on channel count.
    pub channel_layout: Option<ChannelLayout>,

    // Video-specific
    /// Video: coded frame width in pixels. `None` for non-video streams.
    pub width: Option<u32>,
    /// Video: coded frame height in pixels. `None` for non-video streams.
    pub height: Option<u32>,
    /// Video: pixel format of the decoded output (or encoder input).
    pub pixel_format: Option<PixelFormat>,
    /// Video: nominal frame rate in frames per second, as a rational
    /// (e.g. 30000/1001). `None` when unknown or variable.
    pub frame_rate: Option<Rational>,

    /// Per-codec setup bytes (e.g., SPS/PPS, OpusHead). Format defined by codec.
    pub extradata: Vec<u8>,

    /// Nominal stream bit rate in bits per second, when the container
    /// or encoder declares one.
    pub bit_rate: Option<u64>,

    /// Codec-specific tuning knobs (e.g. `{"interlace": "true"}` for PNG's
    /// Adam7 encode, `{"crf": "23"}` for h264). Empty by default. The shape
    /// is declared by each codec's options struct — see
    /// [`crate::options`]. Parsed once at encoder/decoder construction;
    /// the hot path never touches this.
    pub options: CodecOptions,

    /// DoS-protection caps threaded into every decoder constructed from
    /// these parameters. See [`DecoderLimits`] for the semantics of each
    /// field. Defaults are conservative-but-finite (32 k × 32 k pixels,
    /// 1 GiB per arena, etc.) — every existing real-world stream
    /// decodes unchanged. Tighten via [`Self::with_limits`] when the
    /// caller wants to harden the pipeline against untrusted input.
    pub limits: DecoderLimits,

    /// Optional 0-based device selector for hardware-accelerated codecs.
    /// `None` (the default) means "use the backend's default device";
    /// `Some(n)` requests device `n` from the backend's
    /// [`crate::engine::HwDeviceInfo`] enumeration order.
    ///
    /// Software codecs ignore this field. Hardware codecs read it as
    /// `params.device_index.unwrap_or(0)` to pick which physical engine
    /// to bind to. Indexing matches the order of devices reported by the
    /// codec entry's `engine_probe` function.
    pub device_index: Option<u32>,

    /// On-wire tag for this stream — the FourCC / WAVEFORMATEX
    /// `wFormatTag` / MP4 ObjectTypeIndication / Matroska `CodecID`
    /// string carried by the container. Set by the **producer**:
    ///
    /// * **Demuxers** populate this from the stream's container
    ///   header at read-time so muxers re-emitting the same stream
    ///   round-trip the original tag byte-for-byte (`mpeg4video`
    ///   demuxed as `DIVX` re-muxes as `DIVX`, not as the codec
    ///   crate's first-declared `XVID`).
    /// * **Encoders** populate this in [`crate::Encoder::output_params`]
    ///   to tell muxers which wire tag to write — needed for
    ///   multi-FourCC codecs whose configuration (pixel format / bit
    ///   depth / alpha / chroma sampling) selects one of several
    ///   valid FourCCs (e.g. MagicYUV's 17 native v7 codes).
    ///
    /// `None` is the default — sensible for in-memory streams that
    /// haven't been bound to a container yet. Muxers that need a
    /// wire tag and find `None` here will fall back to whatever
    /// container-specific synthesis they support (e.g. AVI's PCM
    /// `wFormatTag` synthesis from `sample_format`, or the
    /// `extradata[0..4]` printable-FourCC hint for legacy callers)
    /// and otherwise return `Error::Unsupported`.
    pub tag: Option<CodecTag>,

    /// BCP-47 / ISO 639 language tag (`"en"`, `"jpn"`, …) when the
    /// container labels the stream's language. `None` means
    /// "unspecified" — not "neutral".
    ///
    /// Demuxers populate this from the container's per-track language
    /// element (MKV `Language` / `LanguageBCP47`, MP4 `mdhd` ISO 639-2
    /// code, Ogg `LANGUAGE=` comment, …). Muxers re-emit it on the
    /// matching container element so a round-trip preserves the
    /// caller-visible tag byte-for-byte. No validation is performed
    /// here — the value is whatever string the producer supplied.
    pub language: Option<String>,

    /// Video: the stream's colour-signal description — sample range
    /// plus the H.273 colour primaries / transfer characteristics /
    /// matrix coefficients. See [`crate::signal`] for the semantics.
    ///
    /// Defaults to [`ColorSignal::unspecified`] (every field open).
    /// Set by the **producer**: a demuxer from the container's colour
    /// record (an ISOBMFF `colr` box, a Matroska `Colour` element, …),
    /// a decoder from the bitstream's own signalling when the
    /// container had none, an encoder in `output_params()` to tell the
    /// muxer what to write. Consumers (colour converters, renderers,
    /// muxers) read it through
    /// [`resolved_color_range`](Self::resolved_color_range) and the
    /// fields directly; a per-frame
    /// [`VideoFrame::color_signal`](crate::VideoFrame::color_signal)
    /// record, when present, refines this stream-level value.
    ///
    /// Not compared by [`matches_core`](Self::matches_core): like
    /// `extradata`, it is descriptive metadata that a lossless copy
    /// may legitimately rewrite.
    pub color_signal: ColorSignal,

    /// Video: description of the stream's layers for multi-layer /
    /// multi-view content (scalable or multi-view coding, auxiliary
    /// alpha / depth layers). Empty — the default — for single-layer
    /// streams and for producers that do not describe the structure.
    /// See [`crate::layer`] for the semantics and
    /// [`VideoFrame::layer`](crate::VideoFrame::layer) for the
    /// per-frame identity that pairs with it.
    pub layers: Vec<LayerInfo>,

    /// Opaque named metadata payloads that travel with the stream — an
    /// ICC profile, Exif, XMP, IPTC, an audio track's cover art, or
    /// any format-specific blob under a custom name. See
    /// [`crate::blob`] for the mechanism and the payload convention of
    /// each well-known [`BlobKind`]. Empty — the default — when the
    /// producer found none.
    ///
    /// Set by the **producer**: a demuxer from the container's
    /// metadata (a PNG `iCCP` chunk, a JPEG `APP1` segment, an MP4
    /// `covr` atom, …), a still-image decoder for formats whose
    /// metadata lives in the codestream, an encoder in
    /// `output_params()` to hand the muxer what it should embed. The
    /// framework never interprets the bytes; consumers route them by
    /// kind through [`blob`](Self::blob) / [`blobs_of`](Self::blobs_of).
    /// A per-frame [`VideoFrame::blob`](crate::VideoFrame::blob) of the
    /// same kind, when present, refines the stream-level one.
    ///
    /// Not compared by [`matches_core`](Self::matches_core): like
    /// `extradata`, descriptive metadata a lossless copy may rewrite.
    pub blobs: Vec<MetadataBlob>,
}

impl CodecParameters {
    /// Construct audio codec parameters with every optional field
    /// unset. Chain builder methods ([`channels`](Self::channels),
    /// [`channel_layout`](Self::channel_layout), ...) or assign fields
    /// directly to fill in the format.
    pub fn audio(codec_id: CodecId) -> Self {
        Self {
            codec_id,
            media_type: MediaType::Audio,
            sample_rate: None,
            channels: None,
            sample_format: None,
            channel_layout: None,
            width: None,
            height: None,
            pixel_format: None,
            frame_rate: None,
            extradata: Vec::new(),
            bit_rate: None,
            options: CodecOptions::default(),
            limits: DecoderLimits::default(),
            device_index: None,
            tag: None,
            language: None,
            color_signal: ColorSignal::unspecified(),
            layers: Vec::new(),
            blobs: Vec::new(),
        }
    }

    /// True when `self` and `other` have the same codec_id and core
    /// format parameters (sample_rate/channels/sample_format for audio,
    /// width/height/pixel_format for video). Extradata and bitrate
    /// differences are tolerated — many containers rewrite extradata
    /// losslessly during a copy operation. `channel_layout` is compared
    /// only via the channel count (through [`Self::resolved_layout`]) so
    /// a stream that surfaces an explicit layout still matches a
    /// count-only stream of the same width.
    pub fn matches_core(&self, other: &CodecParameters) -> bool {
        self.codec_id == other.codec_id
            && self.sample_rate == other.sample_rate
            && self.channels == other.channels
            && self.sample_format == other.sample_format
            && self.width == other.width
            && self.height == other.height
            && self.pixel_format == other.pixel_format
    }

    /// Construct video codec parameters with every optional field
    /// unset. Assign `width` / `height` / `pixel_format` (or use the
    /// builder methods) to fill in the format.
    pub fn video(codec_id: CodecId) -> Self {
        Self {
            codec_id,
            media_type: MediaType::Video,
            sample_rate: None,
            channels: None,
            sample_format: None,
            channel_layout: None,
            width: None,
            height: None,
            pixel_format: None,
            frame_rate: None,
            extradata: Vec::new(),
            bit_rate: None,
            options: CodecOptions::default(),
            limits: DecoderLimits::default(),
            device_index: None,
            tag: None,
            language: None,
            color_signal: ColorSignal::unspecified(),
            layers: Vec::new(),
            blobs: Vec::new(),
        }
    }

    /// Construct subtitle codec parameters. No format-specific fields
    /// are populated — subtitle codecs typically only carry an opaque
    /// `extradata` blob (the format's header / style block) and the
    /// codec id.
    pub fn subtitle(codec_id: CodecId) -> Self {
        Self {
            codec_id,
            media_type: MediaType::Subtitle,
            sample_rate: None,
            channels: None,
            sample_format: None,
            channel_layout: None,
            width: None,
            height: None,
            pixel_format: None,
            frame_rate: None,
            extradata: Vec::new(),
            bit_rate: None,
            options: CodecOptions::default(),
            limits: DecoderLimits::default(),
            device_index: None,
            tag: None,
            language: None,
            color_signal: ColorSignal::unspecified(),
            layers: Vec::new(),
            blobs: Vec::new(),
        }
    }

    /// Construct generic data-stream codec parameters (timed metadata,
    /// chapters, etc.). Like [`Self::subtitle`], no format-specific
    /// fields are populated.
    pub fn data(codec_id: CodecId) -> Self {
        Self {
            codec_id,
            media_type: MediaType::Data,
            sample_rate: None,
            channels: None,
            sample_format: None,
            channel_layout: None,
            width: None,
            height: None,
            pixel_format: None,
            frame_rate: None,
            extradata: Vec::new(),
            bit_rate: None,
            options: CodecOptions::default(),
            limits: DecoderLimits::default(),
            device_index: None,
            tag: None,
            language: None,
            color_signal: ColorSignal::unspecified(),
            layers: Vec::new(),
            blobs: Vec::new(),
        }
    }

    /// Builder method: set the channel count.
    ///
    /// Pairs with [`Self::channel_layout`] for the layout. The two are
    /// kept as independent fields so a codec that only knows one or the
    /// other can populate just the field it has; [`Self::resolved_layout`]
    /// derives a layout from whatever is set.
    pub fn channels(mut self, n: u16) -> Self {
        self.channels = Some(n);
        self
    }

    /// Builder method: set the channel layout. Mirrors
    /// [`Self::channels`]; setting one does not auto-fill the other —
    /// use [`Self::resolved_layout`] / [`Self::resolved_channels`] at
    /// read time to bridge the two.
    pub fn channel_layout(mut self, layout: ChannelLayout) -> Self {
        self.channel_layout = Some(layout);
        self
    }

    /// Best-effort layout: prefers an explicit [`Self::channel_layout`]
    /// when set, otherwise infers one from [`Self::channels`] via
    /// [`ChannelLayout::from_count`]. Returns `None` only when neither
    /// field is populated (e.g. video / data streams, or audio params
    /// surfaced before the codec has been opened).
    ///
    /// This is the canonical call-site for resolving a stream's
    /// channel layout — frames do *not* carry layout, so audio
    /// consumers (downmix, device routing, channel-aware filters)
    /// should read it from the stream's `CodecParameters` once and
    /// pass it down with the frame.
    pub fn resolved_layout(&self) -> Option<ChannelLayout> {
        self.channel_layout
            .or_else(|| self.channels.map(ChannelLayout::from_count))
    }

    /// Best-effort channel count: prefers an explicit
    /// [`Self::channels`] when set, otherwise reads the count off
    /// [`Self::channel_layout`]. Returns `None` only when neither
    /// field is populated.
    pub fn resolved_channels(&self) -> Option<u16> {
        self.channels
            .or_else(|| self.channel_layout.map(|l| l.channel_count()))
    }

    /// Fold a decoder's [`output_params`](crate::Decoder::output_params)
    /// answer into these (stream / container) parameters. The decoder
    /// produced the frames, so **it wins for every output-describing
    /// field it sets**; fields it leaves `None` / unspecified keep the
    /// container's value, and container-side fields are never touched.
    ///
    /// | field | rule |
    /// |---|---|
    /// | `width`, `height`, `pixel_format`, `frame_rate` | decoder's `Some` replaces |
    /// | `sample_rate`, `channels`, `sample_format`, `channel_layout` | decoder's `Some` replaces; a kept layout / count that disagrees with the decoder's count is dropped / re-derived so the pair stays consistent |
    /// | `color_signal` | field-wise: the decoder's specified parts over the container's ([`ColorSignal::or`]) |
    /// | `layers` | the decoder's list replaces when non-empty |
    /// | `codec_id`, `media_type`, `extradata`, `bit_rate`, `tag`, `language`, `blobs`, `options`, `limits`, `device_index` | kept |
    ///
    /// Returns `true` when any field of `self` changed — the signal a
    /// consumer uses to reconfigure (converter, renderer, muxer).
    ///
    /// ```
    /// # use oxideav_core::{CodecId, CodecParameters, PixelFormat};
    /// // What the container said: codec + size, no pixel format.
    /// let mut stream = CodecParameters::video(CodecId::new("h264"));
    /// stream.width = Some(1920);
    /// stream.height = Some(1088);
    /// stream.bit_rate = Some(8_000_000);
    /// // What the decoder resolved from the SPS (cropped size).
    /// let mut out = CodecParameters::video(CodecId::new("h264"));
    /// out.width = Some(1920);
    /// out.height = Some(1080);
    /// out.pixel_format = Some(PixelFormat::Yuv420P);
    /// assert!(stream.merge_output_params(&out));
    /// assert_eq!(stream.height, Some(1080));
    /// assert_eq!(stream.pixel_format, Some(PixelFormat::Yuv420P));
    /// assert_eq!(stream.bit_rate, Some(8_000_000));
    /// assert!(!stream.merge_output_params(&out)); // idempotent
    /// ```
    pub fn merge_output_params(&mut self, decoded: &CodecParameters) -> bool {
        fn take<T: Copy + PartialEq>(dst: &mut Option<T>, src: Option<T>) -> bool {
            match src {
                Some(v) if *dst != Some(v) => {
                    *dst = Some(v);
                    true
                }
                _ => false,
            }
        }
        let mut changed = false;
        changed |= take(&mut self.width, decoded.width);
        changed |= take(&mut self.height, decoded.height);
        changed |= take(&mut self.pixel_format, decoded.pixel_format);
        changed |= take(&mut self.frame_rate, decoded.frame_rate);
        changed |= take(&mut self.sample_rate, decoded.sample_rate);
        changed |= take(&mut self.sample_format, decoded.sample_format);
        changed |= take(&mut self.channels, decoded.channels);
        changed |= take(&mut self.channel_layout, decoded.channel_layout);
        // Keep the count / layout pair consistent when the decoder
        // stated only one of them.
        match (decoded.channels, decoded.channel_layout) {
            (Some(n), None) => {
                if self.channel_layout.is_some_and(|l| l.channel_count() != n) {
                    self.channel_layout = None;
                    changed = true;
                }
            }
            (None, Some(l)) => {
                let n = l.channel_count();
                if self.channels.is_some_and(|c| c != n) {
                    self.channels = Some(n);
                    changed = true;
                }
            }
            _ => {}
        }
        let signal = decoded.color_signal.or(self.color_signal);
        if signal != self.color_signal {
            self.color_signal = signal;
            changed = true;
        }
        if !decoded.layers.is_empty() && decoded.layers != self.layers {
            self.layers = decoded.layers.clone();
            changed = true;
        }
        changed
    }

    /// Read-only access to the DoS-protection caps for any decoder
    /// constructed from these parameters. See [`DecoderLimits`].
    pub fn limits(&self) -> &DecoderLimits {
        &self.limits
    }

    /// Builder method: replace the [`DecoderLimits`] for these
    /// parameters. Use to tighten caps before passing parameters into
    /// `make_decoder` (e.g. when processing untrusted uploads on a
    /// shared server).
    ///
    /// ```
    /// # use oxideav_core::{CodecId, CodecParameters, DecoderLimits};
    /// let limits = DecoderLimits::default()
    ///     .with_max_pixels_per_frame(4096 * 4096)
    ///     .with_max_arenas_in_flight(2);
    /// let p = CodecParameters::video(CodecId::new("h263")).with_limits(limits);
    /// assert_eq!(p.limits().max_pixels_per_frame, 4096 * 4096);
    /// ```
    pub fn with_limits(mut self, limits: DecoderLimits) -> Self {
        self.limits = limits;
        self
    }

    /// Bind subsequent decoder/encoder construction to a specific device.
    /// `index` matches the position in the `engine_probe` device list.
    ///
    /// Software codecs ignore this field. Hardware codecs read it as
    /// `params.device_index.unwrap_or(0)` to pick which physical engine
    /// to bind to.
    pub fn with_device_index(mut self, index: u32) -> Self {
        self.device_index = Some(index);
        self
    }

    /// Builder method: set the on-wire [`tag`](Self::tag).
    ///
    /// Demuxers call this from their stream-format parser so muxers
    /// re-emitting the stream preserve the original FourCC / wFormatTag
    /// byte-for-byte. Encoders call this in `output_params()` to
    /// announce which wire tag they're producing.
    ///
    /// ```
    /// # use oxideav_core::{CodecId, CodecParameters, CodecTag};
    /// let p = CodecParameters::video(CodecId::new("magicyuv"))
    ///     .with_tag(CodecTag::fourcc(b"M8RG"));
    /// assert_eq!(p.tag, Some(CodecTag::fourcc(b"M8RG")));
    /// ```
    pub fn with_tag(mut self, tag: CodecTag) -> Self {
        self.tag = Some(tag);
        self
    }

    /// Builder method: set the per-stream [`language`](Self::language)
    /// tag. Accepts any string — BCP-47 short codes (`"en"`), ISO
    /// 639-2/T three-letter codes (`"jpn"`), or container-native
    /// values are all passed through verbatim. No validation is
    /// performed; the muxer writes whatever the caller hands in.
    ///
    /// ```
    /// # use oxideav_core::{CodecId, CodecParameters};
    /// let p = CodecParameters::audio(CodecId::new("aac")).with_language("jpn");
    /// assert_eq!(p.language.as_deref(), Some("jpn"));
    /// ```
    pub fn with_language(mut self, language: impl Into<String>) -> Self {
        self.language = Some(language.into());
        self
    }

    /// Builder method: set the stream's
    /// [`color_signal`](Self::color_signal).
    ///
    /// ```
    /// # use oxideav_core::{CodecId, CodecParameters, ColorRange, ColorSignal, PixelFormat};
    /// // A 10-bit full-range BT.2020 PQ stream, as a container's colour
    /// // record describes it (primaries 9, transfer 16, matrix 9).
    /// let mut p = CodecParameters::video(CodecId::new("h265"))
    ///     .with_color_signal(ColorSignal::from_code_points(9, 16, 9, true));
    /// p.pixel_format = Some(PixelFormat::Yuv420P10Le);
    /// assert_eq!(p.resolved_color_range(), ColorRange::Full);
    /// ```
    pub fn with_color_signal(mut self, signal: ColorSignal) -> Self {
        self.color_signal = signal;
        self
    }

    /// Builder method: set only the sample range of the stream's
    /// [`color_signal`](Self::color_signal), leaving the H.273 triple
    /// as it was. For producers that know the range (an alpha plane is
    /// full range; a container flag said so) but nothing else.
    pub fn with_color_range(mut self, range: ColorRange) -> Self {
        self.color_signal.range = range;
        self
    }

    /// The stream's effective sample range: the explicit
    /// [`color_signal`](Self::color_signal) range when specified,
    /// otherwise the range implied by the
    /// [`pixel_format`](Self::pixel_format) label
    /// ([`PixelFormat::implied_color_range`] — `Full` for the `YuvJ*`
    /// formats), otherwise [`ColorRange::Unspecified`]. Converters
    /// apply their own policy to `Unspecified` (H.273 suggests limited
    /// for video imagery).
    pub fn resolved_color_range(&self) -> ColorRange {
        if !self.color_signal.range.is_unspecified() {
            return self.color_signal.range;
        }
        self.pixel_format
            .and_then(|f| f.implied_color_range())
            .unwrap_or(ColorRange::Unspecified)
    }

    /// Builder method: replace the stream's [`layers`](Self::layers)
    /// description.
    ///
    /// ```
    /// # use oxideav_core::{CodecId, CodecParameters, LayerInfo};
    /// // A stereo pair: base layer = left view, layer 1 = right view
    /// // predicted from the base.
    /// let p = CodecParameters::video(CodecId::new("h265")).with_layers(vec![
    ///     LayerInfo::new(0).with_view_id(0),
    ///     LayerInfo::new(1).with_view_id(1).with_depends_on([0u16]),
    /// ]);
    /// assert_eq!(p.layers.len(), 2);
    /// assert_eq!(p.layer(1).and_then(|l| l.view_id), Some(1));
    /// assert!(p.is_multi_layer());
    /// ```
    pub fn with_layers(mut self, layers: Vec<LayerInfo>) -> Self {
        self.layers = layers;
        self
    }

    /// The description of layer `layer_id`, when
    /// [`layers`](Self::layers) lists it.
    pub fn layer(&self, layer_id: u16) -> Option<&LayerInfo> {
        self.layers.iter().find(|l| l.layer_id == layer_id)
    }

    /// `true` when [`layers`](Self::layers) describes more than one
    /// layer. Single-layer streams (and streams whose structure is not
    /// described) report `false`.
    pub fn is_multi_layer(&self) -> bool {
        self.layers.len() > 1
    }

    /// The stream's attached metadata [`blobs`](Self::blobs), in
    /// producer order.
    pub fn blobs(&self) -> &[MetadataBlob] {
        &self.blobs
    }

    /// The first attached blob of `kind`, if any. For kinds that may
    /// legitimately repeat (several `cover-art` pictures) iterate
    /// [`blobs_of`](Self::blobs_of) instead.
    ///
    /// ```
    /// # use oxideav_core::{BlobKind, CodecId, CodecParameters, MetadataBlob};
    /// let p = CodecParameters::video(CodecId::new("png"))
    ///     .with_blob(MetadataBlob::new(BlobKind::ICC, vec![0u8; 128]));
    /// assert_eq!(p.blob(&BlobKind::ICC).map(|b| b.data.len()), Some(128));
    /// assert!(p.blob(&BlobKind::EXIF).is_none());
    /// ```
    pub fn blob(&self, kind: &BlobKind) -> Option<&MetadataBlob> {
        self.blobs.iter().find(|b| b.is(kind))
    }

    /// Every attached blob of `kind`, in producer order. Takes the kind
    /// by value so `p.blobs_of(BlobKind::COVER_ART)` borrows nothing
    /// but `self` (the constants are allocation-free).
    pub fn blobs_of(&self, kind: BlobKind) -> impl Iterator<Item = &MetadataBlob> + '_ {
        self.blobs.iter().filter(move |b| b.is(&kind))
    }

    /// Append a metadata blob. Blobs of the same kind are kept in
    /// insertion order; nothing is replaced.
    pub fn push_blob(&mut self, blob: MetadataBlob) {
        self.blobs.push(blob);
    }

    /// Builder method: append a metadata blob (see
    /// [`push_blob`](Self::push_blob)).
    pub fn with_blob(mut self, blob: MetadataBlob) -> Self {
        self.blobs.push(blob);
        self
    }

    /// Builder method: replace the whole [`blobs`](Self::blobs) list.
    pub fn with_blobs(mut self, blobs: Vec<MetadataBlob>) -> Self {
        self.blobs = blobs;
        self
    }

    /// Detach and return every attached blob, leaving the list empty.
    pub fn take_blobs(&mut self) -> Vec<MetadataBlob> {
        std::mem::take(&mut self.blobs)
    }
}

/// Description of a single stream inside a container.
#[derive(Clone, Debug)]
pub struct StreamInfo {
    /// 0-based index of the stream within its container.
    pub index: u32,
    /// Time base in which this stream's packet timestamps (and
    /// `duration` / `start_time` below) are expressed.
    pub time_base: TimeBase,
    /// Stream duration in `time_base` units, when the container
    /// declares one.
    pub duration: Option<i64>,
    /// Presentation timestamp of the first packet, in `time_base`
    /// units, when known.
    pub start_time: Option<i64>,
    /// Codec-level parameters (codec id, format, extradata, ...).
    pub params: CodecParameters,
}

#[cfg(test)]
mod codec_tag_tests {
    use super::*;

    #[test]
    fn fourcc_uppercases_on_construction() {
        let t = CodecTag::fourcc(b"div3");
        assert_eq!(t, CodecTag::Fourcc(*b"DIV3"));
        // Non-alphabetic bytes preserved unchanged.
        let t2 = CodecTag::fourcc(b"MP42");
        assert_eq!(t2, CodecTag::Fourcc(*b"MP42"));
        let t3 = CodecTag::fourcc(&[0xFF, b'a', 0x00, b'1']);
        assert_eq!(t3, CodecTag::Fourcc([0xFF, b'A', 0x00, b'1']));
    }

    #[test]
    fn fourcc_equality_case_insensitive_via_ctor() {
        assert_eq!(CodecTag::fourcc(b"xvid"), CodecTag::fourcc(b"XVID"));
        assert_eq!(CodecTag::fourcc(b"DiV3"), CodecTag::fourcc(b"div3"));
    }

    #[test]
    fn display_printable_fourcc() {
        assert_eq!(CodecTag::fourcc(b"XVID").to_string(), "fourcc(XVID)");
    }

    #[test]
    fn display_non_printable_fourcc_as_hex() {
        let t = CodecTag::Fourcc([0x00, 0x00, 0x00, 0x01]);
        assert_eq!(t.to_string(), "fourcc(0x00000001)");
    }

    #[test]
    fn display_wave_format() {
        assert_eq!(
            CodecTag::wave_format(0x0055).to_string(),
            "wFormatTag(0x0055)"
        );
    }

    #[test]
    fn display_mp4_oti() {
        assert_eq!(CodecTag::mp4_object_type(0x40).to_string(), "mp4_oti(0x40)");
    }

    #[test]
    fn display_matroska() {
        assert_eq!(
            CodecTag::matroska("V_MPEG4/ISO/AVC").to_string(),
            "matroska(V_MPEG4/ISO/AVC)",
        );
    }

    #[test]
    fn null_resolver_resolves_nothing() {
        let r = NullCodecResolver;
        let xvid = CodecTag::fourcc(b"XVID");
        assert!(r.resolve_tag(&ProbeContext::new(&xvid)).is_none());
        let wf = CodecTag::wave_format(0x0055);
        assert!(r.resolve_tag(&ProbeContext::new(&wf)).is_none());
    }

    #[test]
    fn probe_context_builder_fills_hints() {
        let tag = CodecTag::wave_format(0x0001);
        let ctx = ProbeContext::new(&tag)
            .bits(24)
            .channels(2)
            .sample_rate(48_000)
            .header(&[1, 2, 3])
            .packet(&[4, 5]);
        assert_eq!(ctx.bits_per_sample, Some(24));
        assert_eq!(ctx.channels, Some(2));
        assert_eq!(ctx.sample_rate, Some(48_000));
        assert_eq!(ctx.header.unwrap(), &[1, 2, 3]);
        assert_eq!(ctx.packet.unwrap(), &[4, 5]);
    }
}

#[cfg(test)]
mod channel_layout_plumbing_tests {
    use super::*;

    #[test]
    fn audio_params_default_to_no_layout() {
        let p = CodecParameters::audio(CodecId::new("pcm_s16le"));
        assert!(p.channel_layout.is_none());
        assert!(p.channels.is_none());
        assert!(p.resolved_layout().is_none());
        assert!(p.resolved_channels().is_none());
    }

    #[test]
    fn channels_only_infers_layout_via_from_count() {
        let p = CodecParameters::audio(CodecId::new("pcm_s16le")).channels(6);
        assert_eq!(p.channels, Some(6));
        assert!(p.channel_layout.is_none());
        assert_eq!(p.resolved_layout(), Some(ChannelLayout::Surround51));
        assert_eq!(p.resolved_channels(), Some(6));
    }

    #[test]
    fn explicit_layout_wins_over_count() {
        let p = CodecParameters::audio(CodecId::new("ac3"))
            .channels(6)
            .channel_layout(ChannelLayout::Surround60);
        // 6ch by-count would default to Surround51, but the explicit
        // layout overrides.
        assert_eq!(p.resolved_layout(), Some(ChannelLayout::Surround60));
        assert_eq!(p.resolved_channels(), Some(6));
    }

    #[test]
    fn layout_only_yields_count_via_resolved_channels() {
        let p =
            CodecParameters::audio(CodecId::new("ac3")).channel_layout(ChannelLayout::Surround71);
        assert!(p.channels.is_none());
        assert_eq!(p.resolved_channels(), Some(8));
        assert_eq!(p.resolved_layout(), Some(ChannelLayout::Surround71));
    }
}

#[cfg(test)]
mod codec_parameters_device_index_tests {
    use super::*;

    #[test]
    fn codec_parameters_device_index_defaults_to_none() {
        assert!(
            CodecParameters::audio(CodecId::new("pcm_s16le"))
                .device_index
                .is_none()
        );
        assert!(
            CodecParameters::video(CodecId::new("h264"))
                .device_index
                .is_none()
        );
        assert!(
            CodecParameters::subtitle(CodecId::new("srt"))
                .device_index
                .is_none()
        );
        assert!(
            CodecParameters::data(CodecId::new("bin"))
                .device_index
                .is_none()
        );
    }

    #[test]
    fn codec_parameters_with_device_index_sets_field() {
        let p = CodecParameters::video(CodecId::new("h264")).with_device_index(2);
        assert_eq!(p.device_index, Some(2));
    }
}

#[cfg(test)]
mod codec_parameters_tag_tests {
    use super::*;

    #[test]
    fn tag_defaults_to_none_on_every_constructor() {
        assert!(CodecParameters::audio(CodecId::new("aac")).tag.is_none());
        assert!(CodecParameters::video(CodecId::new("h264")).tag.is_none());
        assert!(CodecParameters::subtitle(CodecId::new("srt")).tag.is_none());
        assert!(CodecParameters::data(CodecId::new("bin")).tag.is_none());
    }

    #[test]
    fn with_tag_builder_sets_field() {
        let p =
            CodecParameters::video(CodecId::new("magicyuv")).with_tag(CodecTag::fourcc(b"M8RG"));
        assert_eq!(p.tag, Some(CodecTag::fourcc(b"M8RG")));
    }

    #[test]
    fn with_tag_round_trip_preserves_demuxed_fourcc() {
        // The canonical use-case: a demuxer sees DIVX in the bitstream
        // and tags the params accordingly. The mpeg4video codec also
        // claims XVID / MP4V / FMP4, but the muxer must re-emit DIVX.
        let demuxed =
            CodecParameters::video(CodecId::new("mpeg4video")).with_tag(CodecTag::fourcc(b"DIVX"));
        // Muxer reads `params.tag` directly — no registry round-trip.
        assert_eq!(demuxed.tag, Some(CodecTag::fourcc(b"DIVX")));
    }

    #[test]
    fn wave_format_tag_preserved() {
        let p = CodecParameters::audio(CodecId::new("mp3")).with_tag(CodecTag::wave_format(0x0055));
        assert_eq!(p.tag, Some(CodecTag::WaveFormat(0x0055)));
    }
}

#[cfg(test)]
mod codec_parameters_language_tests {
    use super::*;

    #[test]
    fn language_defaults_to_none_on_every_constructor() {
        assert!(
            CodecParameters::audio(CodecId::new("aac"))
                .language
                .is_none()
        );
        assert!(
            CodecParameters::video(CodecId::new("h264"))
                .language
                .is_none()
        );
        assert!(
            CodecParameters::subtitle(CodecId::new("srt"))
                .language
                .is_none()
        );
        assert!(
            CodecParameters::data(CodecId::new("bin"))
                .language
                .is_none()
        );
    }

    #[test]
    fn with_language_round_trips_value() {
        let p = CodecParameters::audio(CodecId::new("aac")).with_language("jpn");
        assert_eq!(p.language.as_deref(), Some("jpn"));
    }

    #[test]
    fn with_language_accepts_bcp47_short_code() {
        let p = CodecParameters::audio(CodecId::new("aac")).with_language("en");
        assert_eq!(p.language.as_deref(), Some("en"));
    }

    #[test]
    fn with_language_accepts_owned_string() {
        let tag = String::from("fre");
        let p = CodecParameters::audio(CodecId::new("aac")).with_language(tag);
        assert_eq!(p.language.as_deref(), Some("fre"));
    }
}

#[cfg(test)]
mod codec_parameters_color_signal_tests {
    use super::*;
    use crate::signal::{ColorPrimaries, MatrixCoefficients, TransferCharacteristics};

    #[test]
    fn color_signal_defaults_to_unspecified_on_every_constructor() {
        for p in [
            CodecParameters::audio(CodecId::new("aac")),
            CodecParameters::video(CodecId::new("h264")),
            CodecParameters::subtitle(CodecId::new("srt")),
            CodecParameters::data(CodecId::new("bin")),
        ] {
            assert!(p.color_signal.is_unspecified());
            assert_eq!(p.color_signal, ColorSignal::default());
            assert_eq!(p.resolved_color_range(), ColorRange::Unspecified);
            assert!(p.layers.is_empty());
            assert!(!p.is_multi_layer());
            assert!(p.layer(0).is_none());
        }
    }

    #[test]
    fn with_color_signal_sets_field_and_survives_clone() {
        let sig = ColorSignal::from_code_points(9, 16, 9, false);
        let p = CodecParameters::video(CodecId::new("h265")).with_color_signal(sig);
        assert_eq!(p.color_signal, sig);
        assert_eq!(p.resolved_color_range(), ColorRange::Limited);
        let c = p.clone();
        assert_eq!(c.color_signal, sig);
    }

    #[test]
    fn with_color_range_touches_only_the_range() {
        let p = CodecParameters::video(CodecId::new("h265"))
            .with_color_signal(ColorSignal::bt709_limited())
            .with_color_range(ColorRange::Full);
        assert_eq!(p.color_signal.range, ColorRange::Full);
        assert_eq!(p.color_signal.primaries, ColorPrimaries::BT709);
        assert_eq!(p.color_signal.transfer, TransferCharacteristics::BT709);
        assert_eq!(p.color_signal.matrix, MatrixCoefficients::BT709);
    }

    #[test]
    fn resolved_range_prefers_explicit_signal_over_pixel_format_label() {
        // Label only: YuvJ ⇒ Full.
        let mut p = CodecParameters::video(CodecId::new("mjpeg"));
        p.pixel_format = Some(PixelFormat::YuvJ420P);
        assert_eq!(p.resolved_color_range(), ColorRange::Full);
        // Limited label with no signal ⇒ unspecified (not a guess).
        p.pixel_format = Some(PixelFormat::Yuv420P);
        assert_eq!(p.resolved_color_range(), ColorRange::Unspecified);
        // 10-bit full-range signal on a non-J label ⇒ Full — the case
        // the labels alone cannot express.
        p.pixel_format = Some(PixelFormat::Yuv420P10Le);
        p.color_signal.range = ColorRange::Full;
        assert_eq!(p.resolved_color_range(), ColorRange::Full);
        // An explicit Limited signal wins even over a J label.
        p.pixel_format = Some(PixelFormat::YuvJ444P);
        p.color_signal.range = ColorRange::Limited;
        assert_eq!(p.resolved_color_range(), ColorRange::Limited);
        // Alpha as Gray8 with an explicit full range.
        p.pixel_format = Some(PixelFormat::Gray8);
        p.color_signal.range = ColorRange::Full;
        assert_eq!(p.resolved_color_range(), ColorRange::Full);
    }

    #[test]
    fn matches_core_ignores_color_signal_and_layers() {
        let a = CodecParameters::video(CodecId::new("h265"));
        let b = a
            .clone()
            .with_color_signal(ColorSignal::srgb())
            .with_layers(vec![LayerInfo::new(0), LayerInfo::new(1)]);
        assert!(a.matches_core(&b));
        assert!(b.matches_core(&a));
    }

    #[test]
    fn matches_core_ignores_blobs() {
        let a = CodecParameters::video(CodecId::new("png"));
        let b = a
            .clone()
            .with_blob(MetadataBlob::new(BlobKind::ICC, vec![1, 2, 3]));
        assert!(a.matches_core(&b));
        assert!(b.matches_core(&a));
        assert!(a.blobs().is_empty());
        assert_eq!(b.blobs().len(), 1);
    }

    #[test]
    fn blobs_accessors_push_lookup_iterate_take() {
        let mut p = CodecParameters::audio(CodecId::new("mp3"));
        assert!(p.blobs().is_empty());
        assert!(p.blob(&BlobKind::COVER_ART).is_none());
        assert_eq!(p.blobs_of(BlobKind::COVER_ART).count(), 0);

        p.push_blob(
            MetadataBlob::new(BlobKind::COVER_ART, vec![0xFF, 0xD8]).with_mime("image/jpeg"),
        );
        p.push_blob(MetadataBlob::new(
            BlobKind::custom("id3-text"),
            b"TIT2".to_vec(),
        ));
        p.push_blob(
            MetadataBlob::new(BlobKind::COVER_ART, vec![0x89, b'P']).with_mime("image/png"),
        );

        // First of kind, in insertion order; every one of kind.
        assert_eq!(
            p.blob(&BlobKind::COVER_ART).and_then(|b| b.mime.as_deref()),
            Some("image/jpeg")
        );
        let covers: Vec<&str> = p
            .blobs_of(BlobKind::COVER_ART)
            .filter_map(|b| b.mime.as_deref())
            .collect();
        assert_eq!(covers, ["image/jpeg", "image/png"]);
        assert_eq!(
            p.blob(&BlobKind::custom("id3-text"))
                .map(|b| b.data.as_slice()),
            Some(&b"TIT2"[..])
        );
        assert!(p.blob(&BlobKind::ICC).is_none());
        assert_eq!(p.blobs().len(), 3);

        // Field and accessor agree; clone carries the list.
        assert_eq!(p.blobs, p.blobs().to_vec());
        assert_eq!(p.clone().blobs().len(), 3);

        // take empties; with_blobs replaces wholesale.
        let taken = p.take_blobs();
        assert_eq!(taken.len(), 3);
        assert!(p.blobs().is_empty());
        let q = p.with_blobs(vec![MetadataBlob::new(BlobKind::XMP, b"<x/>".to_vec())]);
        assert_eq!(q.blobs().len(), 1);
        assert!(q.blob(&BlobKind::XMP).is_some());
    }

    #[test]
    fn every_constructor_starts_without_blobs() {
        let id = CodecId::new("x");
        for p in [
            CodecParameters::audio(id.clone()),
            CodecParameters::video(id.clone()),
            CodecParameters::subtitle(id.clone()),
            CodecParameters::data(id),
        ] {
            assert!(p.blobs().is_empty());
        }
    }

    /// The same API carries an ICC profile on an image stream and cover
    /// art on an audio stream: nothing in the mechanism is specific to
    /// either medium or to any container.
    #[test]
    fn blobs_are_media_agnostic() {
        let icc = vec![0u8; 16];
        let jpeg = vec![0xFF, 0xD8, 0xFF, 0xE0];

        let mut image = CodecParameters::video(CodecId::new("png"));
        image.push_blob(MetadataBlob::new(BlobKind::ICC, icc.clone()));
        let mut audio = CodecParameters::audio(CodecId::new("flac"));
        audio.push_blob(
            MetadataBlob::new(BlobKind::COVER_ART, jpeg.clone()).with_mime("image/jpeg"),
        );

        // One generic routine serves both streams.
        fn first_payload<'a>(p: &'a CodecParameters, kind: &BlobKind) -> Option<&'a [u8]> {
            p.blob(kind).map(|b| b.data.as_slice())
        }
        assert_eq!(first_payload(&image, &BlobKind::ICC), Some(icc.as_slice()));
        assert_eq!(first_payload(&image, &BlobKind::COVER_ART), None);
        assert_eq!(
            first_payload(&audio, &BlobKind::COVER_ART),
            Some(jpeg.as_slice())
        );
        assert_eq!(first_payload(&audio, &BlobKind::ICC), None);

        // Moving a blob between streams of different media is a plain
        // value move — the blob knows nothing about its carrier.
        let moved = audio.take_blobs();
        let image = image.with_blobs(moved);
        assert_eq!(
            image
                .blob(&BlobKind::COVER_ART)
                .and_then(|b| b.mime.as_deref()),
            Some("image/jpeg")
        );
        assert!(image.blob(&BlobKind::ICC).is_none());
    }

    #[test]
    fn layers_builder_lookup_and_clone() {
        let p = CodecParameters::video(CodecId::new("h265")).with_layers(vec![
            LayerInfo::new(0).with_view_id(0),
            LayerInfo::new(1).with_view_id(1).with_depends_on([0u16]),
            LayerInfo::new(2).with_depends_on(vec![0, 1]),
        ]);
        assert!(p.is_multi_layer());
        assert_eq!(p.layer(0).map(|l| l.view_id), Some(Some(0)));
        assert_eq!(
            p.layer(1).map(|l| l.depends_on.as_slice()),
            Some(&[0u16][..])
        );
        assert_eq!(p.layer(2).map(|l| l.depends_on.len()), Some(2));
        assert!(p.layer(3).is_none());
        let c = p.clone();
        assert_eq!(c.layers, p.layers);
        // A single described layer is not "multi-layer".
        let single =
            CodecParameters::video(CodecId::new("h265")).with_layers(vec![LayerInfo::new(0)]);
        assert!(!single.is_multi_layer());
    }
}
