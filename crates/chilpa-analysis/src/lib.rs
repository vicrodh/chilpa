//! Lo que se puede saber de una pista escuchándola.
//!
//! Hoy: el tempo. Lo que venga (tonalidad, energía, sonoridad) se suma aquí
//! y sale por la misma puerta, porque la biblioteca guarda las features con
//! la versión del analizador y puede recalcular sin revolver lo viejo.
//!
//! Nada de esto toca la ruta de reproducción. El análisis lee archivos y
//! escribe números; el audio que suena no se entera.

pub mod tempo;

pub use tempo::{detect as detect_tempo, Tempo};

/// La versión de este analizador.
///
/// Sube cuando el resultado cambia para la misma entrada. La biblioteca
/// guarda este número junto a cada feature, así que subirlo es lo que
/// permite recalcular sin borrar lo anterior.
pub const VERSION: i64 = 1;

/// Cómo se llama este analizador en la base.
pub const NOMBRE: &str = "chilpa";

/// Lo que el análisis saca de una pista.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Features {
    pub bpm: Option<f32>,
    /// De 0 a 1: qué tan marcado está el pulso.
    pub tempo_confidence: Option<f32>,
}

/// Analiza una señal mono ya decodificada.
pub fn analyze(samples: &[f32], sample_rate_hz: u32) -> Features {
    match tempo::detect(samples, sample_rate_hz) {
        Some(t) => Features { bpm: Some(t.bpm), tempo_confidence: Some(t.confidence) },
        None => Features::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sin_pulso_no_se_reporta_bpm() {
        let f = analyze(&vec![0.0; 44_100 * 10], 44_100);
        assert_eq!(f, Features::default(), "ceros, no un número inventado");
        assert!(f.bpm.is_none());
    }
}
