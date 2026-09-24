//! El pulso: recoger lo que pasó y acomodarlo en las propiedades.
//!
//! QML lo llama desde un temporizador. Todo lo que corre en segundo plano
//! —la reproducción, el escaneo, el análisis, la búsqueda, conectar una
//! cuenta— se recoge aquí y en ningún otro lado.

use super::Chilpa;
use cxx_qt::CxxQtType as _;
use cxx_qt_lib::{QList, QString, QStringList};
use std::pin::Pin;

impl Chilpa {

    pub fn poll(mut self: Pin<&mut Self>) {
        // El análisis avanza solo; lo único que hace falta es contarlo.
        if self.analyzing {
            let state = self.as_mut().rust_mut().api.analysis_status().clone();
            let progress = if state.total > 0 {
                state.done as f64 / state.total as f64
            } else {
                0.0
            };
            self.as_mut().set_analysis_progress(progress);
            if state.running {
                self.as_mut().set_analysis_status(QString::from(&format!(
                    "{} de {} · {}",
                    state.done, state.total, state.last
                )));
            } else {
                self.as_mut().set_analyzing(false);
                self.as_mut().set_analysis_progress(1.0);
                self.as_mut().set_analysis_status(QString::from(&format!(
                    "{} analizadas, {} sin pulso, {} sin leer",
                    state.analyzed, state.without_pulse, state.failed
                )));
                // Las listas de tempo ya tienen con qué llenarse.
                self.as_mut().refresh_playlists();
            }
        }

        // El escaneo primero: al terminar refresca los álbumes.
        if self.scanning {
            let state = self.as_mut().rust_mut().api.scan_status().clone();
            let progress = if state.total > 0 {
                state.done as f64 / state.total as f64
            } else {
                0.0
            };
            self.as_mut().set_scan_progress(progress);
            if !state.running {
                let r = &state.summary;
                let text = if r.unreachable {
                    "no se pudo leer esa carpeta; no se tocó nada".to_string()
                } else {
                    format!(
                        "{} archivos: {} leídos, {} sin cambios, {} ausentes",
                        r.seen, r.read, r.unchanged, r.missing
                    )
                };
                self.as_mut().set_library_status(QString::from(&text));
                self.as_mut().set_scanning(false);
                self.as_mut().set_scan_progress(1.0);
                self.as_mut().refresh_albums();
                // Los géneros y las cuentas de las listas cambian con lo que
                // acaba de entrar.
                self.as_mut().refresh_playlists();
            }
        }

        // Lo que sale, para los visualizadores. Solo mientras suena: leer
        // el círculo con el reproductor quieto es trabajo tirado.
        if self.playing {
            let envelope = self.api.scope_envelope(120);
            let mut e = QList::<QString>::default();
            for v in &envelope {
                e.append(QString::from(&format!("{v:.4}")));
            }
            self.as_mut().set_scope(QStringList::from(&e));

            let spectrum = self.api.scope_spectrum(32);
            let mut b = QList::<QString>::default();
            for v in &spectrum {
                b.append(QString::from(&format!("{v:.4}")));
            }
            self.as_mut().set_spectrum(QStringList::from(&b));

            let pares = self.api.scope_pairs(400);
            let mut pl = QList::<QString>::default();
            for (i, d) in &pares {
                pl.append(QString::from(&format!("{i:.4}\t{d:.4}")));
            }
            self.as_mut().set_pairs(QStringList::from(&pl));
            let correlation = self.api.scope_correlation();
            self.as_mut().set_correlation(f64::from(correlation.unwrap_or(999.0)));

            let levels = self.api.scope_levels();
            let mut l = QList::<QString>::default();
            for (pico, rms) in &levels {
                l.append(QString::from(&format!("{pico:.4}\t{rms:.4}")));
            }
            self.as_mut().set_levels(QStringList::from(&l));
        }

        let now = self.as_mut().rust_mut().api.poll().clone();
        self.as_mut().set_playing(now.playing);
        self.as_mut().set_paused(now.paused);
        self.as_mut().set_seconds(now.seconds);
        self.as_mut().set_total_seconds(now.total_seconds);
        self.as_mut().set_bit_perfect(now.bit_perfect);
        self.as_mut().set_format(QString::from(&now.format));
        self.as_mut().set_title(QString::from(&now.title));
        self.as_mut().set_artist(QString::from(&now.artist));
        self.as_mut().set_album(QString::from(&now.album));
        let cover = if now.artwork_path.is_empty() {
            String::new()
        } else {
            format!("file://{}", now.artwork_path)
        };
        self.as_mut().set_artwork(QString::from(&cover));
        self.as_mut().set_queue_index(now.index as i32);
        // La letra solo se vuelve a armar cuando cambia: rehacer una lista
        // de QString sesenta veces por minuto es tirar trabajo.
        if self.lyrics_of != now.path {
            let mut list = QList::<QString>::default();
            for l in &now.lyrics {
                list.append(QString::from(l));
            }
            self.as_mut().set_lyrics(QStringList::from(&list));
            self.as_mut().set_lyrics_synced(now.lyrics_synced);
            self.as_mut().set_lyrics_source(QString::from(&now.lyrics_source));
            self.as_mut().rust_mut().lyrics_of = now.path.clone();
        }
        self.as_mut().set_lyrics_line(now.lyrics_line);
        self.as_mut().set_analysis(QString::from(&now.analysis));
        self.as_mut().set_error(QString::from(&now.error));
        if !now.path.is_empty() {
            self.as_mut().set_track(QString::from(&now.path));
        }

        // Una cuenta que termina de conectarse cambia la lista de plugins,
        // y el usuario está mirando esa pestaña esperándolo.
        if self.as_mut().rust_mut().api.connecting() != self.connecting_shown {
            let waiting = self.as_mut().rust_mut().api.connecting();
            self.as_mut().rust_mut().connecting_shown = waiting;
            self.as_mut().refresh_plugins();
            if !waiting {
                self.as_mut().set_library_status(QString::from(""));
            }
        }

        // Cuando llega lo de un source, la lista se vuelve a armar: es lo
        // que hace que los resultados aparezcan solos. Se pregunta si
        // cambiaron y no si la consulta sigue viva, porque puede empezar y
        // acabar entre dos pulsos.
        if self.as_mut().rust_mut().api.search_changed() {
            self.as_mut().refresh_results();
        }
    }
}
