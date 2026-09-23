//! El dispositivo visto desde el motor.
//!
//! Dos cosas que la auditoría encontró rotas en QBZ y que aquí son parte del
//! tipo, no del cuidado de quien programa:
//!
//! 1. **Una escritura que falla no es un fin de pista.** En QBZ, si el DAC se
//!    desconectaba o se trababa, el fallo se veía igual que "se acabó la
//!    pista", y la cola entera se saltaba sin decir nada.
//! 2. **Parar nunca espera al dispositivo.** Si el endpoint dejaba de mandar
//!    eventos, el escritor se bloqueaba para siempre y `stop()` se quedaba en
//!    deadlock. Aquí toda espera tiene límite y parar solo levanta una
//!    bandera.

use crate::format::StreamFormat;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Lo que puede salir mal al escribirle al dispositivo. Ninguna de estas es
/// un final normal.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum SinkError {
    /// El dispositivo desapareció: desconectado, apagado o tomado por otro.
    DeviceLost,
    /// Se quedó sin datos y hubo que recuperarlo. Se puede seguir.
    Xrun,
    /// No respondió dentro del límite. Se trata como pérdida, no como
    /// paciencia infinita.
    Timeout(Duration),
    /// Ya estaba cerrado.
    Closed,
    /// Cualquier otra falla del backend, con su texto.
    Backend(String),
}

impl SinkError {
    /// Cierto si tiene caso seguir escribiendo.
    pub fn is_recoverable(&self) -> bool {
        matches!(self, SinkError::Xrun)
    }
}

/// Un dispositivo de salida abierto.
pub trait AudioSink {
    /// Con qué formato quedó abierto de verdad.
    fn format(&self) -> StreamFormat;

    /// Escribe frames y regresa cuántos aceptó. Puede aceptar menos: quien
    /// llama reintenta el resto.
    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError>;

    /// Suelta el dispositivo. No espera a que se vacíe el búfer ni a que el
    /// dispositivo conteste.
    fn close(&mut self) -> Result<(), SinkError>;
}

impl AudioSink for Box<dyn AudioSink + Send> {
    fn format(&self) -> StreamFormat {
        (**self).format()
    }

    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
        (**self).write(frames)
    }

    fn close(&mut self) -> Result<(), SinkError> {
        (**self).close()
    }
}

/// La bandera con la que se para desde otro hilo.
///
/// Parar no manda un comando por un canal que pueda quedarse lleno, ni toma
/// el candado del dispositivo: levanta esto, y el lazo de escritura lo mira
/// entre bloque y bloque.
#[derive(Clone, Debug, Default)]
pub struct StopSignal(Arc<AtomicBool>);

impl StopSignal {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn stop(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_stopped(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    /// Limpia la bandera para la siguiente pista. Una orden de paro vieja no
    /// debe matar lo que empieza después.
    pub fn reset(&self) {
        self.0.store(false, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solo_el_xrun_deja_seguir() {
        assert!(SinkError::Xrun.is_recoverable());
        assert!(!SinkError::DeviceLost.is_recoverable());
        assert!(!SinkError::Timeout(Duration::from_millis(200)).is_recoverable());
    }

    #[test]
    fn limpiar_la_bandera_deja_lista_la_siguiente_pista() {
        let signal = StopSignal::new();
        signal.stop();
        signal.reset();
        assert!(!signal.is_stopped());
    }

    #[test]
    fn la_señal_de_paro_se_ve_desde_otro_hilo() {
        let signal = StopSignal::new();
        let copia = signal.clone();
        assert!(!signal.is_stopped());
        std::thread::spawn(move || copia.stop()).join().unwrap();
        assert!(signal.is_stopped());
    }
}
