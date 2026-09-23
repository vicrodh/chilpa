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
}

/// El acomodo con el que arranca quien no ha elegido ninguno.
pub const LAYOUT_POR_OMISION: &str = "Clasico";

impl Default for Settings {
    fn default() -> Self {
        Self {
            output: String::new(),
            bit_perfect: true,
            last_folder: String::new(),
            layout: LAYOUT_POR_OMISION.to_string(),
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
    let texto = std::fs::read_to_string(file).unwrap_or_default();
    let pares = parse(&texto);
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
    let texto = format!(
        "# Preferencias de Chilpa. Se puede editar a mano.\noutput = {}\nbit_perfect = {}\nlast_folder = {}\nlayout = {}\n",
        settings.output, settings.bit_perfect, settings.last_folder, settings.layout
    );
    std::fs::write(file, texto)
}

fn parse(texto: &str) -> BTreeMap<String, String> {
    texto
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

    fn temporal(nombre: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("chilpa-settings-{nombre}.conf"));
        p
    }

    #[test]
    fn sin_archivo_se_usan_los_valores_por_omision() {
        let s = load_from(&temporal("no-existe"));
        assert_eq!(s, Settings::default());
        assert!(s.bit_perfect, "bit-perfect viene encendido");
    }

    #[test]
    fn lo_guardado_se_vuelve_a_leer_igual() {
        let file = temporal("ida-y-vuelta");
        let s = Settings {
            output: "hw:1,0".into(),
            bit_perfect: false,
            last_folder: "/musica/albumes".into(),
            layout: "Columna".into(),
        };
        save_to(&file, &s).unwrap();
        assert_eq!(load_from(&file), s);
        let _ = std::fs::remove_file(file);
    }

    #[test]
    fn un_archivo_roto_no_tumba_nada() {
        let file = temporal("roto");
        std::fs::write(&file, "esto no es\nun archivo válido }{\noutput = hw:0,0\n").unwrap();
        let s = load_from(&file);
        assert_eq!(s.output, "hw:0,0", "se rescata lo que sí se entiende");
        assert!(s.bit_perfect, "lo demás toma su valor por omisión");
        let _ = std::fs::remove_file(file);
    }

    #[test]
    fn sin_layout_guardado_la_ventana_tiene_con_que_arrancar() {
        let file = temporal("sin-layout");
        std::fs::write(&file, "layout =\n").unwrap();
        assert_eq!(load_from(&file).layout, LAYOUT_POR_OMISION);
        let _ = std::fs::remove_file(file);
    }

    #[test]
    fn los_comentarios_y_los_espacios_no_estorban() {
        let file = temporal("comentarios");
        std::fs::write(&file, "# un comentario\n  output   =   hw:2,0  \nbit_perfect = false\n").unwrap();
        let s = load_from(&file);
        assert_eq!(s.output, "hw:2,0");
        assert!(!s.bit_perfect);
        let _ = std::fs::remove_file(file);
    }
}
