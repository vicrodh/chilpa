//! In-process codec registry.
//!
//! Every codec crate declares itself with one [`CodecInfo`] value —
//! capabilities, factory functions, the container tags it claims, and
//! (optionally) a probe function used to disambiguate genuine tag
//! collisions. The registry stores those registrations and exposes
//! three orthogonal lookups:
//!
//! - **id-keyed** — `make_decoder(params)` / `make_encoder(params)` walk
//!   the implementations registered under `params.codec_id`, filter by
//!   capability restrictions, and try them in priority order with init-
//!   time fallback.
//! - **tag-keyed** — `resolve_tag(&ProbeContext)` walks every
//!   registration whose `tags` contains `ctx.tag`, calls each probe
//!   (treating `None` as "returns 1.0"), and returns the id with the
//!   highest resulting confidence. Ties go to the lower
//!   [`CodecInfo::resolution_priority`], then to the earlier
//!   registration; `resolve_tag_candidates` exposes the ranked list.
//! - **payload-magic-keyed** — `resolve_payload_magic(first_bytes)`
//!   prefix-matches the claimed payload magic prefixes against a
//!   stream's leading bytes; longest matching magic wins, then the
//!   lower resolution priority, then registration order
//!   (`resolve_payload_magic_candidates` exposes the ranked list). For
//!   containers that identify a codec by the payload itself rather
//!   than a tag (e.g. an Ogg logical stream's first packet, or raw
//!   elementary streams).
//! - **diagnostic** — `all_implementations`, `all_tag_registrations`,
//!   `all_payload_magic_registrations`.
//!
//! The tag path explicitly DOES NOT short-circuit on "first claim with
//! no probe" — every claimant is asked, so a lower-priority probed
//! claim can out-rank a higher-priority unprobed one when the content
//! is actually ambiguous (DIV3 XVID-with-real-MSMPEG4 payload etc.).

use std::collections::HashMap;

use crate::arena;
use crate::{
    CodecCapabilities, CodecId, CodecOptionsStruct, CodecParameters, CodecResolver, CodecTag,
    Error, ExecutionContext, Frame, OptionField, Packet, PixelFormat, ProbeContext, ProbeFn,
    Result,
};

// ───────────────────────── codec traits ─────────────────────────

/// A packet-to-frame decoder.
pub trait Decoder: Send {
    /// Identifier of the codec this decoder handles.
    fn codec_id(&self) -> &CodecId;

    /// Feed one compressed packet. May or may not produce a frame immediately —
    /// call `receive_frame` in a loop afterwards.
    fn send_packet(&mut self, packet: &Packet) -> Result<()>;

    /// Feed one compressed packet handed over as a
    /// [`SharedPacket`](crate::packet::SharedPacket) — same contract as
    /// [`send_packet`](Self::send_packet).
    ///
    /// The default converts it (moving the payload when the packet is
    /// its only holder, copying the viewed bytes otherwise) and calls
    /// `send_packet`, so every decoder accepts shared packets. A decoder
    /// that keeps its input around, or whose input is a view of a large
    /// buffer, overrides this to hold the
    /// [`SharedBytes`](crate::packet::SharedBytes) view instead of
    /// copying it; such a decoder implements `send_packet` by wrapping
    /// the bytes and calling this method.
    fn send_shared_packet(&mut self, packet: crate::packet::SharedPacket) -> Result<()> {
        self.send_packet(&packet.into_packet())
    }

    /// Pull the next decoded frame, if any. Returns `Error::NeedMore` when the
    /// decoder needs another packet.
    ///
    /// Besides media frames, a decoder whose output shape can change
    /// mid-stream returns one [`Frame::StreamParams`] event immediately
    /// before the first frame of a new shape (never for the first
    /// shape, which [`output_params`](Self::output_params) announces).
    /// When the event is returned, `output_params()` already describes
    /// the new shape and
    /// [`output_params_generation`](Self::output_params_generation) has
    /// already moved; the next `Video` / `Audio` frame has the new
    /// shape. At most one event per change; the same rule holds after
    /// [`reset`](Self::reset) and while draining after
    /// [`flush`](Self::flush). Callers that do not care skip it
    /// ([`Frame::is_stream_params`]) — the counter carries the same
    /// information.
    fn receive_frame(&mut self) -> Result<Frame>;

    /// Pull the next decoded frame as an arena-backed [`arena::sync::Frame`].
    ///
    /// Decoders that build their output through an
    /// [`arena::sync::ArenaPool`] override this to return the pooled
    /// [`arena::sync::Frame`] **directly**, with no per-plane memcpy
    /// out — the caller gets true zero-copy plane access via
    /// [`arena::sync::FrameInner::plane`].
    ///
    /// The default implementation delegates to [`Self::receive_frame`]
    /// and copies the video planes into a freshly-leased one-shot
    /// `arena::sync::ArenaPool`. This makes the method an additive
    /// change for every existing [`Decoder`] impl: callers using the
    /// new API still work, but pay one memcpy per plane.
    ///
    /// **Audio / subtitle frames:** the [`arena::sync::Frame`] body is
    /// video-only (planes + [`arena::sync::FrameHeader`] with
    /// width/height/pixel format). The default implementation returns
    /// [`Error::Unsupported`] for non-video frames; an audio decoder
    /// that wants to expose `receive_arena_frame()` must override it
    /// with its own arena-backed audio-frame type once the framework
    /// gains one. Until then, audio decoders should keep using
    /// [`Self::receive_frame`].
    ///
    /// **[`Frame::StreamParams`] events** have no arena representation:
    /// the default implementation consumes them and returns the next
    /// frame (whose [`arena::sync::FrameHeader`] carries the new shape);
    /// [`output_params`](Self::output_params) /
    /// [`output_params_generation`](Self::output_params_generation) have
    /// moved by then, so a caller watching the counter still sees the
    /// change.
    fn receive_arena_frame(&mut self) -> Result<arena::sync::Frame> {
        let mut frame = self.receive_frame()?;
        while frame.is_stream_params() {
            frame = self.receive_frame()?;
        }
        match frame {
            Frame::Video(v) => video_frame_to_arena_sync_frame(&v),
            Frame::Audio(_) => Err(Error::unsupported(
                "receive_arena_frame: audio frames not yet supported by default impl",
            )),
            Frame::Subtitle(_) => Err(Error::unsupported(
                "receive_arena_frame: subtitle frames have no arena-backed representation",
            )),
            Frame::Vector(_) => Err(Error::unsupported(
                "receive_arena_frame: vector frames have no arena-backed representation",
            )),
            _ => Err(Error::unsupported(
                "receive_arena_frame: frame kind has no arena-backed representation",
            )),
        }
    }

    /// Signal end-of-stream. After this, `receive_frame` will drain buffered
    /// frames and eventually return `Error::Eof`.
    fn flush(&mut self) -> Result<()>;

    /// Discard all carry-over state so the decoder can resume from a new
    /// bitstream position without producing stale output. Called by the
    /// player after a container seek.
    ///
    /// Unlike [`flush`](Self::flush) (which signals end-of-stream and
    /// drains buffered frames), `reset` is expected to:
    /// * drop every buffered input packet and pending output frame;
    /// * zero any per-stream filter / predictor / overlap memory so the
    ///   next `send_packet` decodes as if it were the first;
    /// * leave the codec id and stream parameters untouched.
    ///
    /// The default is a conservative "drain-then-forget": call
    /// [`flush`](Self::flush) and ignore any remaining frames. Stateful
    /// codecs (LPC predictors, backward-adaptive gain, IMDCT overlap,
    /// reference pictures, …) should override this to wipe their
    /// internal state explicitly — otherwise the first ~N output
    /// samples after a seek will be glitchy until the state re-adapts.
    fn reset(&mut self) -> Result<()> {
        self.flush()?;
        // Drain any remaining output frames so the next send_packet
        // starts clean. NeedMore / Eof both mean "no more frames"; any
        // other error is surfaced so the caller can see why.
        loop {
            match self.receive_frame() {
                Ok(_) => {}
                Err(Error::NeedMore) | Err(Error::Eof) => return Ok(()),
                Err(e) => return Err(e),
            }
        }
    }

    /// Advisory: announce the runtime environment (today: a thread budget
    /// for codec-internal parallelism). Called at most once, before the
    /// first `send_packet`. Default no-op; codecs that want to run
    /// slice-/GOP-/tile-parallel override this to capture the budget.
    /// Ignoring the hint is always safe — callers must still work with
    /// a decoder that runs serial.
    fn set_execution_context(&mut self, _ctx: &ExecutionContext) {}

    /// Pixel layout of the video frames this decoder emits, stated
    /// before the first frame — from the codec's configuration record
    /// in `extradata`, from a negotiated hardware session, or from the
    /// decoder's fixed output policy. `Some` is a **fact about the
    /// frames to come** and takes precedence over the container's
    /// [`CodecParameters::pixel_format`](crate::CodecParameters::pixel_format)
    /// (a stream label is a claim the container made; the decoder is
    /// the one producing the planes). `None` — the default — means
    /// "not known yet: trust the stream parameters", and a decoder
    /// whose output depends on the first packet (an untagged file that
    /// decides its layout per picture) answers `None` until it has
    /// seen one and the real layout afterwards.
    ///
    /// Pipelines and gateways read it to label frames from codec-only
    /// registrations and to pick the conversion into an encoder's
    /// [`accepted_pixel_formats`](crate::CodecCapabilities::accepted_pixel_formats);
    /// a frame whose plane shape disagrees with the stated layout is
    /// the decoder's defect, not the consumer's. The answer may change
    /// between frames only for a decoder that documents it.
    ///
    /// This is the narrow, older form of
    /// [`output_params`](Self::output_params), whose default derives
    /// from it. New decoders implement `output_params` (the whole
    /// output description) and forward this method to it; consumers
    /// should prefer `output_params`.
    fn output_pixel_format(&self) -> Option<crate::PixelFormat> {
        None
    }

    /// The decoder's resolved description of the frames it emits — the
    /// **stream-level** home for what [`Frame`]s deliberately do not
    /// repeat (a [`VideoFrame`](crate::VideoFrame) /
    /// [`AudioFrame`](crate::AudioFrame) carries no width, height,
    /// format or rate: the shape does not change 60 times a second, so
    /// it is stated once, here, and re-stated only when it changes).
    ///
    /// Fields a decoder fills when it knows them:
    ///
    /// * **video** — [`width`](CodecParameters::width) /
    ///   [`height`](CodecParameters::height) as the frames have them
    ///   (the cropped / display size of the emitted planes, not the
    ///   coded macroblock-aligned size),
    ///   [`pixel_format`](CodecParameters::pixel_format),
    ///   [`frame_rate`](CodecParameters::frame_rate) when the bitstream
    ///   signals one, [`color_signal`](CodecParameters::color_signal)
    ///   from the bitstream's own signalling, and
    ///   [`layers`](CodecParameters::layers) for multi-layer output;
    /// * **audio** — [`sample_rate`](CodecParameters::sample_rate),
    ///   [`channels`](CodecParameters::channels),
    ///   [`sample_format`](CodecParameters::sample_format),
    ///   [`channel_layout`](CodecParameters::channel_layout).
    ///
    /// Fields the decoder does not know stay `None` / unspecified;
    /// container-side fields (`extradata`, `bit_rate`, `tag`,
    /// `language`, `blobs`, `options`, `limits`) are not the decoder's
    /// to answer and are ignored by
    /// [`CodecParameters::merge_output_params`], the helper that folds
    /// this answer into a stream's parameters.
    ///
    /// `Some` is a **fact about the frames** and takes precedence over
    /// the container's claim for every field it sets. `None` means "not
    /// known yet: trust the stream parameters". A decoder answers as
    /// soon as it has parsed what defines the output — its
    /// configuration record in `extradata` at construction, or the
    /// first sequence / frame header — ideally before the first frame
    /// is returned, and at the latest once `receive_frame` has returned
    /// the first frame. From then on the answer describes the frame
    /// most recently returned by `receive_frame` (and the ones after
    /// it, until [`output_params_generation`](Self::output_params_generation)
    /// changes).
    ///
    /// **Mid-stream changes** are announced three ways that always
    /// agree: this answer changes, the generation counter moves, and —
    /// for a decoder that emits it — `receive_frame` returns a
    /// [`Frame::StreamParams`] event carrying this same description,
    /// immediately before the first frame of the new shape. When that
    /// event is returned, this method already answers the new shape.
    /// No event is emitted for the first shape (resolving `None` →
    /// `Some`, or refining it, up to the first frame).
    ///
    /// **Default and the relation to [`output_pixel_format`](Self::output_pixel_format):**
    /// the default implementation derives from `output_pixel_format()`
    /// — `Some` of a video [`CodecParameters`] carrying only
    /// `codec_id` and `pixel_format` when that answers `Some`, `None`
    /// otherwise — so every decoder that implements the older narrow
    /// query is described here for free. The direction is one-way:
    /// `output_params` reads `output_pixel_format`, never the other
    /// way round (no default recursion). A decoder that implements
    /// `output_params` itself should also forward
    /// `output_pixel_format` to it
    /// (`self.output_params().and_then(|p| p.pixel_format)`) so callers
    /// of the narrow query keep seeing the same answer. Consumers
    /// should query `output_params` — it is the complete answer.
    fn output_params(&self) -> Option<CodecParameters> {
        self.output_pixel_format().map(|pf| {
            let mut p = CodecParameters::video(self.codec_id().clone());
            p.pixel_format = Some(pf);
            p
        })
    }

    /// A cheap change signal for [`output_params`](Self::output_params):
    /// an integer that changes whenever the output description changes
    /// **mid-stream** (a resolution switch at a keyframe, a new pixel
    /// format at a new sequence header, a sample-rate change in a
    /// chained stream). Consumers keep the last value they saw and
    /// compare one integer after each successful `receive_frame`;
    /// only when it differs do they re-read `output_params()`. Frames
    /// carry nothing extra.
    ///
    /// The contract:
    ///
    /// * Consumers always read `output_params()` once after the first
    ///   frame (or before it, if they need the shape up front) —
    ///   resolving the description from `None` to `Some` before or at
    ///   the first frame is **not** a change and needs no bump.
    /// * After the first frame has been returned, a decoder whose
    ///   output description changes increments the value (wrapping,
    ///   so it only ever needs `!=` comparison) **at a frame
    ///   boundary**: the bump and the new `output_params()` answer
    ///   become visible together, when `receive_frame` returns the
    ///   first frame with the new shape — or, for a decoder that emits
    ///   the in-band [`Frame::StreamParams`] event, when it returns that
    ///   event (immediately before that frame) — not when the decoder
    ///   parses the header announcing it, because frames of the old
    ///   shape may still be queued in its reorder buffer. Once
    ///   `receive_frame` has returned a media frame, `output_params()`
    ///   describes that frame. One change = one bump = at most one
    ///   event.
    /// * It never changes while the description stays the same, and a
    ///   bump always comes with a changed description.
    /// * [`reset`](Self::reset) after a seek may change it (the new
    ///   position can be in a segment of a different shape) under the
    ///   same rule: visible with the first frame returned afterwards.
    ///
    /// The default `0` — "never changes" — is correct for every decoder
    /// whose output shape is fixed for the stream's lifetime.
    /// [`OutputParamsWatch`] packages the consumer side in one call.
    fn output_params_generation(&self) -> u64 {
        0
    }
}

/// Consumer-side helper for the [`Decoder::output_params`] /
/// [`Decoder::output_params_generation`] contract: folds the decoder's
/// output description into a stream's [`CodecParameters`] once, and
/// again only when the decoder signals a mid-stream change — one
/// integer compare per frame otherwise.
///
/// Call [`observe`](Self::observe) after **every** successful
/// `receive_frame` (the first call always reads the description; later
/// calls re-read only when the generation moved). Call
/// [`prime`](Self::prime) before the first frame when the shape is
/// needed up front (to size a renderer, pick a converter); it merges
/// whatever the decoder already knows without consuming the
/// first-frame read, since the description may still be refined up to
/// the first frame.
///
/// A consumer may instead (or additionally) watch the in-band
/// [`Frame::StreamParams`] event with
/// [`observe_frame`](Self::observe_frame); the two agree — the
/// generation has already moved when the event is returned, and
/// merging the same description twice reports no second change — so a
/// mid-stream change is reported exactly once either way.
///
/// ```
/// # use oxideav_core::{CodecParameters, Decoder, OutputParamsWatch};
/// fn pump(dec: &mut dyn Decoder, stream: &mut CodecParameters) -> oxideav_core::Result<()> {
///     let mut watch = OutputParamsWatch::new();
///     while let Ok(_frame) = dec.receive_frame() {
///         if watch.observe(&*dec, stream) {
///             // shape changed (or was first resolved): reconfigure
///             // the converter / renderer from `stream` here.
///         }
///     }
///     Ok(())
/// }
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OutputParamsWatch {
    seen: Option<u64>,
}

impl OutputParamsWatch {
    /// A watch that has not read the decoder yet.
    pub const fn new() -> Self {
        Self { seen: None }
    }

    /// Call after a successful `receive_frame`. Re-reads
    /// [`Decoder::output_params`] on the first call and whenever
    /// [`Decoder::output_params_generation`] differs from the value of
    /// the last read, merging it into `stream` through
    /// [`CodecParameters::merge_output_params`]. Returns `true` when
    /// that changed an output-describing field of `stream`.
    ///
    /// A decoder that still answers `None` leaves `stream` untouched
    /// and is asked again on the next call.
    pub fn observe<D: Decoder + ?Sized>(
        &mut self,
        decoder: &D,
        stream: &mut CodecParameters,
    ) -> bool {
        let generation = decoder.output_params_generation();
        if self.seen == Some(generation) {
            return false;
        }
        match decoder.output_params() {
            Some(p) => {
                self.seen = Some(generation);
                stream.merge_output_params(&p)
            }
            None => false,
        }
    }

    /// Event-side counterpart of [`observe`](Self::observe): when
    /// `frame` is a [`Frame::StreamParams`] event, merge its parameters
    /// into `stream` through [`CodecParameters::merge_output_params`]
    /// and return `true` when that changed an output-describing field;
    /// for any other frame do nothing and return `false`. Needs no
    /// decoder, so it also works downstream of a pipeline stage that
    /// only forwards frames.
    pub fn observe_frame(&mut self, frame: &Frame, stream: &mut CodecParameters) -> bool {
        match frame.stream_params() {
            Some(p) => stream.merge_output_params(p),
            None => false,
        }
    }

    /// Merge what the decoder already knows into `stream` without
    /// recording a read — for use before the first frame. Returns
    /// `true` when an output-describing field of `stream` changed.
    pub fn prime<D: Decoder + ?Sized>(&self, decoder: &D, stream: &mut CodecParameters) -> bool {
        decoder
            .output_params()
            .is_some_and(|p| stream.merge_output_params(&p))
    }

    /// Forget the last read so the next [`observe`](Self::observe)
    /// re-reads unconditionally (e.g. after swapping the decoder).
    pub fn reset(&mut self) {
        self.seen = None;
    }
}

/// A frame-to-packet encoder.
pub trait Encoder: Send {
    /// Identifier of the codec this encoder produces.
    fn codec_id(&self) -> &CodecId;

    /// Parameters describing this encoder's output stream (to feed into a muxer).
    fn output_params(&self) -> &CodecParameters;

    /// Feed one uncompressed frame. May or may not produce a packet
    /// immediately — call `receive_packet` in a loop afterwards.
    ///
    /// A [`Frame::StreamParams`] event (a mid-stream change of the
    /// input shape) is rejected with [`Error::Unsupported`] unless the
    /// encoder can re-configure for the new shape mid-stream; an
    /// encoder that can, re-configures and keeps
    /// [`output_params`](Self::output_params) describing its output.
    fn send_frame(&mut self, frame: &Frame) -> Result<()>;

    /// Pull the next encoded packet, if any. Returns `Error::NeedMore`
    /// when the encoder needs another frame (or a `flush`).
    fn receive_packet(&mut self) -> Result<Packet>;

    /// Signal end of input: drain internal lookahead so the remaining
    /// packets become available via `receive_packet`.
    fn flush(&mut self) -> Result<()>;

    /// Advisory: announce the runtime environment. Same semantics as
    /// [`Decoder::set_execution_context`].
    fn set_execution_context(&mut self, _ctx: &ExecutionContext) {}
}

/// Default-impl helper for [`Decoder::receive_arena_frame`]: copy a
/// heap-backed [`crate::VideoFrame`] into a freshly-leased
/// [`arena::sync::Frame`].
///
/// Allocates a single-slot, single-arena `arena::sync::ArenaPool`
/// sized to fit the planes verbatim. The pool is dropped at the end of
/// this call; the returned `Frame` keeps its leased buffer alive via
/// `Arc<FrameInner>` (the `Arena`'s `Weak` handle to the dropped pool
/// just stops upgrading — the buffer drops normally when the last
/// `Frame` clone goes away).
///
/// Width / height / pixel-format on the returned `FrameHeader` are
/// derived from the plane shape: `width = plane[0].stride`,
/// `height = plane[0].data.len() / stride`. Pixel format is left as
/// [`PixelFormat::Yuv420P`] when there are 3 planes, else the first
/// per-plane sensible default — this is a best-effort label for the
/// generic conversion path; decoders that override
/// `receive_arena_frame` themselves should set the correct pixel
/// format.
fn video_frame_to_arena_sync_frame(v: &crate::VideoFrame) -> Result<arena::sync::Frame> {
    if v.planes.is_empty() {
        return Err(Error::invalid(
            "receive_arena_frame: video frame has no planes",
        ));
    }
    let total_bytes: usize = v.planes.iter().map(|p| p.data.len()).sum();
    if total_bytes == 0 {
        return Err(Error::invalid(
            "receive_arena_frame: video frame planes are empty",
        ));
    }
    // One-shot pool sized exactly to the frame. The pool drops at end
    // of scope; the leased Arena lives on inside the returned Frame
    // (its Weak<ArenaPool> handle just won't upgrade in Drop, so the
    // Box<[u8]> falls through to a normal heap free).
    let pool = arena::sync::ArenaPool::with_alloc_count_cap(
        1,
        total_bytes,
        // One alloc per plane, plus a generous safety margin.
        (v.planes.len() as u32).saturating_add(4),
    );
    let arena = pool.lease()?;
    let mut plane_offsets: Vec<(usize, usize)> = Vec::with_capacity(v.planes.len());
    let mut cursor = 0usize;
    for plane in &v.planes {
        let dst = arena.alloc::<u8>(plane.data.len())?;
        dst.copy_from_slice(&plane.data);
        plane_offsets.push((cursor, plane.data.len()));
        cursor += plane.data.len();
    }
    // Best-effort header: width = stride of plane 0, height inferred
    // from plane 0's data length. Pixel format defaults to Yuv420P for
    // the common 3-plane case, Gray8 for single-plane, otherwise
    // Yuv444P. Decoders that care about exact pixel-format / width /
    // height should override `receive_arena_frame` themselves so they
    // can emit a correct `FrameHeader` straight from their arena
    // build path.
    let stride0 = v.planes[0].stride.max(1);
    let width = stride0 as u32;
    let height = (v.planes[0].data.len() / stride0) as u32;
    // Count only image planes for the format guess — side-channel
    // entries (palette, significant bits; copied verbatim above like
    // any other plane) must not bump e.g. a single-plane palette frame
    // out of the Gray8 label.
    let pixel_format = match v.image_plane_count() {
        1 => PixelFormat::Gray8,
        3 => PixelFormat::Yuv420P,
        _ => PixelFormat::Yuv444P,
    };
    let header = arena::sync::FrameHeader::new(width, height, pixel_format, v.pts);
    arena::sync::FrameInner::new(arena, &plane_offsets, header)
}

/// Factory that builds a decoder for a given codec parameter set.
pub type DecoderFactory = fn(params: &CodecParameters) -> Result<Box<dyn Decoder>>;

/// Factory that builds an encoder for a given codec parameter set.
pub type EncoderFactory = fn(params: &CodecParameters) -> Result<Box<dyn Encoder>>;

// ───────────────────────── CodecInfo ─────────────────────────

/// A single registration: capabilities, decoder/encoder factories,
/// optional probe, and the container tags this codec claims.
///
/// Codec crates build one of these per codec id inside their
/// `register(reg)` function and hand it to
/// [`CodecRegistry::register`]. The struct is `#[non_exhaustive]` so
/// additional fields can be added without breaking existing codec
/// crates — construction is only possible through
/// [`CodecInfo::new`] plus the builder methods below.
#[non_exhaustive]
pub struct CodecInfo {
    /// Canonical codec identifier this entry registers.
    pub id: CodecId,
    /// Capability description (media kind, feature flags, priority).
    pub capabilities: CodecCapabilities,
    /// Factory producing a fresh decoder instance, if decode is supported.
    pub decoder_factory: Option<DecoderFactory>,
    /// Factory producing a fresh encoder instance, if encode is supported.
    pub encoder_factory: Option<EncoderFactory>,
    /// Probe function that returns a confidence in `0.0..=1.0` for a
    /// given [`ProbeContext`]. `None` means "confidence 1.0 for every
    /// claimed tag" — the correct default for codecs whose tag claims
    /// are unambiguous.
    pub probe: Option<ProbeFn>,
    /// Tags this codec is willing to be looked up under. One codec may
    /// claim many tags (an AAC decoder covers several WaveFormat ids,
    /// a FourCC, an MP4 OTI, and a Matroska CodecID string at once).
    pub tags: Vec<CodecTag>,
    /// Payload magic prefixes this codec answers to (`\x01vorbis`,
    /// `OpusHead`, …). Some carriage formats have no codec tag — the
    /// codec is announced by a magic byte prefix on the payload itself
    /// (an Ogg logical stream's first packet is the canonical case;
    /// raw elementary streams are another). Such claims are
    /// prefix-matched by
    /// [`CodecRegistry::resolve_payload_magic_ref`] instead of living in
    /// the exact-match [`CodecTag`] index. Attached with
    /// [`Self::payload_magic`] / [`Self::payload_magics`]. Empty prefixes are
    /// ignored at registration time (a zero-length prefix would match
    /// every stream while carrying no evidence).
    pub payload_magics: Vec<Vec<u8>>,
    /// Schema of the encoder's recognised option keys
    /// (`CodecParameters::options`). Attached with
    /// [`Self::encoder_options`]. Used for validation / `oxideav list`
    /// / pipeline JSON checks.
    pub encoder_options_schema: Option<&'static [OptionField]>,
    /// Schema of the decoder's recognised option keys.
    pub decoder_options_schema: Option<&'static [OptionField]>,
    /// HW backend identifier, e.g. `"nvidia"`, `"vaapi"`, `"vdpau"`,
    /// `"vulkan-video"`, `"videotoolbox"`. Set by HW siblings on every
    /// `CodecInfo` they register; SW codecs leave this `None`.
    /// Consumers (e.g. the CLI's `info` command) use it to group
    /// codec entries by backend and to dedupe probe calls — multiple
    /// `CodecInfo` entries with the same `engine_id` typically share
    /// an `engine_probe` function, and consumers should call the probe
    /// at most once per `engine_id` per pass. Attached via
    /// [`Self::with_engine_id`].
    pub engine_id: Option<&'static str>,
    /// Optional engine probe function. When `Some`, calling it returns
    /// one [`crate::engine::HwDeviceInfo`] entry per device the backend
    /// sees. Phase-2 HW siblings populate this on every `CodecInfo`
    /// they register; Phase-3 consumers (CLI) call it on demand.
    /// Attached via [`Self::with_engine_probe`].
    pub engine_probe: Option<crate::engine::EngineProbeFn>,
    /// Tie-break rank for this registration's **claims** (tags and
    /// payload magics) when another codec id claims the same tag /
    /// magic with equal evidence. Lower is preferred; defaults to
    /// [`DEFAULT_PRIORITY`](crate::DEFAULT_PRIORITY). Deliberately
    /// separate from [`CodecCapabilities::priority`]: that field ranks
    /// *implementations of one codec id* against each other (HW before
    /// SW), and a backend's implementation preference must not silently
    /// out-rank a different codec's identity claim on an ambiguous tag.
    /// Attached via [`Self::with_resolution_priority`].
    pub resolution_priority: i32,
}

impl CodecInfo {
    /// Start a new registration for `id` with empty capabilities, no
    /// factories, no probe, and no tags. Chain the builder methods
    /// below to fill it in, then hand the result to
    /// [`CodecRegistry::register`].
    pub fn new(id: CodecId) -> Self {
        Self {
            capabilities: CodecCapabilities::audio(id.as_str()),
            id,
            decoder_factory: None,
            encoder_factory: None,
            probe: None,
            tags: Vec::new(),
            payload_magics: Vec::new(),
            encoder_options_schema: None,
            decoder_options_schema: None,
            engine_id: None,
            engine_probe: None,
            resolution_priority: crate::DEFAULT_PRIORITY,
        }
    }

    /// Set the claim tie-break priority (see
    /// [`Self::resolution_priority`]). Lower is preferred. Only consulted
    /// when another codec id claims the same tag with equal probe
    /// confidence, or the same-length payload magic; evidence always
    /// out-ranks priority, and equal priorities fall through to
    /// registration order (earlier wins).
    pub fn with_resolution_priority(mut self, priority: i32) -> Self {
        self.resolution_priority = priority;
        self
    }

    /// Replace the capability description. The default built by
    /// [`Self::new`] is a placeholder (audio-flavoured, no flags); every
    /// real registration should call this.
    pub fn capabilities(mut self, caps: CodecCapabilities) -> Self {
        self.capabilities = caps;
        self
    }

    /// Builder: attach the decoder factory.
    pub fn decoder(mut self, factory: DecoderFactory) -> Self {
        self.decoder_factory = Some(factory);
        self
    }

    /// Builder: attach the encoder factory.
    pub fn encoder(mut self, factory: EncoderFactory) -> Self {
        self.encoder_factory = Some(factory);
        self
    }

    /// Builder: attach a confidence probe (see [`CodecInfo::probe`]).
    pub fn probe(mut self, probe: ProbeFn) -> Self {
        self.probe = Some(probe);
        self
    }

    /// Claim a single container tag for this codec. Equivalent to
    /// `.tags([tag])` but avoids the array ceremony for single-tag
    /// claims.
    pub fn tag(mut self, tag: CodecTag) -> Self {
        self.tags.push(tag);
        self
    }

    /// Claim a set of container tags for this codec. Takes any
    /// iterable (arrays, `Vec`, `Option`, …) so the common case of a
    /// codec with 3-6 tags reads as one clean block.
    pub fn tags(mut self, tags: impl IntoIterator<Item = CodecTag>) -> Self {
        self.tags.extend(tags);
        self
    }

    /// Claim one payload magic prefix for this codec (see
    /// [`Self::payload_magics`]). Chain repeatedly for codecs that answer
    /// to more than one magic:
    ///
    /// ```
    /// # use oxideav_core::registry::CodecInfo;
    /// # use oxideav_core::CodecId;
    /// let info = CodecInfo::new(CodecId::new("vorbis")).payload_magic(b"\x01vorbis");
    /// # let _ = info;
    /// ```
    pub fn payload_magic(mut self, magic: impl Into<Vec<u8>>) -> Self {
        self.payload_magics.push(magic.into());
        self
    }

    /// Claim a set of payload magic prefixes for this codec — the
    /// iterable companion to [`Self::payload_magic`], mirroring the
    /// [`Self::tag`] / [`Self::tags`] pair.
    pub fn payload_magics<I>(mut self, magics: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<Vec<u8>>,
    {
        self.payload_magics
            .extend(magics.into_iter().map(Into::into));
        self
    }

    /// Declare the options struct this codec's encoder factory expects.
    /// Attaches `T::SCHEMA` so the registry can enumerate recognised
    /// option keys (for `oxideav list`, pipeline JSON validation, etc.).
    /// The factory itself still has to call
    /// [`crate::parse_options::<T>()`] against
    /// `CodecParameters::options` at init time.
    pub fn encoder_options<T: CodecOptionsStruct>(mut self) -> Self {
        self.encoder_options_schema = Some(T::SCHEMA);
        self
    }

    /// Declare the options struct this codec's decoder factory expects.
    /// See [`Self::encoder_options`] for the encoder counterpart.
    pub fn decoder_options<T: CodecOptionsStruct>(mut self) -> Self {
        self.decoder_options_schema = Some(T::SCHEMA);
        self
    }

    /// Tag this codec as belonging to a HW backend identified by
    /// `engine_id`. Should match the `engine_id` of every other
    /// `CodecInfo` registered by the same backend, and the corresponding
    /// `engine_id` field used by the CLI for grouping. SW codecs leave
    /// this unset.
    pub fn with_engine_id(mut self, engine_id: &'static str) -> Self {
        self.engine_id = Some(engine_id);
        self
    }

    /// Attach a probe function. Consumers call it to enumerate the
    /// engines (devices) this backend can dispatch to. Probes are
    /// expected to be idempotent and side-effect free; consumers may
    /// call them more than once per process and should dedupe by
    /// [`Self::engine_id`].
    pub fn with_engine_probe(mut self, probe: crate::engine::EngineProbeFn) -> Self {
        self.engine_probe = Some(probe);
        self
    }
}

/// Internal per-impl record held inside the registry's id map. Kept
/// distinct from [`CodecInfo`] so the id map stays cheap to walk
/// during `make_decoder` / `make_encoder` lookups.
#[derive(Clone)]
pub struct CodecImplementation {
    /// Capability description copied from the originating [`CodecInfo`].
    pub caps: CodecCapabilities,
    /// Decoder factory, if this implementation can decode.
    pub make_decoder: Option<DecoderFactory>,
    /// Encoder factory, if this implementation can encode.
    pub make_encoder: Option<EncoderFactory>,
    /// Encoder options schema declared via
    /// [`CodecInfo::encoder_options`]. `None` means the encoder accepts
    /// no tuning knobs (any non-empty `CodecParameters::options` will
    /// still be rejected by the factory if the encoder calls
    /// `parse_options` — this is purely informational for discovery).
    pub encoder_options_schema: Option<&'static [OptionField]>,
    /// Decoder options schema declared via
    /// [`CodecInfo::decoder_options`]; same semantics as the encoder
    /// schema above.
    pub decoder_options_schema: Option<&'static [OptionField]>,
    /// HW backend identifier copied verbatim from the originating
    /// [`CodecInfo::engine_id`]. `Some("nvidia"/"vaapi"/...)` on HW
    /// backends; `None` on SW codecs. Consumers (CLI `info` command,
    /// pipeline dispatcher, bench loop) read this to group entries by
    /// backend without grepping `caps.implementation`.
    pub engine_id: Option<&'static str>,
    /// Engine probe function copied verbatim from the originating
    /// [`CodecInfo::engine_probe`]. `Some(fn)` on HW backends with a
    /// probe wired; `None` on SW codecs. Consumers call it on demand
    /// to enumerate per-device info ([`crate::engine::HwDeviceInfo`]).
    pub engine_probe: Option<crate::engine::EngineProbeFn>,
}

/// Registry mapping codec ids and container tags to their registered
/// implementations; the lookup point behind `make_decoder` /
/// `make_encoder` / `resolve_tag`.
#[derive(Default)]
pub struct CodecRegistry {
    /// id → list of implementations. Each registered codec appends one
    /// entry here. `make_decoder` / `make_encoder` walk this list in
    /// preference order.
    impls: HashMap<CodecId, Vec<CodecImplementation>>,
    /// Codec ids in the order their first implementation was
    /// registered — drives every id-level enumeration so listings are
    /// stable across processes.
    impl_order: Vec<CodecId>,
    /// Append-only list of every registration — the `tag_index` stores
    /// offsets into this vector.
    registrations: Vec<RegistrationRecord>,
    /// Tag → indices into `registrations`. Indices are stored in
    /// registration order so tie-breaking in `resolve_tag` is
    /// deterministic (first-registered wins).
    tag_index: HashMap<CodecTag, Vec<usize>>,
    /// Payload magic-prefix claims: `(magic, registration index)` in
    /// registration order. Kept as a flat list rather than a map
    /// because resolution is prefix matching (see
    /// [`Self::resolve_payload_magic_ref`]), not exact-key lookup.
    magic_index: Vec<(Vec<u8>, usize)>,
}

/// Internal registry record. Mirrors the subset of [`CodecInfo`]
/// needed at resolve time.
struct RegistrationRecord {
    id: CodecId,
    probe: Option<ProbeFn>,
    /// Claim tie-break rank copied from [`CodecInfo::resolution_priority`].
    priority: i32,
    /// Tags claimed by this registration, in declaration order — the
    /// source of [`CodecRegistry::all_tag_registrations`]'s ordering.
    tags: Vec<CodecTag>,
}

/// One ranked entry from [`CodecRegistry::resolve_tag_candidates`].
///
/// Ordered by the registry's resolution rule — `confidence`
/// descending, then `priority` ascending (lower is preferred), then
/// `order` ascending (earlier registration wins) — so the first
/// element is exactly what [`CodecRegistry::resolve_tag_ref`] returns.
/// `#[non_exhaustive]`: read by field, never constructed by callers.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TagCandidate<'a> {
    /// Codec id of the claiming registration.
    pub id: &'a CodecId,
    /// Probe confidence for this context (`1.0` for unprobed claims;
    /// never `<= 0.0` — such claims are not candidates).
    pub confidence: crate::Confidence,
    /// Claim tie-break priority (see [`CodecInfo::resolution_priority`]).
    pub priority: i32,
    /// 0-based registration sequence number (one per
    /// [`CodecRegistry::register`] call) — the final tie-break. The
    /// same id can appear more than once when it registered the claim
    /// twice; the registry does not fold duplicates.
    pub order: usize,
}

/// One ranked entry from [`CodecRegistry::resolve_payload_magic_candidates`].
///
/// Ordered by the registry's resolution rule — magic length
/// descending (most specific prefix), then `priority` ascending, then
/// `order` ascending — so the first element is exactly what
/// [`CodecRegistry::resolve_payload_magic_ref`] returns.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PayloadMagicCandidate<'a> {
    /// Codec id of the claiming registration.
    pub id: &'a CodecId,
    /// The magic prefix that matched.
    pub magic: &'a [u8],
    /// Claim tie-break priority (see [`CodecInfo::resolution_priority`]).
    pub priority: i32,
    /// 0-based registration sequence number — the final tie-break.
    pub order: usize,
}

impl CodecRegistry {
    /// An empty registry (same as `Default`).
    pub fn new() -> Self {
        Self::default()
    }

    /// Register one codec. Expands into:
    ///   * an entry in the id → implementations map (for
    ///     `make_decoder` / `make_encoder`);
    ///   * an entry in the tag index for every claimed tag (for
    ///     `resolve_tag`).
    ///
    /// Calling `register` multiple times with the same id is allowed
    /// and how multi-implementation codecs (software-plus-hardware
    /// FLAC, for example) are expressed.
    pub fn register(&mut self, info: CodecInfo) {
        let CodecInfo {
            id,
            capabilities,
            decoder_factory,
            encoder_factory,
            probe,
            tags,
            payload_magics,
            encoder_options_schema,
            decoder_options_schema,
            // engine_id / engine_probe are metadata attached to a
            // CodecInfo for backends that want consumers (CLI `info`,
            // pipeline bench) to enumerate the underlying devices on
            // demand. They're surfaced verbatim on the resulting
            // CodecImplementation so consumers can read them without
            // grepping `caps.implementation`. Tag-only CodecInfo entries
            // (no factories) drop the values on the floor — there's no
            // CodecImplementation built in that branch.
            engine_id,
            engine_probe,
            resolution_priority,
        } = info;

        let caps = {
            let mut c = capabilities;
            if decoder_factory.is_some() {
                c = c.with_decode();
            }
            if encoder_factory.is_some() {
                c = c.with_encode();
            }
            c
        };

        // Only record an implementation entry when at least one factory
        // is present. A "tag-only" CodecInfo — used to attach extra tag
        // claims to a codec that was already registered with factories —
        // shouldn't pollute the impl list.
        if decoder_factory.is_some() || encoder_factory.is_some() {
            let slot = self.impls.entry(id.clone()).or_default();
            if slot.is_empty() {
                self.impl_order.push(id.clone());
            }
            slot.push(CodecImplementation {
                caps,
                make_decoder: decoder_factory,
                make_encoder: encoder_factory,
                encoder_options_schema,
                decoder_options_schema,
                engine_id,
                engine_probe,
            });
        }

        let record_idx = self.registrations.len();
        for tag in &tags {
            self.tag_index
                .entry(tag.clone())
                .or_default()
                .push(record_idx);
        }
        self.registrations.push(RegistrationRecord {
            id: id.clone(),
            probe,
            priority: resolution_priority,
            tags,
        });
        for magic in payload_magics {
            // A zero-length prefix would match every stream while
            // carrying no evidence — drop it here so resolution never
            // has to special-case it.
            if !magic.is_empty() {
                self.magic_index.push((magic, record_idx));
            }
        }
    }

    /// Whether at least one registered implementation of `id` can decode.
    pub fn has_decoder(&self, id: &CodecId) -> bool {
        self.impls
            .get(id)
            .map(|v| v.iter().any(|i| i.make_decoder.is_some()))
            .unwrap_or(false)
    }

    /// Whether at least one registered implementation of `id` can encode.
    pub fn has_encoder(&self, id: &CodecId) -> bool {
        self.impls
            .get(id)
            .map(|v| v.iter().any(|i| i.make_encoder.is_some()))
            .unwrap_or(false)
    }

    /// First registered decoder factory for `params.codec_id`, invoked
    /// with `params`. No priority walk, no preference filter, no
    /// init-time fallback to a lower-priority impl. Errors if no
    /// decoder is registered for the codec.
    ///
    /// Intended for single-impl scenarios — typically a codec crate's
    /// own self-tests, where exactly one impl has been registered into
    /// a freshly-constructed registry. Production callers selecting
    /// among multiple candidates (e.g. h264_sw vs h264_videotoolbox)
    /// should use `oxideav_pipeline::make_decoder_with` instead, which
    /// applies `CodecPreferences` and walks priorities.
    pub fn first_decoder(&self, params: &CodecParameters) -> Result<Box<dyn Decoder>> {
        let imp = self
            .implementations(&params.codec_id)
            .iter()
            .find(|i| i.make_decoder.is_some())
            .ok_or_else(|| {
                Error::CodecNotFound(format!("no decoder for codec {}", params.codec_id))
            })?;
        (imp.make_decoder.expect("checked above"))(params)
    }

    /// First registered encoder factory — see [`first_decoder`].
    ///
    /// [`first_decoder`]: Self::first_decoder
    pub fn first_encoder(&self, params: &CodecParameters) -> Result<Box<dyn Encoder>> {
        let imp = self
            .implementations(&params.codec_id)
            .iter()
            .find(|i| i.make_encoder.is_some())
            .ok_or_else(|| {
                Error::CodecNotFound(format!("no encoder for codec {}", params.codec_id))
            })?;
        (imp.make_encoder.expect("checked above"))(params)
    }

    /// Look up a decoder by exact implementation name
    /// (`"h264_sw"`, `"aac_audiotoolbox"`, ...). Errors if the impl
    /// isn't registered or if it has no decoder factory.
    pub fn decoder_by_impl(
        &self,
        impl_name: &str,
        params: &CodecParameters,
    ) -> Result<Box<dyn Decoder>> {
        let imp = self
            .implementations(&params.codec_id)
            .iter()
            .find(|i| i.caps.implementation == impl_name)
            .ok_or_else(|| {
                Error::CodecNotFound(format!(
                    "no implementation `{impl_name}` for codec {}",
                    params.codec_id
                ))
            })?;
        let factory = imp
            .make_decoder
            .ok_or_else(|| Error::CodecNotFound(format!("`{impl_name}` is encoder-only")))?;
        factory(params)
    }

    /// Look up an encoder by exact implementation name — see
    /// [`decoder_by_impl`].
    ///
    /// [`decoder_by_impl`]: Self::decoder_by_impl
    pub fn encoder_by_impl(
        &self,
        impl_name: &str,
        params: &CodecParameters,
    ) -> Result<Box<dyn Encoder>> {
        let imp = self
            .implementations(&params.codec_id)
            .iter()
            .find(|i| i.caps.implementation == impl_name)
            .ok_or_else(|| {
                Error::CodecNotFound(format!(
                    "no implementation `{impl_name}` for codec {}",
                    params.codec_id
                ))
            })?;
        let factory = imp
            .make_encoder
            .ok_or_else(|| Error::CodecNotFound(format!("`{impl_name}` is decoder-only")))?;
        factory(params)
    }

    /// Iterate codec ids that have at least one decoder implementation,
    /// in the order each id's first implementation was registered.
    pub fn decoder_ids(&self) -> impl Iterator<Item = &CodecId> {
        self.impl_order.iter().filter(|id| self.has_decoder(id))
    }

    /// Iterate codec ids that have at least one encoder implementation,
    /// in the order each id's first implementation was registered.
    pub fn encoder_ids(&self) -> impl Iterator<Item = &CodecId> {
        self.impl_order.iter().filter(|id| self.has_encoder(id))
    }

    /// All registered implementations of a given codec id.
    pub fn implementations(&self, id: &CodecId) -> &[CodecImplementation] {
        self.impls.get(id).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Lookup the encoder options schema for a registered codec. Walks
    /// implementations in registration order and returns the first
    /// schema found. `None` means either the codec isn't registered or
    /// no implementation declared an encoder schema.
    pub fn encoder_options_schema(&self, id: &CodecId) -> Option<&'static [OptionField]> {
        self.impls
            .get(id)?
            .iter()
            .find_map(|i| i.encoder_options_schema)
    }

    /// Lookup the decoder options schema — see
    /// [`encoder_options_schema`](Self::encoder_options_schema).
    pub fn decoder_options_schema(&self, id: &CodecId) -> Option<&'static [OptionField]> {
        self.impls
            .get(id)?
            .iter()
            .find_map(|i| i.decoder_options_schema)
    }

    /// Iterator over every (codec_id, impl) pair — useful for `oxideav list`
    /// to show capability flags per implementation. Ids come in the
    /// order their first implementation was registered; implementations
    /// of one id in their own registration order.
    pub fn all_implementations(&self) -> impl Iterator<Item = (&CodecId, &CodecImplementation)> {
        self.impl_order
            .iter()
            .flat_map(move |id| self.implementations(id).iter().map(move |i| (id, i)))
    }

    /// Iterator over every `(tag, codec_id)` pair currently registered —
    /// used by `oxideav tags` debug output and by tests that want to
    /// walk the tag surface. Registration order, then each
    /// registration's tag declaration order.
    pub fn all_tag_registrations(&self) -> impl Iterator<Item = (&CodecTag, &CodecId)> {
        self.registrations
            .iter()
            .flat_map(|rec| rec.tags.iter().map(move |tag| (tag, &rec.id)))
    }

    /// Inherent form of tag resolution that returns a reference.
    /// The owned-value form used by container code lives behind the
    /// [`CodecResolver`] trait impl below.
    ///
    /// Walks every registration that claimed `ctx.tag`, calls its
    /// probe with `ctx`, and returns the id of the registration that
    /// scored highest. Probes that return `0.0` (or anything not
    /// strictly positive, `NaN` included) are discarded; ties on
    /// confidence are broken by [`CodecInfo::resolution_priority`]
    /// (lower wins), then by registration order (first wins).
    /// Registrations with no probe are treated as returning `1.0`.
    /// [`resolve_tag_candidates`](Self::resolve_tag_candidates) returns
    /// the whole ranked list this picks the head of.
    pub fn resolve_tag_ref(&self, ctx: &ProbeContext) -> Option<&CodecId> {
        self.resolve_tag_candidates(ctx).first().map(|c| c.id)
    }

    /// Every registration that claimed `ctx.tag` and whose probe
    /// accepts the context, ranked by the resolution rule: confidence
    /// descending, then priority ascending, then registration order
    /// ascending. Empty when the tag is unclaimed or every claimant's
    /// probe refused. The observation hook for collision audits: a
    /// list with two heads at equal confidence *and* equal priority is
    /// a tie the registry broke by registration order alone.
    pub fn resolve_tag_candidates(&self, ctx: &ProbeContext) -> Vec<TagCandidate<'_>> {
        let Some(idxs) = self.tag_index.get(ctx.tag) else {
            return Vec::new();
        };
        let mut out: Vec<TagCandidate<'_>> = idxs
            .iter()
            .filter_map(|&i| {
                let rec = &self.registrations[i];
                let confidence = match rec.probe {
                    Some(f) => f(ctx),
                    None => 1.0,
                };
                // `> 0.0` is false for NaN, so a broken probe is
                // dropped rather than ranked unpredictably.
                (confidence > 0.0).then_some(TagCandidate {
                    id: &rec.id,
                    confidence,
                    priority: rec.priority,
                    order: i,
                })
            })
            .collect();
        // `idxs` is already in registration order and `order` is
        // unique, so the sort key is total and the result is fixed by
        // the registrations alone.
        out.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.priority.cmp(&b.priority))
                .then(a.order.cmp(&b.order))
        });
        out
    }

    /// Inherent form of payload-magic resolution that returns a
    /// reference. The owned-value form used by container code lives
    /// behind the [`CodecResolver`] trait impl below.
    ///
    /// Walks every registered payload magic prefix (declared via
    /// [`CodecInfo::payload_magic`] / [`CodecInfo::payload_magics`]) and
    /// returns the codec whose magic is a prefix of `first_bytes` —
    /// however much of the stream's leading payload the caller has
    /// (an Ogg demuxer passes the first packet of a logical stream; a
    /// raw-stream prober passes the file head). The **longest**
    /// matching magic wins (most specific claim); remaining ties are
    /// broken by [`CodecInfo::resolution_priority`] (lower wins), then
    /// by registration order (first wins). Unlike the tag path there
    /// is no probe step: a payload magic is itself the bitstream
    /// evidence a probe would look for, and specificity is expressed
    /// by prefix length instead of a confidence value.
    /// [`resolve_payload_magic_candidates`](Self::resolve_payload_magic_candidates)
    /// returns the whole ranked list this picks the head of.
    pub fn resolve_payload_magic_ref(&self, first_bytes: &[u8]) -> Option<&CodecId> {
        self.resolve_payload_magic_candidates(first_bytes)
            .first()
            .map(|c| c.id)
    }

    /// Every registered payload magic that is a prefix of
    /// `first_bytes`, ranked by the resolution rule: magic length
    /// descending, then priority ascending, then registration order
    /// ascending. The payload-magic companion to
    /// [`resolve_tag_candidates`](Self::resolve_tag_candidates).
    pub fn resolve_payload_magic_candidates(
        &self,
        first_bytes: &[u8],
    ) -> Vec<PayloadMagicCandidate<'_>> {
        let mut out: Vec<PayloadMagicCandidate<'_>> = self
            .magic_index
            .iter()
            .filter(|(magic, _)| first_bytes.starts_with(magic))
            .map(|(magic, i)| {
                let rec = &self.registrations[*i];
                PayloadMagicCandidate {
                    id: &rec.id,
                    magic: magic.as_slice(),
                    priority: rec.priority,
                    order: *i,
                }
            })
            .collect();
        // One registration may claim several magics that all match
        // (`order` then repeats); the stable sort keeps those in their
        // declaration order, which is the only remaining distinction
        // and is itself fixed by the registration.
        out.sort_by(|a, b| {
            b.magic
                .len()
                .cmp(&a.magic.len())
                .then(a.priority.cmp(&b.priority))
                .then(a.order.cmp(&b.order))
        });
        out
    }

    /// Iterator over every `(payload magic, codec_id)` pair currently
    /// registered, in registration order — the payload-magic companion
    /// to [`all_tag_registrations`](Self::all_tag_registrations).
    pub fn all_payload_magic_registrations(&self) -> impl Iterator<Item = (&[u8], &CodecId)> {
        self.magic_index
            .iter()
            .map(move |(magic, i)| (magic.as_slice(), &self.registrations[*i].id))
    }
}

/// Implement the shared [`CodecResolver`] interface so container
/// demuxers can accept `&dyn CodecResolver` without depending on
/// this crate directly — the trait lives in oxideav-core.
impl CodecResolver for CodecRegistry {
    fn resolve_tag(&self, ctx: &ProbeContext) -> Option<CodecId> {
        self.resolve_tag_ref(ctx).cloned()
    }

    fn resolve_payload_magic(&self, first_packet: &[u8]) -> Option<CodecId> {
        self.resolve_payload_magic_ref(first_packet).cloned()
    }
}

#[cfg(test)]
mod tag_tests {
    use super::*;
    use crate::CodecCapabilities;

    /// Probe: return 1.0 iff the peeked bytes look like MS-MPEG4 (no
    /// 0x000001 start code in the first few bytes).
    fn probe_msmpeg4(ctx: &ProbeContext) -> f32 {
        match ctx.packet {
            Some(d) if !d.windows(3).take(6).any(|w| w == [0x00, 0x00, 0x01]) => 1.0,
            Some(_) => 0.0,
            None => 0.5, // no data yet — weak evidence
        }
    }

    /// Probe: return 1.0 iff the peeked bytes look like MPEG-4 Part 2
    /// (starts with a 0x000001 start code in the first few bytes).
    fn probe_mpeg4_part2(ctx: &ProbeContext) -> f32 {
        match ctx.packet {
            Some(d) if d.windows(3).take(6).any(|w| w == [0x00, 0x00, 0x01]) => 1.0,
            Some(_) => 0.0,
            None => 0.5,
        }
    }

    fn info(id: &str) -> CodecInfo {
        CodecInfo::new(CodecId::new(id)).capabilities(CodecCapabilities::audio(id))
    }

    #[test]
    fn resolve_single_claim_no_probe() {
        let mut reg = CodecRegistry::new();
        reg.register(info("flac").tag(CodecTag::fourcc(b"FLAC")));
        let t = CodecTag::fourcc(b"FLAC");
        assert_eq!(
            reg.resolve_tag_ref(&ProbeContext::new(&t))
                .map(|c| c.as_str()),
            Some("flac"),
        );
    }

    #[test]
    fn resolve_missing_tag_returns_none() {
        let reg = CodecRegistry::new();
        let t = CodecTag::fourcc(b"????");
        assert!(reg.resolve_tag_ref(&ProbeContext::new(&t)).is_none());
    }

    #[test]
    fn unprobed_claims_tie_first_registered_wins() {
        // Two unprobed claims on the same tag: deterministic order.
        let mut reg = CodecRegistry::new();
        reg.register(info("first").tag(CodecTag::fourcc(b"TEST")));
        reg.register(info("second").tag(CodecTag::fourcc(b"TEST")));
        let t = CodecTag::fourcc(b"TEST");
        assert_eq!(
            reg.resolve_tag_ref(&ProbeContext::new(&t))
                .map(|c| c.as_str()),
            Some("first"),
        );
    }

    #[test]
    fn probe_picks_matching_bitstream() {
        // The core bug fix: every probe is asked and the highest
        // confidence wins regardless of registration order.
        let mut reg = CodecRegistry::new();
        reg.register(
            info("msmpeg4v3")
                .probe(probe_msmpeg4)
                .tag(CodecTag::fourcc(b"DIV3")),
        );
        reg.register(
            info("mpeg4video")
                .probe(probe_mpeg4_part2)
                .tag(CodecTag::fourcc(b"DIV3")),
        );

        let mpeg4_part2 = [0x00u8, 0x00, 0x01, 0xB0, 0x01, 0x00];
        let ms_mpeg4 = [0x85u8, 0x3F, 0xD4, 0x80, 0x00, 0xA2];
        let tag = CodecTag::fourcc(b"DIV3");

        let ctx_part2 = ProbeContext::new(&tag).packet(&mpeg4_part2);
        assert_eq!(
            reg.resolve_tag_ref(&ctx_part2).map(|c| c.as_str()),
            Some("mpeg4video"),
        );
        let ctx_ms = ProbeContext::new(&tag).packet(&ms_mpeg4);
        assert_eq!(
            reg.resolve_tag_ref(&ctx_ms).map(|c| c.as_str()),
            Some("msmpeg4v3"),
        );
    }

    #[test]
    fn unprobed_claim_wins_against_low_confidence_probe() {
        // One codec claims a tag without a probe (→ confidence 1.0)
        // and another claims it with a probe returning 0.3. The
        // unprobed one wins — a codec that knows it owns the tag
        // outright should not lose to a speculative probe.
        let mut reg = CodecRegistry::new();
        reg.register(info("owner").tag(CodecTag::fourcc(b"OWN_")));
        reg.register(
            info("speculative")
                .probe(|_| 0.3)
                .tag(CodecTag::fourcc(b"OWN_")),
        );
        let t = CodecTag::fourcc(b"OWN_");
        assert_eq!(
            reg.resolve_tag_ref(&ProbeContext::new(&t))
                .map(|c| c.as_str()),
            Some("owner"),
        );
    }

    #[test]
    fn probe_returning_zero_is_skipped() {
        let mut reg = CodecRegistry::new();
        reg.register(
            info("refuses")
                .probe(|_| 0.0)
                .tag(CodecTag::fourcc(b"MAYB")),
        );
        reg.register(info("fallback").tag(CodecTag::fourcc(b"MAYB")));
        let t = CodecTag::fourcc(b"MAYB");
        let ctx = ProbeContext::new(&t).packet(b"hello");
        assert_eq!(
            reg.resolve_tag_ref(&ctx).map(|c| c.as_str()),
            Some("fallback"),
        );
    }

    #[test]
    fn fourcc_case_insensitive_lookup() {
        let mut reg = CodecRegistry::new();
        reg.register(info("vid").tag(CodecTag::fourcc(b"div3")));
        // Registered as "DIV3" (uppercase via ctor); lookup using
        // lowercase / mixed case also hits.
        let upper = CodecTag::fourcc(b"DIV3");
        let lower = CodecTag::fourcc(b"div3");
        let mixed = CodecTag::fourcc(b"DiV3");
        assert!(reg.resolve_tag_ref(&ProbeContext::new(&upper)).is_some());
        assert!(reg.resolve_tag_ref(&ProbeContext::new(&lower)).is_some());
        assert!(reg.resolve_tag_ref(&ProbeContext::new(&mixed)).is_some());
    }

    #[test]
    fn wave_format_and_matroska_tags_work() {
        let mut reg = CodecRegistry::new();
        reg.register(info("mp3").tag(CodecTag::wave_format(0x0055)));
        reg.register(info("h264").tag(CodecTag::matroska("V_MPEG4/ISO/AVC")));
        let wf = CodecTag::wave_format(0x0055);
        let mk = CodecTag::matroska("V_MPEG4/ISO/AVC");
        assert_eq!(
            reg.resolve_tag_ref(&ProbeContext::new(&wf))
                .map(|c| c.as_str()),
            Some("mp3"),
        );
        assert_eq!(
            reg.resolve_tag_ref(&ProbeContext::new(&mk))
                .map(|c| c.as_str()),
            Some("h264"),
        );
    }

    #[test]
    fn mp4_object_type_tag_works() {
        let mut reg = CodecRegistry::new();
        reg.register(info("aac").tag(CodecTag::mp4_object_type(0x40)));
        let t = CodecTag::mp4_object_type(0x40);
        assert_eq!(
            reg.resolve_tag_ref(&ProbeContext::new(&t))
                .map(|c| c.as_str()),
            Some("aac"),
        );
    }

    #[test]
    fn multi_tag_claim_all_resolve() {
        let mut reg = CodecRegistry::new();
        reg.register(info("aac").tags([
            CodecTag::fourcc(b"MP4A"),
            CodecTag::wave_format(0x00FF),
            CodecTag::mp4_object_type(0x40),
            CodecTag::matroska("A_AAC"),
        ]));
        for t in [
            CodecTag::fourcc(b"MP4A"),
            CodecTag::wave_format(0x00FF),
            CodecTag::mp4_object_type(0x40),
            CodecTag::matroska("A_AAC"),
        ] {
            assert_eq!(
                reg.resolve_tag_ref(&ProbeContext::new(&t))
                    .map(|c| c.as_str()),
                Some("aac"),
                "tag {t:?} did not resolve",
            );
        }
    }
}

#[cfg(test)]
mod payload_magic_tests {
    use super::*;
    use crate::CodecCapabilities;

    fn info(id: &str) -> CodecInfo {
        CodecInfo::new(CodecId::new(id)).capabilities(CodecCapabilities::audio(id))
    }

    /// Registry with the classic Ogg family registered, each under its
    /// real BOS magic.
    fn ogg_family_registry() -> CodecRegistry {
        let mut reg = CodecRegistry::new();
        reg.register(info("vorbis").payload_magic(b"\x01vorbis"));
        reg.register(info("opus").payload_magic(b"OpusHead"));
        reg.register(info("theora").payload_magic(b"\x80theora"));
        reg.register(info("flac").payload_magic(b"\x7fFLAC"));
        reg
    }

    #[test]
    fn resolve_payload_magic_matches_first_packet_prefix() {
        let reg = ogg_family_registry();
        // A Vorbis identification header: magic + version + channels +
        // rate + ... — the resolver only needs the prefix to match.
        let vorbis_id_header = b"\x01vorbis\x00\x00\x00\x00\x02\x44\xac\x00\x00";
        assert_eq!(
            reg.resolve_payload_magic_ref(vorbis_id_header)
                .map(|c| c.as_str()),
            Some("vorbis"),
        );
        assert_eq!(
            reg.resolve_payload_magic_ref(b"OpusHead\x01\x02\x38\x01")
                .map(|c| c.as_str()),
            Some("opus"),
        );
        assert_eq!(
            reg.resolve_payload_magic_ref(b"\x80theora\x03\x02\x01")
                .map(|c| c.as_str()),
            Some("theora"),
        );
        assert_eq!(
            reg.resolve_payload_magic_ref(b"\x7fFLAC\x01\x00")
                .map(|c| c.as_str()),
            Some("flac"),
        );
    }

    #[test]
    fn resolve_payload_magic_exact_length_packet_matches() {
        // A packet that is exactly the magic (nothing after it) still
        // resolves — starts_with is inclusive of equality.
        let reg = ogg_family_registry();
        assert_eq!(
            reg.resolve_payload_magic_ref(b"OpusHead")
                .map(|c| c.as_str()),
            Some("opus"),
        );
    }

    #[test]
    fn resolve_payload_magic_unknown_or_short_packet_is_none() {
        let reg = ogg_family_registry();
        // Unknown magic.
        assert!(reg.resolve_payload_magic_ref(b"Speex   1.2.0").is_none());
        // Packet shorter than every registered magic.
        assert!(reg.resolve_payload_magic_ref(b"Opus").is_none());
        // Empty packet.
        assert!(reg.resolve_payload_magic_ref(b"").is_none());
    }

    #[test]
    fn resolve_payload_magic_longest_prefix_wins_regardless_of_order() {
        // A shorter magic that is itself a prefix of a longer one must
        // lose to the more specific claim, whichever registered first.
        let mut reg = CodecRegistry::new();
        reg.register(info("generic").payload_magic(b"Opus"));
        reg.register(info("opus").payload_magic(b"OpusHead"));
        assert_eq!(
            reg.resolve_payload_magic_ref(b"OpusHead\x01")
                .map(|c| c.as_str()),
            Some("opus"),
        );
        // ...but the shorter claim still wins packets only it matches.
        assert_eq!(
            reg.resolve_payload_magic_ref(b"OpusTags")
                .map(|c| c.as_str()),
            Some("generic"),
        );

        // Same result with the registration order flipped.
        let mut reg = CodecRegistry::new();
        reg.register(info("opus").payload_magic(b"OpusHead"));
        reg.register(info("generic").payload_magic(b"Opus"));
        assert_eq!(
            reg.resolve_payload_magic_ref(b"OpusHead\x01")
                .map(|c| c.as_str()),
            Some("opus"),
        );
    }

    #[test]
    fn resolve_payload_magic_equal_length_tie_first_registered_wins() {
        let mut reg = CodecRegistry::new();
        reg.register(info("first").payload_magic(b"SameMagic"));
        reg.register(info("second").payload_magic(b"SameMagic"));
        assert_eq!(
            reg.resolve_payload_magic_ref(b"SameMagic\x00")
                .map(|c| c.as_str()),
            Some("first"),
        );
    }

    #[test]
    fn empty_payload_magic_is_ignored_at_registration() {
        let mut reg = CodecRegistry::new();
        reg.register(info("greedy").payload_magic(b""));
        assert!(reg.resolve_payload_magic_ref(b"anything at all").is_none());
        assert_eq!(reg.all_payload_magic_registrations().count(), 0);
    }

    #[test]
    fn payload_magics_plural_builder_and_diagnostics() {
        // One codec answering to several magics via the iterable
        // builder; the diagnostic iterator surfaces each claim in
        // registration order.
        let mut reg = CodecRegistry::new();
        reg.register(info("speex").payload_magics([b"Speex   ".to_vec(), b"speex-alt".to_vec()]));
        assert_eq!(
            reg.resolve_payload_magic_ref(b"Speex   1.2")
                .map(|c| c.as_str()),
            Some("speex"),
        );
        assert_eq!(
            reg.resolve_payload_magic_ref(b"speex-alt\x00")
                .map(|c| c.as_str()),
            Some("speex"),
        );
        let all: Vec<(&[u8], &str)> = reg
            .all_payload_magic_registrations()
            .map(|(m, id)| (m, id.as_str()))
            .collect();
        assert_eq!(
            all,
            vec![
                (b"Speex   ".as_slice(), "speex"),
                (b"speex-alt".as_slice(), "speex"),
            ],
        );
    }

    #[test]
    fn magic_claims_compose_with_tag_claims_on_one_registration() {
        // A codec that lives in both Ogg and Matroska declares both
        // claim kinds on one CodecInfo; each resolution path finds it.
        let mut reg = CodecRegistry::new();
        reg.register(
            info("vorbis")
                .tag(CodecTag::matroska("A_VORBIS"))
                .payload_magic(b"\x01vorbis"),
        );
        let mk = CodecTag::matroska("A_VORBIS");
        assert_eq!(
            reg.resolve_tag_ref(&ProbeContext::new(&mk))
                .map(|c| c.as_str()),
            Some("vorbis"),
        );
        assert_eq!(
            reg.resolve_payload_magic_ref(b"\x01vorbis\x00")
                .map(|c| c.as_str()),
            Some("vorbis"),
        );
    }

    #[test]
    fn resolver_trait_payload_magic_surface() {
        // The owned-value trait form mirrors the inherent form, and
        // the default implementation (NullCodecResolver) resolves
        // nothing.
        let reg = ogg_family_registry();
        let resolver: &dyn CodecResolver = &reg;
        assert_eq!(
            resolver
                .resolve_payload_magic(b"OpusHead\x01")
                .map(|c| c.0.clone()),
            Some("opus".to_owned()),
        );
        assert!(resolver.resolve_payload_magic(b"unknown").is_none());

        let null = crate::NullCodecResolver;
        assert!(null.resolve_payload_magic(b"OpusHead\x01").is_none());
    }

    /// The surface is container-agnostic: a raw elementary stream
    /// identified by a file-head magic resolves through the same path
    /// as the Ogg family — nothing about the mechanism is Ogg-shaped.
    #[test]
    fn payload_magic_serves_non_ogg_carriage() {
        let mut reg = CodecRegistry::new();
        reg.register(info("flac").payload_magic(b"fLaC"));
        reg.register(info("shorten").payload_magic(b"ajkg"));

        assert_eq!(
            reg.resolve_payload_magic_ref(b"fLaC\x00\x00\x00\x22"),
            Some(&CodecId::new("flac"))
        );
        assert_eq!(
            reg.resolve_payload_magic_ref(b"ajkg\x02"),
            Some(&CodecId::new("shorten"))
        );
        assert_eq!(reg.resolve_payload_magic_ref(b"RIFF"), None);
    }
}

/// Resolution-order tests. Every id / tag / magic here is invented:
/// the rule is format-agnostic and must be provable with synthetic
/// registrations only.
#[cfg(test)]
mod resolution_order_tests {
    use super::*;
    use crate::CodecCapabilities;

    fn info(id: &str) -> CodecInfo {
        CodecInfo::new(CodecId::new(id)).capabilities(CodecCapabilities::audio(id))
    }

    fn tag_ids<'a>(cands: &[TagCandidate<'a>]) -> Vec<&'a str> {
        cands.iter().map(|c| c.id.as_str()).collect()
    }

    fn magic_ids<'a>(cands: &[PayloadMagicCandidate<'a>]) -> Vec<&'a str> {
        cands.iter().map(|c| c.id.as_str()).collect()
    }

    #[test]
    fn codec_info_resolution_priority_defaults_and_builder() {
        assert_eq!(info("x").resolution_priority, crate::DEFAULT_PRIORITY);
        assert_eq!(info("x").with_resolution_priority(7).resolution_priority, 7);
    }

    #[test]
    fn equal_tag_claims_resolve_by_registration_order_across_fresh_contexts() {
        const ORDER: [&str; 5] = ["zeta", "alpha", "omicron", "beta", "kappa"];
        let tag = CodecTag::fourcc(b"SYNT");
        for _ in 0..1000 {
            let mut ctx = crate::RuntimeContext::new();
            for id in ORDER {
                ctx.codecs.register(info(id).tag(tag.clone()));
            }
            let pc = ProbeContext::new(&tag);
            assert_eq!(
                ctx.codecs.resolve_tag_ref(&pc).map(|c| c.as_str()),
                Some("zeta")
            );
            let cands = ctx.codecs.resolve_tag_candidates(&pc);
            assert_eq!(tag_ids(&cands), ORDER.to_vec());
            assert_eq!(
                cands.iter().map(|c| c.order).collect::<Vec<_>>(),
                vec![0, 1, 2, 3, 4]
            );
        }
    }

    #[test]
    fn equal_magic_claims_resolve_by_registration_order_across_fresh_contexts() {
        const ORDER: [&str; 4] = ["zeta", "alpha", "omicron", "beta"];
        for _ in 0..1000 {
            let mut ctx = crate::RuntimeContext::new();
            for id in ORDER {
                ctx.codecs.register(info(id).payload_magic(b"SYNTHMAGIC"));
            }
            assert_eq!(
                ctx.codecs
                    .resolve_payload_magic_ref(b"SYNTHMAGIC\x00")
                    .map(|c| c.as_str()),
                Some("zeta")
            );
            let cands = ctx
                .codecs
                .resolve_payload_magic_candidates(b"SYNTHMAGIC\x00");
            assert_eq!(magic_ids(&cands), ORDER.to_vec());
        }
    }

    #[test]
    fn tag_priority_beats_registration_order() {
        let tag = CodecTag::wave_format(0x7777);
        let mut reg = CodecRegistry::new();
        reg.register(info("earlier").tag(tag.clone()));
        reg.register(
            info("later-preferred")
                .with_resolution_priority(10)
                .tag(tag.clone()),
        );
        let pc = ProbeContext::new(&tag);
        assert_eq!(
            reg.resolve_tag_ref(&pc).map(|c| c.as_str()),
            Some("later-preferred")
        );
        let cands = reg.resolve_tag_candidates(&pc);
        assert_eq!(tag_ids(&cands), vec!["later-preferred", "earlier"]);
        assert_eq!(cands[0].priority, 10);
        assert_eq!(cands[0].order, 1);
        assert_eq!(cands[1].priority, crate::DEFAULT_PRIORITY);
        assert_eq!(cands[1].order, 0);
        assert_eq!(cands[0].confidence, 1.0);
    }

    #[test]
    fn tag_confidence_beats_priority() {
        // A probe that positively identifies the payload wins against
        // an unprobed-but-top-priority claim only when its confidence
        // is higher; at 1.0 vs 1.0 the priority decides.
        let tag = CodecTag::fourcc(b"AMBI");
        let mut reg = CodecRegistry::new();
        reg.register(
            info("prioritised")
                .with_resolution_priority(i32::MIN)
                .probe(|_| 0.6)
                .tag(tag.clone()),
        );
        reg.register(info("certain").tag(tag.clone()));
        let pc = ProbeContext::new(&tag);
        assert_eq!(
            reg.resolve_tag_ref(&pc).map(|c| c.as_str()),
            Some("certain")
        );
        let cands = reg.resolve_tag_candidates(&pc);
        assert_eq!(tag_ids(&cands), vec!["certain", "prioritised"]);
    }

    #[test]
    fn tag_candidates_drop_refusing_and_non_finite_probes() {
        let tag = CodecTag::fourcc(b"NANP");
        let mut reg = CodecRegistry::new();
        reg.register(info("refuses").probe(|_| 0.0).tag(tag.clone()));
        reg.register(info("broken").probe(|_| f32::NAN).tag(tag.clone()));
        reg.register(info("negative").probe(|_| -1.0).tag(tag.clone()));
        reg.register(info("accepts").probe(|_| 0.2).tag(tag.clone()));
        let pc = ProbeContext::new(&tag);
        assert_eq!(tag_ids(&reg.resolve_tag_candidates(&pc)), vec!["accepts"]);
        assert_eq!(
            reg.resolve_tag_ref(&pc).map(|c| c.as_str()),
            Some("accepts")
        );
        // An unclaimed tag yields an empty list, not an error.
        let other = CodecTag::fourcc(b"NONE");
        assert!(
            reg.resolve_tag_candidates(&ProbeContext::new(&other))
                .is_empty()
        );
    }

    #[test]
    fn duplicate_tag_claim_by_one_id_is_visible_not_folded() {
        // The registry contract allows registering one id several
        // times (multi-implementation codecs). A tag claimed twice by
        // the same id resolves to that id either way; the candidate
        // list shows both claims so an audit can flag the redundancy.
        let tag = CodecTag::fourcc(b"TWIC");
        let mut reg = CodecRegistry::new();
        reg.register(info("dup").tag(tag.clone()));
        reg.register(info("other").tag(tag.clone()));
        reg.register(info("dup").tag(tag.clone()));
        let pc = ProbeContext::new(&tag);
        let cands = reg.resolve_tag_candidates(&pc);
        assert_eq!(tag_ids(&cands), vec!["dup", "other", "dup"]);
        assert_eq!(
            cands.iter().map(|c| c.order).collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
        assert_eq!(reg.resolve_tag_ref(&pc).map(|c| c.as_str()), Some("dup"));
    }

    #[test]
    fn magic_priority_beats_registration_order_but_not_length() {
        let mut reg = CodecRegistry::new();
        reg.register(info("earlier").payload_magic(b"MAGIC"));
        reg.register(
            info("later-preferred")
                .with_resolution_priority(1)
                .payload_magic(b"MAGIC"),
        );
        reg.register(info("longer").payload_magic(b"MAGICAL"));
        // Equal-length tie: priority decides.
        let cands = reg.resolve_payload_magic_candidates(b"MAGIC\x00");
        assert_eq!(magic_ids(&cands), vec!["later-preferred", "earlier"]);
        assert_eq!(cands[0].order, 1);
        assert_eq!(cands[0].magic, b"MAGIC");
        // A longer matching magic beats both regardless of priority.
        let cands = reg.resolve_payload_magic_candidates(b"MAGICAL\x00");
        assert_eq!(
            magic_ids(&cands),
            vec!["longer", "later-preferred", "earlier"]
        );
        assert_eq!(
            reg.resolve_payload_magic_ref(b"MAGICAL\x00")
                .map(|c| c.as_str()),
            Some("longer")
        );
        // Nothing matches: empty list, None.
        assert!(reg.resolve_payload_magic_candidates(b"other").is_empty());
        assert!(reg.resolve_payload_magic_ref(b"other").is_none());
    }

    fn dec(_p: &crate::CodecParameters) -> crate::Result<Box<dyn Decoder>> {
        Err(Error::unsupported("dummy"))
    }
    fn enc(_p: &crate::CodecParameters) -> crate::Result<Box<dyn Encoder>> {
        Err(Error::unsupported("dummy"))
    }

    #[test]
    fn enumerations_follow_registration_order() {
        for _ in 0..200 {
            let mut reg = CodecRegistry::new();
            reg.register(
                info("zeta")
                    .decoder(dec)
                    .tags([CodecTag::fourcc(b"ZET1"), CodecTag::fourcc(b"ZET2")]),
            );
            reg.register(info("alpha").encoder(enc).tag(CodecTag::fourcc(b"ALPH")));
            reg.register(info("omicron").decoder(dec).encoder(enc));
            // Second impl of an existing id does not move it.
            reg.register(info("zeta").encoder(enc).tag(CodecTag::fourcc(b"ZET3")));
            // Tag-only registration: no impl, but its tags are listed.
            reg.register(info("beta").tag(CodecTag::fourcc(b"BETA")));

            let ids: Vec<&str> = reg
                .all_implementations()
                .map(|(id, _)| id.as_str())
                .collect();
            assert_eq!(ids, vec!["zeta", "zeta", "alpha", "omicron"]);
            let dec_ids: Vec<&str> = reg.decoder_ids().map(|i| i.as_str()).collect();
            assert_eq!(dec_ids, vec!["zeta", "omicron"]);
            let enc_ids: Vec<&str> = reg.encoder_ids().map(|i| i.as_str()).collect();
            assert_eq!(enc_ids, vec!["zeta", "alpha", "omicron"]);
            let tags: Vec<(String, &str)> = reg
                .all_tag_registrations()
                .map(|(t, id)| (format!("{t:?}"), id.as_str()))
                .collect();
            let want: Vec<(String, &str)> = [
                (CodecTag::fourcc(b"ZET1"), "zeta"),
                (CodecTag::fourcc(b"ZET2"), "zeta"),
                (CodecTag::fourcc(b"ALPH"), "alpha"),
                (CodecTag::fourcc(b"ZET3"), "zeta"),
                (CodecTag::fourcc(b"BETA"), "beta"),
            ]
            .into_iter()
            .map(|(t, id)| (format!("{t:?}"), id))
            .collect();
            assert_eq!(tags, want);
        }
    }

    #[test]
    fn resolution_priority_is_independent_of_capabilities_priority() {
        // A registration that ranks itself first as an *implementation*
        // (HW-style capabilities priority) does not thereby win a tag
        // tie against a different codec id registered earlier.
        let tag = CodecTag::fourcc(b"SHRD");
        let mut reg = CodecRegistry::new();
        reg.register(info("sw-id").tag(tag.clone()));
        reg.register(
            CodecInfo::new(CodecId::new("hw-id"))
                .capabilities(CodecCapabilities::video("hw-id").with_priority(10))
                .tag(tag.clone()),
        );
        let pc = ProbeContext::new(&tag);
        assert_eq!(reg.resolve_tag_ref(&pc).map(|c| c.as_str()), Some("sw-id"));
        let cands = reg.resolve_tag_candidates(&pc);
        assert!(cands.iter().all(|c| c.priority == crate::DEFAULT_PRIORITY));
    }
}

#[cfg(test)]
mod engine_tests {
    use super::*;
    use crate::engine::HwDeviceInfo;

    #[test]
    fn codec_info_engine_id_and_probe_default_to_none() {
        let ci = CodecInfo::new(CodecId::new("h264"));
        assert!(ci.engine_id.is_none());
        assert!(ci.engine_probe.is_none());
    }

    #[test]
    fn codec_info_engine_builder_methods_set_fields() {
        fn dummy_probe() -> Vec<HwDeviceInfo> {
            vec![]
        }
        let ci = CodecInfo::new(CodecId::new("h264"))
            .with_engine_id("nvidia")
            .with_engine_probe(dummy_probe);
        assert_eq!(ci.engine_id, Some("nvidia"));
        assert!(ci.engine_probe.is_some());
        let probe = ci.engine_probe.unwrap();
        let result = probe();
        assert!(result.is_empty());
    }

    #[test]
    fn registering_codec_with_engine_metadata_does_not_panic() {
        // The new fields are passthrough metadata — register() should
        // accept them without affecting existing id/tag bookkeeping.
        fn dummy_probe() -> Vec<HwDeviceInfo> {
            vec![]
        }
        let mut reg = CodecRegistry::new();
        reg.register(
            CodecInfo::new(CodecId::new("h264"))
                .capabilities(CodecCapabilities::audio("h264_nvdec"))
                .tag(CodecTag::fourcc(b"H264"))
                .with_engine_id("nvidia")
                .with_engine_probe(dummy_probe),
        );
        let t = CodecTag::fourcc(b"H264");
        assert_eq!(
            reg.resolve_tag_ref(&ProbeContext::new(&t))
                .map(|c| c.as_str()),
            Some("h264"),
        );
    }

    /// No-op decoder factory so the registration produces a real
    /// CodecImplementation (the registry skips tag-only entries —
    /// without a factory there'd be nothing in `implementations()`
    /// to assert against).
    fn dummy_decoder_factory(
        _params: &crate::CodecParameters,
    ) -> crate::Result<Box<dyn super::Decoder>> {
        Err(crate::Error::unsupported("dummy decoder"))
    }

    #[test]
    fn engine_metadata_propagates_through_register() {
        fn dummy_probe() -> Vec<HwDeviceInfo> {
            vec![]
        }
        let mut reg = CodecRegistry::default();
        reg.register(
            CodecInfo::new(CodecId::new("h264"))
                .capabilities(CodecCapabilities::video("h264_test"))
                .decoder(dummy_decoder_factory)
                .with_engine_id("test-backend")
                .with_engine_probe(dummy_probe),
        );
        let impls = reg.implementations(&CodecId::new("h264"));
        assert_eq!(impls.len(), 1);
        assert_eq!(impls[0].engine_id, Some("test-backend"));
        assert!(impls[0].engine_probe.is_some());
    }

    #[test]
    fn engine_metadata_absent_for_sw_codecs() {
        // SW codecs don't call the engine builders — both fields
        // should land as None on the resulting CodecImplementation.
        let mut reg = CodecRegistry::default();
        reg.register(
            CodecInfo::new(CodecId::new("flac"))
                .capabilities(CodecCapabilities::audio("flac_sw"))
                .decoder(dummy_decoder_factory),
        );
        let impls = reg.implementations(&CodecId::new("flac"));
        assert_eq!(impls.len(), 1);
        assert!(impls[0].engine_id.is_none());
        assert!(impls[0].engine_probe.is_none());
    }
}

#[cfg(test)]
mod output_params_tests {
    use super::*;
    use crate::{
        ChannelLayout, ColorRange, ColorSignal, MediaType, Rational, SampleFormat, VideoFrame,
    };

    /// Shared plumbing for the mocks: the codec id they report.
    struct Base {
        id: CodecId,
    }

    impl Base {
        fn new(id: &str) -> Self {
            Self {
                id: CodecId::new(id),
            }
        }
    }

    /// Implements nothing of the output-description surface.
    struct Neither(Base);

    impl Decoder for Neither {
        fn codec_id(&self) -> &CodecId {
            &self.0.id
        }
        fn send_packet(&mut self, _: &Packet) -> Result<()> {
            Ok(())
        }
        fn receive_frame(&mut self) -> Result<Frame> {
            Err(Error::NeedMore)
        }
        fn flush(&mut self) -> Result<()> {
            Ok(())
        }
    }

    /// Implements only the older narrow query.
    struct PixOnly(Base);

    impl Decoder for PixOnly {
        fn codec_id(&self) -> &CodecId {
            &self.0.id
        }
        fn send_packet(&mut self, _: &Packet) -> Result<()> {
            Ok(())
        }
        fn receive_frame(&mut self) -> Result<Frame> {
            Err(Error::NeedMore)
        }
        fn flush(&mut self) -> Result<()> {
            Ok(())
        }
        fn output_pixel_format(&self) -> Option<PixelFormat> {
            Some(PixelFormat::Yuv420P10Le)
        }
    }

    /// A decoder that changes resolution mid-stream: every packet whose
    /// first byte is non-zero is a "keyframe" announcing that byte as
    /// the new height; the shape becomes visible with the first frame
    /// decoded from it, following the trait contract.
    struct Switching {
        base: Base,
        queued: std::collections::VecDeque<u32>,
        current: Option<u32>,
        generation: u64,
    }

    impl Switching {
        fn new() -> Self {
            Self {
                base: Base::new("mock-switch"),
                queued: Default::default(),
                current: None,
                generation: 0,
            }
        }
    }

    impl Decoder for Switching {
        fn codec_id(&self) -> &CodecId {
            &self.base.id
        }
        fn send_packet(&mut self, packet: &Packet) -> Result<()> {
            let h = packet.data.first().copied().unwrap_or(0) as u32;
            let h = if h == 0 {
                *self.queued.back().or(self.current.as_ref()).unwrap_or(&16)
            } else {
                h
            };
            self.queued.push_back(h);
            Ok(())
        }
        fn receive_frame(&mut self) -> Result<Frame> {
            let h = self.queued.pop_front().ok_or(Error::NeedMore)?;
            if let Some(cur) = self.current
                && cur != h
            {
                self.generation = self.generation.wrapping_add(1);
            }
            self.current = Some(h);
            Ok(Frame::Video(VideoFrame {
                pts: None,
                planes: Vec::new(),
            }))
        }
        fn flush(&mut self) -> Result<()> {
            Ok(())
        }
        fn output_params(&self) -> Option<CodecParameters> {
            let h = self.current?;
            let mut p = CodecParameters::video(self.base.id.clone());
            p.width = Some(32);
            p.height = Some(h);
            p.pixel_format = Some(PixelFormat::Yuv420P);
            Some(p)
        }
        fn output_pixel_format(&self) -> Option<PixelFormat> {
            self.output_params().and_then(|p| p.pixel_format)
        }
        fn output_params_generation(&self) -> u64 {
            self.generation
        }
    }

    fn pkt(b: u8) -> Packet {
        Packet::new(0, crate::TimeBase::new(1, 1), vec![b])
    }

    #[test]
    fn default_without_any_answer_is_none_and_generation_zero() {
        let d = Neither(Base::new("mock"));
        assert!(d.output_params().is_none());
        assert_eq!(d.output_params_generation(), 0);
    }

    #[test]
    fn default_derives_from_output_pixel_format() {
        let d = PixOnly(Base::new("mock-pix"));
        let p = d.output_params().expect("derived from output_pixel_format");
        assert_eq!(p.codec_id, CodecId::new("mock-pix"));
        assert_eq!(p.media_type, MediaType::Video);
        assert_eq!(p.pixel_format, Some(PixelFormat::Yuv420P10Le));
        assert_eq!(p.width, None);
        assert_eq!(p.height, None);
        assert_eq!(d.output_params_generation(), 0);
    }

    #[test]
    fn merge_decoder_wins_for_output_fields_container_keeps_the_rest() {
        let mut stream = CodecParameters::video(CodecId::new("h264"));
        stream.width = Some(1920);
        stream.height = Some(1088);
        stream.frame_rate = Some(Rational::new(25, 1));
        stream.extradata = vec![1, 2, 3];
        stream.bit_rate = Some(5_000_000);
        stream.language = Some("en".into());
        stream.color_signal = ColorSignal::from_code_points(1, 1, 1, false);

        let mut out = CodecParameters::video(CodecId::new("h264"));
        out.height = Some(1080);
        out.pixel_format = Some(PixelFormat::Yuv420P);
        out.color_signal = ColorSignal::default().with_range(ColorRange::Full);

        assert!(stream.merge_output_params(&out));
        assert_eq!(stream.width, Some(1920), "decoder None keeps container");
        assert_eq!(stream.height, Some(1080), "decoder Some wins");
        assert_eq!(stream.pixel_format, Some(PixelFormat::Yuv420P));
        assert_eq!(stream.frame_rate, Some(Rational::new(25, 1)));
        assert_eq!(stream.extradata, vec![1, 2, 3]);
        assert_eq!(stream.bit_rate, Some(5_000_000));
        assert_eq!(stream.language.as_deref(), Some("en"));
        assert_eq!(stream.color_signal.range, ColorRange::Full);
        assert_eq!(
            stream.color_signal.primaries,
            ColorSignal::from_code_points(1, 1, 1, false).primaries,
            "unspecified decoder parts keep the container's"
        );
        assert!(!stream.merge_output_params(&out), "second merge is a no-op");
    }

    #[test]
    fn merge_audio_keeps_count_and_layout_consistent() {
        let mut stream = CodecParameters::audio(CodecId::new("mp3"))
            .channels(2)
            .channel_layout(ChannelLayout::Stereo);
        stream.sample_rate = Some(44_100);
        let mut out = CodecParameters::audio(CodecId::new("mp3")).channels(1);
        out.sample_format = Some(SampleFormat::S16);
        out.sample_rate = Some(48_000);
        assert!(stream.merge_output_params(&out));
        assert_eq!(stream.channels, Some(1));
        assert_eq!(stream.channel_layout, None, "stale stereo layout dropped");
        assert_eq!(stream.resolved_layout(), Some(ChannelLayout::Mono));
        assert_eq!(stream.sample_rate, Some(48_000));
        assert_eq!(stream.sample_format, Some(SampleFormat::S16));

        // Layout-only answer re-derives a disagreeing count.
        let mut stream = CodecParameters::audio(CodecId::new("aac")).channels(2);
        let out = CodecParameters::audio(CodecId::new("aac")).channel_layout(ChannelLayout::Mono);
        assert!(stream.merge_output_params(&out));
        assert_eq!(stream.channels, Some(1));
        assert_eq!(stream.channel_layout, Some(ChannelLayout::Mono));
    }

    #[test]
    fn merge_of_empty_answer_changes_nothing() {
        let mut stream = CodecParameters::video(CodecId::new("vp9"));
        stream.width = Some(640);
        let out = CodecParameters::video(CodecId::new("vp9"));
        assert!(!stream.merge_output_params(&out));
        assert_eq!(stream.width, Some(640));
    }

    #[test]
    fn generation_bumps_with_the_first_frame_of_the_new_shape() {
        let mut d = Switching::new();
        assert!(d.output_params().is_none(), "nothing parsed yet");
        d.send_packet(&pkt(16)).unwrap();
        d.send_packet(&pkt(0)).unwrap();
        // New shape announced while old-shape frames are still queued.
        d.send_packet(&pkt(24)).unwrap();

        d.receive_frame().unwrap();
        assert_eq!(d.output_params().unwrap().height, Some(16));
        assert_eq!(d.output_params_generation(), 0, "first resolve is no bump");
        d.receive_frame().unwrap();
        assert_eq!(d.output_params_generation(), 0, "same shape, no bump");
        assert_eq!(d.output_params().unwrap().height, Some(16));
        d.receive_frame().unwrap();
        assert_eq!(d.output_params_generation(), 1);
        assert_eq!(d.output_params().unwrap().height, Some(24));
        assert_eq!(d.output_pixel_format(), Some(PixelFormat::Yuv420P));
    }

    #[test]
    fn watch_merges_once_and_again_only_on_change() {
        let mut d = Switching::new();
        let mut stream = CodecParameters::video(CodecId::new("mock-switch"));
        stream.bit_rate = Some(1234);
        let mut watch = OutputParamsWatch::new();

        // Nothing known yet: prime and observe leave the stream alone.
        assert!(!watch.prime(&d, &mut stream));
        assert!(!watch.observe(&d, &mut stream));

        for b in [16, 0, 0, 24, 0] {
            d.send_packet(&pkt(b)).unwrap();
        }
        let mut changes = Vec::new();
        let dyn_dec: &mut dyn Decoder = &mut d;
        while dyn_dec.receive_frame().is_ok() {
            changes.push(watch.observe(&*dyn_dec, &mut stream));
        }
        assert_eq!(changes, vec![true, false, false, true, false]);
        assert_eq!(stream.height, Some(24));
        assert_eq!(stream.width, Some(32));
        assert_eq!(stream.bit_rate, Some(1234));

        // reset forces a re-read; same answer → no field change.
        watch.reset();
        assert!(!watch.observe(&d, &mut stream));
        assert_eq!(watch, {
            let mut w = OutputParamsWatch::new();
            w.observe(&d, &mut stream.clone());
            w
        });
    }

    #[test]
    fn watch_picks_up_the_derived_default() {
        let d = PixOnly(Base::new("mock-pix"));
        let mut stream = CodecParameters::video(CodecId::new("mock-pix"));
        stream.width = Some(64);
        let mut watch = OutputParamsWatch::new();
        assert!(watch.observe(&d, &mut stream));
        assert_eq!(stream.pixel_format, Some(PixelFormat::Yuv420P10Le));
        assert_eq!(stream.width, Some(64));
        assert!(!watch.observe(&d, &mut stream));
    }

    /// Like [`Switching`] but also emits the in-band
    /// [`Frame::StreamParams`] event immediately before the first frame
    /// of a new mid-stream shape. Frames carry one real plane
    /// (`32 × height` bytes) so the arena path can be exercised.
    struct Eventing {
        base: Base,
        queued: std::collections::VecDeque<u32>,
        current: Option<u32>,
        generation: u64,
        flushed: bool,
    }

    impl Eventing {
        fn new() -> Self {
            Self {
                base: Base::new("mock-event"),
                queued: Default::default(),
                current: None,
                generation: 0,
                flushed: false,
            }
        }
    }

    impl Decoder for Eventing {
        fn codec_id(&self) -> &CodecId {
            &self.base.id
        }
        fn send_packet(&mut self, packet: &Packet) -> Result<()> {
            let h = packet.data.first().copied().unwrap_or(0) as u32;
            let h = if h == 0 {
                *self.queued.back().or(self.current.as_ref()).unwrap_or(&16)
            } else {
                h
            };
            self.queued.push_back(h);
            Ok(())
        }
        fn receive_frame(&mut self) -> Result<Frame> {
            let Some(&h) = self.queued.front() else {
                return Err(if self.flushed {
                    Error::Eof
                } else {
                    Error::NeedMore
                });
            };
            if let Some(cur) = self.current
                && cur != h
            {
                // Mid-stream change: publish the new shape (answer +
                // bump) together with the event, then the frame.
                self.current = Some(h);
                self.generation = self.generation.wrapping_add(1);
                return Ok(Frame::StreamParams(Box::new(
                    self.output_params().expect("shape known"),
                )));
            }
            self.queued.pop_front();
            self.current = Some(h);
            Ok(Frame::Video(VideoFrame {
                pts: None,
                planes: vec![crate::VideoPlane {
                    stride: 32,
                    data: vec![0; 32 * h as usize],
                }],
            }))
        }
        fn flush(&mut self) -> Result<()> {
            self.flushed = true;
            Ok(())
        }
        fn reset(&mut self) -> Result<()> {
            // Drop pending output; keep the shape of the last frame
            // returned so a different shape after the seek is
            // announced under the same rule.
            self.queued.clear();
            self.flushed = false;
            Ok(())
        }
        fn output_params(&self) -> Option<CodecParameters> {
            let h = self.current?;
            let mut p = CodecParameters::video(self.base.id.clone());
            p.width = Some(32);
            p.height = Some(h);
            p.pixel_format = Some(PixelFormat::Gray8);
            Some(p)
        }
        fn output_pixel_format(&self) -> Option<PixelFormat> {
            self.output_params().and_then(|p| p.pixel_format)
        }
        fn output_params_generation(&self) -> u64 {
            self.generation
        }
    }

    /// One received item, as a consumer sees it.
    #[derive(Debug, PartialEq)]
    enum Seen {
        Video(u32),
        Event(u32, u64),
    }

    fn shape(p: &CodecParameters) -> (Option<u32>, Option<u32>, Option<PixelFormat>) {
        (p.width, p.height, p.pixel_format)
    }

    fn drain(d: &mut Eventing) -> Vec<Seen> {
        let mut out = Vec::new();
        while let Ok(f) = d.receive_frame() {
            match f {
                Frame::Video(v) => {
                    let h = (v.planes[0].data.len() / v.planes[0].stride) as u32;
                    assert_eq!(
                        d.output_params().unwrap().height,
                        Some(h),
                        "output_params describes the frame just returned"
                    );
                    out.push(Seen::Video(h));
                }
                Frame::StreamParams(p) => {
                    assert_eq!(
                        d.output_params().as_ref().map(shape),
                        Some(shape(&p)),
                        "output_params already answers the new shape"
                    );
                    out.push(Seen::Event(p.height.unwrap(), d.output_params_generation()));
                }
                other => panic!("unexpected {other:?}"),
            }
        }
        out
    }

    #[test]
    fn stream_params_frame_helpers() {
        let mut p = CodecParameters::video(CodecId::new("x"));
        p.width = Some(8);
        let ev = Frame::StreamParams(Box::new(p.clone()));
        assert_eq!(ev.pts(), None);
        assert!(ev.is_stream_params());
        assert_eq!(ev.stream_params().map(shape), Some(shape(&p)));
        let media = Frame::Video(VideoFrame {
            pts: Some(7),
            planes: Vec::new(),
        });
        assert_eq!(media.pts(), Some(7));
        assert!(!media.is_stream_params());
        assert!(media.stream_params().is_none());
        // Boxed: the event does not grow `Frame`.
        assert!(std::mem::size_of::<Frame>() < std::mem::size_of::<CodecParameters>());
        assert!(
            std::mem::size_of::<Frame>()
                <= std::mem::size_of::<crate::VectorFrame>()
                    .max(std::mem::size_of::<crate::SubtitleCue>())
                    + std::mem::size_of::<usize>()
        );
    }

    #[test]
    fn event_precedes_the_first_frame_of_the_new_shape_only() {
        let mut d = Eventing::new();
        for b in [16, 0, 24, 0, 0] {
            d.send_packet(&pkt(b)).unwrap();
        }
        d.flush().unwrap();
        assert_eq!(
            drain(&mut d),
            vec![
                Seen::Video(16),
                Seen::Video(16),
                Seen::Event(24, 1),
                Seen::Video(24),
                Seen::Video(24),
                Seen::Video(24),
            ],
            "no event for the first shape, one for the change, flush keeps order"
        );
        assert!(matches!(d.receive_frame(), Err(Error::Eof)));
    }

    #[test]
    fn event_after_reset_follows_the_same_rule() {
        let mut d = Eventing::new();
        d.send_packet(&pkt(16)).unwrap();
        assert_eq!(drain(&mut d), vec![Seen::Video(16)]);
        // Pending output is dropped by the seek; it lands in a 24-high
        // segment while 16 was the last shape out.
        d.send_packet(&pkt(16)).unwrap();
        d.reset().unwrap();
        d.send_packet(&pkt(24)).unwrap();
        d.send_packet(&pkt(0)).unwrap();
        assert_eq!(
            drain(&mut d),
            vec![Seen::Event(24, 1), Seen::Video(24), Seen::Video(24)]
        );
        // Seek back to the original shape: another single event.
        d.reset().unwrap();
        d.send_packet(&pkt(16)).unwrap();
        assert_eq!(drain(&mut d), vec![Seen::Event(16, 2), Seen::Video(16)]);
        // Seek within the same shape: no event.
        d.reset().unwrap();
        d.send_packet(&pkt(16)).unwrap();
        assert_eq!(drain(&mut d), vec![Seen::Video(16)]);
    }

    /// Feed the same switching stream and count reported changes after
    /// the first frame for each consumer style.
    fn changes_after_first(mode: &str) -> (usize, CodecParameters) {
        let mut d = Eventing::new();
        for b in [16, 0, 24, 0, 40, 0] {
            d.send_packet(&pkt(b)).unwrap();
        }
        let mut stream = CodecParameters::video(CodecId::new("mock-event"));
        stream.bit_rate = Some(99);
        let mut watch = OutputParamsWatch::new();
        let mut reports = Vec::new();
        let dec: &mut dyn Decoder = &mut d;
        while let Ok(f) = dec.receive_frame() {
            let changed = match mode {
                // Counter only, observed after every receive_frame.
                "counter" => watch.observe(&*dec, &mut stream),
                // Counter only, events skipped.
                "counter-skip" => !f.is_stream_params() && watch.observe(&*dec, &mut stream),
                // Event only (first shape via the first-frame read).
                "event" => {
                    if reports.is_empty() {
                        watch.observe(&*dec, &mut stream)
                    } else {
                        watch.observe_frame(&f, &mut stream)
                    }
                }
                // Both on every item.
                _ => {
                    let a = watch.observe_frame(&f, &mut stream);
                    let b = watch.observe(&*dec, &mut stream);
                    a | b
                }
            };
            reports.push(changed);
        }
        assert!(reports[0], "{mode}: first frame resolves the shape");
        (reports[1..].iter().filter(|c| **c).count(), stream)
    }

    #[test]
    fn counter_and_event_report_each_change_exactly_once() {
        for mode in ["counter", "counter-skip", "event", "both"] {
            let (n, stream) = changes_after_first(mode);
            assert_eq!(n, 2, "{mode}: two mid-stream changes, each once");
            assert_eq!(stream.height, Some(40), "{mode}");
            assert_eq!(stream.width, Some(32), "{mode}");
            assert_eq!(stream.bit_rate, Some(99), "{mode}: container field kept");
        }
    }

    #[test]
    fn observe_frame_ignores_media_frames() {
        let mut watch = OutputParamsWatch::new();
        let mut stream = CodecParameters::video(CodecId::new("x"));
        let before = stream.clone();
        let f = Frame::Video(VideoFrame {
            pts: None,
            planes: Vec::new(),
        });
        assert!(!watch.observe_frame(&f, &mut stream));
        assert_eq!(shape(&stream), shape(&before));
    }

    #[test]
    fn arena_default_consumes_the_event() {
        let mut d = Eventing::new();
        for b in [16, 24] {
            d.send_packet(&pkt(b)).unwrap();
        }
        let a = d.receive_arena_frame().unwrap();
        assert_eq!(a.header().height, 16);
        let gen_before = d.output_params_generation();
        let b = d.receive_arena_frame().unwrap();
        assert_eq!(
            b.header().height,
            24,
            "event skipped, new-shape frame returned"
        );
        assert_eq!(d.output_params_generation(), gen_before + 1);
        assert!(matches!(d.receive_arena_frame(), Err(Error::NeedMore)));
    }
}
