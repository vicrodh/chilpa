# oxideav-core

[![CI](https://github.com/OxideAV/oxideav-core/actions/workflows/ci.yml/badge.svg)](https://github.com/OxideAV/oxideav-core/actions/workflows/ci.yml) [![crates.io](https://img.shields.io/crates/v/oxideav-core.svg)](https://crates.io/crates/oxideav-core) [![docs.rs](https://docs.rs/oxideav-core/badge.svg)](https://docs.rs/oxideav-core) [![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Core types for the [oxideav](https://github.com/OxideAV/oxideav-workspace)
pure-Rust media framework:

* **`Packet`** — one compressed chunk belonging to one stream, with
  timestamps. Chainable `with_*` builders cover every
  [`PacketFlags`](crate::packet::PacketFlags) field
  (`with_keyframe` / `with_header` / `with_corrupt` / `with_discard` /
  `with_unit_boundary`, plus a bulk `with_flags`) and the
  stream-index / time-base / pts / dts / duration setters used by
  demuxers and remuxers. An `end_pts()` accessor returns the
  overflow-checked `pts + duration` for muxers that need a per-
  packet end timestamp.
* **`SharedPacket`** / **`SharedBytes`** — the same packet with its
  payload as a cheaply clonable view of a shared buffer
  (`SharedBytes::from(vec)`, `.slice(a..b)`, `.buffer()`), for
  producers that hand out many payloads from bytes they already hold.
  `Demuxer::next_shared_packet`, `PacketSource::next_shared_packet`,
  `Decoder::send_shared_packet` and `Muxer::write_shared_packet` are
  provided methods bridging to the `Packet` ones, so it flows through
  components that only know `Packet`; overriding both ends of a hop
  removes the copy.
* **`Frame`** — one uncompressed audio / video / subtitle chunk.
  `VideoFrame` can carry typed in-band side-channels alongside its
  pixel planes: a palette for palette-indexed (`Pal8`) content
  (`palette()` / `set_palette` / `take_palette`) with an optional
  per-entry alpha record (`palette_alpha()` / `set_palette_alpha`,
  `palette_rgba(i)` / `set_palette_rgba` — see [Palettes](#palettes))
  and a per-plane significant-bits record for mixed depths no single `PixelFormat`
  names — e.g. 12-bit luma with 10-bit chroma from a custom signal
  range (`significant_bits()` / `set_significant_bits` /
  `take_significant_bits`, LSB-anchored values), a colour-signal
  record (`color_signal()` / `set_color_signal` / `take_color_signal`)
  and a layer / view identity (`layer()` / `set_layer` /
  `take_layer`). The records compose on one frame; `image_planes()`
  iterates pixel data side-channel-agnostically. See
  [Colour signal](#colour-signal) and
  [Layers and views](#layers-and-views).
  Per-picture metadata blobs ride the same way (`blobs()` /
  `set_blobs` / `push_blob` / `take_blobs`, see
  [Metadata blobs](#metadata-blobs)).
* **`StreamInfo`** / **`CodecParameters`** — what a demuxer advertises and
  what a decoder / encoder consumes, including the stream's
  `color_signal`, its `layers` description and its metadata `blobs`
  (ICC / Exif / XMP / IPTC / cover art / custom).
  A stream's `pixel_format` is the container's claim; a decoder that
  knows what it will emit (configuration record, negotiated hardware
  session, fixed output policy) states the fact with
  `Decoder::output_pixel_format()`, which takes precedence, and an
  encoder states what it takes with
  `CodecCapabilities::accepted_pixel_formats`.
* **`TimeBase`** / **`Timestamp`** / **`Rational`** — rational time per
  stream; timestamps are integers in that base. Named constants
  (`MILLIS` / `MICROS` / `NANOS` / `MPEG_TS` / `AUDIO_48K` / `AUDIO_44K1`
  / `AUDIO_8K` / `SECONDS`) replace the workspace's `TimeBase::new(1, …)`
  magic-numbers; `TimeBase::from_rate(u32)` constructs the inverse-of-rate
  form, and `ticks_of(seconds: f64)` is the overflow-clamped inverse of
  the existing `seconds_of(ticks)`. `Timestamp::from_seconds` /
  `checked_add_ticks` / `checked_sub_ticks` / `checked_diff` /
  `checked_rescale` cover per-stream timestamp arithmetic (including
  cross-base differences for remux pipelines).

  The whole numeric core is **total — no panic, no silent wrap, even on
  `i64::MIN` terms or zero denominators**. `rescale` computes in 128-bit
  sign+magnitude space, rounds half-away-from-zero, and *saturates* at
  the `i64` boundaries; `rescale_checked` returns `None` instead
  wherever `rescale` would saturate or default; `rescale_rnd` takes an
  explicit `Rounding` mode (`NearestAway` / `Floor` for DTS-safe stamps
  / `Ceil` / `TowardZero`). `Rational` supports `+ - * /` and unary `-`
  (exact via `i128` intermediates, reduced, closest-representable
  approximation when even the reduced result exceeds `i64`),
  `checked_add/sub/mul/div` that report `None` exactly where the
  operators approximate, plus `cmp_value` / `equals_value` for value
  comparison (`30000/1001` vs `30/1`) that doesn't disturb the
  structural `Eq`/`Hash` callers rely on to preserve the on-wire
  fraction. Property-tested against independent `i128` oracles (~200k
  edge-biased cases in `tests/props.rs`).
* **`PixelFormat`** / **`SampleFormat`** — enum of supported raw formats
  (82 pixel variants including 8/10/12/16-bit YUV at
  4:2:0/4:2:2/4:4:4/4:1:1/4:4:0, YUV+alpha at 4:2:0/4:2:2/4:4:4 in both
  8-bit and deep 10/12/16-bit flavours, planar GBR(A) across the full
  8/10/12/14/16-bit depth ladder with an alpha companion at every
  depth, scene-referred 32-bit float gray/RGB(A)/planar-GBR(A) for
  linear-light HDR, packed RGB/RGBA, packed 16-bit 5-5-5 / 5-6-5 RGB
  and BGR words in both byte orders (`Rgb555Le` … `Bgr565Be`), padded
  32-bit RGB with one unused byte (`Rgbx` / `Bgrx` / `Xrgb` / `Xbgr`),
  gray+alpha at 8 and 16 bits, CMYK in both ink conventions,
  NV12/NV21, all common sample layouts), plus plane-geometry helpers
  on every variant: `chroma_subsampling()` (log2 shifts per sampling
  class), `plane_dimensions()` (ceil-division subsampled grids),
  tightly-packed sizing via `plane_row_bytes()` / `plane_size_bytes()`
  / `frame_size_bytes()` with checked arithmetic, and the family
  predicates `is_packed_rgb()` / `is_rgb()` next to `is_planar()` /
  `has_alpha()` / `is_float()` / `is_palette()`. The enum is
  `#[non_exhaustive]` with pinned discriminants: variants are only
  ever appended, so a `match` downstream needs a wildcard arm.
* **`AttachedPicture`** / **`PictureType`** — ID3v2 `APIC` taxonomy
  shared by ID3v2 / FLAC / MP4 / Vorbis cover-art carriage. `PictureType`
  round-trips byte-for-byte through `from_u8` ↔ `to_u8` over the spec-
  assigned `0x00..=0x14` range; unassigned bytes collapse to `Unknown`,
  flagged via `is_known()` so strict writers can refuse to emit the
  `0xFF` sentinel. `AttachedPicture::new(mime, kind)` plus chainable
  `with_description` / `with_data` / `with_picture_type` builders cover
  the producer side (parsers writing into a partially-decoded picture
  as bytes arrive), and `is_external_link()` distinguishes ID3v2's
  `"-->"` URL-sentinel mime from inline image bytes without having to
  hardcode the string at every call site.
* **`CodecTag`** / **`CodecResolver`** — neutral abstraction for mapping
  container-level tags (AVI FourCC, WAVEFORMATEX `wFormatTag`, MP4 OTI,
  Matroska CodecID strings) to oxideav `CodecId`s. Lets codec crates own
  their own tag claims without pulling a codec registry into every
  container. Tag-less identification gets its own container-agnostic
  path: codecs declare the payload magic prefixes they answer to
  (`CodecInfo::payload_magic(b"\x01vorbis")`, `b"OpusHead"`, `b"fLaC"`,
  …) and callers resolve a stream's leading payload bytes with
  `CodecResolver::resolve_payload_magic(first_bytes)` — longest
  matching magic wins, then resolution priority, then registration
  order. Serves Ogg's BOS packets and raw elementary-stream sniffing
  alike. Every lookup follows the documented [resolution
  order](#resolution-order) and exposes its ranked candidate list.
* **`bits`** — shared MSB-first / LSB-first `BitReader` / `BitWriter`
  plus unary helpers. Used by the FLAC, AAC, H.264, HEVC, Vorbis and a
  dozen other codecs in the workspace. The LSB pair (the Vorbis §2.1.4
  layout) exposes the full MSB surface — `peek_u32` (Huffman lookup
  windows), `skip` / `consume`, `align_to_byte`, `read_bytes`,
  positional bookkeeping, `write_bytes` and the alias set. Criterion
  baselines live in `benches/primitives.rs` (~1.3 GiB/s read,
  ~430 MiB/s write on a mixed-width field schedule).
* **`register!`** — the sibling entry-point macro. Expands to
  `pub fn __oxideav_entry(ctx)` (hidden dispatch plumbing) that
  `oxideav-meta`'s generated `register_all` calls at the sibling's
  **crate root** — invoke it in `lib.rs`, or re-export it there
  (`pub use registry::__oxideav_entry;`) when it lives in a
  submodule. Contract spelled out in `registry::slice`.
* **`SourceRegistry`** — URI scheme dispatch for sources. Drivers
  register as one of three shapes — `BytesSource` (file / http),
  `PacketSource` (transport-layer protocols that pre-demux), or
  `FrameSource` (synthetic generators that emit decoded frames) —
  and `open(uri)` returns a `SourceOutput` enum the pipeline executor
  branches on.
* **`Error`** — one unified error enum used across the ecosystem, with
  a documented caller-action taxonomy (verdict vs starvation vs
  backpressure), constructors for every string variant, and
  `is_eof` / `is_need_more` / `is_starved` / `is_resource_exhausted`
  predicates (the enum can't be `PartialEq` — `Io` wraps
  `std::io::Error`).

Every public item is documented (`#![warn(missing_docs)]` is enforced
at the crate root, promoted to deny by CI's clippy gate) and
`cargo doc` is warning-clean under docs.rs-strict settings.

Zero C dependencies. Zero FFI. Zero `*-sys` crates.

## Palettes

Palette-indexed content (`PixelFormat::Pal8`) carries its colour table
on the frame as two in-band side-channel records:

* **Palette** (`stride == 0`) — packed RGB triplets, entry `i` at bytes
  `3*i .. 3*i + 3`; `palette()` / `set_palette` / `with_palette` /
  `take_palette`, per-entry `palette_rgb(i)`. Up to 256 entries;
  producers attach exactly as many as the source declares.
* **Palette alpha** (`stride == usize::MAX - 3`) — one alpha byte per
  entry in the same order, for a GIF transparent index, a PNG `tRNS`
  chunk on an indexed image, TGA / BMP alpha palettes;
  `palette_alpha()` / `set_palette_alpha` / `with_palette_alpha` /
  `take_palette_alpha`. The record may be **shorter** than the palette
  (a GIF with transparent index 2 needs three bytes) and every entry it
  does not cover is opaque. It is only meaningful next to a palette:
  without one, or when it is **longer** than the palette's entry count,
  it reads as `None` and consumers fall back to opaque. Validation is
  at read time, so the two records may be attached in either order.

`palette_rgba(i)` is the one lookup a `Pal8` → RGBA expander needs: the
RGB triplet plus the alpha (`255` when absent), `None` exactly when
`palette_rgb(i)` is. `set_palette_rgba(&[[u8; 4]])` /
`with_palette_rgba` write both records from one RGBA table. Frames
without a transparent entry attach no alpha record and stay
byte-for-byte what they were.

## Colour signal

A `PixelFormat` says how samples are laid out; it does not say what
the values mean. `ColorSignal` (module `signal`) carries the
coding-independent description of Rec. ITU-T H.273 | ISO/IEC 23091-2:

| field       | type                      | H.273 name                |
|-------------|---------------------------|---------------------------|
| `range`     | `ColorRange`              | `VideoFullRangeFlag` (+ `Unspecified`) |
| `primaries` | `ColorPrimaries(u8)`      | `ColourPrimaries`         |
| `transfer`  | `TransferCharacteristics(u8)` | `TransferCharacteristics` |
| `matrix`    | `MatrixCoefficients(u8)`  | `MatrixCoefficients`      |

The three code points are raw 8-bit newtypes with named constants for
every value H.273 (07/2024) defines (`ColorPrimaries::BT2020`,
`TransferCharacteristics::SMPTE_ST2084`, `MatrixCoefficients::BT709`,
…); reserved values pass through unchanged. Every field defaults to
*unspecified* (code point 2 / `ColorRange::Unspecified`) and the crate
never substitutes a guess — `ColorSignal::or(fallback)` layers one
description over another field-wise, and consumers apply their own
policy to what remains open.

Where it lives:

* **Stream** — `CodecParameters::color_signal` (default unspecified),
  set with `with_color_signal` / `with_color_range`. Demuxers fill it
  from the container's colour record (an ISOBMFF `colr` box, a
  Matroska `Colour` element, …), decoders from the bitstream's own
  signalling when the container had none, encoders in
  `output_params()` so muxers know what to write.
* **Frame** — `VideoFrame::color_signal()` / `set_color_signal` /
  `with_color_signal` / `take_color_signal`, an in-band side-channel
  record (`stride == usize::MAX - 1`) for producers whose signal is
  per-picture or that have no stream object (a still-image item, an
  auxiliary alpha image whose range differs from its master). A frame
  record refines the stream value:
  `frame.color_signal().unwrap_or_default().or(params.color_signal)`.
* **Pixel-format labels** — only the legacy `YuvJ420P` / `YuvJ422P` /
  `YuvJ444P` labels commit to a range (`PixelFormat::implied_color_range`
  → `Full`). Every other format, including every >8-bit and
  alpha-bearing surface, leaves the range to the signal.
  `CodecParameters::resolved_color_range()` combines the two: explicit
  signal first, then the label, else `Unspecified`. Converters should
  read that instead of inferring range from the format name — a 10-bit
  full-range stream on `Yuv420P10Le` and an 8-bit alpha plane on
  `Gray8` are only expressible this way.

## Layers and views

Scalable and multi-view coding (spatial / quality / view scalability in
H.264 Annexes G–H, H.265 / H.266 Annex F, AV1 operating points, …) put
more than one layer of pictures in one stream. Module `layer` gives
that structure a codec-neutral shape; identifiers are the codec's own
values passed through verbatim, `0` is the base layer.

* **Per frame** — `LayerIdentity { layer_id: u16, view_id: Option<u16>,
  access_unit: Option<u64> }`, attached by the decoder through
  `VideoFrame::set_layer` / `with_layer` and read with
  `VideoFrame::layer()` (or `layer_or_base()`, which reads `None` as
  the base layer). It is an in-band side-channel record
  (`stride == usize::MAX - 2`); single-layer decoders attach nothing and
  their frames are byte-for-byte unchanged. `access_unit` lets a
  consumer regroup the per-layer frames of one presentation instant.
* **Per stream** — `CodecParameters::layers: Vec<LayerInfo>` with
  `LayerInfo { layer_id, view_id, depends_on }`, set with `with_layers`
  and queried with `layer(id)` / `is_multi_layer()`. Empty for
  single-layer streams. A container that exposes an operating point or
  a single view filters on these ids and must feed every layer reachable
  through `depends_on`; a stereo renderer routes frames by `view_id`.

Both features are strictly additive: no existing field, signature or
variant changed, `VideoFrame` remains constructible by struct literal,
and `CodecParameters` gained its two fields behind its existing
`#[non_exhaustive]` marker.

## Container metadata

Text metadata — the ordered `(key, value)` pairs a demuxer reports from
`Demuxer::metadata()` (`title`, `artist`, `album`, `comment`, `date`,
…) — now has a write side: `Muxer::set_metadata(&[(String, String)])`,
called after construction and before `write_header`, so a remux is
`mux.set_metadata(demux.metadata())`. The contract is best-effort and
explicit about "nothing": the default returns `Error::Unsupported`
(the format carries no text, nothing was stored — "not carried", not
a failed write); a muxer with text fields keeps the pairs it has a
field for, drops the rest without error and says which keys it writes;
`Muxer::carries_metadata_key(key)` answers "will the title survive
this save?" before anything is written (default `false`; override
both together). A format's native text fields — a comment extension,
an annotation chunk, an author field in a trailer — go through this
hook; opaque payloads go through the blobs below, never the other way
round.

## Metadata blobs

Formats embed metadata the framework cannot and should not parse — an
ICC profile, an Exif block, an XMP packet, IPTC datasets, a track's
cover art, a format's own attribute table. Module `blob` carries such
payloads unchanged from the producer that found them to the consumer
that wants them, on the same path the samples travel:

* **`MetadataBlob { kind: BlobKind, mime: Option<String>, data: Vec<u8> }`**
  (`#[non_exhaustive]`; `MetadataBlob::new(kind, data)` /
  `with_mime`). `BlobKind` is a name compared byte-for-byte with
  well-known constants `ICC`, `EXIF`, `XMP`, `IPTC`, `COVER_ART` and
  `BlobKind::custom("exr-attributes")` for anything else. The
  well-known kinds pin a **format-neutral payload** (the carrying
  format's framing removed: an Exif blob starts at its byte-order mark
  whether it came from a JPEG `APP1`, a PNG `eXIf` or a HEIF item; an
  ICC blob is the complete profile file) so a blob lifted from one
  format drops into another. The module docs hold the table.
* **Stream** — `CodecParameters::blobs: Vec<MetadataBlob>` (empty by
  default), with `blobs()` / `blob(&kind)` / `blobs_of(&kind)` /
  `push_blob` / `with_blob` / `with_blobs` / `take_blobs`. Demuxers
  fill it from the container, still-image decoders from the
  codestream, encoders in `output_params()` so muxers know what to
  embed. Not part of `matches_core`.
* **Frame** — `VideoFrame::blobs()` / `blob(&kind)` / `set_blobs` /
  `push_blob` / `with_blob` / `with_blobs` / `take_blobs`, an in-band
  side-channel record (`stride == usize::MAX - 4`, wire form
  `blob::encode_blobs` / `decode_blobs`) for pictures whose metadata is
  genuinely per-picture — a multi-page TIFF's per-page Exif, HEIF burst
  items with their own profile. A malformed record reads as the empty
  list, never half a list. A frame blob refines the stream blob of the
  same kind: `frame.blob(&k).or_else(|| params.blob(&k).cloned())`.

* **Custom kinds** follow one rule, `<producer>-<chunk>`: the producing
  crate's short format name, a hyphen, the native chunk the bytes come
  from, lower-case ASCII (`gif-comment`, `tga-extension-area`,
  `iff-camg`). `BlobKind::format_native("iff", "CAMG")` spells it,
  `producer()` reads the prefix back, `follows_naming_rule()` checks
  it; the module docs list the names in use so producers reuse rather
  than reinvent. A container's *string* metadata is not a blob — see
  [Container metadata](#container-metadata).
* **Encoder declaration** — `CodecCapabilities::embedded_blob_kinds:
  Vec<BlobKind>` (`with_embedded_blob_kind` / `with_embedded_blob_kinds`,
  `embeds_blob(&kind)`, `declares_embedded_blobs()`): the kinds an
  encoder reads from the input and writes into its output. Empty (the
  default) is "nothing declared" — a gateway reports pass-through;
  populated it is a promise both ways (listed kinds embedded when
  present, unlisted kinds dropped), so "will the ICC profile survive
  this save?" is answerable before a pixel is encoded.

The mechanism is medium- and container-agnostic: the same API carries
an ICC profile on an image stream and cover art on an audio stream, and
nothing in core names a container. Container-level file attachments
with filenames remain `Attachment`; the demuxer-level typed cover-art
pathway remains `AttachedPicture`.

## Resolution order

Every registry lookup that can have more than one candidate ranks
them by **one rule**, so the answer depends only on the registered
claims and the input — never on hash-map iteration order or memory
layout, and identically in every process:

1. **Evidence, descending** — the probe score (`ContainerRegistry`
   content probes), the probe confidence (`CodecRegistry` tag claims;
   unprobed claims count as `1.0`), or the matched prefix length
   (payload magics). Evidence always beats the two tie-breaks below.
2. **Resolution priority, ascending** — an `i32` attached at
   registration; **lower is preferred**, the same convention as
   `CodecCapabilities::priority`, default `DEFAULT_PRIORITY` (100).
   Set it with `ContainerRegistry::register_probe_with_priority`,
   `ContainerRegistry::register_extension_with_priority`, or
   `CodecInfo::with_resolution_priority`. It is deliberately *not*
   `CodecCapabilities::priority`: that field ranks implementations of
   one codec id (hardware before software) and must not let a backend
   out-rank another codec's identity claim on an ambiguous tag.
3. **Registration order** — earlier registration wins for probes,
   tags and magics. Extension hints are a replacement map and keep
   their historical contract: at equal priority the *most recent*
   claim wins.

Each path exposes the whole ranked list for audits —
`ContainerRegistry::probe_candidates` / `extension_candidates`,
`CodecRegistry::resolve_tag_candidates` /
`resolve_payload_magic_candidates` — whose first element is exactly
what `probe_input` / `container_for_extension` / `resolve_tag` /
`resolve_payload_magic` return. Two heads at equal evidence *and*
equal priority mean the registry settled the claim by order alone; a
sibling that should own such an input pins it with a priority below
the default instead of relying on where it lands in `register_all`.

Registration contracts: a container name registered twice
(`register_demuxer` / `register_muxer` / `register_probe`) is
replaced in place and keeps its original order slot; a codec id may
register any number of times (multi-implementation codecs), and a tag
claimed twice by one id resolves to that id either way while both
claims stay visible in the candidate list. Non-positive or non-finite
probe confidences are never candidates.

## Usage

```toml
[dependencies]
oxideav-core = "0.1"
```

Everything downstream in oxideav (codec traits, container traits, codec
implementations, the CLI) depends on this crate transitively, so the
surface is kept deliberately small. The 0.1 series is the first stable
semver line — additive changes are `0.1.x` patch bumps; breaking
reshapes go to `0.2.0`.

## License

MIT — see [LICENSE](LICENSE).
