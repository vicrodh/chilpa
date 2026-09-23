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
const MAX_XRUNS_SEGUIDOS: u32 = 8;

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
        let mut avisado = false;
        while offset < block.len() {
            if stop.is_stopped() {
                return Outcome::Stopped { frames };
            }
            if pause.is_paused() {
                if !avisado {
                    let _ = sink.set_paused(true);
                    avisado = true;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
                continue;
            }
            if avisado {
                let _ = sink.set_paused(false);
                avisado = false;
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
                    if xruns >= MAX_XRUNS_SEGUIDOS {
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
        let mut fuente = Bloques(vec![bloque(8)]);
        let mut sink = Falso::nuevo(100);
        let listo = Arc::new(AtomicBool::new(false));

        std::thread::scope(|s| {
            let pause_hilo = pause.clone();
            let listo_hilo = Arc::clone(&listo);
            s.spawn(move || {
                // Mientras está en pausa no debe salir nada.
                std::thread::sleep(Duration::from_millis(60));
                listo_hilo.store(true, Ordering::Release);
                pause_hilo.set(false);
            });
            let out = render_paused(&mut fuente, &mut sink, &StopSignal::new(), &pause, &mut |_| {});
            assert!(listo.load(Ordering::Acquire), "no escribió antes de que se quitara la pausa");
            assert_eq!(out, Outcome::Finished { frames: 8 });
        });
    }

    #[test]
    fn parar_desde_una_pausa_sale_de_inmediato() {
        let pause = PauseSignal::new();
        pause.set(true);
        let stop = StopSignal::new();
        stop.stop();
        let mut fuente = Bloques(vec![bloque(8)]);
        let mut sink = Falso::nuevo(100);
        let out = render_paused(&mut fuente, &mut sink, &stop, &pause, &mut |_| {});
        assert_eq!(out, Outcome::Stopped { frames: 0 });
    }

    #[test]
    fn el_progreso_avisa_los_frames_que_de_verdad_salieron() {
        let mut fuente = Bloques(vec![bloque(10), bloque(6)]);
        let mut sink = Falso::nuevo(4);
        let mut avisos = Vec::new();
        let out = render_with(&mut fuente, &mut sink, &StopSignal::new(), &mut |f| avisos.push(f));
        assert_eq!(out, Outcome::Finished { frames: 16 });
        assert_eq!(avisos.last(), Some(&16));
        assert!(avisos.windows(2).all(|p| p[0] < p[1]), "el progreso solo avanza");
    }

    struct Bloques(Vec<Vec<u8>>);

    impl FrameSource for Bloques {
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
    struct Falso {
        accept: usize,
        fail_at: Option<(usize, SinkError)>,
        escrituras: usize,
        recibidos: usize,
        cerrado: bool,
    }

    impl Falso {
        fn nuevo(accept: usize) -> Self {
            Self { accept, fail_at: None, escrituras: 0, recibidos: 0, cerrado: false }
        }

        fn que_falla(accept: usize, en: usize, error: SinkError) -> Self {
            Self { fail_at: Some((en, error)), ..Self::nuevo(accept) }
        }
    }

    impl AudioSink for Falso {
        fn format(&self) -> StreamFormat {
            StreamFormat::new(SampleFormat::S16, 44_100, 2)
        }

        fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
            self.escrituras += 1;
            if let Some((en, error)) = &self.fail_at {
                if self.escrituras == *en {
                    return Err(error.clone());
                }
            }
            let cabe = (frames.len() / 4).min(self.accept);
            self.recibidos += cabe;
            Ok(cabe)
        }

        fn close(&mut self) -> Result<(), SinkError> {
            self.cerrado = true;
            Ok(())
        }
    }

    fn bloque(frames: usize) -> Vec<u8> {
        vec![0u8; frames * 4]
    }

    #[test]
    fn una_pista_completa_termina_y_deja_avanzar_la_cola() {
        let mut fuente = Bloques(vec![bloque(10), bloque(10)]);
        let mut sink = Falso::nuevo(100);
        let out = render(&mut fuente, &mut sink, &StopSignal::new());
        assert_eq!(out, Outcome::Finished { frames: 20 });
        assert!(out.may_advance_queue());
    }

    #[test]
    fn las_escrituras_parciales_se_reintentan_hasta_entregar_todo() {
        let mut fuente = Bloques(vec![bloque(10)]);
        let mut sink = Falso::nuevo(3);
        let out = render(&mut fuente, &mut sink, &StopSignal::new());
        assert_eq!(out, Outcome::Finished { frames: 10 });
        assert_eq!(sink.recibidos, 10);
        assert!(sink.escrituras > 1, "se necesitaron varias escrituras");
    }

    #[test]
    fn un_dispositivo_perdido_no_se_confunde_con_fin_de_pista() {
        // El hallazgo: en QBZ esto avanzaba la cola entera sin avisar.
        let mut fuente = Bloques(vec![bloque(10), bloque(10)]);
        let mut sink = Falso::que_falla(100, 1, SinkError::DeviceLost);
        let out = render(&mut fuente, &mut sink, &StopSignal::new());
        assert_eq!(out, Outcome::Failed { frames: 0, error: SinkError::DeviceLost });
        assert!(!out.may_advance_queue(), "la cola NO avanza");
    }

    #[test]
    fn un_timeout_tambien_es_falla_y_no_espera_para_siempre() {
        let mut fuente = Bloques(vec![bloque(4)]);
        let espera = Duration::from_millis(200);
        let mut sink = Falso::que_falla(100, 1, SinkError::Timeout(espera));
        let out = render(&mut fuente, &mut sink, &StopSignal::new());
        assert_eq!(out, Outcome::Failed { frames: 0, error: SinkError::Timeout(espera) });
    }

    #[test]
    fn un_xrun_suelto_se_aguanta_y_la_pista_termina_bien() {
        let mut fuente = Bloques(vec![bloque(6)]);
        let mut sink = Falso::que_falla(100, 1, SinkError::Xrun);
        assert_eq!(render(&mut fuente, &mut sink, &StopSignal::new()), Outcome::Finished { frames: 6 });
    }

    #[test]
    fn muchos_xruns_seguidos_dan_el_dispositivo_por_perdido() {
        struct SiempreXrun;
        impl AudioSink for SiempreXrun {
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
        let mut fuente = Bloques(vec![bloque(4)]);
        let out = render(&mut fuente, &mut SiempreXrun, &StopSignal::new());
        assert_eq!(out, Outcome::Failed { frames: 0, error: SinkError::DeviceLost });
    }

    #[test]
    fn parar_no_espera_al_dispositivo() {
        // La señal ya está puesta antes de empezar: el lazo la ve entre
        // bloques y sale sin pedirle nada al dispositivo.
        let stop = StopSignal::new();
        stop.stop();
        let mut fuente = Bloques(vec![bloque(10)]);
        let mut sink = Falso::nuevo(1);
        assert_eq!(render(&mut fuente, &mut sink, &stop), Outcome::Stopped { frames: 0 });
        assert_eq!(sink.escrituras, 0, "no se le escribió nada al dispositivo");
    }

    #[test]
    fn parar_a_media_pista_sale_entre_escrituras() {
        struct ParaEnLaTercera(StopSignal, usize);
        impl AudioSink for ParaEnLaTercera {
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
        let mut fuente = Bloques(vec![bloque(20)]);
        let mut sink = ParaEnLaTercera(stop.clone(), 0);
        assert_eq!(render(&mut fuente, &mut sink, &stop), Outcome::Stopped { frames: 6 });
    }
}
