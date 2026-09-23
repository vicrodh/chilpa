//! Reproducir un CD de audio leyendo sus sectores.
//!
//! Un CD-DA no se decodifica: cada sector trae 2352 bytes que YA son PCM de
//! 16 bits, estéreo, a 44 100 Hz, en little endian. Lo único que hay que
//! hacer es leerlos y entregarlos, y eso es lo que hace bit-perfect a la
//! reproducción de un disco.
//!
//! La unidad vive detrás de [`CdDevice`], así que la lógica se prueba con un
//! dispositivo de mentiras y el acceso real queda en un solo lugar.

pub mod toc;

#[cfg(target_os = "linux")]
pub mod linux;

pub use toc::{build, Toc, TocEntry, Track, SECTOR_BYTES, SECTORS_PER_SEC};

use chilpa_audio::format::{SampleFormat, StreamFormat};

/// El formato de todo CD de audio.
pub const CDDA_FORMAT: StreamFormat = StreamFormat::new(SampleFormat::S16, 44_100, 2);
/// Frames estéreo en un sector.
pub const FRAMES_PER_SECTOR: usize = SECTOR_BYTES / 4;
/// Lo máximo que acepta el kernel por lectura.
pub const MAX_SECTORS_PER_READ: u32 = 75;

#[derive(Debug)]
pub enum CdError {
    /// No se pudo hablar con la unidad.
    Device(String),
    /// La unidad está vacía o todavía no reconoce el disco.
    NoDisc,
    /// El disco no trae pistas de audio.
    NotAudio,
    /// Un sector ilegible, con su posición.
    Read { lsn: u32, why: String },
}

impl std::fmt::Display for CdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CdError::Device(s) => write!(f, "la unidad no responde: {s}"),
            CdError::NoDisc => write!(f, "no hay disco en la unidad"),
            CdError::NotAudio => write!(f, "el disco no trae pistas de audio"),
            CdError::Read { lsn, why } => write!(f, "no se pudo leer el sector {lsn}: {why}"),
        }
    }
}

/// Una unidad de disco.
pub trait CdDevice {
    /// Lee el TOC crudo: las entradas y el lead-out.
    fn read_toc(&mut self) -> Result<(Vec<TocEntry>, u32), CdError>;

    /// Lee `sectors` sectores de audio desde `lsn` y los deja en `out`.
    fn read_audio(&mut self, lsn: u32, sectors: u32, out: &mut Vec<u8>) -> Result<(), CdError>;
}

/// Lee el TOC de una unidad y le calcula las duraciones.
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

/// Entrega una pista de CD como bloques de PCM, listos para el dispositivo de
/// salida.
pub struct TrackReader<'a> {
    device: &'a mut dyn CdDevice,
    lsn: u32,
    end_lsn: u32,
    /// Sectores por lectura. Más sectores es menos ida y vuelta con la
    /// unidad; el kernel no acepta más de 75.
    chunk: u32,
    /// El primer sector ilegible, si lo hubo.
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

    /// Cuántos sectores faltan.
    pub fn remaining(&self) -> u32 {
        self.end_lsn.saturating_sub(self.lsn)
    }
}

impl chilpa_audio::render::FrameSource for TrackReader<'_> {
    fn next_block(&mut self) -> Option<Vec<u8>> {
        if self.lsn >= self.end_lsn || self.error.is_some() {
            return None;
        }
        let sectores = self.chunk.min(self.remaining());
        let mut buf = Vec::new();
        match self.device.read_audio(self.lsn, sectores, &mut buf) {
            Ok(()) => {
                self.lsn += sectores;
                Some(buf)
            }
            Err(e) => {
                // Un sector ilegible no se rellena con silencio a escondidas:
                // se corta y se reporta.
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

    struct Falso {
        fallar_en: Option<u32>,
    }

    impl CdDevice for Falso {
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
            if self.fallar_en == Some(lsn) {
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
    fn el_formato_de_un_cd_es_siempre_el_mismo() {
        assert_eq!(CDDA_FORMAT.sample_rate_hz, 44_100);
        assert_eq!(CDDA_FORMAT.sample_format, SampleFormat::S16);
        assert_eq!(FRAMES_PER_SECTOR, 588);
    }

    #[test]
    fn se_lee_el_toc_con_sus_duraciones() {
        let mut d = Falso { fallar_en: None };
        let toc = read_toc(&mut d).unwrap();
        assert_eq!(toc.tracks.len(), 2);
        assert_eq!(toc.tracks[0].sectors, 150);
        assert_eq!(toc.tracks[1].sectors, 150);
    }

    #[test]
    fn una_pista_se_entrega_completa_y_en_bloques() {
        let mut d = Falso { fallar_en: None };
        let toc = read_toc(&mut d).unwrap();
        let pista = *toc.track(1).unwrap();
        let mut lector = TrackReader::new(&mut d, &pista);
        let mut bytes = 0;
        while let Some(bloque) = lector.next_block() {
            bytes += bloque.len();
        }
        assert_eq!(bytes, 150 * SECTOR_BYTES, "salieron los 150 sectores");
        assert!(lector.error.is_none());
    }

    #[test]
    fn un_sector_ilegible_corta_y_se_reporta_en_vez_de_sonar_a_silencio() {
        let mut d = Falso { fallar_en: Some(75) };
        let toc = read_toc(&mut d).unwrap();
        let pista = *toc.track(1).unwrap();
        let mut lector = TrackReader::new(&mut d, &pista);
        let primero = lector.next_block().expect("el primer bloque sí sale");
        assert_eq!(primero.len(), 75 * SECTOR_BYTES);
        assert!(lector.next_block().is_none());
        assert!(matches!(lector.error, Some(CdError::Read { lsn: 75, .. })));
    }
}
