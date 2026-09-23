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
    pub fn new(mode: Repeat) -> Self {
        let m = Self::default();
        m.set(mode);
        m
    }

    pub fn set(&self, mode: Repeat) {
        self.0.store(mode as u8, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn get(&self) -> Repeat {
        match self.0.load(std::sync::atomic::Ordering::Relaxed) {
            1 => Repeat::All,
            2 => Repeat::One,
            _ => Repeat::Off,
        }
    }
}

/// La lista de pistas, compartida con quien maneja la cola.
///
/// Se puede agregar mientras suena: encolar algo no debería cortar lo que
/// está sonando ni obligar a rearmar la cola desde el principio. El lazo
/// vuelve a leer el largo en cada pista, así que lo agregado entra solo.
#[derive(Clone, Default)]
pub struct SharedQueue(std::sync::Arc<std::sync::Mutex<Vec<Entry>>>);

/// Una pista de la cola. Quitarla no la saca del arreglo: se marca, y así
/// las posiciones de las demás no se mueven bajo los pies de quien está
/// reproduciendo.
#[derive(Clone, PartialEq, Eq, Debug)]
struct Entry {
    path: PathBuf,
    removed: bool,
}

impl SharedQueue {
    pub fn new(paths: Vec<PathBuf>) -> Self {
        Self(std::sync::Arc::new(std::sync::Mutex::new(entries(paths))))
    }

    pub fn set(&self, paths: Vec<PathBuf>) {
        if let Ok(mut g) = self.0.lock() {
            *g = entries(paths);
        }
    }

    /// Agrega al final. Regresa el largo nuevo.
    pub fn extend(&self, paths: &[PathBuf]) -> usize {
        match self.0.lock() {
            Ok(mut g) => {
                g.extend(entries(paths.to_vec()));
                g.len()
            }
            Err(_) => 0,
        }
    }

    /// Cuántas posiciones tiene, incluyendo las quitadas.
    pub fn len(&self) -> usize {
        self.0.lock().map(|g| g.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Quita una pista de la cola. La posición se conserva vacía.
    pub fn remove(&self, i: usize) -> bool {
        match self.0.lock() {
            Ok(mut g) => match g.get_mut(i) {
                Some(e) if !e.removed => {
                    e.removed = true;
                    true
                }
                _ => false,
            },
            Err(_) => false,
        }
    }

    /// Cierto si esa posición ya se quitó.
    pub fn is_removed(&self, i: usize) -> bool {
        self.0.lock().ok().and_then(|g| g.get(i).map(|e| e.removed)).unwrap_or(false)
    }

    /// La pista de esa posición, o `None` si no existe o si se quitó.
    pub fn get(&self, i: usize) -> Option<PathBuf> {
        let g = self.0.lock().ok()?;
        let e = g.get(i)?;
        (!e.removed).then(|| e.path.clone())
    }

    /// Todas las rutas, incluyendo las quitadas, para que quien muestre la
    /// cola sepa de qué posición habla.
    pub fn to_vec(&self) -> Vec<PathBuf> {
        self.0.lock().map(|g| g.iter().map(|e| e.path.clone()).collect()).unwrap_or_default()
    }
}

fn entries(paths: Vec<PathBuf>) -> Vec<Entry> {
    paths.into_iter().map(|path| Entry { path, removed: false }).collect()
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
    pub fn set(&self, order: Vec<usize>, len: usize) -> bool {
        if !is_permutation(&order, len) {
            return false;
        }
        if let Ok(mut g) = self.0.lock() {
            *g = order;
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
    fn next(&self, actual: usize, len: usize, wrap: bool) -> Option<usize> {
        let order = self.get();
        if order.len() != len {
            // Sin orden propio, la cola va como viene.
            let n = actual + 1;
            return if n < len {
                Some(n)
            } else if wrap {
                Some(0)
            } else {
                None
            };
        }
        // Se busca dónde quedó la pista actual: si el orden cambió a media
        // reproducción, se sigue desde su lugar nuevo.
        let pos = order.iter().position(|i| *i == actual)?;
        match order.get(pos + 1) {
            Some(n) => Some(*n),
            None if wrap => order.first().copied(),
            None => None,
        }
    }

    /// Por dónde empieza.
    fn first(&self, len: usize) -> usize {
        let order = self.get();
        if order.len() == len { order.first().copied().unwrap_or(0) } else { 0 }
    }
}

/// Una permutación al azar de `0..len`.
///
/// El generador es un xorshift de tres líneas: para barajar una cola no hace
/// falta nada mejor, y una dependencia más es una dependencia más.
pub fn shuffled(len: usize, seed: u64) -> Vec<usize> {
    let mut order: Vec<usize> = (0..len).collect();
    let mut x = seed | 1;
    // Fisher-Yates, de atrás hacia adelante: cada permutación sale con la
    // misma probabilidad.
    for i in (1..len).rev() {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        order.swap(i, (x % (i as u64 + 1)) as usize);
    }
    order
}

/// Lo mismo, sembrado con el reloj.
pub fn shuffled_now(len: usize) -> Vec<usize> {
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x2545_F491_4F6C_DD1D);
    shuffled(len, seed)
}

fn is_permutation(order: &[usize], len: usize) -> bool {
    if order.len() != len {
        return false;
    }
    let mut seen = vec![false; len];
    for i in order {
        match seen.get_mut(*i) {
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
    exhausted: bool,
    /// Dónde se piden los saltos. Se atienden entre bloques, nunca a media
    /// escritura.
    seek: SeekSignal,
    /// Dónde se pide saltar de pista. Se atiende terminando la fuente, que
    /// es lo que hace que la cola avance en lugar de detenerse.
    skip: SkipSignal,
    /// Cierto si esta pista terminó porque alguien saltó, no porque se
    /// acabara.
    skipped: bool,
    /// Frames que hay que tirar para caer en el punto exacto pedido.
    discard: u64,
    /// Dónde cayó el último salto. La posición que ve la interfaz cuenta
    /// desde el archivo, así que a los frames entregados se les suma esto.
    base: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

impl FrameSource for TrackFrames<'_> {
    fn next_block(&mut self) -> Option<Vec<u8>> {
        loop {
            if self.exhausted {
                return None;
            }
            if self.skip.take() {
                self.skipped = true;
                self.exhausted = true;
                self.trimmer.finish();
                return None;
            }
            if let Some(target) = self.seek.take() {
                let frame = chilpa_audio::seek::frames_at(target, self.opened.sample_rate_hz);
                match self.decoded.seek_to_frame(frame) {
                    Ok(plan) => {
                        self.discard = plan.discard;
                        // La base es el punto PEDIDO, no donde aterrizó el
                        // contenedor: los frames que se descartan no salen,
                        // así que sumarlos aparte dejaría la posición corta.
                        self.base.store(frame, std::sync::atomic::Ordering::Relaxed);
                    }
                    // Un contenedor que no sabe saltar sigue sonando desde
                    // donde iba, en vez de tirar la pista.
                    Err(_) => self.discard = 0,
                }
            }
            let clipped = match self.decoded.next_block() {
                Ok(Some(Block::Int { data, bits })) => {
                    let channels = self.opened.channels.max(1) as usize;
                    let data = self.drop_discard(data, channels);
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
                    self.exhausted = true;
                    self.trimmer.finish();
                    return None;
                }
                Err(_) => return None,
            };
            if !clipped.is_empty() {
                return Some(clipped);
            }
        }
    }
}

impl TrackFrames<'_> {
    /// Tira los frames que sobran de un salto, que pueden abarcar varios
    /// bloques.
    fn drop_discard(&mut self, data: Vec<i32>, channels: usize) -> Vec<i32> {
        if self.discard == 0 {
            return data;
        }
        let frames = (data.len() / channels) as u64;
        if self.discard >= frames {
            self.discard -= frames;
            return Vec::new();
        }
        let cut = self.discard as usize * channels;
        self.discard = 0;
        data[cut..].to_vec()
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
        &SharedQueue::new(paths.to_vec()),
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
    queue: &SharedQueue,
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
    let mut results = Vec::new();
    let mut opened: Option<(StreamFormat, K)> = None;
    let mut index = order.first(queue.len());
    // Un archivo que no entrega un solo frame y una repetición encendida
    // serían un lazo que gira para siempre. Se cuenta cuántas pistas
    // seguidas no sonaron y se corta al dar una vuelta completa.
    let mut consecutive_silent = 0usize;

    // El largo se vuelve a leer en cada vuelta: la cola puede crecer
    // mientras suena.
    loop {
        // Una pista quitada se salta sin sonar; si ya no queda ninguna, se
        // acabó la cola.
        let Some(path) = next_live(queue, order, repeat, &mut index) else { break };
        let path = &path;
        let mut decoded = Decoded::open(path).map_err(PlayError::Decode)?;
        let delivery = negotiate(decoded.stream_format(), caps, policy)
            .map_err(PlayError::Negotiation)?;

        let paso = match &opened {
            Some((actual, _)) => transition(*actual, delivery.opened),
            None => Transition::Seamless,
        };
        if !paso.is_seamless() || opened.is_none() {
            if let Some((_, mut old)) = opened.take() {
                old.close().map_err(PlayError::Sink)?;
            }
            let new = open_sink(delivery.opened).map_err(PlayError::Sink)?;
            opened = Some((delivery.opened, new));
        }
        let (_, sink) = opened.as_mut().expect("recién abierto");

        let trimmer = Trimmer::new(
            decoded.encoder_delay(),
            decoded.encoder_padding(),
            delivery.opened.channels,
        );
        let mut frames = TrackFrames {
            decoded: &mut decoded,
            opened: delivery.opened,
            trimmer,
            exhausted: false,
            seek: seek.clone(),
            skip: skip.clone(),
            skipped: false,
            discard: 0,
            base: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
        };
        let base = std::sync::Arc::clone(&frames.base);
        on_track(index, path, &delivery);
        let outcome =
            chilpa_audio::render::render_paused(&mut frames, sink, stop, pause, &mut |f| {
                on_progress(index, f + base.load(std::sync::atomic::Ordering::Relaxed))
            });
        let finished_ok = outcome.may_advance_queue();
        consecutive_silent = if outcome.frames() == 0 { consecutive_silent + 1 } else { 0 };
        // Saltar de pista termina la fuente igual que un final natural, así
        // que hay que preguntarle: con repeat-one, saltar avanza.
        let skipped = frames.skipped;
        results.push(TrackResult {
            path: path.clone(),
            delivery,
            outcome,
            transition: paso,
        });
        if !finished_ok || consecutive_silent > queue.len() {
            break;
        }
        let mode = repeat.get();
        if mode == Repeat::One && !skipped {
            continue;
        }
        match order.next(index, queue.len(), mode == Repeat::All) {
            Some(n) => index = n,
            None => break,
        }
    }

    if let Some((_, mut sink)) = opened.take() {
        sink.close().map_err(PlayError::Sink)?;
    }
    Ok(results)
}

/// La pista de `indice`, saltándose las que se quitaron.
///
/// Avanza `indice` hasta dar con una viva. Devuelve `None` cuando ya no
/// queda ninguna, contando las vueltas para no girar para siempre si se
/// quitó la cola entera con la repetición encendida.
fn next_live(
    queue: &SharedQueue,
    order: &Order,
    repeat: &RepeatMode,
    index: &mut usize,
) -> Option<PathBuf> {
    let wrap = repeat.get() == Repeat::All;
    for _ in 0..=queue.len() {
        if let Some(p) = queue.get(*index) {
            return Some(p);
        }
        if *index >= queue.len() {
            return None;
        }
        *index = order.next(*index, queue.len(), wrap)?;
    }
    None
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
    fn without_an_order_of_its_own_the_queue_plays_as_it_came() {
        let o = Order::new();
        assert_eq!(o.first(3), 0);
        assert_eq!(o.next(0, 3, false), Some(1));
        assert_eq!(o.next(2, 3, false), None, "ahí se acaba");
        assert_eq!(o.next(2, 3, true), Some(0), "con vuelta, vuelve a la primera");
    }

    #[test]
    fn an_explicit_order_wins() {
        let o = Order::new();
        assert!(o.set(vec![2, 0, 1], 3));
        assert_eq!(o.first(3), 2);
        assert_eq!(o.next(2, 3, false), Some(0));
        assert_eq!(o.next(1, 3, false), None, "la 1 es la última del orden");
        assert_eq!(o.next(1, 3, true), Some(2));
    }

    #[test]
    fn changing_the_order_mid_queue_continues_from_where_it_was() {
        let o = Order::new();
        assert!(o.set(vec![0, 1, 2, 3], 4));
        // Suena la 1 y se reordena: lo que sigue es lo que la siga ahora.
        assert!(o.set(vec![3, 1, 0, 2], 4));
        assert_eq!(o.next(1, 4, false), Some(0));
    }

    #[test]
    fn an_order_that_is_not_a_permutation_is_refused() {
        let o = Order::new();
        assert!(!o.set(vec![0, 0, 1], 3), "repetida");
        assert!(!o.set(vec![0, 1], 3), "incompleta");
        assert!(!o.set(vec![0, 1, 5], 3), "fuera de rango");
        assert!(o.get().is_empty(), "nada de eso se guardó");
    }

    #[test]
    fn shuffling_gives_a_permutation_and_not_always_the_same_one() {
        for seed in [1u64, 42, 99_999] {
            let o = shuffled(8, seed);
            assert!(is_permutation(&o, 8), "{o:?} no es una permutación");
        }
        assert_ne!(shuffled(16, 1), shuffled(16, 2), "semillas distintas, barajas distintas");
        assert_eq!(shuffled(16, 7), shuffled(16, 7), "la misma semilla siempre da lo mismo");
        assert_eq!(shuffled(1, 3), vec![0], "una sola pista no se baraja");
        assert!(shuffled(0, 3).is_empty());
    }

    #[test]
    fn clearing_restores_the_array_order() {
        let o = Order::new();
        assert!(o.set(vec![1, 0], 2));
        o.clear();
        assert_eq!(o.first(2), 0);
    }
}
