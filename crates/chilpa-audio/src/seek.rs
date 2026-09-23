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

/// Un lugar donde pedir un salto desde otro hilo.
///
/// Igual que la señal de paro: quien reproduce la mira entre bloques y la
/// consume. Pedir un salto nunca bloquea a quien lo pide ni toma el candado
/// del dispositivo.
#[derive(Clone, Debug, Default)]
pub struct SeekSignal(std::sync::Arc<std::sync::Mutex<Option<Duration>>>);

impl SeekSignal {
    pub fn new() -> Self {
        Self::default()
    }

    /// Pide saltar a ese punto. Un salto nuevo reemplaza al que no se haya
    /// atendido: lo que importa es el último que pidió el usuario.
    pub fn request(&self, position: Duration) {
        if let Ok(mut guard) = self.0.lock() {
            *guard = Some(position);
        }
    }

    /// Se lleva el salto pendiente, si hay.
    pub fn take(&self) -> Option<Duration> {
        self.0.lock().ok().and_then(|mut g| g.take())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_becomes_frames_without_truncating_to_seconds() {
        assert_eq!(frames_at(Duration::from_millis(1500), 44_100), 66_150);
        assert_eq!(frames_at(Duration::from_millis(1), 44_100), 44);
        assert_eq!(frames_at(Duration::ZERO, 44_100), 0);
    }

    #[test]
    fn a_position_round_trip_keeps_the_frame() {
        let c = Cursor { frame: 123_456, sample_rate_hz: 96_000 };
        assert_eq!(frames_at(c.position(), 96_000), 123_456);
    }

    #[test]
    fn pause_and_resume_does_not_jump_back_to_a_rounded_second() {
        // El hallazgo de QBZ: al reanudar se re-buscaba el segundo truncado
        // y se repetía casi medio segundo de música.
        let mut c = Cursor::new(44_100);
        c.advance(66_150); // 1.5 s
        let on_pause = c.frame;
        // Reanudar no toca el cursor.
        assert_eq!(c.frame, on_pause);
        assert_eq!(c.position(), Duration::from_millis(1500));
    }

    #[test]
    fn the_last_seek_asked_for_is_the_one_that_counts() {
        let s = SeekSignal::new();
        assert_eq!(s.take(), None);
        s.request(Duration::from_secs(10));
        s.request(Duration::from_secs(30));
        assert_eq!(s.take(), Some(Duration::from_secs(30)), "gana el más reciente");
        assert_eq!(s.take(), None, "se consume una sola vez");
    }

    #[test]
    fn what_the_container_cannot_reach_is_dropped_while_decoding() {
        let plan = SeekPlan::after_landing(100_000, 98_304);
        assert_eq!(plan.request, 98_304);
        assert_eq!(plan.discard, 1_696);
        assert!(!plan.is_exact());
    }

    #[test]
    fn if_the_container_lands_exactly_nothing_is_dropped() {
        let plan = SeekPlan::after_landing(44_100, 44_100);
        assert!(plan.is_exact());
    }

    #[test]
    fn overshooting_never_leaves_a_negative_discard() {
        let plan = SeekPlan::after_landing(1_000, 2_000);
        assert_eq!(plan.discard, 0, "nunca se descarta hacia atrás");
    }
}
