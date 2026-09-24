//! SACD images: read the disc and hand its tracks over as DSD.
//!
//! A SACD image is a file of 2048-byte sectors with two tables of contents
//! (the master one and the stereo area's) and an audio area where the DSD
//! bits travel packed per sector.
//!
//! This crate only reads: it never writes to the image nor converts the
//! signal. What it hands over are the same bits the disc carries, so the DSD
//! path that already exists (DoP or native output) sends them to the DAC
//! untouched.

pub mod frames;
pub mod sector;
pub mod toc;

pub use frames::SacdTrackReader;
pub use toc::{open, SacdArea, SacdTrack};

/// Un sector de SACD.
pub const SECTOR: usize = 2048;
/// Sectors per second of playback.
pub const SECTORS_PER_SEC: u32 = 350;
/// An uncompressed stereo DSD64 frame: 1/75 of a second.
pub const STEREO_FRAME: usize = 9408;
/// The only rate the Scarlet Book's stereo area uses.
pub const DSD64_HZ: u32 = 2_822_400;
/// The data type that marks audio in a packet.
pub const DATA_TYPE_AUDIO: u16 = 2;

/// How the area's bits are stored.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Encoding {
    /// Uncompressed DSD, 3 bytes in 14.
    Dsd3In14,
    /// Uncompressed DSD, 3 bytes in 16.
    Dsd3In16,
    /// DST compressed. Cannot be played yet.
    Dst,
}

impl Encoding {
    pub fn is_compressed(self) -> bool {
        self == Encoding::Dst
    }
}

#[derive(Debug)]
pub enum SacdError {
    Open(std::io::Error),
    /// The image is not shaped like a SACD.
    MalformedToc(String),
    /// A sector that does not add up to what its header says.
    BadSector { lsn: u64, why: String },
    /// Something the disc carries that we do not handle yet.
    Unsupported(String),
}

impl std::fmt::Display for SacdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SacdError::Open(e) => write!(f, "no se pudo leer la imagen: {e}"),
            SacdError::MalformedToc(s) => write!(f, "tabla de contenido inválida: {s}"),
            SacdError::BadSector { lsn, why } => write!(f, "sector {lsn} inválido: {why}"),
            SacdError::Unsupported(s) => write!(f, "no soportado: {s}"),
        }
    }
}

impl From<std::io::Error> for SacdError {
    fn from(e: std::io::Error) -> Self {
        SacdError::Open(e)
    }
}

pub(crate) fn be16(b: &[u8], at: usize) -> u16 {
    u16::from_be_bytes([b[at], b[at + 1]])
}

pub(crate) fn be32(b: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// A disc timecode (minutes, seconds, 1/75 s frames) in
/// frames.
pub(crate) fn timecode_frames(b: &[u8], at: usize) -> u32 {
    (u32::from(b[at]) * 60 + u32::from(b[at + 1])) * 75 + u32::from(b[at + 2])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stereo_frame_lasts_one_seventy_fifth_of_a_second() {
        // 2,822,400 bits per channel per second, 8 bits per byte, 2 channels,
        // 75 frames per second.
        assert_eq!(DSD64_HZ as usize / 8 * 2 / 75, STEREO_FRAME);
    }

    #[test]
    fn the_timecode_becomes_frames() {
        assert_eq!(timecode_frames(&[0, 0, 1], 0), 1);
        assert_eq!(timecode_frames(&[1, 30, 0], 0), (60 + 30) * 75);
    }

    #[test]
    fn dst_is_recognized_as_compressed() {
        assert!(Encoding::Dst.is_compressed());
        assert!(!Encoding::Dsd3In14.is_compressed());
    }
}
