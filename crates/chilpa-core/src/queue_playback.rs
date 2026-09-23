//! Reproducir varias pistas seguidas.
//!
//! La cola encadena sin hueco mientras el formato del stream no cambie. Si
//! cambia, hay que reabrir el dispositivo y el corte es inevitable: se
//! reporta, no se disimula.
//!
//! Cada pista termina con su propio [`Outcome`], y solo un fin natural deja
//! pasar a la siguiente. Una falla del dispositivo detiene la cola ahí mismo,
//! que es justo lo que QBZ no hacía: se saltaba todo en silencio.

use chilpa_audio::format::StreamFormat;
use chilpa_audio::gapless::{transition, Transition, Trimmer};
use chilpa_audio::negotiate::{negotiate, DeviceCaps, Delivery, Policy};
use chilpa_audio::pack::{pack, Samples};
use chilpa_audio::render::{FrameSource, Outcome};
use chilpa_audio::seek::SeekSignal;
use chilpa_audio::sink::{AudioSink, SinkError, StopSignal};
use chilpa_decode::{Block, Decoded};
use std::path::{Path, PathBuf};

use crate::playback::PlayError;

/// Cómo terminó una pista de la cola.
pub struct TrackResult {
    pub path: PathBuf,
    pub delivery: Delivery,
    pub outcome: Outcome,
    /// Cómo se llegó a esta pista desde la anterior.
    pub transition: Transition,
}

impl TrackResult {
    pub fn frames(&self) -> u64 {
        self.outcome.frames()
    }
}

/// Fuente de una pista, con su recorte de relleno aplicado.
struct TrackFrames<'a> {
    decoded: &'a mut Decoded,
    opened: StreamFormat,
    trimmer: Trimmer,
    agotado: bool,
    /// Dónde se piden los saltos. Se atienden entre bloques, nunca a media
    /// escritura.
    seek: SeekSignal,
    /// Frames que hay que tirar para caer en el punto exacto pedido.
    descartar: u64,
    /// Dónde cayó el último salto. La posición que ve la interfaz cuenta
    /// desde el archivo, así que a los frames entregados se les suma esto.
    base: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

impl FrameSource for TrackFrames<'_> {
    fn next_block(&mut self) -> Option<Vec<u8>> {
        loop {
            if self.agotado {
                return None;
            }
            if let Some(destino) = self.seek.take() {
                let frame = chilpa_audio::seek::frames_at(destino, self.opened.sample_rate_hz);
                match self.decoded.seek_to_frame(frame) {
                    Ok(plan) => {
                        self.descartar = plan.discard;
                        // La base es el punto PEDIDO, no donde aterrizó el
                        // contenedor: los frames que se descartan no salen,
                        // así que sumarlos aparte dejaría la posición corta.
                        self.base.store(frame, std::sync::atomic::Ordering::Relaxed);
                    }
                    // Un contenedor que no sabe saltar sigue sonando desde
                    // donde iba, en vez de tirar la pista.
                    Err(_) => self.descartar = 0,
                }
            }
            let recortado = match self.decoded.next_block() {
                Ok(Some(Block::Int { data, bits })) => {
                    let canales = self.opened.channels.max(1) as usize;
                    let data = self.tirar_descarte(data, canales);
                    if data.is_empty() {
                        continue;
                    }
                    let frames = self.trimmer.push(&data);
                    if frames.is_empty() {
                        continue;
                    }
                    let mut bytes = Vec::new();
                    let samples = Samples::Int { data: &frames, bits };
                    pack(&samples, self.opened.sample_format, &mut bytes).ok()?;
                    bytes
                }
                Ok(Some(Block::Float(data))) => {
                    let mut bytes = Vec::new();
                    pack(&Samples::Float(&data), self.opened.sample_format, &mut bytes).ok()?;
                    bytes
                }
                Ok(None) => {
                    // Lo que quedó retenido era relleno del codificador.
                    self.agotado = true;
                    self.trimmer.finish();
                    return None;
                }
                Err(_) => return None,
            };
            if !recortado.is_empty() {
                return Some(recortado);
            }
        }
    }
}

impl TrackFrames<'_> {
    /// Tira los frames que sobran de un salto, que pueden abarcar varios
    /// bloques.
    fn tirar_descarte(&mut self, data: Vec<i32>, canales: usize) -> Vec<i32> {
        if self.descartar == 0 {
            return data;
        }
        let frames = (data.len() / canales) as u64;
        if self.descartar >= frames {
            self.descartar -= frames;
            return Vec::new();
        }
        let corte = self.descartar as usize * canales;
        self.descartar = 0;
        data[corte..].to_vec()
    }
}

/// Reproduce las pistas en orden.
///
/// `open_sink` abre el dispositivo con el formato que se le pase. Se le llama
/// una vez por cada renegociación, no una vez por pista.
pub fn play_queue<F, K>(
    paths: &[PathBuf],
    caps: &DeviceCaps,
    policy: Policy,
    stop: &StopSignal,
    open_sink: F,
) -> Result<Vec<TrackResult>, PlayError>
where
    F: FnMut(StreamFormat) -> Result<K, SinkError>,
    K: AudioSink,
{
    play_queue_with(
        paths,
        caps,
        policy,
        stop,
        &SeekSignal::new(),
        open_sink,
        &mut |_, _, _| {},
        &mut |_, _| {},
    )
}

/// Lo mismo, avisando cuándo arranca cada pista y cuántos frames lleva.
///
/// `on_track` recibe el índice, la ruta y cómo quedó la entrega; `on_progress`
/// el índice y los frames entregados de esa pista.
#[allow(clippy::too_many_arguments)]
pub fn play_queue_with<F, K>(
    paths: &[PathBuf],
    caps: &DeviceCaps,
    policy: Policy,
    stop: &StopSignal,
    seek: &SeekSignal,
    mut open_sink: F,
    on_track: &mut dyn FnMut(usize, &Path, &Delivery),
    on_progress: &mut dyn FnMut(usize, u64),
) -> Result<Vec<TrackResult>, PlayError>
where
    F: FnMut(StreamFormat) -> Result<K, SinkError>,
    K: AudioSink,
{
    let mut resultados = Vec::new();
    let mut abierto: Option<(StreamFormat, K)> = None;

    for (indice, path) in paths.iter().enumerate() {
        let mut decoded = Decoded::open(path).map_err(PlayError::Decode)?;
        let delivery = negotiate(decoded.stream_format(), caps, policy)
            .map_err(PlayError::Negotiation)?;

        let paso = match &abierto {
            Some((actual, _)) => transition(*actual, delivery.opened),
            None => Transition::Seamless,
        };
        if !paso.is_seamless() || abierto.is_none() {
            if let Some((_, mut viejo)) = abierto.take() {
                viejo.close().map_err(PlayError::Sink)?;
            }
            let nuevo = open_sink(delivery.opened).map_err(PlayError::Sink)?;
            abierto = Some((delivery.opened, nuevo));
        }
        let (_, sink) = abierto.as_mut().expect("recién abierto");

        let trimmer = Trimmer::new(
            decoded.encoder_delay(),
            decoded.encoder_padding(),
            delivery.opened.channels,
        );
        let mut frames = TrackFrames {
            decoded: &mut decoded,
            opened: delivery.opened,
            trimmer,
            agotado: false,
            seek: seek.clone(),
            descartar: 0,
            base: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
        };
        let base = std::sync::Arc::clone(&frames.base);
        on_track(indice, path, &delivery);
        let outcome = chilpa_audio::render::render_with(&mut frames, sink, stop, &mut |f| {
            on_progress(indice, f + base.load(std::sync::atomic::Ordering::Relaxed))
        });
        let termino_bien = outcome.may_advance_queue();
        resultados.push(TrackResult {
            path: path.clone(),
            delivery,
            outcome,
            transition: paso,
        });
        if !termino_bien {
            break;
        }
    }

    if let Some((_, mut sink)) = abierto.take() {
        sink.close().map_err(PlayError::Sink)?;
    }
    Ok(resultados)
}

/// Cuántas veces se reabrió el dispositivo en una cola ya reproducida.
pub fn renegotiations(results: &[TrackResult]) -> usize {
    results.iter().filter(|r| !r.transition.is_seamless()).count()
}

/// Las pistas que sí sonaron completas.
pub fn finished(results: &[TrackResult]) -> impl Iterator<Item = &Path> {
    results
        .iter()
        .filter(|r| r.outcome.may_advance_queue())
        .map(|r| r.path.as_path())
}
