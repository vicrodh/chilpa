//! La cola y el estado de reproducción.

use crate::track::TrackRef;
use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum RepeatMode {
    #[default]
    Off,
    All,
    One,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum PlaybackState {
    #[default]
    Stopped,
    Playing,
    Paused,
}

/// Un elemento de la cola: qué track y desde dónde se encoló.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct QueueItem {
    pub track: TrackRef,
    /// El contenedor del que salió (álbum, playlist, artista), para el
    /// "estás escuchando desde". Se sella al encolar, por elemento, así que
    /// nunca queda un valor global viejo.
    pub context: Option<String>,
}

impl QueueItem {
    pub fn new(track: TrackRef) -> Self {
        Self { track, context: None }
    }
}

/// La cola, con su orden de reproducción explícito.
///
/// `order` son índices sobre `items`. Chilpa nunca genera un orden por su
/// cuenta cuando alguien más manda: si la cola la gobierna un servicio
/// remoto, se aplica el orden que ese servicio devuelve con
/// [`Queue::set_order`]. Generar un orden local en paralelo es exactamente el
/// error que se repitió en QBZ y que cuatro modelos volvieron a cometer.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Queue {
    items: Vec<QueueItem>,
    order: Vec<usize>,
    position: usize,
    pub repeat: RepeatMode,
}

impl Queue {
    pub fn new(items: Vec<QueueItem>) -> Self {
        let order = (0..items.len()).collect();
        Self { items, order, position: 0, repeat: RepeatMode::Off }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Lo que suena ahora, si hay algo.
    pub fn current(&self) -> Option<&QueueItem> {
        self.order.get(self.position).and_then(|i| self.items.get(*i))
    }

    /// Aplica un orden que viene de afuera. Se rechaza el que no sea una
    /// permutación exacta de la cola, para no quedar con una cola a medias.
    pub fn set_order(&mut self, order: Vec<usize>) -> Result<(), OrderError> {
        if order.len() != self.items.len() {
            return Err(OrderError::WrongLength { expected: self.items.len(), got: order.len() });
        }
        let mut seen = vec![false; self.items.len()];
        for &i in &order {
            match seen.get_mut(i) {
                Some(visto) if !*visto => *visto = true,
                Some(_) => return Err(OrderError::Repeated(i)),
                None => return Err(OrderError::OutOfRange(i)),
            }
        }
        let actual = self.current().cloned();
        self.order = order;
        // Seguir sonando lo mismo: la posición se recalcula sobre el orden
        // nuevo en lugar de saltar a otra pista.
        if let Some(actual) = actual {
            if let Some(p) = self.order.iter().position(|i| self.items[*i] == actual) {
                self.position = p;
            }
        }
        Ok(())
    }

    /// Avanza. Regresa `None` cuando la cola se acabó y no hay repetición.
    pub fn advance(&mut self) -> Option<&QueueItem> {
        if self.items.is_empty() {
            return None;
        }
        match self.repeat {
            RepeatMode::One => {}
            RepeatMode::All => self.position = (self.position + 1) % self.order.len(),
            RepeatMode::Off => {
                if self.position + 1 >= self.order.len() {
                    return None;
                }
                self.position += 1;
            }
        }
        self.current()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OrderError {
    WrongLength { expected: usize, got: usize },
    Repeated(usize),
    OutOfRange(usize),
}

/// Cuánto lleva sonando la pista actual, contado en frames reproducidos.
///
/// QBZ medía la posición con el reloj del sistema en segundos enteros, y de
/// ahí salían los saltos y los cambios de pista dobles. Los frames son lo
/// único que conoce la verdad de la salida.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Progress {
    pub frames_played: u64,
    pub sample_rate_hz: u32,
}

impl Progress {
    pub fn position(&self) -> Duration {
        if self.sample_rate_hz == 0 {
            return Duration::ZERO;
        }
        Duration::from_nanos(self.frames_played * 1_000_000_000 / u64::from(self.sample_rate_hz))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceId;

    fn cola(n: usize) -> Queue {
        Queue::new(
            (0..n)
                .map(|i| QueueItem::new(TrackRef::new(SourceId::local(), i.to_string())))
                .collect(),
        )
    }

    #[test]
    fn la_cola_arranca_en_el_primero() {
        assert_eq!(cola(3).current().unwrap().track.external_id, "0");
    }

    #[test]
    fn un_orden_de_afuera_no_cambia_lo_que_suena() {
        let mut q = cola(3);
        q.set_order(vec![2, 0, 1]).unwrap();
        assert_eq!(q.current().unwrap().track.external_id, "0", "sigue sonando el mismo track");
        assert_eq!(q.advance().unwrap().track.external_id, "1");
    }

    #[test]
    fn se_rechaza_un_orden_que_no_es_permutacion() {
        let mut q = cola(3);
        assert_eq!(q.set_order(vec![0, 1]), Err(OrderError::WrongLength { expected: 3, got: 2 }));
        assert_eq!(q.set_order(vec![0, 1, 1]), Err(OrderError::Repeated(1)));
        assert_eq!(q.set_order(vec![0, 1, 9]), Err(OrderError::OutOfRange(9)));
        assert_eq!(q.current().unwrap().track.external_id, "0", "la cola no quedó a medias");
    }

    #[test]
    fn sin_repeticion_la_cola_se_acaba() {
        let mut q = cola(2);
        assert!(q.advance().is_some());
        assert!(q.advance().is_none());
    }

    #[test]
    fn repetir_todo_da_la_vuelta_y_repetir_una_se_queda() {
        let mut q = cola(2);
        q.repeat = RepeatMode::All;
        q.advance();
        assert_eq!(q.advance().unwrap().track.external_id, "0");
        let mut q = cola(2);
        q.repeat = RepeatMode::One;
        assert_eq!(q.advance().unwrap().track.external_id, "0");
    }

    #[test]
    fn la_posicion_se_cuenta_en_frames() {
        let p = Progress { frames_played: 44_100, sample_rate_hz: 44_100 };
        assert_eq!(p.position(), Duration::from_secs(1));
        let p = Progress { frames_played: 96_000 * 3 / 2, sample_rate_hz: 96_000 };
        assert_eq!(p.position(), Duration::from_millis(1500), "sin truncar a segundos enteros");
    }
}
