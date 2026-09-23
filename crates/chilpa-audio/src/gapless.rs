//! Pegar dos pistas sin hueco y sin muestras de más.
//!
//! Dos cosas hay que resolver para que gapless sea de verdad:
//!
//! 1. **El relleno del codificador.** AAC y MP3 traen frames de arranque y de
//!    cola que el codificador metió y que no son música. Si no se recortan,
//!    entre pista y pista se oye un chasquido; si se recortan de más, se come
//!    música. El recorte tiene que ser exacto aunque caiga a mitad de un
//!    bloque.
//! 2. **El formato.** Solo se puede seguir sin cortar si la pista que sigue
//!    suena con el MISMO formato de stream. Si cambia la frecuencia o los
//!    bits, hay que renegociar el dispositivo, y eso siempre deja un hueco:
//!    se dice, no se disimula.

use crate::format::StreamFormat;

/// Qué se puede hacer al llegar a la siguiente pista.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Transition {
    /// Mismo formato: se sigue escribiendo en el mismo dispositivo, sin
    /// cortar ni vaciar el búfer.
    Seamless,
    /// Cambia el formato: hay que reabrir el dispositivo. El hueco es
    /// inevitable, y el usuario merece saber por qué.
    Renegotiate { from: StreamFormat, to: StreamFormat },
}

impl Transition {
    pub fn is_seamless(&self) -> bool {
        matches!(self, Transition::Seamless)
    }
}

/// Decide cómo pasar de una pista a la siguiente.
pub fn transition(current: StreamFormat, next: StreamFormat) -> Transition {
    if current.same_stream(&next) {
        Transition::Seamless
    } else {
        Transition::Renegotiate { from: current, to: next }
    }
}

/// Recorta el relleno del codificador contando frames, no bloques.
///
/// El recorte del final necesita saber cuántos frames tiene la pista; si el
/// contenedor no lo dice, se retienen los últimos `padding` frames hasta
/// saber si eran los últimos (`finish`).
#[derive(Debug)]
pub struct Trimmer {
    delay: u64,
    padding: usize,
    channels: usize,
    consumed: u64,
    /// Frames retenidos a la espera de saber si son el final.
    queue: Vec<i32>,
}

impl Trimmer {
    /// `delay` y `padding` en frames, como los declara el contenedor.
    pub fn new(delay: u64, padding: u32, channels: u16) -> Self {
        Self {
            delay,
            padding: padding as usize,
            channels: channels.max(1) as usize,
            consumed: 0,
            queue: Vec::new(),
        }
    }

    /// Frames que ya salieron, sin contar los recortados.
    pub fn emitted(&self) -> u64 {
        self.consumed
    }

    /// Pasa un bloque por el recorte. Lo que regresa ya es música.
    pub fn push(&mut self, block: &[i32]) -> Vec<i32> {
        let mut data = std::mem::take(&mut self.queue);
        data.extend_from_slice(block);

        // Recorte del arranque: puede abarcar varios bloques.
        let pending = self.delay.saturating_sub(self.consumed);
        let frames = data.len() / self.channels;
        let skip = pending.min(frames as u64) as usize;
        if skip > 0 {
            data.drain(..skip * self.channels);
            self.consumed += skip as u64;
        }

        // Se retienen los últimos `padding` frames por si son la cola.
        let frames = data.len() / self.channels;
        let hold = self.padding.min(frames);
        let cut = (frames - hold) * self.channels;
        self.queue = data.split_off(cut);
        self.consumed += (data.len() / self.channels) as u64;
        data
    }

    /// Se acabó la pista: lo retenido era relleno y se tira.
    pub fn finish(&mut self) -> Vec<i32> {
        self.queue.clear();
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::SampleFormat;

    fn fmt(rate: u32) -> StreamFormat {
        StreamFormat::new(SampleFormat::S16, rate, 2)
    }

    fn frames(from: i32, n: i32) -> Vec<i32> {
        (from..from + n).flat_map(|v| [v, -v]).collect()
    }

    #[test]
    fn el_mismo_formato_encadena_sin_cortar() {
        assert!(transition(fmt(44_100), fmt(44_100)).is_seamless());
    }

    #[test]
    fn cambiar_de_frecuencia_obliga_a_renegociar_y_se_dice() {
        let t = transition(fmt(44_100), fmt(96_000));
        assert!(!t.is_seamless());
        assert!(matches!(t, Transition::Renegotiate { .. }));
    }

    #[test]
    fn sin_relleno_no_se_recorta_nada() {
        let mut t = Trimmer::new(0, 0, 2);
        let output = t.push(&frames(0, 10));
        assert_eq!(output, frames(0, 10));
        assert_eq!(t.emitted(), 10);
    }

    #[test]
    fn el_recorte_de_arranque_cruza_bloques() {
        // 5 frames de delay repartidos entre dos bloques de 3.
        let mut t = Trimmer::new(5, 0, 2);
        assert!(t.push(&frames(0, 3)).is_empty(), "el primer bloque es todo relleno");
        let second = t.push(&frames(3, 3));
        assert_eq!(second, frames(5, 1), "solo sale el frame 5 en adelante");
    }

    #[test]
    fn el_recorte_de_cola_no_deja_salir_el_relleno_final() {
        let mut t = Trimmer::new(0, 4, 2);
        let primero = t.push(&frames(0, 10));
        assert_eq!(primero, frames(0, 6), "se retienen los 4 últimos");
        assert!(t.finish().is_empty(), "el relleno retenido se tira");
        assert_eq!(t.emitted(), 6, "la pista duró 6 frames de música");
    }

    #[test]
    fn lo_retenido_sale_cuando_llega_mas_musica() {
        let mut t = Trimmer::new(0, 4, 2);
        t.push(&frames(0, 10));
        let second = t.push(&frames(10, 2));
        assert_eq!(second, frames(6, 2), "lo retenido resulta ser música");
    }

    #[test]
    fn delay_y_padding_juntos_dejan_la_pista_exacta() {
        let mut t = Trimmer::new(3, 2, 2);
        let mut output = t.push(&frames(0, 12));
        output.extend(t.finish());
        assert_eq!(output, frames(3, 7), "12 frames menos 3 de arranque y 2 de cola");
    }
}
