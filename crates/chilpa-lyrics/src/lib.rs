//! Las letras que ya vienen con la música.
//!
//! Este crate no habla con nadie por la red: busca lo que está en disco, que
//! es de donde salen las letras que de verdad se tienen. Dos lugares, en
//! este orden:
//!
//! 1. Un `.lrc` junto al archivo, con el mismo nombre. Es el que suele traer
//!    los tiempos, y por eso va primero.
//! 2. La letra guardada dentro del archivo, en sus tags.
//!
//! Una letra con tiempos se puede seguir mientras suena; una sin tiempos se
//! muestra completa. Las dos cosas valen, y cuál es cuál se dice, no se
//! adivina.

pub mod lrc;

use std::path::{Path, PathBuf};

/// Un renglón de la letra, con el momento en que entra.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Line {
    /// Milisegundos desde el principio de la pista. Cero en una letra sin
    /// sincronizar.
    pub at_ms: u64,
    pub text: String,
}

/// La letra de una pista.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Lyrics {
    /// Lo que la letra dice de sí misma, cuando lo dice.
    pub title: Option<String>,
    pub artist: Option<String>,
    pub lines: Vec<Line>,
    /// Cierto si trae tiempos y se puede seguir mientras suena.
    pub synced: bool,
}

impl Lyrics {
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Qué renglón va en ese momento, por posición en `lines`.
    ///
    /// Es el último que ya entró. Antes del primero no hay ninguno: mostrar
    /// el primer renglón desde el segundo cero, cuando entra en el 20,
    /// es peor que no mostrar nada.
    pub fn line_at(&self, ms: u64) -> Option<usize> {
        if !self.synced || self.lines.is_empty() {
            return None;
        }
        // `partition_point` da cuántos renglones ya entraron.
        let ya = self.lines.partition_point(|l| l.at_ms <= ms);
        ya.checked_sub(1)
    }

    /// La letra en texto plano, un renglón por línea.
    pub fn plain(&self) -> String {
        self.lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n")
    }
}

/// De dónde salió la letra. Sirve para decirlo en la vista.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Source {
    /// Un archivo `.lrc` junto a la pista.
    Sidecar(PathBuf),
    /// Los tags del archivo.
    Embedded,
}

/// Una letra encontrada, con su procedencia.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Found {
    pub lyrics: Lyrics,
    pub source: Source,
}

/// Busca la letra de una pista.
///
/// Devuelve `None` cuando no hay ninguna, que es distinto de una letra
/// vacía: una letra vacía en los tags no debe tapar al `.lrc` de al lado.
pub fn find(track: &Path) -> Option<Found> {
    if let Some(ruta) = sidecar(track) {
        if let Ok(texto) = std::fs::read_to_string(&ruta) {
            let lyrics = lrc::parse(&texto);
            if !lyrics.is_empty() {
                return Some(Found { lyrics, source: Source::Sidecar(ruta) });
            }
        }
    }
    let texto = embedded(track)?;
    let lyrics = lrc::parse(&texto);
    (!lyrics.is_empty()).then_some(Found { lyrics, source: Source::Embedded })
}

/// El `.lrc` que le toca a una pista, si existe.
///
/// Se prueba `.lrc` y `.LRC`: en un disco que vino de Windows la extensión
/// puede estar en mayúsculas, y en Linux eso sí importa.
pub fn sidecar(track: &Path) -> Option<PathBuf> {
    for ext in ["lrc", "LRC", "Lrc"] {
        let candidato = track.with_extension(ext);
        if candidato.is_file() {
            return Some(candidato);
        }
    }
    None
}

/// La letra guardada dentro del archivo.
fn embedded(track: &Path) -> Option<String> {
    use lofty::file::TaggedFileExt;
    use lofty::prelude::ItemKey;

    let archivo = lofty::probe::Probe::open(track).ok()?.read().ok()?;
    let tag = archivo.primary_tag().or_else(|| archivo.first_tag())?;
    let texto = tag.get_string(ItemKey::Lyrics)?;
    (!texto.trim().is_empty()).then(|| texto.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn carpeta(nombre: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("chilpa-lyrics-{nombre}"));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn el_renglon_que_va_es_el_ultimo_que_entro() {
        let l = lrc::parse("[00:05.00]Uno\n[00:10.00]Dos\n[00:20.00]Tres\n");
        assert_eq!(l.line_at(0), None, "antes del primero no hay ninguno");
        assert_eq!(l.line_at(4_999), None);
        assert_eq!(l.line_at(5_000), Some(0), "justo cuando entra");
        assert_eq!(l.line_at(9_999), Some(0));
        assert_eq!(l.line_at(10_000), Some(1));
        assert_eq!(l.line_at(999_999), Some(2), "y el último se queda");
    }

    #[test]
    fn una_letra_sin_tiempos_no_se_sigue() {
        let l = lrc::parse("Nomás el texto\n");
        assert_eq!(l.line_at(5_000), None);
        assert_eq!(l.plain(), "Nomás el texto");
    }

    #[test]
    fn el_lrc_de_al_lado_es_el_que_manda() {
        let dir = carpeta("sidecar");
        let pista = dir.join("cancion.flac");
        std::fs::write(&pista, b"no importa el contenido").unwrap();
        std::fs::write(dir.join("cancion.lrc"), "[00:01.00]De al lado\n").unwrap();

        let hallada = find(&pista).expect("se encontró");
        assert_eq!(hallada.source, Source::Sidecar(dir.join("cancion.lrc")));
        assert!(hallada.lyrics.synced);
        assert_eq!(hallada.lyrics.lines[0].text, "De al lado");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn sin_letra_no_se_inventa_nada() {
        let dir = carpeta("vacio");
        let pista = dir.join("cancion.flac");
        std::fs::write(&pista, b"tampoco importa").unwrap();
        assert!(find(&pista).is_none());

        // Un .lrc vacío tampoco cuenta como letra.
        std::fs::write(dir.join("cancion.lrc"), "\n\n").unwrap();
        assert!(find(&pista).is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn la_extension_en_mayusculas_tambien_se_encuentra() {
        let dir = carpeta("mayusculas");
        let pista = dir.join("cancion.flac");
        std::fs::write(&pista, b"x").unwrap();
        std::fs::write(dir.join("cancion.LRC"), "[00:01.00]Arriba\n").unwrap();
        assert!(find(&pista).is_some(), "en Linux la extensión distingue");
        let _ = std::fs::remove_dir_all(dir);
    }
}
