//! Medir qué tan fuerte suena una pista, en LUFS.
//!
//! Es la UIT-R BS.1770: se filtra con la curva K —que es lo que se parece a
//! cómo oye una cabeza—, se mide la potencia en bloques de 400 ms y se
//! promedian solo los bloques que pasan dos compuertas, para que el silencio
//! entre canciones no baje la cuenta.
//!
//! **Esto mide, no corrige.** Aplicar una ganancia cambiaría las muestras y
//! ahí se acabó el bit-perfect. Lo que hace este módulo es decir un número;
//! qué se hace con él es otra decisión.
//!
//! Se mide al vuelo, bloque por bloque: una pista de una hora no tiene por
//! qué caber en memoria para saber qué tan fuerte suena.

/// El desplazamiento de la norma. Cancela la ganancia de la curva K en 1 kHz,
/// que es por lo que un seno de 0 dBFS en un canal mide -3.01 LKFS.
const OFFSET: f64 = -0.691;

/// Por debajo de esto un bloque es silencio y no cuenta.
const ABSOLUTE_GATE: f64 = -70.0;

/// La segunda compuerta va diez unidades abajo del promedio.
const RELATIVE_GATE: f64 = -10.0;

/// Cuánto dura un bloque, y cuánto se avanza entre uno y otro.
const BLOCK_MS: f64 = 400.0;
const PASO_MS: f64 = 100.0;

/// Un biquad en forma directa II transpuesta.
#[derive(Clone, Copy, Debug, Default)]
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: f64,
    z2: f64,
}

impl Biquad {
    fn paso(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }

    /// El primer tramo de la curva K: un realce de agudos, el que imita la
    /// cabeza.
    fn boost(rate: f64) -> Self {
        let (f0, gain_db, q) = (1681.974450955533, 3.999843853973347, 0.7071752369554196);
        let k = (std::f64::consts::PI * f0 / rate).tan();
        let vh = 10f64.powf(gain_db / 20.0);
        let vb = vh.powf(0.4996667741545416);
        let a0 = 1.0 + k / q + k * k;
        Self {
            b0: (vh + vb * k / q + k * k) / a0,
            b1: 2.0 * (k * k - vh) / a0,
            b2: (vh - vb * k / q + k * k) / a0,
            a1: 2.0 * (k * k - 1.0) / a0,
            a2: (1.0 - k / q + k * k) / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    /// El segundo tramo: quita lo que está tan abajo que no se oye como
    /// volumen.
    fn high_pass(rate: f64) -> Self {
        let (f0, q) = (38.13547087602444, 0.5003270373238773);
        let k = (std::f64::consts::PI * f0 / rate).tan();
        let a0 = 1.0 + k / q + k * k;
        Self {
            b0: 1.0,
            b1: -2.0,
            b2: 1.0,
            a1: 2.0 * (k * k - 1.0) / a0,
            a2: (1.0 - k / q + k * k) / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }
}

/// El peso de cada canal, según la norma.
///
/// Los frontales pesan uno; los de atrás, un poco más, porque el envolvente
/// se percibe más fuerte de lo que mide.
fn peso(canal: usize, channels: usize) -> f64 {
    // Con estéreo o mono no hay traseros que pesar.
    if channels <= 2 {
        return 1.0;
    }
    // En 5.1 el orden habitual es I, D, C, LFE, TI, TD.
    match canal {
        3 => 0.0,           // el LFE no entra en la cuenta
        4 | 5 => 1.41,      // los traseros
        _ => 1.0,
    }
}

/// Un medidor que se va comiendo la señal bloque por bloque.
pub struct Meter {
    channels: usize,
    filters: Vec<(Biquad, Biquad)>,
    /// Cuántas muestras por canal lleva el bloque en curso.
    in_block: usize,
    samples_per_block: usize,
    samples_per_step: usize,
    /// La suma de cuadrados de cada canal, por cada tramo de `PASO_MS`.
    segments: Vec<Vec<f64>>,
    current_segment: Vec<f64>,
    /// La potencia de cada bloque ya cerrado.
    blocks: Vec<f64>,
}

impl Meter {
    pub fn new(sample_rate_hz: u32, channels: u16) -> Option<Self> {
        let channels = channels.max(1) as usize;
        if sample_rate_hz < 8_000 {
            return None;
        }
        let rate = f64::from(sample_rate_hz);
        Some(Self {
            channels,
            filters: (0..channels)
                .map(|_| (Biquad::boost(rate), Biquad::high_pass(rate)))
                .collect(),
            in_block: 0,
            samples_per_block: (rate * BLOCK_MS / 1000.0).round() as usize,
            samples_per_step: (rate * PASO_MS / 1000.0).round() as usize,
            segments: Vec::new(),
            current_segment: vec![0.0; channels],
            blocks: Vec::new(),
        })
    }

    /// Come un pedazo de audio intercalado por canal.
    pub fn push(&mut self, interleaved: &[f32]) {
        for frame in interleaved.chunks_exact(self.channels) {
            for (c, m) in frame.iter().enumerate() {
                let (boost, altos) = &mut self.filters[c];
                let y = altos.paso(boost.paso(f64::from(*m)));
                self.current_segment[c] += y * y;
            }
            self.in_block += 1;
            if self.in_block == self.samples_per_step {
                self.close_segment();
            }
        }
    }

    fn close_segment(&mut self) {
        self.segments.push(std::mem::replace(&mut self.current_segment, vec![0.0; self.channels]));
        self.in_block = 0;
        // Un bloque son cuatro tramos; se cierra uno por cada tramo nuevo,
        // que es el traslape del 75 % que pide la norma.
        let per_block = self.samples_per_block / self.samples_per_step;
        if self.segments.len() >= per_block {
            let from = self.segments.len() - per_block;
            let mut z = 0.0;
            for c in 0..self.channels {
                let sum: f64 = self.segments[from..].iter().map(|t| t[c]).sum();
                z += peso(c, self.channels) * sum / self.samples_per_block as f64;
            }
            self.blocks.push(z);
        }
    }

    /// La sonoridad integrada, en LUFS.
    ///
    /// `None` cuando no hubo ni un bloque que pasara la compuerta: silencio,
    /// o una pista más corta que 400 ms.
    pub fn integrated(&self) -> Option<f64> {
        if self.blocks.is_empty() {
            return None;
        }
        let level = |z: f64| if z > 0.0 { OFFSET + 10.0 * z.log10() } else { f64::NEG_INFINITY };

        // Primera compuerta: lo que sea silencio no cuenta.
        let pass: Vec<f64> =
            self.blocks.iter().copied().filter(|z| level(*z) > ABSOLUTE_GATE).collect();
        if pass.is_empty() {
            return None;
        }
        // Segunda compuerta: diez unidades abajo del promedio de lo que
        // quedó. Es lo que evita que los silencios entre partes bajen la
        // cuenta de una pista dinámica.
        let media = pass.iter().sum::<f64>() / pass.len() as f64;
        let umbral = level(media) + RELATIVE_GATE;
        let finales: Vec<f64> =
            pass.into_iter().filter(|z| level(*z) > umbral).collect();
        if finales.is_empty() {
            return None;
        }
        let media = finales.iter().sum::<f64>() / finales.len() as f64;
        let l = level(media);
        l.is_finite().then_some(l)
    }
}

/// Mide una señal ya completa.
pub fn integrated(interleaved: &[f32], sample_rate_hz: u32, channels: u16) -> Option<f64> {
    let mut m = Meter::new(sample_rate_hz, channels)?;
    m.push(interleaved);
    m.integrated()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    /// Un seno de 997 Hz repartido en los canales que se pidan.
    fn sine(amplitude: f32, rate: u32, seconds: u32, channels: u16, only_one: bool) -> Vec<f32> {
        let frames = (rate * seconds) as usize;
        let mut v = Vec::with_capacity(frames * channels as usize);
        for i in 0..frames {
            let m = amplitude * (2.0 * PI * 997.0 * i as f64 / f64::from(rate)).sin() as f32;
            for c in 0..channels {
                v.push(if only_one && c > 0 { 0.0 } else { m });
            }
        }
        v
    }

    #[test]
    fn un_seno_de_cero_dbfs_en_un_canal_mide_menos_tres_lkfs() {
        // Es la calibración de la norma: 0 dBFS a 997 Hz en un solo canal
        // son -3.01 LKFS. Si esto se mueve, la curva K está mal.
        let data = sine(1.0, 48_000, 5, 2, true);
        let l = integrated(&data, 48_000, 2).expect("hay sonido");
        assert!((l - (-3.01)).abs() < 0.1, "midió {l:.3} LKFS, se esperaba -3.01");
    }

    #[test]
    fn el_mismo_seno_en_los_dos_canales_mide_tres_unidades_mas() {
        let one = integrated(&sine(1.0, 48_000, 5, 2, true), 48_000, 2).unwrap();
        let dos = integrated(&sine(1.0, 48_000, 5, 2, false), 48_000, 2).unwrap();
        assert!((dos - one - 3.01).abs() < 0.1, "de {one:.2} a {dos:.2}");
    }

    #[test]
    fn al_doble_de_amplitud_seis_unidades_mas() {
        let low = integrated(&sine(0.25, 48_000, 5, 2, false), 48_000, 2).unwrap();
        let alto = integrated(&sine(0.5, 48_000, 5, 2, false), 48_000, 2).unwrap();
        assert!((alto - low - 6.02).abs() < 0.1, "de {low:.2} a {alto:.2}");
    }

    #[test]
    fn la_medida_no_depende_de_la_frecuencia_de_muestreo() {
        // La curva K se recalcula para cada frecuencia; si estuviera fija a
        // 48 kHz, esto se separaría.
        let a = integrated(&sine(0.5, 44_100, 5, 2, false), 44_100, 2).unwrap();
        let b = integrated(&sine(0.5, 48_000, 5, 2, false), 48_000, 2).unwrap();
        let c = integrated(&sine(0.5, 96_000, 5, 2, false), 96_000, 2).unwrap();
        assert!((a - b).abs() < 0.1, "44.1 dio {a:.2} y 48 dio {b:.2}");
        assert!((c - b).abs() < 0.1, "96 dio {c:.2} y 48 dio {b:.2}");
    }

    #[test]
    fn el_silencio_no_tiene_sonoridad() {
        assert_eq!(integrated(&vec![0.0; 48_000 * 4], 48_000, 2), None);
    }

    #[test]
    fn una_pista_mas_corta_que_un_bloque_no_da_numero() {
        // 400 ms es el bloque; 200 no alcanzan.
        assert_eq!(integrated(&sine(0.5, 48_000, 0, 2, false), 48_000, 2), None);
        let short = sine(0.5, 48_000, 1, 2, false);
        assert!(integrated(&short[..48_000 * 2 / 5], 48_000, 2).is_none());
    }

    #[test]
    fn los_silencios_no_bajan_la_cuenta() {
        // Cinco segundos de tono y cinco de silencio: la compuerta relativa
        // tiene que dejar fuera el silencio.
        let mut data = sine(0.5, 48_000, 5, 2, false);
        let tone_only = integrated(&data, 48_000, 2).unwrap();
        data.extend(std::iter::repeat(0.0).take(48_000 * 5 * 2));
        let with_silence = integrated(&data, 48_000, 2).unwrap();
        assert!(
            (tone_only - with_silence).abs() < 0.2,
            "el silencio movió la cuenta de {tone_only:.2} a {with_silence:.2}"
        );
    }

    #[test]
    fn una_frecuencia_de_muestreo_imposible_no_arma_medidor() {
        assert!(Meter::new(0, 2).is_none());
        assert!(Meter::new(4_000, 2).is_none());
    }
}
