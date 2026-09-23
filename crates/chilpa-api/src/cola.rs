//! La cola: qué suena, en qué orden y qué sigue.
//!
//! Todo lo que echa a andar una lista pasa por `lanzar`, para que la
//! repetición, el aleatorio y la cuenta de posiciones se apliquen en un solo
//! lugar.

use super::*;

impl Chilpa {
    /// Echa a andar la cola que ya está en `self.cola`, desde esa posición.
    ///
    /// Todo lo que reproduce una lista pasa por aquí: así la repetición, el
    /// aleatorio y la cuenta de posiciones se aplican en un solo lugar.
    pub(crate) fn lanzar(&mut self, desde: usize, device: &str, bit_perfect: bool) {
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

    /// Reordena la cola para que el tempo cambie poco de una pista a otra.
    ///
    /// Es lo que en otros lados se llama automix: se arranca en la que
    /// suena y cada vez se toma la más cercana en BPM de las que quedan. Lo
    /// que no tiene análisis va al final, en su orden: mezclar a ciegas no
    /// es mezclar.
    ///
    /// Regresa cuántas pistas entraron en la mezcla.
    pub fn automix(&mut self) -> usize {
        if self.cola.len() < 3 {
            self.now.error = "hace falta más cola para mezclar".into();
            return 0;
        }
        // Las posiciones dentro de lo que el reproductor tiene, no dentro de
        // la cola entera.
        let rutas: Vec<String> = self.cola.iter().skip(self.base).cloned().collect();
        let actual = self.now.index.saturating_sub(self.base);

        let Some(lib) = self.library() else { return 0 };
        let source = match lib.source_id("local", "default") {
            Ok(s) => s,
            Err(_) => return 0,
        };
        let bpms: Vec<Option<f32>> = rutas
            .iter()
            .map(|r| {
                let id = lib.track_by_path(source, std::path::Path::new(r)).ok().flatten()?;
                lib.track_bpm(id, chilpa_core::analysis_job::ANALIZADOR).ok().flatten()
            })
            .collect();

        let con_tempo = bpms.iter().filter(|b| b.is_some()).count();
        if con_tempo < 3 {
            self.now.error = "casi nada de la cola tiene tempo analizado".into();
            return 0;
        }

        let orden = orden_por_tempo(&bpms, actual.min(rutas.len().saturating_sub(1)));
        if let Some(p) = &self.player {
            p.set_order(orden, rutas.len());
        }
        // Mezclar y barajar son dos formas de lo mismo: no pueden estar las
        // dos encendidas.
        self.shuffle = false;
        con_tempo
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
}
