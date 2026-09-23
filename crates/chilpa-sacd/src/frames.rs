//! De sectores a bloques de DSD por canal.
//!
//! Un frame estéreo sin comprimir mide 9408 bytes y trae los dos canales
//! **intercalados byte a byte**. Eso no se dedujo: se midió. En QBZ se probó
//! primero la otra hipótesis (un bloque contiguo por canal) y el resultado
//! dejaba 23 % de la energía arriba de la banda audible; con el intercalado
//! por byte quedó 99.8 % adentro. La hipótesis equivocada llegó a salir en
//! una versión, así que aquí queda escrito.

use crate::sector;
use crate::{Encoding, SacdError, SECTOR, STEREO_FRAME};
use chilpa_dsd::{DsdInfo, DsdSource};

use crate::toc::{SacdImage, SacdTrack};

/// Lee una pista del área de audio y la entrega como DSD por canal.
pub struct SacdTrackReader {
    image: SacdImage,
    /// Sectores que faltan por leer de la pista.
    lsn: u64,
    end_lsn: u64,
    /// Audio leído que todavía no completa un frame.
    pendiente: Vec<u8>,
    info: DsdInfo,
}

impl SacdTrackReader {
    /// Abre una pista. El DST todavía no se puede reproducir.
    pub fn open(image: SacdImage, track: &SacdTrack) -> Result<Self, SacdError> {
        if track.encoding.is_compressed() {
            return Err(SacdError::Unsupported(
                "la pista viene comprimida con DST y todavía no hay decodificador".into(),
            ));
        }
        let samples = u64::from(track.duration_frames) * u64::from(crate::DSD64_HZ / 75);
        Ok(Self {
            lsn: u64::from(track.start_lsn),
            end_lsn: u64::from(track.start_lsn) + u64::from(track.length_lsn),
            image,
            pendiente: Vec::with_capacity(STEREO_FRAME * 2),
            info: DsdInfo { dsd_rate_hz: crate::DSD64_HZ, channels: 2, samples },
        })
    }

    /// Lee sectores hasta juntar al menos un frame completo.
    fn llenar(&mut self) -> Result<(), SacdError> {
        let mut sector_buf = vec![0u8; SECTOR];
        while self.pendiente.len() < STEREO_FRAME && self.lsn < self.end_lsn {
            self.image.read_sector(self.lsn, &mut sector_buf)?;
            let paquetes = sector::parse(&sector_buf, self.lsn)?;
            if paquetes.dst {
                return Err(SacdError::Unsupported(
                    "el sector viene comprimido con DST".into(),
                ));
            }
            for p in paquetes.packets.iter().filter(|p| p.is_audio()) {
                self.pendiente.extend_from_slice(&sector_buf[p.at..p.at + p.len]);
            }
            self.lsn += 1;
        }
        Ok(())
    }
}

impl DsdSource for SacdTrackReader {
    fn info(&self) -> &DsdInfo {
        &self.info
    }

    fn next_block(&mut self) -> Result<Option<Vec<Vec<u8>>>, chilpa_dsd::DsdError> {
        self.llenar().map_err(|e| chilpa_dsd::DsdError::Corrupt(e.to_string()))?;
        if self.pendiente.len() < STEREO_FRAME {
            // Lo que sobra no completa un frame: es el final de la pista.
            self.pendiente.clear();
            return Ok(None);
        }
        let frame: Vec<u8> = self.pendiente.drain(..STEREO_FRAME).collect();
        Ok(Some(split_channels(&frame)))
    }
}

/// Parte un frame estéreo en sus dos canales. Los bytes vienen alternados:
/// el par va al izquierdo y el impar al derecho.
pub fn split_channels(frame: &[u8]) -> Vec<Vec<u8>> {
    let mitad = frame.len() / 2;
    let mut izquierdo = Vec::with_capacity(mitad);
    let mut derecho = Vec::with_capacity(mitad);
    for par in frame.chunks_exact(2) {
        izquierdo.push(par[0]);
        derecho.push(par[1]);
    }
    vec![izquierdo, derecho]
}

/// Qué codificaciones se pueden reproducir hoy.
pub fn is_playable(encoding: Encoding) -> bool {
    !encoding.is_compressed()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn los_canales_vienen_intercalados_byte_a_byte() {
        let frame: Vec<u8> = (0..8u8).collect();
        let canales = split_channels(&frame);
        assert_eq!(canales[0], vec![0, 2, 4, 6], "el izquierdo toma los pares");
        assert_eq!(canales[1], vec![1, 3, 5, 7], "el derecho toma los impares");
    }

    #[test]
    fn un_frame_se_parte_en_dos_mitades_iguales() {
        let canales = split_channels(&vec![0u8; STEREO_FRAME]);
        assert_eq!(canales[0].len(), STEREO_FRAME / 2);
        assert_eq!(canales[0].len(), canales[1].len());
    }

    #[test]
    fn dst_no_es_reproducible_todavia() {
        assert!(!is_playable(Encoding::Dst));
        assert!(is_playable(Encoding::Dsd3In14));
    }
}
