//! Las listas: guardadas, inteligentes y por género.
//!
//! Una lista guardada tiene pistas; una inteligente tiene reglas y se
//! resuelve cada vez que se mira. Las dos salen por la misma puerta.

use super::*;

impl Chilpa {
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
}
