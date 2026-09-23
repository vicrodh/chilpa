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
        /// Qué posiciones de la cola se quitaron, como "0" o "1".
        #[qproperty(QStringList, queue_removed)]
        /// La biblioteca: un renglón por álbum, ya listo para la vista.
        /// Cada entrada es
        /// "id\ttítulo\tartista\taño\tpistas\tportada\tescuchas".
        #[qproperty(QStringList, albums)]
        /// Los resultados de la última búsqueda o del álbum abierto: un
        /// renglón por pista, con "ruta, título, artista, álbum, duración y
        /// lo que el análisis sacó".
        #[qproperty(QStringList, results)]
        /// El álbum que se está mirando, vacío si no hay ninguno abierto.
        #[qproperty(QString, open_album)]
        #[qproperty(i64, open_album_id)]
        /// Lo demás del álbum abierto, para su encabezado.
        #[qproperty(QString, open_album_artist)]
        #[qproperty(QString, open_album_art)]
        #[qproperty(QString, open_album_line)]
        #[qproperty(QString, library_status)]
        /// Cómo va el análisis: de 0 a 1, si corre, y qué se está mirando.
        #[qproperty(f64, analysis_progress)]
        #[qproperty(bool, analyzing)]
        #[qproperty(QString, analysis_status)]
        /// Cómo va el escaneo: de 0 a 1, y si sigue corriendo.
        #[qproperty(f64, scan_progress)]
        #[qproperty(bool, scanning)]
        /// El layout de la ventana. Es el nombre de un `Layout<nombre>.qml`,
        /// para que agregar un acomodo sea agregar un archivo.
        #[qproperty(QString, layout)]
        /// Los acomodos que trae el binario. La lista la arma `build.rs` a
        /// partir de los archivos, no se escribe a mano.
        #[qproperty(QStringList, layouts)]
        /// Lo que hay escrito en la búsqueda. Vive aquí porque decide qué se
        /// ve, y la caja donde se escribe cambia de lugar con cada layout.
        #[qproperty(QString, search_text)]
        /// Lo que el análisis dice de lo que suena: "128 BPM · 8B".
        #[qproperty(QString, analysis)]
        /// La letra de lo que suena: un renglón por entrada, qué renglón
        /// va (-1 si ninguno) y de dónde salió.
        #[qproperty(QStringList, lyrics)]
        #[qproperty(i32, lyrics_line)]
        #[qproperty(bool, lyrics_synced)]
        #[qproperty(QString, lyrics_source)]
        /// La última carpeta que se abrió, para que los diálogos no
        /// empiecen en el home cada vez.
        #[qproperty(QString, last_folder)]
        /// La envolvente de lo que sale: un valor de 0 a 1 por punto.
        #[qproperty(QStringList, scope)]
        /// Qué visualizador se dibuja: "onda" o "espectro".
        #[qproperty(QString, visualizer)]
        /// El espectro de lo que sale: un valor de 0 a 1 por banda.
        #[qproperty(QStringList, spectrum)]
        /// Pares "izquierdo\tderecho" para el goniómetro, y la correlación
        /// entre canales de -1 a 1 (999 cuando no hay dos canales).
        #[qproperty(QStringList, pairs)]
        #[qproperty(f64, correlation)]
        /// El nivel por canal, como "pico\trms".
        #[qproperty(QStringList, levels)]
        /// Los plugins instalados, como "id\tnombre\testado\tproblema".
        #[qproperty(QStringList, plugins)]
        /// El diagnóstico de la salida: una línea de resumen y un renglón
        /// por formato, como "nombre\tformato\tveredicto".
        #[qproperty(QString, wizard_summary)]
        #[qproperty(QStringList, wizard_rows)]
        /// Las unidades de CD que hay en la máquina.
        #[qproperty(QStringList, cd_drives)]
        /// Los géneros de la biblioteca, como "nombre\tpistas".
        #[qproperty(QStringList, genres)]
        /// Las listas inteligentes, como "nombre\tpistas".
        #[qproperty(QStringList, smart_lists)]
        /// Las listas guardadas, como "id\tnombre\tde dónde\tpistas".
        #[qproperty(QStringList, playlists)]
        /// El resumen de lo escuchado, ya en renglones de "etiqueta\tvalor".
        #[qproperty(QStringList, stats)]
        /// Las más escuchadas y lo último que sonó, como
        /// "título\tartista\tálbum\tveces\truta".
        #[qproperty(QStringList, top_tracks)]
        #[qproperty(QStringList, recent_tracks)]
        /// Los artistas de la biblioteca: un renglón con
        /// "nombre\tdiscos\tpistas\tportada".
        #[qproperty(QStringList, artists)]
        /// El artista al que está filtrada la reja, vacío si se ven todos.
        #[qproperty(QString, open_artist)]
        /// Las vistas rápidas de la biblioteca y cuál está puesta.
        #[qproperty(QStringList, album_views)]
        #[qproperty(i32, album_view, READ = album_view, WRITE = choose_album_view, NOTIFY = album_view_changed)]
        /// Cómo se recorre la cola: barajada o no, y la repetición como
        /// 0 = ninguna, 1 = toda la cola, 2 = la pista.
        #[qproperty(bool, shuffle, READ = shuffle, WRITE = shuffle_queue, NOTIFY = shuffle_changed)]
        #[qproperty(i32, repeat)]
        /// Lo elegido para reproducir: la salida y si se exige bit-perfect.
        /// Vive aquí, no en un control de la ventana, para que cualquier
        /// layout lo lea sin depender de dónde quedó el selector.
        #[qproperty(QString, device)]
        #[qproperty(bool, want_bit_perfect)]
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
        /// Un álbum de arranque, por id, para probar la vista sin ratón.
        #[qproperty(i64, initial_album)]
        /// Con qué pestaña de la biblioteca arranca: "artistas" o vacío.
        #[qproperty(QString, initial_tab)]
        /// Cierto para volver a escanear al arrancar, sin tocar un botón.
        #[qproperty(bool, initial_rescan)]
        /// Cierto para analizar al arrancar, igual de útil para probar.
        #[qproperty(bool, initial_analysis)]
        type Chilpa = super::ChilpaRust;

        /// Elige el visualizador y lo recuerda.
        #[qinvokable]
        fn choose_visualizer(self: Pin<&mut Chilpa>, name: &QString);

        /// Elige el layout de la ventana y lo recuerda.
        #[qinvokable]
        fn choose_layout(self: Pin<&mut Chilpa>, name: &QString);

        /// Elige la salida y la política, y las recuerda para la próxima vez.
        #[qinvokable]
        fn choose(self: Pin<&mut Chilpa>, device: &QString, bit_perfect: bool);

        /// Vuelve a leer las salidas de la máquina.
        #[qinvokable]
        fn refresh_outputs(self: Pin<&mut Chilpa>);

        /// Pone a sonar un archivo en la salida elegida.
        #[qinvokable]
        fn play(self: Pin<&mut Chilpa>, path: &QString);

        /// Busca en la biblioteca.
        #[qinvokable]
        fn search(self: Pin<&mut Chilpa>, text: &QString);

        /// Pone a sonar lo que encontró la búsqueda, desde esa posición.
        #[qinvokable]
        fn play_results(self: Pin<&mut Chilpa>, from: i32);

        /// Arranca el análisis de lo que falte, en segundo plano.
        #[qinvokable]
        fn analyze_library(self: Pin<&mut Chilpa>);

        /// Pide que el análisis se detenga.
        #[qinvokable]
        fn cancel_analysis(self: Pin<&mut Chilpa>);

        /// Vuelve a escanear las carpetas que ya están en la biblioteca.
        #[qinvokable]
        fn rescan(self: Pin<&mut Chilpa>);

        /// Escanea una carpeta hacia la biblioteca y refresca los álbumes.
        #[qinvokable]
        fn scan_folder(self: Pin<&mut Chilpa>, folder: &QString);

        /// El lector y el asignador de `album_view`: cambiar la vista
        /// vuelve a leer la biblioteca en ese orden.
        #[cxx_name = "albumView"]
        fn album_view(self: &Chilpa) -> i32;
        #[cxx_name = "setAlbumView"]
        fn choose_album_view(self: Pin<&mut Chilpa>, view: i32);

        #[qsignal]
        #[cxx_name = "albumViewChanged"]
        fn album_view_changed(self: Pin<&mut Chilpa>);

        /// Vuelve a leer las listas guardadas.
        #[qinvokable]
        fn refresh_playlists(self: Pin<&mut Chilpa>);

        /// Pone a sonar un género, revuelto.
        #[qinvokable]
        fn play_genre(self: Pin<&mut Chilpa>, genre: &QString);

        /// Pone a sonar una lista inteligente, por su nombre.
        #[qinvokable]
        fn play_smart_list(self: Pin<&mut Chilpa>, name: &QString);

        /// Guarda la cola como una lista con ese nombre.
        #[qinvokable]
        fn save_queue_as(self: Pin<&mut Chilpa>, name: &QString);

        /// Pone a sonar una lista guardada.
        #[qinvokable]
        fn play_saved_playlist(self: Pin<&mut Chilpa>, playlist_id: i64);

        /// Borra una lista guardada. Las pistas no se tocan.
        #[qinvokable]
        fn delete_playlist(self: Pin<&mut Chilpa>, playlist_id: i64);

        /// Vuelve a leer las estadísticas de escucha.
        #[qinvokable]
        fn refresh_stats(self: Pin<&mut Chilpa>);

        /// Vuelve a leer los artistas de la biblioteca.
        #[qinvokable]
        fn refresh_artists(self: Pin<&mut Chilpa>);

        /// Filtra la reja a un artista. Un nombre vacío quita el filtro.
        #[qinvokable]
        fn open_artist_albums(self: Pin<&mut Chilpa>, name: &QString);

        /// Vuelve a leer los álbumes de la biblioteca.
        #[qinvokable]
        fn refresh_albums(self: Pin<&mut Chilpa>);

        /// Abre un álbum: muestra sus pistas sin reproducirlas.
        #[qinvokable]
        fn open_album_tracks(self: Pin<&mut Chilpa>, album_id: i64, title: &QString);

        /// Cierra el álbum abierto.
        #[qinvokable]
        fn close_album(self: Pin<&mut Chilpa>);

        /// Encola lo que encontró la búsqueda, desde esa posición.
        #[qinvokable]
        fn enqueue_results(self: Pin<&mut Chilpa>, from: i32);

        /// Encola un álbum completo, sin cortar lo que suena.
        #[qinvokable]
        fn enqueue_album(self: Pin<&mut Chilpa>, album_id: i64);

        /// Pone a sonar un álbum completo.
        #[qinvokable]
        fn play_album(self: Pin<&mut Chilpa>, album_id: i64);

        /// Pone a sonar el CD que haya en esa unidad.
        #[qinvokable]
        fn play_cd(self: Pin<&mut Chilpa>, drive: &QString);

        /// Carga los plugins que haya y vuelve a leer la lista.
        #[qinvokable]
        fn load_plugins(self: Pin<&mut Chilpa>);

        /// Saca un plugin de cuarentena. Solo lo pide el usuario.
        #[qinvokable]
        fn reactivate_plugin(self: Pin<&mut Chilpa>, id: &QString);

        /// Mide la salida elegida y dice qué formatos salen tal cual.
        #[qinvokable]
        fn run_wizard(self: Pin<&mut Chilpa>);

        /// Vuelve a mirar qué unidades de CD hay.
        #[qinvokable]
        fn refresh_cd_drives(self: Pin<&mut Chilpa>);

        /// Pone a sonar una radio o cualquier stream por HTTP.
        #[qinvokable]
        fn play_url(self: Pin<&mut Chilpa>, url: &QString);

        /// Abre una lista M3U y la pone a sonar.
        #[qinvokable]
        fn play_playlist(self: Pin<&mut Chilpa>, path: &QString);

        /// Guarda la cola actual como lista M3U.
        #[qinvokable]
        fn save_queue(self: Pin<&mut Chilpa>, path: &QString);

        /// Encola una carpeta entera.
        #[qinvokable]
        fn play_folder(self: Pin<&mut Chilpa>, folder: &QString);

        /// El lector y el asignador de `shuffle`: QML escribe la propiedad
        /// y el asignador la aplica en la fachada.
        #[cxx_name = "shuffle"]
        fn shuffle(self: &Chilpa) -> bool;
        #[cxx_name = "setShuffle"]
        fn shuffle_queue(self: Pin<&mut Chilpa>, shuffle: bool);

        #[qsignal]
        #[cxx_name = "shuffleChanged"]
        fn shuffle_changed(self: Pin<&mut Chilpa>);

        /// Cambia la repetición: 0 ninguna, 1 toda la cola, 2 la pista.
        #[qinvokable]
        fn cycle_repeat(self: Pin<&mut Chilpa>);

        /// Reordena la cola para que el tempo cambie poco entre pistas.
        #[qinvokable]
        fn automix(self: Pin<&mut Chilpa>);

        /// Quita de la cola la pista en esa posición.
        #[qinvokable]
        fn remove_from_queue(self: Pin<&mut Chilpa>, index: i32);

        /// Para y vacía la cola.
        #[qinvokable]
        fn clear_queue(self: Pin<&mut Chilpa>);

        /// Salta a la siguiente pista de la cola.
        #[qinvokable]
        fn next_track(self: Pin<&mut Chilpa>);

        /// Vuelve a la anterior; en la primera, la reinicia.
        #[qinvokable]
        fn previous_track(self: Pin<&mut Chilpa>);

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

/// Un tiempo largo dicho como se dice: "3 h 21 min", no 12 060 000 ms.
fn long_duration(ms: u64) -> String {
    let minutes = ms / 60_000;
    if minutes == 0 {
        // Un disco de prueba de seis segundos no dura "0 min".
        return format!("{} s", ms / 1000);
    }
    let (hours, rest) = (minutes / 60, minutes % 60);
    if hours == 0 {
        return format!("{minutes} min");
    }
    format!("{hours} h {rest} min")
}

/// Una fecha unix en año-mes-día, sin arrastrar una biblioteca de fechas.
fn short_date(unix: i64) -> String {
    // Días desde 1970 a fecha civil (Howard Hinnant). Son cuatro líneas y
    // evitan una dependencia para mostrar un renglón.
    let dias = unix.div_euclid(86_400) + 719_468;
    let era = dias.div_euclid(146_097);
    let doe = dias - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Las vistas rápidas de la biblioteca, con su nombre para la lista.
const VISTAS: &[(chilpa_api::AlbumSort, &str)] = &[
    (chilpa_api::AlbumSort::Artist, "Por artista"),
    (chilpa_api::AlbumSort::Title, "Por título"),
    (chilpa_api::AlbumSort::Year, "Por año"),
    (chilpa_api::AlbumSort::Added, "Agregados"),
    (chilpa_api::AlbumSort::Played, "Más escuchados"),
    (chilpa_api::AlbumSort::LastPlayed, "Escuchados hace poco"),
];

/// La lista de acomodos, generada por `build.rs` a partir de los QML.
mod layouts {
    include!(concat!(env!("OUT_DIR"), "/layouts.rs"));
}

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
    queue_removed: QStringList,
    albums: QStringList,
    results: QStringList,
    result_paths: Vec<String>,
    open_album: QString,
    open_album_id: i64,
    open_album_artist: QString,
    open_album_art: QString,
    open_album_line: QString,
    library_status: QString,
    analysis_progress: f64,
    analyzing: bool,
    analysis_status: QString,
    scan_progress: f64,
    scanning: bool,
    layout: QString,
    layouts: QStringList,
    analysis: QString,
    lyrics: QStringList,
    /// De qué pista es la letra que está armada, para no rearmarla en cada
    /// pulso.
    lyrics_of: String,
    lyrics_line: i32,
    lyrics_synced: bool,
    lyrics_source: QString,
    last_folder: QString,
    scope: QStringList,
    visualizer: QString,
    spectrum: QStringList,
    pairs: QStringList,
    correlation: f64,
    levels: QStringList,
    plugins: QStringList,
    wizard_summary: QString,
    wizard_rows: QStringList,
    cd_drives: QStringList,
    genres: QStringList,
    smart_lists: QStringList,
    playlists: QStringList,
    stats: QStringList,
    top_tracks: QStringList,
    recent_tracks: QStringList,
    artists: QStringList,
    open_artist: QString,
    album_views: QStringList,
    album_view: i32,
    shuffle: bool,
    repeat: i32,
    search_text: QString,
    device: QString,
    want_bit_perfect: bool,
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
    initial_album: i64,
    initial_tab: QString,
    initial_rescan: bool,
    initial_analysis: bool,
    api: chilpa_api::Chilpa,
}

impl Default for ChilpaRust {
    fn default() -> Self {
        let remembered = chilpa_api::Chilpa::new().settings();
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
            queue_removed: QStringList::default(),
            albums: QStringList::default(),
            results: QStringList::default(),
            result_paths: Vec::new(),
            open_album: QString::from(""),
            open_album_id: 0,
            open_album_artist: QString::from(""),
            open_album_art: QString::from(""),
            open_album_line: QString::from(""),
            library_status: QString::from(""),
            analysis_progress: 0.0,
            analyzing: false,
            analysis_status: QString::from(""),
            scan_progress: 0.0,
            scanning: false,
            analysis: QString::from(""),
            lyrics: QStringList::default(),
            lyrics_of: String::new(),
            lyrics_line: -1,
            lyrics_synced: false,
            lyrics_source: QString::from(""),
            last_folder: QString::from(&remembered.last_folder),
            scope: QStringList::default(),
            visualizer: QString::from(&remembered.visualizer),
            spectrum: QStringList::default(),
            pairs: QStringList::default(),
            correlation: 999.0,
            levels: QStringList::default(),
            plugins: QStringList::default(),
            wizard_summary: QString::from(""),
            wizard_rows: QStringList::default(),
            cd_drives: QStringList::default(),
            genres: QStringList::default(),
            smart_lists: QStringList::default(),
            playlists: QStringList::default(),
            stats: QStringList::default(),
            top_tracks: QStringList::default(),
            recent_tracks: QStringList::default(),
            artists: QStringList::default(),
            open_artist: QString::from(""),
            album_views: {
                let mut l = QList::<QString>::default();
                for v in VISTAS {
                    l.append(QString::from(v.1));
                }
                QStringList::from(&l)
            },
            album_view: {
                let actual = chilpa_api::Chilpa::new().album_sort();
                VISTAS.iter().position(|v| v.0 == actual).unwrap_or(0) as i32
            },
            shuffle: false,
            repeat: 0,
            layout: QString::from(&remembered.layout),
            layouts: {
                let mut l = QList::<QString>::default();
                for n in layouts::LAYOUTS {
                    l.append(QString::from(*n));
                }
                QStringList::from(&l)
            },
            search_text: QString::from(""),
            device: QString::from(&remembered.output),
            want_bit_perfect: remembered.bit_perfect,
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
            initial_album: std::env::var("CHILPA_ALBUM").ok().and_then(|v| v.parse().ok()).unwrap_or(0),
            initial_tab: QString::from(&std::env::var("CHILPA_PESTANA").unwrap_or_default()),
            initial_rescan: std::env::var("CHILPA_RESCAN").is_ok(),
            initial_analysis: std::env::var("CHILPA_ANALIZAR").is_ok(),
            api: chilpa_api::Chilpa::new(),
        }
    }
}

impl Chilpa {
    pub fn refresh_outputs(mut self: Pin<&mut Self>) {
        let outputs = self.api.outputs();
        let mut tags = QList::<QString>::default();
        let mut ids = QList::<QString>::default();
        for output in &outputs {
            tags.append(QString::from(&output.label));
            ids.append(QString::from(&output.id));
        }
        self.as_mut().set_outputs(QStringList::from(&tags));
        self.as_mut().set_output_ids(QStringList::from(&ids));
    }

    pub fn play(mut self: Pin<&mut Self>, path: &QString) {
        let path = path.to_string();
        // QML entrega la ruta de un diálogo de archivo como URL.
        let path = path.strip_prefix("file://").unwrap_or(&path).to_string();
        let (device, bit_perfect) = self.as_mut().chosen();
        self.as_mut().set_error(QString::from(""));
        self.as_mut().set_track(QString::from(&path));
        self.as_mut().set_playing(true);
        self.as_mut()
            .rust_mut()
            .api
            .play_file(std::path::Path::new(&path), &device, bit_perfect);
        // Un archivo suelto también es una cola de una pista: la vista tiene
        // que poder guardarla y vaciarla como cualquier otra.
        let queue = self.as_mut().rust_mut().api.queue_titles();
        self.as_mut().refresh_queue(&queue);
        self.as_mut().set_queue_index(0);
        self.as_mut().remember_folder(&path);
    }

    pub fn scan_folder(mut self: Pin<&mut Self>, folder: &QString) {
        let path = folder.to_string();
        let path = path.strip_prefix("file://").unwrap_or(&path).to_string();
        // El escaneo corre en su propio hilo: una biblioteca grande dejaría
        // la ventana congelada si se hiciera aquí.
        self.as_mut().remember_folder(&format!("{path}/."));
        self.as_mut().rust_mut().api.start_scan(std::path::Path::new(&path));
        self.as_mut().set_scanning(true);
        self.as_mut().set_scan_progress(0.0);
        self.as_mut().set_library_status(QString::from("escaneando..."));
    }

    pub fn choose_visualizer(mut self: Pin<&mut Self>, name: &QString) {
        let owned = name.to_string();
        self.as_mut().set_visualizer(name.clone());
        self.as_mut().rust_mut().api.remember_visualizer(&owned);
    }

    pub fn choose_layout(mut self: Pin<&mut Self>, name: &QString) {
        let owned = name.to_string();
        self.as_mut().set_layout(name.clone());
        self.as_mut().rust_mut().api.remember_layout(&owned);
    }

    pub fn analyze_library(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().api.start_analysis();
        self.as_mut().set_analyzing(true);
        self.as_mut().set_analysis_progress(0.0);
        self.as_mut().set_analysis_status(QString::from("analizando..."));
    }

    pub fn cancel_analysis(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().api.cancel_analysis();
    }

    pub fn rescan(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().api.rescan();
        self.as_mut().set_scanning(true);
        self.as_mut().set_scan_progress(0.0);
        self.as_mut().set_library_status(QString::from("escaneando..."));
    }

    pub fn search(mut self: Pin<&mut Self>, text: &QString) {
        let owned = text.to_string();
        self.as_mut().set_search_text(text.clone());
        let found = self.as_mut().rust_mut().api.search(&owned, 200);
        let mut list = QList::<QString>::default();
        let mut paths = Vec::with_capacity(found.len());
        for t in &found {
            let seconds = t.duration_ms / 1000;
            list.append(QString::from(&format!(
                "{}\t{}\t{}\t{}\t{}:{:02}\t{}",
                t.path,
                t.title,
                t.artist,
                t.album,
                seconds / 60,
                seconds % 60,
                t.analysis
            )));
            paths.push(t.path.clone());
        }
        self.as_mut().set_results(QStringList::from(&list));
        self.as_mut().rust_mut().result_paths = paths;
    }

    pub fn play_results(mut self: Pin<&mut Self>, from: i32) {
        let (device, bit_perfect) = self.as_mut().chosen();
        let from = from.max(0) as usize;
        let paths: Vec<String> = self
            .as_mut()
            .rust_mut()
            .result_paths
            .iter()
            .skip(from)
            .cloned()
            .collect();
        let titulos = self.as_mut().rust_mut().api.play_tracks(&paths, &device, bit_perfect);
        let mut list = QList::<QString>::default();
        for t in &titulos {
            list.append(QString::from(t));
        }
        self.as_mut().set_queue(QStringList::from(&list));
        self.as_mut().set_queue_index(0);
        self.as_mut().set_playing(!titulos.is_empty());
    }

    pub fn album_view(&self) -> i32 {
        self.album_view
    }

    /// Recuerda dónde estaba el usuario: lo que se abre es, casi siempre,
    /// vecino de lo último que se abrió.
    fn remember_folder(mut self: Pin<&mut Self>, path: &str) {
        let folder = std::path::Path::new(path)
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        if folder.is_empty() {
            return;
        }
        self.as_mut().set_last_folder(QString::from(&folder));
        self.as_mut().rust_mut().api.remember_folder(&folder);
    }

    pub fn choose_album_view(mut self: Pin<&mut Self>, view: i32) {
        let Some((order, _)) = VISTAS.get(view.max(0) as usize) else { return };
        let order = *order;
        self.as_mut().rust_mut().album_view = view;
        self.as_mut().rust_mut().api.set_album_sort(order);
        self.as_mut().album_view_changed();
        self.as_mut().refresh_albums();
    }

    pub fn refresh_playlists(mut self: Pin<&mut Self>) {
        let generos = self.as_mut().rust_mut().api.genres(40);
        let mut g = QList::<QString>::default();
        for (name, n) in &generos {
            g.append(QString::from(&format!("{name}\t{n}")));
        }
        self.as_mut().set_genres(QStringList::from(&g));

        let smart = self.as_mut().rust_mut().api.smart_lists();
        let mut out = QList::<QString>::default();
        for l in &smart {
            out.append(QString::from(&format!("{}\t{}", l.name, l.track_count)));
        }
        self.as_mut().set_smart_lists(QStringList::from(&out));

        let lists = self.as_mut().rust_mut().api.playlists();
        let mut l = QList::<QString>::default();
        for p in &lists {
            l.append(QString::from(&format!(
                "{}\t{}\t{}\t{}",
                p.id, p.name, p.provider, p.track_count
            )));
        }
        self.as_mut().set_playlists(QStringList::from(&l));
    }

    pub fn play_genre(mut self: Pin<&mut Self>, genre: &QString) {
        let name = genre.to_string();
        let (device, bit_perfect) = self.as_mut().chosen();
        let titulos = self.as_mut().rust_mut().api.play_genre(&name, &device, bit_perfect);
        self.as_mut().refresh_queue(&titulos);
        self.as_mut().set_queue_index(0);
        self.as_mut().set_playing(!titulos.is_empty());
    }

    pub fn play_smart_list(mut self: Pin<&mut Self>, name: &QString) {
        let name = name.to_string();
        let (device, bit_perfect) = self.as_mut().chosen();
        let titulos =
            self.as_mut().rust_mut().api.play_smart_list(&name, &device, bit_perfect);
        self.as_mut().refresh_queue(&titulos);
        self.as_mut().set_queue_index(0);
        self.as_mut().set_playing(!titulos.is_empty());
    }

    pub fn save_queue_as(mut self: Pin<&mut Self>, name: &QString) {
        let name = name.to_string();
        let saved = self.as_mut().rust_mut().api.save_queue_as(&name).is_some();
        if saved {
            self.as_mut()
                .set_library_status(QString::from(&format!("lista \"{name}\" guardada")));
            self.as_mut().refresh_playlists();
        }
    }

    pub fn play_saved_playlist(mut self: Pin<&mut Self>, playlist_id: i64) {
        let (device, bit_perfect) = self.as_mut().chosen();
        let titulos = self
            .as_mut()
            .rust_mut()
            .api
            .play_saved_playlist(playlist_id, &device, bit_perfect);
        self.as_mut().refresh_queue(&titulos);
        self.as_mut().set_queue_index(0);
        self.as_mut().set_playing(!titulos.is_empty());
    }

    pub fn delete_playlist(mut self: Pin<&mut Self>, playlist_id: i64) {
        self.as_mut().rust_mut().api.delete_playlist(playlist_id);
        self.as_mut().refresh_playlists();
    }

    pub fn refresh_stats(mut self: Pin<&mut Self>) {
        let s = self.as_mut().rust_mut().api.stats();
        let mut summary = QList::<QString>::default();
        let mut line = |tag: &str, valor: String| {
            summary.append(QString::from(&format!("{tag}\t{valor}")));
        };
        line("Escuchas", s.plays.to_string());
        line("Pistas distintas", s.distinct_tracks.to_string());
        line("Tiempo escuchado", long_duration(s.listened_ms));
        line("Saltos", s.skips.to_string());
        line(
            "Bit-perfect",
            if s.plays + s.skips == 0 {
                "—".to_string()
            } else {
                format!("{} de {}", s.bit_perfect, s.plays + s.skips)
            },
        );
        line("Desde", s.since.map(short_date).unwrap_or_else(|| "—".into()));
        self.as_mut().set_stats(QStringList::from(&summary));

        let to_lines = |tracks: Vec<(chilpa_api::LibraryTrack, usize)>| {
            let mut l = QList::<QString>::default();
            for (t, times) in tracks {
                l.append(QString::from(&format!(
                    "{}\t{}\t{}\t{}\t{}",
                    t.title, t.artist, t.album, times, t.path
                )));
            }
            QStringList::from(&l)
        };
        let top = self.as_mut().rust_mut().api.top_tracks(20);
        self.as_mut().set_top_tracks(to_lines(top));
        let recent = self.as_mut().rust_mut().api.recently_played(20);
        self.as_mut().set_recent_tracks(to_lines(recent));
    }

    pub fn refresh_artists(mut self: Pin<&mut Self>) {
        let artists = self.as_mut().rust_mut().api.artists(1000, 0);
        let mut list = QList::<QString>::default();
        for a in &artists {
            list.append(QString::from(&format!(
                "{}\t{}\t{}\t{}",
                a.name, a.album_count, a.track_count, a.artwork_path
            )));
        }
        self.as_mut().set_artists(QStringList::from(&list));
    }

    pub fn open_artist_albums(mut self: Pin<&mut Self>, name: &QString) {
        let owned = name.to_string();
        self.as_mut().rust_mut().api.set_open_artist(&owned);
        self.as_mut().set_open_artist(name.clone());
        self.as_mut().refresh_albums();
    }

    pub fn refresh_albums(mut self: Pin<&mut Self>) {
        let albums = self.as_mut().rust_mut().api.albums(500, 0);
        let mut list = QList::<QString>::default();
        for a in &albums {
            let año = a.year.map(|y| y.to_string()).unwrap_or_default();
            // Un renglón por álbum, separado por tabuladores: QML lo parte y
            // arma su modelo sin que el puente invente un tipo nuevo.
            let line = format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}",
                a.id, a.title, a.artist, año, a.track_count, a.artwork_path, a.play_count
            );
            list.append(QString::from(&line));
        }
        self.as_mut().set_albums(QStringList::from(&list));
    }

    pub fn open_album_tracks(mut self: Pin<&mut Self>, album_id: i64, title: &QString) {
        let tracks = self.as_mut().rust_mut().api.album_tracks(album_id);
        let mut list = QList::<QString>::default();
        let mut paths = Vec::with_capacity(tracks.len());
        for t in &tracks {
            let seconds = t.duration_ms / 1000;
            let number = t.track_number.map(|n| format!("{n}. ")).unwrap_or_default();
            list.append(QString::from(&format!(
                "{}\t{}{}\t{}\t{}\t{}:{:02}\t{}",
                t.path,
                number,
                t.title,
                t.artist,
                t.album,
                seconds / 60,
                seconds % 60,
                t.analysis
            )));
            paths.push(t.path.clone());
        }
        self.as_mut().set_results(QStringList::from(&list));
        self.as_mut().rust_mut().result_paths = paths;
        self.as_mut().set_open_album(title.clone());
        self.as_mut().set_open_album_id(album_id);

        // El encabezado del álbum: portada, artista y una línea con año,
        // pistas, duración y escuchas.
        let duration: u64 = tracks.iter().map(|t| t.duration_ms).sum();
        match self.as_mut().rust_mut().api.album(album_id) {
            Some(a) => {
                let cover = if a.artwork_path.is_empty() {
                    String::new()
                } else {
                    format!("file://{}", a.artwork_path)
                };
                let mut parts: Vec<String> = Vec::new();
                if let Some(año) = a.year {
                    parts.push(año.to_string());
                }
                parts.push(format!(
                    "{} {}",
                    a.track_count,
                    if a.track_count == 1 { "pista" } else { "pistas" }
                ));
                parts.push(long_duration(duration));
                if a.play_count > 0 {
                    parts.push(format!(
                        "{} {}",
                        a.play_count,
                        if a.play_count == 1 { "escucha" } else { "escuchas" }
                    ));
                }
                // El título sale de la biblioteca, no de quien abrió la
                // vista: así siempre dice lo que dice el disco.
                self.as_mut().set_open_album(QString::from(&a.title));
                self.as_mut().set_open_album_artist(QString::from(&a.artist));
                self.as_mut().set_open_album_art(QString::from(&cover));
                self.as_mut().set_open_album_line(QString::from(&parts.join(" · ")));
            }
            None => {
                self.as_mut().set_open_album_artist(QString::from(""));
                self.as_mut().set_open_album_art(QString::from(""));
                self.as_mut().set_open_album_line(QString::from(""));
            }
        }
    }

    pub fn close_album(mut self: Pin<&mut Self>) {
        self.as_mut().set_open_album(QString::from(""));
        self.as_mut().set_open_album_id(0);
        self.as_mut().set_open_album_artist(QString::from(""));
        self.as_mut().set_open_album_art(QString::from(""));
        self.as_mut().set_open_album_line(QString::from(""));
        self.as_mut().set_results(QStringList::default());
        self.as_mut().rust_mut().result_paths.clear();
    }

    /// Deja la cola que reporta la fachada en las propiedades de la vista.
    fn refresh_queue(mut self: Pin<&mut Self>, titulos: &[String]) {
        let mut list = QList::<QString>::default();
        for t in titulos {
            list.append(QString::from(t));
        }
        self.as_mut().set_queue(QStringList::from(&list));
        self.as_mut().refresh_removed();
    }

    /// Qué posiciones de la cola están apagadas.
    fn refresh_removed(mut self: Pin<&mut Self>) {
        let removed: Vec<bool> = self.api.queue_removed().to_vec();
        let mut l = QList::<QString>::default();
        for q in removed {
            l.append(QString::from(if q { "1" } else { "0" }));
        }
        self.as_mut().set_queue_removed(QStringList::from(&l));
    }

    pub fn remove_from_queue(mut self: Pin<&mut Self>, index: i32) {
        if index < 0 {
            return;
        }
        if self.as_mut().rust_mut().api.remove_from_queue(index as usize) {
            self.as_mut().refresh_removed();
        }
    }

    pub fn clear_queue(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().api.clear_queue();
        self.as_mut().set_queue(QStringList::default());
        self.as_mut().set_queue_removed(QStringList::default());
        self.as_mut().set_queue_index(0);
        self.as_mut().set_playing(false);
    }

    pub fn enqueue_results(mut self: Pin<&mut Self>, from: i32) {
        let (device, bit_perfect) = self.as_mut().chosen();
        let from = from.max(0) as usize;
        let paths: Vec<String> =
            self.as_mut().rust_mut().result_paths.iter().skip(from).cloned().collect();
        self.as_mut().rust_mut().api.enqueue(&paths, &device, bit_perfect);
        let queue = self.as_mut().rust_mut().api.queue_titles();
        self.as_mut().refresh_queue(&queue);
        self.as_mut().set_playing(true);
    }

    pub fn enqueue_album(mut self: Pin<&mut Self>, album_id: i64) {
        let (device, bit_perfect) = self.as_mut().chosen();
        self.as_mut().rust_mut().api.enqueue_album(album_id, &device, bit_perfect);
        let queue = self.as_mut().rust_mut().api.queue_titles();
        self.as_mut().refresh_queue(&queue);
        self.as_mut().set_playing(true);
    }

    pub fn play_album(mut self: Pin<&mut Self>, album_id: i64) {
        let (device, bit_perfect) = self.as_mut().chosen();
        let tracks = self.as_mut().rust_mut().api.play_album(album_id, &device, bit_perfect);
        let mut list = QList::<QString>::default();
        for track in &tracks {
            list.append(QString::from(track));
        }
        self.as_mut().set_queue(QStringList::from(&list));
        self.as_mut().set_queue_index(0);
        self.as_mut().set_playing(!tracks.is_empty());
    }

    pub fn load_plugins(mut self: Pin<&mut Self>) {
        let count = self.as_mut().rust_mut().api.load_plugins();
        self.as_mut().refresh_plugins();
        let total = self.as_mut().rust_mut().api.plugins().len();
        if total > 0 {
            self.as_mut().set_library_status(QString::from(&format!(
                "{count} de {total} plugins cargados"
            )));
        }
    }

    pub fn reactivate_plugin(mut self: Pin<&mut Self>, id: &QString) {
        let name = id.to_string();
        self.as_mut().rust_mut().api.reactivate_plugin(&name);
        self.as_mut().refresh_plugins();
    }

    fn refresh_plugins(mut self: Pin<&mut Self>) {
        let list = self.as_mut().rust_mut().api.plugins();
        let mut l = QList::<QString>::default();
        for p in &list {
            l.append(QString::from(&format!(
                "{}\t{}\t{}\t{}",
                p.id, p.name, p.state, p.problem
            )));
        }
        self.as_mut().set_plugins(QStringList::from(&l));
    }

    pub fn run_wizard(mut self: Pin<&mut Self>) {
        let (device, _) = self.as_mut().chosen();
        let report = self.as_mut().rust_mut().api.wizard(&device);
        let mut l = QList::<QString>::default();
        // Primero lo que el dispositivo dijo de sí mismo.
        if report.probed {
            l.append(QString::from(&format!(
                "Frecuencias\t{}\t",
                report
                    .rates
                    .iter()
                    .map(|r| r.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
            l.append(QString::from(&format!("Formatos\t{}\t", report.formats.join(", "))));
        }
        for f in &report.rows {
            l.append(QString::from(&format!("{}\t{}\t{}", f.name, f.format, f.verdict)));
        }
        self.as_mut().set_wizard_rows(QStringList::from(&l));
        self.as_mut().set_wizard_summary(QString::from(&format!(
            "{}: {}",
            report.device, report.summary
        )));
    }

    pub fn refresh_cd_drives(mut self: Pin<&mut Self>) {
        let units = self.api.cd_drives();
        let mut l = QList::<QString>::default();
        for u in &units {
            l.append(QString::from(u));
        }
        self.as_mut().set_cd_drives(QStringList::from(&l));
    }

    pub fn play_cd(mut self: Pin<&mut Self>, drive: &QString) {
        let unit = drive.to_string();
        let (device, bit_perfect) = self.as_mut().chosen();
        self.as_mut().set_error(QString::from(""));
        if self.as_mut().rust_mut().api.play_cd(&unit, &device, bit_perfect) {
            self.as_mut().set_playing(true);
        }
    }

    pub fn play_url(mut self: Pin<&mut Self>, url: &QString) {
        let address = url.to_string();
        let (device, bit_perfect) = self.as_mut().chosen();
        self.as_mut().rust_mut().api.play_url(&address, &device, bit_perfect);
        let mut list = QList::<QString>::default();
        list.append(QString::from(&address));
        self.as_mut().set_queue(QStringList::from(&list));
        self.as_mut().set_queue_index(0);
        self.as_mut().set_playing(true);
        self.as_mut().set_track(QString::from(&address));
    }

    pub fn play_playlist(mut self: Pin<&mut Self>, path: &QString) {
        let path = path.to_string();
        let path = path.strip_prefix("file://").unwrap_or(&path).to_string();
        let (device, bit_perfect) = self.as_mut().chosen();
        self.as_mut().remember_folder(&path);
        let titulos = self.as_mut().rust_mut().api.play_playlist(
            std::path::Path::new(&path),
            &device,
            bit_perfect,
        );
        let mut list = QList::<QString>::default();
        for t in &titulos {
            list.append(QString::from(t));
        }
        self.as_mut().set_queue(QStringList::from(&list));
        self.as_mut().set_queue_index(0);
        self.as_mut().set_playing(!titulos.is_empty());
    }

    pub fn save_queue(mut self: Pin<&mut Self>, path: &QString) {
        let path = path.to_string();
        let path = path.strip_prefix("file://").unwrap_or(&path).to_string();
        let ok = self.as_mut().rust_mut().api.save_queue(std::path::Path::new(&path));
        if ok {
            self.as_mut().set_library_status(QString::from(&format!("lista guardada en {path}")));
        }
    }

    /// Encola una carpeta entera y la echa a andar.
    pub fn play_folder(mut self: Pin<&mut Self>, folder: &QString) {
        let path = folder.to_string();
        let path = path.strip_prefix("file://").unwrap_or(&path).to_string();
        let (device, bit_perfect) = self.as_mut().chosen();
        self.as_mut().remember_folder(&format!("{path}/."));
        let tracks = self
            .as_mut()
            .rust_mut()
            .api
            .play_folder(std::path::Path::new(&path), &device, bit_perfect);
        let mut list = QList::<QString>::default();
        for track in &tracks {
            list.append(QString::from(track));
        }
        self.as_mut().set_queue(QStringList::from(&list));
        self.as_mut().set_queue_index(0);
        self.as_mut().set_playing(!tracks.is_empty());
    }

    /// La salida y la política elegidas, que es lo que usa todo lo que suena.
    fn chosen(self: Pin<&mut Self>) -> (String, bool) {
        (self.device.to_string(), self.want_bit_perfect)
    }

    pub fn choose(mut self: Pin<&mut Self>, device: &QString, bit_perfect: bool) {
        let id = device.to_string();
        self.as_mut().set_device(device.clone());
        self.as_mut().set_want_bit_perfect(bit_perfect);
        self.as_mut().rust_mut().api.remember(&id, bit_perfect);
    }

    pub fn automix(mut self: Pin<&mut Self>) {
        let mixed = self.as_mut().rust_mut().api.automix();
        if mixed > 0 {
            self.as_mut().set_library_status(QString::from(&format!(
                "mezcla por tempo: {mixed} pistas"
            )));
            // La mezcla apaga el aleatorio: son dos formas de lo mismo.
            self.as_mut().rust_mut().shuffle = false;
            self.as_mut().shuffle_changed();
        }
    }

    pub fn shuffle(&self) -> bool {
        self.shuffle
    }

    pub fn shuffle_queue(mut self: Pin<&mut Self>, shuffle: bool) {
        if self.shuffle == shuffle {
            return;
        }
        self.as_mut().rust_mut().shuffle = shuffle;
        self.as_mut().rust_mut().api.set_shuffle(shuffle);
        self.as_mut().shuffle_changed();
    }

    pub fn cycle_repeat(mut self: Pin<&mut Self>) {
        let next = (self.repeat + 1) % 3;
        let mode = match next {
            1 => chilpa_api::Repeat::All,
            2 => chilpa_api::Repeat::One,
            _ => chilpa_api::Repeat::Off,
        };
        self.as_mut().set_repeat(next);
        self.as_mut().rust_mut().api.set_repeat(mode);
    }

    pub fn next_track(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().api.next_track();
    }

    pub fn previous_track(mut self: Pin<&mut Self>) {
        let (device, bit_perfect) = self.as_mut().chosen();
        self.as_mut().rust_mut().api.previous_track(&device, bit_perfect);
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
            let correlacion = self.api.scope_correlation();
            self.as_mut().set_correlation(f64::from(correlacion.unwrap_or(999.0)));

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
    }
}
