//! Estimar la tonalidad de una pista.
//!
//! Se arma un cromagrama —cuánta energía hay en cada una de las doce notas,
//! sin importar la octava— y se compara contra los perfiles de
//! Krumhansl-Schmuckler, que son los que salieron de preguntarle a gente
//! qué tan bien encaja cada nota en un tono. La rotación que mejor
//! correlaciona es la tonalidad.
//!
//! Lo que hay que decir de esto:
//!
//! - **Se equivoca con lo relativo.** Do mayor y la menor usan las mismas
//!   notas; lo que las distingue es cuál se siente como reposo, y eso no
//!   está en el cromagrama. Por eso se reporta la confianza.
//! - **Se equivoca con música que cambia de tono.** Devuelve el tono que
//!   más pesa en lo que se miró, no una historia.
//! - **Sale en dos notaciones.** La musical ("F#m") y la de Camelot ("11A"),
//!   que es con la que se mezcla.

use std::f32::consts::PI;

/// Los nombres de las doce notas, con sostenidos.
const NOTES: [&str; 12] =
    ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

/// Perfiles de Krumhansl-Kessler para mayor y menor.
const MAYOR: [f32; 12] = [
    6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88,
];
const MINOR: [f32; 12] = [
    6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17,
];

/// Cuántas muestras entran en cada ventana. A 11 025 Hz son 0.74 s, que dan
/// 1.35 Hz por bin: tres bins por semitono en el registro grave.
const WINDOW: usize = 8192;

/// La tonalidad que se encontró.
#[derive(Clone, PartialEq, Debug)]
pub struct Key {
    /// Como se escribe: "C", "F#m".
    pub name: String,
    /// Como se mezcla: "8B", "11A".
    pub camelot: String,
    /// De 0 a 1. Por debajo de 0.6 el perfil apenas distingue una cosa de
    /// otra y más vale no afirmar nada.
    pub confidence: f32,
}

/// Estima la tonalidad de una señal mono.
pub fn detect(samples: &[f32], sample_rate_hz: u32) -> Option<Key> {
    let chroma = chroma(samples, sample_rate_hz)?;
    let (index, is_major, confidence) = best_profile(&chroma)?;
    Some(Key {
        name: name(index, is_major),
        camelot: camelot(index, is_major),
        confidence: confidence,
    })
}

/// Cuánta energía hay en cada nota, sumando todas las octavas.
pub fn chroma(samples: &[f32], sample_rate_hz: u32) -> Option<[f32; 12]> {
    if sample_rate_hz == 0 || samples.len() < WINDOW {
        return None;
    }
    let half = WINDOW / 2;
    let per_bin = sample_rate_hz as f32 / WINDOW as f32;
    // Se mira de 65 Hz (C2) a 2100 Hz (C7): abajo está el ruido del bombo y
    // arriba los armónicos ya no dicen de qué tono es.
    let k_min = ((65.0 / per_bin) as usize).max(1);
    let k_max = ((2100.0 / per_bin) as usize).min(half - 1);
    if k_max <= k_min {
        return None;
    }

    let mut chroma = [0.0f32; 12];
    let skip = WINDOW / 2;
    let mut start = 0;
    let mut windows = 0usize;
    while start + WINDOW <= samples.len() {
        let mut re: Vec<f32> = Vec::with_capacity(WINDOW);
        for (i, m) in samples[start..start + WINDOW].iter().enumerate() {
            let hann = 0.5 - 0.5 * (2.0 * PI * i as f32 / (WINDOW as f32 - 1.0)).cos();
            re.push(m * hann);
        }
        let mut im = vec![0.0f32; WINDOW];
        chilpa_audio::spectrum::fft(&mut re, &mut im);

        for k in k_min..=k_max {
            let magnitude = (re[k] * re[k] + im[k] * im[k]).sqrt();
            if magnitude <= 0.0 {
                continue;
            }
            let hz = k as f32 * per_bin;
            // Nota MIDI: 69 es el la de 440 Hz.
            let midi = 69.0 + 12.0 * (hz / 440.0).log2();
            let class = (midi.round() as i32).rem_euclid(12) as usize;
            chroma[class] += magnitude;
        }
        windows += 1;
        start += skip;
    }
    if windows == 0 {
        return None;
    }
    let total: f32 = chroma.iter().sum();
    if total <= 0.0 {
        return None;
    }
    for c in chroma.iter_mut() {
        *c /= total;
    }
    Some(chroma)
}

/// La rotación y el modo que mejor correlacionan, con su confianza.
fn best_profile(chroma: &[f32; 12]) -> Option<(usize, bool, f32)> {
    let mut results: Vec<(usize, bool, f32)> = Vec::with_capacity(24);
    for tonic in 0..12 {
        for (is_major, profile) in [(true, &MAYOR), (false, &MINOR)] {
            let rotated: Vec<f32> = (0..12).map(|i| profile[(i + 12 - tonic) % 12]).collect();
            results.push((tonic, is_major, correlation(chroma, &rotated)));
        }
    }
    results.sort_by(|a, b| b.2.total_cmp(&a.2));
    let best = results.first().copied()?;
    if !best.2.is_finite() || best.2 <= 0.0 {
        return None;
    }
    // La confianza es qué tanto le gana al segundo lugar: un cromagrama que
    // encaja igual de bien en dos tonos no dice nada.
    let second = results.get(1).map(|r| r.2).unwrap_or(0.0).max(0.0);
    let confidence = ((best.2 - second) / best.2).clamp(0.0, 1.0);
    Some((best.0, best.1, confidence))
}

/// Correlación de Pearson entre dos vectores del mismo largo.
fn correlation(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len() as f32;
    let (ma, mb) = (a.iter().sum::<f32>() / n, b.iter().sum::<f32>() / n);
    let mut num = 0.0f32;
    let (mut da, mut db) = (0.0f32, 0.0f32);
    for (x, y) in a.iter().zip(b) {
        num += (x - ma) * (y - mb);
        da += (x - ma) * (x - ma);
        db += (y - mb) * (y - mb);
    }
    let den = (da * db).sqrt();
    if den <= 0.0 {
        0.0
    } else {
        num / den
    }
}

fn name(index: usize, is_major: bool) -> String {
    format!("{}{}", NOTES[index % 12], if is_major { "" } else { "m" })
}

/// La rueda de Camelot: el número dice qué tan lejos está en el círculo de
/// quintas, y la letra si es menor (A) o mayor (B).
fn camelot(index: usize, is_major: bool) -> String {
    // El círculo de quintas empezando en do. Do mayor es 8B y la menor es
    // 8A, y de ahí se cuenta.
    let quintas = [0usize, 7, 2, 9, 4, 11, 6, 1, 8, 3, 10, 5];
    let pos = quintas.iter().position(|q| *q == index % 12).unwrap_or(0);
    let number = if is_major {
        // Do está en la posición 0 y le toca el 8.
        (pos + 7) % 12 + 1
    } else {
        // La está en la posición 3 y le toca el 8: la rueda menor va tres
        // quintas atrás de la mayor.
        (pos + 4) % 12 + 1
    };
    format!("{number}{}", if is_major { "B" } else { "A" })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// La frecuencia de una nota MIDI.
    fn hz(midi: f32) -> f32 {
        440.0 * 2f32.powf((midi - 69.0) / 12.0)
    }

    /// Un acorde sostenido con esas notas MIDI.
    fn chord(notes: &[f32], rate: u32, seconds: u32) -> Vec<f32> {
        let total = (rate * seconds) as usize;
        (0..total)
            .map(|i| {
                notes
                    .iter()
                    .map(|n| (2.0 * PI * hz(*n) * i as f32 / rate as f32).sin())
                    .sum::<f32>()
                    / notes.len() as f32
            })
            .collect()
    }

    #[test]
    fn c_major_comes_out_as_c_major() {
        // Las siete notas de la escala de do, para que el perfil tenga con
        // qué trabajar: con una tríada sola, do mayor y la menor empatan.
        let scale: Vec<f32> = [60.0, 62.0, 64.0, 65.0, 67.0, 69.0, 71.0].to_vec();
        // Pesando la tónica y la dominante, como en la música de verdad.
        let mut notes = scale.clone();
        notes.extend_from_slice(&[60.0, 67.0, 60.0]);
        let k = detect(&chord(&notes, 11_025, 6), 11_025).expect("hay tonalidad");
        assert_eq!(k.name, "C", "salió {} ({})", k.name, k.camelot);
        assert_eq!(k.camelot, "8B");
    }

    #[test]
    fn a_minor_comes_out_as_a_minor() {
        let scale: Vec<f32> = [57.0, 59.0, 60.0, 62.0, 64.0, 65.0, 67.0].to_vec();
        let mut notes = scale.clone();
        // En menor, lo que manda es la tónica y la tercera menor.
        notes.extend_from_slice(&[57.0, 60.0, 57.0, 64.0]);
        let k = detect(&chord(&notes, 11_025, 6), 11_025).expect("hay tonalidad");
        assert_eq!(k.name, "Am", "salió {} ({})", k.name, k.camelot);
        assert_eq!(k.camelot, "8A");
    }

    #[test]
    fn silence_has_no_key() {
        assert!(detect(&vec![0.0; 11_025 * 4], 11_025).is_none());
    }

    #[test]
    fn too_little_material_invents_no_key() {
        assert!(detect(&[0.1; 100], 11_025).is_none());
        assert!(detect(&[0.1; 20_000], 0).is_none());
    }

    #[test]
    fn the_camelot_wheel_wraps_all_the_way_around() {
        // Las veinticuatro casillas, sin repetir ni faltar.
        let mut vistas: Vec<String> = Vec::new();
        for i in 0..12 {
            for mayor in [true, false] {
                vistas.push(camelot(i, mayor));
            }
        }
        vistas.sort();
        vistas.dedup();
        assert_eq!(vistas.len(), 24, "hay 24 casillas distintas");
        // Y las dos anclas de siempre.
        assert_eq!(camelot(0, true), "8B", "do mayor");
        assert_eq!(camelot(9, false), "8A", "la menor");
        assert_eq!(camelot(7, true), "9B", "sol mayor, una quinta arriba");
        assert_eq!(camelot(0, false), "5A", "do menor");
        assert_eq!(camelot(6, false), "11A", "fa sostenido menor");
        // Cada tono mayor comparte casilla con su relativo menor.
        for i in 0..12usize {
            let mayor = camelot(i, true);
            let relative = camelot((i + 9) % 12, false);
            assert_eq!(
                mayor[..mayor.len() - 1],
                relative[..relative.len() - 1],
                "{} y su relativo menor comparten número",
                NOTES[i]
            );
        }
    }

    #[test]
    fn a_flat_chromagram_picks_nothing() {
        // Todas las notas igual de fuertes: no hay tónica que valga.
        let flat = [1.0f32 / 12.0; 12];
        let (_, _, confidence) = best_profile(&flat).unwrap_or((0, true, 0.0));
        assert!(confidence < 0.2, "no debería estar seguro: {confidence}");
    }
}
