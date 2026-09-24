//! El puente entre QML y la fachada.
//!
//! Aquí no vive lógica: el objeto expone propiedades que QML lee y órdenes
//! que QML dispara, y cada una es una línea que llama a `chilpa-api`
//! (ADR-001). Si una vista necesita algo que no está en la fachada, se amplía
//! la fachada, no este archivo.

use cxx_qt_lib::{QList, QString, QStringList};

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
        /// Los sources que contestaron mal, ya redactado para mostrarse.
        /// Vacío cuando todos contestaron bien.
        #[qproperty(QString, search_problems)]
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
        /// Qué visualizador se dibuja: "wave", "spectrum" o "goniometer".
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
        /// Qué resultado de la búsqueda poner a sonar al arrancar, contando
        /// desde 1. Para probar sin teclado ni ratón, como los de arriba.
        #[qproperty(i32, initial_play)]
        /// Qué plugin conectar al arrancar, por la misma razón.
        #[qproperty(QString, initial_connect)]
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
        ///
        /// Lo de un source se resuelve antes de sonar, así que puede tardar
        /// lo que el plugin tarde en contestar.
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

        /// El usuario aprueba lo que un plugin pide; hasta entonces no
        /// corre, ni se compila su wasm (ADR-008).
        #[qinvokable]
        fn approve_plugin(self: Pin<&mut Chilpa>, id: &QString);

        /// Conecta la cuenta de un plugin: abre el navegador donde el
        /// servicio pide permiso. Lo que conteste llega en el pulso.
        #[qinvokable]
        fn connect_account(self: Pin<&mut Chilpa>, id: &QString);

        /// Deja de esperar al navegador.
        #[qinvokable]
        fn cancel_connect(self: Pin<&mut Chilpa>);

        /// Borra la credencial guardada. El plugin sigue instalado.
        #[qinvokable]
        fn disconnect_account(self: Pin<&mut Chilpa>, id: &QString);

        /// Le quita la aprobación: deja de correr en el momento.
        #[qinvokable]
        fn revoke_plugin(self: Pin<&mut Chilpa>, id: &QString);

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

mod library;
mod playback;
mod plugins;
mod search;
mod tick;

/// Un tiempo largo dicho como se dice: "3 h 21 min", no 12 060 000 ms.
pub(super) fn long_duration(ms: u64) -> String {
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
pub(super) fn short_date(unix: i64) -> String {
    // Días desde 1970 a fecha civil (Howard Hinnant). Son cuatro líneas y
    // evitan una dependencia para mostrar un renglón.
    let days_ago = unix.div_euclid(86_400) + 719_468;
    let era = days_ago.div_euclid(146_097);
    let doe = days_ago - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Las vistas rápidas de la biblioteca, con su nombre para la lista.
pub(super) const VIEWS: &[(chilpa_api::AlbumSort, &str)] = &[
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
    search_problems: QString,
    result_paths: Vec<String>,
    /// Si el pulso anterior ya sabía que se estaba esperando al navegador,
    /// para no rearmar la lista de plugins en cada pulso.
    connecting_shown: bool,
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
    initial_play: i32,
    initial_connect: QString,
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
            search_problems: QString::from(""),
            result_paths: Vec::new(),
            connecting_shown: false,
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
                for v in VIEWS {
                    l.append(QString::from(v.1));
                }
                QStringList::from(&l)
            },
            album_view: {
                let actual = chilpa_api::Chilpa::new().album_sort();
                VIEWS.iter().position(|v| v.0 == actual).unwrap_or(0) as i32
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
            initial_search: QString::from(&std::env::var("CHILPA_SEARCH").unwrap_or_default()),
            initial_album: std::env::var("CHILPA_ALBUM").ok().and_then(|v| v.parse().ok()).unwrap_or(0),
            initial_tab: QString::from(&std::env::var("CHILPA_TAB").unwrap_or_default()),
            // Qué plugin conectar al arrancar. Vacío no conecta nada.
            initial_connect: QString::from(
                &std::env::var("CHILPA_CONNECT").unwrap_or_default(),
            ),
            // Cuál resultado poner a sonar al arrancar, contando desde 1.
            // Con 0 no suena nada, que es lo normal.
            initial_play: std::env::var("CHILPA_PLAY")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
            initial_rescan: std::env::var("CHILPA_RESCAN").is_ok(),
            initial_analysis: std::env::var("CHILPA_ANALYZE").is_ok(),
            api: chilpa_api::Chilpa::new(),
        }
    }
}


/// Abre una URL en el navegador del sistema.
///
/// `xdg-open` en Linux, `open` en macOS, `start` en Windows. No se espera a
/// que termine: el navegador vive su vida y Chilpa sigue con la suya, que
/// es esperar en su hilo a que el usuario regrese.
pub(super) fn open_in_browser(url: &str) -> std::io::Result<()> {
    let mut cmd = if cfg!(target_os = "macos") {
        std::process::Command::new("open")
    } else if cfg!(target_os = "windows") {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", ""]);
        c
    } else {
        std::process::Command::new("xdg-open")
    };
    cmd.arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
}
