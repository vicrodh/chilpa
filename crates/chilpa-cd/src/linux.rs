//! The disc drive on Linux, through ioctl.
//!
//! Three calls are enough to play: the TOC header, each entry, and the audio
//! read. The kernel hands the samples over little endian, which is exactly
//! what the output device wants, so the bytes pass through without being
//! swapped.

use crate::{CdDevice, CdError, TocEntry, MAX_SECTORS_PER_READ, SECTOR_BYTES};
use std::fs::File;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

const CDROMREADTOCHDR: libc::c_ulong = 0x5305;
const CDROMREADTOCENTRY: libc::c_ulong = 0x5306;
const CDROMREADAUDIO: libc::c_ulong = 0x530e;
const CDROM_DRIVE_STATUS: libc::c_ulong = 0x5326;
const CDROM_LBA: u8 = 0x01;
/// The track number you ask with to get the lead-out.
const CDROM_LEADOUT: u8 = 0xAA;
const CDS_DISC_OK: libc::c_int = 4;
/// The control bit that marks a data track.
const CTRL_DATA: u8 = 0x04;

#[repr(C)]
#[derive(Default)]
struct TocHdr {
    first: u8,
    last: u8,
}

#[repr(C)]
#[derive(Default)]
struct TocEntryRaw {
    track: u8,
    adr_ctrl: u8,
    format: u8,
    addr_lba: i32,
    datamode: u8,
}

#[repr(C)]
struct ReadAudio {
    addr_lba: i32,
    addr_format: u8,
    _pad: [u8; 3],
    nframes: libc::c_int,
    buf: *mut u8,
}

/// An open drive.
pub struct LinuxDrive {
    file: File,
    path: PathBuf,
}

impl LinuxDrive {
    /// Opens the drive and checks it has a disc ready.
    pub fn open(path: &Path) -> Result<Self, CdError> {
        let file = File::open(path).map_err(|e| CdError::Device(e.to_string()))?;
        let state = unsafe { libc::ioctl(file.as_raw_fd(), CDROM_DRIVE_STATUS, 0) };
        if state != CDS_DISC_OK {
            return Err(CdError::NoDisc);
        }
        Ok(Self { file, path: path.to_path_buf() })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn toc_entry(&self, track: u8) -> Result<(u32, bool), CdError> {
        let mut e = TocEntryRaw { track, format: CDROM_LBA, ..Default::default() };
        if unsafe { libc::ioctl(self.file.as_raw_fd(), CDROMREADTOCENTRY, &mut e) } < 0 {
            return Err(CdError::Device(std::io::Error::last_os_error().to_string()));
        }
        Ok((e.addr_lba.max(0) as u32, e.adr_ctrl & CTRL_DATA == 0))
    }
}

impl CdDevice for LinuxDrive {
    fn read_toc(&mut self) -> Result<(Vec<TocEntry>, u32), CdError> {
        let mut hdr = TocHdr::default();
        if unsafe { libc::ioctl(self.file.as_raw_fd(), CDROMREADTOCHDR, &mut hdr) } < 0 {
            return Err(CdError::Device(std::io::Error::last_os_error().to_string()));
        }
        let (leadout, _) = self.toc_entry(CDROM_LEADOUT)?;

        let mut entries = Vec::new();
        let mut session = 1u8;
        let mut previous_is_audio = None;
        for n in hdr.first..=hdr.last {
            let (lsn, is_audio) = self.toc_entry(n)?;
            // The TOC ioctl does not say which session. On a CD-Extra the data
            // track lives in the second one, and is recognized because it comes
            // after the audio: the only case where the gap must be subtracted.
            if previous_is_audio == Some(true) && !is_audio {
                session = 2;
            }
            previous_is_audio = Some(is_audio);
            entries.push(TocEntry { number: n, start_lsn: lsn, is_audio: is_audio, session: session });
        }
        Ok((entries, leadout))
    }

    fn read_audio(&mut self, lsn: u32, sectors: u32, out: &mut Vec<u8>) -> Result<(), CdError> {
        let sectors = sectors.clamp(1, MAX_SECTORS_PER_READ);
        out.clear();
        out.resize(sectors as usize * SECTOR_BYTES, 0);
        let mut request = ReadAudio {
            addr_lba: lsn as i32,
            addr_format: CDROM_LBA,
            _pad: [0; 3],
            nframes: sectors as libc::c_int,
            buf: out.as_mut_ptr(),
        };
        if unsafe { libc::ioctl(self.file.as_raw_fd(), CDROMREADAUDIO, &mut request) } < 0 {
            return Err(CdError::Read {
                lsn,
                why: std::io::Error::last_os_error().to_string(),
            });
        }
        Ok(())
    }
}

/// A heuristic for diagnosing drives that hand the samples over the wrong
/// way round. It does not apply itself: if swapping the bytes is ever
/// needed, that is decided by a measurement, not by a guess.
///
/// The idea: in real music two consecutive samples are alike. If the average
/// difference reading big endian is much smaller than little endian, the
/// bytes arrive swapped.
pub fn looks_byte_swapped(pcm: &[u8]) -> bool {
    if pcm.len() < 8 {
        return false;
    }
    let mut difference_le: u64 = 0;
    let mut difference_be: u64 = 0;
    let mut previous_le = 0i32;
    let mut previous_be = 0i32;
    for (i, pair) in pcm.chunks_exact(2).enumerate() {
        let le = i16::from_le_bytes([pair[0], pair[1]]) as i32;
        let be = i16::from_be_bytes([pair[0], pair[1]]) as i32;
        if i > 0 {
            difference_le += (le - previous_le).unsigned_abs() as u64;
            difference_be += (be - previous_be).unsigned_abs() as u64;
        }
        previous_le = le;
        previous_be = be;
    }
    difference_be * 4 < difference_le
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_little_endian_ramp_does_not_look_byte_swapped() {
        let mut pcm = Vec::new();
        for i in 0..2000i16 {
            pcm.extend_from_slice(&i.to_le_bytes());
        }
        assert!(!looks_byte_swapped(&pcm));
    }

    #[test]
    fn the_same_ramp_written_backwards_does_show() {
        let mut pcm = Vec::new();
        for i in 0..2000i16 {
            pcm.extend_from_slice(&i.to_be_bytes());
        }
        assert!(looks_byte_swapped(&pcm));
    }
}
