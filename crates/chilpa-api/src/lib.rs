//! La fachada de Chilpa: lo único que un frontend puede usar (ADR-001).
//!
//! Un shell manda [`Command`]s y recibe [`Event`]s. No ve el core, ni la
//! biblioteca, ni el player. Si una vista necesita algo que no está aquí,
//! primero se amplía esta API.

pub use chilpa_core::queue_playback::Repeat;
pub use chilpa_library::{AlbumSort, Stats};
pub use plugins::PluginInfo;
pub use search::{Hit, KEYSTROKE_DEBOUNCE, SOURCE_COOLDOWN};

mod library;
mod queue;
mod playlists;
mod approvals;
mod plugins;
mod search;

use chilpa_models::{AudioQuality, SourceId, TrackRef};
use std::path::PathBuf;

/// Lo que un frontend le pide a Chilpa.
#[derive(Clone, PartialEq, Debug)]
pub enum Command {
    /// Reproducir un track de cualquier source.
    Play(TrackRef),
    Pause,
    Resume,
    /// Listar los sources registrados.
    ListSources,
    /// Reproducir un archivo local en un dispositivo, con la política de
    /// salida dada. Es la ruta mínima mientras la biblioteca no existe.
    PlayFile { path: PathBuf, device: String, bit_perfect: bool },
}

/// Lo que Chilpa le cuenta al frontend.
#[derive(Clone, PartialEq, Debug)]
pub enum Event {
    Sources(Vec<SourceId>),
    /// Empezó a sonar algo, con la calidad real de la salida.
    Playing { track: TrackRef, quality: AudioQuality },
    Paused,
    /// Terminó de sonar un archivo: cuántos frames salieron, con qué formato
    /// se abrió el dispositivo y si fue bit-perfect.
    Played { frames: u64, quality: AudioQuality, bit_perfect: bool },
    /// Algo salió mal, en un texto que la UI puede mostrar.
    Failed { message: String },
}

/// Una salida de audio que el usuario puede elegir.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Output {
    pub id: String,
    pub label: String,
}

/// Lo que está sonando, como lo necesita una vista.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct NowPlaying {
    pub path: String,
    /// El título que dice el archivo; si no dice nada, su nombre.
    pub title: String,
    pub artist: String,
    pub album: String,
    /// La portada ya en disco, lista para mostrarse. Vacío si no hay.
    pub artwork_path: String,
    pub playing: bool,
    pub paused: bool,
    /// Frames entregados y su equivalente en segundos.
    pub frames: u64,
    pub seconds: f64,
    /// Duración total, cuando se conoce.
    pub total_seconds: f64,
    /// Con qué se abrió el dispositivo, ya listo para mostrar.
    pub format: String,
    pub bit_perfect: bool,
    /// Qué lugar ocupa en la cola y cuántas pistas tiene.
    pub index: usize,
    pub queue_len: usize,
    /// Lo que el análisis dice de lo que suena: "128 BPM · 8B". Vacío si
    /// todavía no se ha analizado.
    pub analysis: String,
    /// La letra de lo que suena, un renglón por entrada. Vacía si no hay.
    pub lyrics: Vec<String>,
    /// Qué renglón va, o -1 cuando la letra no trae tiempos o todavía no
    /// entra ninguno.
    pub lyrics_line: i32,
    /// Cierto si la letra trae tiempos y se puede seguir.
    pub lyrics_synced: bool,
    /// De dónde salió: "archivo", "tags" o vacío.
    pub lyrics_source: String,
    /// El último problema, vacío si no hay.
    pub error: String,
}

/// Las extensiones que Chilpa sabe abrir hoy.
const EXTENSIONS: &[&str] =
    &["flac", "wav", "aiff", "aif", "m4a", "mp4", "alac", "mp3", "ogg", "oga", "dsf"];

/// Los archivos de audio de una carpeta, en orden alfabético.
///
/// No entra a subcarpetas: eso es trabajo de la biblioteca, no de una vista.
pub fn audio_files_in(folder: &std::path::Path) -> Vec<std::path::PathBuf> {
    let Ok(dir) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut files: Vec<_> = dir
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .and_then(|e| e.to_str())
                    .map(|e| EXTENSIONS.contains(&e.to_lowercase().as_str()))
                    .unwrap_or(false)
        })
        .collect();
    files.sort();
    files
}

/// El recorrido que deja el tempo más parejo, empezando en `desde`.
///
/// Es un vecino más cercano, a propósito: una mezcla no necesita el orden
/// óptimo, necesita no dar saltos de cuarenta BPM entre dos pistas. Lo que
/// no trae tempo se va al final, en su orden.
fn order_by_tempo(bpms: &[Option<f32>], from: usize) -> Vec<usize> {
    let mut missing: Vec<usize> = (0..bpms.len()).filter(|i| *i != from).collect();
    let mut order = vec![from];
    let mut ultimo = bpms.get(from).copied().flatten();

    while !missing.is_empty() {
        // De las que tienen tempo, la más cercana a la anterior.
        let chosen = match ultimo {
            Some(t) => missing
                .iter()
                .enumerate()
                .filter(|(_, i)| bpms[**i].is_some())
                .min_by(|a, b| {
                    let da = (bpms[*a.1].unwrap() - t).abs();
                    let db = (bpms[*b.1].unwrap() - t).abs();
                    da.total_cmp(&db)
                })
                .map(|(pos, _)| pos),
            None => None,
        };
        // Si ya no queda ninguna con tempo, se vacía lo que sobra tal cual.
        let pos = chosen.unwrap_or(0);
        let i = missing.remove(pos);
        ultimo = bpms[i].or(ultimo);
        order.push(i);
    }
    order
}

/// Lo que el análisis sacó, en una línea.
///
/// La sonoridad se dice, no se aplica: corregirla cambiaría las muestras, y
/// ahí se acabó el bit-perfect.
pub(crate) fn describe_analysis(
    bpm: Option<f32>,
    tone: Option<&str>,
    lufs: Option<f32>,
) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(b) = bpm {
        parts.push(format!("{b:.0} BPM"));
    }
    if let Some(t) = tone {
        parts.push(t.to_string());
    }
    if let Some(l) = lufs {
        parts.push(format!("{l:.1} LUFS"));
    }
    parts.join(" · ")
}

/// Traduce una pista de la biblioteca a lo que necesita una vista.
fn track_of(t: chilpa_library::StoredTrack) -> LibraryTrack {
    LibraryTrack {
        id: t.id,
        path: t.external_id,
        title: t.title,
        artist: t.artist,
        album: t.album,
        track_number: t.track_number,
        duration_ms: t.duration_ms,
        present: t.present,
        analysis: String::new(),
    }
}

/// El título que muestra una cola: el del tag, o el nombre del archivo.
fn title_of(path: &std::path::Path) -> String {
    chilpa_tags::read(path)
        .ok()
        .and_then(|info| info.tags.title)
        .unwrap_or_else(|| {
            path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
        })
}

/// Ahora, en segundos desde el epoch.
fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Cómo se nombra un número de canales, ya en plural cuando toca.
fn channels(n: u16) -> String {
    match n {
        1 => "mono".to_string(),
        2 => "estéreo".to_string(),
        others => format!("{others} canales"),
    }
}

/// Lo que se sabe de la pista que suena, para registrarla al terminar.
#[derive(Clone, Debug)]
struct InProgress {
    path: String,
    started_at: i64,
    sample_rate_hz: u32,
    bit_depth: Option<u8>,
    bit_perfect: bool,
    duration_ms: u64,
}

/// Una lista inteligente, con lo que le sale hoy.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SmartListInfo {
    pub name: String,
    pub track_count: usize,
}

/// Una lista de reproducción, lista para mostrar.
///
/// `provider` dice de dónde vino. La vista puede filtrar por él: es lo que
/// permite que un plugin traiga sus listas sin que se confundan con las de
/// aquí.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Playlist {
    pub id: i64,
    pub name: String,
    pub provider: String,
    pub track_count: usize,
}

/// Lo que el asistente encontró de una salida.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct WizardReport {
    pub device: String,
    /// Cierto si se midió abriendo el dispositivo.
    pub probed: bool,
    pub summary: String,
    pub rates: Vec<u32>,
    pub formats: Vec<String>,
    pub rows: Vec<WizardRow>,
}

/// Un formato y qué pasa con él.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct WizardRow {
    pub name: String,
    pub format: String,
    /// "bit-perfect", "convertido a ..." o "no se puede".
    pub verdict: String,
}

/// Un artista de la biblioteca, listo para mostrar.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Artist {
    pub name: String,
    pub album_count: usize,
    pub track_count: usize,
    pub artwork_path: String,
}

/// Un álbum como lo necesita una vista.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Album {
    pub id: i64,
    pub title: String,
    pub artist: String,
    pub year: Option<i32>,
    pub track_count: usize,
    /// La portada en disco, vacío si no hay.
    pub artwork_path: String,
    /// Cuántas escuchas lleva, contadas del log.
    pub play_count: usize,
}

/// Una pista de la biblioteca.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LibraryTrack {
    pub id: i64,
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub track_number: Option<u16>,
    pub duration_ms: u64,
    pub present: bool,
    /// Lo que el análisis sacó, vacío si todavía no se ha analizado.
    pub analysis: String,
}

/// Cómo va el análisis que corre en segundo plano.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct AnalysisStatus {
    pub running: bool,
    pub done: usize,
    pub total: usize,
    /// La última pista que se miró, para que se vea que avanza.
    pub last: String,
    /// Cuántas quedaron analizadas, sin pulso y sin poder leerse.
    pub analyzed: usize,
    pub without_pulse: usize,
    pub failed: usize,
}

/// Cómo va el escaneo que corre en segundo plano.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct ScanStatus {
    pub running: bool,
    pub done: usize,
    pub total: usize,
    /// Lo que dejó el último escaneo terminado.
    pub summary: ScanSummary,
}

/// Qué pasó en un escaneo, en una línea para la vista.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct ScanSummary {
    pub seen: usize,
    pub read: usize,
    pub unchanged: usize,
    pub missing: usize,
    pub unreachable: bool,
}

/// La instancia de Chilpa con la que habla un frontend.
pub struct Chilpa {
    core: chilpa_core::Core,
    library: Option<chilpa_library::Library>,
    /// Lo que hace falta para registrar la reproducción cuando termine.
    in_progress: Option<InProgress>,
    /// Las rutas de la cola actual, completa y en el orden en que se
    /// encoló. Volver atrás no la recorta.
    queue: Vec<String>,
    /// Qué posición de `cola` fue la primera que se le dio al reproductor.
    /// Volver atrás relanza desde ahí, y la posición que ve la vista se
    /// cuenta sobre la cola entera.
    base: usize,
    shuffle: bool,
    repeat: Repeat,
    album_sort: AlbumSort,
    /// El artista al que está filtrada la reja, vacío si no hay filtro.
    artist: String,
    /// Qué posiciones de `cola` se quitaron, para que la vista las apague.
    removed: Vec<bool>,
    /// La letra de lo que suena, para no releerla en cada pulso.
    lyrics: Option<chilpa_lyrics::Lyrics>,
    /// El sandbox de plugins, armado la primera vez que se pide. Va
    /// compartido porque la búsqueda lo consulta desde su propio hilo: un
    /// source lento no puede dejar la ventana esperando.
    plugins: Option<std::sync::Arc<std::sync::Mutex<plugins::Plugins>>>,
    /// Lo que de verdad sale por el dispositivo, para los visualizadores.
    tap: chilpa_core::tap::Tap,
    /// El escaneo en segundo plano, mientras corre.
    scan: Option<chilpa_library::ScanJob>,
    scan_status: ScanStatus,
    analysis: Option<chilpa_core::analysis_job::AnalysisJob>,
    analysis_status: AnalysisStatus,
    player: Option<chilpa_core::player::Player>,
    /// La búsqueda en curso: lo local, lo que aportaron los sources y lo
    /// que la vista dibuja.
    search: search::SearchState,
    now: NowPlaying,
    sample_rate: u32,
}

impl Default for Chilpa {
    fn default() -> Self {
        Self::new()
    }
}

impl Chilpa {
    pub fn new() -> Self {
        Self {
            core: chilpa_core::Core::new(),
            library: None,
            in_progress: None,
            queue: Vec::new(),
            base: 0,
            shuffle: false,
            repeat: Repeat::Off,
            album_sort: AlbumSort::from_name(&chilpa_core::settings::load().album_sort),
            artist: String::new(),
            removed: Vec::new(),
            lyrics: None,
            plugins: None,
            tap: chilpa_core::tap::Tap::new(),
            scan: None,
            scan_status: ScanStatus::default(),
            analysis: None,
            analysis_status: AnalysisStatus::default(),
            player: None,
            search: search::SearchState::default(),
            now: NowPlaying::default(),
            sample_rate: 0,
        }
    }

    /// La biblioteca, abriéndola la primera vez que se usa.
    pub(crate) fn library(&mut self) -> Option<&chilpa_library::Library> {
        if self.library.is_none() {
            match chilpa_core::open_library() {
                Ok(lib) => self.library = Some(lib),
                Err(e) => {
                    self.now.error = format!("no se pudo abrir la biblioteca: {e}");
                    return None;
                }
            }
        }
        self.library.as_ref()
    }










    /// Lo que el análisis ya sabía de la pista que empieza.
    fn load_analysis(&mut self, path: &std::path::Path) {
        self.now.analysis = String::new();
        let Some(lib) = self.library() else { return };
        let Ok(source) = lib.source_id("local", "default") else { return };
        let Some(id) = lib.track_by_path(source, path).ok().flatten() else { return };
        let Some((bpm, tone, loudness)) =
            lib.track_features(id, chilpa_core::analysis_job::ANALYZER).ok().flatten()
        else {
            return;
        };
        self.now.analysis = describe_analysis(bpm, tone.as_deref(), loudness);
    }

    /// Busca la letra de lo que empieza a sonar.
    ///
    /// Se lee una vez por pista, no en cada pulso: son unos kilobytes, pero
    /// abrir un archivo sesenta veces por minuto no tiene ningún sentido.
    fn load_lyrics(&mut self, path: &std::path::Path) {
        let found = chilpa_lyrics::find(path);
        self.now.lyrics = found
            .as_ref()
            .map(|h| h.lyrics.lines.iter().map(|l| l.text.clone()).collect())
            .unwrap_or_default();
        self.now.lyrics_synced = found.as_ref().is_some_and(|h| h.lyrics.synced);
        self.now.lyrics_source = match found.as_ref().map(|h| &h.source) {
            Some(chilpa_lyrics::Source::Sidecar(_)) => "archivo".into(),
            Some(chilpa_lyrics::Source::Embedded) => "tags".into(),
            None => String::new(),
        };
        self.now.lyrics_line = -1;
        self.lyrics = found.map(|h| h.lyrics);
    }

    /// Qué renglón de la letra va en ese segundo.
    fn line_at(&self, seconds: f64) -> i32 {
        let Some(lyrics) = &self.lyrics else { return -1 };
        let ms = (seconds.max(0.0) * 1000.0) as u64;
        lyrics.line_at(ms).map(|i| i as i32).unwrap_or(-1)
    }



































    /// Las unidades de CD que hay en la máquina.
    ///
    /// Se miran los nodos de siempre; una unidad que no está no es un
    /// error, nomás no sale en la lista.
    pub fn cd_drives(&self) -> Vec<String> {
        let mut units = Vec::new();
        for name in ["/dev/sr0", "/dev/sr1", "/dev/cdrom", "/dev/dvd"] {
            let p = std::path::Path::new(name);
            if p.exists() && !units.iter().any(|u: &String| u == name) {
                units.push(name.to_string());
            }
        }
        units
    }

    /// Pone a sonar el CD que haya en esa unidad, de principio a fin.
    pub fn play_cd(&mut self, drive: &str, device: &str, bit_perfect: bool) -> bool {
        let unit = if drive.is_empty() {
            match self.cd_drives().first() {
                Some(u) => u.clone(),
                None => {
                    self.now.error = "no encontré una unidad de CD".into();
                    return false;
                }
            }
        } else {
            drive.to_string()
        };
        self.queue = vec![format!("cdda:{unit}")];
        self.removed = vec![false];
        self.spawn(0, device, bit_perfect);
        true
    }

    /// Pone a sonar una radio o cualquier stream por HTTP.
    pub fn play_url(&mut self, url: &str, device: &str, bit_perfect: bool) {
        let url = url.trim().to_string();
        if !url.starts_with("http://") && !url.starts_with("https://") {
            self.now.error = "eso no parece una dirección de stream".into();
            return;
        }
        self.queue = vec![url.clone()];
        self.spawn(0, device, bit_perfect);
    }




    /// Lo que Chilpa recuerda: la última salida, si se pide bit-perfect y
    /// la última carpeta abierta.
    pub fn settings(&self) -> chilpa_core::settings::Settings {
        chilpa_core::settings::load()
    }

    /// Recuerda la salida y la política elegidas.
    pub fn remember(&mut self, output: &str, bit_perfect: bool) {
        let mut s = chilpa_core::settings::load();
        s.output = output.to_string();
        s.bit_perfect = bit_perfect;
        // Si no se puede guardar, la sesión sigue: es una preferencia, no
        // una parte del reproductor.
        let _ = chilpa_core::settings::save(&s);
    }

    /// Recuerda qué visualizador se está dibujando.
    pub fn remember_visualizer(&mut self, visualizer: &str) {
        let mut s = chilpa_core::settings::load();
        s.visualizer = visualizer.to_string();
        let _ = chilpa_core::settings::save(&s);
    }

    /// Recuerda el acomodo elegido para la ventana.
    pub fn remember_layout(&mut self, layout: &str) {
        let mut s = chilpa_core::settings::load();
        s.layout = layout.to_string();
        let _ = chilpa_core::settings::save(&s);
    }

    /// Recuerda la última carpeta abierta.
    pub fn remember_folder(&mut self, folder: &str) {
        let mut s = chilpa_core::settings::load();
        s.last_folder = folder.to_string();
        let _ = chilpa_core::settings::save(&s);
    }

    /// El diagnóstico de una salida: qué formatos salen tal cual.
    ///
    /// Mide el dispositivo de verdad. No configura nada: el asistente de QBZ
    /// escribía ajustes a ciegas, y eso es peor que no saber.
    pub fn wizard(&mut self, device: &str) -> WizardReport {
        let id = if device.is_empty() { self.device_or_first() } else { device.to_string() };
        let caps = match chilpa_core::probe_device(&id) {
            Ok(c) => c,
            Err(e) => {
                return WizardReport {
                    device: id,
                    probed: false,
                    summary: format!("no se pudo abrir el dispositivo: {e:?}"),
                    rates: Vec::new(),
                    formats: Vec::new(),
                    rows: Vec::new(),
                }
            }
        };
        let d = chilpa_core::wizard::diagnose(&id, &caps);
        WizardReport {
            device: d.device,
            probed: d.probed,
            summary: d.summary,
            rates: d.sample_rates,
            formats: d.formats,
            rows: d
                .rows
                .into_iter()
                .map(|f| WizardRow {
                    name: f.name,
                    format: f.format,
                    verdict: match f.verdict {
                        chilpa_core::wizard::Verdict::BitPerfect => "bit-perfect".into(),
                        chilpa_core::wizard::Verdict::Converted { a } => {
                            format!("convertido a {a}")
                        }
                        chilpa_core::wizard::Verdict::Unsupported => "no se puede".into(),
                    },
                })
                .collect(),
        }
    }

    /// La salida elegida, o la primera que haya.
    fn device_or_first(&self) -> String {
        self.outputs().first().map(|o| o.id.clone()).unwrap_or_default()
    }

    /// Las salidas que el usuario puede elegir.
    pub fn outputs(&self) -> Vec<Output> {
        chilpa_core::outputs()
            .into_iter()
            .map(|o| Output { id: o.id, label: o.label })
            .collect()
    }

    /// Pone a sonar un archivo. Regresa de inmediato: lo que pase después
    /// llega por [`Chilpa::poll`].
    pub fn play_file(&mut self, path: &std::path::Path, device: &str, bit_perfect: bool) {
        // Un archivo suelto también es una cola de una pista: así pasa por
        // el mismo camino y hereda el tap, la repetición y el orden.
        self.queue = vec![path.display().to_string()];
        self.removed = vec![false];
        self.spawn(0, device, bit_perfect);
        self.now.path = path.display().to_string();
    }

    /// La envolvente de lo que acaba de salir, en `puntos` valores de 0 a 1.
    ///
    /// Es el pico de cada tajada, no el promedio: una onda dibujada con
    /// promedios se ve plana y no se parece a lo que se oye.
    pub fn scope_envelope(&self, points: usize) -> Vec<f32> {
        let snapshot = self.tap.snapshot();
        let channels = snapshot.channels.max(1) as usize;
        let frames = snapshot.samples.len() / channels;
        if frames == 0 || points == 0 {
            return Vec::new();
        }
        let per_point = (frames / points).max(1);
        let mut output = Vec::with_capacity(points);
        for i in 0..points {
            let from = i * per_point;
            if from >= frames {
                break;
            }
            let hasta = (from + per_point).min(frames);
            let mut pico = 0.0f32;
            for f in from..hasta {
                for c in 0..channels {
                    pico = pico.max(snapshot.samples[f * channels + c].abs());
                }
            }
            output.push(pico);
        }
        output
    }

    /// El espectro de lo que sale, en `bandas` valores de 0 a 1.
    ///
    /// Los canales se mezclan a mono: un espectro por canal se ve igual en
    /// casi toda la música y ocupa el doble de pantalla.
    pub fn scope_spectrum(&self, bands: usize) -> Vec<f32> {
        let snapshot = self.tap.snapshot();
        let channels = snapshot.channels.max(1) as usize;
        if snapshot.samples.is_empty() {
            return Vec::new();
        }
        let mono: Vec<f32> = snapshot
            .samples
            .chunks_exact(channels)
            .map(|f| f.iter().sum::<f32>() / channels as f32)
            .collect();
        chilpa_core::spectrum::bands(&mono, snapshot.sample_rate_hz, bands)
    }

    /// Pares (izquierdo, derecho) de lo que acaba de salir, para el
    /// goniómetro. En mono se devuelve el mismo valor dos veces, que es lo
    /// que dibuja una línea a 45 grados, como debe ser.
    pub fn scope_pairs(&self, points: usize) -> Vec<(f32, f32)> {
        let snapshot = self.tap.snapshot();
        let channels = snapshot.channels.max(1) as usize;
        let frames = snapshot.samples.len() / channels;
        if frames == 0 || points == 0 {
            return Vec::new();
        }
        // Se toma uno de cada tantos: dibujar ocho mil puntos por cuadro no
        // se ve distinto y sí se siente.
        let skip = (frames / points).max(1);
        (0..frames)
            .step_by(skip)
            .take(points)
            .map(|f| {
                let i = snapshot.samples[f * channels];
                let d = if channels > 1 { snapshot.samples[f * channels + 1] } else { i };
                (i, d)
            })
            .collect()
    }

    /// La correlación entre los dos canales, de -1 a 1.
    ///
    /// Uno es mono, cero es estéreo ancho y por debajo de cero hay algo en
    /// contrafase, que en mono se cancela. Es el número que un medidor de
    /// fase enseña, y el que dice si una mezcla va a sobrevivir a un
    /// altavoz solo.
    pub fn scope_correlation(&self) -> Option<f32> {
        let snapshot = self.tap.snapshot();
        let channels = snapshot.channels.max(1) as usize;
        if channels < 2 || snapshot.samples.is_empty() {
            return None;
        }
        let (mut si, mut sd, mut sid) = (0.0f64, 0.0f64, 0.0f64);
        for f in snapshot.samples.chunks_exact(channels) {
            let (i, d) = (f64::from(f[0]), f64::from(f[1]));
            si += i * i;
            sd += d * d;
            sid += i * d;
        }
        let den = (si * sd).sqrt();
        (den > 0.0).then(|| (sid / den) as f32)
    }

    /// El nivel por canal, de 0 a 1: pico y rms.
    pub fn scope_levels(&self) -> Vec<(f32, f32)> {
        let snapshot = self.tap.snapshot();
        snapshot.peaks.iter().copied().zip(snapshot.rms.iter().copied()).collect()
    }

    /// Lo que de verdad está saliendo por el dispositivo, para dibujarlo.
    ///
    /// Son las muestras entregadas, copiadas después de empaquetarlas: lo
    /// que se ve es lo que sonó, no lo que se decodificó.
    pub fn scope(&self) -> chilpa_core::tap::Snapshot {
        self.tap.snapshot()
    }

    /// Pone a sonar una carpeta completa, en orden alfabético.
    ///
    /// Regresa los títulos encolados: el del tag cuando el archivo lo trae,
    /// y si no, el nombre del archivo.
    pub fn play_folder(
        &mut self,
        folder: &std::path::Path,
        device: &str,
        bit_perfect: bool,
    ) -> Vec<String> {
        let tracks = audio_files_in(folder);
        if tracks.is_empty() {
            self.now.error = format!("no hay audio en {}", folder.display());
            return Vec::new();
        }
        // La carpeta también entra a la cola: si no, guardarla o volver
        // atrás no tendrían de dónde leer.
        self.queue = tracks.iter().map(|p| p.display().to_string()).collect();
        self.spawn(0, device, bit_perfect);
        tracks.iter().map(|p| title_of(p)).collect()
    }

    /// Pausa o reanuda lo que suena.
    pub fn set_paused(&mut self, paused: bool) {
        if let Some(p) = &self.player {
            p.set_paused(paused);
            self.now.paused = paused;
        }
    }

    /// Salta a ese segundo de la pista que suena.
    pub fn seek(&mut self, seconds: f64) {
        if let Some(p) = &self.player {
            let safe = seconds.max(0.0);
            p.seek(std::time::Duration::from_secs_f64(safe));
        }
    }

    /// Salta a la siguiente pista de la cola.
    pub fn next_track(&mut self) {
        if let Some(p) = &self.player {
            p.next();
        }
    }

    /// Vuelve a la pista anterior.
    ///
    /// No hay una señal para esto: la cola se vuelve a lanzar desde la pista
    /// de antes, que es lo que el reproductor sabe hacer. La cola no se
    /// recorta, solo cambia por dónde empieza.
    ///
    /// Con el aleatorio encendido no hay "la de antes" que valga: lo que se
    /// hace es reiniciar la que suena.
    pub fn previous_track(&mut self, device: &str, bit_perfect: bool) {
        if self.queue.is_empty() || self.now.index == 0 || self.shuffle {
            // En la primera pista, "anterior" es volver a empezarla.
            self.seek(0.0);
            return;
        }
        self.spawn(self.now.index - 1, device, bit_perfect);
    }

    /// Para lo que esté sonando.
    pub fn stop(&mut self) {
        if let Some(p) = &self.player {
            p.stop();
        }
    }

    /// Recoge lo que haya pasado y actualiza el estado que ve la vista.
    pub fn poll(&mut self) -> &NowPlaying {
        use chilpa_core::player::PlayerEvent;
        self.advance_search();
        self.now.paused = self.player.as_ref().is_some_and(|p| p.is_paused());
        let events = self.player.as_ref().map(|p| p.poll()).unwrap_or_default();
        for event in events {
            match event {
                PlayerEvent::Started {
                    path,
                    opened,
                    bit_perfect,
                    total_frames,
                    tags,
                    artwork,
                    index,
                    queue_len,
                    dsd_multiple,
                } => {
                    self.sample_rate = opened.sample_rate_hz;
                    self.now.path = path.display().to_string();
                    let name = path
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default();
                    self.now.artwork_path = artwork
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_default();
                    match &tags {
                        Some(t) => {
                            self.now.title = t.title.clone().unwrap_or(name);
                            self.now.artist = t.artist_line();
                            self.now.album = t.album.clone().unwrap_or_default();
                        }
                        None => {
                            self.now.title = name;
                            self.now.artist = String::new();
                            self.now.album = String::new();
                        }
                    }
                    self.now.playing = true;
                    self.now.bit_perfect = bit_perfect;
                    self.load_lyrics(&path);
                    self.load_analysis(&path);
                    // El reproductor cuenta sobre lo que se le dio; la vista
                    // cuenta sobre la cola entera.
                    self.now.index = self.base + index;
                    self.now.queue_len = if self.queue.is_empty() {
                        queue_len
                    } else {
                        self.queue.len()
                    };
                    // Cada arranque cierra lo anterior: en una cola, la
                    // pista que empieza es la prueba de que la de antes
                    // terminó.
                    self.log_play(false);
                    self.in_progress = Some(InProgress {
                        path: path.display().to_string(),
                        started_at: now_unix(),
                        sample_rate_hz: opened.sample_rate_hz,
                        bit_depth: opened
                            .sample_format
                            .is_integer()
                            .then(|| opened.sample_format.valid_bits()),
                        bit_perfect,
                        duration_ms: match (total_frames, opened.sample_rate_hz) {
                            (Some(f), rate) if rate > 0 => f * 1000 / u64::from(rate),
                            _ => 0,
                        },
                    });
                    self.now.total_seconds = match (total_frames, opened.sample_rate_hz) {
                        (Some(f), rate) if rate > 0 => f as f64 / f64::from(rate),
                        _ => 0.0,
                    };
                    self.now.format = match dsd_multiple {
                        // En DSD lo que importa es la tasa de DSD; el
                        // portador de DoP se dice aparte, porque es el
                        // formato con el que se abrió el dispositivo.
                        Some(multiple) => format!(
                            "DSD{multiple} · DoP a {} Hz · {}",
                            opened.sample_rate_hz,
                            channels(opened.channels)
                        ),
                        None => format!(
                            "{} Hz · {} bits · {}",
                            opened.sample_rate_hz,
                            opened.sample_format.valid_bits(),
                            channels(opened.channels)
                        ),
                    };
                }
                PlayerEvent::StreamTitle { title } => {
                    // En una radio, el título lo manda la estación y cambia
                    // con cada canción.
                    self.now.title = title;
                }
                PlayerEvent::Progress { frames } => {
                    self.now.frames = frames;
                    if self.sample_rate > 0 {
                        self.now.seconds = frames as f64 / f64::from(self.sample_rate);
                    }
                    self.now.lyrics_line = self.line_at(self.now.seconds);
                }
                PlayerEvent::Finished { frames } => {
                    self.now.frames = frames;
                    self.now.playing = false;
                    self.log_play(false);
                }
                PlayerEvent::Stopped { frames } => {
                    self.now.frames = frames;
                    self.now.playing = false;
                    // Parar a media pista cuenta como salto.
                    self.log_play(true);
                }
                PlayerEvent::Failed { message } => {
                    self.now.playing = false;
                    self.now.error = message;
                    // Una falla del dispositivo no es una escucha, pero lo
                    // que alcanzó a sonar sí se registra, marcado como
                    // interrumpido.
                    self.log_play(true);
                }
            }
        }
        &self.now
    }

    /// Guarda en el log lo que llevaba sonando la pista en curso.
    ///
    /// El log es append-only: de aquí salen las stats, las smart lists y el
    /// sync de play counts con un DAP.
    fn log_play(&mut self, skipped: bool) {
        let Some(current) = self.in_progress.take() else { return };
        let frames = self.now.frames;
        if current.sample_rate_hz == 0 || frames == 0 {
            return;
        }
        let played_ms = frames * 1000 / u64::from(current.sample_rate_hz);
        let path = std::path::PathBuf::from(&current.path);
        let Some(lib) = self.library() else { return };
        let Ok(source) = lib.source_id("local", "default") else { return };
        let track = lib.track_by_path(source, &path).ok().flatten();
        let _ = lib.log_play(
            track,
            Some(source),
            current.started_at,
            played_ms,
            current.duration_ms,
            skipped,
            Some(current.sample_rate_hz),
            current.bit_depth,
            current.bit_perfect,
        );
    }

    /// El estado actual, sin recoger eventos.
    pub fn now_playing(&self) -> &NowPlaying {
        &self.now
    }

    /// Ejecuta un comando y regresa lo que el frontend debe saber.
    pub fn dispatch(&mut self, command: Command) -> Event {
        match command {
            Command::ListSources => Event::Sources(self.core.sources().to_vec()),
            Command::PlayFile { path, device, bit_perfect } => {
                match self.core.play_local(&path, &device, bit_perfect) {
                    Ok(played) => Event::Played {
                        frames: played.frames,
                        quality: played.quality,
                        bit_perfect: played.bit_perfect,
                    },
                    Err(message) => Event::Failed { message },
                }
            }
            Command::Play(_) | Command::Pause | Command::Resume => {
                Event::Failed { message: "el player todavía no se porta".into() }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listing_sources_returns_the_local_library() {
        let mut chilpa = Chilpa::new();
        assert_eq!(chilpa.dispatch(Command::ListSources), Event::Sources(vec![SourceId::local()]));
    }

    #[test]
    fn the_mix_walks_tempo_to_tempo_without_jumps() {
        // Empieza en la de 120 y debería seguir 118, 125, 90.
        let bpms = vec![Some(90.0), Some(120.0), Some(125.0), Some(118.0)];
        assert_eq!(order_by_tempo(&bpms, 1), vec![1, 3, 2, 0]);
    }

    #[test]
    fn what_has_no_tempo_goes_last() {
        let bpms = vec![Some(120.0), None, Some(122.0), None, Some(160.0)];
        let order = order_by_tempo(&bpms, 0);
        assert_eq!(&order[..3], &[0, 2, 4], "primero las que sí, de cerca en cerca");
        let queue: Vec<usize> = order[3..].to_vec();
        assert_eq!(queue, vec![1, 3], "y las ciegas al final, en su orden");
    }

    #[test]
    fn a_mix_loses_no_track_and_repeats_none() {
        let bpms = vec![Some(100.0), Some(140.0), None, Some(101.0), Some(139.0), None];
        let order = order_by_tempo(&bpms, 3);
        let mut seen = order.clone();
        seen.sort_unstable();
        assert_eq!(seen, (0..6).collect::<Vec<_>>(), "es una permutación: {order:?}");
        assert_eq!(order[0], 3, "arranca donde se le dijo");
    }

    #[test]
    fn with_no_tempo_at_all_the_mix_leaves_the_order_alone() {
        let bpms = vec![None, None, None];
        assert_eq!(order_by_tempo(&bpms, 0), vec![0, 1, 2]);
    }

    #[test]
    fn channels_are_named_the_way_they_are_said() {
        assert_eq!(channels(1), "mono");
        assert_eq!(channels(2), "estéreo");
        assert_eq!(channels(6), "6 canales");
    }

    #[test]
    fn a_folder_yields_audio_files_only_and_in_order() {
        let mut dir = std::env::temp_dir();
        dir.push("chilpa-api-carpeta");
        let _ = std::fs::create_dir_all(&dir);
        for name in ["b.flac", "a.wav", "notas.txt", "c.MP3"] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        let found: Vec<String> = audio_files_in(&dir)
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(found, vec!["a.wav", "b.flac", "c.MP3"], "sin el .txt y ordenados");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_state_starts_empty_and_without_error() {
        let chilpa = Chilpa::new();
        assert!(!chilpa.now_playing().playing);
        assert!(chilpa.now_playing().error.is_empty());
        assert_eq!(chilpa.now_playing().frames, 0);
    }

    #[test]
    fn the_listed_outputs_are_direct_ones() {
        for output in Chilpa::new().outputs() {
            assert!(
                output.id.starts_with("hw:") || output.id == "capture",
                "{} no es una salida válida",
                output.id
            );
        }
    }

    #[test]
    fn what_does_not_exist_fails_with_a_message_for_the_ui() {
        let mut chilpa = Chilpa::new();
        let track = TrackRef { source: SourceId::local(), external_id: "1".into() };
        assert!(matches!(chilpa.dispatch(Command::Play(track)), Event::Failed { .. }));
    }
}
