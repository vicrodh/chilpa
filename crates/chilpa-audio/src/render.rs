//! El lazo que lleva las muestras al dispositivo y dice cómo terminó.
//!
//! El punto de este módulo es que **el final de una pista tiene tipo**. En
//! QBZ, una escritura fallida caía en el mismo camino que un fin natural, así
//! que un DAC desconectado se veía como "ya acabó" y la cola se saltaba
//! entera en silencio. Aquí solo [`Outcome::Finished`] permite avanzar la
//! cola; lo demás se le cuenta al usuario.

use crate::sink::{AudioSink, PauseSignal, SinkError, StopSignal};

/// De dónde salen los bytes que van al dispositivo.
pub trait FrameSource {
    /// El siguiente bloque, ya en el formato del dispositivo. `None` = se
    /// acabó la pista de forma natural.
    fn next_block(&mut self) -> Option<Vec<u8>>;
}

/// Cómo terminó una pista.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Outcome {
    /// La fuente se agotó y todo lo suyo se entregó. Es el ÚNICO final que
    /// permite avanzar a la siguiente pista.
    Finished { frames: u64 },
    /// Alguien pidió parar.
    Stopped { frames: u64 },
    /// El dispositivo falló. La cola no avanza y el usuario se entera.
    Failed { frames: u64, error: SinkError },
}

impl Outcome {
    pub fn may_advance_queue(&self) -> bool {
        matches!(self, Outcome::Finished { .. })
    }

    pub fn frames(&self) -> u64 {
        match self {
            Outcome::Finished { frames } | Outcome::Stopped { frames } | Outcome::Failed { frames, .. } => *frames,
        }
    }
}

/// Cuántos xruns seguidos se toleran antes de dar el dispositivo por perdido.
const MAX_CONSECUTIVE_XRUNS: u32 = 8;

/// Entrega todo lo que dé la fuente y regresa cómo terminó.
///
/// Mira la señal de paro entre bloques, así que parar no depende de que el
/// dispositivo conteste. Reintenta las escrituras parciales, tolera algunos
/// xruns seguidos y, cuando el dispositivo se pierde o se tarda de más, no
/// finge un final natural.
pub fn render<S, K>(source: &mut S, sink: &mut K, stop: &StopSignal) -> Outcome
where
    S: FrameSource,
    K: AudioSink,
{
    render_with(source, sink, stop, &mut |_| {})
}

/// Lo mismo, avisando cuántos frames llevan entregados.
///
/// El aviso sale del lazo de entrega, así que la posición que ve la interfaz
/// es la que de verdad salió por el cable y no una cuenta del reloj. Se llama
/// una vez por escritura aceptada, y nunca desde el hilo de audio del
/// sistema.
pub fn render_with<S, K>(
    source: &mut S,
    sink: &mut K,
    stop: &StopSignal,
    on_progress: &mut dyn FnMut(u64),
) -> Outcome
where
    S: FrameSource,
    K: AudioSink,
{
    render_paused(source, sink, stop, &PauseSignal::new(), on_progress)
}

/// Lo mismo, atendiendo también la pausa.
///
/// En pausa el lazo deja de escribir y espera, y le avisa al dispositivo por
/// si sabe pausar de verdad. Sigue mirando la señal de paro, así que parar
/// desde una pausa sale de inmediato.
pub fn render_paused<S, K>(
    source: &mut S,
    sink: &mut K,
    stop: &StopSignal,
    pause: &PauseSignal,
    on_progress: &mut dyn FnMut(u64),
) -> Outcome
where
    S: FrameSource,
    K: AudioSink,
{
    let frame_bytes = sink.format().frame_bytes();
    let mut frames: u64 = 0;
    let mut xruns = 0;

    while !stop.is_stopped() {
        let Some(block) = source.next_block() else {
            return Outcome::Finished { frames };
        };
        let mut offset = 0;
        let mut warned = false;
        while offset < block.len() {
            if stop.is_stopped() {
                return Outcome::Stopped { frames };
            }
            if pause.is_paused() {
                if !warned {
                    let _ = sink.set_paused(true);
                    warned = true;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
                continue;
            }
            if warned {
                let _ = sink.set_paused(false);
                warned = false;
            }
            match sink.write(&block[offset..]) {
                Ok(0) => break,
                Ok(written) => {
                    xruns = 0;
                    offset += written * frame_bytes;
                    frames += written as u64;
                    on_progress(frames);
                }
                Err(e) if e.is_recoverable() => {
                    xruns += 1;
                    if xruns >= MAX_CONSECUTIVE_XRUNS {
                        return Outcome::Failed { frames, error: SinkError::DeviceLost };
                    }
                }
                Err(error) => return Outcome::Failed { frames, error },
            }
        }
    }
    Outcome::Stopped { frames }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::{SampleFormat, StreamFormat};
    use std::time::Duration;

    #[test]
    fn en_pausa_no_se_escribe_y_al_reanudar_sigue_donde_iba() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let pause = PauseSignal::new();
        pause.set(true);
        let mut source = Blocks(vec![block(8)]);
        let mut sink = FakeSink::new(100);
        let ready = Arc::new(AtomicBool::new(false));

        std::thread::scope(|s| {
            let pause_thread = pause.clone();
            let ready_thread = Arc::clone(&ready);
            s.spawn(move || {
                // Mientras está en pausa no debe salir nada.
                std::thread::sleep(Duration::from_millis(60));
                ready_thread.store(true, Ordering::Release);
                pause_thread.set(false);
            });
            let out = render_paused(&mut source, &mut sink, &StopSignal::new(), &pause, &mut |_| {});
            assert!(ready.load(Ordering::Acquire), "no escribió antes de que se quitara la pausa");
            assert_eq!(out, Outcome::Finished { frames: 8 });
        });
    }

    #[test]
    fn parar_desde_una_pausa_sale_de_inmediato() {
        let pause = PauseSignal::new();
        pause.set(true);
        let stop = StopSignal::new();
        stop.stop();
        let mut source = Blocks(vec![block(8)]);
        let mut sink = FakeSink::new(100);
        let out = render_paused(&mut source, &mut sink, &stop, &pause, &mut |_| {});
        assert_eq!(out, Outcome::Stopped { frames: 0 });
    }

    #[test]
    fn el_progreso_avisa_los_frames_que_de_verdad_salieron() {
        let mut source = Blocks(vec![block(10), block(6)]);
        let mut sink = FakeSink::new(4);
        let mut warnings = Vec::new();
        let out = render_with(&mut source, &mut sink, &StopSignal::new(), &mut |f| warnings.push(f));
        assert_eq!(out, Outcome::Finished { frames: 16 });
        assert_eq!(warnings.last(), Some(&16));
        assert!(warnings.windows(2).all(|p| p[0] < p[1]), "el progreso solo avanza");
    }

    struct Blocks(Vec<Vec<u8>>);

    impl FrameSource for Blocks {
        fn next_block(&mut self) -> Option<Vec<u8>> {
            if self.0.is_empty() {
                None
            } else {
                Some(self.0.remove(0))
            }
        }
    }

    /// Un dispositivo de mentiras: acepta `accept` frames por escritura y
    /// falla en la escritura número `fail_at` con `error`.
    struct FakeSink {
        accept: usize,
        fail_at: Option<(usize, SinkError)>,
        writes: usize,
        received: usize,
        closed: bool,
    }

    impl FakeSink {
        fn new(accept: usize) -> Self {
            Self { accept, fail_at: None, writes: 0, received: 0, closed: false }
        }

        fn failing(accept: usize, en: usize, error: SinkError) -> Self {
            Self { fail_at: Some((en, error)), ..Self::new(accept) }
        }
    }

    impl AudioSink for FakeSink {
        fn format(&self) -> StreamFormat {
            StreamFormat::new(SampleFormat::S16, 44_100, 2)
        }

        fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
            self.writes += 1;
            if let Some((en, error)) = &self.fail_at {
                if self.writes == *en {
                    return Err(error.clone());
                }
            }
            let fits = (frames.len() / 4).min(self.accept);
            self.received += fits;
            Ok(fits)
        }

        fn close(&mut self) -> Result<(), SinkError> {
            self.closed = true;
            Ok(())
        }
    }

    fn block(frames: usize) -> Vec<u8> {
        vec![0u8; frames * 4]
    }

    #[test]
    fn una_pista_completa_termina_y_deja_avanzar_la_cola() {
        let mut source = Blocks(vec![block(10), block(10)]);
        let mut sink = FakeSink::new(100);
        let out = render(&mut source, &mut sink, &StopSignal::new());
        assert_eq!(out, Outcome::Finished { frames: 20 });
        assert!(out.may_advance_queue());
    }

    #[test]
    fn las_escrituras_parciales_se_reintentan_hasta_entregar_todo() {
        let mut source = Blocks(vec![block(10)]);
        let mut sink = FakeSink::new(3);
        let out = render(&mut source, &mut sink, &StopSignal::new());
        assert_eq!(out, Outcome::Finished { frames: 10 });
        assert_eq!(sink.received, 10);
        assert!(sink.writes > 1, "se necesitaron varias escrituras");
    }

    #[test]
    fn un_dispositivo_perdido_no_se_confunde_con_fin_de_pista() {
        // El hallazgo: en QBZ esto avanzaba la cola entera sin avisar.
        let mut source = Blocks(vec![block(10), block(10)]);
        let mut sink = FakeSink::failing(100, 1, SinkError::DeviceLost);
        let out = render(&mut source, &mut sink, &StopSignal::new());
        assert_eq!(out, Outcome::Failed { frames: 0, error: SinkError::DeviceLost });
        assert!(!out.may_advance_queue(), "la cola NO avanza");
    }

    #[test]
    fn un_timeout_tambien_es_falla_y_no_espera_para_siempre() {
        let mut source = Blocks(vec![block(4)]);
        let wait = Duration::from_millis(200);
        let mut sink = FakeSink::failing(100, 1, SinkError::Timeout(wait));
        let out = render(&mut source, &mut sink, &StopSignal::new());
        assert_eq!(out, Outcome::Failed { frames: 0, error: SinkError::Timeout(wait) });
    }

    #[test]
    fn un_xrun_suelto_se_aguanta_y_la_pista_termina_bien() {
        let mut source = Blocks(vec![block(6)]);
        let mut sink = FakeSink::failing(100, 1, SinkError::Xrun);
        assert_eq!(render(&mut source, &mut sink, &StopSignal::new()), Outcome::Finished { frames: 6 });
    }

    #[test]
    fn muchos_xruns_seguidos_dan_el_dispositivo_por_perdido() {
        struct AlwaysXrun;
        impl AudioSink for AlwaysXrun {
            fn format(&self) -> StreamFormat {
                StreamFormat::new(SampleFormat::S16, 44_100, 2)
            }
            fn write(&mut self, _: &[u8]) -> Result<usize, SinkError> {
                Err(SinkError::Xrun)
            }
            fn close(&mut self) -> Result<(), SinkError> {
                Ok(())
            }
        }
        let mut source = Blocks(vec![block(4)]);
        let out = render(&mut source, &mut AlwaysXrun, &StopSignal::new());
        assert_eq!(out, Outcome::Failed { frames: 0, error: SinkError::DeviceLost });
    }

    #[test]
    fn parar_no_espera_al_dispositivo() {
        // La señal ya está puesta antes de empezar: el lazo la ve entre
        // bloques y sale sin pedirle nada al dispositivo.
        let stop = StopSignal::new();
        stop.stop();
        let mut source = Blocks(vec![block(10)]);
        let mut sink = FakeSink::new(1);
        assert_eq!(render(&mut source, &mut sink, &stop), Outcome::Stopped { frames: 0 });
        assert_eq!(sink.writes, 0, "no se le escribió nada al dispositivo");
    }

    #[test]
    fn parar_a_media_pista_sale_entre_escrituras() {
        struct StopsOnTheThird(StopSignal, usize);
        impl AudioSink for StopsOnTheThird {
            fn format(&self) -> StreamFormat {
                StreamFormat::new(SampleFormat::S16, 44_100, 2)
            }
            fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
                self.1 += 1;
                if self.1 == 3 {
                    self.0.stop();
                }
                Ok((frames.len() / 4).min(2))
            }
            fn close(&mut self) -> Result<(), SinkError> {
                Ok(())
            }
        }
        let stop = StopSignal::new();
        let mut source = Blocks(vec![block(20)]);
        let mut sink = StopsOnTheThird(stop.clone(), 0);
        assert_eq!(render(&mut source, &mut sink, &stop), Outcome::Stopped { frames: 6 });
    }
}
