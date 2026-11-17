//! Uncompressed audio and video frames.

use crate::blob::{BlobKind, MetadataBlob, decode_blobs, encode_blobs};
use crate::layer::LayerIdentity;
use crate::signal::ColorSignal;
use crate::subtitle::SubtitleCue;
use crate::vector::VectorFrame;

/// A decoded chunk of uncompressed data: either audio samples, a video
/// picture, (for subtitle streams) a single styled cue, a vector frame —
/// or the in-band [`StreamParams`](Self::StreamParams) event announcing
/// a mid-stream change of the output shape.
///
/// Marked `#[non_exhaustive]` — consumers that match on variants must
/// include a wildcard arm. This lets the crate add new frame kinds (data
/// tracks, hap rops, …) without breaking downstream code.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum Frame {
    /// Uncompressed audio samples.
    Audio(AudioFrame),
    /// One uncompressed video picture.
    Video(VideoFrame),
    /// A single subtitle cue. Timing is carried inside the cue itself
    /// (`start_us`/`end_us`) so it's independent of container time bases,
    /// but the enclosing pipeline/muxer can still rescale via `pts` at
    /// the packet layer.
    Subtitle(SubtitleCue),
    /// A resolution-independent vector-graphics frame. Produced by
    /// vector-format decoders (`oxideav-svg`, the vector path of
    /// `oxideav-pdf`) and consumed by vector renderers / writers.
    /// See [`crate::vector`] for the full primitive set.
    Vector(VectorFrame),
    /// In-band announcement of a **mid-stream** change of the decoder's
    /// output shape: the new resolved stream parameters (the same
    /// description [`Decoder::output_params`](crate::Decoder::output_params)
    /// now answers). It carries no picture or samples.
    ///
    /// Frames stay lean — width / height / pixel format / sample format
    /// / rate are stated on the stream, not on every frame. The initial
    /// shape is announced by `output_params()` (no event for it); this
    /// event exists only so a consumer pulling frames sees a later change
    /// in the frame stream itself, at the exact position it takes effect.
    ///
    /// The contract (for decoders that emit it — a decoder whose output
    /// shape cannot change never does):
    ///
    /// * It is returned by [`Decoder::receive_frame`](crate::Decoder::receive_frame)
    ///   only for a change **after the first frame** has been returned,
    ///   **immediately before** the first `Video` / `Audio` frame of the
    ///   new shape. The next media frame returned has the new shape; every
    ///   frame returned before it had the old one.
    /// * When it is returned, `output_params()` already describes the new
    ///   shape and [`output_params_generation`](crate::Decoder::output_params_generation)
    ///   has already been bumped — the counter and the event agree, so a
    ///   consumer watching either sees exactly one change.
    /// * At most one event per change; never one whose parameters equal
    ///   the current shape.
    /// * After [`reset`](crate::Decoder::reset) (a seek) the same rule
    ///   applies relative to the shape of the last frame returned before
    ///   the reset; draining after [`flush`](crate::Decoder::flush) keeps
    ///   the ordering (queued old-shape frames, then the event, then the
    ///   new-shape frames).
    ///
    /// Consumers fold it into the stream's parameters with
    /// [`OutputParamsWatch::observe_frame`](crate::OutputParamsWatch::observe_frame)
    /// (or [`CodecParameters::merge_output_params`](crate::CodecParameters::merge_output_params))
    /// and re-plan converters / renderers. A filter receiving it re-plans
    /// for the new input shape (forwarding its own `StreamParams` when its
    /// output shape changes as a result) or rejects it with
    /// [`Error::Unsupported`](crate::Error::Unsupported) when it cannot
    /// adapt. An encoder's [`send_frame`](crate::Encoder::send_frame)
    /// returns [`Error::Unsupported`](crate::Error::Unsupported) unless it
    /// can re-configure for the new shape mid-stream.
    ///
    /// Boxed so the variant does not grow `Frame` (a [`CodecParameters`](crate::CodecParameters)
    /// is larger than every media variant).
    StreamParams(Box<crate::CodecParameters>),
}

impl Frame {
    /// Presentation timestamp of the frame in its stream's time base
    /// (a subtitle cue reports its `start_us`); `None` if unknown.
    pub fn pts(&self) -> Option<i64> {
        match self {
            Self::Audio(a) => a.pts,
            Self::Video(v) => v.pts,
            Self::Subtitle(s) => Some(s.start_us),
            Self::Vector(v) => v.pts,
            Self::StreamParams(_) => None,
        }
    }

    /// The new stream parameters when this is a
    /// [`StreamParams`](Self::StreamParams) event, `None` for a media
    /// frame.
    pub fn stream_params(&self) -> Option<&crate::CodecParameters> {
        match self {
            Self::StreamParams(p) => Some(p),
            _ => None,
        }
    }

    /// `true` for a [`StreamParams`](Self::StreamParams) event — an
    /// in-band announcement that carries no picture, samples, cue or
    /// vector content.
    pub fn is_stream_params(&self) -> bool {
        matches!(self, Self::StreamParams(_))
    }
}

/// Uncompressed audio frame.
///
/// Stream-level properties (sample format, channel count, sample rate,
/// time base) are NOT carried per-frame — read them from the stream's
/// [`CodecParameters`](crate::CodecParameters). Frames stay lightweight
/// because real-time playback moves thousands per second per stream.
///
/// This is deliberate, not an omission: a property that does not change
/// from one frame to the next is stated once, on the stream, instead of
/// being repeated on every frame. The decoder states what it actually
/// emits through [`Decoder::output_params`](crate::Decoder::output_params)
/// (sample rate, channels, sample format, layout — authoritative over
/// the container's claim), and signals a mid-stream change (a chained
/// stream switching sample rate) through
/// [`Decoder::output_params_generation`](crate::Decoder::output_params_generation),
/// one integer compared after `receive_frame`, and in-band through a
/// [`Frame::StreamParams`] event returned just before the first frame
/// of the new shape.
/// [`OutputParamsWatch`](crate::OutputParamsWatch) folds them into the
/// stream's parameters in one call per frame.
///
/// Sample layout is determined by the stream's `SampleFormat`:
/// - Interleaved formats: `data` has one plane; samples are stored as
///   `ch0 ch1 ... chN ch0 ch1 ... chN ...`.
/// - Planar formats: `data` has one plane per channel.
///
/// Use [`SampleFormat::plane_count`](crate::SampleFormat::plane_count)
/// with the stream's channel count to compute the expected `data.len()`.
#[derive(Clone, Debug)]
pub struct AudioFrame {
    /// Number of samples *per channel* in this frame. Variable per-frame
    /// for VBR codecs and on partial flushes.
    pub samples: u32,
    /// Presentation timestamp in the stream's time base; `None` if unknown.
    pub pts: Option<i64>,
    /// Raw sample bytes. Length matches `format.plane_count(channels)`
    /// from the stream's `CodecParameters`.
    pub data: Vec<Vec<u8>>,
}

/// Uncompressed video frame.
///
/// Stream-level properties (pixel format, width, height, time base) are
/// NOT carried per-frame — read them from the stream's
/// [`CodecParameters`](crate::CodecParameters). Frames stay lightweight
/// because real-time playback moves thousands per second per stream.
///
/// This is deliberate, not an omission: a property that does not change
/// 60 times a second is stated once, on the stream, instead of being
/// repeated on every frame. The decoder states what it actually emits
/// through [`Decoder::output_params`](crate::Decoder::output_params)
/// (the frames' width / height, pixel format, frame rate, colour signal
/// — authoritative over the container's claim), and signals a
/// mid-stream change (a resolution switch at a keyframe) through
/// [`Decoder::output_params_generation`](crate::Decoder::output_params_generation),
/// one integer compared after `receive_frame`, and in-band through a
/// [`Frame::StreamParams`] event returned just before the first frame
/// of the new shape.
/// [`OutputParamsWatch`](crate::OutputParamsWatch) folds them into the
/// stream's parameters in one call per frame.
///
/// # Side-channels
///
/// `VideoFrame` (like [`VideoPlane`]) is a fully-public struct built by
/// struct literal throughout the codec crates, so per-frame metadata
/// cannot be added as new fields without breaking every constructor.
/// Instead, optional metadata rides in-band as *side-channel* entries at
/// the tail of `planes`: [`VideoPlane`] values whose shape is impossible
/// for an image plane, which makes them unambiguous. Every record has
/// non-empty `data` and a `stride` that no image plane can have —
/// either `0` (an image plane's `data` is `stride × rows` long, so a
/// zero stride forces empty data) or a value above `isize::MAX`
/// (`stride × rows` with any non-zero row count would exceed what a
/// `Vec` can hold). The whole `stride > isize::MAX` band is reserved
/// for side-channel tags. Six record kinds exist, distinguished by
/// their `stride` tag:
///
/// - **Palette** — `stride == 0`. Carries the color table for
///   palette-indexed content
///   ([`PixelFormat::Pal8`](crate::PixelFormat::Pal8)); see
///   [`palette`](Self::palette) / [`set_palette`](Self::set_palette).
/// - **Palette alpha** — `stride == usize::MAX - 3`. Carries one
///   alpha byte per palette entry for tables with transparent or
///   translucent entries (a GIF transparent index, a PNG `tRNS` chunk
///   on a colour-type-3 image, TGA / BMP alpha palettes); entries the
///   record does not cover are opaque. Only meaningful next to a
///   palette record; see [`palette_alpha`](Self::palette_alpha) /
///   [`set_palette_alpha`](Self::set_palette_alpha) and the combined
///   [`palette_rgba`](Self::palette_rgba) /
///   [`set_palette_rgba`](Self::set_palette_rgba).
/// - **Per-plane significant bits** — `stride == usize::MAX`. Carries
///   mixed per-plane bit depths (e.g. 12-bit luma with 10-bit chroma
///   from a wavelet codec's custom signal range); see
///   [`significant_bits`](Self::significant_bits) /
///   [`set_significant_bits`](Self::set_significant_bits).
/// - **Colour signal** — `stride == usize::MAX - 1`. Carries a
///   [`ColorSignal`] (sample range + H.273 primaries / transfer /
///   matrix) for producers whose signal is per-picture or that have no
///   stream object to put it on; see
///   [`color_signal`](Self::color_signal) /
///   [`set_color_signal`](Self::set_color_signal).
/// - **Layer identity** — `stride == usize::MAX - 2`. Carries a
///   [`LayerIdentity`] (layer / view / access-unit) for frames of
///   multi-layer or multi-view streams; see [`layer`](Self::layer) /
///   [`set_layer`](Self::set_layer).
/// - **Metadata blobs** — `stride == usize::MAX - 4`. Carries a list
///   of [`MetadataBlob`]s (ICC profile, Exif, XMP, … — see
///   [`crate::blob`]) for pictures whose metadata is per-picture: a
///   multi-page TIFF's per-page Exif, HEIF burst items with their own
///   profile. Wire form [`encode_blobs`] / [`decode_blobs`]; see
///   [`blobs`](Self::blobs) / [`set_blobs`](Self::set_blobs). A blob
///   that applies to the whole stream belongs on
///   [`CodecParameters::blobs`](crate::CodecParameters::blobs) instead.
///
/// The records compose: a frame can carry any subset at once, in any
/// order, within the trailing run of side-channel-shaped entries. The
/// typed accessors find each record by its `stride` tag regardless of
/// order, and [`image_planes`](Self::image_planes) /
/// [`image_plane_count`](Self::image_plane_count) exclude the whole
/// trailing run. Frames without any attached side-channel are
/// byte-for-byte identical to what they always were.
///
/// Consumers that index `planes` directly (rather than through
/// [`image_planes`](Self::image_planes)) see the records as extra
/// trailing entries; producers attaching a record to every frame of a
/// stream should therefore prefer the stream-level home
/// ([`CodecParameters`](crate::CodecParameters)) for data that does not
/// vary per picture, and attach per-frame records when the value is
/// genuinely per-picture or no stream object exists.
#[derive(Clone, Debug)]
pub struct VideoFrame {
    /// Presentation timestamp in the stream's time base; `None` if unknown.
    pub pts: Option<i64>,
    /// One entry per plane (e.g., 3 for Yuv420P). Each entry is `(stride, bytes)`.
    ///
    /// May additionally end with side-channel entries (palette, palette
    /// alpha, per-plane significant bits, colour signal, layer identity,
    /// metadata blobs — see the type-level docs). Code that
    /// wants only pixel planes should iterate
    /// [`image_planes`](Self::image_planes) instead of this field.
    pub planes: Vec<VideoPlane>,
}

/// `stride` tag of the per-plane significant-bits side-channel record.
/// (The palette record's tag is `0`; see the [`VideoFrame`] docs.)
const SIGNIFICANT_BITS_STRIDE: usize = usize::MAX;

/// `stride` tag of the colour-signal side-channel record.
const COLOR_SIGNAL_STRIDE: usize = usize::MAX - 1;

/// `stride` tag of the layer-identity side-channel record.
const LAYER_IDENTITY_STRIDE: usize = usize::MAX - 2;

/// `stride` tag of the palette-alpha side-channel record.
const PALETTE_ALPHA_STRIDE: usize = usize::MAX - 3;

/// `stride` tag of the metadata-blobs side-channel record.
const BLOBS_STRIDE: usize = usize::MAX - 4;

/// Smallest `stride` value that is impossible for an image plane with
/// at least one row: `stride × rows` would exceed `isize::MAX`, the
/// largest allocation a `Vec` can hold. Every non-zero side-channel
/// tag lives at or above this value.
const SIDE_CHANNEL_STRIDE_FLOOR: usize = isize::MAX as usize + 1;

impl VideoFrame {
    /// `true` when `plane` has a side-channel record shape: non-empty
    /// data with an impossible-for-an-image-plane stride (`0`, or any
    /// value above `isize::MAX`) as described in the type-level docs.
    fn is_side_channel_entry(plane: &VideoPlane) -> bool {
        (plane.stride == 0 || plane.stride >= SIDE_CHANNEL_STRIDE_FLOOR) && !plane.data.is_empty()
    }

    /// Index of the first entry of the trailing side-channel run — equal
    /// to the number of image planes. Scans backwards from the tail
    /// while entries have a side-channel shape.
    fn side_channel_run_start(&self) -> usize {
        let mut start = self.planes.len();
        while start > 0 && Self::is_side_channel_entry(&self.planes[start - 1]) {
            start -= 1;
        }
        start
    }

    /// Index in `planes` of the side-channel record tagged with
    /// `stride_tag`, searching the trailing side-channel run only (the
    /// last match wins if a malformed frame carries duplicates).
    fn side_channel_index(&self, stride_tag: usize) -> Option<usize> {
        let start = self.side_channel_run_start();
        self.planes[start..]
            .iter()
            .rposition(|p| p.stride == stride_tag)
            .map(|i| start + i)
    }

    /// Remove every record tagged `stride_tag` from the trailing
    /// side-channel run, returning the data of the record the readers
    /// would have reported (the last match — consistent with
    /// [`side_channel_index`](Self::side_channel_index)).
    fn remove_side_channel(&mut self, stride_tag: usize) -> Option<Vec<u8>> {
        let reported = self
            .side_channel_index(stride_tag)
            .map(|i| self.planes.remove(i).data);
        while let Some(i) = self.side_channel_index(stride_tag) {
            self.planes.remove(i);
        }
        reported
    }

    /// The frame's attached palette, if any.
    ///
    /// Returns the raw bytes of the palette side-channel (see the
    /// type-level docs): packed 3-byte RGB entries, entry `i` at bytes
    /// `3*i .. 3*i + 3` in R, G, B order. A full
    /// [`Pal8`](crate::PixelFormat::Pal8) table is 256 entries
    /// (768 bytes), but producers may attach fewer when the source
    /// image declares a shorter table; indices at or beyond
    /// `len / 3` are undefined by this frame and up to the consumer's
    /// missing-entry policy (typically black).
    pub fn palette(&self) -> Option<&[u8]> {
        self.side_channel_index(0)
            .map(|i| self.planes[i].data.as_slice())
    }

    /// The RGB triplet for palette entry `index`, or `None` when no
    /// palette is attached or the attached table is too short to cover
    /// `index`. Sugar over [`palette`](Self::palette) for per-pixel
    /// lookups.
    pub fn palette_rgb(&self, index: u8) -> Option<[u8; 3]> {
        let pal = self.palette()?;
        let at = usize::from(index) * 3;
        let entry = pal.get(at..at + 3)?;
        Some([entry[0], entry[1], entry[2]])
    }

    /// Attach (or replace) the frame's palette side-channel.
    ///
    /// `rgb` is packed 3-byte RGB entries — see
    /// [`palette`](Self::palette) for the exact layout; pass a length
    /// that is a multiple of 3 (up to 768 bytes for a full 256-entry
    /// [`Pal8`](crate::PixelFormat::Pal8) table). The bytes are stored
    /// verbatim. An empty `rgb` removes any attached palette instead
    /// (the sentinel requires non-empty data), leaving the frame
    /// exactly as it was before any palette was attached.
    pub fn set_palette(&mut self, rgb: Vec<u8>) {
        self.remove_side_channel(0);
        if !rgb.is_empty() {
            self.planes.push(VideoPlane {
                stride: 0,
                data: rgb,
            });
        }
    }

    /// Builder-style counterpart to [`set_palette`](Self::set_palette)
    /// for construction chains:
    /// `VideoFrame { pts, planes }.with_palette(rgb)`.
    pub fn with_palette(mut self, rgb: Vec<u8>) -> Self {
        self.set_palette(rgb);
        self
    }

    /// Detach and return the frame's palette side-channel, if any.
    /// Afterwards the frame carries no palette (any other side-channel
    /// record is left in place — including a palette-alpha record,
    /// which then reads as `None` until a palette is attached again).
    pub fn take_palette(&mut self) -> Option<Vec<u8>> {
        self.remove_side_channel(0)
    }

    /// Number of entries the attached palette covers (`len / 3`), or
    /// `0` without a palette.
    fn palette_entry_count(&self) -> usize {
        self.palette().map_or(0, |p| p.len() / 3)
    }

    /// The frame's attached palette-alpha record, if any.
    ///
    /// Returns the raw bytes of the palette-alpha side-channel (see the
    /// type-level docs): byte `i` is the alpha of palette entry `i`
    /// (`0` transparent, `255` opaque), in the same entry order as
    /// [`palette`](Self::palette). The record may be shorter than the
    /// palette — a GIF with one transparent index needs only
    /// `index + 1` bytes — and every entry it does not cover is opaque.
    ///
    /// The record is only meaningful next to a palette: it reads as
    /// `None` when no palette is attached, and a malformed record
    /// **longer** than the palette's entry count also reads as `None`
    /// (consumers fall back to opaque). Validation happens at read
    /// time, so palette and alpha may be attached in either order.
    pub fn palette_alpha(&self) -> Option<&[u8]> {
        let i = self.side_channel_index(PALETTE_ALPHA_STRIDE)?;
        let alpha = self.planes[i].data.as_slice();
        let entries = self.palette_entry_count();
        (entries > 0 && alpha.len() <= entries).then_some(alpha)
    }

    /// The RGBA quadruplet for palette entry `index`: the RGB triplet
    /// from the palette record and the alpha from the palette-alpha
    /// record, or `255` (opaque) when no alpha record is attached, it
    /// does not cover `index`, or it is malformed. `None` when no
    /// palette is attached or the palette is too short to cover
    /// `index` — exactly when [`palette_rgb`](Self::palette_rgb) is
    /// `None`. The one lookup a `Pal8` → RGBA expander needs.
    pub fn palette_rgba(&self, index: u8) -> Option<[u8; 4]> {
        let [r, g, b] = self.palette_rgb(index)?;
        let a = self
            .palette_alpha()
            .and_then(|a| a.get(usize::from(index)).copied())
            .unwrap_or(u8::MAX);
        Some([r, g, b, a])
    }

    /// Attach (or replace) the frame's palette-alpha side-channel.
    ///
    /// `alpha` holds one byte per palette entry, in entry order — see
    /// [`palette_alpha`](Self::palette_alpha) for the semantics (may be
    /// shorter than the palette; uncovered entries are opaque). The
    /// bytes are stored verbatim; the length is checked against the
    /// palette when read, not here, so the two records may be attached
    /// in either order. An empty `alpha` removes any attached record
    /// instead (the sentinel requires non-empty data). Other
    /// side-channel records — the palette included — are unaffected.
    pub fn set_palette_alpha(&mut self, alpha: Vec<u8>) {
        self.remove_side_channel(PALETTE_ALPHA_STRIDE);
        if !alpha.is_empty() {
            self.planes.push(VideoPlane {
                stride: PALETTE_ALPHA_STRIDE,
                data: alpha,
            });
        }
    }

    /// Builder-style counterpart to
    /// [`set_palette_alpha`](Self::set_palette_alpha) for construction
    /// chains:
    /// `VideoFrame { pts, planes }.with_palette(rgb).with_palette_alpha(alpha)`.
    pub fn with_palette_alpha(mut self, alpha: Vec<u8>) -> Self {
        self.set_palette_alpha(alpha);
        self
    }

    /// Detach and return the frame's palette-alpha side-channel, if
    /// any — the bytes [`palette_alpha`](Self::palette_alpha) would
    /// have reported, so a record that read as `None` (no palette, or
    /// longer than the palette) is removed but returns `None`.
    /// Afterwards the frame carries no palette alpha (the palette and
    /// every other record are left in place).
    pub fn take_palette_alpha(&mut self) -> Option<Vec<u8>> {
        let valid = self.palette_alpha().is_some();
        self.remove_side_channel(PALETTE_ALPHA_STRIDE)
            .filter(|_| valid)
    }

    /// Attach (or replace) both palette records at once from RGBA
    /// entries: entry `i` of `rgba` becomes bytes `3*i .. 3*i + 3` of
    /// the palette record and byte `i` of the palette-alpha record.
    /// The alpha record is written even when every entry is opaque
    /// (stored verbatim, no trimming). An empty `rgba` removes both
    /// records.
    ///
    /// ```
    /// # use oxideav_core::{VideoFrame, VideoPlane};
    /// let mut f = VideoFrame { pts: None, planes: vec![VideoPlane { stride: 1, data: vec![1] }] };
    /// f.set_palette_rgba(&[[0, 0, 0, 0], [255, 255, 255, 255]]);
    /// assert_eq!(f.palette_rgba(0), Some([0, 0, 0, 0]));
    /// assert_eq!(f.palette_rgba(1), Some([255, 255, 255, 255]));
    /// assert_eq!(f.palette_rgba(2), None);
    /// assert_eq!(f.image_plane_count(), 1);
    /// ```
    pub fn set_palette_rgba(&mut self, rgba: &[[u8; 4]]) {
        let rgb = rgba.iter().flat_map(|e| [e[0], e[1], e[2]]).collect();
        let alpha = rgba.iter().map(|e| e[3]).collect();
        self.set_palette(rgb);
        self.set_palette_alpha(alpha);
    }

    /// Builder-style counterpart to
    /// [`set_palette_rgba`](Self::set_palette_rgba).
    pub fn with_palette_rgba(mut self, rgba: &[[u8; 4]]) -> Self {
        self.set_palette_rgba(rgba);
        self
    }

    /// The frame's attached per-plane significant-bits record, if any.
    ///
    /// Returns the raw bytes of the significant-bits side-channel (see
    /// the type-level docs): byte `k` is the number of significant bits
    /// in the samples of image plane `k`, in plane order. This lets a
    /// producer express **mixed** per-plane depths that no single
    /// [`PixelFormat`](crate::PixelFormat) variant can name — e.g. a
    /// wavelet codec's custom signal range with 12-bit luma and 10-bit
    /// chroma, stored on a `Yuv444P12Le` surface with an attached
    /// record of `[12, 10, 10]`.
    ///
    /// # Semantics
    ///
    /// - Values are **LSB-anchored**: a plane with `b` significant bits
    ///   keeps its sample values in the low `b` bits of each storage
    ///   word, with the upper bits zero — the same convention as this
    ///   crate's partial-depth formats (`Gray10Le`, `Yuv420P10Le`,
    ///   `Gbrp12Le`, …, each documented as "uses the low N bits of a
    ///   16-bit word"). Full-scale for `b` significant bits is
    ///   `(1 << b) - 1`.
    /// - Each value must satisfy `1 ≤ b ≤ 8 × storage-word-bytes` of
    ///   the frame's pixel format (so at most 8 for byte-sized planes,
    ///   16 for LE-16-bit-word planes). The record refines the storage
    ///   format's *significant* depth; it never changes the storage
    ///   word size or plane geometry.
    /// - A record shorter than the image-plane count (or a missing
    ///   record) leaves the uncovered planes at the pixel format's own
    ///   documented depth. Bytes are stored verbatim; out-of-range
    ///   values are a producer bug and consumers may clamp or reject
    ///   them.
    pub fn significant_bits(&self) -> Option<&[u8]> {
        self.side_channel_index(SIGNIFICANT_BITS_STRIDE)
            .map(|i| self.planes[i].data.as_slice())
    }

    /// The significant-bit count for image plane `plane`, or `None`
    /// when no record is attached or the attached record is too short
    /// to cover `plane` (fall back to the pixel format's own depth).
    /// Sugar over [`significant_bits`](Self::significant_bits) for
    /// per-plane lookups.
    pub fn plane_significant_bits(&self, plane: usize) -> Option<u8> {
        self.significant_bits()?.get(plane).copied()
    }

    /// Attach (or replace) the frame's per-plane significant-bits
    /// side-channel.
    ///
    /// `bits` holds one byte per image plane, in plane order — see
    /// [`significant_bits`](Self::significant_bits) for the exact
    /// semantics (LSB-anchored values, `1 ≤ b ≤ storage word bits`).
    /// The bytes are stored verbatim. An empty `bits` removes any
    /// attached record instead (the sentinel requires non-empty data),
    /// leaving the frame exactly as it was before any record was
    /// attached. Any attached palette is unaffected.
    pub fn set_significant_bits(&mut self, bits: Vec<u8>) {
        self.remove_side_channel(SIGNIFICANT_BITS_STRIDE);
        if !bits.is_empty() {
            self.planes.push(VideoPlane {
                stride: SIGNIFICANT_BITS_STRIDE,
                data: bits,
            });
        }
    }

    /// Builder-style counterpart to
    /// [`set_significant_bits`](Self::set_significant_bits) for
    /// construction chains:
    /// `VideoFrame { pts, planes }.with_significant_bits(bits)`.
    pub fn with_significant_bits(mut self, bits: Vec<u8>) -> Self {
        self.set_significant_bits(bits);
        self
    }

    /// Detach and return the frame's per-plane significant-bits
    /// side-channel, if any. Afterwards the frame carries no
    /// significant-bits record (any attached palette is left in place).
    pub fn take_significant_bits(&mut self) -> Option<Vec<u8>> {
        self.remove_side_channel(SIGNIFICANT_BITS_STRIDE)
    }

    /// The frame's attached colour-signal description, if any.
    ///
    /// Decoded from the colour-signal side-channel record (see the
    /// type-level docs; wire form in [`ColorSignal::to_bytes`]). A
    /// per-frame description refines the stream-level
    /// [`CodecParameters::color_signal`](crate::CodecParameters::color_signal):
    /// consumers resolve `frame.color_signal().unwrap_or_default()
    /// .or(params.color_signal)` and then apply their own policy to
    /// whatever is still unspecified. A malformed (too short) record
    /// reads as `None`.
    pub fn color_signal(&self) -> Option<ColorSignal> {
        self.side_channel_index(COLOR_SIGNAL_STRIDE)
            .and_then(|i| ColorSignal::from_bytes(&self.planes[i].data))
    }

    /// Attach (or replace) the frame's colour-signal side-channel.
    /// Other side-channel records are unaffected.
    pub fn set_color_signal(&mut self, signal: ColorSignal) {
        self.remove_side_channel(COLOR_SIGNAL_STRIDE);
        self.planes.push(VideoPlane {
            stride: COLOR_SIGNAL_STRIDE,
            data: signal.to_bytes().to_vec(),
        });
    }

    /// Builder-style counterpart to
    /// [`set_color_signal`](Self::set_color_signal) for construction
    /// chains: `VideoFrame { pts, planes }.with_color_signal(sig)`.
    pub fn with_color_signal(mut self, signal: ColorSignal) -> Self {
        self.set_color_signal(signal);
        self
    }

    /// Detach and return the frame's colour-signal side-channel, if
    /// any. Afterwards the frame carries no colour signal (other
    /// records are left in place).
    pub fn take_color_signal(&mut self) -> Option<ColorSignal> {
        self.remove_side_channel(COLOR_SIGNAL_STRIDE)
            .and_then(|d| ColorSignal::from_bytes(&d))
    }

    /// The frame's attached layer / view identity, if any.
    ///
    /// Decoded from the layer-identity side-channel record (see the
    /// type-level docs; wire form in [`LayerIdentity::to_bytes`]).
    /// Single-layer decoders attach nothing; consumers treat `None` as
    /// "base layer, no view". A malformed (too short) record reads as
    /// `None`.
    pub fn layer(&self) -> Option<LayerIdentity> {
        self.side_channel_index(LAYER_IDENTITY_STRIDE)
            .and_then(|i| LayerIdentity::from_bytes(&self.planes[i].data))
    }

    /// The frame's layer identity, or the base layer when none is
    /// attached. Sugar over [`layer`](Self::layer) for consumers that
    /// handle single- and multi-layer streams alike.
    pub fn layer_or_base(&self) -> LayerIdentity {
        self.layer().unwrap_or_default()
    }

    /// Attach (or replace) the frame's layer-identity side-channel.
    /// Other side-channel records are unaffected.
    pub fn set_layer(&mut self, layer: LayerIdentity) {
        self.remove_side_channel(LAYER_IDENTITY_STRIDE);
        self.planes.push(VideoPlane {
            stride: LAYER_IDENTITY_STRIDE,
            data: layer.to_bytes().to_vec(),
        });
    }

    /// Builder-style counterpart to [`set_layer`](Self::set_layer) for
    /// construction chains:
    /// `VideoFrame { pts, planes }.with_layer(LayerIdentity::new(1))`.
    pub fn with_layer(mut self, layer: LayerIdentity) -> Self {
        self.set_layer(layer);
        self
    }

    /// Detach and return the frame's layer-identity side-channel, if
    /// any. Afterwards the frame carries no layer identity (other
    /// records are left in place).
    pub fn take_layer(&mut self) -> Option<LayerIdentity> {
        self.remove_side_channel(LAYER_IDENTITY_STRIDE)
            .and_then(|d| LayerIdentity::from_bytes(&d))
    }

    /// The frame's attached metadata blobs, decoded from the blobs
    /// side-channel record (see the type-level docs; wire form in
    /// [`encode_blobs`]). Empty when no record is attached — and when
    /// the record is malformed, so a consumer never sees half a list.
    /// Decodes on every call: read it once per frame.
    ///
    /// A per-frame blob refines the stream-level
    /// [`CodecParameters::blobs`](crate::CodecParameters::blobs) of
    /// the same kind: resolve
    /// `frame.blob(&k).or_else(|| params.blob(&k).cloned())`.
    pub fn blobs(&self) -> Vec<MetadataBlob> {
        self.side_channel_index(BLOBS_STRIDE)
            .and_then(|i| decode_blobs(&self.planes[i].data))
            .unwrap_or_default()
    }

    /// The first attached blob of `kind`, if any (see
    /// [`blobs`](Self::blobs)).
    pub fn blob(&self, kind: &BlobKind) -> Option<MetadataBlob> {
        self.blobs().into_iter().find(|b| b.is(kind))
    }

    /// Attach (or replace) the frame's metadata-blobs side-channel
    /// with `blobs`, in the given order. An empty list removes any
    /// attached record instead (the sentinel requires non-empty data).
    /// Other side-channel records are unaffected.
    pub fn set_blobs(&mut self, blobs: Vec<MetadataBlob>) {
        self.remove_side_channel(BLOBS_STRIDE);
        if !blobs.is_empty() {
            self.planes.push(VideoPlane {
                stride: BLOBS_STRIDE,
                data: encode_blobs(&blobs),
            });
        }
    }

    /// Append one blob to the frame's metadata-blobs record (creating
    /// it when absent; a malformed existing record is replaced by the
    /// single new blob). Blobs of the same kind are kept in insertion
    /// order; nothing is replaced.
    pub fn push_blob(&mut self, blob: MetadataBlob) {
        let mut blobs = self.blobs();
        blobs.push(blob);
        self.set_blobs(blobs);
    }

    /// Builder-style counterpart to [`push_blob`](Self::push_blob) for
    /// construction chains:
    /// `VideoFrame { pts, planes }.with_blob(MetadataBlob::new(BlobKind::EXIF, exif))`.
    pub fn with_blob(mut self, blob: MetadataBlob) -> Self {
        self.push_blob(blob);
        self
    }

    /// Builder-style counterpart to [`set_blobs`](Self::set_blobs).
    pub fn with_blobs(mut self, blobs: Vec<MetadataBlob>) -> Self {
        self.set_blobs(blobs);
        self
    }

    /// Detach and return the frame's metadata blobs, if any (empty for
    /// no record or a malformed one — which is removed all the same).
    /// Afterwards the frame carries no blobs record (other records are
    /// left in place).
    pub fn take_blobs(&mut self) -> Vec<MetadataBlob> {
        self.remove_side_channel(BLOBS_STRIDE)
            .and_then(|d| decode_blobs(&d))
            .unwrap_or_default()
    }

    /// The frame's image planes — `planes` with the trailing
    /// side-channel entries (palette, palette alpha, significant bits,
    /// colour signal, layer identity, metadata blobs) excluded.
    /// Prefer this over indexing `planes` directly in code that
    /// handles side-channel-capable frames.
    pub fn image_planes(&self) -> &[VideoPlane] {
        &self.planes[..self.image_plane_count()]
    }

    /// Number of image planes (excludes every side-channel entry).
    /// Matches the stream pixel format's
    /// [`plane_count`](crate::PixelFormat::plane_count) for well-formed
    /// frames.
    pub fn image_plane_count(&self) -> usize {
        self.side_channel_run_start()
    }
}

/// One plane of a [`VideoFrame`]: row-major sample bytes plus the
/// stride between rows.
///
/// An entry with non-empty `data` and a `stride` of `0` or above
/// `isize::MAX` is not an image plane: it is a side-channel record
/// (palette, palette alpha, per-plane significant bits, colour signal,
/// layer identity, metadata blobs) described on [`VideoFrame`] — only
/// meaningful within the trailing
/// run of `VideoFrame::planes`.
#[derive(Clone, Debug)]
pub struct VideoPlane {
    /// Bytes per row in `data`.
    pub stride: usize,
    /// Raw plane bytes, `stride × rows` long (rows may carry padding
    /// beyond the visible width).
    pub data: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gray_frame() -> VideoFrame {
        // 4×2 Gray8 image plane.
        VideoFrame {
            pts: Some(7),
            planes: vec![VideoPlane {
                stride: 4,
                data: vec![0u8; 8],
            }],
        }
    }

    /// A full 256-entry table where entry i is (i, !i, i^0x55).
    fn full_palette() -> Vec<u8> {
        (0u16..256)
            .flat_map(|i| {
                let i = i as u8;
                [i, !i, i ^ 0x55]
            })
            .collect()
    }

    #[test]
    fn frame_without_palette_reports_none_and_full_image_planes() {
        let f = gray_frame();
        assert_eq!(f.palette(), None);
        assert_eq!(f.palette_rgb(0), None);
        assert_eq!(f.image_plane_count(), 1);
        assert_eq!(f.image_planes().len(), 1);
        assert_eq!(f.image_planes()[0].stride, 4);
    }

    #[test]
    fn set_palette_round_trips_and_keeps_image_planes_intact() {
        let mut f = gray_frame();
        let pal = full_palette();
        f.set_palette(pal.clone());

        assert_eq!(f.palette(), Some(pal.as_slice()));
        // Image-plane view is unchanged by the side-channel.
        assert_eq!(f.image_plane_count(), 1);
        assert_eq!(f.image_planes()[0].data.len(), 8);
        // The raw field sees the sentinel entry at the tail.
        assert_eq!(f.planes.len(), 2);
        assert_eq!(f.planes[1].stride, 0);

        // Entry lookup: entry i is (i, !i, i ^ 0x55) by construction.
        assert_eq!(f.palette_rgb(0), Some([0x00, 0xFF, 0x55]));
        assert_eq!(f.palette_rgb(0xAB), Some([0xAB, 0x54, 0xFE]));
        assert_eq!(f.palette_rgb(255), Some([0xFF, 0x00, 0xAA]));
    }

    #[test]
    fn set_palette_replaces_existing_table() {
        let mut f = gray_frame();
        f.set_palette(vec![1, 2, 3]);
        f.set_palette(vec![9, 8, 7, 6, 5, 4]);
        // Replacement, not stacking: one image plane + one sentinel.
        assert_eq!(f.planes.len(), 2);
        assert_eq!(f.palette(), Some(&[9, 8, 7, 6, 5, 4][..]));
        assert_eq!(f.palette_rgb(1), Some([6, 5, 4]));
    }

    #[test]
    fn short_palette_covers_only_its_entries() {
        let f = gray_frame().with_palette(vec![10, 20, 30, 40, 50, 60]);
        assert_eq!(f.palette_rgb(0), Some([10, 20, 30]));
        assert_eq!(f.palette_rgb(1), Some([40, 50, 60]));
        // Beyond the table: undefined by the frame → None.
        assert_eq!(f.palette_rgb(2), None);
        assert_eq!(f.palette_rgb(255), None);
    }

    #[test]
    fn empty_palette_clears_and_take_palette_detaches() {
        let mut f = gray_frame();
        f.set_palette(vec![1, 2, 3]);
        assert!(f.palette().is_some());

        // Empty input removes the side-channel entirely.
        f.set_palette(Vec::new());
        assert_eq!(f.palette(), None);
        assert_eq!(f.planes.len(), 1);

        // take_palette detaches and returns the bytes.
        f.set_palette(vec![4, 5, 6]);
        assert_eq!(f.take_palette(), Some(vec![4, 5, 6]));
        assert_eq!(f.palette(), None);
        assert_eq!(f.take_palette(), None);
        assert_eq!(f.planes.len(), 1);
    }

    #[test]
    fn zero_stride_empty_plane_is_not_mistaken_for_a_palette() {
        // stride == 0 with EMPTY data is the degenerate (but
        // contract-consistent) empty image plane, not the sentinel.
        let f = VideoFrame {
            pts: None,
            planes: vec![
                VideoPlane {
                    stride: 4,
                    data: vec![0u8; 8],
                },
                VideoPlane {
                    stride: 0,
                    data: Vec::new(),
                },
            ],
        };
        assert_eq!(f.palette(), None);
        assert_eq!(f.image_plane_count(), 2);
    }

    #[test]
    fn palette_on_frame_without_image_planes() {
        // A palette can be attached before pixel planes exist (encoder
        // scaffolding); the image-plane view is then empty.
        let f = VideoFrame {
            pts: None,
            planes: Vec::new(),
        }
        .with_palette(vec![1, 2, 3]);
        assert_eq!(f.palette(), Some(&[1, 2, 3][..]));
        assert_eq!(f.image_plane_count(), 0);
        assert!(f.image_planes().is_empty());
    }

    #[test]
    fn frame_without_significant_bits_reports_none() {
        let f = gray_frame();
        assert_eq!(f.significant_bits(), None);
        assert_eq!(f.plane_significant_bits(0), None);
        assert_eq!(f.image_plane_count(), 1);
    }

    #[test]
    fn set_significant_bits_round_trips_and_keeps_image_planes_intact() {
        // A 12-bit-luma / 10-bit-chroma mixed-depth frame (the VC-2
        // custom-signal-range shape that motivated the record).
        let mut f = VideoFrame {
            pts: Some(3),
            planes: vec![
                VideoPlane {
                    stride: 8,
                    data: vec![0u8; 16],
                },
                VideoPlane {
                    stride: 8,
                    data: vec![0u8; 16],
                },
                VideoPlane {
                    stride: 8,
                    data: vec![0u8; 16],
                },
            ],
        };
        f.set_significant_bits(vec![12, 10, 10]);

        assert_eq!(f.significant_bits(), Some(&[12, 10, 10][..]));
        assert_eq!(f.plane_significant_bits(0), Some(12));
        assert_eq!(f.plane_significant_bits(1), Some(10));
        assert_eq!(f.plane_significant_bits(2), Some(10));
        // Beyond the record: fall back to the format default → None.
        assert_eq!(f.plane_significant_bits(3), None);

        // Image-plane view is unchanged by the side-channel.
        assert_eq!(f.image_plane_count(), 3);
        assert_eq!(f.image_planes().len(), 3);
        // The raw field sees the sentinel entry at the tail.
        assert_eq!(f.planes.len(), 4);
        assert_eq!(f.planes[3].stride, usize::MAX);
    }

    #[test]
    fn set_significant_bits_replaces_and_empty_clears_and_take_detaches() {
        let mut f = gray_frame();
        f.set_significant_bits(vec![7]);
        f.set_significant_bits(vec![6]);
        // Replacement, not stacking.
        assert_eq!(f.planes.len(), 2);
        assert_eq!(f.significant_bits(), Some(&[6][..]));

        // Empty input removes the side-channel entirely.
        f.set_significant_bits(Vec::new());
        assert_eq!(f.significant_bits(), None);
        assert_eq!(f.planes.len(), 1);

        // take_significant_bits detaches and returns the bytes.
        f.set_significant_bits(vec![5]);
        assert_eq!(f.take_significant_bits(), Some(vec![5]));
        assert_eq!(f.significant_bits(), None);
        assert_eq!(f.take_significant_bits(), None);
        assert_eq!(f.planes.len(), 1);
    }

    #[test]
    fn palette_and_significant_bits_compose_in_either_order() {
        // Palette first, then depths.
        let mut f = gray_frame()
            .with_palette(vec![1, 2, 3])
            .with_significant_bits(vec![8]);
        assert_eq!(f.palette(), Some(&[1, 2, 3][..]));
        assert_eq!(f.significant_bits(), Some(&[8][..]));
        assert_eq!(f.image_plane_count(), 1);
        assert_eq!(f.planes.len(), 3);

        // Replacing one record must not disturb the other, regardless
        // of which currently sits at the tail.
        f.set_palette(vec![9, 8, 7]);
        assert_eq!(f.palette(), Some(&[9, 8, 7][..]));
        assert_eq!(f.significant_bits(), Some(&[8][..]));
        f.set_significant_bits(vec![7]);
        assert_eq!(f.palette(), Some(&[9, 8, 7][..]));
        assert_eq!(f.significant_bits(), Some(&[7][..]));
        assert_eq!(f.image_plane_count(), 1);

        // Depths first, then palette.
        let g = gray_frame()
            .with_significant_bits(vec![4])
            .with_palette(full_palette());
        assert_eq!(g.significant_bits(), Some(&[4][..]));
        assert_eq!(g.palette_rgb(0), Some([0x00, 0xFF, 0x55]));
        assert_eq!(g.image_plane_count(), 1);

        // Detaching one leaves the other attached.
        let mut h = g;
        assert_eq!(h.take_significant_bits(), Some(vec![4]));
        assert_eq!(h.significant_bits(), None);
        assert_eq!(h.palette().map(<[u8]>::len), Some(768));
        assert_eq!(h.take_palette().map(|p| p.len()), Some(768));
        assert_eq!(h.planes.len(), 1);
        assert_eq!(h.image_plane_count(), 1);
    }

    #[test]
    fn max_stride_empty_plane_is_not_mistaken_for_significant_bits() {
        // stride == usize::MAX with EMPTY data is not the sentinel
        // (mirroring the palette rule: sentinels require non-empty
        // data). Degenerate, but must not be misread as a record.
        let f = VideoFrame {
            pts: None,
            planes: vec![
                VideoPlane {
                    stride: 4,
                    data: vec![0u8; 8],
                },
                VideoPlane {
                    stride: usize::MAX,
                    data: Vec::new(),
                },
            ],
        };
        assert_eq!(f.significant_bits(), None);
        assert_eq!(f.image_plane_count(), 2);
    }

    #[test]
    fn significant_bits_on_frame_without_image_planes() {
        // Like the palette, the record can be attached before pixel
        // planes exist (encoder scaffolding).
        let f = VideoFrame {
            pts: None,
            planes: Vec::new(),
        }
        .with_significant_bits(vec![12, 10, 10]);
        assert_eq!(f.significant_bits(), Some(&[12, 10, 10][..]));
        assert_eq!(f.image_plane_count(), 0);
        assert!(f.image_planes().is_empty());
    }

    #[test]
    fn side_channels_survive_clone_and_frame_wrapping() {
        let f = gray_frame()
            .with_palette(vec![1, 2, 3])
            .with_significant_bits(vec![6]);
        let cloned = f.clone();
        assert_eq!(cloned.palette(), f.palette());
        assert_eq!(cloned.significant_bits(), f.significant_bits());

        let wrapped = Frame::Video(cloned);
        assert_eq!(wrapped.pts(), Some(7));
        if let Frame::Video(v) = wrapped {
            assert_eq!(v.palette(), Some(&[1, 2, 3][..]));
            assert_eq!(v.significant_bits(), Some(&[6][..]));
        } else {
            unreachable!("wrapped as Video above");
        }
    }

    #[test]
    fn frame_without_color_signal_or_layer_reports_none() {
        let f = gray_frame();
        assert_eq!(f.color_signal(), None);
        assert_eq!(f.layer(), None);
        assert_eq!(f.layer_or_base(), LayerIdentity::base());
        assert_eq!(f.image_plane_count(), 1);
        assert_eq!(f.planes.len(), 1);
    }

    #[test]
    fn set_color_signal_round_trips_and_keeps_image_planes_intact() {
        let mut f = gray_frame();
        let sig = ColorSignal::from_code_points(9, 16, 9, true);
        f.set_color_signal(sig);
        assert_eq!(f.color_signal(), Some(sig));
        assert_eq!(f.image_plane_count(), 1);
        assert_eq!(f.image_planes()[0].data.len(), 8);
        assert_eq!(f.planes.len(), 2);
        assert_eq!(f.planes[1].stride, usize::MAX - 1);
        assert_eq!(f.planes[1].data, sig.to_bytes());

        // Replacement, not stacking.
        f.set_color_signal(ColorSignal::srgb());
        assert_eq!(f.planes.len(), 2);
        assert_eq!(f.color_signal(), Some(ColorSignal::srgb()));

        // Detach.
        assert_eq!(f.take_color_signal(), Some(ColorSignal::srgb()));
        assert_eq!(f.color_signal(), None);
        assert_eq!(f.take_color_signal(), None);
        assert_eq!(f.planes.len(), 1);
    }

    #[test]
    fn set_layer_round_trips_and_keeps_image_planes_intact() {
        let mut f = gray_frame();
        let id = LayerIdentity::new(1).with_view_id(1).with_access_unit(9);
        f.set_layer(id);
        assert_eq!(f.layer(), Some(id));
        assert_eq!(f.layer_or_base(), id);
        assert_eq!(f.image_plane_count(), 1);
        assert_eq!(f.planes.len(), 2);
        assert_eq!(f.planes[1].stride, usize::MAX - 2);

        f.set_layer(LayerIdentity::new(2));
        assert_eq!(f.planes.len(), 2);
        assert_eq!(f.layer(), Some(LayerIdentity::new(2)));

        assert_eq!(f.take_layer(), Some(LayerIdentity::new(2)));
        assert_eq!(f.layer(), None);
        assert_eq!(f.take_layer(), None);
        assert_eq!(f.planes.len(), 1);
    }

    #[test]
    fn all_four_side_channels_compose_in_any_order() {
        let sig = ColorSignal::bt709_limited();
        let id = LayerIdentity::new(1).with_view_id(1);
        let mut f = gray_frame()
            .with_layer(id)
            .with_palette(vec![1, 2, 3])
            .with_color_signal(sig)
            .with_significant_bits(vec![8]);
        assert_eq!(f.planes.len(), 5);
        assert_eq!(f.image_plane_count(), 1);
        assert_eq!(f.palette(), Some(&[1, 2, 3][..]));
        assert_eq!(f.significant_bits(), Some(&[8][..]));
        assert_eq!(f.color_signal(), Some(sig));
        assert_eq!(f.layer(), Some(id));

        // Replacing one record from the middle of the run leaves the
        // others in place.
        f.set_color_signal(ColorSignal::srgb());
        assert_eq!(f.planes.len(), 5);
        assert_eq!(f.palette(), Some(&[1, 2, 3][..]));
        assert_eq!(f.significant_bits(), Some(&[8][..]));
        assert_eq!(f.layer(), Some(id));
        assert_eq!(f.color_signal(), Some(ColorSignal::srgb()));

        // Detaching in an arbitrary order.
        assert_eq!(f.take_layer(), Some(id));
        assert_eq!(f.take_palette(), Some(vec![1, 2, 3]));
        assert_eq!(f.color_signal(), Some(ColorSignal::srgb()));
        assert_eq!(f.significant_bits(), Some(&[8][..]));
        assert_eq!(f.take_color_signal(), Some(ColorSignal::srgb()));
        assert_eq!(f.take_significant_bits(), Some(vec![8]));
        assert_eq!(f.planes.len(), 1);
        assert_eq!(f.image_plane_count(), 1);
    }

    #[test]
    fn huge_stride_empty_plane_is_not_a_side_channel() {
        // Any stride in the reserved band with EMPTY data stays an
        // (degenerate) image plane — sentinels require non-empty data.
        let f = VideoFrame {
            pts: None,
            planes: vec![
                VideoPlane {
                    stride: 4,
                    data: vec![0u8; 8],
                },
                VideoPlane {
                    stride: usize::MAX - 1,
                    data: Vec::new(),
                },
                VideoPlane {
                    stride: usize::MAX - 2,
                    data: Vec::new(),
                },
            ],
        };
        assert_eq!(f.color_signal(), None);
        assert_eq!(f.layer(), None);
        assert_eq!(f.image_plane_count(), 3);
    }

    #[test]
    fn largest_real_stride_is_still_an_image_plane() {
        // isize::MAX is the largest stride a one-row plane can have;
        // it must not be classified as a side-channel tag.
        let f = VideoFrame {
            pts: None,
            planes: vec![VideoPlane {
                stride: isize::MAX as usize,
                data: vec![0u8; 1],
            }],
        };
        assert_eq!(f.image_plane_count(), 1);
        assert_eq!(f.color_signal(), None);
        assert_eq!(f.layer(), None);
    }

    #[test]
    fn malformed_short_records_read_as_none() {
        let f = VideoFrame {
            pts: None,
            planes: vec![
                VideoPlane {
                    stride: 4,
                    data: vec![0u8; 8],
                },
                VideoPlane {
                    stride: usize::MAX - 1,
                    data: vec![1, 2],
                },
                VideoPlane {
                    stride: usize::MAX - 2,
                    data: vec![0; 5],
                },
            ],
        };
        // They are side-channel-shaped (excluded from the image planes)
        // but decode to nothing.
        assert_eq!(f.image_plane_count(), 1);
        assert_eq!(f.color_signal(), None);
        assert_eq!(f.layer(), None);
    }

    #[test]
    fn color_signal_and_layer_survive_clone_and_frame_wrapping() {
        let sig = ColorSignal::from_code_points(1, 13, 0, true);
        let id = LayerIdentity::new(1).with_view_id(1).with_access_unit(3);
        let f = gray_frame().with_color_signal(sig).with_layer(id);
        let cloned = f.clone();
        assert_eq!(cloned.color_signal(), Some(sig));
        assert_eq!(cloned.layer(), Some(id));
        let wrapped = Frame::Video(cloned);
        assert_eq!(wrapped.pts(), Some(7));
        if let Frame::Video(v) = wrapped {
            assert_eq!(v.color_signal(), Some(sig));
            assert_eq!(v.layer(), Some(id));
        } else {
            unreachable!("wrapped as Video above");
        }
    }

    #[test]
    fn frame_without_palette_alpha_reads_opaque() {
        // No alpha record: palette_alpha is None and every covered
        // entry is opaque through palette_rgba.
        let f = gray_frame().with_palette(vec![1, 2, 3, 4, 5, 6]);
        assert_eq!(f.palette_alpha(), None);
        assert_eq!(f.palette_rgba(0), Some([1, 2, 3, 255]));
        assert_eq!(f.palette_rgba(1), Some([4, 5, 6, 255]));
        // Beyond the palette: None, exactly like palette_rgb.
        assert_eq!(f.palette_rgba(2), None);
        assert_eq!(f.image_plane_count(), 1);
        assert_eq!(f.planes.len(), 2);

        // No palette at all: no RGBA either.
        let g = gray_frame();
        assert_eq!(g.palette_alpha(), None);
        assert_eq!(g.palette_rgba(0), None);
    }

    #[test]
    fn set_palette_alpha_round_trips_and_keeps_image_planes_intact() {
        // A GIF-style table: entry 2 is the transparent index, the
        // record covers only entries 0..=2 and the rest stay opaque.
        let mut f = gray_frame().with_palette(full_palette());
        f.set_palette_alpha(vec![255, 255, 0]);

        assert_eq!(f.palette_alpha(), Some(&[255, 255, 0][..]));
        assert_eq!(f.palette_rgba(0), Some([0x00, 0xFF, 0x55, 255]));
        assert_eq!(f.palette_rgba(2), Some([0x02, 0xFD, 0x57, 0]));
        // Uncovered entries are opaque.
        assert_eq!(f.palette_rgba(3), Some([0x03, 0xFC, 0x56, 255]));
        assert_eq!(f.palette_rgba(255), Some([0xFF, 0x00, 0xAA, 255]));
        // Image-plane view is unchanged; the raw field sees two records.
        assert_eq!(f.image_plane_count(), 1);
        assert_eq!(f.image_planes()[0].data.len(), 8);
        assert_eq!(f.planes.len(), 3);
        assert_eq!(f.planes[2].stride, usize::MAX - 3);
        // The RGB record is untouched by the alpha record.
        assert_eq!(f.palette().map(<[u8]>::len), Some(768));

        // Replacement, not stacking.
        f.set_palette_alpha(vec![128]);
        assert_eq!(f.planes.len(), 3);
        assert_eq!(f.palette_alpha(), Some(&[128][..]));
        assert_eq!(f.palette_rgba(0), Some([0x00, 0xFF, 0x55, 128]));

        // Empty input removes the record; the palette stays.
        f.set_palette_alpha(Vec::new());
        assert_eq!(f.palette_alpha(), None);
        assert_eq!(f.planes.len(), 2);
        assert_eq!(f.palette().map(<[u8]>::len), Some(768));

        // take_palette_alpha detaches and returns the bytes.
        f.set_palette_alpha(vec![0, 255]);
        assert_eq!(f.take_palette_alpha(), Some(vec![0, 255]));
        assert_eq!(f.palette_alpha(), None);
        assert_eq!(f.take_palette_alpha(), None);
        assert_eq!(f.planes.len(), 2);
    }

    #[test]
    fn palette_alpha_longer_than_palette_is_malformed_and_reads_none() {
        // Two-entry palette, three alpha bytes: malformed → None, and
        // palette_rgba falls back to opaque.
        let mut f = gray_frame()
            .with_palette(vec![1, 2, 3, 4, 5, 6])
            .with_palette_alpha(vec![0, 0, 0]);
        assert_eq!(f.palette_alpha(), None);
        assert_eq!(f.palette_rgba(0), Some([1, 2, 3, 255]));
        assert_eq!(f.palette_rgba(1), Some([4, 5, 6, 255]));
        // The entry is still side-channel-shaped: excluded from the
        // image planes, present in the raw field.
        assert_eq!(f.image_plane_count(), 1);
        assert_eq!(f.planes.len(), 3);
        // Exactly as long as the palette is fine.
        f.set_palette_alpha(vec![0, 7]);
        assert_eq!(f.palette_alpha(), Some(&[0, 7][..]));
        assert_eq!(f.palette_rgba(1), Some([4, 5, 6, 7]));
        // Growing the palette afterwards makes a previously malformed
        // record valid: validation is at read time.
        f.set_palette_alpha(vec![0, 0, 0]);
        assert_eq!(f.palette_alpha(), None);
        f.set_palette(vec![1, 2, 3, 4, 5, 6, 7, 8, 9]);
        assert_eq!(f.palette_alpha(), Some(&[0, 0, 0][..]));
        // Taking a malformed record removes it but reports None.
        f.set_palette(vec![1, 2, 3]);
        assert_eq!(f.palette_alpha(), None);
        assert_eq!(f.take_palette_alpha(), None);
        assert_eq!(f.planes.len(), 2);
        assert_eq!(f.palette(), Some(&[1, 2, 3][..]));
    }

    #[test]
    fn palette_alpha_without_palette_reads_none_in_either_order() {
        // Alpha attached first (encoder scaffolding): meaningless until
        // the palette arrives, then readable.
        let mut f = gray_frame().with_palette_alpha(vec![0]);
        assert_eq!(f.palette_alpha(), None);
        assert_eq!(f.palette_rgba(0), None);
        assert_eq!(f.image_plane_count(), 1);
        assert_eq!(f.planes.len(), 2);
        f.set_palette(vec![9, 9, 9]);
        assert_eq!(f.palette_alpha(), Some(&[0][..]));
        assert_eq!(f.palette_rgba(0), Some([9, 9, 9, 0]));

        // Detaching the palette orphans the alpha record: None again,
        // still side-channel-shaped, and set_palette brings it back.
        assert_eq!(f.take_palette(), Some(vec![9, 9, 9]));
        assert_eq!(f.palette_alpha(), None);
        assert_eq!(f.planes.len(), 2);
        assert_eq!(f.image_plane_count(), 1);
        f.set_palette(vec![1, 1, 1]);
        assert_eq!(f.palette_alpha(), Some(&[0][..]));
    }

    #[test]
    fn set_palette_rgba_writes_both_records() {
        let table = [[10, 20, 30, 0], [40, 50, 60, 255], [70, 80, 90, 128]];
        let mut f = gray_frame().with_palette_rgba(&table);
        assert_eq!(f.palette(), Some(&[10, 20, 30, 40, 50, 60, 70, 80, 90][..]));
        assert_eq!(f.palette_alpha(), Some(&[0, 255, 128][..]));
        for (i, e) in table.iter().enumerate() {
            assert_eq!(f.palette_rgba(i as u8), Some(*e));
        }
        assert_eq!(f.palette_rgba(3), None);
        assert_eq!(f.image_plane_count(), 1);
        assert_eq!(f.planes.len(), 3);

        // Replaces an existing pair.
        f.set_palette_rgba(&[[1, 2, 3, 4]]);
        assert_eq!(f.planes.len(), 3);
        assert_eq!(f.palette_rgba(0), Some([1, 2, 3, 4]));
        assert_eq!(f.palette_rgba(1), None);

        // Empty clears both.
        f.set_palette_rgba(&[]);
        assert_eq!(f.palette(), None);
        assert_eq!(f.palette_alpha(), None);
        assert_eq!(f.planes.len(), 1);
    }

    #[test]
    fn all_five_side_channels_compose_in_any_order() {
        let sig = ColorSignal::bt709_limited();
        let id = LayerIdentity::new(1).with_view_id(1);
        // Alpha before palette, palette in the middle, every other
        // record around them.
        let mut f = gray_frame()
            .with_palette_alpha(vec![0])
            .with_layer(id)
            .with_palette(vec![1, 2, 3, 4, 5, 6])
            .with_color_signal(sig)
            .with_significant_bits(vec![8]);
        assert_eq!(f.planes.len(), 6);
        assert_eq!(f.image_plane_count(), 1);
        assert_eq!(f.palette(), Some(&[1, 2, 3, 4, 5, 6][..]));
        assert_eq!(f.palette_alpha(), Some(&[0][..]));
        assert_eq!(f.palette_rgba(0), Some([1, 2, 3, 0]));
        assert_eq!(f.palette_rgba(1), Some([4, 5, 6, 255]));
        assert_eq!(f.significant_bits(), Some(&[8][..]));
        assert_eq!(f.color_signal(), Some(sig));
        assert_eq!(f.layer(), Some(id));

        // Replacing records from the middle of the run leaves the
        // others in place.
        f.set_palette_alpha(vec![255, 0]);
        f.set_color_signal(ColorSignal::srgb());
        f.set_palette(vec![7, 8, 9, 10, 11, 12]);
        assert_eq!(f.planes.len(), 6);
        assert_eq!(f.palette_rgba(1), Some([10, 11, 12, 0]));
        assert_eq!(f.significant_bits(), Some(&[8][..]));
        assert_eq!(f.layer(), Some(id));
        assert_eq!(f.color_signal(), Some(ColorSignal::srgb()));

        // Detaching in an arbitrary order.
        assert_eq!(f.take_layer(), Some(id));
        assert_eq!(f.take_palette_alpha(), Some(vec![255, 0]));
        assert_eq!(f.palette_rgba(1), Some([10, 11, 12, 255]));
        assert_eq!(f.take_palette(), Some(vec![7, 8, 9, 10, 11, 12]));
        assert_eq!(f.take_color_signal(), Some(ColorSignal::srgb()));
        assert_eq!(f.take_significant_bits(), Some(vec![8]));
        assert_eq!(f.planes.len(), 1);
        assert_eq!(f.image_plane_count(), 1);

        // Reverse construction order lands on the same answers.
        let g = gray_frame()
            .with_significant_bits(vec![8])
            .with_color_signal(sig)
            .with_palette(vec![1, 2, 3])
            .with_layer(id)
            .with_palette_alpha(vec![9]);
        assert_eq!(g.planes.len(), 6);
        assert_eq!(g.image_plane_count(), 1);
        assert_eq!(g.palette_rgba(0), Some([1, 2, 3, 9]));
        assert_eq!(g.significant_bits(), Some(&[8][..]));
        assert_eq!(g.color_signal(), Some(sig));
        assert_eq!(g.layer(), Some(id));
    }

    #[test]
    fn palette_alpha_tag_with_empty_data_is_not_a_record() {
        let f = VideoFrame {
            pts: None,
            planes: vec![
                VideoPlane {
                    stride: 4,
                    data: vec![0u8; 8],
                },
                VideoPlane {
                    stride: usize::MAX - 3,
                    data: Vec::new(),
                },
            ],
        };
        assert_eq!(f.palette_alpha(), None);
        assert_eq!(f.image_plane_count(), 2);
    }

    #[test]
    fn palette_alpha_survives_clone_and_frame_wrapping() {
        let f = gray_frame().with_palette_rgba(&[[1, 2, 3, 0], [4, 5, 6, 255]]);
        let cloned = f.clone();
        assert_eq!(cloned.palette_alpha(), f.palette_alpha());
        let wrapped = Frame::Video(cloned);
        assert_eq!(wrapped.pts(), Some(7));
        if let Frame::Video(v) = wrapped {
            assert_eq!(v.palette_rgba(0), Some([1, 2, 3, 0]));
            assert_eq!(v.palette_rgba(1), Some([4, 5, 6, 255]));
        } else {
            unreachable!("wrapped as Video above");
        }
    }

    #[test]
    fn frame_without_blobs_reads_empty() {
        let f = gray_frame();
        assert!(f.blobs().is_empty());
        assert_eq!(f.blob(&BlobKind::ICC), None);
        assert_eq!(f.image_plane_count(), 1);
        assert_eq!(f.planes.len(), 1);
    }

    #[test]
    fn set_blobs_round_trips_and_keeps_image_planes_intact() {
        let icc = MetadataBlob::new(BlobKind::ICC, vec![1, 2, 3, 4]);
        let exif = MetadataBlob::new(BlobKind::EXIF, b"II*\0".to_vec());
        let mut f = gray_frame();
        f.set_blobs(vec![icc.clone(), exif.clone()]);

        assert_eq!(f.blobs(), vec![icc.clone(), exif.clone()]);
        assert_eq!(f.blob(&BlobKind::ICC), Some(icc.clone()));
        assert_eq!(f.blob(&BlobKind::EXIF), Some(exif.clone()));
        assert_eq!(f.blob(&BlobKind::XMP), None);
        // Image-plane view is unchanged; the raw field sees one record
        // holding the whole list, tagged usize::MAX - 4.
        assert_eq!(f.image_plane_count(), 1);
        assert_eq!(f.image_planes()[0].data.len(), 8);
        assert_eq!(f.planes.len(), 2);
        assert_eq!(f.planes[1].stride, usize::MAX - 4);
        assert_eq!(f.planes[1].data, encode_blobs(&[icc.clone(), exif.clone()]));

        // Replacement, not stacking.
        f.set_blobs(vec![exif.clone()]);
        assert_eq!(f.planes.len(), 2);
        assert_eq!(f.blobs(), vec![exif.clone()]);

        // Empty input removes the record entirely.
        f.set_blobs(Vec::new());
        assert!(f.blobs().is_empty());
        assert_eq!(f.planes.len(), 1);

        // take_blobs detaches and returns the list.
        f.set_blobs(vec![icc.clone()]);
        assert_eq!(f.take_blobs(), vec![icc]);
        assert!(f.blobs().is_empty());
        assert!(f.take_blobs().is_empty());
        assert_eq!(f.planes.len(), 1);
    }

    #[test]
    fn push_blob_appends_in_order_and_with_blob_chains() {
        let a = MetadataBlob::new(BlobKind::COVER_ART, vec![1]).with_mime("image/jpeg");
        let b = MetadataBlob::new(BlobKind::COVER_ART, vec![2]).with_mime("image/png");
        let c = MetadataBlob::new(BlobKind::custom("exr-attributes"), vec![3]);
        let mut f = gray_frame().with_blob(a.clone());
        f.push_blob(b.clone());
        f.push_blob(c.clone());
        // Same kind twice: both kept, first-of-kind lookup is stable.
        assert_eq!(f.blobs(), vec![a.clone(), b.clone(), c.clone()]);
        assert_eq!(f.blob(&BlobKind::COVER_ART), Some(a));
        assert_eq!(f.blob(&BlobKind::custom("exr-attributes")), Some(c));
        // Still one record.
        assert_eq!(f.planes.len(), 2);
        assert_eq!(f.image_plane_count(), 1);
    }

    #[test]
    fn malformed_blobs_record_reads_empty_and_is_replaced_by_push() {
        let mut f = VideoFrame {
            pts: None,
            planes: vec![
                VideoPlane {
                    stride: 4,
                    data: vec![0u8; 8],
                },
                VideoPlane {
                    stride: usize::MAX - 4,
                    data: vec![9, 9, 9],
                },
            ],
        };
        // Side-channel-shaped (excluded from the image planes) but
        // decodes to nothing.
        assert_eq!(f.image_plane_count(), 1);
        assert!(f.blobs().is_empty());
        assert_eq!(f.blob(&BlobKind::ICC), None);
        // push replaces the unreadable record with the single new blob.
        let icc = MetadataBlob::new(BlobKind::ICC, vec![7]);
        f.push_blob(icc.clone());
        assert_eq!(f.planes.len(), 2);
        assert_eq!(f.blobs(), vec![icc]);
        // take on a malformed record removes it and reports empty.
        f.planes[1].data = vec![9, 9, 9];
        assert!(f.take_blobs().is_empty());
        assert_eq!(f.planes.len(), 1);
    }

    #[test]
    fn blobs_tag_with_empty_data_is_not_a_record() {
        let f = VideoFrame {
            pts: None,
            planes: vec![
                VideoPlane {
                    stride: 4,
                    data: vec![0u8; 8],
                },
                VideoPlane {
                    stride: usize::MAX - 4,
                    data: Vec::new(),
                },
            ],
        };
        assert!(f.blobs().is_empty());
        assert_eq!(f.image_plane_count(), 2);
    }

    #[test]
    fn all_six_side_channels_compose_in_any_order() {
        let sig = ColorSignal::bt709_limited();
        let id = LayerIdentity::new(1).with_view_id(1);
        let icc = MetadataBlob::new(BlobKind::ICC, vec![1, 2, 3]);
        let xmp = MetadataBlob::new(BlobKind::XMP, b"<x/>".to_vec());
        let mut f = gray_frame()
            .with_blob(icc.clone())
            .with_palette_alpha(vec![0])
            .with_layer(id)
            .with_palette(vec![1, 2, 3, 4, 5, 6])
            .with_color_signal(sig)
            .with_significant_bits(vec![8]);
        assert_eq!(f.planes.len(), 7);
        assert_eq!(f.image_plane_count(), 1);
        assert_eq!(f.blobs(), vec![icc.clone()]);
        assert_eq!(f.palette_rgba(0), Some([1, 2, 3, 0]));
        assert_eq!(f.significant_bits(), Some(&[8][..]));
        assert_eq!(f.color_signal(), Some(sig));
        assert_eq!(f.layer(), Some(id));

        // Growing the blobs list from the middle of the run leaves the
        // others in place and keeps one record.
        f.push_blob(xmp.clone());
        f.set_color_signal(ColorSignal::srgb());
        assert_eq!(f.planes.len(), 7);
        assert_eq!(f.blobs(), vec![icc.clone(), xmp.clone()]);
        assert_eq!(f.palette_rgba(1), Some([4, 5, 6, 255]));
        assert_eq!(f.layer(), Some(id));
        assert_eq!(f.color_signal(), Some(ColorSignal::srgb()));

        // Detaching in an arbitrary order.
        assert_eq!(f.take_palette(), Some(vec![1, 2, 3, 4, 5, 6]));
        assert_eq!(f.blobs(), vec![icc.clone(), xmp.clone()]);
        assert_eq!(f.take_blobs(), vec![icc, xmp]);
        assert_eq!(f.take_layer(), Some(id));
        assert_eq!(f.take_palette_alpha(), None); // orphaned → None, removed
        assert_eq!(f.take_color_signal(), Some(ColorSignal::srgb()));
        assert_eq!(f.take_significant_bits(), Some(vec![8]));
        assert_eq!(f.planes.len(), 1);
        assert_eq!(f.image_plane_count(), 1);
    }

    #[test]
    fn blobs_survive_clone_and_frame_wrapping() {
        let exif = MetadataBlob::new(BlobKind::EXIF, b"MM\0*".to_vec());
        let f = gray_frame().with_blob(exif.clone());
        let cloned = f.clone();
        assert_eq!(cloned.blobs(), f.blobs());
        let wrapped = Frame::Video(cloned);
        assert_eq!(wrapped.pts(), Some(7));
        if let Frame::Video(v) = wrapped {
            assert_eq!(v.blob(&BlobKind::EXIF), Some(exif));
        } else {
            unreachable!("wrapped as Video above");
        }
    }

    #[test]
    fn palette_survives_clone_and_frame_wrapping() {
        let f = gray_frame().with_palette(full_palette());
        let cloned = f.clone();
        assert_eq!(cloned.palette(), f.palette());

        // Through the Frame enum, pts and palette both survive.
        let wrapped = Frame::Video(cloned);
        assert_eq!(wrapped.pts(), Some(7));
        if let Frame::Video(v) = wrapped {
            assert_eq!(v.palette().map(<[u8]>::len), Some(768));
        } else {
            unreachable!("wrapped as Video above");
        }
    }
}
