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
const NOTAS: [&str; 12] =
    ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

/// Perfiles de Krumhansl-Kessler para mayor y menor.
const MAYOR: [f32; 12] = [
    6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88,
];
const MENOR: [f32; 12] = [
    6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17,
];

/// Cuántas muestras entran en cada ventana. A 11 025 Hz son 0.74 s, que dan
/// 1.35 Hz por bin: tres bins por semitono en el registro grave.
const VENTANA: usize = 8192;

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
    let croma = chroma(samples, sample_rate_hz)?;
    let (indice, es_mayor, confianza) = mejor_perfil(&croma)?;
    Some(Key {
        name: nombre(indice, es_mayor),
        camelot: camelot(indice, es_mayor),
        confidence: confianza,
    })
}

/// Cuánta energía hay en cada nota, sumando todas las octavas.
pub fn chroma(samples: &[f32], sample_rate_hz: u32) -> Option<[f32; 12]> {
    if sample_rate_hz == 0 || samples.len() < VENTANA {
        return None;
    }
    let mitad = VENTANA / 2;
    let por_bin = sample_rate_hz as f32 / VENTANA as f32;
    // Se mira de 65 Hz (C2) a 2100 Hz (C7): abajo está el ruido del bombo y
    // arriba los armónicos ya no dicen de qué tono es.
    let k_min = ((65.0 / por_bin) as usize).max(1);
    let k_max = ((2100.0 / por_bin) as usize).min(mitad - 1);
    if k_max <= k_min {
        return None;
    }

    let mut croma = [0.0f32; 12];
    let salto = VENTANA / 2;
    let mut inicio = 0;
    let mut ventanas = 0usize;
    while inicio + VENTANA <= samples.len() {
        let mut re: Vec<f32> = Vec::with_capacity(VENTANA);
        for (i, m) in samples[inicio..inicio + VENTANA].iter().enumerate() {
            let hann = 0.5 - 0.5 * (2.0 * PI * i as f32 / (VENTANA as f32 - 1.0)).cos();
            re.push(m * hann);
        }
        let mut im = vec![0.0f32; VENTANA];
        chilpa_audio::spectrum::fft(&mut re, &mut im);

        for k in k_min..=k_max {
            let magnitud = (re[k] * re[k] + im[k] * im[k]).sqrt();
            if magnitud <= 0.0 {
                continue;
            }
            let hz = k as f32 * por_bin;
            // Nota MIDI: 69 es el la de 440 Hz.
            let midi = 69.0 + 12.0 * (hz / 440.0).log2();
            let clase = (midi.round() as i32).rem_euclid(12) as usize;
            croma[clase] += magnitud;
        }
        ventanas += 1;
        inicio += salto;
    }
    if ventanas == 0 {
        return None;
    }
    let total: f32 = croma.iter().sum();
    if total <= 0.0 {
        return None;
    }
    for c in croma.iter_mut() {
        *c /= total;
    }
    Some(croma)
}

/// La rotación y el modo que mejor correlacionan, con su confianza.
fn mejor_perfil(croma: &[f32; 12]) -> Option<(usize, bool, f32)> {
    let mut resultados: Vec<(usize, bool, f32)> = Vec::with_capacity(24);
    for tonica in 0..12 {
        for (es_mayor, perfil) in [(true, &MAYOR), (false, &MENOR)] {
            let rotado: Vec<f32> = (0..12).map(|i| perfil[(i + 12 - tonica) % 12]).collect();
            resultados.push((tonica, es_mayor, correlacion(croma, &rotado)));
        }
    }
    resultados.sort_by(|a, b| b.2.total_cmp(&a.2));
    let mejor = resultados.first().copied()?;
    if !mejor.2.is_finite() || mejor.2 <= 0.0 {
        return None;
    }
    // La confianza es qué tanto le gana al segundo lugar: un cromagrama que
    // encaja igual de bien en dos tonos no dice nada.
    let segundo = resultados.get(1).map(|r| r.2).unwrap_or(0.0).max(0.0);
    let confianza = ((mejor.2 - segundo) / mejor.2).clamp(0.0, 1.0);
    Some((mejor.0, mejor.1, confianza))
}

/// Correlación de Pearson entre dos vectores del mismo largo.
fn correlacion(a: &[f32], b: &[f32]) -> f32 {
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

fn nombre(indice: usize, es_mayor: bool) -> String {
    format!("{}{}", NOTAS[indice % 12], if es_mayor { "" } else { "m" })
}

/// La rueda de Camelot: el número dice qué tan lejos está en el círculo de
/// quintas, y la letra si es menor (A) o mayor (B).
fn camelot(indice: usize, es_mayor: bool) -> String {
    // El círculo de quintas empezando en do. Do mayor es 8B y la menor es
    // 8A, y de ahí se cuenta.
    let quintas = [0usize, 7, 2, 9, 4, 11, 6, 1, 8, 3, 10, 5];
    let pos = quintas.iter().position(|q| *q == indice % 12).unwrap_or(0);
    let numero = if es_mayor {
        // Do está en la posición 0 y le toca el 8.
        (pos + 7) % 12 + 1
    } else {
        // La está en la posición 3 y le toca el 8: la rueda menor va tres
        // quintas atrás de la mayor.
        (pos + 4) % 12 + 1
    };
    format!("{numero}{}", if es_mayor { "B" } else { "A" })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// La frecuencia de una nota MIDI.
    fn hz(midi: f32) -> f32 {
        440.0 * 2f32.powf((midi - 69.0) / 12.0)
    }

    /// Un acorde sostenido con esas notas MIDI.
    fn acorde(notas: &[f32], rate: u32, segundos: u32) -> Vec<f32> {
        let total = (rate * segundos) as usize;
        (0..total)
            .map(|i| {
                notas
                    .iter()
                    .map(|n| (2.0 * PI * hz(*n) * i as f32 / rate as f32).sin())
                    .sum::<f32>()
                    / notas.len() as f32
            })
            .collect()
    }

    #[test]
    fn do_mayor_sale_como_do_mayor() {
        // Las siete notas de la escala de do, para que el perfil tenga con
        // qué trabajar: con una tríada sola, do mayor y la menor empatan.
        let escala: Vec<f32> = [60.0, 62.0, 64.0, 65.0, 67.0, 69.0, 71.0].to_vec();
        // Pesando la tónica y la dominante, como en la música de verdad.
        let mut notas = escala.clone();
        notas.extend_from_slice(&[60.0, 67.0, 60.0]);
        let k = detect(&acorde(&notas, 11_025, 6), 11_025).expect("hay tonalidad");
        assert_eq!(k.name, "C", "salió {} ({})", k.name, k.camelot);
        assert_eq!(k.camelot, "8B");
    }

    #[test]
    fn la_menor_sale_como_la_menor() {
        let escala: Vec<f32> = [57.0, 59.0, 60.0, 62.0, 64.0, 65.0, 67.0].to_vec();
        let mut notas = escala.clone();
        // En menor, lo que manda es la tónica y la tercera menor.
        notas.extend_from_slice(&[57.0, 60.0, 57.0, 64.0]);
        let k = detect(&acorde(&notas, 11_025, 6), 11_025).expect("hay tonalidad");
        assert_eq!(k.name, "Am", "salió {} ({})", k.name, k.camelot);
        assert_eq!(k.camelot, "8A");
    }

    #[test]
    fn el_silencio_no_tiene_tonalidad() {
        assert!(detect(&vec![0.0; 11_025 * 4], 11_025).is_none());
    }

    #[test]
    fn con_poco_material_no_se_inventa_una_tonalidad() {
        assert!(detect(&[0.1; 100], 11_025).is_none());
        assert!(detect(&[0.1; 20_000], 0).is_none());
    }

    #[test]
    fn la_rueda_de_camelot_da_la_vuelta_completa() {
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
            let relativo = camelot((i + 9) % 12, false);
            assert_eq!(
                mayor[..mayor.len() - 1],
                relativo[..relativo.len() - 1],
                "{} y su relativo menor comparten número",
                NOTAS[i]
            );
        }
    }

    #[test]
    fn un_cromagrama_plano_no_se_decide() {
        // Todas las notas igual de fuertes: no hay tónica que valga.
        let plano = [1.0f32 / 12.0; 12];
        let (_, _, confianza) = mejor_perfil(&plano).unwrap_or((0, true, 0.0));
        assert!(confianza < 0.2, "no debería estar seguro: {confianza}");
    }
}
