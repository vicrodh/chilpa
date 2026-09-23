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
use chilpa_audio::sink::{AudioSink, PauseSignal, SinkError, SkipSignal, StopSignal};
use chilpa_decode::{Block, Decoded};
use std::path::{Path, PathBuf};

use crate::playback::PlayError;

/// Qué hacer al llegar al final de la cola.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Repeat {
    /// Se acaba la cola y se para.
    #[default]
    Off = 0,
    /// Vuelve a empezar desde la primera.
    All = 1,
    /// Repite la pista que suena. Saltar de pista sí avanza: repetir no es
    /// quedarse atorado.
    One = 2,
}

/// La repetición, compartida con quien maneja la cola.
///
/// Se puede cambiar mientras algo suena y se atiende al terminar la pista:
/// cortar el audio para cambiar de modo no tendría sentido.
#[derive(Clone, Default)]
pub struct RepeatMode(std::sync::Arc<std::sync::atomic::AtomicU8>);

impl RepeatMode {
    pub fn new(modo: Repeat) -> Self {
        let m = Self::default();
        m.set(modo);
        m
    }

    pub fn set(&self, modo: Repeat) {
        self.0.store(modo as u8, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn get(&self) -> Repeat {
        match self.0.load(std::sync::atomic::Ordering::Relaxed) {
            1 => Repeat::All,
            2 => Repeat::One,
            _ => Repeat::Off,
        }
    }
}

/// El orden en que se recorre la cola, compartido.
///
/// Se puede cambiar mientras algo suena: al terminar la pista se busca dónde
/// quedó en el orden nuevo y se sigue desde ahí, sin cortar el audio. Vacío
/// quiere decir el orden del arreglo.
#[derive(Clone, Default)]
pub struct Order(std::sync::Arc<std::sync::Mutex<Vec<usize>>>);

impl Order {
    pub fn new() -> Self {
        Self::default()
    }

    /// Pone un orden. Uno que no sea una permutación de `0..len` se ignora:
    /// un orden a medias dejaría pistas sin sonar o sonando dos veces.
    pub fn set(&self, orden: Vec<usize>, len: usize) -> bool {
        if !es_permutacion(&orden, len) {
            return false;
        }
        if let Ok(mut g) = self.0.lock() {
            *g = orden;
            return true;
        }
        false
    }

    /// Vuelve al orden del arreglo.
    pub fn clear(&self) {
        if let Ok(mut g) = self.0.lock() {
            g.clear();
        }
    }

    pub fn get(&self) -> Vec<usize> {
        self.0.lock().map(|g| g.clone()).unwrap_or_default()
    }

    /// Qué pista sigue después de `actual`, o `None` si ahí se acaba.
    fn siguiente(&self, actual: usize, len: usize, vuelta: bool) -> Option<usize> {
        let orden = self.get();
        if orden.len() != len {
            // Sin orden propio, la cola va como viene.
            let n = actual + 1;
            return if n < len {
                Some(n)
            } else if vuelta {
                Some(0)
            } else {
                None
            };
        }
        // Se busca dónde quedó la pista actual: si el orden cambió a media
        // reproducción, se sigue desde su lugar nuevo.
        let pos = orden.iter().position(|i| *i == actual)?;
        match orden.get(pos + 1) {
            Some(n) => Some(*n),
            None if vuelta => orden.first().copied(),
            None => None,
        }
    }

    /// Por dónde empieza.
    fn primera(&self, len: usize) -> usize {
        let orden = self.get();
        if orden.len() == len { orden.first().copied().unwrap_or(0) } else { 0 }
    }
}

fn es_permutacion(orden: &[usize], len: usize) -> bool {
    if orden.len() != len {
        return false;
    }
    let mut vistos = vec![false; len];
    for i in orden {
        match vistos.get_mut(*i) {
            Some(v) if !*v => *v = true,
            _ => return false,
        }
    }
    true
}

/// Las señales con las que se maneja una cola mientras suena.
///
/// Van juntas porque siempre se usan juntas, y porque todas se miran entre
/// bloques o entre pistas: ninguna bloquea esperando al dispositivo.
#[derive(Clone, Default)]
pub struct Controls {
    pub stop: StopSignal,
    pub pause: PauseSignal,
    pub seek: SeekSignal,
    pub skip: SkipSignal,
    pub repeat: RepeatMode,
    pub order: Order,
}

impl Controls {
    pub fn new() -> Self {
        Self::default()
    }
}

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
    /// Dónde se pide saltar de pista. Se atiende terminando la fuente, que
    /// es lo que hace que la cola avance en lugar de detenerse.
    skip: SkipSignal,
    /// Cierto si esta pista terminó porque alguien saltó, no porque se
    /// acabara.
    salto: bool,
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
            if self.skip.take() {
                self.salto = true;
                self.agotado = true;
                self.trimmer.finish();
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
    let controls = Controls { stop: stop.clone(), ..Controls::new() };
    play_queue_with(
        paths,
        caps,
        policy,
        &controls,
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
    controls: &Controls,
    mut open_sink: F,
    on_track: &mut dyn FnMut(usize, &Path, &Delivery),
    on_progress: &mut dyn FnMut(usize, u64),
) -> Result<Vec<TrackResult>, PlayError>
where
    F: FnMut(StreamFormat) -> Result<K, SinkError>,
    K: AudioSink,
{
    let Controls { stop, pause, seek, skip, repeat, order } = controls;
    let mut resultados = Vec::new();
    let mut abierto: Option<(StreamFormat, K)> = None;
    let mut indice = order.primera(paths.len());
    // Un archivo que no entrega un solo frame y una repetición encendida
    // serían un lazo que gira para siempre. Se cuenta cuántas pistas
    // seguidas no sonaron y se corta al dar una vuelta completa.
    let mut mudas_seguidas = 0usize;

    while indice < paths.len() {
        let path = &paths[indice];
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
            skip: skip.clone(),
            salto: false,
            descartar: 0,
            base: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
        };
        let base = std::sync::Arc::clone(&frames.base);
        on_track(indice, path, &delivery);
        let outcome =
            chilpa_audio::render::render_paused(&mut frames, sink, stop, pause, &mut |f| {
                on_progress(indice, f + base.load(std::sync::atomic::Ordering::Relaxed))
            });
        let termino_bien = outcome.may_advance_queue();
        mudas_seguidas = if outcome.frames() == 0 { mudas_seguidas + 1 } else { 0 };
        // Saltar de pista termina la fuente igual que un final natural, así
        // que hay que preguntarle: con repeat-one, saltar avanza.
        let salto = frames.salto;
        resultados.push(TrackResult {
            path: path.clone(),
            delivery,
            outcome,
            transition: paso,
        });
        if !termino_bien || mudas_seguidas > paths.len() {
            break;
        }
        let modo = repeat.get();
        if modo == Repeat::One && !salto {
            continue;
        }
        match order.siguiente(indice, paths.len(), modo == Repeat::All) {
            Some(n) => indice = n,
            None => break,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sin_orden_propio_la_cola_va_como_viene() {
        let o = Order::new();
        assert_eq!(o.primera(3), 0);
        assert_eq!(o.siguiente(0, 3, false), Some(1));
        assert_eq!(o.siguiente(2, 3, false), None, "ahí se acaba");
        assert_eq!(o.siguiente(2, 3, true), Some(0), "con vuelta, vuelve a la primera");
    }

    #[test]
    fn un_orden_propio_manda() {
        let o = Order::new();
        assert!(o.set(vec![2, 0, 1], 3));
        assert_eq!(o.primera(3), 2);
        assert_eq!(o.siguiente(2, 3, false), Some(0));
        assert_eq!(o.siguiente(1, 3, false), None, "la 1 es la última del orden");
        assert_eq!(o.siguiente(1, 3, true), Some(2));
    }

    #[test]
    fn cambiar_el_orden_a_media_cola_sigue_desde_donde_quedo() {
        let o = Order::new();
        assert!(o.set(vec![0, 1, 2, 3], 4));
        // Suena la 1 y se reordena: lo que sigue es lo que la siga ahora.
        assert!(o.set(vec![3, 1, 0, 2], 4));
        assert_eq!(o.siguiente(1, 4, false), Some(0));
    }

    #[test]
    fn un_orden_que_no_es_permutacion_se_rechaza() {
        let o = Order::new();
        assert!(!o.set(vec![0, 0, 1], 3), "repetida");
        assert!(!o.set(vec![0, 1], 3), "incompleta");
        assert!(!o.set(vec![0, 1, 5], 3), "fuera de rango");
        assert!(o.get().is_empty(), "nada de eso se guardó");
    }

    #[test]
    fn limpiar_devuelve_el_orden_del_arreglo() {
        let o = Order::new();
        assert!(o.set(vec![1, 0], 2));
        o.clear();
        assert_eq!(o.primera(2), 0);
    }
}
