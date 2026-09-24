//! From decoded samples to the bytes on the wire.
//!
//! The rule: **an integer never goes through a float.** In QBZ the packer
//! took `f32` and scaled it back (`s * 32767.0`, with rounding), so a
//! 24-bit sample could come out 1 LSB low while the UI said
//! "bit-perfect". Here integers are laid out with shifts, which lose
//! nothing, and the float path exists only for sources that really are
//! float.

use crate::format::SampleFormat;

/// Already decoded samples, in the type they arrived as.
pub enum Samples<'a> {
    /// Signed integers, right-justified, with `bits` useful ones.
    Int { data: &'a [i32], bits: u8 },
    /// Floats in the range [-1.0, 1.0].
    Float(&'a [f32]),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PackError {
    /// The source's bits do not fit the wire format without throwing
    /// information away. Nothing is ever truncated silently.
    WouldTruncate { from_bits: u8, to_bits: u8 },
}

/// Writes the samples in the wire format.
///
/// Widening (16 to 24, say) is exact: a left shift. Narrowing is not: it is
/// refused, and the caller decides whether to dither or change device. An
/// `f32` into an integer format does scale, and that is no longer
/// bit-perfect; the bit-perfect path never arrives here with floats.
pub fn pack(samples: &Samples<'_>, to: SampleFormat, out: &mut Vec<u8>) -> Result<(), PackError> {
    match samples {
        Samples::Int { data, bits } => pack_int(data, *bits, to, out),
        Samples::Float(data) => {
            pack_float(data, to, out);
            Ok(())
        }
    }
}

fn pack_int(data: &[i32], bits: u8, to: SampleFormat, out: &mut Vec<u8>) -> Result<(), PackError> {
    if to == SampleFormat::F32 {
        // The device only takes floats: convert, and the caller already knows
        // from `Delivery` that this is not bit-perfect.
        out.reserve(data.len() * 4);
        let full = (1i64 << (bits - 1)) as f32;
        for &v in data {
            out.extend_from_slice(&(v as f32 / full).to_le_bytes());
        }
        return Ok(());
    }
    let target = to.valid_bits();
    if bits > target {
        return Err(PackError::WouldTruncate { from_bits: bits, to_bits: target });
    }
    let shift = target - bits;
    out.reserve(data.len() * to.bytes());
    for &v in data {
        let v = v << shift;
        match to {
            SampleFormat::S16 => out.extend_from_slice(&(v as i16).to_le_bytes()),
            SampleFormat::S24Packed => out.extend_from_slice(&v.to_le_bytes()[..3]),
            // Left-justified: the 24 valid bits take the high bytes and the low
            // one stays zero.
            SampleFormat::S24In32 => out.extend_from_slice(&(v << 8).to_le_bytes()),
            SampleFormat::S32 => out.extend_from_slice(&v.to_le_bytes()),
            SampleFormat::F32 => unreachable!("handled above"),
        }
    }
    Ok(())
}

fn pack_float(data: &[f32], to: SampleFormat, out: &mut Vec<u8>) {
    out.reserve(data.len() * to.bytes());
    for &s in data {
        let s = s.clamp(-1.0, 1.0);
        match to {
            SampleFormat::F32 => out.extend_from_slice(&s.to_le_bytes()),
            SampleFormat::S16 => out.extend_from_slice(&((s * 32767.0).round() as i16).to_le_bytes()),
            SampleFormat::S24Packed => {
                out.extend_from_slice(&((s * 8_388_607.0).round() as i32).to_le_bytes()[..3])
            }
            SampleFormat::S24In32 => {
                let v = (s * 8_388_607.0).round() as i32;
                out.extend_from_slice(&(v << 8).to_le_bytes());
            }
            SampleFormat::S32 => {
                let v = (f64::from(s) * f64::from(i32::MAX)).round() as i32;
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packed(samples: &Samples<'_>, to: SampleFormat) -> Vec<u8> {
        let mut out = Vec::new();
        pack(samples, to, &mut out).unwrap();
        out
    }

    #[test]
    fn a_16_bit_integer_comes_out_identical_in_16() {
        let data = [0i32, 1, -1, 32767, -32768];
        let bytes = packed(&Samples::Int { data: &data, bits: 16 }, SampleFormat::S16);
        let wrap: Vec<i32> = bytes
            .chunks(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]) as i32)
            .collect();
        assert_eq!(wrap, data, "a round trip without losing a single LSB");
    }

    #[test]
    fn going_from_16_to_24_is_an_exact_shift() {
        let data = [1i32, -1, 32767];
        let bytes = packed(&Samples::Int { data: &data, bits: 16 }, SampleFormat::S24In32);
        let wrap: Vec<i32> = bytes
            .chunks(4)
            .map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]]) >> 8)
            .collect();
        assert_eq!(wrap, data.iter().map(|v| v << 8).collect::<Vec<_>>());
    }

    #[test]
    fn the_maximum_24_bit_value_keeps_its_last_lsb() {
        // This is the finding from QBZ's packer: scaling by 8_388_607.0 and
        // rounding moved loud samples by an LSB.
        let data = [8_388_607i32, -8_388_608, 4_194_303];
        let bytes = packed(&Samples::Int { data: &data, bits: 24 }, SampleFormat::S24Packed);
        let wrap: Vec<i32> = bytes
            .chunks(3)
            .map(|c| (i32::from_le_bytes([0, c[0], c[1], c[2]])) >> 8)
            .collect();
        assert_eq!(wrap, data);
    }

    #[test]
    fn the_low_byte_of_24_in_32_stays_zero() {
        let bytes = packed(&Samples::Int { data: &[0x7FFFFF], bits: 24 }, SampleFormat::S24In32);
        assert_eq!(bytes[0], 0, "left justified");
    }

    #[test]
    fn narrowing_bits_is_refused_instead_of_truncating_silently() {
        let mut out = Vec::new();
        let r = pack(&Samples::Int { data: &[1], bits: 24 }, SampleFormat::S16, &mut out);
        assert_eq!(r, Err(PackError::WouldTruncate { from_bits: 24, to_bits: 16 }));
        assert!(out.is_empty());
    }
}
