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

const EXTENSIONS: &[&str] =
    &["flac", "wav", "aiff", "aif", "m4a", "mp4", "alac", "mp3", "ogg", "oga", "dsf"];

/// Escanea una carpeta y sus subcarpetas, dejando las portadas en `cache`.
pub fn scan_root_with_cache(
    lib: &Library,
    root: &Path,
    cache: Option<&Path>,
) -> rusqlite::Result<ScanReport> {
    scan(lib, root, cache, &|_| {})
}

/// Escanea una carpeta y sus subcarpetas.
pub fn scan_root(lib: &Library, root: &Path) -> rusqlite::Result<ScanReport> {
    scan(lib, root, None, &|_| {})
}

/// Lo mismo, avisando cuántos archivos lleva mirados.
pub fn scan_root_reporting(
    lib: &Library,
    root: &Path,
    cache: Option<&Path>,
    on_progress: &dyn Fn(usize),
) -> rusqlite::Result<ScanReport> {
    scan(lib, root, cache, on_progress)
}

/// Cuántos archivos de audio hay bajo una carpeta. No toca la base.
pub fn count_audio_files(root: &Path) -> usize {
    walk(root).len()
}

fn scan(
    lib: &Library,
    root: &Path,
    cache: Option<&Path>,
    on_progress: &dyn Fn(usize),
) -> rusqlite::Result<ScanReport> {
    let mut report = ScanReport { root: root.to_path_buf(), ..ScanReport::default() };
    let source = lib.source_id("local", "default")?;
    let root_id = lib.root_id(source, root)?;

    if !root.is_dir() {
        lib.set_root_reachable(root_id, false)?;
        report.unreachable = true;
        return Ok(report);
    }
    lib.set_root_reachable(root_id, true)?;

    let now = now_unix();
    let files = walk(root);
    report.seen = files.len();

    for (examined, file) in files.iter().enumerate() {
        // Se avisa cada 25 archivos: uno por uno inunda el canal sin que la
        // interfaz pueda dibujar más rápido.
        if examined % 25 == 0 {
            on_progress(examined);
        }
        let Ok(meta) = std::fs::metadata(file) else { continue };
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
        let path = file.display().to_string();

        let known: Option<(i64, i64, Option<i64>)> = lib
            .db
            .query_row(
                "SELECT mtime, size, track_id FROM scanned_files WHERE path = ?1",
                params![path],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .ok();

        if let Some((m, s, track)) = &known {
            if *m == mtime && *s == size && track.is_some() {
                // Igual que la última vez: ni se lee ni se reescribe.
                report.unchanged += 1;
                lib.db.execute(
                    "UPDATE tracks SET present = 1 WHERE id = ?1",
                    params![track.unwrap()],
                )?;
                continue;
            }
        }

        let track_id = save_file(lib, source, file, now, cache)?;
        lib.db.execute(
            "INSERT INTO scanned_files (root_id, path, mtime, size, inode, track_id, scanned_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(path) DO UPDATE SET
                mtime = excluded.mtime, size = excluded.size,
                track_id = excluded.track_id, scanned_at = excluded.scanned_at",
            params![root_id, path, mtime, size, inodo(&meta), track_id, now],
        )?;
        report.read += 1;
    }

    on_progress(files.len());

    // Lo que estaba y ya no: se marca ausente, nunca se borra.
    let seen: Vec<String> = files.iter().map(|p| p.display().to_string()).collect();
    report.missing = mark_missing(lib, root_id, &seen)?;
    lib.db
        .execute("UPDATE roots SET last_scan = ?2 WHERE id = ?1", params![root_id, now])?;
    Ok(report)
}

fn save_file(
    lib: &Library,
    source: i64,
    path: &Path,
    now: i64,
    cache: Option<&Path>,
) -> rusqlite::Result<i64> {
    let info = chilpa_tags::read(path).ok();
    let tags = info.as_ref().map(|i| i.tags.clone()).unwrap_or_default();
    let title = tags.title.clone().unwrap_or_else(|| {
        path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
    });
    let artist = tags.artist_line();
    let album_id = match &tags.album {
        Some(album_title) => {
            let album_artist = tags.album_artist.clone().unwrap_or_else(|| artist.clone());
            let id = album_id(lib, album_title, &album_artist, tags.year())?;
            // La portada del álbum sale del primer archivo que traiga una.
            if let Some(cache) = cache {
                save_cover(lib, id, path, cache)?;
            }
            Some(id)
        }
        None => None,
    };
    let duration = info.as_ref().map(|i| i.duration.as_millis() as i64).unwrap_or(0);

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
            title,
            artist,
            album_id,
            tags.track_number,
            tags.disc_number,
            duration,
            info.as_ref().and_then(|i| i.sample_rate_hz),
            info.as_ref().and_then(|i| i.bit_depth),
            info.as_ref().and_then(|i| i.channels),
            tags.isrc,
            tags.musicbrainz_recording_id,
            now
        ],
    )?;
    let track_id: i64 = lib.db.query_row(
        "SELECT id FROM tracks WHERE source_id = ?1 AND external_id = ?2",
        params![source, path.display().to_string()],
        |r| r.get(0),
    )?;
    // Los géneros van en su tabla: un archivo declara varios, y juntarlos en
    // una cadena hace que ninguno sirva para filtrar.
    lib.set_track_genres(track_id, &tags.genres)?;
    Ok(track_id)
}

/// Nombres de portada que se usan en las carpetas de música.
const COVERS: &[&str] = &[
    "cover.jpg", "cover.jpeg", "cover.png", "folder.jpg", "folder.jpeg", "folder.png",
    "front.jpg", "front.png", "album.jpg", "album.png", "Cover.jpg", "Folder.jpg",
];

/// Le pone portada al álbum si todavía no tiene.
///
/// Primero la del archivo; si no trae, la que esté junto a él en la carpeta,
/// que es como guarda las portadas media biblioteca del mundo.
fn save_cover(lib: &Library, album: i64, path: &Path, cache: &Path) -> rusqlite::Result<()> {
    let has: Option<String> = lib
        .db
        .query_row("SELECT artwork_path FROM albums WHERE id = ?1", params![album], |r| r.get(0))
        .unwrap_or(None);
    if has.is_some() {
        return Ok(());
    }
    let cover = chilpa_tags::extract_artwork(path, cache)
        .ok()
        .flatten()
        .or_else(|| cover_in_folder(path));
    if let Some(cover) = cover {
        lib.db.execute(
            "UPDATE albums SET artwork_path = ?2 WHERE id = ?1",
            params![album, cover.display().to_string()],
        )?;
    }
    Ok(())
}

/// La portada que esté junto al archivo, si la hay.
fn cover_in_folder(path: &Path) -> Option<PathBuf> {
    let dir = path.parent()?;
    for name in COVERS {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    // Cualquier imagen suelta, si no hay una con nombre conocido.
    let mut loose: Vec<PathBuf> = std::fs::read_dir(dir)
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
    loose.sort();
    loose.into_iter().next()
}

fn album_id(lib: &Library, title: &str, artist: &str, año: Option<u32>) -> rusqlite::Result<i64> {
    lib.db.execute(
        "INSERT OR IGNORE INTO albums (title, album_artist, year) VALUES (?1, ?2, ?3)",
        params![title, artist, año],
    )?;
    lib.db.query_row(
        "SELECT id FROM albums WHERE title = ?1 AND album_artist = ?2",
        params![title, artist],
        |r| r.get(0),
    )
}

/// Marca ausentes los archivos del root que ya no aparecieron.
fn mark_missing(lib: &Library, root_id: i64, seen: &[String]) -> rusqlite::Result<usize> {
    let mut stmt = lib
        .db
        .prepare("SELECT path, track_id FROM scanned_files WHERE root_id = ?1")?;
    let rows: Vec<(String, Option<i64>)> = stmt
        .query_map(params![root_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut missing = 0;
    for (path, track) in rows {
        if seen.iter().any(|v| v == &path) {
            continue;
        }
        if let Some(id) = track {
            lib.db.execute("UPDATE tracks SET present = 0 WHERE id = ?1", params![id])?;
        }
        lib.db.execute("DELETE FROM scanned_files WHERE path = ?1", params![path])?;
        missing += 1;
    }
    Ok(missing)
}

/// Todos los archivos de audio bajo una carpeta.
fn walk(root: &Path) -> Vec<PathBuf> {
    let mut output = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            // Una subcarpeta ilegible no cancela el escaneo ni desactiva el
            // prune: se salta y ya. En QBZ un lost+found lo desactivaba para
            // siempre.
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if is_audio(&path) {
                output.push(path);
            }
        }
    }
    output.sort();
    output
}

fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| EXTENSIONS.contains(&e.to_lowercase().as_str()))
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

fn now_unix() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}
