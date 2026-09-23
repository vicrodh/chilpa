//! Escanear de verdad: incremental, y sin borrar lo que no se ve.

use chilpa_library::{scan_root, Library};
use std::path::PathBuf;
use std::process::Command;

fn hay_ffmpeg() -> bool {
    Command::new("ffmpeg").arg("-version").output().map(|o| o.status.success()).unwrap_or(false)
}

/// Un álbum de prueba en disco.
fn album(nombre: &str, pistas: usize) -> Option<PathBuf> {
    if !hay_ffmpeg() {
        return None;
    }
    let mut dir = std::env::temp_dir();
    dir.push(format!("chilpa-escaneo-{nombre}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("disco 1")).ok()?;
    for i in 1..=pistas {
        let destino = dir.join("disco 1").join(format!("{i:02}-pista.flac"));
        let ok = Command::new("ffmpeg")
            .args(["-y", "-loglevel", "error", "-f", "lavfi", "-i", "sine=frequency=440:duration=1", "-c:a", "flac"])
            .args(["-metadata", &format!("title=Pista {i}")])
            .args(["-metadata", "artist=Artista"])
            .args(["-metadata", "album=Disco"])
            .args(["-metadata", &format!("track={i}")])
            .arg(&destino)
            .output()
            .ok()?
            .status
            .success();
        if !ok {
            return None;
        }
    }
    // Algo que no es audio, para comprobar que no entra.
    std::fs::write(dir.join("notas.txt"), b"hola").ok()?;
    Some(dir)
}

#[test]
fn un_escaneo_lee_los_tags_y_arma_el_album() {
    let Some(dir) = album("basico", 3) else {
        eprintln!("sin ffmpeg: se salta");
        return;
    };
    let lib = Library::in_memory().unwrap();
    let reporte = scan_root(&lib, &dir).unwrap();

    assert_eq!(reporte.seen, 3, "el .txt no cuenta");
    assert_eq!(reporte.read, 3);
    assert_eq!(lib.track_count(false).unwrap(), 3);

    let albumes = lib.albums(10, 0).unwrap();
    assert_eq!(albumes.len(), 1);
    assert_eq!(albumes[0].title, "Disco");
    assert_eq!(albumes[0].album_artist, "Artista");
    assert_eq!(albumes[0].track_count, 3);

    let pistas = lib.album_tracks(albumes[0].id).unwrap();
    assert_eq!(pistas.len(), 3);
    assert_eq!(pistas[0].title, "Pista 1", "vienen en el orden del disco");
    assert_eq!(pistas[2].title, "Pista 3");
    assert!(pistas[0].duration_ms > 800, "duró {} ms", pistas[0].duration_ms);

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn un_segundo_escaneo_no_vuelve_a_leer_lo_que_no_cambio() {
    let Some(dir) = album("incremental", 4) else {
        eprintln!("sin ffmpeg: se salta");
        return;
    };
    let lib = Library::in_memory().unwrap();
    scan_root(&lib, &dir).unwrap();
    let segundo = scan_root(&lib, &dir).unwrap();

    assert_eq!(segundo.read, 0, "nada cambió, nada se releyó");
    assert_eq!(segundo.unchanged, 4);
    assert_eq!(lib.track_count(false).unwrap(), 4, "y la biblioteca quedó igual");

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn un_root_que_desaparece_marca_sus_tracks_pero_no_los_borra() {
    // El hallazgo alto de la auditoría: un mountpoint desmontado se veía
    // vacío y el prune borraba todos sus tracks.
    let Some(dir) = album("desmontado", 2) else {
        eprintln!("sin ffmpeg: se salta");
        return;
    };
    let lib = Library::in_memory().unwrap();
    scan_root(&lib, &dir).unwrap();
    assert_eq!(lib.track_count(false).unwrap(), 2);

    std::fs::remove_dir_all(&dir).unwrap();
    let reporte = scan_root(&lib, &dir).unwrap();

    assert!(reporte.unreachable, "el root no se pudo leer");
    assert_eq!(reporte.missing, 0, "no se podó nada");
    assert_eq!(lib.track_count(true).unwrap(), 2, "las filas siguen ahí");
    assert_eq!(lib.track_count(false).unwrap(), 0, "marcadas como ausentes");

    // Y al volver, vuelven a contar.
    let dir2 = album("desmontado", 2).unwrap();
    let vuelta = scan_root(&lib, &dir2).unwrap();
    assert_eq!(vuelta.read, 2);
    assert_eq!(lib.track_count(false).unwrap(), 2);
    let _ = std::fs::remove_dir_all(dir2);
}

#[test]
fn un_archivo_borrado_se_marca_ausente_y_no_se_pierde_su_historia() {
    let Some(dir) = album("borrado", 3) else {
        eprintln!("sin ffmpeg: se salta");
        return;
    };
    let lib = Library::in_memory().unwrap();
    scan_root(&lib, &dir).unwrap();

    std::fs::remove_file(dir.join("disco 1").join("02-pista.flac")).unwrap();
    let reporte = scan_root(&lib, &dir).unwrap();

    assert_eq!(reporte.missing, 1);
    assert_eq!(lib.track_count(false).unwrap(), 2, "quedan dos presentes");
    assert_eq!(lib.track_count(true).unwrap(), 3, "la fila del borrado sobrevive");

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn la_busqueda_encuentra_por_titulo_artista_y_album() {
    let Some(dir) = album("busqueda", 2) else {
        eprintln!("sin ffmpeg: se salta");
        return;
    };
    let lib = Library::in_memory().unwrap();
    scan_root(&lib, &dir).unwrap();

    assert_eq!(lib.search("Pista 1", 10).unwrap().len(), 1);
    assert_eq!(lib.search("Artista", 10).unwrap().len(), 2);
    assert_eq!(lib.search("Disco", 10).unwrap().len(), 2);
    assert!(lib.search("nada de esto", 10).unwrap().is_empty());

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn la_portada_de_la_carpeta_se_usa_cuando_el_archivo_no_trae_una() {
    let Some(dir) = album("portada-carpeta", 2) else {
        eprintln!("sin ffmpeg: se salta");
        return;
    };
    // Los archivos que genera la prueba no traen portada embebida.
    let cover = dir.join("disco 1").join("cover.jpg");
    std::fs::write(&cover, b"no es un jpeg de verdad, pero existe").unwrap();

    let lib = Library::in_memory().unwrap();
    let cache = std::env::temp_dir().join("chilpa-escaneo-cache");
    chilpa_library::scan_root_with_cache(&lib, &dir, Some(&cache)).unwrap();

    let albumes = lib.albums(10, 0).unwrap();
    assert_eq!(albumes.len(), 1);
    assert_eq!(
        albumes[0].artwork_path.as_deref(),
        Some(cover.display().to_string().as_str()),
        "se tomó la portada de la carpeta"
    );
    let _ = std::fs::remove_dir_all(dir);
}
