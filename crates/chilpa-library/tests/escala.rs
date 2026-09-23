//! Que una biblioteca grande siga siendo rápida.
//!
//! El hallazgo alto de la auditoría de QBZ era la base de datos: cada página
//! de la reja volvía a agregar la biblioteca entera, y una consulta por
//! renglón hacía el resto. Esta prueba arma una biblioteca de verdad grande
//! y le pone un techo de tiempo a lo que la interfaz consulta.
//!
//! Los techos son generosos a propósito. No están para medir la máquina:
//! están para que una consulta que se vuelva cuadrática se note el día que
//! alguien la escriba, no el día que un usuario abra su biblioteca.

use chilpa_library::{AlbumSort, Library, Rule, SmartList, SmartOrder};
use rusqlite::params;
use std::time::{Duration, Instant};

const ALBUMES: i64 = 5_000;
const POR_ALBUM: i64 = 10;
const TRACKS: i64 = ALBUMES * POR_ALBUM;

/// Arma una biblioteca con cincuenta mil pistas y un log de escuchas.
fn biblioteca_grande() -> Library {
    let lib = Library::in_memory().unwrap();
    let source = lib.source_id("local", "default").unwrap();
    let tx = lib.connection().unchecked_transaction().unwrap();
    {
        let mut albumes = tx
            .prepare("INSERT INTO albums (id, title, album_artist, year) VALUES (?1, ?2, ?3, ?4)")
            .unwrap();
        let mut tracks = tx
            .prepare(
                "INSERT INTO tracks (id, source_id, external_id, title, artist, album_id,
                                     duration_ms, bit_depth, sample_rate_hz, added_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            )
            .unwrap();
        let mut plays = tx
            .prepare(
                "INSERT INTO play_events (track_id, source_id, started_at, played_ms,
                                          duration_ms, bit_perfect)
                 VALUES (?1, ?2, ?3, ?4, ?4, 1)",
            )
            .unwrap();

        for a in 1..=ALBUMES {
            // Doscientos artistas, para que la reja por artista agrupe de
            // verdad y no sea una fila por artista.
            let artista = format!("Artista {:03}", a % 200);
            albumes
                .execute(params![a, format!("Disco {a}"), artista, 1960 + (a % 60) as i32])
                .unwrap();
            for p in 1..=POR_ALBUM {
                let id = (a - 1) * POR_ALBUM + p;
                let bits = if id % 7 == 0 { 24 } else { 16 };
                let rate = if id % 7 == 0 { 96_000 } else { 44_100 };
                tracks
                    .execute(params![
                        id,
                        source,
                        format!("/musica/{a}/{p:02}.flac"),
                        format!("Pista {p} del {a}"),
                        format!("Artista {:03}", a % 200),
                        a,
                        200_000 + (id % 400_000),
                        bits,
                        rate,
                        1_700_000_000i64 + id
                    ])
                    .unwrap();
                // Una de cada tres se ha escuchado, algunas varias veces.
                if id % 3 == 0 {
                    for k in 0..=(id % 4) {
                        plays
                            .execute(params![id, source, 1_700_000_000i64 + id * 10 + k, 200_000])
                            .unwrap();
                    }
                }
            }
        }
    }
    tx.commit().unwrap();
    lib
}

/// Corre algo y dice cuánto tardó.
fn cuanto<T>(que: impl FnOnce() -> T) -> (T, Duration) {
    let inicio = Instant::now();
    let r = que();
    (r, inicio.elapsed())
}

fn dentro_de(nombre: &str, tardo: Duration, techo: Duration) {
    // Con `--nocapture` se ven los tiempos, que es lo que hace útil este
    // archivo cuando algo empieza a ponerse lento pero todavía pasa.
    eprintln!("{nombre}: {tardo:?}");
    assert!(
        tardo <= techo,
        "{nombre} tardó {tardo:?}, el techo es {techo:?}: algo se volvió cuadrático"
    );
}

#[test]
fn una_biblioteca_de_cincuenta_mil_pistas_no_se_arrastra() {
    let (lib, armado) = cuanto(biblioteca_grande);
    eprintln!("armar {TRACKS} pistas: {armado:?}");
    assert_eq!(lib.track_count(false).unwrap(), TRACKS as usize);

    // La reja: una página de cien álbumes con su cuenta de escuchas. Esto
    // es lo que en QBZ agregaba la biblioteca entera por página.
    //
    // Los órdenes que solo miran columnas de `albums` recortan la página
    // antes de contar nada y salen en unos diez milisegundos; los que
    // ordenan por escuchas o por fecha de alta tienen que agregar, y andan
    // en ciento cincuenta. Por eso el techo es el mismo para todos: lo que
    // se vigila es que ninguno se vuelva cuadrático.
    for orden in AlbumSort::all() {
        let (pagina, t) = cuanto(|| lib.albums_sorted(orden, 100, 0).unwrap());
        assert_eq!(pagina.len(), 100, "{}", orden.name());
        dentro_de(&format!("reja por {}", orden.name()), t, Duration::from_millis(1500));
    }

    // Una página del fondo: no puede costar más que la primera por estar
    // lejos.
    let (_, t) = cuanto(|| lib.albums_sorted(AlbumSort::Artist, 100, 4_000).unwrap());
    dentro_de("reja en la página 40", t, Duration::from_millis(1500));

    // Los artistas.
    let (artistas, t) = cuanto(|| lib.artists(500, 0).unwrap());
    assert_eq!(artistas.len(), 200, "doscientos artistas");
    dentro_de("artistas", t, Duration::from_millis(1500));

    // La búsqueda, que recorre títulos, artistas, álbumes y géneros.
    let (encontradas, t) = cuanto(|| lib.search("Pista 7", 200).unwrap());
    assert!(!encontradas.is_empty());
    dentro_de("búsqueda", t, Duration::from_millis(1500));

    // Las estadísticas, que suman el log entero.
    let (stats, t) = cuanto(|| lib.stats().unwrap());
    assert!(stats.plays > 0);
    dentro_de("estadísticas", t, Duration::from_millis(1500));

    let (_, t) = cuanto(|| lib.top_tracks(20).unwrap());
    dentro_de("más escuchadas", t, Duration::from_millis(1500));
    let (_, t) = cuanto(|| lib.recently_played(20).unwrap());
    dentro_de("lo último que sonó", t, Duration::from_millis(1500));

    // Las listas inteligentes, que es lo que se resuelve cada vez que se
    // miran.
    for lista in chilpa_library::smart_builtins() {
        let (_, t) = cuanto(|| lib.smart_tracks(&lista).unwrap());
        dentro_de(&format!("lista \"{}\"", lista.name), t, Duration::from_millis(1500));
    }

    // Y una lista con varias reglas a la vez.
    let compuesta = SmartList::new(
        "compuesta",
        vec![Rule::HiRes, Rule::PlayedAtLeast(2), Rule::LongerThanMs(300_000)],
        SmartOrder::MostPlayed,
        200,
    );
    let (_, t) = cuanto(|| lib.smart_tracks(&compuesta).unwrap());
    dentro_de("lista con tres reglas", t, Duration::from_millis(1500));
}

#[test]
fn las_features_de_una_pagina_salen_en_una_sola_consulta() {
    let lib = biblioteca_grande();
    let ids: Vec<i64> = (1..=200).collect();
    for id in &ids {
        lib.save_features(*id, "chilpa", 2, Some(120.0), Some(0.8), None, Some(-12.0)).unwrap();
    }
    let (mapa, t) = cuanto(|| lib.features_for(&ids, "chilpa").unwrap());
    assert_eq!(mapa.len(), 200);
    dentro_de("features de doscientas pistas", t, Duration::from_millis(500));
}
