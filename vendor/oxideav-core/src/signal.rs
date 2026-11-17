//! Video colour-signal description: sample range plus the three
//! coding-independent colour code points (colour primaries, transfer
//! characteristics, matrix coefficients) of Rec. ITU-T H.273 |
//! ISO/IEC 23091-2.
//!
//! A [`PixelFormat`](crate::PixelFormat) says how samples are laid out
//! in memory; it does not say what the sample values *mean*. Two
//! `Yuv420P10Le` streams can differ in every respect that matters to a
//! colour converter — one limited-range BT.709, the other full-range
//! BT.2020 PQ — and a pipeline that guesses stretches or crushes the
//! signal. [`ColorSignal`] carries that meaning explicitly:
//!
//! * on the stream, via
//!   [`CodecParameters::color_signal`](crate::CodecParameters::color_signal)
//!   — populated by demuxers (from a container's colour-description
//!   record, e.g. an ISOBMFF `colr` box), by decoders (from the
//!   bitstream's video-usability / sequence-header signalling), or by
//!   encoders in `output_params()`;
//! * per frame, via
//!   [`VideoFrame::color_signal`](crate::VideoFrame::color_signal) —
//!   for producers whose signal can change between pictures or that
//!   have no stream object in hand (a still-image item decoder, an
//!   auxiliary image whose range differs from its master).
//!
//! Every code point is carried as the raw 8-bit value defined by
//! H.273, wrapped in a newtype with named constants for the values that
//! specification defines. Reserved / not-yet-defined values pass
//! through untouched, so a stream that signals a code point this crate
//! predates round-trips byte-for-byte.
//!
//! The four fields are independent and each defaults to "unspecified"
//! (H.273 code point 2 for the triple; [`ColorRange::Unspecified`] for
//! the range). Consumers resolve unspecified fields with their own
//! policy — H.273 itself says the default range for video imagery is
//! limited when nothing is signalled — but this crate never
//! substitutes a guess for a value the producer left open.

/// Nominal range of the sample values relative to their bit depth.
///
/// Corresponds to H.273 `VideoFullRangeFlag`, with an explicit
/// "unspecified" state for streams and frames that carry no signalling.
/// For an `n`-bit signal, *limited* (a.k.a. video / studio / MPEG
/// range) places nominal black at `16 << (n − 8)` and nominal white at
/// `235 << (n − 8)` for luma (chroma spans `16..=240` scaled the same
/// way), whereas *full* uses the whole `0..=(1 << n) − 1` code space.
/// The range applies to RGB signals as well as to Y′CbCr ones.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum ColorRange {
    /// No range was signalled. Consumers pick a policy; H.273 suggests
    /// limited for video imagery when nothing is present.
    #[default]
    Unspecified,
    /// Limited (video / studio) range: `VideoFullRangeFlag == 0`.
    Limited,
    /// Full (PC / JPEG) range: `VideoFullRangeFlag == 1`.
    Full,
}

impl ColorRange {
    /// Map an H.273 `VideoFullRangeFlag` value to a range.
    pub fn from_full_range_flag(full: bool) -> Self {
        if full { Self::Full } else { Self::Limited }
    }

    /// The H.273 `VideoFullRangeFlag` for this range, or `None` when
    /// the range is unspecified.
    pub fn full_range_flag(self) -> Option<bool> {
        match self {
            Self::Limited => Some(false),
            Self::Full => Some(true),
            _ => None,
        }
    }

    /// `true` for [`ColorRange::Unspecified`] (and any future state
    /// that carries no decision).
    pub fn is_unspecified(self) -> bool {
        !matches!(self, Self::Limited | Self::Full)
    }

    /// Wire byte used by the frame side-channel record:
    /// `0` unspecified, `1` limited, `2` full.
    fn to_byte(self) -> u8 {
        match self {
            Self::Limited => 1,
            Self::Full => 2,
            _ => 0,
        }
    }

    /// Inverse of [`to_byte`](Self::to_byte); unknown bytes decode as
    /// unspecified.
    fn from_byte(b: u8) -> Self {
        match b {
            1 => Self::Limited,
            2 => Self::Full,
            _ => Self::Unspecified,
        }
    }
}

/// H.273 `ColourPrimaries` code point — the chromaticity coordinates
/// of the source colour primaries and reference white.
///
/// The inner value is the raw 8-bit code point; named constants cover
/// the values H.273 (07/2024) defines. Any other value is reserved by
/// the specification and is carried through unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ColorPrimaries(pub u8);

impl ColorPrimaries {
    /// Rec. ITU-R BT.709-6; also sRGB / sYCC (IEC 61966-2-1) and
    /// IEC 61966-2-4.
    pub const BT709: Self = Self(1);
    /// Unspecified — unknown or determined by the application.
    pub const UNSPECIFIED: Self = Self(2);
    /// Rec. ITU-R BT.470-6 System M (historical NTSC 1953 primaries).
    pub const BT470_SYSTEM_M: Self = Self(4);
    /// Rec. ITU-R BT.470-6 System B, G; Rec. ITU-R BT.601-7 625-line.
    pub const BT470_SYSTEM_BG: Self = Self(5);
    /// Rec. ITU-R BT.601-7 525-line; SMPTE ST 170 (functionally the
    /// same as [`SMPTE_ST240`](Self::SMPTE_ST240)).
    pub const BT601_525: Self = Self(6);
    /// SMPTE ST 240 (functionally the same as
    /// [`BT601_525`](Self::BT601_525)).
    pub const SMPTE_ST240: Self = Self(7);
    /// Generic film (colour filters using Illuminant C).
    pub const GENERIC_FILM: Self = Self(8);
    /// Rec. ITU-R BT.2020-2 / BT.2100-2 wide-gamut primaries.
    pub const BT2020: Self = Self(9);
    /// SMPTE ST 428-1 — CIE 1931 XYZ.
    pub const SMPTE_ST428: Self = Self(10);
    /// SMPTE RP 431-2 (digital-cinema P3 with its own white point).
    pub const SMPTE_RP431: Self = Self(11);
    /// SMPTE EG 432-1 (P3 primaries with a D65 white).
    pub const SMPTE_EG432: Self = Self(12);

    /// Wrap a raw code point.
    pub const fn new(code_point: u8) -> Self {
        Self(code_point)
    }

    /// The raw 8-bit code point.
    pub const fn code_point(self) -> u8 {
        self.0
    }

    /// `true` for code point 2.
    pub const fn is_unspecified(self) -> bool {
        self.0 == 2
    }

    /// Short identifier for the code point when H.273 defines it
    /// (`"bt709"`, `"bt2020"`, …); `None` for reserved values.
    pub fn name(self) -> Option<&'static str> {
        Some(match self.0 {
            1 => "bt709",
            2 => "unspecified",
            4 => "bt470m",
            5 => "bt470bg",
            6 => "bt601-525",
            7 => "smpte240m",
            8 => "film",
            9 => "bt2020",
            10 => "smpte428",
            11 => "smpte431",
            12 => "smpte432",
            22 => "cp22",
            _ => return None,
        })
    }
}

impl Default for ColorPrimaries {
    fn default() -> Self {
        Self::UNSPECIFIED
    }
}

impl From<u8> for ColorPrimaries {
    fn from(code_point: u8) -> Self {
        Self(code_point)
    }
}

impl std::fmt::Display for ColorPrimaries {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.name() {
            Some(n) => f.write_str(n),
            None => write!(f, "reserved({})", self.0),
        }
    }
}

/// H.273 `TransferCharacteristics` code point — the reference
/// opto-electronic transfer function of the source (or the inverse of
/// the reference electro-optical function, for the display-referred
/// entries).
///
/// The inner value is the raw 8-bit code point; named constants cover
/// the values H.273 (07/2024) defines. Any other value is reserved by
/// the specification and is carried through unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TransferCharacteristics(pub u8);

impl TransferCharacteristics {
    /// Rec. ITU-R BT.709-6 (functionally the same as 6, 14 and 15).
    pub const BT709: Self = Self(1);
    /// Unspecified — unknown or determined by the application.
    pub const UNSPECIFIED: Self = Self(2);
    /// Assumed display gamma 2.2 (Rec. ITU-R BT.470-6 System M).
    pub const GAMMA22: Self = Self(4);
    /// Assumed display gamma 2.8 (Rec. ITU-R BT.470-6 System B, G).
    pub const GAMMA28: Self = Self(5);
    /// Rec. ITU-R BT.601-7 525 or 625; SMPTE ST 170 (functionally the
    /// same as 1, 14 and 15).
    pub const BT601: Self = Self(6);
    /// SMPTE ST 240.
    pub const SMPTE_ST240: Self = Self(7);
    /// Linear transfer characteristics.
    pub const LINEAR: Self = Self(8);
    /// Logarithmic transfer characteristic, 100:1 range.
    pub const LOG100: Self = Self(9);
    /// Logarithmic transfer characteristic, 100·√10 : 1 range.
    pub const LOG100_SQRT10: Self = Self(10);
    /// IEC 61966-2-4 (xvYCC).
    pub const IEC61966_2_4: Self = Self(11);
    /// Rec. ITU-R BT.1361-0 extended colour gamut system (historical).
    pub const BT1361_EXTENDED: Self = Self(12);
    /// IEC 61966-2-1 sRGB (with the identity matrix) or sYCC (with
    /// matrix 5).
    pub const IEC61966_2_1: Self = Self(13);
    /// Rec. ITU-R BT.2020-2, 10-bit system (functionally the same as
    /// 1, 6 and 15).
    pub const BT2020_10BIT: Self = Self(14);
    /// Rec. ITU-R BT.2020-2, 12-bit system (functionally the same as
    /// 1, 6 and 14).
    pub const BT2020_12BIT: Self = Self(15);
    /// SMPTE ST 2084 — Rec. ITU-R BT.2100-2 perceptual quantization
    /// (PQ).
    pub const SMPTE_ST2084: Self = Self(16);
    /// SMPTE ST 428-1 (digital cinema).
    pub const SMPTE_ST428: Self = Self(17);
    /// ARIB STD-B67 — Rec. ITU-R BT.2100-2 hybrid log-gamma (HLG).
    pub const ARIB_STD_B67: Self = Self(18);

    /// Wrap a raw code point.
    pub const fn new(code_point: u8) -> Self {
        Self(code_point)
    }

    /// The raw 8-bit code point.
    pub const fn code_point(self) -> u8 {
        self.0
    }

    /// `true` for code point 2.
    pub const fn is_unspecified(self) -> bool {
        self.0 == 2
    }

    /// Short identifier for the code point when H.273 defines it
    /// (`"bt709"`, `"pq"`, `"hlg"`, …); `None` for reserved values.
    pub fn name(self) -> Option<&'static str> {
        Some(match self.0 {
            1 => "bt709",
            2 => "unspecified",
            4 => "gamma22",
            5 => "gamma28",
            6 => "bt601",
            7 => "smpte240m",
            8 => "linear",
            9 => "log100",
            10 => "log100-sqrt10",
            11 => "iec61966-2-4",
            12 => "bt1361e",
            13 => "iec61966-2-1",
            14 => "bt2020-10",
            15 => "bt2020-12",
            16 => "pq",
            17 => "smpte428",
            18 => "hlg",
            _ => return None,
        })
    }
}

impl Default for TransferCharacteristics {
    fn default() -> Self {
        Self::UNSPECIFIED
    }
}

impl From<u8> for TransferCharacteristics {
    fn from(code_point: u8) -> Self {
        Self(code_point)
    }
}

impl std::fmt::Display for TransferCharacteristics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.name() {
            Some(n) => f.write_str(n),
            None => write!(f, "reserved({})", self.0),
        }
    }
}

/// H.273 `MatrixCoefficients` code point — the matrix used to derive
/// luma and chroma (or the identity, for GBR / XYZ signals) from the
/// colour primaries.
///
/// The inner value is the raw 8-bit code point; named constants cover
/// the values H.273 (07/2024) defines. Any other value is reserved by
/// the specification and is carried through unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MatrixCoefficients(pub u8);

impl MatrixCoefficients {
    /// Identity — GBR (RGB) or YZX (XYZ) signals; sRGB; SMPTE ST 428-1.
    pub const IDENTITY: Self = Self(0);
    /// Rec. ITU-R BT.709-6 (KR = 0.2126, KB = 0.0722).
    pub const BT709: Self = Self(1);
    /// Unspecified — unknown or determined by the application.
    pub const UNSPECIFIED: Self = Self(2);
    /// United States FCC Title 47 CFR 73.682 (KR = 0.30, KB = 0.11).
    pub const FCC: Self = Self(4);
    /// Rec. ITU-R BT.470-6 System B, G; BT.601-7 625; sYCC; xvYCC601
    /// (KR = 0.299, KB = 0.114; functionally the same as 6).
    pub const BT470_SYSTEM_BG: Self = Self(5);
    /// Rec. ITU-R BT.601-7 525; SMPTE ST 170 (KR = 0.299, KB = 0.114;
    /// functionally the same as 5).
    pub const BT601_525: Self = Self(6);
    /// SMPTE ST 240 (KR = 0.212, KB = 0.087).
    pub const SMPTE_ST240: Self = Self(7);
    /// YCgCo (equal luma / chroma depth) or YCgCo-R (chroma one bit
    /// deeper than luma).
    pub const YCGCO: Self = Self(8);
    /// Rec. ITU-R BT.2020-2 non-constant luminance; BT.2100-2 Y′CbCr.
    pub const BT2020_NCL: Self = Self(9);
    /// Rec. ITU-R BT.2020-2 constant luminance.
    pub const BT2020_CL: Self = Self(10);
    /// SMPTE ST 2085 Y′D′ZD′X.
    pub const SMPTE_ST2085: Self = Self(11);
    /// Chromaticity-derived non-constant luminance system.
    pub const CHROMATICITY_DERIVED_NCL: Self = Self(12);
    /// Chromaticity-derived constant luminance system.
    pub const CHROMATICITY_DERIVED_CL: Self = Self(13);
    /// Rec. ITU-R BT.2100-2 ICTCP.
    pub const ICTCP: Self = Self(14);
    /// IPT-C2 (SMPTE IPT-PQ-C2).
    pub const IPT_C2: Self = Self(15);
    /// YCgCo-Re.
    pub const YCGCO_RE: Self = Self(16);
    /// YCgCo-Ro.
    pub const YCGCO_RO: Self = Self(17);

    /// Wrap a raw code point.
    pub const fn new(code_point: u8) -> Self {
        Self(code_point)
    }

    /// The raw 8-bit code point.
    pub const fn code_point(self) -> u8 {
        self.0
    }

    /// `true` for code point 2.
    pub const fn is_unspecified(self) -> bool {
        self.0 == 2
    }

    /// `true` for the code points H.273 interprets without a luma /
    /// chroma split — identity (0), YCgCo (8), YCgCo-Re (16) and
    /// YCgCo-Ro (17) — i.e. the signals whose range equations scale
    /// all three components alike (equations 27–29 / 33–35).
    pub const fn is_rgb_like(self) -> bool {
        matches!(self.0, 0 | 8 | 16 | 17)
    }

    /// Short identifier for the code point when H.273 defines it
    /// (`"identity"`, `"bt709"`, `"bt2020ncl"`, …); `None` for reserved
    /// values.
    pub fn name(self) -> Option<&'static str> {
        Some(match self.0 {
            0 => "identity",
            1 => "bt709",
            2 => "unspecified",
            4 => "fcc",
            5 => "bt470bg",
            6 => "bt601-525",
            7 => "smpte240m",
            8 => "ycgco",
            9 => "bt2020ncl",
            10 => "bt2020cl",
            11 => "smpte2085",
            12 => "chroma-derived-ncl",
            13 => "chroma-derived-cl",
            14 => "ictcp",
            15 => "ipt-c2",
            16 => "ycgco-re",
            17 => "ycgco-ro",
            _ => return None,
        })
    }
}

impl Default for MatrixCoefficients {
    fn default() -> Self {
        Self::UNSPECIFIED
    }
}

impl From<u8> for MatrixCoefficients {
    fn from(code_point: u8) -> Self {
        Self(code_point)
    }
}

impl std::fmt::Display for MatrixCoefficients {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.name() {
            Some(n) => f.write_str(n),
            None => write!(f, "reserved({})", self.0),
        }
    }
}

/// The complete colour-signal description of a video stream or frame:
/// sample [`range`](Self::range) plus the H.273 triple
/// ([`primaries`](Self::primaries), [`transfer`](Self::transfer),
/// [`matrix`](Self::matrix)).
///
/// `Default` (and [`unspecified`](Self::unspecified)) leaves every
/// field unspecified. The struct is `#[non_exhaustive]` so further
/// signal descriptors (chroma sample location, …) can be added
/// without a breaking change; build it with [`new`](Self::new) or the
/// `with_*` builders and read the public fields directly.
///
/// The description never overrides the
/// [`PixelFormat`](crate::PixelFormat): the format still says how the
/// samples are stored and whether the surface is Y′CbCr, GBR or grey.
/// For the three legacy full-range formats (`YuvJ420P` / `YuvJ422P` /
/// `YuvJ444P`) the format itself implies `range == Full`; see
/// [`PixelFormat::implied_color_range`](crate::PixelFormat::implied_color_range)
/// and
/// [`CodecParameters::resolved_color_range`](crate::CodecParameters::resolved_color_range)
/// for the precedence rule (an explicit signal wins over the label).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub struct ColorSignal {
    /// Nominal sample range (H.273 `VideoFullRangeFlag`).
    pub range: ColorRange,
    /// H.273 `ColourPrimaries`.
    pub primaries: ColorPrimaries,
    /// H.273 `TransferCharacteristics`.
    pub transfer: TransferCharacteristics,
    /// H.273 `MatrixCoefficients`.
    pub matrix: MatrixCoefficients,
}

impl ColorSignal {
    /// Size in bytes of the frame side-channel encoding
    /// ([`to_bytes`](Self::to_bytes)): `range, primaries, transfer,
    /// matrix`, one byte each.
    pub const WIRE_LEN: usize = 4;

    /// Build a description from its four parts.
    pub const fn new(
        range: ColorRange,
        primaries: ColorPrimaries,
        transfer: TransferCharacteristics,
        matrix: MatrixCoefficients,
    ) -> Self {
        Self {
            range,
            primaries,
            transfer,
            matrix,
        }
    }

    /// Build a description straight from H.273 code points and the
    /// `VideoFullRangeFlag`, as read from a bitstream or container.
    pub const fn from_code_points(
        primaries: u8,
        transfer: u8,
        matrix: u8,
        full_range: bool,
    ) -> Self {
        Self {
            range: if full_range {
                ColorRange::Full
            } else {
                ColorRange::Limited
            },
            primaries: ColorPrimaries(primaries),
            transfer: TransferCharacteristics(transfer),
            matrix: MatrixCoefficients(matrix),
        }
    }

    /// Every field unspecified — identical to `Default`.
    pub const fn unspecified() -> Self {
        Self {
            range: ColorRange::Unspecified,
            primaries: ColorPrimaries::UNSPECIFIED,
            transfer: TransferCharacteristics::UNSPECIFIED,
            matrix: MatrixCoefficients::UNSPECIFIED,
        }
    }

    /// sRGB (IEC 61966-2-1): BT.709 primaries, sRGB transfer, identity
    /// matrix, full range. The signal of most 8-bit RGB stills.
    pub const fn srgb() -> Self {
        Self::new(
            ColorRange::Full,
            ColorPrimaries::BT709,
            TransferCharacteristics::IEC61966_2_1,
            MatrixCoefficients::IDENTITY,
        )
    }

    /// Rec. ITU-R BT.709 Y′CbCr, limited range — the HD television
    /// baseline (`1 / 1 / 1`, `VideoFullRangeFlag == 0`).
    pub const fn bt709_limited() -> Self {
        Self::new(
            ColorRange::Limited,
            ColorPrimaries::BT709,
            TransferCharacteristics::BT709,
            MatrixCoefficients::BT709,
        )
    }

    /// Builder: replace the range.
    pub const fn with_range(mut self, range: ColorRange) -> Self {
        self.range = range;
        self
    }

    /// Builder: replace the colour primaries.
    pub const fn with_primaries(mut self, primaries: ColorPrimaries) -> Self {
        self.primaries = primaries;
        self
    }

    /// Builder: replace the transfer characteristics.
    pub const fn with_transfer(mut self, transfer: TransferCharacteristics) -> Self {
        self.transfer = transfer;
        self
    }

    /// Builder: replace the matrix coefficients.
    pub const fn with_matrix(mut self, matrix: MatrixCoefficients) -> Self {
        self.matrix = matrix;
        self
    }

    /// `true` when no field carries a decision (the `Default` value).
    pub fn is_unspecified(&self) -> bool {
        self.range.is_unspecified()
            && self.primaries.is_unspecified()
            && self.transfer.is_unspecified()
            && self.matrix.is_unspecified()
    }

    /// Field-wise merge: every field of `self` that is unspecified is
    /// taken from `fallback`; specified fields are kept. The natural
    /// way to layer a frame-level description over the stream-level
    /// one, or a stream-level description over a container's.
    pub fn or(self, fallback: Self) -> Self {
        Self {
            range: if self.range.is_unspecified() {
                fallback.range
            } else {
                self.range
            },
            primaries: if self.primaries.is_unspecified() {
                fallback.primaries
            } else {
                self.primaries
            },
            transfer: if self.transfer.is_unspecified() {
                fallback.transfer
            } else {
                self.transfer
            },
            matrix: if self.matrix.is_unspecified() {
                fallback.matrix
            } else {
                self.matrix
            },
        }
    }

    /// Fixed-size wire form used by the [`VideoFrame`](crate::VideoFrame)
    /// side-channel record: `[range, primaries, transfer, matrix]`
    /// where `range` is `0` unspecified / `1` limited / `2` full and
    /// the other three are the raw H.273 code points.
    pub fn to_bytes(&self) -> [u8; Self::WIRE_LEN] {
        [
            self.range.to_byte(),
            self.primaries.0,
            self.transfer.0,
            self.matrix.0,
        ]
    }

    /// Inverse of [`to_bytes`](Self::to_bytes). Accepts any slice of at
    /// least [`WIRE_LEN`](Self::WIRE_LEN) bytes (extra trailing bytes are
    /// reserved for future descriptors and ignored); returns `None`
    /// for a shorter slice. An unknown range byte decodes as
    /// unspecified.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let b = bytes.get(..Self::WIRE_LEN)?;
        Some(Self {
            range: ColorRange::from_byte(b[0]),
            primaries: ColorPrimaries(b[1]),
            transfer: TransferCharacteristics(b[2]),
            matrix: MatrixCoefficients(b[3]),
        })
    }
}

impl std::fmt::Display for ColorSignal {
    /// `primaries/transfer/matrix range`, e.g. `bt709/bt709/bt709 limited`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let range = match self.range {
            ColorRange::Limited => "limited",
            ColorRange::Full => "full",
            _ => "unspecified-range",
        };
        write!(
            f,
            "{}/{}/{} {}",
            self.primaries, self.transfer, self.matrix, range
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_fully_unspecified_and_matches_h273_code_point_2() {
        let s = ColorSignal::default();
        assert!(s.is_unspecified());
        assert_eq!(s, ColorSignal::unspecified());
        assert_eq!(s.range, ColorRange::Unspecified);
        assert_eq!(s.primaries.code_point(), 2);
        assert_eq!(s.transfer.code_point(), 2);
        assert_eq!(s.matrix.code_point(), 2);
        assert_eq!(ColorPrimaries::default(), ColorPrimaries::UNSPECIFIED);
        assert_eq!(
            TransferCharacteristics::default(),
            TransferCharacteristics::UNSPECIFIED
        );
        assert_eq!(
            MatrixCoefficients::default(),
            MatrixCoefficients::UNSPECIFIED
        );
    }

    #[test]
    fn named_constants_carry_the_h273_code_points() {
        // Table 2.
        assert_eq!(ColorPrimaries::BT709.0, 1);
        assert_eq!(ColorPrimaries::BT470_SYSTEM_M.0, 4);
        assert_eq!(ColorPrimaries::BT470_SYSTEM_BG.0, 5);
        assert_eq!(ColorPrimaries::BT601_525.0, 6);
        assert_eq!(ColorPrimaries::SMPTE_ST240.0, 7);
        assert_eq!(ColorPrimaries::GENERIC_FILM.0, 8);
        assert_eq!(ColorPrimaries::BT2020.0, 9);
        assert_eq!(ColorPrimaries::SMPTE_ST428.0, 10);
        assert_eq!(ColorPrimaries::SMPTE_RP431.0, 11);
        assert_eq!(ColorPrimaries::SMPTE_EG432.0, 12);
        // Table 3.
        assert_eq!(TransferCharacteristics::BT709.0, 1);
        assert_eq!(TransferCharacteristics::GAMMA22.0, 4);
        assert_eq!(TransferCharacteristics::GAMMA28.0, 5);
        assert_eq!(TransferCharacteristics::BT601.0, 6);
        assert_eq!(TransferCharacteristics::SMPTE_ST240.0, 7);
        assert_eq!(TransferCharacteristics::LINEAR.0, 8);
        assert_eq!(TransferCharacteristics::LOG100.0, 9);
        assert_eq!(TransferCharacteristics::LOG100_SQRT10.0, 10);
        assert_eq!(TransferCharacteristics::IEC61966_2_4.0, 11);
        assert_eq!(TransferCharacteristics::BT1361_EXTENDED.0, 12);
        assert_eq!(TransferCharacteristics::IEC61966_2_1.0, 13);
        assert_eq!(TransferCharacteristics::BT2020_10BIT.0, 14);
        assert_eq!(TransferCharacteristics::BT2020_12BIT.0, 15);
        assert_eq!(TransferCharacteristics::SMPTE_ST2084.0, 16);
        assert_eq!(TransferCharacteristics::SMPTE_ST428.0, 17);
        assert_eq!(TransferCharacteristics::ARIB_STD_B67.0, 18);
        // Table 4.
        assert_eq!(MatrixCoefficients::IDENTITY.0, 0);
        assert_eq!(MatrixCoefficients::BT709.0, 1);
        assert_eq!(MatrixCoefficients::FCC.0, 4);
        assert_eq!(MatrixCoefficients::BT470_SYSTEM_BG.0, 5);
        assert_eq!(MatrixCoefficients::BT601_525.0, 6);
        assert_eq!(MatrixCoefficients::SMPTE_ST240.0, 7);
        assert_eq!(MatrixCoefficients::YCGCO.0, 8);
        assert_eq!(MatrixCoefficients::BT2020_NCL.0, 9);
        assert_eq!(MatrixCoefficients::BT2020_CL.0, 10);
        assert_eq!(MatrixCoefficients::SMPTE_ST2085.0, 11);
        assert_eq!(MatrixCoefficients::CHROMATICITY_DERIVED_NCL.0, 12);
        assert_eq!(MatrixCoefficients::CHROMATICITY_DERIVED_CL.0, 13);
        assert_eq!(MatrixCoefficients::ICTCP.0, 14);
        assert_eq!(MatrixCoefficients::IPT_C2.0, 15);
        assert_eq!(MatrixCoefficients::YCGCO_RE.0, 16);
        assert_eq!(MatrixCoefficients::YCGCO_RO.0, 17);
    }

    #[test]
    fn reserved_code_points_pass_through_and_have_no_name() {
        for cp in [0u8, 3, 13, 21, 23, 200, 255] {
            let p = ColorPrimaries::new(cp);
            assert_eq!(p.code_point(), cp);
            assert_eq!(p.name(), None);
            assert_eq!(p.to_string(), format!("reserved({cp})"));
        }
        assert_eq!(TransferCharacteristics::new(0).name(), None);
        assert_eq!(TransferCharacteristics::new(19).name(), None);
        assert_eq!(MatrixCoefficients::new(3).name(), None);
        assert_eq!(MatrixCoefficients::new(18).name(), None);
        // Defined values all have names.
        for cp in [1u8, 2, 4, 5, 6, 7, 8, 9, 10, 11, 12, 22] {
            assert!(ColorPrimaries::new(cp).name().is_some(), "cp {cp}");
        }
        for cp in 1u8..=18 {
            if cp != 3 {
                assert!(TransferCharacteristics::new(cp).name().is_some(), "tc {cp}");
            }
        }
        for cp in 0u8..=17 {
            if cp != 3 {
                assert!(MatrixCoefficients::new(cp).name().is_some(), "mc {cp}");
            }
        }
    }

    #[test]
    fn range_flag_round_trip() {
        assert_eq!(ColorRange::from_full_range_flag(true), ColorRange::Full);
        assert_eq!(ColorRange::from_full_range_flag(false), ColorRange::Limited);
        assert_eq!(ColorRange::Full.full_range_flag(), Some(true));
        assert_eq!(ColorRange::Limited.full_range_flag(), Some(false));
        assert_eq!(ColorRange::Unspecified.full_range_flag(), None);
        assert!(ColorRange::Unspecified.is_unspecified());
        assert!(!ColorRange::Full.is_unspecified());
        assert!(!ColorRange::Limited.is_unspecified());
    }

    #[test]
    fn from_code_points_and_builders() {
        // BT.2020 PQ, limited range, as a container colour record
        // would carry it: 9 / 16 / 9, full_range_flag 0.
        let s = ColorSignal::from_code_points(9, 16, 9, false);
        assert_eq!(s.primaries, ColorPrimaries::BT2020);
        assert_eq!(s.transfer, TransferCharacteristics::SMPTE_ST2084);
        assert_eq!(s.matrix, MatrixCoefficients::BT2020_NCL);
        assert_eq!(s.range, ColorRange::Limited);
        assert!(!s.is_unspecified());

        let t = ColorSignal::unspecified()
            .with_range(ColorRange::Full)
            .with_primaries(ColorPrimaries::BT709)
            .with_transfer(TransferCharacteristics::IEC61966_2_1)
            .with_matrix(MatrixCoefficients::IDENTITY);
        assert_eq!(t, ColorSignal::srgb());
        assert_eq!(
            ColorSignal::bt709_limited(),
            ColorSignal::from_code_points(1, 1, 1, false)
        );
    }

    #[test]
    fn or_fills_only_unspecified_fields() {
        let frame = ColorSignal::unspecified().with_range(ColorRange::Full);
        let stream = ColorSignal::bt709_limited();
        let merged = frame.or(stream);
        assert_eq!(merged.range, ColorRange::Full);
        assert_eq!(merged.primaries, ColorPrimaries::BT709);
        assert_eq!(merged.transfer, TransferCharacteristics::BT709);
        assert_eq!(merged.matrix, MatrixCoefficients::BT709);
        // A fully specified value is unchanged by any fallback.
        assert_eq!(ColorSignal::srgb().or(stream), ColorSignal::srgb());
        // Unspecified over unspecified stays unspecified.
        assert!(
            ColorSignal::default()
                .or(ColorSignal::default())
                .is_unspecified()
        );
    }

    #[test]
    fn wire_bytes_round_trip_and_tolerate_extra_or_unknown() {
        let s = ColorSignal::from_code_points(12, 13, 0, true);
        let bytes = s.to_bytes();
        assert_eq!(bytes, [2, 12, 13, 0]);
        assert_eq!(ColorSignal::from_bytes(&bytes), Some(s));
        // Unspecified encodes as all-2 with range byte 0.
        assert_eq!(ColorSignal::default().to_bytes(), [0, 2, 2, 2]);
        // Limited is byte 1.
        assert_eq!(ColorSignal::bt709_limited().to_bytes()[0], 1);
        // Extra trailing bytes are ignored (forward compatibility).
        assert_eq!(
            ColorSignal::from_bytes(&[1, 1, 1, 1, 0xAA, 0xBB]),
            Some(ColorSignal::bt709_limited())
        );
        // Too short → None.
        assert_eq!(ColorSignal::from_bytes(&[1, 1, 1]), None);
        assert_eq!(ColorSignal::from_bytes(&[]), None);
        // Unknown range byte decodes as unspecified.
        assert_eq!(
            ColorSignal::from_bytes(&[7, 1, 1, 1]).unwrap().range,
            ColorRange::Unspecified
        );
    }

    #[test]
    fn display_is_compact() {
        assert_eq!(
            ColorSignal::bt709_limited().to_string(),
            "bt709/bt709/bt709 limited"
        );
        assert_eq!(
            ColorSignal::srgb().to_string(),
            "bt709/iec61966-2-1/identity full"
        );
        assert_eq!(
            ColorSignal::default().to_string(),
            "unspecified/unspecified/unspecified unspecified-range"
        );
        assert_eq!(
            ColorSignal::from_code_points(9, 18, 200, false).to_string(),
            "bt2020/hlg/reserved(200) limited"
        );
    }

    #[test]
    fn rgb_like_matrices() {
        assert!(MatrixCoefficients::IDENTITY.is_rgb_like());
        assert!(MatrixCoefficients::YCGCO.is_rgb_like());
        assert!(MatrixCoefficients::YCGCO_RE.is_rgb_like());
        assert!(MatrixCoefficients::YCGCO_RO.is_rgb_like());
        assert!(!MatrixCoefficients::BT709.is_rgb_like());
        assert!(!MatrixCoefficients::UNSPECIFIED.is_rgb_like());
    }

    #[test]
    fn code_point_newtypes_convert_from_u8_and_clone() {
        let p: ColorPrimaries = 9u8.into();
        assert_eq!(p, ColorPrimaries::BT2020);
        let t: TransferCharacteristics = 16u8.into();
        assert_eq!(t, TransferCharacteristics::SMPTE_ST2084);
        let m: MatrixCoefficients = 9u8.into();
        assert_eq!(m, MatrixCoefficients::BT2020_NCL);
        let s = ColorSignal::new(ColorRange::Limited, p, t, m);
        #[allow(clippy::clone_on_copy)]
        let c = s.clone();
        assert_eq!(c, s);
    }
}
