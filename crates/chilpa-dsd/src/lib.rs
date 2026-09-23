//! DSD: leer 1 bit por muestra y entregarlo sin tocarlo.
//!
//! DSD no se "decodifica": los bits que trae el archivo son los que tiene que
//! recibir el DAC. Este crate abre los contenedores (DSF hoy, DFF y SACD
//! después), normaliza el orden de bits dentro del byte y entrega bloques
//! planares por canal, listos para el empaquetado DoP de `chilpa-audio` o
//! para una salida nativa.
//!
//! Lo único que se toca es el orden de los bits dentro del byte, porque DSF
//! los guarda con el primero en tiempo en la posición menos significativa y
//! el cable los quiere al revés. Esa inversión es exacta y reversible.

pub mod dsf;

pub use dsf::DsfReader;

/// Errores al leer DSD.
#[derive(Debug)]
pub enum DsdError {
    Open(std::io::Error),
    /// El archivo no es lo que dice ser.
    Corrupt(String),
    /// Una frecuencia o un número de canales que no manejamos.
    Unsupported(String),
}

impl std::fmt::Display for DsdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DsdError::Open(e) => write!(f, "no se pudo abrir: {e}"),
            DsdError::Corrupt(s) => write!(f, "archivo de DSD dañado: {s}"),
            DsdError::Unsupported(s) => write!(f, "DSD no soportado: {s}"),
        }
    }
}

impl From<std::io::Error> for DsdError {
    fn from(e: std::io::Error) -> Self {
        DsdError::Open(e)
    }
}

/// Lo que se sabe de un stream de DSD antes de leerlo.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DsdInfo {
    /// Bits de DSD por segundo y por canal: 2 822 400 es DSD64.
    pub dsd_rate_hz: u32,
    pub channels: u16,
    /// Total de bits por canal, o sea de muestras.
    pub samples: u64,
}

impl DsdInfo {
    /// El múltiplo de 44 100 que nombra la tasa: 64 para DSD64.
    pub fn multiple(&self) -> u32 {
        self.dsd_rate_hz / 44_100
    }

    /// Duración en segundos.
    pub fn seconds(&self) -> f64 {
        if self.dsd_rate_hz == 0 {
            return 0.0;
        }
        self.samples as f64 / f64::from(self.dsd_rate_hz)
    }
}

/// Una fuente de DSD abierta.
pub trait DsdSource {
    fn info(&self) -> &DsdInfo;

    /// Siguiente bloque: un vector de bytes por canal, todos del mismo largo,
    /// con el bit primero en tiempo en la posición más significativa.
    /// `None` cuando se acabó.
    fn next_block(&mut self) -> Result<Option<Vec<Vec<u8>>>, DsdError>;
}

/// Invierte el orden de los bits de un byte.
///
/// DSF guarda el bit primero en tiempo en la posición menos significativa;
/// DoP y las salidas nativas lo quieren en la más significativa.
pub const fn reverse_bits(b: u8) -> u8 {
    b.reverse_bits()
}

/// Frecuencias de DSD que aceptamos, de DSD64 a DSD512.
pub fn is_supported_rate(rate_hz: u32) -> bool {
    matches!(rate_hz, 2_822_400 | 5_644_800 | 11_289_600 | 22_579_200)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn las_tasas_de_dsd_son_multiplos_de_44_100() {
        assert!(is_supported_rate(2_822_400));
        assert!(is_supported_rate(11_289_600));
        assert!(!is_supported_rate(44_100), "eso es PCM");
        assert!(!is_supported_rate(3_000_000));
    }

    #[test]
    fn el_multiplo_nombra_la_tasa() {
        let info = DsdInfo { dsd_rate_hz: 2_822_400, channels: 2, samples: 2_822_400 };
        assert_eq!(info.multiple(), 64);
        assert_eq!(info.seconds(), 1.0);
    }

    #[test]
    fn invertir_los_bits_es_reversible() {
        for b in [0u8, 1, 0x80, 0xAB, 0xFF] {
            assert_eq!(reverse_bits(reverse_bits(b)), b);
        }
        assert_eq!(reverse_bits(0b0000_0001), 0b1000_0000);
    }
}
