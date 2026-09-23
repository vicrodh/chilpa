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
pub const FORMATS: &[(&str, SampleFormat, u32, u16)] = &[
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
pub enum Verdict {
    /// Sale tal cual entró.
    BitPerfect,
    /// El dispositivo no lo acepta y habría que convertirlo. Se dice a qué.
    Converted { a: String },
    /// Ni convirtiendo: el dispositivo no ofrece nada parecido.
    Unsupported,
}

/// Un renglón del diagnóstico.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Row {
    pub name: String,
    /// Como se dice: "24 bits a 96000 Hz, estéreo".
    pub format: String,
    pub verdict: Verdict,
}

/// Lo que el asistente encontró.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Diagnosis {
    pub device: String,
    /// Cierto si las capacidades se midieron abriendo el dispositivo.
    pub probed: bool,
    /// Lo que el dispositivo dijo de sí mismo.
    pub sample_rates: Vec<u32>,
    pub formats: Vec<String>,
    pub channels: Vec<u16>,
    pub rows: Vec<Row>,
    /// Lo que hay que decirle al usuario, en una línea.
    pub summary: String,
}

/// Arma el diagnóstico de un dispositivo ya medido.
pub fn diagnose(device: &str, caps: &DeviceCaps) -> Diagnosis {
    let mut rows = Vec::with_capacity(FORMATS.len());
    for (name, format, rate, channels) in FORMATS {
        let request = StreamFormat::new(*format, *rate, *channels);
        let verdict = if negotiate(request, caps, Policy::BitPerfect).is_ok() {
            Verdict::BitPerfect
        } else {
            match negotiate(request, caps, Policy::AllowAnything) {
                Ok(d) => Verdict::Converted { a: describe(d.opened) },
                Err(_) => Verdict::Unsupported,
            }
        };
        rows.push(Row {
            name: name.to_string(),
            format: describe(request),
            verdict,
        });
    }

    let bit_perfect = rows.iter().filter(|f| f.verdict == Verdict::BitPerfect).count();
    let summary = if !caps.probed {
        // Sin medir no hay nada que prometer.
        "no se pudo medir el dispositivo; nada de lo de abajo está comprobado".to_string()
    } else if bit_perfect == 0 {
        "este dispositivo no entrega bit-perfect en ningún formato común".to_string()
    } else {
        format!("{bit_perfect} de {} formatos salen tal cual", rows.len())
    };

    Diagnosis {
        device: device.to_string(),
        probed: caps.probed,
        sample_rates: caps.sample_rates.clone(),
        formats: caps.formats.iter().map(|f| format_name(*f)).collect(),
        channels: caps.channels.clone(),
        rows,
        summary,
    }
}

/// Un formato dicho como se dice.
pub fn describe(f: StreamFormat) -> String {
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

fn format_name(f: SampleFormat) -> String {
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
        let d = diagnose("hw:0,0", &caps(&[44_100], &[SampleFormat::S16], true));
        let cd = d.rows.iter().find(|f| f.name == "CD").unwrap();
        assert_eq!(cd.verdict, Verdict::BitPerfect);

        let high = d.rows.iter().find(|f| f.name == "Alta 24/96").unwrap();
        match &high.verdict {
            Verdict::Converted { a } => assert!(a.contains("44100"), "se convierte a {a}"),
            other => panic!("24/96 no cabe en un dispositivo de CD: {other:?}"),
        }
        assert!(d.summary.contains("1 de"), "{}", d.summary);
    }

    #[test]
    fn un_dispositivo_sin_medir_no_promete_nada() {
        let d = diagnose(
            "hw:0,0",
            &caps(&[44_100, 96_000], &[SampleFormat::S16, SampleFormat::S24In32], false),
        );
        assert!(!d.probed);
        assert!(d.summary.contains("no se pudo medir"), "{}", d.summary);
        // Y ninguna fila dice bit-perfect: sin medir, no se promete.
        assert!(
            d.rows.iter().all(|f| f.verdict != Verdict::BitPerfect),
            "prometió bit-perfect sin haber medido"
        );
    }

    #[test]
    fn un_dac_de_los_buenos_entrega_casi_todo() {
        let d = diagnose(
            "hw:1,0",
            &caps(
                &[44_100, 48_000, 88_200, 96_000, 176_400, 192_000, 352_800],
                &[SampleFormat::S16, SampleFormat::S24In32, SampleFormat::S32],
                true,
            ),
        );
        let perfectos: Vec<&str> = d
            .rows
            .iter()
            .filter(|f| f.verdict == Verdict::BitPerfect)
            .map(|f| f.name.as_str())
            .collect();
        assert!(perfectos.contains(&"CD"));
        assert!(perfectos.contains(&"Alta 24/192"));
        assert!(perfectos.contains(&"DSD64 por DoP"), "DoP va a 176.4 kHz");
        assert!(perfectos.contains(&"DSD128 por DoP"), "y DSD128 a 352.8");
        // 384 kHz no está en la lista del dispositivo.
        let max = d.rows.iter().find(|f| f.name == "Máximo 32/384").unwrap();
        assert_ne!(max.verdict, Verdict::BitPerfect);
    }

    #[test]
    fn un_formato_dicho_como_se_dice() {
        let f = StreamFormat::new(SampleFormat::S24In32, 96_000, 2);
        assert_eq!(describe(f), "24 bits a 96000 Hz, estéreo");
        let mono = StreamFormat::new(SampleFormat::S16, 44_100, 1);
        assert_eq!(describe(mono), "16 bits a 44100 Hz, mono");
    }
}
