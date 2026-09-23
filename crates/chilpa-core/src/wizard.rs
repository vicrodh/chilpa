//! El asistente de alta fidelidad: qué puede y qué no puede tu salida.
//!
//! Es un diagnóstico, no una configuración. No apaga nada, no toca mezclador
//! y no escribe ajustes. Mide el dispositivo y dice, formato por formato, si
//! lo que saldría es lo mismo que entró.
//!
//! El de QBZ hacía lo contrario: escribía una lista de frecuencias supuestas
//! y apagaba el resampleo del sistema. Eso no es saber, es esperar. Aquí
//! todo lo que se dice sale de haber abierto el dispositivo y preguntarle, y
//! si no se pudo medir, se dice que no se pudo.

use chilpa_audio::format::{SampleFormat, StreamFormat};
use chilpa_audio::negotiate::{negotiate, DeviceCaps, Policy};

/// Los formatos que vale la pena preguntar: lo que trae la música que
/// existe, del CD a lo que sale de un estudio.
pub const FORMATOS: &[(&str, SampleFormat, u32, u16)] = &[
    ("CD", SampleFormat::S16, 44_100, 2),
    ("DAT", SampleFormat::S16, 48_000, 2),
    ("Estudio 24/48", SampleFormat::S24In32, 48_000, 2),
    ("Alta 24/88.2", SampleFormat::S24In32, 88_200, 2),
    ("Alta 24/96", SampleFormat::S24In32, 96_000, 2),
    ("Alta 24/176.4", SampleFormat::S24In32, 176_400, 2),
    ("Alta 24/192", SampleFormat::S24In32, 192_000, 2),
    ("Máximo 32/384", SampleFormat::S32, 384_000, 2),
    // DSD64 por DoP viaja como PCM de 24 bits a 176.4 kHz.
    ("DSD64 por DoP", SampleFormat::S24In32, 176_400, 2),
    ("DSD128 por DoP", SampleFormat::S24In32, 352_800, 2),
];

/// Qué pasa con un formato en este dispositivo.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Veredicto {
    /// Sale tal cual entró.
    BitPerfect,
    /// El dispositivo no lo acepta y habría que convertirlo. Se dice a qué.
    Convertido { a: String },
    /// Ni convirtiendo: el dispositivo no ofrece nada parecido.
    NoSePuede,
}

/// Un renglón del diagnóstico.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Fila {
    pub nombre: String,
    /// Como se dice: "24 bits a 96000 Hz, estéreo".
    pub formato: String,
    pub veredicto: Veredicto,
}

/// Lo que el asistente encontró.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Diagnostico {
    pub device: String,
    /// Cierto si las capacidades se midieron abriendo el dispositivo.
    pub probed: bool,
    /// Lo que el dispositivo dijo de sí mismo.
    pub sample_rates: Vec<u32>,
    pub formats: Vec<String>,
    pub channels: Vec<u16>,
    pub filas: Vec<Fila>,
    /// Lo que hay que decirle al usuario, en una línea.
    pub resumen: String,
}

/// Arma el diagnóstico de un dispositivo ya medido.
pub fn diagnosticar(device: &str, caps: &DeviceCaps) -> Diagnostico {
    let mut filas = Vec::with_capacity(FORMATOS.len());
    for (nombre, formato, rate, canales) in FORMATOS {
        let pedido = StreamFormat::new(*formato, *rate, *canales);
        let veredicto = if negotiate(pedido, caps, Policy::BitPerfect).is_ok() {
            Veredicto::BitPerfect
        } else {
            match negotiate(pedido, caps, Policy::AllowAnything) {
                Ok(d) => Veredicto::Convertido { a: describir(d.opened) },
                Err(_) => Veredicto::NoSePuede,
            }
        };
        filas.push(Fila {
            nombre: nombre.to_string(),
            formato: describir(pedido),
            veredicto,
        });
    }

    let bit_perfect = filas.iter().filter(|f| f.veredicto == Veredicto::BitPerfect).count();
    let resumen = if !caps.probed {
        // Sin medir no hay nada que prometer.
        "no se pudo medir el dispositivo; nada de lo de abajo está comprobado".to_string()
    } else if bit_perfect == 0 {
        "este dispositivo no entrega bit-perfect en ningún formato común".to_string()
    } else {
        format!("{bit_perfect} de {} formatos salen tal cual", filas.len())
    };

    Diagnostico {
        device: device.to_string(),
        probed: caps.probed,
        sample_rates: caps.sample_rates.clone(),
        formats: caps.formats.iter().map(|f| nombre_formato(*f)).collect(),
        channels: caps.channels.clone(),
        filas,
        resumen,
    }
}

/// Un formato dicho como se dice.
pub fn describir(f: StreamFormat) -> String {
    format!(
        "{} bits a {} Hz, {}",
        f.sample_format.valid_bits(),
        f.sample_rate_hz,
        match f.channels {
            1 => "mono".to_string(),
            2 => "estéreo".to_string(),
            n => format!("{n} canales"),
        }
    )
}

fn nombre_formato(f: SampleFormat) -> String {
    match f {
        SampleFormat::S16 => "16 bits".into(),
        SampleFormat::S24Packed => "24 bits en 3 bytes".into(),
        SampleFormat::S24In32 => "24 bits en 32".into(),
        SampleFormat::S32 => "32 bits".into(),
        SampleFormat::F32 => "flotante de 32".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps(rates: &[u32], formats: &[SampleFormat], probed: bool) -> DeviceCaps {
        DeviceCaps {
            sample_rates: rates.to_vec(),
            formats: formats.to_vec(),
            channels: vec![2],
            probed,
        }
    }

    #[test]
    fn un_dispositivo_de_cd_solo_entrega_cd() {
        let d = diagnosticar("hw:0,0", &caps(&[44_100], &[SampleFormat::S16], true));
        let cd = d.filas.iter().find(|f| f.nombre == "CD").unwrap();
        assert_eq!(cd.veredicto, Veredicto::BitPerfect);

        let alta = d.filas.iter().find(|f| f.nombre == "Alta 24/96").unwrap();
        match &alta.veredicto {
            Veredicto::Convertido { a } => assert!(a.contains("44100"), "se convierte a {a}"),
            otro => panic!("24/96 no cabe en un dispositivo de CD: {otro:?}"),
        }
        assert!(d.resumen.contains("1 de"), "{}", d.resumen);
    }

    #[test]
    fn un_dispositivo_sin_medir_no_promete_nada() {
        let d = diagnosticar(
            "hw:0,0",
            &caps(&[44_100, 96_000], &[SampleFormat::S16, SampleFormat::S24In32], false),
        );
        assert!(!d.probed);
        assert!(d.resumen.contains("no se pudo medir"), "{}", d.resumen);
        // Y ninguna fila dice bit-perfect: sin medir, no se promete.
        assert!(
            d.filas.iter().all(|f| f.veredicto != Veredicto::BitPerfect),
            "prometió bit-perfect sin haber medido"
        );
    }

    #[test]
    fn un_dac_de_los_buenos_entrega_casi_todo() {
        let d = diagnosticar(
            "hw:1,0",
            &caps(
                &[44_100, 48_000, 88_200, 96_000, 176_400, 192_000, 352_800],
                &[SampleFormat::S16, SampleFormat::S24In32, SampleFormat::S32],
                true,
            ),
        );
        let perfectos: Vec<&str> = d
            .filas
            .iter()
            .filter(|f| f.veredicto == Veredicto::BitPerfect)
            .map(|f| f.nombre.as_str())
            .collect();
        assert!(perfectos.contains(&"CD"));
        assert!(perfectos.contains(&"Alta 24/192"));
        assert!(perfectos.contains(&"DSD64 por DoP"), "DoP va a 176.4 kHz");
        assert!(perfectos.contains(&"DSD128 por DoP"), "y DSD128 a 352.8");
        // 384 kHz no está en la lista del dispositivo.
        let maximo = d.filas.iter().find(|f| f.nombre == "Máximo 32/384").unwrap();
        assert_ne!(maximo.veredicto, Veredicto::BitPerfect);
    }

    #[test]
    fn un_formato_dicho_como_se_dice() {
        let f = StreamFormat::new(SampleFormat::S24In32, 96_000, 2);
        assert_eq!(describir(f), "24 bits a 96000 Hz, estéreo");
        let mono = StreamFormat::new(SampleFormat::S16, 44_100, 1);
        assert_eq!(describir(mono), "16 bits a 44100 Hz, mono");
    }
}
