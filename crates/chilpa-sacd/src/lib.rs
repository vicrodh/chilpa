//! Imágenes de SACD: leer el disco y entregar sus pistas como DSD.
//!
//! Una imagen de SACD es un archivo de sectores de 2048 bytes con dos tablas
//! de contenido (la maestra y la del área estéreo) y un área de audio donde
//! los bits de DSD viajan empaquetados por sector.
//!
//! Este crate solo lee: nunca escribe en la imagen ni convierte la señal. Lo
//! que entrega son los mismos bits que trae el disco, para que la ruta de DSD
//! que ya existe (DoP o salida nativa) los mande al DAC sin tocarlos.

pub mod frames;
pub mod sector;
pub mod toc;

pub use frames::SacdTrackReader;
pub use toc::{open, SacdArea, SacdTrack};

/// Un sector de SACD.
pub const SECTOR: usize = 2048;
/// Sectores por segundo de reproducción.
pub const SECTORS_PER_SEC: u32 = 350;
/// Un frame estéreo de DSD64 sin comprimir: 1/75 de segundo.
pub const STEREO_FRAME: usize = 9408;
/// La única tasa que usa el área estéreo del Scarlet Book.
pub const DSD64_HZ: u32 = 2_822_400;
/// El tipo de dato que marca audio en un paquete.
pub const DATA_TYPE_AUDIO: u16 = 2;

/// Cómo vienen guardados los bits del área.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Encoding {
    /// DSD sin comprimir, 3 bytes en 14.
    Dsd3In14,
    /// DSD sin comprimir, 3 bytes en 16.
    Dsd3In16,
    /// Comprimido con DST. Todavía no se puede reproducir.
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
    /// La imagen no tiene la forma de un SACD.
    MalformedToc(String),
    /// Un sector que no cuadra con lo que dice su encabezado.
    BadSector { lsn: u64, why: String },
    /// Algo que el disco trae pero todavía no manejamos.
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

/// Un código de tiempo del disco (minutos, segundos, frames de 1/75 s) en
/// frames.
pub(crate) fn timecode_frames(b: &[u8], at: usize) -> u32 {
    (u32::from(b[at]) * 60 + u32::from(b[at + 1])) * 75 + u32::from(b[at + 2])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stereo_frame_lasts_one_seventy_fifth_of_a_second() {
        // 2 822 400 bits por canal y por segundo, 8 bits por byte, 2 canales,
        // 75 frames por segundo.
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
