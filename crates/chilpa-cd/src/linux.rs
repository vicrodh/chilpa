//! La unidad de disco en Linux, por ioctl.
//!
//! Tres llamadas bastan para reproducir: el encabezado del TOC, cada entrada
//! y la lectura de audio. El kernel entrega las muestras en little endian,
//! que es justo lo que quiere el dispositivo de salida, así que los bytes
//! pasan sin voltearse.

use crate::{CdDevice, CdError, TocEntry, MAX_SECTORS_PER_READ, SECTOR_BYTES};
use std::fs::File;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

const CDROMREADTOCHDR: libc::c_ulong = 0x5305;
const CDROMREADTOCENTRY: libc::c_ulong = 0x5306;
const CDROMREADAUDIO: libc::c_ulong = 0x530e;
const CDROM_DRIVE_STATUS: libc::c_ulong = 0x5326;
const CDROM_LBA: u8 = 0x01;
/// El número de pista con el que se pregunta por el lead-out.
const CDROM_LEADOUT: u8 = 0xAA;
const CDS_DISC_OK: libc::c_int = 4;
/// Bit de control que marca una pista de datos.
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

/// Una unidad abierta.
pub struct LinuxDrive {
    file: File,
    path: PathBuf,
}

impl LinuxDrive {
    /// Abre la unidad y comprueba que tenga un disco listo.
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
            // El ioctl del TOC no dice la sesión. En un CD-Extra la pista de
            // datos vive en la segunda, y se reconoce porque llega después
            // del audio: es el único caso donde hay que descontar el hueco.
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

/// Un heurístico para diagnosticar unidades que entregan las muestras al
/// revés. No se aplica solo: si alguna vez hace falta voltear los bytes, se
/// decide con una medición, no con una suposición.
///
/// La idea: en música real dos muestras seguidas se parecen. Si la diferencia
/// promedio leyendo en big endian es mucho menor que en little endian, los
/// bytes vienen volteados.
pub fn looks_byte_swapped(pcm: &[u8]) -> bool {
    if pcm.len() < 8 {
        return false;
    }
    let mut dif_le: u64 = 0;
    let mut dif_be: u64 = 0;
    let mut anterior_le = 0i32;
    let mut anterior_be = 0i32;
    for (i, par) in pcm.chunks_exact(2).enumerate() {
        let le = i16::from_le_bytes([par[0], par[1]]) as i32;
        let be = i16::from_be_bytes([par[0], par[1]]) as i32;
        if i > 0 {
            dif_le += (le - anterior_le).unsigned_abs() as u64;
            dif_be += (be - anterior_be).unsigned_abs() as u64;
        }
        anterior_le = le;
        anterior_be = be;
    }
    dif_be * 4 < dif_le
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
