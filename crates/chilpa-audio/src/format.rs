//! What a stream looks like on the wire.

/// What the device accepts in each sample.
///
/// There is no "hi-res" format and no flags: there is a sample format, a
/// rate and a channel count.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum SampleFormat {
    /// 16-bit integer, little endian.
    S16,
    /// 24-bit integer in 3 bytes.
    S24Packed,
    /// 24-bit integer inside 32, left-justified: the 24 valid bits go in the
    /// high bytes and the low byte stays zero.
    S24In32,
    /// 32-bit integer.
    S32,
    /// 32-bit float. The device converts, so what reaches the DAC is no
    /// longer what we produced.
    F32,
}

impl SampleFormat {
    /// Bytes a sample takes on the wire.
    pub const fn bytes(self) -> usize {
        match self {
            SampleFormat::S16 => 2,
            SampleFormat::S24Packed => 3,
            SampleFormat::S24In32 | SampleFormat::S32 | SampleFormat::F32 => 4,
        }
    }

    /// Bits carrying information, not bits on the wire. `S24In32` carries 24.
    pub const fn valid_bits(self) -> u8 {
        match self {
            SampleFormat::S16 => 16,
            SampleFormat::S24Packed | SampleFormat::S24In32 => 24,
            SampleFormat::S32 => 32,
            SampleFormat::F32 => 32,
        }
    }

    /// True when the device receives integers as they are. A float stream can
    /// never be bit-perfect, because somebody converts.
    pub const fn is_integer(self) -> bool {
        !matches!(self, SampleFormat::F32)
    }
}

/// One concrete stream: format, rate and channels.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct StreamFormat {
    pub sample_format: SampleFormat,
    pub sample_rate_hz: u32,
    pub channels: u16,
}

impl StreamFormat {
    pub const fn new(sample_format: SampleFormat, sample_rate_hz: u32, channels: u16) -> Self {
        Self { sample_format, sample_rate_hz, channels }
    }

    pub const fn frame_bytes(&self) -> usize {
        self.sample_format.bytes() * self.channels as usize
    }

    /// True when both can play without reconfiguring the device. Changing
    /// rate or format between tracks forces a renegotiation, and that is
    /// where QBZ put clicks.
    pub fn same_stream(&self, other: &Self) -> bool {
        self == other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_format_knows_its_bytes_and_its_useful_bits() {
        assert_eq!(SampleFormat::S24Packed.bytes(), 3);
        assert_eq!(SampleFormat::S24In32.bytes(), 4);
        assert_eq!(SampleFormat::S24In32.valid_bits(), 24, "van 24 bits en 32");
    }

    #[test]
    fn floats_are_not_integers() {
        assert!(SampleFormat::S32.is_integer());
        assert!(!SampleFormat::F32.is_integer());
    }

    #[test]
    fn a_stereo_frame_of_24_in_32_is_eight_bytes() {
        assert_eq!(StreamFormat::new(SampleFormat::S24In32, 96_000, 2).frame_bytes(), 8);
    }
}
