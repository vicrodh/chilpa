//! La fachada de Chilpa: lo único que un frontend puede usar (ADR-001).
//!
//! Un shell manda [`Command`]s y recibe [`Event`]s. No ve el core, ni la
//! biblioteca, ni el player. Si una vista necesita algo que no está aquí,
//! primero se amplía esta API.

pub use chilpa_core::queue_playback::Repeat;
pub use chilpa_library::{AlbumSort, Stats};

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

    /// Arranca un escaneo en segundo plano. La interfaz no se bloquea.
    pub fn start_scan(&mut self, folder: &std::path::Path) {
        if self.scan.is_some() {
            self.now.error = "ya hay un escaneo corriendo".into();
            return;
        }
        // La conexión de la interfaz se suelta: el escaneo abre la suya, y
        // dos conexiones a la misma base en WAL conviven sin pelearse.
        self.scan = Some(chilpa_library::ScanJob::start(
            chilpa_library::Library::default_path(),
            folder.to_path_buf(),
            Some(chilpa_core::artwork_cache()),
        ));
        self.scan_status = ScanStatus { running: true, ..ScanStatus::default() };
    }

    /// Vuelve a escanear todas las carpetas que ya están en la biblioteca.
    ///
    /// Es lo normal después del primer día: la música vive donde vive, y
    /// volver a buscarla a mano cada vez es trabajo del usuario que la
    /// biblioteca ya tiene anotado.
    pub fn rescan(&mut self) {
        if self.scan.is_some() {
            self.now.error = "ya hay un escaneo corriendo".into();
            return;
        }
        let carpetas = self.roots();
        if carpetas.is_empty() {
            self.now.error = "todavía no hay ninguna carpeta en la biblioteca".into();
            return;
        }
        self.scan = Some(chilpa_library::ScanJob::start_all(
            chilpa_library::Library::default_path(),
            carpetas,
            Some(chilpa_core::artwork_cache()),
        ));
        self.scan_status = ScanStatus { running: true, ..ScanStatus::default() };
    }

    /// Arranca el análisis de lo que falte, en segundo plano.
    pub fn start_analysis(&mut self) {
        if self.analysis.is_some() {
            self.now.error = "ya hay un análisis corriendo".into();
            return;
        }
        self.analysis = Some(chilpa_core::analysis_job::AnalysisJob::start(
            chilpa_library::Library::default_path(),
        ));
        self.analysis_status = AnalysisStatus { running: true, ..AnalysisStatus::default() };
    }

    /// Pide que el análisis se detenga.
    pub fn cancel_analysis(&mut self) {
        if let Some(j) = &self.analysis {
            j.cancel();
        }
    }

    /// Cómo va el análisis.
    pub fn analysis_status(&mut self) -> &AnalysisStatus {
        use chilpa_core::analysis_job::AnalysisProgress;
        let eventos = self.analysis.as_ref().map(|j| j.poll()).unwrap_or_default();
        let mut termino = false;
        for evento in eventos {
            match evento {
                AnalysisProgress::Counted { total } => self.analysis_status.total = total,
                AnalysisProgress::Advanced { done, total, title, bpm } => {
                    self.analysis_status.done = done;
                    self.analysis_status.total = total;
                    self.analysis_status.last = match bpm {
                        Some(b) => format!("{title} · {b:.0} BPM"),
                        None => format!("{title} · sin pulso"),
                    };
                }
                AnalysisProgress::Done { analyzed, without_pulse, failed } => {
                    self.analysis_status.running = false;
                    self.analysis_status.analyzed = analyzed;
                    self.analysis_status.without_pulse = without_pulse;
                    self.analysis_status.failed = failed;
                    termino = true;
                }
                AnalysisProgress::Failed(e) => {
                    self.analysis_status.running = false;
                    self.now.error = format!("el análisis se detuvo: {e}");
                    termino = true;
                }
            }
        }
        if termino {
            self.analysis = None;
        }
        &self.analysis_status
    }

    /// Las carpetas que forman la biblioteca.
    pub fn roots(&mut self) -> Vec<std::path::PathBuf> {
        let Some(lib) = self.library() else { return Vec::new() };
        lib.roots().unwrap_or_default().into_iter().map(|r| r.path).collect()
    }

    /// Cómo va el escaneo. Al terminar, refresca lo que la vista muestra.
    pub fn scan_status(&mut self) -> &ScanStatus {
        let eventos = self.scan.as_ref().map(|j| j.poll()).unwrap_or_default();
        for evento in eventos {
            match evento {
                chilpa_library::ScanProgress::Counted { total } => {
                    self.scan_status.total = total;
                }
                chilpa_library::ScanProgress::Advanced { done, total } => {
                    self.scan_status.done = done;
                    self.scan_status.total = total;
                }
                chilpa_library::ScanProgress::Done(r) => {
                    self.scan_status = ScanStatus {
                        running: false,
                        done: r.seen,
                        total: r.seen,
                        summary: ScanSummary {
                            seen: r.seen,
                            read: r.read,
                            unchanged: r.unchanged,
                            missing: r.missing,
                            unreachable: r.unreachable,
                        },
                    };
                    self.scan = None;
                }
                chilpa_library::ScanProgress::Failed(e) => {
                    self.now.error = format!("el escaneo falló: {e}");
                    self.scan_status.running = false;
                    self.scan = None;
                }
            }
        }
        &self.scan_status
    }

    /// Escanea una carpeta hacia la biblioteca, esperando a que termine.
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
        let vista = self.album_sort;
        self.albums_sorted(vista, limit, offset)
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

    /// Los géneros de la biblioteca, del más común al menos.
    pub fn genres(&mut self, limit: usize) -> Vec<(String, usize)> {
        let Some(lib) = self.library() else { return Vec::new() };
        let mut g = lib.genres().unwrap_or_default();
        g.truncate(limit);
        g
    }

    /// Pone a sonar un género, revuelto.
    pub fn play_genre(&mut self, genre: &str, device: &str, bit_perfect: bool) -> Vec<String> {
        let lista = chilpa_library::SmartList::new(
            genre,
            vec![chilpa_library::Rule::GenreIs(genre.to_string())],
            chilpa_library::SmartOrder::Random,
            300,
        );
        let Some(lib) = self.library() else { return Vec::new() };
        let rutas: Vec<String> = lib
            .smart_tracks(&lista)
            .unwrap_or_default()
            .into_iter()
            .map(|t| t.external_id)
            .collect();
        if rutas.is_empty() {
            self.now.error = format!("no hay nada de {genre}");
            return Vec::new();
        }
        self.play_tracks(&rutas, device, bit_perfect)
    }

    /// Las listas inteligentes que Chilpa trae de fábrica, con cuántas
    /// pistas les salen hoy.
    ///
    /// Una lista inteligente no guarda pistas: guarda reglas y se vuelve a
    /// resolver cada vez. Por eso la cuenta cambia sola.
    pub fn smart_lists(&mut self) -> Vec<SmartListInfo> {
        let Some(lib) = self.library() else { return Vec::new() };
        chilpa_library::smart_builtins()
            .into_iter()
            .map(|l| {
                let n = lib.smart_tracks(&l).map(|t| t.len()).unwrap_or(0);
                SmartListInfo { name: l.name, track_count: n }
            })
            .collect()
    }

    /// Pone a sonar una lista inteligente, por su nombre.
    pub fn play_smart_list(
        &mut self,
        name: &str,
        device: &str,
        bit_perfect: bool,
    ) -> Vec<String> {
        let Some(lista) = chilpa_library::smart_builtins().into_iter().find(|l| l.name == name)
        else {
            self.now.error = format!("no conozco la lista \"{name}\"");
            return Vec::new();
        };
        let Some(lib) = self.library() else { return Vec::new() };
        let rutas: Vec<String> = lib
            .smart_tracks(&lista)
            .unwrap_or_default()
            .into_iter()
            .map(|t| t.external_id)
            .collect();
        if rutas.is_empty() {
            self.now.error = format!("\"{name}\" no trae nada hoy");
            return Vec::new();
        }
        self.play_tracks(&rutas, device, bit_perfect)
    }

    /// Las listas guardadas, las de aquí y las que traiga un source.
    pub fn playlists(&mut self) -> Vec<Playlist> {
        let Some(lib) = self.library() else { return Vec::new() };
        lib.playlists()
            .unwrap_or_default()
            .into_iter()
            .map(|p| Playlist {
                id: p.id,
                name: p.name,
                provider: p.provider,
                track_count: p.track_count,
            })
            .collect()
    }

    /// Guarda la cola como una lista con ese nombre.
    ///
    /// Regresa el id de la lista. Lo que no esté en la biblioteca se guarda
    /// igual, por su ruta: la lista vale aunque falte escanear.
    pub fn save_queue_as(&mut self, name: &str) -> Option<i64> {
        let nombre = name.trim().to_string();
        if nombre.is_empty() {
            self.now.error = "una lista necesita un nombre".into();
            return None;
        }
        if self.cola.is_empty() {
            self.now.error = "no hay cola que guardar".into();
            return None;
        }
        let rutas = self.cola.clone();
        let lib = self.library()?;
        let source = lib.source_id("local", "default").ok()?;
        let entradas: Vec<chilpa_library::PlaylistEntry> = rutas
            .iter()
            .map(|r| chilpa_library::PlaylistEntry {
                track_id: lib
                    .track_by_path(source, std::path::Path::new(r))
                    .ok()
                    .flatten(),
                external_id: r.clone(),
            })
            .collect();
        let id = lib.create_playlist(source, &nombre, "").ok()?;
        lib.set_playlist_entries(id, &entradas).ok()?;
        Some(id)
    }

    /// Pone a sonar una lista guardada.
    pub fn play_saved_playlist(
        &mut self,
        playlist_id: i64,
        device: &str,
        bit_perfect: bool,
    ) -> Vec<String> {
        let Some(lib) = self.library() else { return Vec::new() };
        let rutas: Vec<String> = lib
            .playlist_entries(playlist_id)
            .unwrap_or_default()
            .into_iter()
            .map(|e| e.external_id)
            .filter(|r| !r.is_empty())
            .collect();
        if rutas.is_empty() {
            self.now.error = "esa lista está vacía".into();
            return Vec::new();
        }
        self.play_tracks(&rutas, device, bit_perfect)
    }

    /// Borra una lista. Las pistas no se tocan.
    pub fn delete_playlist(&mut self, playlist_id: i64) {
        if let Some(lib) = self.library() {
            let _ = lib.delete_playlist(playlist_id);
        }
    }

    /// Cambia el nombre de una lista.
    pub fn rename_playlist(&mut self, playlist_id: i64, name: &str) {
        let nombre = name.trim().to_string();
        if nombre.is_empty() {
            self.now.error = "una lista necesita un nombre".into();
            return;
        }
        if let Some(lib) = self.library() {
            let _ = lib.rename_playlist(playlist_id, &nombre);
        }
    }

    /// El resumen de lo escuchado.
    pub fn stats(&mut self) -> Stats {
        let Some(lib) = self.library() else { return Stats::default() };
        lib.stats().unwrap_or_default()
    }

    /// Las pistas más escuchadas.
    pub fn top_tracks(&mut self, limit: usize) -> Vec<(LibraryTrack, usize)> {
        let Some(lib) = self.library() else { return Vec::new() };
        lib.top_tracks(limit)
            .unwrap_or_default()
            .into_iter()
            .map(|t| (pista_de(t.track), t.count))
            .collect()
    }

    /// Lo último que sonó, una pista por renglón.
    pub fn recently_played(&mut self, limit: usize) -> Vec<(LibraryTrack, usize)> {
        let Some(lib) = self.library() else { return Vec::new() };
        lib.recently_played(limit)
            .unwrap_or_default()
            .into_iter()
            .map(|t| (pista_de(t.track), t.count))
            .collect()
    }

    /// Los artistas de la biblioteca.
    pub fn artists(&mut self, limit: usize, offset: usize) -> Vec<Artist> {
        let Some(lib) = self.library() else { return Vec::new() };
        lib.artists(limit, offset)
            .unwrap_or_default()
            .into_iter()
            .map(|a| Artist {
                name: a.name,
                album_count: a.album_count,
                track_count: a.track_count,
                artwork_path: a.artwork_path.unwrap_or_default(),
            })
            .collect()
    }

    /// El artista que se está mirando, vacío si se ven todos.
    pub fn open_artist(&self) -> &str {
        &self.artista
    }

    /// Filtra la reja a un artista. Un nombre vacío quita el filtro.
    pub fn set_open_artist(&mut self, name: &str) {
        self.artista = name.to_string();
    }

    /// La vista rápida con la que se está mirando la biblioteca.
    pub fn album_sort(&self) -> AlbumSort {
        self.album_sort
    }

    /// Cambia la vista y la recuerda para la próxima sesión.
    pub fn set_album_sort(&mut self, orden: AlbumSort) {
        self.album_sort = orden;
        let mut s = chilpa_core::settings::load();
        s.album_sort = orden.name().to_string();
        let _ = chilpa_core::settings::save(&s);
    }

    /// Los álbumes en el orden que se pida, con su cuenta de escuchas.
    pub fn albums_sorted(
        &mut self,
        orden: AlbumSort,
        limit: usize,
        offset: usize,
    ) -> Vec<Album> {
        let artista = self.artista.clone();
        let filtro = (!artista.is_empty()).then_some(artista.as_str());
        let Some(lib) = self.library() else { return Vec::new() };
        lib.albums_filtered(orden, filtro, limit, offset)
            .unwrap_or_default()
            .into_iter()
            .map(|(a, escuchas)| Album {
                id: a.id,
                title: a.title,
                artist: a.album_artist,
                year: a.year,
                track_count: a.track_count,
                artwork_path: a.artwork_path.unwrap_or_default(),
                play_count: escuchas,
            })
            .collect()
    }

    /// Un álbum por su id, con su portada y su cuenta de escuchas.
    pub fn album(&mut self, album_id: i64) -> Option<Album> {
        let lib = self.library()?;
        let (a, escuchas) = lib.album(album_id).ok().flatten()?;
        Some(Album {
            id: a.id,
            title: a.title,
            artist: a.album_artist,
            year: a.year,
            track_count: a.track_count,
            artwork_path: a.artwork_path.unwrap_or_default(),
            play_count: escuchas,
        })
    }

    /// Las pistas de un álbum, en el orden del disco.
    pub fn album_tracks(&mut self, album_id: i64) -> Vec<LibraryTrack> {
        let Some(lib) = self.library() else { return Vec::new() };
        lib.album_tracks(album_id).unwrap_or_default().into_iter().map(pista_de).collect()
    }

    /// Busca en la biblioteca por título, artista o álbum.
    pub fn search(&mut self, texto: &str, limit: usize) -> Vec<LibraryTrack> {
        if texto.trim().is_empty() {
            return Vec::new();
        }
        let Some(lib) = self.library() else { return Vec::new() };
        lib.search(texto.trim(), limit).unwrap_or_default().into_iter().map(pista_de).collect()
    }

    /// Echa a andar la cola que ya está en `self.cola`, desde esa posición.
    ///
    /// Todo lo que reproduce una lista pasa por aquí: así la repetición, el
    /// aleatorio y la cuenta de posiciones se aplican en un solo lugar.
    fn lanzar(&mut self, desde: usize, device: &str, bit_perfect: bool) {
        let pistas: Vec<std::path::PathBuf> =
            self.cola.iter().skip(desde).map(std::path::PathBuf::from).collect();
        if pistas.is_empty() {
            return;
        }
        self.base = desde;
        self.quitadas = vec![false; self.cola.len()];
        let (shuffle, repeat, total) = (self.shuffle, self.repeat, self.cola.len());
        let tap = self.tap.clone();
        let player = self
            .player
            .get_or_insert_with(move || chilpa_core::spawn_player_with(tap));
        player.set_repeat(repeat);
        if shuffle {
            player.shuffle(pistas.len());
        } else {
            player.clear_order();
        }
        self.now = NowPlaying { playing: true, queue_len: total, ..NowPlaying::default() };
        player.play_all(&pistas, device, bit_perfect);
    }

    /// Pone a sonar lo que encontró una búsqueda, en ese orden.
    pub fn play_tracks(&mut self, paths: &[String], device: &str, bit_perfect: bool) -> Vec<String> {
        let pistas: Vec<std::path::PathBuf> =
            paths.iter().map(std::path::PathBuf::from).collect();
        if pistas.is_empty() {
            return Vec::new();
        }
        self.cola = pistas.iter().map(|p| p.display().to_string()).collect();
        self.lanzar(0, device, bit_perfect);
        pistas.iter().map(|p| titulo_de(p)).collect()
    }

    /// Los títulos de la cola, en el orden en que se encoló.
    pub fn queue_titles(&self) -> Vec<String> {
        self.cola.iter().map(|r| titulo_de(std::path::Path::new(r))).collect()
    }

    /// Qué posiciones de la cola se quitaron.
    pub fn queue_removed(&self) -> &[bool] {
        &self.quitadas
    }

    /// Quita de la cola la pista en esa posición.
    ///
    /// La que suena no se quita: para esa está el botón de siguiente. Y una
    /// que ya pasó tampoco, porque no cambiaría nada.
    pub fn remove_from_queue(&mut self, index: usize) -> bool {
        if index <= self.now.index || index >= self.cola.len() {
            return false;
        }
        let Some(p) = &self.player else { return false };
        // El reproductor cuenta desde donde arrancó; la vista, desde el
        // principio de la cola.
        if !p.remove_from_queue(index - self.base) {
            return false;
        }
        if let Some(q) = self.quitadas.get_mut(index) {
            *q = true;
        }
        true
    }

    /// Para y vacía la cola.
    pub fn clear_queue(&mut self) {
        self.stop();
        self.cola.clear();
        self.quitadas.clear();
        self.base = 0;
        self.now.queue_len = 0;
    }

    /// Agrega pistas al final de la cola. Si no hay nada sonando, arranca.
    ///
    /// Encolar no corta lo que suena: la cola le crece por atrás y lo
    /// agregado entra cuando le toca.
    pub fn enqueue(&mut self, paths: &[String], device: &str, bit_perfect: bool) -> Vec<String> {
        let pistas: Vec<std::path::PathBuf> =
            paths.iter().map(std::path::PathBuf::from).collect();
        if pistas.is_empty() {
            return Vec::new();
        }
        let sonando = self.now.playing && self.player.is_some();
        if !sonando {
            return self.play_tracks(paths, device, bit_perfect);
        }
        self.cola.extend(pistas.iter().map(|p| p.display().to_string()));
        self.quitadas.resize(self.cola.len(), false);
        if let Some(p) = &self.player {
            p.enqueue(&pistas);
        }
        self.now.queue_len = self.cola.len();
        pistas.iter().map(|p| titulo_de(p)).collect()
    }

    /// Encola un álbum completo.
    pub fn enqueue_album(&mut self, album_id: i64, device: &str, bit_perfect: bool) -> Vec<String> {
        let rutas: Vec<String> = self
            .album_tracks(album_id)
            .into_iter()
            .filter(|t| t.present)
            .map(|t| t.path)
            .collect();
        self.enqueue(&rutas, device, bit_perfect)
    }

    /// Si la cola suena barajada.
    pub fn shuffle(&self) -> bool {
        self.shuffle
    }

    /// Baraja la cola o la devuelve a su orden. Toma efecto al terminar la
    /// pista que suena: barajar no corta el audio.
    pub fn set_shuffle(&mut self, shuffle: bool) {
        self.shuffle = shuffle;
        let restantes = self.cola.len().saturating_sub(self.base);
        if let Some(p) = &self.player {
            if shuffle {
                p.shuffle(restantes);
            } else {
                p.clear_order();
            }
        }
    }

    pub fn repeat(&self) -> Repeat {
        self.repeat
    }

    /// Cambia la repetición. También toma efecto al terminar la pista.
    pub fn set_repeat(&mut self, repeat: Repeat) {
        self.repeat = repeat;
        if let Some(p) = &self.player {
            p.set_repeat(repeat);
        }
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

    /// Abre una lista M3U y la pone a sonar.
    ///
    /// Regresa los títulos encolados; lo que la lista menciona y ya no está
    /// se reporta en el error, en vez de desaparecer sin decir nada.
    pub fn play_playlist(
        &mut self,
        path: &std::path::Path,
        device: &str,
        bit_perfect: bool,
    ) -> Vec<String> {
        let lista = match chilpa_library::playlist::read(path) {
            Ok(l) => l,
            Err(e) => {
                self.now.error = format!("no se pudo leer la lista: {e}");
                return Vec::new();
            }
        };
        if !lista.missing.is_empty() {
            self.now.error = format!(
                "{} de las pistas de la lista ya no están",
                lista.missing.len()
            );
        }
        if lista.tracks.is_empty() {
            if self.now.error.is_empty() {
                self.now.error = "la lista no trae pistas".into();
            }
            return Vec::new();
        }
        let rutas: Vec<String> = lista.tracks.iter().map(|p| p.display().to_string()).collect();
        let error = std::mem::take(&mut self.now.error);
        let titulos = self.play_tracks(&rutas, device, bit_perfect);
        self.now.error = error;
        titulos
    }

    /// Guarda la cola actual como una lista M3U.
    pub fn save_queue(&mut self, path: &std::path::Path) -> bool {
        if self.cola.is_empty() {
            self.now.error = "no hay cola que guardar".into();
            return false;
        }
        let pistas: Vec<std::path::PathBuf> =
            self.cola.iter().map(std::path::PathBuf::from).collect();
        match chilpa_library::playlist::write_m3u(path, &pistas) {
            Ok(()) => true,
            Err(e) => {
                self.now.error = format!("no se pudo guardar la lista: {e}");
                false
            }
        }
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
        self.cola = pistas.iter().map(|p| p.display().to_string()).collect();
        self.lanzar(0, device, bit_perfect);
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
