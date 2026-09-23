//! Lo que se puede saber de una pista escuchándola.
//!
//! Hoy: el tempo. Lo que venga (tonalidad, energía, sonoridad) se suma aquí
//! y sale por la misma puerta, porque la biblioteca guarda las features con
//! la versión del analizador y puede recalcular sin revolver lo viejo.
//!
//! Nada de esto toca la ruta de reproducción. El análisis lee archivos y
//! escribe números; el audio que suena no se entera.

pub mod key;
pub mod loudness;
pub mod tempo;

pub use key::{detect as detect_key, Key};
pub use loudness::Meter as LoudnessMeter;
pub use tempo::{detect as detect_tempo, Tempo};

/// La versión de este analizador.
///
/// Sube cuando el resultado cambia para la misma entrada, y también cuando
/// se saca algo nuevo: una fila de la versión 1 no trae tonalidad ni
/// sonoridad, y dejarla como buena sería tener media biblioteca coja. La
/// biblioteca guarda este número junto a cada feature, así que subirlo es lo
/// que permite recalcular sin borrar lo anterior.
///
/// - 1: tempo.
/// - 2: tempo, tonalidad y sonoridad.
pub const VERSION: i64 = 2;

/// Cómo se llama este analizador en la base.
pub const NOMBRE: &str = "chilpa";

/// Lo que el análisis saca de una pista.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Analysis {
    pub bpm: Option<f32>,
    /// De 0 a 1: qué tan marcado está el pulso.
    pub tempo_confidence: Option<f32>,
    /// La tonalidad, cuando se pudo decidir.
    pub key: Option<Key>,
    /// Qué tan fuerte suena, en LUFS. Se mide, no se corrige.
    pub loudness_lufs: Option<f32>,
}

/// Confianza mínima para afirmar una tonalidad.
///
/// Por debajo de esto el cromagrama encaja casi igual de bien en dos tonos,
/// y decir uno sería inventar.
pub const CONFIANZA_MINIMA_TONALIDAD: f32 = 0.05;

/// Analiza una señal mono ya decodificada.
pub fn analyze(samples: &[f32], sample_rate_hz: u32) -> Analysis {
    let t = tempo::detect(samples, sample_rate_hz);
    // La tonalidad se busca sobre la señal remuestreada, que es donde los
    // bins caen del tamaño que le conviene al cromagrama.
    let bajada = tempo::remuestrear_para_analisis(samples, sample_rate_hz);
    let k = key::detect(&bajada, tempo::HZ_ANALISIS)
        .filter(|k| k.confidence >= CONFIANZA_MINIMA_TONALIDAD);
    Analysis {
        bpm: t.map(|t| t.bpm),
        tempo_confidence: t.map(|t| t.confidence),
        key: k,
        // La sonoridad se mide con la señal como viene, no con la que se
        // remuestreó para contar golpes; quien llama la pone.
        loudness_lufs: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sin_pulso_ni_tono_no_se_reporta_nada() {
        let f = analyze(&vec![0.0; 44_100 * 10], 44_100);
        assert_eq!(f, Analysis::default(), "ceros, no números inventados");
        assert!(f.bpm.is_none() && f.key.is_none());
    }
}
