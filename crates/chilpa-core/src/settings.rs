//! Lo que Chilpa recuerda entre sesiones.
//!
//! Un archivo de texto con `clave = valor`, en el directorio de configuración
//! del usuario. Nada de un formato que obligue a una dependencia: lo que se
//! guarda aquí son un puñado de preferencias, y un archivo que se puede abrir
//! con cualquier editor se arregla a mano cuando algo sale mal.
//!
//! Leer nunca falla: un archivo ausente, roto o de otra versión da los
//! valores por omisión. Guardar sí puede fallar, y quien llama decide qué
//! hacer.

use std::collections::BTreeMap;
use std::path::PathBuf;

/// Las preferencias que hoy se recuerdan.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Settings {
    /// El identificador de la última salida usada.
    pub output: String,
    /// Si se pide bit-perfect.
    pub bit_perfect: bool,
    /// La última carpeta que se abrió, para no empezar en el home cada vez.
    pub last_folder: String,
    /// El acomodo de la ventana, por nombre.
    pub layout: String,
    /// La vista rápida con la que se mira la biblioteca.
    pub album_sort: String,
    /// Qué visualizador se dibuja, por nombre.
    pub visualizer: String,
}

/// El visualizador con el que arranca quien no ha elegido ninguno.
pub const DEFAULT_VISUALIZER: &str = "onda";

/// El acomodo con el que arranca quien no ha elegido ninguno.
pub const DEFAULT_LAYOUT: &str = "Clasico";

impl Default for Settings {
    fn default() -> Self {
        Self {
            output: String::new(),
            bit_perfect: true,
            last_folder: String::new(),
            layout: DEFAULT_LAYOUT.to_string(),
            album_sort: String::new(),
            visualizer: DEFAULT_VISUALIZER.to_string(),
        }
    }
}

/// Dónde vive el archivo.
pub fn path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(std::env::temp_dir);
    base.join("chilpa").join("settings.conf")
}

/// Lee las preferencias. Lo que falte o no se entienda toma su valor por
/// omisión.
pub fn load() -> Settings {
    load_from(&path())
}

pub fn load_from(file: &std::path::Path) -> Settings {
    let text = std::fs::read_to_string(file).unwrap_or_default();
    let pares = parse(&text);
    let mut s = Settings::default();
    if let Some(v) = pares.get("output") {
        s.output = v.clone();
    }
    if let Some(v) = pares.get("bit_perfect") {
        s.bit_perfect = v != "false";
    }
    if let Some(v) = pares.get("last_folder") {
        s.last_folder = v.clone();
    }
    // Un layout vacío en el archivo no deja la ventana sin nada que cargar.
    if let Some(v) = pares.get("layout") {
        if !v.is_empty() {
            s.layout = v.clone();
        }
    }
    if let Some(v) = pares.get("album_sort") {
        s.album_sort = v.clone();
    }
    if let Some(v) = pares.get("visualizer") {
        if !v.is_empty() {
            s.visualizer = v.clone();
        }
    }
    s
}

/// Guarda las preferencias, creando el directorio si hace falta.
pub fn save(settings: &Settings) -> std::io::Result<()> {
    save_to(&path(), settings)
}

pub fn save_to(file: &std::path::Path, settings: &Settings) -> std::io::Result<()> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text = format!(
        "# Preferencias de Chilpa. Se puede editar a mano.\noutput = {}\nbit_perfect = {}\nlast_folder = {}\nlayout = {}\nalbum_sort = {}\nvisualizer = {}\n",
        settings.output,
        settings.bit_perfect,
        settings.last_folder,
        settings.layout,
        settings.album_sort,
        settings.visualizer
    );
    std::fs::write(file, text)
}

fn parse(text: &str) -> BTreeMap<String, String> {
    text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporal(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("chilpa-settings-{name}.conf"));
        p
    }

    #[test]
    fn with_no_file_the_defaults_are_used() {
        let s = load_from(&temporal("no-existe"));
        assert_eq!(s, Settings::default());
        assert!(s.bit_perfect, "bit-perfect viene encendido");
    }

    #[test]
    fn what_was_saved_reads_back_the_same() {
        let file = temporal("ida-y-vuelta");
        let s = Settings {
            output: "hw:1,0".into(),
            bit_perfect: false,
            last_folder: "/musica/albumes".into(),
            layout: "Columna".into(),
            album_sort: "played".into(),
            visualizer: "espectro".into(),
        };
        save_to(&file, &s).unwrap();
        assert_eq!(load_from(&file), s);
        let _ = std::fs::remove_file(file);
    }

    #[test]
    fn a_broken_file_brings_nothing_down() {
        let file = temporal("roto");
        std::fs::write(&file, "esto no es\nun archivo válido }{\noutput = hw:0,0\n").unwrap();
        let s = load_from(&file);
        assert_eq!(s.output, "hw:0,0", "se rescata lo que sí se entiende");
        assert!(s.bit_perfect, "lo demás toma su valor por omisión");
        let _ = std::fs::remove_file(file);
    }

    #[test]
    fn with_no_saved_layout_the_window_still_has_something_to_start_with() {
        let file = temporal("sin-layout");
        std::fs::write(&file, "layout =\n").unwrap();
        assert_eq!(load_from(&file).layout, DEFAULT_LAYOUT);
        let _ = std::fs::remove_file(file);
    }

    #[test]
    fn comments_and_whitespace_do_not_get_in_the_way() {
        let file = temporal("comentarios");
        std::fs::write(&file, "# un comentario\n  output   =   hw:2,0  \nbit_perfect = false\n").unwrap();
        let s = load_from(&file);
        assert_eq!(s.output, "hw:2,0");
        assert!(!s.bit_perfect);
        let _ = std::fs::remove_file(file);
    }
}
