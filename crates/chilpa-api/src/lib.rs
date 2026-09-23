//! La fachada de Chilpa: lo único que un frontend puede usar (ADR-001).
//!
//! Un shell manda [`Command`]s y recibe [`Event`]s. No ve el core, ni la
//! biblioteca, ni el player. Si una vista necesita algo que no está aquí,
//! primero se amplía esta API.

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
    pub track_number: Option<u16>,
    pub duration_ms: u64,
    pub present: bool,
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
            player: None,
            now: NowPlaying::default(),
            sample_rate: 0,
        }
    }

    /// La biblioteca, abriéndola la primera vez que se usa.
    fn library(&mut self) -> Option<&chilpa_library::Library> {
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

    /// Escanea una carpeta hacia la biblioteca.
    pub fn scan(&mut self, folder: &std::path::Path) -> ScanSummary {
        let Some(lib) = self.library() else { return ScanSummary::default() };
        match chilpa_core::scan_into_library(lib, folder) {
            Ok(r) => ScanSummary {
                seen: r.seen,
                read: r.read,
                unchanged: r.unchanged,
                missing: r.missing,
                unreachable: r.unreachable,
            },
            Err(e) => {
                self.now.error = format!("el escaneo falló: {e}");
                ScanSummary::default()
            }
        }
    }

    /// Los álbumes de la biblioteca.
    pub fn albums(&mut self, limit: usize, offset: usize) -> Vec<Album> {
        let Some(lib) = self.library() else { return Vec::new() };
        lib.albums(limit, offset)
            .unwrap_or_default()
            .into_iter()
            .map(|a| {
                let escuchas = lib.album_play_count(a.id).unwrap_or(0);
                Album {
                    id: a.id,
                    title: a.title,
                    artist: a.album_artist,
                    year: a.year,
                    track_count: a.track_count,
                    artwork_path: a.artwork_path.unwrap_or_default(),
                    play_count: escuchas,
                }
            })
            .collect()
    }

    /// Las pistas de un álbum, en el orden del disco.
    pub fn album_tracks(&mut self, album_id: i64) -> Vec<LibraryTrack> {
        let Some(lib) = self.library() else { return Vec::new() };
        lib.album_tracks(album_id)
            .unwrap_or_default()
            .into_iter()
            .map(|t| LibraryTrack {
                id: t.id,
                path: t.external_id,
                title: t.title,
                artist: t.artist,
                track_number: t.track_number,
                duration_ms: t.duration_ms,
                present: t.present,
            })
            .collect()
    }

    /// Pone a sonar un álbum completo, en el orden del disco.
    ///
    /// Regresa los títulos de la cola, no las rutas: una cola que muestra
    /// nombres de archivo no le dice nada a nadie.
    pub fn play_album(&mut self, album_id: i64, device: &str, bit_perfect: bool) -> Vec<String> {
        let disponibles: Vec<LibraryTrack> =
            self.album_tracks(album_id).into_iter().filter(|t| t.present).collect();
        if disponibles.is_empty() {
            self.now.error = "ese álbum no tiene pistas disponibles".into();
            return Vec::new();
        }
        let pistas: Vec<std::path::PathBuf> =
            disponibles.iter().map(|t| std::path::PathBuf::from(&t.path)).collect();
        let player = self.player.get_or_insert_with(chilpa_core::spawn_player);
        self.now = NowPlaying { playing: true, queue_len: pistas.len(), ..NowPlaying::default() };
        player.play_all(&pistas, device, bit_perfect);
        disponibles.into_iter().map(|t| t.title).collect()
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

    /// Recuerda la última carpeta abierta.
    pub fn remember_folder(&mut self, folder: &str) {
        let mut s = chilpa_core::settings::load();
        s.last_folder = folder.to_string();
        let _ = chilpa_core::settings::save(&s);
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
        let player = self.player.get_or_insert_with(chilpa_core::spawn_player);
        self.now = NowPlaying {
            path: path.display().to_string(),
            playing: true,
            ..NowPlaying::default()
        };
        player.play(path, device, bit_perfect);
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
        let player = self.player.get_or_insert_with(chilpa_core::spawn_player);
        self.now = NowPlaying {
            playing: true,
            queue_len: pistas.len(),
            ..NowPlaying::default()
        };
        player.play_all(&pistas, device, bit_perfect);
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
                    self.now.index = index;
                    self.now.queue_len = queue_len;
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
                PlayerEvent::Progress { frames } => {
                    self.now.frames = frames;
                    if self.sample_rate > 0 {
                        self.now.seconds = frames as f64 / f64::from(self.sample_rate);
                    }
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
