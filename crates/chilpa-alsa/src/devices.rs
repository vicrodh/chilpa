//! Qué salidas hay en la máquina.
//!
//! Solo se listan dispositivos directos (`hw:tarjeta,dispositivo`), porque
//! son los únicos por los que Chilpa reproduce. Lo que la lista dice de cada
//! uno sale de abrirlo y medirlo, no de adivinar.

use crate::probe;
use chilpa_audio::negotiate::DeviceCaps;
use std::fs;

/// Una salida disponible.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Device {
    /// Lo que se le pasa a ALSA: `hw:0,0`.
    pub id: String,
    /// Nombre de la tarjeta, como lo reporta el sistema.
    pub card: String,
    /// Nombre del dispositivo dentro de la tarjeta.
    pub name: String,
}

impl Device {
    /// Cómo se ve en una lista: "ALC3254 Analog (hw:0,0)".
    pub fn label(&self) -> String {
        format!("{} ({})", self.name, self.id)
    }
}

/// Lista las salidas directas leyendo `/proc/asound`.
///
/// Se usa `/proc` en vez de la enumeración de ALSA porque esta lista no debe
/// abrir nada: abrir un dispositivo ocupado se lo quita a quien lo esté
/// usando. Medir capacidades sí abre, y por eso va aparte ([`capabilities`]).
pub fn list() -> Vec<Device> {
    let mut salidas = Vec::new();
    let Ok(cards) = fs::read_to_string("/proc/asound/cards") else {
        return salidas;
    };
    for linea in cards.lines() {
        // " 0 [PCH            ]: HDA-Intel - HDA Intel PCH"
        let Some((numero, resto)) = linea.trim_start().split_once(' ') else {
            continue;
        };
        let Ok(card) = numero.parse::<u32>() else {
            continue;
        };
        let nombre_tarjeta = resto
            .split_once(':')
            .map(|(_, d)| d.trim())
            .unwrap_or(resto)
            .to_string();
        for dispositivo in dispositivos_de_la_tarjeta(card) {
            salidas.push(Device {
                id: format!("hw:{card},{}", dispositivo.0),
                card: nombre_tarjeta.clone(),
                name: dispositivo.1,
            });
        }
    }
    salidas
}

/// Los dispositivos de reproducción de una tarjeta, con su nombre.
fn dispositivos_de_la_tarjeta(card: u32) -> Vec<(u32, String)> {
    let mut out = Vec::new();
    let Ok(dir) = fs::read_dir(format!("/proc/asound/card{card}")) else {
        return out;
    };
    let mut entradas: Vec<_> = dir
        .flatten()
        .filter_map(|e| {
            let nombre = e.file_name().to_string_lossy().to_string();
            // Los de reproducción terminan en "p": pcm0p, pcm3p...
            let resto = nombre.strip_prefix("pcm")?.strip_suffix('p')?;
            resto.parse::<u32>().ok().map(|n| (n, nombre))
        })
        .collect();
    entradas.sort_by_key(|(n, _)| *n);
    for (numero, carpeta) in entradas {
        let info = fs::read_to_string(format!("/proc/asound/card{card}/{carpeta}/info"))
            .unwrap_or_default();
        let nombre = info
            .lines()
            .find_map(|l| l.strip_prefix("name: "))
            .unwrap_or("salida")
            .trim()
            .to_string();
        out.push((numero, nombre));
    }
    out
}

/// Mide un dispositivo abriéndolo. Falla si está ocupado.
pub fn capabilities(id: &str) -> Result<DeviceCaps, chilpa_audio::sink::SinkError> {
    probe(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_etiqueta_junta_nombre_e_identificador() {
        let d = Device {
            id: "hw:0,0".into(),
            card: "HDA Intel PCH".into(),
            name: "ALC3254 Analog".into(),
        };
        assert_eq!(d.label(), "ALC3254 Analog (hw:0,0)");
    }

    #[test]
    fn listar_no_truena_aunque_no_haya_tarjetas() {
        // En una máquina sin audio la lista sale vacía, no en error.
        let _ = list();
    }

    #[test]
    fn todos_los_identificadores_son_directos() {
        for d in list() {
            assert!(d.id.starts_with("hw:"), "{} no es un dispositivo directo", d.id);
        }
    }
}
