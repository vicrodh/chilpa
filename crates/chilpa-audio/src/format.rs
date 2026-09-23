//! Cómo se ve un stream en el cable.

/// Lo que el dispositivo acepta en cada muestra.
///
/// No hay un formato "hi-res" ni banderas: hay un formato de muestra, una
/// frecuencia y un número de canales.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum SampleFormat {
    /// Entero de 16 bits, little endian.
    S16,
    /// Entero de 24 bits en 3 bytes.
    S24Packed,
    /// Entero de 24 bits dentro de 32, justificado a la izquierda: los 24
    /// bits válidos van en los bytes altos y el byte bajo queda en cero.
    S24In32,
    /// Entero de 32 bits.
    S32,
    /// Flotante de 32 bits. El dispositivo convierte, así que lo que llega al
    /// DAC ya no es lo que produjimos.
    F32,
}

impl SampleFormat {
    /// Bytes que ocupa una muestra en el cable.
    pub const fn bytes(self) -> usize {
        match self {
            SampleFormat::S16 => 2,
            SampleFormat::S24Packed => 3,
            SampleFormat::S24In32 | SampleFormat::S32 | SampleFormat::F32 => 4,
        }
    }

    /// Bits con información, no bits en el cable. `S24In32` lleva 24.
    pub const fn valid_bits(self) -> u8 {
        match self {
            SampleFormat::S16 => 16,
            SampleFormat::S24Packed | SampleFormat::S24In32 => 24,
            SampleFormat::S32 => 32,
            SampleFormat::F32 => 32,
        }
    }

    /// Cierto si el dispositivo recibe enteros tal cual. Un stream de
    /// flotantes nunca puede ser bit-perfect, porque alguien convierte.
    pub const fn is_integer(self) -> bool {
        !matches!(self, SampleFormat::F32)
    }
}

/// Un stream concreto: formato, frecuencia y canales.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct StreamFormat {
    pub sample_format: SampleFormat,
    pub sample_rate_hz: u32,
    pub channels: u16,
}

impl StreamFormat {
    pub const fn new(sample_format: SampleFormat, sample_rate_hz: u32, channels: u16) -> Self {
        Self { sample_format, sample_rate_hz, channels }
    }

    pub const fn frame_bytes(&self) -> usize {
        self.sample_format.bytes() * self.channels as usize
    }

    /// Cierto si los dos pueden sonar sin reconfigurar el dispositivo.
    /// Cambiar de frecuencia o de formato entre pistas obliga a renegociar,
    /// y ahí es donde QBZ metía clicks.
    pub fn same_stream(&self, other: &Self) -> bool {
        self == other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_formato_sabe_cuantos_bytes_y_bits_utiles_lleva() {
        assert_eq!(SampleFormat::S24Packed.bytes(), 3);
        assert_eq!(SampleFormat::S24In32.bytes(), 4);
        assert_eq!(SampleFormat::S24In32.valid_bits(), 24, "van 24 bits en 32");
    }

    #[test]
    fn los_flotantes_no_son_enteros() {
        assert!(SampleFormat::S32.is_integer());
        assert!(!SampleFormat::F32.is_integer());
    }

    #[test]
    fn un_frame_estereo_de_24_en_32_mide_ocho_bytes() {
        assert_eq!(StreamFormat::new(SampleFormat::S24In32, 96_000, 2).frame_bytes(), 8);
    }
}
