//! Core types and registries for the oxideav framework.
//!
//! This crate is the dependency-light foundation: primitive types
//! (timestamps, packets, frames, media formats) plus the registries
//! every sibling crate registers itself into. The aggregate
//! [`RuntimeContext`] bundles all four registries (codec / container /
//! source / filter) into a single value that consumers pass around.
//!
//! # Resolution order
//!
//! Every registry lookup that can have several candidates ranks them by
//! one rule, so the winner depends only on the registered claims and
//! the input — never on hash-map iteration order:
//!
//! 1. **evidence, descending** — probe score
//!    ([`ContainerRegistry::probe_input`]), probe confidence
//!    ([`CodecRegistry::resolve_tag_ref`]; unprobed claims count as
//!    `1.0`), or matched prefix length
//!    ([`CodecRegistry::resolve_payload_magic_ref`]);
//! 2. **resolution priority, ascending** — lower is preferred, default
//!    [`DEFAULT_PRIORITY`]; set with
//!    [`ContainerRegistry::register_probe_with_priority`],
//!    [`ContainerRegistry::register_extension_with_priority`] or
//!    [`CodecInfo::with_resolution_priority`];
//! 3. **registration order** — earlier wins (extension hints keep their
//!    historical most-recent-wins contract at equal priority).
//!
//! The ranked lists are observable via
//! [`ContainerRegistry::probe_candidates`],
//! [`ContainerRegistry::extension_candidates`],
//! [`CodecRegistry::resolve_tag_candidates`] and
//! [`CodecRegistry::resolve_payload_magic_candidates`].

#![warn(missing_docs)]

pub mod arena;
pub mod bits;
pub mod blob;
pub mod capabilities;
pub mod engine;
pub mod error;
pub mod execution;
pub mod filter;
pub mod format;
pub mod frame;
pub mod layer;
pub mod limits;
pub mod metadata;
pub mod options;
pub mod packet;
pub mod picture;
pub mod rational;
pub mod registry;
pub mod signal;
pub mod stream;
pub mod subtitle;
pub mod time;
pub mod vector;

pub use blob::{BlobKind, MetadataBlob};
pub use capabilities::{CodecCapabilities, DEFAULT_PRIORITY};
pub use engine::{EngineProbeFn, HwCodecCaps, HwDeviceInfo};
pub use error::{Error, Result};
pub use execution::ExecutionContext;
pub use filter::{FilterContext, PortParams, PortSpec, StreamFilter};
pub use format::{
    ChannelLayout, ChannelPosition, MediaType, ParseChannelLayoutError, PixelFormat, SampleFormat,
};
pub use frame::{AudioFrame, Frame, VideoFrame, VideoPlane};
pub use layer::{LayerIdentity, LayerInfo};
pub use limits::DecoderLimits;
pub use metadata::{Attachment, Chapter};
pub use options::{
    CodecOptions, CodecOptionsStruct, OptionField, OptionKind, OptionValue, parse_options,
};
pub use packet::{Packet, SharedBytes, SharedPacket};
pub use picture::{AttachedPicture, PictureType};
pub use rational::Rational;
pub use registry::{
    BytesSource, CodecImplementation, CodecInfo, CodecRegistry, ContainerProbeFn,
    ContainerRegistry, Decoder, DecoderFactory, Demuxer, Encoder, EncoderFactory,
    ExtensionCandidate, FilterFactory, FilterRegistry, FrameSource, MAX_PROBE_SCORE,
    MultiTitleSource, Muxer, OpenBytesFn, OpenDemuxerFn, OpenFramesFn, OpenMultiTitleFn,
    OpenMuxerFn, OpenPacketsFn, OutputParamsWatch, PROBE_SCORE_EXTENSION, PacketSource,
    PayloadMagicCandidate, ProbeCandidate, ProbeData, ProbeScore, ReadSeek, RuntimeContext,
    SourceOutput, SourceRegistry, TagCandidate, WriteSeek,
};
pub use signal::{
    ColorPrimaries, ColorRange, ColorSignal, MatrixCoefficients, TransferCharacteristics,
};
pub use stream::{
    CodecId, CodecParameters, CodecResolver, CodecTag, Confidence, NullCodecResolver, ProbeContext,
    ProbeFn, StreamInfo,
};
pub use subtitle::{CuePosition, Segment, SubtitleCue, SubtitleStyle, TextAlign};
pub use time::{Rounding, TimeBase, Timestamp, rescale, rescale_checked, rescale_rnd};
pub use vector::{
    DashPattern, FillRule, GradientStop, Group, ImageRef, LineCap, LineJoin, LinearGradient,
    MaskKind, Node, Paint, Path, PathCommand, PathNode, Point, RadialGradient, Rect, Rgba,
    SpreadMethod, Stroke, Transform2D, VectorFrame, ViewBox,
};
