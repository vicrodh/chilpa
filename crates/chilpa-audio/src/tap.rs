//! Mirar lo que sale, sin tocarlo.
//!
//! Un visualizador necesita las muestras que de verdad se entregaron. La
//! única forma honesta de dárselas es copiarlas **después** de empaquetarlas
//! y **antes** de escribirlas, sin cambiar un solo byte de lo que va al
//! dispositivo.
//!
//! Tres reglas que hacen que esto no se coma el bit-perfect ni el CPU:
//!
//! 1. **Lo que se entrega no se modifica nunca.** [`TapSink`] copia y
//!    reenvía; lo que escribe es exactamente lo que le dieron, y solo lo que
//!    el dispositivo aceptó cuenta como entregado.
//! 2. **La copia es de tamaño fijo y se pisa.** Un búfer circular chico: si
//!    nadie mira, lo viejo se tira. Nunca crece, nunca bloquea al audio.
//! 3. **Si el candado está ocupado, se salta la copia.** El hilo que suena
//!    no espera al que dibuja. Perder un cuadro de un visualizador no le
//!    importa a nadie; un hueco en el audio sí.

use crate::format::{SampleFormat, StreamFormat};
use crate::sink::{AudioSink, SinkError};
use std::sync::{Arc, Mutex};

/// Cuántos frames se guardan para mirar. A 192 kHz son unos 43 ms, que es
/// más de lo que cabe en un cuadro de pantalla.
pub const FRAMES_GUARDADOS: usize = 8192;

/// Lo último que salió, para quien lo quiera dibujar.
#[derive(Clone, Default)]
pub struct Tap(Arc<Mutex<Estado>>);

#[derive(Default)]
struct Estado {
    /// Muestras normalizadas a -1.0..1.0, intercaladas por canal.
    muestras: Vec<f32>,
    /// Dónde va la escritura dentro del círculo.
    cursor: usize,
    channels: u16,
    sample_rate_hz: u32,
    /// Cuántos frames se han copiado desde que se abrió.
    frames: u64,
}

/// Una foto de lo que acaba de salir.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Snapshot {
    /// Las muestras en orden cronológico, intercaladas por canal.
    pub samples: Vec<f32>,
    pub channels: u16,
    pub sample_rate_hz: u32,
    /// El pico por canal en lo que se guardó, de 0.0 a 1.0.
    pub peaks: Vec<f32>,
    /// La raíz cuadrática media por canal, que es lo que se oye como
    /// volumen.
    pub rms: Vec<f32>,
}

impl Tap {
    pub fn new() -> Self {
        Self::default()
    }

    /// Prepara el círculo para un formato. Se llama al abrir el dispositivo.
    fn preparar(&self, formato: StreamFormat) {
        if let Ok(mut e) = self.0.lock() {
            let canales = formato.channels.max(1);
            e.muestras = vec![0.0; FRAMES_GUARDADOS * canales as usize];
            e.cursor = 0;
            e.channels = canales;
            e.sample_rate_hz = formato.sample_rate_hz;
            e.frames = 0;
        }
    }

    /// Copia lo que se entregó. Si el candado está ocupado, no hace nada.
    fn copiar(&self, bytes: &[u8], formato: StreamFormat) {
        let Ok(mut e) = self.0.try_lock() else { return };
        if e.muestras.is_empty() {
            return;
        }
        let canales = e.channels.max(1) as usize;
        let ancho = formato.sample_format.bytes();
        if ancho == 0 {
            return;
        }
        let total = e.muestras.len();
        let mut cursor = e.cursor;
        let mut escritas = 0usize;
        for muestra in bytes.chunks_exact(ancho) {
            e.muestras[cursor] = normalizar(muestra, formato.sample_format);
            cursor = (cursor + 1) % total;
            escritas += 1;
        }
        e.cursor = cursor;
        e.frames += (escritas / canales) as u64;
    }

    /// Lo último que salió, en orden cronológico.
    pub fn snapshot(&self) -> Snapshot {
        let Ok(e) = self.0.lock() else { return Snapshot::default() };
        if e.muestras.is_empty() {
            return Snapshot::default();
        }
        let canales = e.channels.max(1) as usize;
        // El círculo se endereza: de la posición del cursor en adelante es
        // lo más viejo.
        let mut muestras = Vec::with_capacity(e.muestras.len());
        muestras.extend_from_slice(&e.muestras[e.cursor..]);
        muestras.extend_from_slice(&e.muestras[..e.cursor]);

        let mut peaks = vec![0.0f32; canales];
        let mut sumas = vec![0.0f64; canales];
        for (i, m) in muestras.iter().enumerate() {
            let c = i % canales;
            peaks[c] = peaks[c].max(m.abs());
            sumas[c] += f64::from(*m) * f64::from(*m);
        }
        let por_canal = (muestras.len() / canales).max(1) as f64;
        let rms = sumas.iter().map(|s| (s / por_canal).sqrt() as f32).collect();

        Snapshot {
            samples: muestras,
            channels: e.channels,
            sample_rate_hz: e.sample_rate_hz,
            peaks,
            rms,
        }
    }

    /// Cuántos frames se han copiado.
    pub fn frames(&self) -> u64 {
        self.0.lock().map(|e| e.frames).unwrap_or(0)
    }

    /// Envuelve un dispositivo para mirar lo que se le entrega.
    pub fn wrap<S: AudioSink>(&self, sink: S) -> TapSink<S> {
        let formato = sink.format();
        self.preparar(formato);
        TapSink { inner: sink, tap: self.clone(), formato }
    }
}

/// Un dispositivo envuelto: copia lo entregado y lo pasa tal cual.
pub struct TapSink<S: AudioSink> {
    inner: S,
    tap: Tap,
    formato: StreamFormat,
}

impl<S: AudioSink> AudioSink for TapSink<S> {
    fn format(&self) -> StreamFormat {
        self.inner.format()
    }

    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
        // Primero se escribe, luego se copia lo que el dispositivo aceptó:
        // copiar de más mostraría muestras que nunca sonaron.
        let aceptados = self.inner.write(frames)?;
        let bytes = aceptados * self.formato.frame_bytes();
        if let Some(entregado) = frames.get(..bytes) {
            self.tap.copiar(entregado, self.formato);
        }
        Ok(aceptados)
    }

    fn close(&mut self) -> Result<(), SinkError> {
        self.inner.close()
    }

    fn set_paused(&mut self, paused: bool) -> Result<(), SinkError> {
        self.inner.set_paused(paused)
    }
}

/// Una muestra empaquetada, llevada a -1.0..1.0.
fn normalizar(bytes: &[u8], formato: SampleFormat) -> f32 {
    match formato {
        SampleFormat::S16 => {
            let v = i16::from_le_bytes([bytes[0], bytes[1]]);
            f32::from(v) / 32_768.0
        }
        // S24In32 va justificado a la izquierda, así que se lee igual que
        // un entero de 32 bits.
        SampleFormat::S24In32 | SampleFormat::S32 => {
            let v = i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            v as f32 / 2_147_483_648.0
        }
        SampleFormat::S24Packed => {
            // Tres bytes con signo: se estiran a 32 para leerlos.
            let v = i32::from_le_bytes([0, bytes[0], bytes[1], bytes[2]]);
            v as f32 / 2_147_483_648.0
        }
        SampleFormat::F32 => f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Clone)]
    struct Espejo {
        formato: StreamFormat,
        escrito: Rc<RefCell<Vec<u8>>>,
        /// Cuántos frames acepta por escritura, o todos si es cero.
        tope: usize,
    }

    impl AudioSink for Espejo {
        fn format(&self) -> StreamFormat {
            self.formato
        }
        fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
            let n = frames.len() / self.formato.frame_bytes();
            let n = if self.tope > 0 { n.min(self.tope) } else { n };
            let bytes = n * self.formato.frame_bytes();
            self.escrito.borrow_mut().extend_from_slice(&frames[..bytes]);
            Ok(n)
        }
        fn close(&mut self) -> Result<(), SinkError> {
            Ok(())
        }
    }

    fn espejo(formato: StreamFormat, tope: usize) -> Espejo {
        Espejo { formato, escrito: Rc::new(RefCell::new(Vec::new())), tope }
    }

    /// Una rampa de 16 bits en estéreo.
    fn rampa(frames: usize) -> Vec<u8> {
        let mut v = Vec::with_capacity(frames * 4);
        for i in 0..frames {
            let m = (i as i16).wrapping_mul(300);
            v.extend_from_slice(&m.to_le_bytes());
            v.extend_from_slice(&(-m).to_le_bytes());
        }
        v
    }

    #[test]
    fn mirar_no_cambia_un_solo_byte() {
        let formato = StreamFormat::new(SampleFormat::S16, 44_100, 2);
        let datos = rampa(1000);

        let directo = espejo(formato, 0);
        let mut d = directo.clone();
        d.write(&datos).unwrap();

        let mirado = espejo(formato, 0);
        let tap = Tap::new();
        let mut m = tap.wrap(mirado.clone());
        m.write(&datos).unwrap();

        assert_eq!(
            *directo.escrito.borrow(),
            *mirado.escrito.borrow(),
            "lo entregado con tap y sin tap es lo mismo"
        );
        assert_eq!(*mirado.escrito.borrow(), datos, "y es lo que se le dio");
    }

    #[test]
    fn solo_se_copia_lo_que_el_dispositivo_acepto() {
        let formato = StreamFormat::new(SampleFormat::S16, 44_100, 2);
        let tap = Tap::new();
        // Acepta 10 frames por escritura; se le dan 100.
        let mut s = tap.wrap(espejo(formato, 10));
        let aceptados = s.write(&rampa(100)).unwrap();
        assert_eq!(aceptados, 10);
        assert_eq!(tap.frames(), 10, "no se copiaron los que no sonaron");
    }

    #[test]
    fn el_pico_y_el_rms_salen_por_canal() {
        let formato = StreamFormat::new(SampleFormat::S16, 44_100, 2);
        let tap = Tap::new();
        let mut s = tap.wrap(espejo(formato, 0));

        // Izquierdo a tope, derecho en silencio, llenando el círculo.
        let mut datos = Vec::new();
        for _ in 0..FRAMES_GUARDADOS {
            datos.extend_from_slice(&i16::MAX.to_le_bytes());
            datos.extend_from_slice(&0i16.to_le_bytes());
        }
        s.write(&datos).unwrap();

        let foto = tap.snapshot();
        assert_eq!(foto.channels, 2);
        assert_eq!(foto.sample_rate_hz, 44_100);
        assert!(foto.peaks[0] > 0.99, "el izquierdo va a tope: {}", foto.peaks[0]);
        assert_eq!(foto.peaks[1], 0.0, "el derecho está callado");
        assert!(foto.rms[0] > 0.99);
        assert_eq!(foto.rms[1], 0.0);
    }

    #[test]
    fn el_circulo_se_endereza_y_queda_lo_ultimo() {
        let formato = StreamFormat::new(SampleFormat::S16, 44_100, 1);
        let tap = Tap::new();
        let mut s = tap.wrap(espejo(formato, 0));

        // Se escribe más de lo que cabe: lo viejo se pisa.
        let mut datos = Vec::new();
        for i in 0..(FRAMES_GUARDADOS + 100) {
            datos.extend_from_slice(&((i % 1000) as i16).to_le_bytes());
        }
        s.write(&datos).unwrap();

        let foto = tap.snapshot();
        assert_eq!(foto.samples.len(), FRAMES_GUARDADOS);
        // La última muestra es la última escrita, no una de en medio.
        let ultima = (((FRAMES_GUARDADOS + 99) % 1000) as i16) as f32 / 32_768.0;
        let vista = *foto.samples.last().unwrap();
        assert!((vista - ultima).abs() < 1e-6, "quedó {vista}, se esperaba {ultima}");
    }

    #[test]
    fn sin_nada_escrito_la_foto_esta_vacia_y_no_truena() {
        let tap = Tap::new();
        assert_eq!(tap.snapshot(), Snapshot::default());
        assert_eq!(tap.frames(), 0);
    }

    #[test]
    fn las_muestras_se_normalizan_segun_su_formato() {
        assert!((normalizar(&i16::MAX.to_le_bytes(), SampleFormat::S16) - 1.0).abs() < 1e-4);
        assert_eq!(normalizar(&i16::MIN.to_le_bytes(), SampleFormat::S16), -1.0);
        assert!((normalizar(&i32::MAX.to_le_bytes(), SampleFormat::S32) - 1.0).abs() < 1e-6);
        assert_eq!(normalizar(&1.0f32.to_le_bytes(), SampleFormat::F32), 1.0);
        // Los 24 bits empaquetados van en tres bytes: el tope es 0x7FFFFF.
        assert!((normalizar(&[0xFF, 0xFF, 0x7F], SampleFormat::S24Packed) - 1.0).abs() < 1e-4);
    }
}
