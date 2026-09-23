//! Escanear una carpeta hacia la biblioteca.
//!
//! Dos reglas que vienen de la auditoría de QBZ:
//!
//! 1. **Lo que no cambió no se toca.** Un archivo cuya fecha y tamaño son los
//!    mismos no se vuelve a leer ni se reescribe su fila. En QBZ un rescan sin
//!    cambios hacía tres recorridos y reescribía cada fila.
//! 2. **Un root que no se puede leer no borra nada.** Si la carpeta no
//!    existe o no se puede abrir, se marca como inalcanzable y se sale. En
//!    QBZ un mountpoint desmontado se veía vacío y podaba todos sus tracks.

use crate::store::Library;
use rusqlite::params;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Qué pasó en un escaneo.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct ScanReport {
    pub root: PathBuf,
    /// Archivos de audio vistos.
    pub seen: usize,
    /// Los que se leyeron porque son nuevos o cambiaron.
    pub read: usize,
    /// Los que se saltaron por no haber cambiado.
    pub unchanged: usize,
    /// Los que ya no están y se marcaron ausentes.
    pub missing: usize,
    /// Cierto si el root no se pudo leer; entonces no se tocó nada.
    pub unreachable: bool,
}

const EXTENSIONES: &[&str] =
    &["flac", "wav", "aiff", "aif", "m4a", "mp4", "alac", "mp3", "ogg", "oga", "dsf"];

/// Escanea una carpeta y sus subcarpetas, dejando las portadas en `cache`.
pub fn scan_root_with_cache(
    lib: &Library,
    root: &Path,
    cache: Option<&Path>,
) -> rusqlite::Result<ScanReport> {
    escanear(lib, root, cache, &|_| {})
}

/// Escanea una carpeta y sus subcarpetas.
pub fn scan_root(lib: &Library, root: &Path) -> rusqlite::Result<ScanReport> {
    escanear(lib, root, None, &|_| {})
}

/// Lo mismo, avisando cuántos archivos lleva mirados.
pub fn scan_root_reporting(
    lib: &Library,
    root: &Path,
    cache: Option<&Path>,
    on_progress: &dyn Fn(usize),
) -> rusqlite::Result<ScanReport> {
    escanear(lib, root, cache, on_progress)
}

/// Cuántos archivos de audio hay bajo una carpeta. No toca la base.
pub fn count_audio_files(root: &Path) -> usize {
    recorrer(root).len()
}

fn escanear(
    lib: &Library,
    root: &Path,
    cache: Option<&Path>,
    on_progress: &dyn Fn(usize),
) -> rusqlite::Result<ScanReport> {
    let mut reporte = ScanReport { root: root.to_path_buf(), ..ScanReport::default() };
    let source = lib.source_id("local", "default")?;
    let root_id = lib.root_id(source, root)?;

    if !root.is_dir() {
        lib.set_root_reachable(root_id, false)?;
        reporte.unreachable = true;
        return Ok(reporte);
    }
    lib.set_root_reachable(root_id, true)?;

    let ahora = ahora_unix();
    let archivos = recorrer(root);
    reporte.seen = archivos.len();

    for (mirados, archivo) in archivos.iter().enumerate() {
        // Se avisa cada 25 archivos: uno por uno inunda el canal sin que la
        // interfaz pueda dibujar más rápido.
        if mirados % 25 == 0 {
            on_progress(mirados);
        }
        let Ok(meta) = std::fs::metadata(archivo) else { continue };
        // En nanosegundos, no en segundos: dos versiones distintas de un
        // archivo escritas dentro del mismo segundo se verían iguales, y el
        // escaneo se saltaría el cambio.
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_nanos() as i64)
            .unwrap_or(0);
        let size = meta.len() as i64;
        let ruta = archivo.display().to_string();

        let conocido: Option<(i64, i64, Option<i64>)> = lib
            .db
            .query_row(
                "SELECT mtime, size, track_id FROM scanned_files WHERE path = ?1",
                params![ruta],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .ok();

        if let Some((m, s, track)) = &conocido {
            if *m == mtime && *s == size && track.is_some() {
                // Igual que la última vez: ni se lee ni se reescribe.
                reporte.unchanged += 1;
                lib.db.execute(
                    "UPDATE tracks SET present = 1 WHERE id = ?1",
                    params![track.unwrap()],
                )?;
                continue;
            }
        }

        let track_id = guardar_archivo(lib, source, archivo, ahora, cache)?;
        lib.db.execute(
            "INSERT INTO scanned_files (root_id, path, mtime, size, inode, track_id, scanned_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(path) DO UPDATE SET
                mtime = excluded.mtime, size = excluded.size,
                track_id = excluded.track_id, scanned_at = excluded.scanned_at",
            params![root_id, ruta, mtime, size, inodo(&meta), track_id, ahora],
        )?;
        reporte.read += 1;
    }

    on_progress(archivos.len());

    // Lo que estaba y ya no: se marca ausente, nunca se borra.
    let vistos: Vec<String> = archivos.iter().map(|p| p.display().to_string()).collect();
    reporte.missing = marcar_ausentes(lib, root_id, &vistos)?;
    lib.db
        .execute("UPDATE roots SET last_scan = ?2 WHERE id = ?1", params![root_id, ahora])?;
    Ok(reporte)
}

fn guardar_archivo(
    lib: &Library,
    source: i64,
    path: &Path,
    ahora: i64,
    cache: Option<&Path>,
) -> rusqlite::Result<i64> {
    let info = chilpa_tags::read(path).ok();
    let tags = info.as_ref().map(|i| i.tags.clone()).unwrap_or_default();
    let titulo = tags.title.clone().unwrap_or_else(|| {
        path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
    });
    let artista = tags.artist_line();
    let album_id = match &tags.album {
        Some(titulo_album) => {
            let album_artist = tags.album_artist.clone().unwrap_or_else(|| artista.clone());
            let id = album_id(lib, titulo_album, &album_artist, tags.year())?;
            // La portada del álbum sale del primer archivo que traiga una.
            if let Some(cache) = cache {
                guardar_portada(lib, id, path, cache)?;
            }
            Some(id)
        }
        None => None,
    };
    let duracion = info.as_ref().map(|i| i.duration.as_millis() as i64).unwrap_or(0);

    lib.db.execute(
        "INSERT INTO tracks
           (source_id, external_id, title, artist, album_id, track_number, disc_number,
            duration_ms, sample_rate_hz, bit_depth, channels, isrc, recording_mbid,
            present, added_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, 1, ?14)
         ON CONFLICT(source_id, external_id) DO UPDATE SET
           title = excluded.title, artist = excluded.artist, album_id = excluded.album_id,
           track_number = excluded.track_number, disc_number = excluded.disc_number,
           duration_ms = excluded.duration_ms, sample_rate_hz = excluded.sample_rate_hz,
           bit_depth = excluded.bit_depth, channels = excluded.channels,
           isrc = excluded.isrc, recording_mbid = excluded.recording_mbid, present = 1",
        params![
            source,
            path.display().to_string(),
            titulo,
            artista,
            album_id,
            tags.track_number,
            tags.disc_number,
            duracion,
            info.as_ref().and_then(|i| i.sample_rate_hz),
            info.as_ref().and_then(|i| i.bit_depth),
            info.as_ref().and_then(|i| i.channels),
            tags.isrc,
            tags.musicbrainz_recording_id,
            ahora
        ],
    )?;
    lib.db.query_row(
        "SELECT id FROM tracks WHERE source_id = ?1 AND external_id = ?2",
        params![source, path.display().to_string()],
        |r| r.get(0),
    )
}

/// Nombres de portada que se usan en las carpetas de música.
const PORTADAS: &[&str] = &[
    "cover.jpg", "cover.jpeg", "cover.png", "folder.jpg", "folder.jpeg", "folder.png",
    "front.jpg", "front.png", "album.jpg", "album.png", "Cover.jpg", "Folder.jpg",
];

/// Le pone portada al álbum si todavía no tiene.
///
/// Primero la del archivo; si no trae, la que esté junto a él en la carpeta,
/// que es como guarda las portadas media biblioteca del mundo.
fn guardar_portada(lib: &Library, album: i64, path: &Path, cache: &Path) -> rusqlite::Result<()> {
    let tiene: Option<String> = lib
        .db
        .query_row("SELECT artwork_path FROM albums WHERE id = ?1", params![album], |r| r.get(0))
        .unwrap_or(None);
    if tiene.is_some() {
        return Ok(());
    }
    let portada = chilpa_tags::extract_artwork(path, cache)
        .ok()
        .flatten()
        .or_else(|| portada_en_carpeta(path));
    if let Some(portada) = portada {
        lib.db.execute(
            "UPDATE albums SET artwork_path = ?2 WHERE id = ?1",
            params![album, portada.display().to_string()],
        )?;
    }
    Ok(())
}

/// La portada que esté junto al archivo, si la hay.
fn portada_en_carpeta(path: &Path) -> Option<PathBuf> {
    let dir = path.parent()?;
    for nombre in PORTADAS {
        let candidata = dir.join(nombre);
        if candidata.is_file() {
            return Some(candidata);
        }
    }
    // Cualquier imagen suelta, si no hay una con nombre conocido.
    let mut sueltas: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .and_then(|e| e.to_str())
                    .map(|e| matches!(e.to_lowercase().as_str(), "jpg" | "jpeg" | "png"))
                    .unwrap_or(false)
        })
        .collect();
    sueltas.sort();
    sueltas.into_iter().next()
}

fn album_id(lib: &Library, titulo: &str, artista: &str, año: Option<u32>) -> rusqlite::Result<i64> {
    lib.db.execute(
        "INSERT OR IGNORE INTO albums (title, album_artist, year) VALUES (?1, ?2, ?3)",
        params![titulo, artista, año],
    )?;
    lib.db.query_row(
        "SELECT id FROM albums WHERE title = ?1 AND album_artist = ?2",
        params![titulo, artista],
        |r| r.get(0),
    )
}

/// Marca ausentes los archivos del root que ya no aparecieron.
fn marcar_ausentes(lib: &Library, root_id: i64, vistos: &[String]) -> rusqlite::Result<usize> {
    let mut stmt = lib
        .db
        .prepare("SELECT path, track_id FROM scanned_files WHERE root_id = ?1")?;
    let filas: Vec<(String, Option<i64>)> = stmt
        .query_map(params![root_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut ausentes = 0;
    for (ruta, track) in filas {
        if vistos.iter().any(|v| v == &ruta) {
            continue;
        }
        if let Some(id) = track {
            lib.db.execute("UPDATE tracks SET present = 0 WHERE id = ?1", params![id])?;
        }
        lib.db.execute("DELETE FROM scanned_files WHERE path = ?1", params![ruta])?;
        ausentes += 1;
    }
    Ok(ausentes)
}

/// Todos los archivos de audio bajo una carpeta.
fn recorrer(root: &Path) -> Vec<PathBuf> {
    let mut salida = Vec::new();
    let mut pendientes = vec![root.to_path_buf()];
    while let Some(dir) = pendientes.pop() {
        let Ok(entradas) = std::fs::read_dir(&dir) else {
            // Una subcarpeta ilegible no cancela el escaneo ni desactiva el
            // prune: se salta y ya. En QBZ un lost+found lo desactivaba para
            // siempre.
            continue;
        };
        for entrada in entradas.flatten() {
            let path = entrada.path();
            if path.is_dir() {
                pendientes.push(path);
            } else if es_audio(&path) {
                salida.push(path);
            }
        }
    }
    salida.sort();
    salida
}

fn es_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| EXTENSIONES.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

#[cfg(target_family = "unix")]
fn inodo(meta: &std::fs::Metadata) -> Option<i64> {
    use std::os::unix::fs::MetadataExt;
    Some(meta.ino() as i64)
}

#[cfg(not(target_family = "unix"))]
fn inodo(_: &std::fs::Metadata) -> Option<i64> {
    None
}

fn ahora_unix() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}
