//! El espectro de lo que sale.
//!
//! Una FFT de radix-2 escrita aquí, de unas cuarenta líneas. Para 1024
//! puntos diez veces por segundo no hace falta una biblioteca, y una
//! dependencia menos es una dependencia menos que auditar.
//!
//! Tres cosas que hacen que el dibujo se parezca a lo que se oye:
//!
//! 1. **Ventana de Hann antes de transformar.** Sin ventana, el corte del
//!    bloque mete energía en todas las frecuencias y el espectro se ve como
//!    una alfombra.
//! 2. **Bandas repartidas en logaritmo.** El oído oye en octavas: repartir
//!    las bandas lineales deja media pantalla para los agudos que nadie
//!    distingue y aplasta los graves.
//! 3. **Decibeles, no amplitud.** Una escala lineal deja todo pegado al
//!    piso salvo el bombo.

use std::f32::consts::PI;

/// El piso del dibujo. Por debajo de esto es silencio para lo que importa.
pub const PISO_DB: f32 = -90.0;

/// Reparte el espectro de `samples` en `bandas`, de 0.0 a 1.0.
///
/// `sample_rate_hz` decide hasta dónde llegan las bandas: la mitad, que es
/// lo único que una FFT puede decir de una señal muestreada.
pub fn bands(samples: &[f32], sample_rate_hz: u32, bandas: usize) -> Vec<f32> {
    if samples.is_empty() || bandas == 0 || sample_rate_hz == 0 {
        return Vec::new();
    }
    // Se usa la última potencia de dos que quepa: la FFT de radix-2 no sabe
    // de otros tamaños, y lo último es lo que se está oyendo.
    let n = potencia_de_dos(samples.len());
    if n < 32 {
        return Vec::new();
    }
    let desde = samples.len() - n;

    let mut re: Vec<f32> = Vec::with_capacity(n);
    for (i, m) in samples[desde..].iter().enumerate() {
        // Hann: 0.5 - 0.5 cos(2 pi i / (n-1)).
        let ventana = 0.5 - 0.5 * (2.0 * PI * i as f32 / (n as f32 - 1.0)).cos();
        re.push(m * ventana);
    }
    let mut im = vec![0.0f32; n];
    fft(&mut re, &mut im);

    // La mitad útil, en magnitud normalizada por el tamaño.
    let mitad = n / 2;
    let magnitudes: Vec<f32> = (0..mitad)
        .map(|k| (re[k] * re[k] + im[k] * im[k]).sqrt() * 2.0 / n as f32)
        .collect();

    // Las bandas se reparten en logaritmo entre 20 Hz y Nyquist.
    let nyquist = sample_rate_hz as f32 / 2.0;
    let por_bin = nyquist / mitad as f32;
    let (bajo, alto) = (20.0f32, nyquist.max(40.0));
    let paso = (alto / bajo).ln() / bandas as f32;

    let mut salida = Vec::with_capacity(bandas);
    for b in 0..bandas {
        let desde_hz = bajo * (paso * b as f32).exp();
        let hasta_hz = bajo * (paso * (b + 1) as f32).exp();
        let k0 = ((desde_hz / por_bin) as usize).min(mitad.saturating_sub(1));
        let k1 = ((hasta_hz / por_bin).ceil() as usize).clamp(k0 + 1, mitad);
        // El pico de la banda, no el promedio: un tono fino no debe
        // desaparecer por caer en una banda ancha.
        let pico = magnitudes[k0..k1].iter().copied().fold(0.0f32, f32::max);
        salida.push(a_db_normalizado(pico));
    }
    salida
}

/// Una magnitud de 0..1 llevada a 0..1 en decibeles sobre el piso.
fn a_db_normalizado(magnitud: f32) -> f32 {
    if magnitud <= 0.0 {
        return 0.0;
    }
    let db = 20.0 * magnitud.log10();
    ((db - PISO_DB) / -PISO_DB).clamp(0.0, 1.0)
}

/// La mayor potencia de dos que cabe en `n`.
fn potencia_de_dos(n: usize) -> usize {
    if n == 0 {
        return 0;
    }
    1usize << (usize::BITS - 1 - n.leading_zeros()) as usize
}

/// FFT de radix-2, en el lugar. `re` e `im` tienen que medir lo mismo y ser
/// una potencia de dos.
///
/// Es pública porque el análisis de tonalidad la necesita y escribirla dos
/// veces sería tener dos FFT que se pueden desincronizar.
pub fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    if n <= 1 {
        return;
    }
    // Reordenar por inversión de bits.
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    // Mariposas, de a pares, de a cuartetos, y así.
    let mut largo = 2usize;
    while largo <= n {
        let angulo = -2.0 * PI / largo as f32;
        let (wr, wi) = (angulo.cos(), angulo.sin());
        for inicio in (0..n).step_by(largo) {
            let (mut cr, mut ci) = (1.0f32, 0.0f32);
            for k in 0..largo / 2 {
                let (a, b) = (inicio + k, inicio + k + largo / 2);
                let tr = cr * re[b] - ci * im[b];
                let ti = cr * im[b] + ci * re[b];
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let nuevo = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = nuevo;
            }
        }
        largo <<= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un seno de esa frecuencia, en mono.
    fn seno(hz: f32, rate: u32, frames: usize, amplitud: f32) -> Vec<f32> {
        (0..frames)
            .map(|i| amplitud * (2.0 * PI * hz * i as f32 / rate as f32).sin())
            .collect()
    }

    #[test]
    fn la_potencia_de_dos_es_la_que_cabe() {
        assert_eq!(potencia_de_dos(1024), 1024);
        assert_eq!(potencia_de_dos(1025), 1024);
        assert_eq!(potencia_de_dos(1023), 512);
        assert_eq!(potencia_de_dos(0), 0);
    }

    #[test]
    fn un_tono_aparece_en_su_banda_y_no_en_las_demas() {
        let rate = 44_100;
        let datos = seno(1_000.0, rate, 4096, 0.9);
        let bandas = bands(&datos, rate, 24);
        assert_eq!(bandas.len(), 24);

        let mayor = bandas
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(i, _)| i)
            .unwrap();

        // La banda del pico tiene que cubrir los 1000 Hz.
        let paso = ((rate as f32 / 2.0) / 20.0).ln() / 24.0;
        let desde = 20.0 * (paso * mayor as f32).exp();
        let hasta = 20.0 * (paso * (mayor + 1) as f32).exp();
        assert!(
            (desde..=hasta).contains(&1_000.0),
            "el pico cayó en {desde:.0}..{hasta:.0} Hz, no en 1000"
        );
        // Y las bandas lejanas están muy por debajo.
        let lejos = bandas[(mayor + 8).min(23)];
        assert!(lejos < bandas[mayor] / 2.0, "se derramó: {lejos} contra {}", bandas[mayor]);
    }

    #[test]
    fn el_silencio_no_dibuja_nada() {
        let bandas = bands(&vec![0.0; 2048], 44_100, 16);
        assert!(bandas.iter().all(|b| *b == 0.0), "{bandas:?}");
    }

    #[test]
    fn un_tono_mas_fuerte_da_una_banda_mas_alta() {
        let rate = 44_100;
        let bajo = bands(&seno(1_000.0, rate, 4096, 0.05), rate, 24);
        let alto = bands(&seno(1_000.0, rate, 4096, 0.9), rate, 24);
        let pico = |v: &[f32]| v.iter().copied().fold(0.0f32, f32::max);
        assert!(pico(&alto) > pico(&bajo), "{} contra {}", pico(&alto), pico(&bajo));
        assert!(pico(&alto) <= 1.0, "nunca se pasa de uno");
    }

    #[test]
    fn con_poco_material_no_se_inventa_un_espectro() {
        assert!(bands(&[], 44_100, 16).is_empty());
        assert!(bands(&[0.1; 8], 44_100, 16).is_empty(), "ocho muestras no son un espectro");
        assert!(bands(&[0.1; 2048], 0, 16).is_empty(), "sin frecuencia de muestreo, nada");
        assert!(bands(&[0.1; 2048], 44_100, 0).is_empty());
    }

    #[test]
    fn la_fft_de_una_constante_deja_todo_en_el_bin_cero() {
        let mut re = vec![1.0f32; 8];
        let mut im = vec![0.0f32; 8];
        fft(&mut re, &mut im);
        assert!((re[0] - 8.0).abs() < 1e-4, "la suma va en el bin 0: {}", re[0]);
        for k in 1..8 {
            assert!(re[k].abs() < 1e-4 && im[k].abs() < 1e-4, "el bin {k} debía ser cero");
        }
    }
}
