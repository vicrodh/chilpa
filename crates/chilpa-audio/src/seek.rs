//! Buscar dentro de una pista, contando frames.
//!
//! En QBZ la posición se llevaba con el reloj del sistema en segundos
//! enteros. Eso rompía dos cosas: al reanudar una pausa se volvía al segundo
//! truncado, y el cambio de pista lo disparaban dos detectores distintos.
//! Aquí la posición siempre es un número de frames y el tiempo solo se usa
//! para hablar con la UI.

use std::time::Duration;

/// Convierte tiempo a frames redondeando al más cercano, no truncando.
pub fn frames_at(position: Duration, sample_rate_hz: u32) -> u64 {
    if sample_rate_hz == 0 {
        return 0;
    }
    let nanos = position.as_nanos();
    let rate = u128::from(sample_rate_hz);
    ((nanos * rate + 500_000_000) / 1_000_000_000) as u64
}

/// El tiempo que representa un frame, para mostrarlo.
pub fn position_of(frames: u64, sample_rate_hz: u32) -> Duration {
    if sample_rate_hz == 0 {
        return Duration::ZERO;
    }
    Duration::from_nanos(frames * 1_000_000_000 / u64::from(sample_rate_hz))
}

/// Lo que hay que hacer para quedar exactamente en el frame pedido.
///
/// Un contenedor no siempre puede colocarse en cualquier frame: lo normal es
/// que caiga en el arranque de un paquete, antes del objetivo. La diferencia
/// se decodifica y se tira, que es lo que hace que la búsqueda sea exacta.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SeekPlan {
    /// Frame al que se le pide al contenedor que se mueva.
    pub request: u64,
    /// Frames que hay que decodificar y tirar después.
    pub discard: u64,
}

impl SeekPlan {
    /// `landed` es dónde quedó el contenedor de verdad.
    pub fn after_landing(target: u64, landed: u64) -> Self {
        Self { request: landed, discard: target.saturating_sub(landed) }
    }

    pub fn is_exact(&self) -> bool {
        self.discard == 0
    }
}

/// Dónde va la reproducción, en frames.
///
/// Pausar no mueve nada: al reanudar se sigue en el mismo frame, no en el
/// segundo redondeado.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Cursor {
    pub frame: u64,
    pub sample_rate_hz: u32,
}

impl Cursor {
    pub fn new(sample_rate_hz: u32) -> Self {
        Self { frame: 0, sample_rate_hz }
    }

    pub fn advance(&mut self, frames: u64) {
        self.frame += frames;
    }

    pub fn seek_to(&mut self, position: Duration) -> u64 {
        self.frame = frames_at(position, self.sample_rate_hz);
        self.frame
    }

    pub fn position(&self) -> Duration {
        position_of(self.frame, self.sample_rate_hz)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_tiempo_se_vuelve_frames_sin_truncar_a_segundos() {
        assert_eq!(frames_at(Duration::from_millis(1500), 44_100), 66_150);
        assert_eq!(frames_at(Duration::from_millis(1), 44_100), 44);
        assert_eq!(frames_at(Duration::ZERO, 44_100), 0);
    }

    #[test]
    fn ida_y_vuelta_de_una_posicion_no_pierde_el_frame() {
        let c = Cursor { frame: 123_456, sample_rate_hz: 96_000 };
        assert_eq!(frames_at(c.position(), 96_000), 123_456);
    }

    #[test]
    fn pausar_y_reanudar_no_regresa_al_segundo_redondeado() {
        // El hallazgo de QBZ: al reanudar se re-buscaba el segundo truncado
        // y se repetía casi medio segundo de música.
        let mut c = Cursor::new(44_100);
        c.advance(66_150); // 1.5 s
        let al_pausar = c.frame;
        // Reanudar no toca el cursor.
        assert_eq!(c.frame, al_pausar);
        assert_eq!(c.position(), Duration::from_millis(1500));
    }

    #[test]
    fn lo_que_el_contenedor_no_alcanza_se_tira_decodificando() {
        let plan = SeekPlan::after_landing(100_000, 98_304);
        assert_eq!(plan.request, 98_304);
        assert_eq!(plan.discard, 1_696);
        assert!(!plan.is_exact());
    }

    #[test]
    fn si_el_contenedor_cae_justo_no_se_tira_nada() {
        let plan = SeekPlan::after_landing(44_100, 44_100);
        assert!(plan.is_exact());
    }

    #[test]
    fn un_aterrizaje_pasado_no_deja_un_descarte_negativo() {
        let plan = SeekPlan::after_landing(1_000, 2_000);
        assert_eq!(plan.discard, 0, "nunca se descarta hacia atrás");
    }
}
