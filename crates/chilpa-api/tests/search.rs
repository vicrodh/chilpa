//! Buscar con un plugin de verdad puesto en disco.
//!
//! Las pruebas de `merge` viven en el módulo y no tocan nada; esta comprueba
//! lo otro: que la consulta salga en su hilo, que lo que el plugin contesta
//! llegue por `poll`, y que una canción que ya está en la biblioteca no se
//! muestre dos veces sino con su "también está en".
//!
//! Es una sola prueba porque apunta `XDG_DATA_HOME` a un directorio
//! temporal, y esa variable es del proceso entero.

use std::path::{Path, PathBuf};

/// El ISRC que comparten la pista local y la que contesta el plugin.
const ISRC: &str = "USRC17607839";

/// Un plugin de catálogo que contesta siempre lo mismo.
///
/// `reply` va con sus comillas de verdad: aquí se escapa para meterlo en el
/// texto de wasm y el largo se saca del original, no del texto escapado.
/// Contar a mano es cómo una prueba pasa por la razón equivocada.
fn source_plugin(dir: &Path, id: &str, reply: &str) {
    std::fs::create_dir_all(dir.join(id)).unwrap();
    std::fs::write(
        dir.join(id).join("manifest.json"),
        format!(
            r#"{{
                "id": "{id}",
                "name": "Source {id}",
                "version": "0.1.0",
                "api_version": 1,
                "kind": "source",
                "capabilities": []
            }}"#
        ),
    )
    .unwrap();
    let len = reply.len();
    // Una cadena de wasm no lleva saltos de línea crudos; el escape sigue
    // siendo un byte, así que el largo no cambia.
    let escaped = reply.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n");
    let wat = format!(
        r#"(module
            (memory (export "memory") 1)
            (global $next (mut i32) (i32.const 4096))
            (func (export "chilpa_alloc") (param i32) (result i32)
              (local i32)
              (local.set 1 (global.get $next))
              (global.set $next (i32.add (global.get $next) (local.get 0)))
              (local.get 1))
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (data (i32.const 8) "{escaped}")
            ;; Contesta lo mismo sin mirar la consulta: lo que se prueba aquí
            ;; es el camino, no el buscador.
            (func (export "chilpa_search") (param i32 i32) (result i64)
              (i64.or
                (i64.shl (i64.const 8) (i64.const 32))
                (i64.const {len}))))"#
    );
    std::fs::write(dir.join(id).join("plugin.wasm"), wat::parse_str(&wat).expect("compila"))
        .unwrap();
}

/// Un WAV chiquito con su ISRC puesto después, directo en la base.
fn wave(dir: &Path, name: &str) -> PathBuf {
    use std::io::Write;
    let p = dir.join(name);
    let frames = 4_410u32;
    let data = frames * 4;
    let mut f = std::fs::File::create(&p).unwrap();
    f.write_all(b"RIFF").unwrap();
    f.write_all(&(36 + data).to_le_bytes()).unwrap();
    f.write_all(b"WAVEfmt ").unwrap();
    f.write_all(&16u32.to_le_bytes()).unwrap();
    f.write_all(&1u16.to_le_bytes()).unwrap();
    f.write_all(&2u16.to_le_bytes()).unwrap();
    f.write_all(&44_100u32.to_le_bytes()).unwrap();
    f.write_all(&176_400u32.to_le_bytes()).unwrap();
    f.write_all(&4u16.to_le_bytes()).unwrap();
    f.write_all(&16u16.to_le_bytes()).unwrap();
    f.write_all(b"data").unwrap();
    f.write_all(&data.to_le_bytes()).unwrap();
    for i in 0..frames {
        let v = (i % 3_000) as i16;
        f.write_all(&v.to_le_bytes()).unwrap();
        f.write_all(&v.to_le_bytes()).unwrap();
    }
    p
}

#[test]
fn a_source_plugin_answers_the_search_and_does_not_duplicate_what_is_local() {
    let dir = std::env::temp_dir().join("chilpa-api-search");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("XDG_DATA_HOME", dir.join("datos"));

    // Una biblioteca con una pista, y su ISRC anotado como lo dejaría el
    // escaneo de un archivo que sí lo trae.
    let music = dir.join("musica");
    std::fs::create_dir_all(&music).unwrap();
    wave(&music, "Suzie Q.wav");
    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.scan(&music);
    let local = chilpa.search("Suzie", 20);
    assert_eq!(local.len(), 1, "la pista se escaneó");

    // El ISRC lo deja el escaneo cuando el archivo lo trae; este WAV no
    // trae tags, así que se pone a mano en la misma base.
    rusqlite::Connection::open(chilpa_library::Library::default_path())
        .unwrap()
        .execute("UPDATE tracks SET isrc = ?1", rusqlite::params![ISRC])
        .unwrap();

    // Dos sources: uno trae la misma canción (mismo ISRC) y otra que no
    // está en disco; el otro se cae.
    let plugins = chilpa_api::Chilpa::plugins_dir();
    std::fs::create_dir_all(&plugins).unwrap();
    source_plugin(
        &plugins,
        "com.prueba.bueno",
        &format!(
            r#"{{"tracks":[
                 {{"external_id":"a1","title":"Suzie Q","artist":"CCR","isrc":"{ISRC}",
                   "quality":{{"codec":"flac","bit_depth":24,"sample_rate_hz":96000,
                               "channels":2}}}},
                 {{"external_id":"a2","title":"Otra que no tengo","artist":"CCR"}}]}}"#
        ),
    );
    source_plugin(&plugins, "com.prueba.roto", "esto no es json");

    assert_eq!(chilpa.load_plugins(), 2, "los dos cargaron");

    // Lo local sale de inmediato, sin esperar a nadie.
    let immediate = chilpa.search_all("Suzie", 20);
    assert_eq!(immediate.len(), 1, "lo local no espera a los plugins");
    assert_eq!(immediate[0].source, "local");
    assert!(immediate[0].playable, "lo que está en disco sí se puede tocar");

    // Y lo de los sources llega por poll, cuando pasa el descanso de tecleo.
    // Se mira como lo mira la ventana: solo cuando avisan que cambió. Una
    // consulta puede empezar y acabar entre dos pulsos, y aun así el último
    // source en contestar tiene que quedar dibujado.
    let mut hits = Vec::new();
    let arrived = wait_for(&mut chilpa, |c| {
        if c.search_changed() {
            hits = c.found().to_vec();
        }
        hits.len() >= 2
    });
    assert!(arrived, "los sources contestaron: {hits:?}");
    assert!(!chilpa.search_running(), "y ya no falta nadie");
    assert_eq!(hits.len(), 2, "la misma canción no se muestra dos veces: {hits:?}");

    let suzie = &hits[0];
    assert_eq!(suzie.source, "local", "lo local conserva su lugar");
    assert_eq!(
        suzie.also_in,
        vec!["Source com.prueba.bueno"],
        "y dice en qué source más está"
    );

    let other = &hits[1];
    assert_eq!(other.source, "com.prueba.bueno");
    assert_eq!(other.title, "Otra que no tengo");
    assert!(!other.playable, "lo de un plugin todavía no se resuelve a un stream");
    assert_eq!(other.quality, "", "esa no declaró calidad y no se le inventa");

    // El que contestó mal se dice, y no se llevó a nadie.
    let problems = chilpa.search_problems();
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].starts_with("Source com.prueba.roto"), "{problems:?}");

    // Buscar otra cosa tira lo de la consulta anterior de inmediato.
    let other_search = chilpa.search_all("no existe nada así", 20);
    assert!(other_search.is_empty());
    assert!(chilpa.found().is_empty(), "no se quedan resultados de la búsqueda vieja");
    assert!(chilpa.search_problems().is_empty(), "ni sus problemas");

    let _ = std::fs::remove_dir_all(dir);
}

/// Llama a `poll` deadline que se cumpla la condición, o se acabe la paciencia.
fn wait_for(
    chilpa: &mut chilpa_api::Chilpa,
    mut cond: impl FnMut(&mut chilpa_api::Chilpa) -> bool,
) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        chilpa.poll();
        if cond(chilpa) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    false
}
