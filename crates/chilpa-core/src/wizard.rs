//! The high fidelity wizard: what your output can and cannot do.
//!
//! It is a diagnosis, not a configuration. It turns nothing off, touches no
//! mixer and writes no settings. It measures the device and says, format by
//! format, whether what would come out is the same as what went in.
//!
//! The one in QBZ did the opposite: it wrote a list of assumed rates and
//! turned off the system's resampling. That is not knowing, it is hoping.
//! Here everything said comes from having opened the device and asked it,
//! and if it could not be measured, it says so.

use chilpa_audio::format::{SampleFormat, StreamFormat};
use chilpa_audio::negotiate::{negotiate, DeviceCaps, Policy};

/// The formats worth asking about: what the music that exists comes in,
/// from CD to what comes out of a studio.
pub const FORMATS: &[(&str, SampleFormat, u32, u16)] = &[
    ("CD", SampleFormat::S16, 44_100, 2),
    ("DAT", SampleFormat::S16, 48_000, 2),
    ("Estudio 24/48", SampleFormat::S24In32, 48_000, 2),
    ("Alta 24/88.2", SampleFormat::S24In32, 88_200, 2),
    ("Alta 24/96", SampleFormat::S24In32, 96_000, 2),
    ("Alta 24/176.4", SampleFormat::S24In32, 176_400, 2),
    ("Alta 24/192", SampleFormat::S24In32, 192_000, 2),
    ("Máximo 32/384", SampleFormat::S32, 384_000, 2),
    // DSD64 over DoP travels as 24 bit PCM at 176.4 kHz.
    ("DSD64 por DoP", SampleFormat::S24In32, 176_400, 2),
    ("DSD128 por DoP", SampleFormat::S24In32, 352_800, 2),
];

/// What happens to a format on this device.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Verdict {
    /// It comes out exactly as it went in.
    BitPerfect,
    /// The device does not accept it and it would have to be converted. Says to what.
    Converted { a: String },
    /// Not even converting: the device offers nothing close.
    Unsupported,
}

/// One line of the diagnosis.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Row {
    pub name: String,
    /// As it is said: "24 bit at 96000 Hz, stereo".
    pub format: String,
    pub verdict: Verdict,
}

/// What the wizard found.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Diagnosis {
    pub device: String,
    /// True if the capabilities were measured by opening the device.
    pub probed: bool,
    /// What the device said about itself.
    pub sample_rates: Vec<u32>,
    pub formats: Vec<String>,
    pub channels: Vec<u16>,
    pub rows: Vec<Row>,
    /// What to tell the user, in one line.
    pub summary: String,
}

/// Builds the diagnosis of an already measured device.
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
        // With nothing measured there is nothing to promise.
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

/// A format said the way it is said.
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
    fn a_cd_device_delivers_cd_and_nothing_more() {
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
    fn an_unmeasured_device_promises_nothing() {
        let d = diagnose(
            "hw:0,0",
            &caps(&[44_100, 96_000], &[SampleFormat::S16, SampleFormat::S24In32], false),
        );
        assert!(!d.probed);
        assert!(d.summary.contains("no se pudo medir"), "{}", d.summary);
        // And no row says bit-perfect: nothing measured, nothing promised.
        assert!(
            d.rows.iter().all(|f| f.verdict != Verdict::BitPerfect),
            "prometió bit-perfect sin haber medido"
        );
    }

    #[test]
    fn a_good_dac_delivers_nearly_everything() {
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
        // 384 kHz is not on the device's list.
        let max = d.rows.iter().find(|f| f.name == "Máximo 32/384").unwrap();
        assert_ne!(max.verdict, Verdict::BitPerfect);
    }

    #[test]
    fn a_format_spelled_the_way_it_is_spoken() {
        let f = StreamFormat::new(SampleFormat::S24In32, 96_000, 2);
        assert_eq!(describe(f), "24 bits a 96000 Hz, estéreo");
        let mono = StreamFormat::new(SampleFormat::S16, 44_100, 1);
        assert_eq!(describe(mono), "16 bits a 44100 Hz, mono");
    }
}
