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

use crate::playback::PlayError;

/// Lo que el reproductor le cuenta a quien lo controla.
#[derive(Clone, PartialEq, Debug)]
pub enum PlayerEvent {
    /// Empezó a sonar: con qué formato se abrió el dispositivo y si la
    /// entrega es bit-perfect.
    Started {
        path: PathBuf,
        opened: StreamFormat,
        bit_perfect: bool,
        total_frames: Option<u64>,
        /// Lo que el archivo dice de sí mismo, cuando se pudo leer.
        tags: Option<chilpa_tags::Tags>,
        /// La portada ya extraída a disco, cuando el archivo trae una.
        artwork: Option<PathBuf>,
        /// Qué lugar ocupa en la cola y de cuántas.
        index: usize,
        queue_len: usize,
        /// Para DSD: el múltiplo que lo nombra (64, 128…). `None` en PCM.
        dsd_multiple: Option<u32>,
    },
    /// Frames entregados hasta ahora.
    Progress { frames: u64 },
    /// Lo que la estación dice que suena.
    StreamTitle { title: String },
    /// Terminó de forma natural.
    Finished { frames: u64 },
    /// Se detuvo porque alguien lo pidió.
    Stopped { frames: u64 },
    /// Algo falló, en un texto que la interfaz puede mostrar.
    Failed { message: String },
}

enum Order {
    Play { paths: Vec<PathBuf>, device: String, bit_perfect: bool },
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
    controls: crate::queue_playback::Controls,
    thread: Option<JoinHandle<()>>,
}

impl Player {
    /// Arranca el hilo del reproductor.
    pub fn spawn(mut probe: ProbeDevice, mut open: OpenSink) -> Self {
        let (orders_tx, orders_rx) = channel::<Order>();
        let (events_tx, events_rx) = channel::<PlayerEvent>();
        let controls = crate::queue_playback::Controls::new();
        let controls_hilo = controls.clone();
        let stop_hilo = controls.stop.clone();
        let pause_hilo = controls.pause.clone();
        let skip_hilo = controls.skip.clone();

        let thread = std::thread::Builder::new()
            .name("chilpa-player".into())
            .spawn(move || {
                while let Ok(order) = orders_rx.recv() {
                    match order {
                        Order::Quit => break,
                        Order::Stop => {}
                        Order::Play { paths, device, bit_perfect } => {
                            // Cada pista arranca con la bandera limpia: una
                            // orden de paro vieja no mata la siguiente.
                            stop_hilo.reset();
                            pause_hilo.set(false);
                            skip_hilo.take();
                            let resultado = reproducir(
                                &paths,
                                &device,
                                bit_perfect,
                                &controls_hilo,
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

        Self { orders: orders_tx, events: events_rx, controls, thread: Some(thread) }
    }

    /// Pide reproducir un archivo. Regresa de inmediato.
    pub fn play(&self, path: &Path, device: &str, bit_perfect: bool) {
        self.play_all(&[path.to_path_buf()], device, bit_perfect);
    }

    /// Pide reproducir varias pistas en orden, encadenadas.
    pub fn play_all(&self, paths: &[PathBuf], device: &str, bit_perfect: bool) {
        let _ = self.orders.send(Order::Play {
            paths: paths.to_vec(),
            device: device.to_string(),
            bit_perfect,
        });
    }

    /// Pausa o reanuda. La pista no pierde su lugar.
    pub fn set_paused(&self, paused: bool) {
        self.controls.pause.set(paused);
    }

    pub fn is_paused(&self) -> bool {
        self.controls.pause.is_paused()
    }

    /// Salta a la siguiente pista de la cola. La actual termina como si se
    /// hubiera acabado, así que la cola avanza en vez de detenerse.
    pub fn next(&self) {
        self.controls.skip.skip();
    }

    /// Pide saltar a ese punto de la pista que suena. El salto se atiende
    /// entre bloques; pedirlo no bloquea.
    pub fn seek(&self, position: std::time::Duration) {
        self.controls.seek.request(position);
    }

    /// Cambia la repetición. Toma efecto al terminar la pista que suena.
    pub fn set_repeat(&self, repeat: crate::queue_playback::Repeat) {
        self.controls.repeat.set(repeat);
    }

    pub fn repeat(&self) -> crate::queue_playback::Repeat {
        self.controls.repeat.get()
    }

    /// Pide parar. No espera al dispositivo.
    pub fn stop(&self) {
        self.controls.stop.stop();
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
        self.controls.stop.stop();
        let _ = self.orders.send(Order::Quit);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn reproducir(
    paths: &[PathBuf],
    device: &str,
    bit_perfect: bool,
    controls: &crate::queue_playback::Controls,
    probe: &mut ProbeDevice,
    open: &mut OpenSink,
    events: &Sender<PlayerEvent>,
) -> Result<Outcome, PlayError> {
    let (stop, pause, skip) = (&controls.stop, &controls.pause, &controls.skip);
    let caps = probe(device).map_err(PlayError::Sink)?;
    let policy = if bit_perfect { Policy::BitPerfect } else { Policy::AllowAnything };
    let total = paths.len();

    // DSD no pasa por el decodificador de PCM: va por DoP, y hoy se
    // reproduce de a una pista.
    if let Some(primera) = paths.first() {
        if es_url(primera) {
            return reproducir_stream(primera, device, &caps, policy, stop, pause, open, events);
        }
        if es_sacd(primera) {
            return reproducir_sacd(primera, device, &caps, stop, pause, skip, open, events);
        }
        if es_dsd(primera) {
            return reproducir_dsd(primera, device, &caps, stop, pause, open, events);
        }
    }
    let resultados = crate::queue_playback::play_queue_with(
        paths,
        &caps,
        policy,
        controls,
        |formato| open(device, formato),
        &mut |indice, ruta, delivery| {
            // Los tags y la portada se leen al arrancar cada pista, no al
            // encolarla: una cola larga no debe leer cientos de archivos
            // antes de que suene el primero.
            let tags = chilpa_tags::read(ruta).ok().map(|info| info.tags);
            let artwork = chilpa_tags::extract_artwork(ruta, &crate::artwork_cache()).ok().flatten();
            let total_frames = chilpa_decode::Decoded::open(ruta).ok().and_then(|d| d.frame_count());
            let _ = events.send(PlayerEvent::Started {
                path: ruta.to_path_buf(),
                opened: delivery.opened,
                bit_perfect: delivery.is_bit_perfect(),
                total_frames,
                tags,
                artwork,
                index: indice,
                queue_len: total,
                dsd_multiple: None,
            });
        },
        &mut |_, frames| {
            let _ = events.send(PlayerEvent::Progress { frames });
        },
    )?;

    // El final de la cola es el final de su última pista.
    Ok(resultados
        .last()
        .map(|r| r.outcome.clone())
        .unwrap_or(Outcome::Finished { frames: 0 }))
}

/// Cierto si lo que se pidió es una URL y no un archivo.
fn es_url(path: &Path) -> bool {
    path.to_string_lossy().starts_with("http://") || path.to_string_lossy().starts_with("https://")
}

/// Reproduce una radio o cualquier stream por HTTP.
#[allow(clippy::too_many_arguments)]
fn reproducir_stream(
    url: &Path,
    device: &str,
    caps: &chilpa_audio::negotiate::DeviceCaps,
    policy: Policy,
    stop: &StopSignal,
    pause: &chilpa_audio::sink::PauseSignal,
    open: &mut OpenSink,
    events: &Sender<PlayerEvent>,
) -> Result<Outcome, PlayError> {
    let direccion = url.to_string_lossy().to_string();
    let stream = chilpa_stream::open(&direccion)
        .map_err(|e| PlayError::Sink(SinkError::Backend(e.to_string())))?;
    let titulos = stream.now_playing();
    let estacion = stream.info().name.clone();

    let ruta = url.to_path_buf();
    let eventos = events.clone();
    let etiqueta = chilpa_tags::Tags {
        title: estacion.clone().or_else(|| Some(direccion.clone())),
        album: Some("Radio".into()),
        ..chilpa_tags::Tags::default()
    };

    let played = crate::playback::play_stream(
        stream,
        caps,
        policy,
        stop,
        pause,
        |formato| open(device, formato),
        &mut |delivery| {
            let _ = eventos.send(PlayerEvent::Started {
                path: ruta.clone(),
                opened: delivery.opened,
                bit_perfect: delivery.is_bit_perfect(),
                total_frames: None,
                tags: Some(etiqueta.clone()),
                artwork: None,
                index: 0,
                queue_len: 1,
                dsd_multiple: None,
            });
        },
        &mut |frames| {
            // El título de la estación cambia con cada canción; se publica
            // con el progreso, que es lo único que viaja seguido.
            if let Some(titulo) = titulos.get() {
                let _ = events.send(PlayerEvent::StreamTitle { title: titulo });
            }
            let _ = events.send(PlayerEvent::Progress { frames });
        },
    )?;
    Ok(played.outcome)
}

/// Cierto si el archivo es una imagen de SACD.
fn es_sacd(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("iso"))
        .unwrap_or(false)
}

/// Reproduce las pistas del área estéreo de una imagen de SACD, en orden.
#[allow(clippy::too_many_arguments)]
fn reproducir_sacd(
    path: &Path,
    device: &str,
    caps: &chilpa_audio::negotiate::DeviceCaps,
    stop: &StopSignal,
    pause: &chilpa_audio::sink::PauseSignal,
    skip: &chilpa_audio::sink::SkipSignal,
    open: &mut OpenSink,
    events: &Sender<PlayerEvent>,
) -> Result<Outcome, PlayError> {
    let imagen = chilpa_sacd::open(path)
        .map_err(|e| PlayError::Sink(SinkError::Backend(e.to_string())))?;
    let pistas = imagen.area.tracks.clone();
    if pistas.is_empty() {
        return Err(PlayError::Sink(SinkError::Backend("el disco no trae pistas".into())));
    }
    let total = pistas.len();
    let mut ultimo = Outcome::Finished { frames: 0 };

    for (indice, pista) in pistas.iter().enumerate() {
        if stop.is_stopped() {
            return Ok(Outcome::Stopped { frames: ultimo.frames() });
        }
        skip.take();
        // Cada pista abre la imagen otra vez: el lector se queda con ella, y
        // así una pista que falla no se lleva el resto del disco.
        let imagen = chilpa_sacd::open(path)
            .map_err(|e| PlayError::Sink(SinkError::Backend(e.to_string())))?;
        let mut lector = chilpa_sacd::SacdTrackReader::open(imagen, pista)
            .map_err(|e| PlayError::Sink(SinkError::Backend(e.to_string())))?;

        let etiqueta = chilpa_tags::Tags {
            title: Some(format!("Pista {}", pista.number)),
            album: path.file_stem().map(|s| s.to_string_lossy().to_string()),
            ..chilpa_tags::Tags::default()
        };
        let eventos = events.clone();
        let ruta = path.to_path_buf();
        let duracion = Some(u64::from(pista.duration_frames) * u64::from(chilpa_sacd::DSD64_HZ / 75) / 16);

        let played = crate::dsd_playback::play_dsd(
            &mut lector,
            caps,
            stop,
            pause,
            |formato| {
                let sink = open(device, formato);
                if sink.is_ok() {
                    let _ = eventos.send(PlayerEvent::Started {
                        path: ruta.clone(),
                        opened: formato,
                        bit_perfect: true,
                        total_frames: duracion,
                        tags: Some(etiqueta.clone()),
                        artwork: None,
                        index: indice,
                        queue_len: total,
                        dsd_multiple: Some(64),
                    });
                }
                sink
            },
            &mut |frames| {
                let _ = events.send(PlayerEvent::Progress { frames });
            },
        )
        .map_err(|e| PlayError::Sink(SinkError::Backend(e.to_string())))?;

        ultimo = played.outcome;
        if !ultimo.may_advance_queue() {
            break;
        }
    }
    Ok(ultimo)
}

/// Cierto si el archivo es DSD por su extensión.
fn es_dsd(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("dsf"))
        .unwrap_or(false)
}

/// Reproduce un archivo DSD y cuenta lo mismo que la ruta de PCM.
#[allow(clippy::too_many_arguments)]
fn reproducir_dsd(
    path: &Path,
    device: &str,
    caps: &chilpa_audio::negotiate::DeviceCaps,
    stop: &StopSignal,
    pause: &chilpa_audio::sink::PauseSignal,
    open: &mut OpenSink,
    events: &Sender<PlayerEvent>,
) -> Result<Outcome, PlayError> {
    let tags = chilpa_tags::read(path).ok().map(|info| info.tags);
    let artwork = chilpa_tags::extract_artwork(path, &crate::artwork_cache()).ok().flatten();
    use chilpa_dsd::DsdSource as _;
    let info = chilpa_dsd::DsfReader::open(path)
        .map(|r| r.info().clone())
        .map_err(|e| PlayError::Sink(SinkError::Backend(e.to_string())))?;

    let ruta = path.to_path_buf();
    let eventos = events.clone();
    let multiplo = info.multiple();
    let total_frames = Some(info.samples / 16);

    let played = crate::dsd_playback::play_dsf(
        path,
        caps,
        stop,
        pause,
        |formato| {
            let sink = open(device, formato);
            if let Ok(_) = &sink {
                let _ = eventos.send(PlayerEvent::Started {
                    path: ruta.clone(),
                    opened: formato,
                    bit_perfect: true,
                    total_frames,
                    tags: tags.clone(),
                    artwork: artwork.clone(),
                    index: 0,
                    queue_len: 1,
                    dsd_multiple: Some(multiplo),
                });
            }
            sink
        },
        &mut |frames| {
            let _ = events.send(PlayerEvent::Progress { frames });
        },
    )
    .map_err(|e| PlayError::Sink(SinkError::Backend(e.to_string())))?;

    Ok(played.outcome)
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
    fn una_cola_suena_en_orden_y_avisa_el_lugar_de_cada_pista() {
        let escritos = Arc::new(AtomicU64::new(0));
        let p = player(Arc::clone(&escritos));
        let a = wav("cola-a", 1_000);
        let b = wav("cola-b", 2_000);
        p.play_all(&[a.clone(), b.clone()], "falso", true);

        let eventos = esperar(&p, |e| e.iter().any(|x| matches!(x, PlayerEvent::Finished { .. })));
        let arranques: Vec<(usize, usize)> = eventos
            .iter()
            .filter_map(|e| match e {
                PlayerEvent::Started { index, queue_len, .. } => Some((*index, *queue_len)),
                _ => None,
            })
            .collect();
        assert_eq!(arranques, vec![(0, 2), (1, 2)], "las dos pistas, en orden");
        assert_eq!(escritos.load(Ordering::Relaxed), 3_000, "salieron todos los frames");
        let _ = std::fs::remove_file(a);
        let _ = std::fs::remove_file(b);
    }

    #[test]
    fn saltar_avanza_la_cola_en_vez_de_detenerla() {
        let escritos = Arc::new(AtomicU64::new(0));
        let p = player(Arc::clone(&escritos));
        // Pistas largas: sin saltar, la prueba tardaría en llegar al final.
        let a = wav("saltar-a", 400_000);
        let b = wav("saltar-b", 1_000);
        p.play_all(&[a.clone(), b.clone()], "falso", true);

        // En cuanto arranca la primera, se pide saltar. Los eventos se
        // juntan de las dos esperas: cada una consume lo que ya recogió.
        let mut eventos =
            esperar(&p, |e| e.iter().any(|x| matches!(x, PlayerEvent::Started { .. })));
        p.next();
        eventos.extend(esperar(&p, |e| {
            e.iter().any(|x| matches!(x, PlayerEvent::Finished { .. }))
        }));

        let arranques = eventos
            .iter()
            .filter(|e| matches!(e, PlayerEvent::Started { .. }))
            .count();
        assert!(arranques >= 2, "la segunda pista arrancó: {arranques} arranques");
        assert!(
            escritos.load(Ordering::Relaxed) < 401_000,
            "no sonó la primera entera"
        );
        let _ = std::fs::remove_file(a);
        let _ = std::fs::remove_file(b);
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
