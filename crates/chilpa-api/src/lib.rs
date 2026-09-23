//! La fachada de Chilpa: lo único que un frontend puede usar (ADR-001).
//!
//! Un shell manda [`Command`]s y recibe [`Event`]s. No ve el core, ni la
//! biblioteca, ni el player. Si una vista necesita algo que no está aquí,
//! primero se amplía esta API.

pub use chilpa_core::queue_playback::Repeat;
pub use chilpa_library::{AlbumSort, Stats};

mod biblioteca;
mod cola;
mod listas;

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
const EXTENSIONES: &[&str] =
    &["flac", "wav", "aiff", "aif", "m4a", "mp4", "alac", "mp3", "ogg", "oga", "dsf"];

/// Los archivos de audio de una carpeta, en orden alfabético.
///
/// No entra a subcarpetas: eso es trabajo de la biblioteca, no de una vista.
pub fn audio_files_in(folder: &std::path::Path) -> Vec<std::path::PathBuf> {
    let Ok(dir) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut archivos: Vec<_> = dir
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .and_then(|e| e.to_str())
                    .map(|e| EXTENSIONES.contains(&e.to_lowercase().as_str()))
                    .unwrap_or(false)
        })
        .collect();
    archivos.sort();
    archivos
}

/// El recorrido que deja el tempo más parejo, empezando en `desde`.
///
/// Es un vecino más cercano, a propósito: una mezcla no necesita el orden
/// óptimo, necesita no dar saltos de cuarenta BPM entre dos pistas. Lo que
/// no trae tempo se va al final, en su orden.
fn orden_por_tempo(bpms: &[Option<f32>], desde: usize) -> Vec<usize> {
    let mut faltan: Vec<usize> = (0..bpms.len()).filter(|i| *i != desde).collect();
    let mut orden = vec![desde];
    let mut ultimo = bpms.get(desde).copied().flatten();

    while !faltan.is_empty() {
        // De las que tienen tempo, la más cercana a la anterior.
        let elegido = match ultimo {
            Some(t) => faltan
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
        let pos = elegido.unwrap_or(0);
        let i = faltan.remove(pos);
        ultimo = bpms[i].or(ultimo);
        orden.push(i);
    }
    orden
}

/// Lo que el análisis sacó, en una línea.
///
/// La sonoridad se dice, no se aplica: corregirla cambiaría las muestras, y
/// ahí se acabó el bit-perfect.
pub(crate) fn describir_analisis(
    bpm: Option<f32>,
    tono: Option<&str>,
    lufs: Option<f32>,
) -> String {
    let mut partes: Vec<String> = Vec::new();
    if let Some(b) = bpm {
        partes.push(format!("{b:.0} BPM"));
    }
    if let Some(t) = tono {
        partes.push(t.to_string());
    }
    if let Some(l) = lufs {
        partes.push(format!("{l:.1} LUFS"));
    }
    partes.join(" · ")
}

/// Traduce una pista de la biblioteca a lo que necesita una vista.
fn pista_de(t: chilpa_library::StoredTrack) -> LibraryTrack {
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
fn titulo_de(path: &std::path::Path) -> String {
    chilpa_tags::read(path)
        .ok()
        .and_then(|info| info.tags.title)
        .unwrap_or_else(|| {
            path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
        })
}

/// Ahora, en segundos desde el epoch.
fn ahora_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Cómo se nombra un número de canales, ya en plural cuando toca.
fn canales(n: u16) -> String {
    match n {
        1 => "mono".to_string(),
        2 => "estéreo".to_string(),
        otros => format!("{otros} canales"),
    }
}

/// Lo que se sabe de la pista que suena, para registrarla al terminar.
#[derive(Clone, Debug)]
struct EnCurso {
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
    en_curso: Option<EnCurso>,
    /// Las rutas de la cola actual, completa y en el orden en que se
    /// encoló. Volver atrás no la recorta.
    cola: Vec<String>,
    /// Qué posición de `cola` fue la primera que se le dio al reproductor.
    /// Volver atrás relanza desde ahí, y la posición que ve la vista se
    /// cuenta sobre la cola entera.
    base: usize,
    shuffle: bool,
    repeat: Repeat,
    album_sort: AlbumSort,
    /// El artista al que está filtrada la reja, vacío si no hay filtro.
    artista: String,
    /// Qué posiciones de `cola` se quitaron, para que la vista las apague.
    quitadas: Vec<bool>,
    /// La letra de lo que suena, para no releerla en cada pulso.
    letra: Option<chilpa_lyrics::Lyrics>,
    /// Lo que de verdad sale por el dispositivo, para los visualizadores.
    tap: chilpa_core::tap::Tap,
    /// El escaneo en segundo plano, mientras corre.
    scan: Option<chilpa_library::ScanJob>,
    scan_status: ScanStatus,
    analysis: Option<chilpa_core::analysis_job::AnalysisJob>,
    analysis_status: AnalysisStatus,
    player: Option<chilpa_core::player::Player>,
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
            en_curso: None,
            cola: Vec::new(),
            base: 0,
            shuffle: false,
            repeat: Repeat::Off,
            album_sort: AlbumSort::from_name(&chilpa_core::settings::load().album_sort),
            artista: String::new(),
            quitadas: Vec::new(),
            letra: None,
            tap: chilpa_core::tap::Tap::new(),
            scan: None,
            scan_status: ScanStatus::default(),
            analysis: None,
            analysis_status: AnalysisStatus::default(),
            player: None,
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
    fn cargar_analisis(&mut self, path: &std::path::Path) {
        self.now.analysis = String::new();
        let Some(lib) = self.library() else { return };
        let Ok(source) = lib.source_id("local", "default") else { return };
        let Some(id) = lib.track_by_path(source, path).ok().flatten() else { return };
        let Some((bpm, tono, sonoridad)) =
            lib.track_features(id, chilpa_core::analysis_job::ANALIZADOR).ok().flatten()
        else {
            return;
        };
        self.now.analysis = describir_analisis(bpm, tono.as_deref(), sonoridad);
    }

    /// Busca la letra de lo que empieza a sonar.
    ///
    /// Se lee una vez por pista, no en cada pulso: son unos kilobytes, pero
    /// abrir un archivo sesenta veces por minuto no tiene ningún sentido.
    fn cargar_letra(&mut self, path: &std::path::Path) {
        let hallada = chilpa_lyrics::find(path);
        self.now.lyrics = hallada
            .as_ref()
            .map(|h| h.lyrics.lines.iter().map(|l| l.text.clone()).collect())
            .unwrap_or_default();
        self.now.lyrics_synced = hallada.as_ref().is_some_and(|h| h.lyrics.synced);
        self.now.lyrics_source = match hallada.as_ref().map(|h| &h.source) {
            Some(chilpa_lyrics::Source::Sidecar(_)) => "archivo".into(),
            Some(chilpa_lyrics::Source::Embedded) => "tags".into(),
            None => String::new(),
        };
        self.now.lyrics_line = -1;
        self.letra = hallada.map(|h| h.lyrics);
    }

    /// Qué renglón de la letra va en ese segundo.
    fn renglon_en(&self, segundos: f64) -> i32 {
        let Some(letra) = &self.letra else { return -1 };
        let ms = (segundos.max(0.0) * 1000.0) as u64;
        letra.line_at(ms).map(|i| i as i32).unwrap_or(-1)
    }



































    /// Las unidades de CD que hay en la máquina.
    ///
    /// Se miran los nodos de siempre; una unidad que no está no es un
    /// error, nomás no sale en la lista.
    pub fn cd_drives(&self) -> Vec<String> {
        let mut unidades = Vec::new();
        for nombre in ["/dev/sr0", "/dev/sr1", "/dev/cdrom", "/dev/dvd"] {
            let p = std::path::Path::new(nombre);
            if p.exists() && !unidades.iter().any(|u: &String| u == nombre) {
                unidades.push(nombre.to_string());
            }
        }
        unidades
    }

    /// Pone a sonar el CD que haya en esa unidad, de principio a fin.
    pub fn play_cd(&mut self, drive: &str, device: &str, bit_perfect: bool) -> bool {
        let unidad = if drive.is_empty() {
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
        self.cola = vec![format!("cdda:{unidad}")];
        self.quitadas = vec![false];
        self.lanzar(0, device, bit_perfect);
        true
    }

    /// Pone a sonar una radio o cualquier stream por HTTP.
    pub fn play_url(&mut self, url: &str, device: &str, bit_perfect: bool) {
        let url = url.trim().to_string();
        if !url.starts_with("http://") && !url.starts_with("https://") {
            self.now.error = "eso no parece una dirección de stream".into();
            return;
        }
        self.cola = vec![url.clone()];
        self.lanzar(0, device, bit_perfect);
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
        let d = chilpa_core::wizard::diagnosticar(&id, &caps);
        WizardReport {
            device: d.device,
            probed: d.probed,
            summary: d.resumen,
            rates: d.sample_rates,
            formats: d.formats,
            rows: d
                .filas
                .into_iter()
                .map(|f| WizardRow {
                    name: f.nombre,
                    format: f.formato,
                    verdict: match f.veredicto {
                        chilpa_core::wizard::Veredicto::BitPerfect => "bit-perfect".into(),
                        chilpa_core::wizard::Veredicto::Convertido { a } => {
                            format!("convertido a {a}")
                        }
                        chilpa_core::wizard::Veredicto::NoSePuede => "no se puede".into(),
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
        self.cola = vec![path.display().to_string()];
        self.quitadas = vec![false];
        self.lanzar(0, device, bit_perfect);
        self.now.path = path.display().to_string();
    }

    /// La envolvente de lo que acaba de salir, en `puntos` valores de 0 a 1.
    ///
    /// Es el pico de cada tajada, no el promedio: una onda dibujada con
    /// promedios se ve plana y no se parece a lo que se oye.
    pub fn scope_envelope(&self, puntos: usize) -> Vec<f32> {
        let foto = self.tap.snapshot();
        let canales = foto.channels.max(1) as usize;
        let frames = foto.samples.len() / canales;
        if frames == 0 || puntos == 0 {
            return Vec::new();
        }
        let por_punto = (frames / puntos).max(1);
        let mut salida = Vec::with_capacity(puntos);
        for i in 0..puntos {
            let desde = i * por_punto;
            if desde >= frames {
                break;
            }
            let hasta = (desde + por_punto).min(frames);
            let mut pico = 0.0f32;
            for f in desde..hasta {
                for c in 0..canales {
                    pico = pico.max(foto.samples[f * canales + c].abs());
                }
            }
            salida.push(pico);
        }
        salida
    }

    /// El espectro de lo que sale, en `bandas` valores de 0 a 1.
    ///
    /// Los canales se mezclan a mono: un espectro por canal se ve igual en
    /// casi toda la música y ocupa el doble de pantalla.
    pub fn scope_spectrum(&self, bandas: usize) -> Vec<f32> {
        let foto = self.tap.snapshot();
        let canales = foto.channels.max(1) as usize;
        if foto.samples.is_empty() {
            return Vec::new();
        }
        let mono: Vec<f32> = foto
            .samples
            .chunks_exact(canales)
            .map(|f| f.iter().sum::<f32>() / canales as f32)
            .collect();
        chilpa_core::spectrum::bands(&mono, foto.sample_rate_hz, bandas)
    }

    /// El nivel por canal, de 0 a 1: pico y rms.
    pub fn scope_levels(&self) -> Vec<(f32, f32)> {
        let foto = self.tap.snapshot();
        foto.peaks.iter().copied().zip(foto.rms.iter().copied()).collect()
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
        let pistas = audio_files_in(folder);
        if pistas.is_empty() {
            self.now.error = format!("no hay audio en {}", folder.display());
            return Vec::new();
        }
        // La carpeta también entra a la cola: si no, guardarla o volver
        // atrás no tendrían de dónde leer.
        self.cola = pistas.iter().map(|p| p.display().to_string()).collect();
        self.lanzar(0, device, bit_perfect);
        pistas.iter().map(|p| titulo_de(p)).collect()
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
            let seguro = seconds.max(0.0);
            p.seek(std::time::Duration::from_secs_f64(seguro));
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
        if self.cola.is_empty() || self.now.index == 0 || self.shuffle {
            // En la primera pista, "anterior" es volver a empezarla.
            self.seek(0.0);
            return;
        }
        self.lanzar(self.now.index - 1, device, bit_perfect);
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
        self.now.paused = self.player.as_ref().is_some_and(|p| p.is_paused());
        let eventos = self.player.as_ref().map(|p| p.poll()).unwrap_or_default();
        for evento in eventos {
            match evento {
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
                    let nombre = path
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default();
                    self.now.artwork_path = artwork
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_default();
                    match &tags {
                        Some(t) => {
                            self.now.title = t.title.clone().unwrap_or(nombre);
                            self.now.artist = t.artist_line();
                            self.now.album = t.album.clone().unwrap_or_default();
                        }
                        None => {
                            self.now.title = nombre;
                            self.now.artist = String::new();
                            self.now.album = String::new();
                        }
                    }
                    self.now.playing = true;
                    self.now.bit_perfect = bit_perfect;
                    self.cargar_letra(&path);
                    self.cargar_analisis(&path);
                    // El reproductor cuenta sobre lo que se le dio; la vista
                    // cuenta sobre la cola entera.
                    self.now.index = self.base + index;
                    self.now.queue_len = if self.cola.is_empty() {
                        queue_len
                    } else {
                        self.cola.len()
                    };
                    // Cada arranque cierra lo anterior: en una cola, la
                    // pista que empieza es la prueba de que la de antes
                    // terminó.
                    self.registrar_reproduccion(false);
                    self.en_curso = Some(EnCurso {
                        path: path.display().to_string(),
                        started_at: ahora_unix(),
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
                        Some(multiplo) => format!(
                            "DSD{multiplo} · DoP a {} Hz · {}",
                            opened.sample_rate_hz,
                            canales(opened.channels)
                        ),
                        None => format!(
                            "{} Hz · {} bits · {}",
                            opened.sample_rate_hz,
                            opened.sample_format.valid_bits(),
                            canales(opened.channels)
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
                    self.now.lyrics_line = self.renglon_en(self.now.seconds);
                }
                PlayerEvent::Finished { frames } => {
                    self.now.frames = frames;
                    self.now.playing = false;
                    self.registrar_reproduccion(false);
                }
                PlayerEvent::Stopped { frames } => {
                    self.now.frames = frames;
                    self.now.playing = false;
                    // Parar a media pista cuenta como salto.
                    self.registrar_reproduccion(true);
                }
                PlayerEvent::Failed { message } => {
                    self.now.playing = false;
                    self.now.error = message;
                    // Una falla del dispositivo no es una escucha, pero lo
                    // que alcanzó a sonar sí se registra, marcado como
                    // interrumpido.
                    self.registrar_reproduccion(true);
                }
            }
        }
        &self.now
    }

    /// Guarda en el log lo que llevaba sonando la pista en curso.
    ///
    /// El log es append-only: de aquí salen las stats, las smart lists y el
    /// sync de play counts con un DAP.
    fn registrar_reproduccion(&mut self, skipped: bool) {
        let Some(curso) = self.en_curso.take() else { return };
        let frames = self.now.frames;
        if curso.sample_rate_hz == 0 || frames == 0 {
            return;
        }
        let played_ms = frames * 1000 / u64::from(curso.sample_rate_hz);
        let ruta = std::path::PathBuf::from(&curso.path);
        let Some(lib) = self.library() else { return };
        let Ok(source) = lib.source_id("local", "default") else { return };
        let track = lib.track_by_path(source, &ruta).ok().flatten();
        let _ = lib.log_play(
            track,
            Some(source),
            curso.started_at,
            played_ms,
            curso.duration_ms,
            skipped,
            Some(curso.sample_rate_hz),
            curso.bit_depth,
            curso.bit_perfect,
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
    fn listar_sources_regresa_la_biblioteca_local() {
        let mut chilpa = Chilpa::new();
        assert_eq!(chilpa.dispatch(Command::ListSources), Event::Sources(vec![SourceId::local()]));
    }

    #[test]
    fn la_mezcla_va_de_tempo_en_tempo_sin_saltos() {
        // Empieza en la de 120 y debería seguir 118, 125, 90.
        let bpms = vec![Some(90.0), Some(120.0), Some(125.0), Some(118.0)];
        assert_eq!(orden_por_tempo(&bpms, 1), vec![1, 3, 2, 0]);
    }

    #[test]
    fn lo_que_no_tiene_tempo_se_va_al_final() {
        let bpms = vec![Some(120.0), None, Some(122.0), None, Some(160.0)];
        let orden = orden_por_tempo(&bpms, 0);
        assert_eq!(&orden[..3], &[0, 2, 4], "primero las que sí, de cerca en cerca");
        let cola: Vec<usize> = orden[3..].to_vec();
        assert_eq!(cola, vec![1, 3], "y las ciegas al final, en su orden");
    }

    #[test]
    fn una_mezcla_no_pierde_ni_repite_pistas() {
        let bpms = vec![Some(100.0), Some(140.0), None, Some(101.0), Some(139.0), None];
        let orden = orden_por_tempo(&bpms, 3);
        let mut visto = orden.clone();
        visto.sort_unstable();
        assert_eq!(visto, (0..6).collect::<Vec<_>>(), "es una permutación: {orden:?}");
        assert_eq!(orden[0], 3, "arranca donde se le dijo");
    }

    #[test]
    fn sin_ningun_tempo_la_mezcla_deja_el_orden_como_estaba() {
        let bpms = vec![None, None, None];
        assert_eq!(orden_por_tempo(&bpms, 0), vec![0, 1, 2]);
    }

    #[test]
    fn los_canales_se_nombran_como_se_dicen() {
        assert_eq!(canales(1), "mono");
        assert_eq!(canales(2), "estéreo");
        assert_eq!(canales(6), "6 canales");
    }

    #[test]
    fn de_una_carpeta_solo_salen_archivos_de_audio_y_en_orden() {
        let mut dir = std::env::temp_dir();
        dir.push("chilpa-api-carpeta");
        let _ = std::fs::create_dir_all(&dir);
        for nombre in ["b.flac", "a.wav", "notas.txt", "c.MP3"] {
            std::fs::write(dir.join(nombre), b"x").unwrap();
        }
        let encontrados: Vec<String> = audio_files_in(&dir)
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(encontrados, vec!["a.wav", "b.flac", "c.MP3"], "sin el .txt y ordenados");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn el_estado_arranca_vacio_y_sin_error() {
        let chilpa = Chilpa::new();
        assert!(!chilpa.now_playing().playing);
        assert!(chilpa.now_playing().error.is_empty());
        assert_eq!(chilpa.now_playing().frames, 0);
    }

    #[test]
    fn las_salidas_que_se_listan_son_directas() {
        for salida in Chilpa::new().outputs() {
            assert!(
                salida.id.starts_with("hw:") || salida.id == "capture",
                "{} no es una salida válida",
                salida.id
            );
        }
    }

    #[test]
    fn lo_que_no_existe_falla_con_un_mensaje_para_la_ui() {
        let mut chilpa = Chilpa::new();
        let track = TrackRef { source: SourceId::local(), external_id: "1".into() };
        assert!(matches!(chilpa.dispatch(Command::Play(track)), Event::Failed { .. }));
    }
}
