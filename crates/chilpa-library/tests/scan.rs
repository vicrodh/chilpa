//! Escanear de verdad: incremental, y sin borrar lo que no se ve.

use chilpa_library::{scan_root, Library};
use std::path::PathBuf;
use std::process::Command;

fn hay_ffmpeg() -> bool {
    Command::new("ffmpeg").arg("-version").output().map(|o| o.status.success()).unwrap_or(false)
}

/// Un álbum de prueba en disco.
fn album(name: &str, tracks: usize) -> Option<PathBuf> {
    if !hay_ffmpeg() {
        return None;
    }
    let mut dir = std::env::temp_dir();
    dir.push(format!("chilpa-escaneo-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("disco 1")).ok()?;
    for i in 1..=tracks {
        let target = dir.join("disco 1").join(format!("{i:02}-pista.flac"));
        let ok = Command::new("ffmpeg")
            .args(["-y", "-loglevel", "error", "-f", "lavfi", "-i", "sine=frequency=440:duration=1", "-c:a", "flac"])
            .args(["-metadata", &format!("title=Pista {i}")])
            .args(["-metadata", "artist=Artista"])
            .args(["-metadata", "album=Disco"])
            .args(["-metadata", &format!("track={i}")])
            .args(["-metadata", "genre=Rock"])
            .arg(&target)
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
fn a_scan_reads_the_tags_and_builds_the_album() {
    let Some(dir) = album("basico", 3) else {
        eprintln!("sin ffmpeg: se salta");
        return;
    };
    let lib = Library::in_memory().unwrap();
    let report = scan_root(&lib, &dir).unwrap();

    assert_eq!(report.seen, 3, "el .txt no cuenta");
    assert_eq!(report.read, 3);
    assert_eq!(lib.track_count(false).unwrap(), 3);

    let albums = lib.albums(10, 0).unwrap();
    assert_eq!(albums.len(), 1);
    assert_eq!(albums[0].title, "Disco");
    assert_eq!(albums[0].album_artist, "Artista");
    assert_eq!(albums[0].track_count, 3);

    let tracks = lib.album_tracks(albums[0].id).unwrap();
    assert_eq!(tracks.len(), 3);
    assert_eq!(tracks[0].title, "Pista 1", "vienen en el orden del disco");
    assert_eq!(tracks[2].title, "Pista 3");
    assert!(tracks[0].duration_ms > 800, "duró {} ms", tracks[0].duration_ms);

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_second_scan_does_not_reread_what_did_not_change() {
    let Some(dir) = album("incremental", 4) else {
        eprintln!("sin ffmpeg: se salta");
        return;
    };
    let lib = Library::in_memory().unwrap();
    scan_root(&lib, &dir).unwrap();
    let second = scan_root(&lib, &dir).unwrap();

    assert_eq!(second.read, 0, "nada cambió, nada se releyó");
    assert_eq!(second.unchanged, 4);
    assert_eq!(lib.track_count(false).unwrap(), 4, "y la biblioteca quedó igual");

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_root_that_disappears_marks_its_tracks_but_does_not_delete_them() {
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
    let report = scan_root(&lib, &dir).unwrap();

    assert!(report.unreachable, "el root no se pudo leer");
    assert_eq!(report.missing, 0, "no se podó nada");
    assert_eq!(lib.track_count(true).unwrap(), 2, "las filas siguen ahí");
    assert_eq!(lib.track_count(false).unwrap(), 0, "marcadas como ausentes");

    // Y al volver, vuelven a contar.
    let dir2 = album("desmontado", 2).unwrap();
    let wrap = scan_root(&lib, &dir2).unwrap();
    assert_eq!(wrap.read, 2);
    assert_eq!(lib.track_count(false).unwrap(), 2);
    let _ = std::fs::remove_dir_all(dir2);
}

#[test]
fn a_deleted_file_is_marked_missing_and_keeps_its_history() {
    let Some(dir) = album("borrado", 3) else {
        eprintln!("sin ffmpeg: se salta");
        return;
    };
    let lib = Library::in_memory().unwrap();
    scan_root(&lib, &dir).unwrap();

    std::fs::remove_file(dir.join("disco 1").join("02-pista.flac")).unwrap();
    let report = scan_root(&lib, &dir).unwrap();

    assert_eq!(report.missing, 1);
    assert_eq!(lib.track_count(false).unwrap(), 2, "quedan dos presentes");
    assert_eq!(lib.track_count(true).unwrap(), 3, "la fila del borrado sobrevive");

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn search_finds_by_title_artist_and_album() {
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
fn the_folder_cover_is_used_when_the_file_carries_none() {
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

    let albums = lib.albums(10, 0).unwrap();
    assert_eq!(albums.len(), 1);
    assert_eq!(
        albums[0].artwork_path.as_deref(),
        Some(cover.display().to_string().as_str()),
        "se tomó la portada de la carpeta"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_genres_in_the_file_reach_the_library() {
    let Some(dir) = album("generos", 2) else {
        eprintln!("sin ffmpeg: se salta");
        return;
    };
    let lib = Library::in_memory().unwrap();
    scan_root(&lib, &dir).unwrap();

    let generos = lib.genres().unwrap();
    assert_eq!(generos, vec![("Rock".to_string(), 2)], "un género con sus dos pistas");

    // Y cada pista lo trae.
    let source = lib.source_id("local", "default").unwrap();
    let id = lib
        .track_by_path(source, &dir.join("disco 1").join("01-pista.flac"))
        .unwrap()
        .expect("la pista está");
    assert_eq!(lib.track_genres(id).unwrap(), vec!["Rock".to_string()]);

    let _ = std::fs::remove_dir_all(dir);
}
