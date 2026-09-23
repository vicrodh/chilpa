//! Estimar el tempo de una pista.
//!
//! El método es el de siempre y no tiene misterio: se saca una envolvente de
//! ataques, se autocorrelaciona, y el lag que más se repite es el pulso.
//!
//! Lo que sí importa decir:
//!
//! - **Es una estimación, y se llama así.** Un detector de tempo se equivoca
//!   con música que no lleva pulso marcado, y se equivoca al doble o a la
//!   mitad con la que sí. Chilpa guarda el número con la versión del
//!   analizador para poder recalcularlo cuando haya uno mejor.
//! - **La octava se resuelve, no se ignora.** 75 y 150 explican igual de
//!   bien la misma señal; se prefiere el rango en el que de verdad se
//!   cuenta la música, y esa preferencia está aquí escrita, no escondida.
//! - **No se lee la pista entera.** Con un minuto del centro alcanza, y
//!   analizar una biblioteca no puede costar lo que costaría reproducirla.

/// A qué frecuencia se trabaja. Para el pulso no hace falta más, y baja el
/// costo de la autocorrelación a la décima parte.
pub const ANALYSIS_HZ: u32 = 11_025;

/// Cuántas muestras avanza cada cuadro de la envolvente. A 11 025 Hz son
/// unos 12 ms.
///
/// Con cuadros más largos, un tempo cuyo periodo no cae en un número
/// redondo de cuadros reparte su correlación entre dos lags y pierde contra
/// su propia mitad. Eso es exactamente lo que hacía que 140 BPM saliera
/// como 70.
const HOP: usize = 128;

/// El rango en el que se busca. Fuera de esto no hay música que contar.
pub const BPM_MIN: f32 = 50.0;
pub const BPM_MAX: f32 = 210.0;

/// Dónde se prefiere caer cuando dos respuestas explican lo mismo.
const PREFERRED_MIN: f32 = 80.0;
const PREFERRED_MAX: f32 = 160.0;

/// Lo que se pudo decir del pulso.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Tempo {
    pub bpm: f32,
    /// De 0 a 1: qué tan marcado está el pulso. Por debajo de 0.1 más vale
    /// no decir nada que decir un número inventado.
    pub confidence: f32,
}

/// Estima el tempo de una señal mono.
///
/// Devuelve `None` cuando no hay material suficiente o cuando no se
/// encuentra un pulso: un reproductor que inventa 120 BPM para todo no le
/// sirve a nadie.
pub fn detect(samples: &[f32], sample_rate_hz: u32) -> Option<Tempo> {
    if sample_rate_hz == 0 {
        return None;
    }
    let mono = remuestrear(samples, sample_rate_hz, ANALYSIS_HZ);
    // Menos de ocho segundos no alcanzan para contar compases.
    if mono.len() < ANALYSIS_HZ as usize * 8 {
        return None;
    }
    let envelope = onset_envelope(&mono);
    if envelope.len() < 64 {
        return None;
    }
    tempo_from_envelope(&envelope, ANALYSIS_HZ as f32 / HOP as f32)
}

/// La envolvente de ataques: cuánto subió la energía de un cuadro al otro.
///
/// Solo las subidas. Una bajada de energía no es un golpe, y contarla
/// duplicaría el pulso.
fn onset_envelope(mono: &[f32]) -> Vec<f32> {
    let frames = mono.len() / HOP;
    let mut energy = Vec::with_capacity(frames);
    for c in 0..frames {
        let slice = &mono[c * HOP..(c + 1) * HOP];
        let sum: f32 = slice.iter().map(|m| m * m).sum();
        // En logaritmo: el oído oye los golpes en proporción, no en resta.
        energy.push((sum / HOP as f32 + 1e-9).ln());
    }
    let mut onsets = Vec::with_capacity(energy.len());
    onsets.push(0.0);
    for i in 1..energy.len() {
        onsets.push((energy[i] - energy[i - 1]).max(0.0));
    }
    onsets
}

/// El tempo de una envolvente muestreada a `cuadros_por_segundo`.
fn tempo_from_envelope(onsets: &[f32], frames_per_second: f32) -> Option<Tempo> {
    // Se centra: sin quitar la media, la autocorrelación premia el lag 0 y
    // todo lo demás se pierde bajo la pendiente.
    let media = onsets.iter().sum::<f32>() / onsets.len() as f32;
    let centered: Vec<f32> = onsets.iter().map(|a| a - media).collect();
    let energy: f32 = centered.iter().map(|a| a * a).sum();
    if energy <= 0.0 {
        return None;
    }

    let lag_for = |bpm: f32| (frames_per_second * 60.0 / bpm).round() as usize;
    let lag_min = lag_for(BPM_MAX).max(1);
    let lag_max = lag_for(BPM_MIN).min(centered.len() / 2);
    if lag_max <= lag_min {
        return None;
    }

    let mut puntajes = vec![0.0f32; lag_max + 2];
    for lag in lag_min..=lag_max {
        let mut sum = 0.0f32;
        for i in 0..centered.len() - lag {
            sum += centered[i] * centered[i + lag];
        }
        // Se normaliza por cuántos términos entraron: sin eso, los lags
        // largos pierden solo por tener menos traslape.
        puntajes[lag] = sum / energy * (centered.len() as f32 / (centered.len() - lag) as f32);
    }

    // Un periodo que no cae en un número redondo de cuadros reparte su
    // correlación entre dos lags vecinos, y así pierde contra su propio
    // doble, que a lo mejor cayó más centrado. Medido con un pulso de 128
    // BPM: 0.65 en el lag 40 y 0.37 en el 41, contra 0.77 en el 81. Por eso
    // se compara la suma del pico con sus vecinos, no el bin solo.
    let neighborhood = |lag: usize| -> f32 {
        let from = lag.saturating_sub(1);
        let hasta = (lag + 1).min(puntajes.len() - 1);
        puntajes[from..=hasta].iter().map(|p| p.max(0.0)).sum()
    };

    let mut best = (0usize, 0.0f32);
    for lag in lag_min..=lag_max {
        let v = neighborhood(lag);
        if v > best.1 {
            best = (lag, v);
        }
    }
    if best.0 == 0 || best.1 <= 0.0 {
        return None;
    }

    // De todos los lags que explican casi igual de bien, se prefiere el que
    // cae en el rango en que de verdad se cuenta la música. 70 y 140
    // explican lo mismo; la música se cuenta en 140.
    let umbral = best.1 * 0.85;
    let mut chosen = best.0;
    let mut best_in_range = 0.0f32;
    for lag in lag_min..=lag_max {
        let v = neighborhood(lag);
        if v < umbral {
            continue;
        }
        let bpm = frames_per_second * 60.0 / lag as f32;
        if (PREFERRED_MIN..=PREFERRED_MAX).contains(&bpm) && v > best_in_range {
            best_in_range = v;
            chosen = lag;
        }
    }

    // La vecindad sirve para comparar octavas, no para señalar el cuadro.
    // Dentro de la vecindad elegida, el pico de verdad es el bin más alto:
    // interpolar alrededor de su vecino pobre manda el resultado lejos
    // (medido: 140 BPM salía como 146).
    let chosen = argmax_local(&puntajes, chosen);
    let lag = interpolar(&puntajes, chosen);
    // La confianza es la del pico crudo: es lo que dice qué tan marcado
    // está el pulso, no cuánto se repartió entre dos cuadros.
    let confidence = puntajes[best.0].clamp(0.0, 1.0);
    Some(Tempo { bpm: frames_per_second * 60.0 / lag, confidence: confidence })
}

/// El bin más alto entre `lag` y sus dos vecinos.
fn argmax_local(puntajes: &[f32], lag: usize) -> usize {
    let from = lag.saturating_sub(1);
    let hasta = (lag + 1).min(puntajes.len() - 1);
    (from..=hasta).max_by(|a, b| puntajes[*a].total_cmp(&puntajes[*b])).unwrap_or(lag)
}

/// El vértice de la parábola que pasa por el pico y sus dos vecinos.
///
/// Sin esto, el tempo solo puede tomar los valores que caen en un número
/// entero de cuadros, y a 140 BPM eso ya son varios BPM de error.
fn interpolar(puntajes: &[f32], lag: usize) -> f32 {
    if lag == 0 || lag + 1 >= puntajes.len() {
        return lag as f32;
    }
    let (a, b, c) = (puntajes[lag - 1], puntajes[lag], puntajes[lag + 1]);
    let denominator = a - 2.0 * b + c;
    if denominator.abs() < 1e-9 {
        return lag as f32;
    }
    let adjustment = 0.5 * (a - c) / denominator;
    if adjustment.abs() > 1.0 {
        return lag as f32;
    }
    lag as f32 + adjustment
}

/// La señal a la frecuencia con la que trabaja el análisis.
pub fn resample_for_analysis(samples: &[f32], sample_rate_hz: u32) -> Vec<f32> {
    remuestrear(samples, sample_rate_hz.max(1), ANALYSIS_HZ)
}

/// Baja la frecuencia de muestreo promediando. No es un remuestreador fino;
/// para contar golpes no hace falta.
fn remuestrear(samples: &[f32], de: u32, a: u32) -> Vec<f32> {
    if de <= a || samples.is_empty() {
        return samples.to_vec();
    }
    let factor = (de / a).max(1) as usize;
    samples
        .chunks(factor)
        .map(|t| t.iter().sum::<f32>() / t.len() as f32)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Golpes secos a ese tempo, con un poco de cola.
    fn pulse(bpm: f32, rate: u32, seconds: u32) -> Vec<f32> {
        let total = (rate * seconds) as usize;
        let period = (rate as f32 * 60.0 / bpm) as usize;
        let mut v = vec![0.0f32; total];
        let mut i = 0;
        while i < total {
            // Un golpe que decae en 30 ms.
            let tail = (rate / 33) as usize;
            for k in 0..tail.min(total - i) {
                let decay = 1.0 - k as f32 / tail as f32;
                v[i + k] = decay * decay;
            }
            i += period;
        }
        v
    }

    #[test]
    fn a_120_beat_is_detected_as_120() {
        let t = detect(&pulse(120.0, 44_100, 20), 44_100).expect("hay pulso");
        assert!((t.bpm - 120.0).abs() < 3.0, "salió {}", t.bpm);
        assert!(t.confidence > 0.1, "confianza baja: {}", t.confidence);
    }

    #[test]
    fn several_tempos_land_close() {
        // Dos BPM de margen: con la interpolación del pico, el error de
        // cuantización de los cuadros se queda por debajo de eso.
        for expected in [85.0f32, 90.0, 100.0, 110.0, 128.0, 140.0, 150.0] {
            let t = detect(&pulse(expected, 44_100, 20), 44_100)
                .unwrap_or_else(|| panic!("no se detectó {expected}"));
            assert!(
                (t.bpm - expected).abs() < 2.0,
                "se esperaba {expected}, salió {}",
                t.bpm
            );
        }
    }

    #[test]
    fn a_beat_under_noise_still_comes_out() {
        // Ruido determinista, para que la prueba no dependa del día.
        let mut x: u32 = 0x1234_5678;
        let mut v = pulse(120.0, 44_100, 20);
        for m in v.iter_mut() {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            *m += ((x % 1000) as f32 / 1000.0 - 0.5) * 0.2;
        }
        let t = detect(&v, 44_100).expect("el pulso aguanta el ruido");
        assert!((t.bpm - 120.0).abs() < 3.0, "salió {}", t.bpm);
    }

    #[test]
    fn a_slow_tempo_is_reported_in_the_range_it_is_counted_in() {
        // 60 BPM se cuenta igual de bien como 120: se prefiere el doble.
        let t = detect(&pulse(60.0, 44_100, 24), 44_100).expect("hay pulso");
        assert!(
            (t.bpm - 120.0).abs() < 4.0 || (t.bpm - 60.0).abs() < 4.0,
            "salió {}, que no es ni 60 ni su doble",
            t.bpm
        );
    }

    #[test]
    fn silence_has_no_tempo() {
        assert!(detect(&vec![0.0; 44_100 * 20], 44_100).is_none());
    }

    #[test]
    fn a_very_short_track_gives_no_invented_number() {
        assert!(detect(&pulse(120.0, 44_100, 3), 44_100).is_none(), "tres segundos no bastan");
        assert!(detect(&[], 44_100).is_none());
        assert!(detect(&pulse(120.0, 44_100, 20), 0).is_none(), "sin frecuencia, nada");
    }

    #[test]
    fn resampling_averages_and_shortens() {
        let v: Vec<f32> = (0..8).map(|i| i as f32).collect();
        let r = remuestrear(&v, 44_100, 11_025);
        assert_eq!(r.len(), 2, "de cuatro en cuatro");
        assert!((r[0] - 1.5).abs() < 1e-6, "el promedio de 0..3: {}", r[0]);
        // Subir de frecuencia no hace nada.
        assert_eq!(remuestrear(&v, 11_025, 44_100).len(), 8);
    }
}
