//! Playing an audio CD by reading its sectors.
//!
//! A CD-DA is not decoded: every sector carries 2352 bytes that ALREADY are
//! 16-bit stereo PCM at 44,100 Hz, little endian. All there is to do is read
//! them and hand them over, and that is what makes playing a disc
//! bit-perfect.
//!
//! The drive lives behind [`CdDevice`], so the logic is tested with a fake
//! device and the real access stays in one place.

pub mod toc;

#[cfg(target_os = "linux")]
pub mod linux;

pub use toc::{build, Toc, TocEntry, Track, SECTOR_BYTES, SECTORS_PER_SEC};

use chilpa_audio::format::{SampleFormat, StreamFormat};

/// The format every audio CD has.
pub const CDDA_FORMAT: StreamFormat = StreamFormat::new(SampleFormat::S16, 44_100, 2);
/// Stereo frames in a sector.
pub const FRAMES_PER_SECTOR: usize = SECTOR_BYTES / 4;
/// The most the kernel accepts per read.
pub const MAX_SECTORS_PER_READ: u32 = 75;

#[derive(Debug)]
pub enum CdError {
    /// The drive could not be talked to.
    Device(String),
    /// The drive is empty, or has not recognized the disc yet.
    NoDisc,
    /// The disc carries no audio tracks.
    NotAudio,
    /// An unreadable sector, with its position.
    Read { lsn: u32, why: String },
}

impl std::fmt::Display for CdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CdError::Device(s) => {
                write!(f, "{}", chilpa_i18n::t_args("the drive does not answer: {}", &[s]))
            }
            CdError::NoDisc => {
                write!(f, "{}", chilpa_i18n::t("there is no disc in the drive"))
            }
            CdError::NotAudio => {
                write!(f, "{}", chilpa_i18n::t("the disc carries no audio tracks"))
            }
            CdError::Read { lsn, why } => write!(
                f,
                "{}",
                chilpa_i18n::t_args("sector {} could not be read: {}", &[&lsn.to_string(), why])
            ),
        }
    }
}

/// A disc drive.
pub trait CdDevice {
    /// Reads the raw TOC: the entries and the lead-out.
    fn read_toc(&mut self) -> Result<(Vec<TocEntry>, u32), CdError>;

    /// Reads `sectors` audio sectors from `lsn` into `out`.
    fn read_audio(&mut self, lsn: u32, sectors: u32, out: &mut Vec<u8>) -> Result<(), CdError>;
}

/// Reads a drive's TOC and works out the durations.
pub fn read_toc(device: &mut dyn CdDevice) -> Result<Toc, CdError> {
    let (entries, leadout) = device.read_toc()?;
    if entries.is_empty() {
        return Err(CdError::NoDisc);
    }
    let toc = build(&entries, leadout);
    if toc.audio_tracks().count() == 0 {
        return Err(CdError::NotAudio);
    }
    Ok(toc)
}

/// Hands a CD track over as PCM blocks, ready for the output device.
pub struct TrackReader<'a> {
    device: &'a mut dyn CdDevice,
    lsn: u32,
    end_lsn: u32,
    /// Sectors per read. More sectors means fewer round trips with the
    /// drive; the kernel takes no more than 75.
    chunk: u32,
    /// The first unreadable sector, if there was one.
    pub error: Option<CdError>,
}

impl<'a> TrackReader<'a> {
    pub fn new(device: &'a mut dyn CdDevice, track: &Track) -> Self {
        Self {
            device,
            lsn: track.start_lsn,
            end_lsn: track.end_lsn(),
            chunk: MAX_SECTORS_PER_READ,
            error: None,
        }
    }

    /// How many sectors are left.
    pub fn remaining(&self) -> u32 {
        self.end_lsn.saturating_sub(self.lsn)
    }
}

impl chilpa_audio::render::FrameSource for TrackReader<'_> {
    fn next_block(&mut self) -> Option<Vec<u8>> {
        if self.lsn >= self.end_lsn || self.error.is_some() {
            return None;
        }
        let sectors = self.chunk.min(self.remaining());
        let mut buf = Vec::new();
        match self.device.read_audio(self.lsn, sectors, &mut buf) {
            Ok(()) => {
                self.lsn += sectors;
                Some(buf)
            }
            Err(e) => {
                // An unreadable sector is not quietly filled with silence: the track
                // is cut and it is reported.
                self.error = Some(e);
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chilpa_audio::render::FrameSource;

    struct FakeSink {
        fail_at: Option<u32>,
    }

    impl CdDevice for FakeSink {
        fn read_toc(&mut self) -> Result<(Vec<TocEntry>, u32), CdError> {
            Ok((
                vec![
                    TocEntry { number: 1, start_lsn: 0, is_audio: true, session: 1 },
                    TocEntry { number: 2, start_lsn: 150, is_audio: true, session: 1 },
                ],
                300,
            ))
        }

        fn read_audio(&mut self, lsn: u32, sectors: u32, out: &mut Vec<u8>) -> Result<(), CdError> {
            if self.fail_at == Some(lsn) {
                return Err(CdError::Read { lsn, why: "error de C2".into() });
            }
            out.clear();
            for s in 0..sectors {
                for i in 0..SECTOR_BYTES {
                    out.push(((lsn + s) as usize + i) as u8);
                }
            }
            Ok(())
        }
    }

    #[test]
    fn a_cd_always_has_the_same_format() {
        assert_eq!(CDDA_FORMAT.sample_rate_hz, 44_100);
        assert_eq!(CDDA_FORMAT.sample_format, SampleFormat::S16);
        assert_eq!(FRAMES_PER_SECTOR, 588);
    }

    #[test]
    fn the_toc_is_read_with_its_durations() {
        let mut d = FakeSink { fail_at: None };
        let toc = read_toc(&mut d).unwrap();
        assert_eq!(toc.tracks.len(), 2);
        assert_eq!(toc.tracks[0].sectors, 150);
        assert_eq!(toc.tracks[1].sectors, 150);
    }

    #[test]
    fn a_track_is_delivered_whole_and_in_blocks() {
        let mut d = FakeSink { fail_at: None };
        let toc = read_toc(&mut d).unwrap();
        let track = *toc.track(1).unwrap();
        let mut lector = TrackReader::new(&mut d, &track);
        let mut bytes = 0;
        while let Some(block) = lector.next_block() {
            bytes += block.len();
        }
        assert_eq!(bytes, 150 * SECTOR_BYTES, "all 150 sectors came out");
        assert!(lector.error.is_none());
    }

    #[test]
    fn an_unreadable_sector_cuts_and_is_reported_instead_of_playing_silence() {
        let mut d = FakeSink { fail_at: Some(75) };
        let toc = read_toc(&mut d).unwrap();
        let track = *toc.track(1).unwrap();
        let mut lector = TrackReader::new(&mut d, &track);
        let primero = lector.next_block().expect("the first block does come out");
        assert_eq!(primero.len(), 75 * SECTOR_BYTES);
        assert!(lector.next_block().is_none());
        assert!(matches!(lector.error, Some(CdError::Read { lsn: 75, .. })));
    }
}
