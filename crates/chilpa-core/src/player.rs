//! El reproductor: suena en su propio hilo y cuenta lo que pasa.
//!
//! La interfaz no se bloquea nunca. Manda una orden, sigue con lo suyo y va
//! recogiendo eventos. Parar levanta una bandera que el lazo de entrega mira
//! entre bloques, así que no espera al dispositivo.

use chilpa_audio::negotiate::{DeviceCaps, Policy};
use chilpa_audio::render::Outcome;
use chilpa_audio::sink::{AudioSink, SinkError, StopSignal};
use chilpa_audio::format::StreamFormat;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::thread::JoinHandle;

use crate::playback::{play_file_with, PlayError};

/// Lo que el reproductor le cuenta a quien lo controla.
#[derive(Clone, PartialEq, Debug)]
pub enum PlayerEvent {
    /// Empezó a sonar: con qué formato se abrió el dispositivo y si la
    /// entrega es bit-perfect.
    Started { path: PathBuf, opened: StreamFormat, bit_perfect: bool, total_frames: Option<u64> },
    /// Frames entregados hasta ahora.
    Progress { frames: u64 },
    /// Terminó de forma natural.
    Finished { frames: u64 },
    /// Se detuvo porque alguien lo pidió.
    Stopped { frames: u64 },
    /// Algo falló, en un texto que la interfaz puede mostrar.
    Failed { message: String },
}

enum Order {
    Play { path: PathBuf, device: String, bit_perfect: bool },
    Stop,
    Quit,
}

/// Abre el dispositivo de salida. Se inyecta para que las pruebas usen uno
/// falso y la aplicación use el real.
pub type OpenSink = Box<dyn FnMut(&str, StreamFormat) -> Result<Box<dyn AudioSink + Send>, SinkError> + Send>;

/// Mide un dispositivo por su nombre.
pub type ProbeDevice = Box<dyn FnMut(&str) -> Result<DeviceCaps, SinkError> + Send>;

/// El reproductor, visto desde la interfaz.
pub struct Player {
    orders: Sender<Order>,
    events: Receiver<PlayerEvent>,
    stop: StopSignal,
    thread: Option<JoinHandle<()>>,
}

impl Player {
    /// Arranca el hilo del reproductor.
    pub fn spawn(mut probe: ProbeDevice, mut open: OpenSink) -> Self {
        let (orders_tx, orders_rx) = channel::<Order>();
        let (events_tx, events_rx) = channel::<PlayerEvent>();
        let stop = StopSignal::new();
        let stop_hilo = stop.clone();

        let thread = std::thread::Builder::new()
            .name("chilpa-player".into())
            .spawn(move || {
                while let Ok(order) = orders_rx.recv() {
                    match order {
                        Order::Quit => break,
                        Order::Stop => {}
                        Order::Play { path, device, bit_perfect } => {
                            // Cada pista arranca con la bandera limpia: una
                            // orden de paro vieja no mata la siguiente.
                            stop_hilo.reset();
                            let stop = stop_hilo.clone();
                            let resultado = reproducir(
                                &path,
                                &device,
                                bit_perfect,
                                &stop,
                                &mut probe,
                                &mut open,
                                &events_tx,
                            );
                            let evento = match resultado {
                                Ok(Outcome::Finished { frames }) => PlayerEvent::Finished { frames },
                                Ok(Outcome::Stopped { frames }) => PlayerEvent::Stopped { frames },
                                Ok(Outcome::Failed { frames, error }) => PlayerEvent::Failed {
                                    message: format!("el dispositivo falló tras {frames} frames: {error:?}"),
                                },
                                Err(e) => PlayerEvent::Failed { message: e.to_string() },
                            };
                            let _ = events_tx.send(evento);
                        }
                    }
                }
            })
            .expect("el hilo del reproductor arranca");

        Self { orders: orders_tx, events: events_rx, stop, thread: Some(thread) }
    }

    /// Pide reproducir un archivo. Regresa de inmediato.
    pub fn play(&self, path: &Path, device: &str, bit_perfect: bool) {
        let _ = self.orders.send(Order::Play {
            path: path.to_path_buf(),
            device: device.to_string(),
            bit_perfect,
        });
    }

    /// Pide parar. No espera al dispositivo.
    pub fn stop(&self) {
        self.stop.stop();
        let _ = self.orders.send(Order::Stop);
    }

    /// Los eventos que haya, sin bloquear.
    pub fn poll(&self) -> Vec<PlayerEvent> {
        let mut out = Vec::new();
        loop {
            match self.events.try_recv() {
                Ok(e) => out.push(e),
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            }
        }
        out
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.stop.stop();
        let _ = self.orders.send(Order::Quit);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn reproducir(
    path: &Path,
    device: &str,
    bit_perfect: bool,
    stop: &StopSignal,
    probe: &mut ProbeDevice,
    open: &mut OpenSink,
    events: &Sender<PlayerEvent>,
) -> Result<Outcome, PlayError> {
    let caps = probe(device).map_err(PlayError::Sink)?;
    let policy = if bit_perfect { Policy::BitPerfect } else { Policy::AllowAnything };
    let ruta = path.to_path_buf();
    let eventos = events.clone();
    let duracion = chilpa_decode::Decoded::open(path).ok().and_then(|d| d.frame_count());
    play_file_with(
        path,
        &caps,
        policy,
        stop,
        |formato| {
            let sink = open(device, formato)?;
            let _ = eventos.send(PlayerEvent::Started {
                path: ruta.clone(),
                opened: formato,
                bit_perfect: false,
                total_frames: duracion,
            });
            Ok(sink)
        },
        &mut |frames| {
            let _ = events.send(PlayerEvent::Progress { frames });
        },
    )
    .map(|played| {
        // El aviso de arranque sale antes de saber el veredicto, así que la
        // verdad sobre bit-perfect se manda ya con la entrega abierta.
        let _ = events.send(PlayerEvent::Started {
            path: path.to_path_buf(),
            opened: played.delivery.opened,
            bit_perfect: played.delivery.is_bit_perfect(),
            total_frames: played.total_frames,
        });
        played.outcome
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chilpa_audio::format::SampleFormat;
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    struct Nulo {
        stream: StreamFormat,
        escritos: Arc<AtomicU64>,
    }

    impl AudioSink for Nulo {
        fn format(&self) -> StreamFormat {
            self.stream
        }
        fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
            let n = frames.len() / self.stream.frame_bytes();
            self.escritos.fetch_add(n as u64, Ordering::Relaxed);
            Ok(n)
        }
        fn close(&mut self) -> Result<(), SinkError> {
            Ok(())
        }
    }

    fn wav(nombre: &str, frames: u32) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("chilpa-player-{nombre}.wav"));
        let data = frames * 4;
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(b"RIFF").unwrap();
        f.write_all(&(36 + data).to_le_bytes()).unwrap();
        f.write_all(b"WAVEfmt ").unwrap();
        f.write_all(&16u32.to_le_bytes()).unwrap();
        f.write_all(&1u16.to_le_bytes()).unwrap();
        f.write_all(&2u16.to_le_bytes()).unwrap();
        f.write_all(&44_100u32.to_le_bytes()).unwrap();
        f.write_all(&176_400u32.to_le_bytes()).unwrap();
        f.write_all(&4u16.to_le_bytes()).unwrap();
        f.write_all(&16u16.to_le_bytes()).unwrap();
        f.write_all(b"data").unwrap();
        f.write_all(&data.to_le_bytes()).unwrap();
        for i in 0..frames {
            let v = (i % 1000) as i16;
            f.write_all(&v.to_le_bytes()).unwrap();
            f.write_all(&(-v).to_le_bytes()).unwrap();
        }
        p
    }

    fn player(escritos: Arc<AtomicU64>) -> Player {
        Player::spawn(
            Box::new(|_| {
                Ok(DeviceCaps {
                    sample_rates: vec![44_100],
                    formats: vec![SampleFormat::S16],
                    channels: vec![2],
                    probed: true,
                })
            }),
            Box::new(move |_, stream| {
                Ok(Box::new(Nulo { stream, escritos: Arc::clone(&escritos) }) as Box<dyn AudioSink + Send>)
            }),
        )
    }

    fn esperar<F: Fn(&[PlayerEvent]) -> bool>(p: &Player, hasta: F) -> Vec<PlayerEvent> {
        let limite = Instant::now() + Duration::from_secs(5);
        let mut todos = Vec::new();
        while Instant::now() < limite {
            todos.extend(p.poll());
            if hasta(&todos) {
                return todos;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        todos
    }

    #[test]
    fn reproducir_no_bloquea_y_avisa_progreso_y_final() {
        let escritos = Arc::new(AtomicU64::new(0));
        let p = player(Arc::clone(&escritos));
        let archivo = wav("basico", 5_000);

        let antes = Instant::now();
        p.play(&archivo, "falso", true);
        assert!(antes.elapsed() < Duration::from_millis(100), "play() regresa de inmediato");

        let eventos = esperar(&p, |e| e.iter().any(|x| matches!(x, PlayerEvent::Finished { .. })));
        assert!(eventos.iter().any(|e| matches!(e, PlayerEvent::Started { .. })));
        assert!(eventos.iter().any(|e| matches!(e, PlayerEvent::Progress { .. })));
        assert!(matches!(eventos.last(), Some(PlayerEvent::Finished { frames: 5_000 })));
        assert_eq!(escritos.load(Ordering::Relaxed), 5_000);
        let _ = std::fs::remove_file(archivo);
    }

    #[test]
    fn un_archivo_que_no_existe_avisa_y_el_reproductor_sigue_vivo() {
        let p = player(Arc::new(AtomicU64::new(0)));
        p.play(Path::new("/no/existe.flac"), "falso", true);
        let eventos = esperar(&p, |e| e.iter().any(|x| matches!(x, PlayerEvent::Failed { .. })));
        assert!(matches!(eventos.last(), Some(PlayerEvent::Failed { .. })));

        let archivo = wav("despues-de-fallar", 1_000);
        p.play(&archivo, "falso", true);
        let eventos = esperar(&p, |e| e.iter().any(|x| matches!(x, PlayerEvent::Finished { .. })));
        assert!(eventos.iter().any(|e| matches!(e, PlayerEvent::Finished { .. })), "sigue aceptando órdenes");
        let _ = std::fs::remove_file(archivo);
    }
}
