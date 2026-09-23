//! La biblioteca vista desde la fachada.
//!
//! Escanear, analizar y preguntarle a la base. Todo lo que aquí regresa ya
//! viene en los tipos de la vista: quien dibuja no conoce a rusqlite.

use super::*;

impl Chilpa {
    /// Le pega a cada pista lo que el análisis sacó, en una sola consulta.
    fn with_analysis(&mut self, mut tracks: Vec<LibraryTrack>) -> Vec<LibraryTrack> {
        if tracks.is_empty() {
            return tracks;
        }
        let ids: Vec<i64> = tracks.iter().map(|t| t.id).collect();
        let Some(lib) = self.library() else { return tracks };
        let Ok(map) = lib.features_for(&ids, chilpa_core::analysis_job::ANALYZER) else {
            return tracks;
        };
        for track in tracks.iter_mut() {
            let Some((bpm, tone, loudness)) = map.get(&track.id) else { continue };
            track.analysis = describe_analysis(*bpm, tone.as_deref(), *loudness);
        }
        tracks
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
        let folders = self.roots();
        if folders.is_empty() {
            self.now.error = "todavía no hay ninguna carpeta en la biblioteca".into();
            return;
        }
        self.scan = Some(chilpa_library::ScanJob::start_all(
            chilpa_library::Library::default_path(),
            folders,
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
        let events = self.analysis.as_ref().map(|j| j.poll()).unwrap_or_default();
        let mut finished = false;
        for event in events {
            match event {
                AnalysisProgress::Counted { total } => self.analysis_status.total = total,
                AnalysisProgress::Advanced { done, total, title, bpm, key } => {
                    self.analysis_status.done = done;
                    self.analysis_status.total = total;
                    let tempo = match bpm {
                        Some(b) => format!("{b:.0} BPM"),
                        None => "sin pulso".to_string(),
                    };
                    self.analysis_status.last = match key {
                        Some(k) => format!("{title} · {tempo} · {k}"),
                        None => format!("{title} · {tempo}"),
                    };
                }
                AnalysisProgress::Done { analyzed, without_pulse, failed } => {
                    self.analysis_status.running = false;
                    self.analysis_status.analyzed = analyzed;
                    self.analysis_status.without_pulse = without_pulse;
                    self.analysis_status.failed = failed;
                    finished = true;
                }
                AnalysisProgress::Failed(e) => {
                    self.analysis_status.running = false;
                    self.now.error = format!("el análisis se detuvo: {e}");
                    finished = true;
                }
            }
        }
        if finished {
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
        let events = self.scan.as_ref().map(|j| j.poll()).unwrap_or_default();
        for event in events {
            match event {
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

    /// Los géneros de la biblioteca, del más común al menos.
    pub fn genres(&mut self, limit: usize) -> Vec<(String, usize)> {
        let Some(lib) = self.library() else { return Vec::new() };
        let mut g = lib.genres().unwrap_or_default();
        g.truncate(limit);
        g
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
            .map(|t| (track_of(t.track), t.count))
            .collect()
    }

    /// Lo último que sonó, una pista por renglón.
    pub fn recently_played(&mut self, limit: usize) -> Vec<(LibraryTrack, usize)> {
        let Some(lib) = self.library() else { return Vec::new() };
        lib.recently_played(limit)
            .unwrap_or_default()
            .into_iter()
            .map(|t| (track_of(t.track), t.count))
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
        &self.artist
    }

    /// Filtra la reja a un artista. Un nombre vacío quita el filtro.
    pub fn set_open_artist(&mut self, name: &str) {
        self.artist = name.to_string();
    }

    /// La vista rápida con la que se está mirando la biblioteca.
    pub fn album_sort(&self) -> AlbumSort {
        self.album_sort
    }

    /// Cambia la vista y la recuerda para la próxima sesión.
    pub fn set_album_sort(&mut self, order: AlbumSort) {
        self.album_sort = order;
        let mut s = chilpa_core::settings::load();
        s.album_sort = order.name().to_string();
        let _ = chilpa_core::settings::save(&s);
    }

    /// Los álbumes en el orden que se pida, con su cuenta de escuchas.
    pub fn albums_sorted(
        &mut self,
        order: AlbumSort,
        limit: usize,
        offset: usize,
    ) -> Vec<Album> {
        let artist = self.artist.clone();
        let filter = (!artist.is_empty()).then_some(artist.as_str());
        let Some(lib) = self.library() else { return Vec::new() };
        lib.albums_filtered(order, filter, limit, offset)
            .unwrap_or_default()
            .into_iter()
            .map(|(a, listens)| Album {
                id: a.id,
                title: a.title,
                artist: a.album_artist,
                year: a.year,
                track_count: a.track_count,
                artwork_path: a.artwork_path.unwrap_or_default(),
                play_count: listens,
            })
            .collect()
    }

    /// Un álbum por su id, con su portada y su cuenta de escuchas.
    pub fn album(&mut self, album_id: i64) -> Option<Album> {
        let lib = self.library()?;
        let (a, listens) = lib.album(album_id).ok().flatten()?;
        Some(Album {
            id: a.id,
            title: a.title,
            artist: a.album_artist,
            year: a.year,
            track_count: a.track_count,
            artwork_path: a.artwork_path.unwrap_or_default(),
            play_count: listens,
        })
    }

    /// Las pistas de un álbum, en el orden del disco.
    pub fn album_tracks(&mut self, album_id: i64) -> Vec<LibraryTrack> {
        let Some(lib) = self.library() else { return Vec::new() };
        let tracks: Vec<LibraryTrack> =
            lib.album_tracks(album_id).unwrap_or_default().into_iter().map(track_of).collect();
        self.with_analysis(tracks)
    }

    /// Busca en la biblioteca por título, artista o álbum.
    pub fn search(&mut self, text: &str, limit: usize) -> Vec<LibraryTrack> {
        if text.trim().is_empty() {
            return Vec::new();
        }
        let Some(lib) = self.library() else { return Vec::new() };
        let tracks: Vec<LibraryTrack> =
            lib.search(text.trim(), limit).unwrap_or_default().into_iter().map(track_of).collect();
        self.with_analysis(tracks)
    }
}
