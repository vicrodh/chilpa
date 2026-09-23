//! El puente entre QML y la fachada.
//!
//! Aquí no vive lógica: el objeto expone propiedades que QML lee y órdenes
//! que QML dispara, y cada una es una línea que llama a `chilpa-api`
//! (ADR-001). Si una vista necesita algo que no está en la fachada, se amplía
//! la fachada, no este archivo.

use cxx_qt::CxxQtType as _;
use cxx_qt_lib::{QList, QString, QStringList};
use std::pin::Pin;

#[cxx_qt::bridge]
pub mod chilpa_bridge {
    extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        /// Las salidas que se pueden elegir, ya con etiqueta para mostrar.
        #[qproperty(QStringList, outputs)]
        /// Los identificadores, en el mismo orden que las etiquetas.
        #[qproperty(QStringList, output_ids)]
        /// Lo que suena y su estado.
        #[qproperty(QString, track)]
        #[qproperty(QString, title)]
        #[qproperty(QString, artist)]
        #[qproperty(QString, album)]
        /// La portada como URL de archivo, lista para un Image de QML.
        #[qproperty(QString, artwork)]
        /// Las pistas encoladas y cuál suena.
        #[qproperty(QStringList, queue)]
        #[qproperty(i32, queue_index)]
        /// La biblioteca: un renglón por álbum, ya listo para la vista.
        /// Cada entrada es
        /// "id\ttítulo\tartista\taño\tpistas\tportada\tescuchas".
        #[qproperty(QStringList, albums)]
        /// Los resultados de la última búsqueda: un renglón por pista, con
        /// "ruta, título, artista, álbum, duración".
        #[qproperty(QStringList, results)]
        #[qproperty(QString, library_status)]
        /// Lo que se recordó de la última sesión.
        #[qproperty(QString, remembered_output)]
        #[qproperty(bool, remembered_bit_perfect)]
        #[qproperty(QString, format)]
        #[qproperty(QString, error)]
        #[qproperty(bool, playing)]
        #[qproperty(bool, paused)]
        #[qproperty(bool, bit_perfect)]
        #[qproperty(f64, seconds)]
        #[qproperty(f64, total_seconds)]
        /// Lo que se pidió reproducir al arrancar, si se pasó por la línea
        /// de comandos. Vacío cuando no.
        #[qproperty(QString, autoplay_path)]
        #[qproperty(QString, autoplay_device)]
        /// Una búsqueda de arranque, para probar la vista sin teclear.
        #[qproperty(QString, initial_search)]
        type Chilpa = super::ChilpaRust;

        /// Guarda la salida y la política elegidas.
        #[qinvokable]
        fn remember(self: Pin<&mut Chilpa>, device: &QString, bit_perfect: bool);

        /// Vuelve a leer las salidas de la máquina.
        #[qinvokable]
        fn refresh_outputs(self: Pin<&mut Chilpa>);

        /// Pone a sonar un archivo en la salida elegida.
        #[qinvokable]
        fn play(self: Pin<&mut Chilpa>, path: &QString, device: &QString, bit_perfect: bool);

        /// Busca en la biblioteca.
        #[qinvokable]
        fn search(self: Pin<&mut Chilpa>, text: &QString);

        /// Pone a sonar lo que encontró la búsqueda, desde esa posición.
        #[qinvokable]
        fn play_results(self: Pin<&mut Chilpa>, from: i32, device: &QString, bit_perfect: bool);

        /// Escanea una carpeta hacia la biblioteca y refresca los álbumes.
        #[qinvokable]
        fn scan_folder(self: Pin<&mut Chilpa>, folder: &QString);

        /// Vuelve a leer los álbumes de la biblioteca.
        #[qinvokable]
        fn refresh_albums(self: Pin<&mut Chilpa>);

        /// Pone a sonar un álbum completo.
        #[qinvokable]
        fn play_album(self: Pin<&mut Chilpa>, album_id: i64, device: &QString, bit_perfect: bool);

        /// Encola una carpeta entera.
        #[qinvokable]
        fn play_folder(self: Pin<&mut Chilpa>, folder: &QString, device: &QString, bit_perfect: bool);

        /// Pausa o reanuda. No se llama set_paused: ese nombre ya lo usa el
        /// asignador que genera la propiedad `paused`.
        #[qinvokable]
        fn pause(self: Pin<&mut Chilpa>, paused: bool);

        /// Salta a ese segundo de lo que suena.
        #[qinvokable]
        fn seek(self: Pin<&mut Chilpa>, seconds: f64);

        /// Para lo que esté sonando.
        #[qinvokable]
        fn stop(self: Pin<&mut Chilpa>);

        /// Recoge los eventos del reproductor y actualiza las propiedades.
        /// QML lo llama desde un temporizador.
        #[qinvokable]
        fn poll(self: Pin<&mut Chilpa>);
    }
}

use chilpa_bridge::Chilpa;

/// El estado del lado de Rust.
pub struct ChilpaRust {
    outputs: QStringList,
    output_ids: QStringList,
    track: QString,
    title: QString,
    artist: QString,
    album: QString,
    artwork: QString,
    queue: QStringList,
    queue_index: i32,
    albums: QStringList,
    results: QStringList,
    result_paths: Vec<String>,
    library_status: QString,
    remembered_output: QString,
    remembered_bit_perfect: bool,
    format: QString,
    error: QString,
    playing: bool,
    paused: bool,
    bit_perfect: bool,
    seconds: f64,
    total_seconds: f64,
    autoplay_path: QString,
    autoplay_device: QString,
    initial_search: QString,
    api: chilpa_api::Chilpa,
}

impl Default for ChilpaRust {
    fn default() -> Self {
        let recordado = chilpa_api::Chilpa::new().settings();
        Self {
            outputs: QStringList::default(),
            output_ids: QStringList::default(),
            track: QString::from(""),
            title: QString::from(""),
            artist: QString::from(""),
            album: QString::from(""),
            artwork: QString::from(""),
            queue: QStringList::default(),
            queue_index: 0,
            albums: QStringList::default(),
            results: QStringList::default(),
            result_paths: Vec::new(),
            library_status: QString::from(""),
            remembered_output: QString::from(&recordado.output),
            remembered_bit_perfect: recordado.bit_perfect,
            format: QString::from(""),
            error: QString::from(""),
            playing: false,
            paused: false,
            bit_perfect: false,
            seconds: 0.0,
            total_seconds: 0.0,
            // La ventana se puede arrancar apuntando a un archivo, para
            // probarla sin tocar nada: chilpa-qt <archivo> [salida].
            autoplay_path: QString::from(&std::env::args().nth(1).unwrap_or_default()),
            autoplay_device: QString::from(&std::env::args().nth(2).unwrap_or_default()),
            initial_search: QString::from(&std::env::var("CHILPA_BUSCAR").unwrap_or_default()),
            api: chilpa_api::Chilpa::new(),
        }
    }
}

impl Chilpa {
    pub fn refresh_outputs(mut self: Pin<&mut Self>) {
        let salidas = self.api.outputs();
        let mut etiquetas = QList::<QString>::default();
        let mut ids = QList::<QString>::default();
        for salida in &salidas {
            etiquetas.append(QString::from(&salida.label));
            ids.append(QString::from(&salida.id));
        }
        self.as_mut().set_outputs(QStringList::from(&etiquetas));
        self.as_mut().set_output_ids(QStringList::from(&ids));
    }

    pub fn play(mut self: Pin<&mut Self>, path: &QString, device: &QString, bit_perfect: bool) {
        let ruta = path.to_string();
        // QML entrega la ruta de un diálogo de archivo como URL.
        let ruta = ruta.strip_prefix("file://").unwrap_or(&ruta).to_string();
        let dispositivo = device.to_string();
        self.as_mut().set_error(QString::from(""));
        self.as_mut().set_track(QString::from(&ruta));
        self.as_mut().set_playing(true);
        self.as_mut()
            .rust_mut()
            .api
            .play_file(std::path::Path::new(&ruta), &dispositivo, bit_perfect);
    }

    pub fn scan_folder(mut self: Pin<&mut Self>, folder: &QString) {
        let ruta = folder.to_string();
        let ruta = ruta.strip_prefix("file://").unwrap_or(&ruta).to_string();
        let resumen = self.as_mut().rust_mut().api.scan(std::path::Path::new(&ruta));
        let texto = if resumen.unreachable {
            format!("no se pudo leer {ruta}; no se tocó nada")
        } else {
            format!(
                "{} archivos: {} leídos, {} sin cambios, {} ausentes",
                resumen.seen, resumen.read, resumen.unchanged, resumen.missing
            )
        };
        self.as_mut().set_library_status(QString::from(&texto));
        self.as_mut().refresh_albums();
    }

    pub fn search(mut self: Pin<&mut Self>, text: &QString) {
        let texto = text.to_string();
        let encontrados = self.as_mut().rust_mut().api.search(&texto, 200);
        let mut lista = QList::<QString>::default();
        let mut rutas = Vec::with_capacity(encontrados.len());
        for t in &encontrados {
            let segundos = t.duration_ms / 1000;
            lista.append(QString::from(&format!(
                "{}\t{}\t{}\t{}\t{}:{:02}",
                t.path,
                t.title,
                t.artist,
                t.album,
                segundos / 60,
                segundos % 60
            )));
            rutas.push(t.path.clone());
        }
        self.as_mut().set_results(QStringList::from(&lista));
        self.as_mut().rust_mut().result_paths = rutas;
    }

    pub fn play_results(mut self: Pin<&mut Self>, from: i32, device: &QString, bit_perfect: bool) {
        let dispositivo = device.to_string();
        let desde = from.max(0) as usize;
        let rutas: Vec<String> = self
            .as_mut()
            .rust_mut()
            .result_paths
            .iter()
            .skip(desde)
            .cloned()
            .collect();
        let titulos = self.as_mut().rust_mut().api.play_tracks(&rutas, &dispositivo, bit_perfect);
        let mut lista = QList::<QString>::default();
        for t in &titulos {
            lista.append(QString::from(t));
        }
        self.as_mut().set_queue(QStringList::from(&lista));
        self.as_mut().set_queue_index(0);
        self.as_mut().set_playing(!titulos.is_empty());
    }

    pub fn refresh_albums(mut self: Pin<&mut Self>) {
        let albumes = self.as_mut().rust_mut().api.albums(500, 0);
        let mut lista = QList::<QString>::default();
        for a in &albumes {
            let año = a.year.map(|y| y.to_string()).unwrap_or_default();
            // Un renglón por álbum, separado por tabuladores: QML lo parte y
            // arma su modelo sin que el puente invente un tipo nuevo.
            let renglon = format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}",
                a.id, a.title, a.artist, año, a.track_count, a.artwork_path, a.play_count
            );
            lista.append(QString::from(&renglon));
        }
        self.as_mut().set_albums(QStringList::from(&lista));
    }

    pub fn play_album(mut self: Pin<&mut Self>, album_id: i64, device: &QString, bit_perfect: bool) {
        let dispositivo = device.to_string();
        let pistas = self.as_mut().rust_mut().api.play_album(album_id, &dispositivo, bit_perfect);
        let mut lista = QList::<QString>::default();
        for pista in &pistas {
            lista.append(QString::from(pista));
        }
        self.as_mut().set_queue(QStringList::from(&lista));
        self.as_mut().set_queue_index(0);
        self.as_mut().set_playing(!pistas.is_empty());
    }

    /// Encola una carpeta entera y la echa a andar.
    pub fn play_folder(mut self: Pin<&mut Self>, folder: &QString, device: &QString, bit_perfect: bool) {
        let ruta = folder.to_string();
        let ruta = ruta.strip_prefix("file://").unwrap_or(&ruta).to_string();
        let dispositivo = device.to_string();
        let pistas = self
            .as_mut()
            .rust_mut()
            .api
            .play_folder(std::path::Path::new(&ruta), &dispositivo, bit_perfect);
        let mut lista = QList::<QString>::default();
        for pista in &pistas {
            lista.append(QString::from(pista));
        }
        self.as_mut().set_queue(QStringList::from(&lista));
        self.as_mut().set_queue_index(0);
        self.as_mut().set_playing(!pistas.is_empty());
    }

    pub fn remember(mut self: Pin<&mut Self>, device: &QString, bit_perfect: bool) {
        let id = device.to_string();
        self.as_mut().rust_mut().api.remember(&id, bit_perfect);
    }

    pub fn pause(mut self: Pin<&mut Self>, paused: bool) {
        self.as_mut().rust_mut().api.set_paused(paused);
        self.as_mut().set_paused(paused);
    }

    pub fn seek(mut self: Pin<&mut Self>, seconds: f64) {
        self.as_mut().rust_mut().api.seek(seconds);
    }

    pub fn stop(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().api.stop();
    }

    pub fn poll(mut self: Pin<&mut Self>) {
        let ahora = self.as_mut().rust_mut().api.poll().clone();
        self.as_mut().set_playing(ahora.playing);
        self.as_mut().set_paused(ahora.paused);
        self.as_mut().set_seconds(ahora.seconds);
        self.as_mut().set_total_seconds(ahora.total_seconds);
        self.as_mut().set_bit_perfect(ahora.bit_perfect);
        self.as_mut().set_format(QString::from(&ahora.format));
        self.as_mut().set_title(QString::from(&ahora.title));
        self.as_mut().set_artist(QString::from(&ahora.artist));
        self.as_mut().set_album(QString::from(&ahora.album));
        let portada = if ahora.artwork_path.is_empty() {
            String::new()
        } else {
            format!("file://{}", ahora.artwork_path)
        };
        self.as_mut().set_artwork(QString::from(&portada));
        self.as_mut().set_queue_index(ahora.index as i32);
        self.as_mut().set_error(QString::from(&ahora.error));
        if !ahora.path.is_empty() {
            self.as_mut().set_track(QString::from(&ahora.path));
        }
    }
}
