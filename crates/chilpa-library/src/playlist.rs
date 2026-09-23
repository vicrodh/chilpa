//! Listas M3U: leerlas y escribirlas.
//!
//! El formato es viejo y simple, y por eso sigue siendo el que todo el mundo
//! entiende. Lo que importa al leerlo:
//!
//! - **Las rutas relativas se resuelven contra la lista**, no contra el
//!   directorio de trabajo. Una lista dentro de la carpeta del álbum es lo
//!   normal, y resolverla mal la deja vacía.
//! - **Una entrada que ya no existe no tumba la lista**: se reporta aparte.
//!   Lo mismo que con la biblioteca, lo ausente se dice, no se borra.

use std::io::Write;
use std::path::{Path, PathBuf};

/// Lo que trae una lista.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Playlist {
    /// Las pistas que existen, en el orden de la lista.
    pub tracks: Vec<PathBuf>,
    /// Las que la lista menciona pero no están en disco.
    pub missing: Vec<PathBuf>,
}

/// Lee una lista M3U o M3U8.
pub fn read(path: &Path) -> std::io::Result<Playlist> {
    let text = std::fs::read_to_string(path)?;
    let base = path.parent().unwrap_or(Path::new("."));
    let mut list = Playlist::default();
    for line in text.lines() {
        let line = line.trim();
        // `#EXTINF` y sus compañeros son metadata: la fuente de verdad es el
        // archivo, así que se saltan.
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // Una URL no es un archivo local; hoy no se abren.
        if line.contains("://") {
            continue;
        }
        let path = PathBuf::from(line);
        let path = if path.is_absolute() { path } else { base.join(path) };
        if path.is_file() {
            list.tracks.push(path);
        } else {
            list.missing.push(path);
        }
    }
    Ok(list)
}

/// Escribe una lista M3U con rutas relativas cuando se puede.
///
/// Relativas porque una lista junto a la música se puede mover con ella; una
/// lista llena de rutas absolutas se rompe en cuanto cambia el disco.
pub fn write_m3u(path: &Path, tracks: &[PathBuf]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let base = path.parent().unwrap_or(Path::new("."));
    let mut f = std::fs::File::create(path)?;
    writeln!(f, "#EXTM3U")?;
    for track in tracks {
        let path = relative(base, track);
        writeln!(f, "{}", path.display())?;
    }
    Ok(())
}

/// La ruta relativa de `track` respecto a `base`, si comparten prefijo.
fn relative(base: &Path, track: &Path) -> PathBuf {
    track.strip_prefix(base).map(PathBuf::from).unwrap_or_else(|_| track.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("chilpa-playlist-{name}"));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn las_rutas_relativas_se_resuelven_contra_la_lista() {
        let dir = folder("relativas");
        std::fs::write(dir.join("a.flac"), b"x").unwrap();
        std::fs::write(dir.join("b.flac"), b"x").unwrap();
        let list = dir.join("album.m3u");
        std::fs::write(&list, "#EXTM3U\n#EXTINF:120,Una\na.flac\nb.flac\n").unwrap();

        let read_back = read(&list).unwrap();
        assert_eq!(read_back.tracks, vec![dir.join("a.flac"), dir.join("b.flac")]);
        assert!(read_back.missing.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn lo_que_ya_no_esta_se_reporta_aparte() {
        let dir = folder("ausentes");
        std::fs::write(dir.join("a.flac"), b"x").unwrap();
        let list = dir.join("l.m3u");
        std::fs::write(&list, "a.flac\nse-fue.flac\n").unwrap();

        let read_back = read(&list).unwrap();
        assert_eq!(read_back.tracks.len(), 1);
        assert_eq!(read_back.missing, vec![dir.join("se-fue.flac")]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn las_urls_se_saltan_por_ahora() {
        let dir = folder("urls");
        let list = dir.join("l.m3u");
        std::fs::write(&list, "http://ejemplo/stream\n").unwrap();
        let read_back = read(&list).unwrap();
        assert!(read_back.tracks.is_empty() && read_back.missing.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn lo_escrito_se_vuelve_a_leer_igual_y_con_rutas_relativas() {
        let dir = folder("ida-y-vuelta");
        std::fs::create_dir_all(dir.join("disco")).unwrap();
        let tracks: Vec<PathBuf> =
            ["disco/01.flac", "disco/02.flac"].iter().map(|n| dir.join(n)).collect();
        for p in &tracks {
            std::fs::write(p, b"x").unwrap();
        }
        let list = dir.join("mia.m3u");
        write_m3u(&list, &tracks).unwrap();

        let text = std::fs::read_to_string(&list).unwrap();
        assert!(text.contains("disco/01.flac"), "se guardó relativa: {text}");
        assert!(!text.contains(dir.to_str().unwrap()), "sin rutas absolutas");

        assert_eq!(read(&list).unwrap().tracks, tracks);
        let _ = std::fs::remove_dir_all(dir);
    }
}
