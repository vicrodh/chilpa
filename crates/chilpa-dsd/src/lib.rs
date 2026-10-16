//! DSD: read one bit per sample and hand it over untouched.
//!
//! DSD is not "decoded": the bits in the file are the bits the DAC has to
//! receive. This crate opens the containers, DSF and DSDIFF (a SACD image is
//! `chilpa-sacd`'s), normalizes the bit order inside the byte, and hands over
//! planar blocks per channel, ready for the DoP packing in `chilpa-audio` or
//! for a native output.
//!
//! The only thing touched is the bit order inside the byte, because DSF
//! stores the earliest bit in the least significant position and the cable
//! wants it the other way round. That inversion is exact and reversible.

mod convert;
pub mod dff;
pub mod dsf;

pub use convert::{DsdPcmConverter, DEFAULT_GAIN_DB, OUTPUT_RATE};
pub use dff::DffReader;
pub use dsf::DsfReader;

/// What can go wrong reading DSD.
#[derive(Debug)]
pub enum DsdError {
    Open(std::io::Error),
    /// The file is not what it says it is.
    Corrupt(String),
    /// A rate or a channel count we do not handle.
    Unsupported(String),
}

impl std::fmt::Display for DsdError {
    /// The reasons are marked where they are made and looked up here. One
    /// already put into words with what it carries is no msgid, and `t`
    /// hands it back as it is.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DsdError::Open(e) => {
                write!(f, "{}", chilpa_i18n::t_args("could not open: {}", &[&e.to_string()]))
            }
            DsdError::Corrupt(s) => {
                write!(f, "{}", chilpa_i18n::t_args("damaged DSD file: {}", &[&chilpa_i18n::t(s)]))
            }
            DsdError::Unsupported(s) => {
                write!(f, "{}", chilpa_i18n::t_args("DSD not supported: {}", &[&chilpa_i18n::t(s)]))
            }
        }
    }
}

impl From<std::io::Error> for DsdError {
    fn from(e: std::io::Error) -> Self {
        DsdError::Open(e)
    }
}

/// What is known about a DSD stream before reading it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DsdInfo {
    /// DSD bits per second per channel: 2,822,400 is DSD64.
    pub dsd_rate_hz: u32,
    pub channels: u16,
    /// Total bits per channel, which is the sample count.
    pub samples: u64,
}

impl DsdInfo {
    /// The multiple of 44,100 that names the rate: 64 for DSD64.
    pub fn multiple(&self) -> u32 {
        self.dsd_rate_hz / 44_100
    }

    /// Duration in seconds.
    pub fn seconds(&self) -> f64 {
        if self.dsd_rate_hz == 0 {
            return 0.0;
        }
        self.samples as f64 / f64::from(self.dsd_rate_hz)
    }
}

/// An open DSD source. It is read on a thread of its own, ahead of the
/// device, so it has to be able to move there.
pub trait DsdSource: Send {
    fn info(&self) -> &DsdInfo;

    /// The next block: one byte vector per channel, all the same length, with
    /// the earliest bit in the most significant position. `None` when it is
    /// over.
    fn next_block(&mut self) -> Result<Option<Vec<Vec<u8>>>, DsdError>;

    /// Moves to one-bit sample `sample` of each channel, or to a point before
    /// it the container can land on, and says which sample that is. The
    /// caller drops the difference. `Ok(None)` when this source cannot move:
    /// it goes on from where it was.
    fn seek(&mut self, sample: u64) -> Result<Option<u64>, DsdError> {
        let _ = sample;
        Ok(None)
    }
}

/// Reverses the bit order of a byte.
///
/// DSF stores the earliest bit in the least significant position; DoP and
/// native outputs want it in the most significant one.
pub const fn reverse_bits(b: u8) -> u8 {
    b.reverse_bits()
}

/// The DSD rates we accept, from DSD64 to DSD512.
pub fn is_supported_rate(rate_hz: u32) -> bool {
    matches!(rate_hz, 2_822_400 | 5_644_800 | 11_289_600 | 22_579_200)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dsd_rates_are_multiples_of_44_100() {
        assert!(is_supported_rate(2_822_400));
        assert!(is_supported_rate(11_289_600));
        assert!(!is_supported_rate(44_100), "that is PCM");
        assert!(!is_supported_rate(3_000_000));
    }

    #[test]
    fn the_multiple_names_the_rate() {
        let info = DsdInfo { dsd_rate_hz: 2_822_400, channels: 2, samples: 2_822_400 };
        assert_eq!(info.multiple(), 64);
        assert_eq!(info.seconds(), 1.0);
    }

    #[test]
    fn inverting_the_bits_is_reversible() {
        for b in [0u8, 1, 0x80, 0xAB, 0xFF] {
            assert_eq!(reverse_bits(reverse_bits(b)), b);
        }
        assert_eq!(reverse_bits(0b0000_0001), 0b1000_0000);
    }
}
