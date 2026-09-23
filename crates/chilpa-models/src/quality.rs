//! Cómo suena algo, en términos que no dependen de ningún servicio.

/// Formato, profundidad de bits, frecuencia y canales.
///
/// En QBZ esto era un `hires: bool` más campos sueltos, y la UI etiquetaba
/// como bit-perfect salidas que no lo eran. Aquí no hay etiquetas: hay
/// números (ADR-004).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AudioQuality {
    /// `flac`, `alac`, `opus`, `dsd`, `pcm`…
    pub codec: String,
    /// `None` para formatos con pérdida, donde no significa nada.
    pub bit_depth: Option<u8>,
    pub sample_rate_hz: u32,
    pub channels: u8,
}

impl AudioQuality {
    /// Cierto si conserva la señal original. Un formato con pérdida nunca lo
    /// es, y por eso tampoco declara profundidad de bits.
    pub fn is_lossless(&self) -> bool {
        self.bit_depth.is_some()
    }

    /// Bits por segundo del PCM equivalente. Sirve para comparar dos
    /// versiones del mismo track; no es el bitrate del archivo.
    pub fn rank(&self) -> u64 {
        u64::from(self.bit_depth.unwrap_or(0))
            * u64::from(self.sample_rate_hz)
            * u64::from(self.channels)
    }

    /// Cierto si las dos pueden salir por el mismo dispositivo sin
    /// reconfigurarlo. Cambiar de frecuencia o de formato entre pistas obliga
    /// a renegociar la salida, que es donde QBZ metía clicks y resampling
    /// silencioso.
    pub fn same_stream_format(&self, other: &Self) -> bool {
        self.sample_rate_hz == other.sample_rate_hz
            && self.bit_depth == other.bit_depth
            && self.channels == other.channels
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(codec: &str, bits: Option<u8>, hz: u32) -> AudioQuality {
        AudioQuality { codec: codec.into(), bit_depth: bits, sample_rate_hz: hz, channels: 2 }
    }

    #[test]
    fn lo_que_tiene_perdida_no_es_lossless() {
        assert!(q("flac", Some(24), 96_000).is_lossless());
        assert!(!q("opus", None, 48_000).is_lossless());
    }

    #[test]
    fn hi_res_rankea_arriba_de_cd() {
        assert!(q("flac", Some(24), 96_000).rank() > q("flac", Some(16), 44_100).rank());
    }

    #[test]
    fn lo_que_tiene_perdida_no_le_gana_a_nada_lossless() {
        assert!(q("opus", None, 48_000).rank() < q("flac", Some(16), 44_100).rank());
    }

    #[test]
    fn el_mismo_formato_no_renegocia_la_salida() {
        assert!(q("flac", Some(16), 44_100).same_stream_format(&q("alac", Some(16), 44_100)));
        assert!(!q("flac", Some(16), 44_100).same_stream_format(&q("flac", Some(16), 48_000)));
    }
}
