//! DoP: DSD sobre PCM, según el estándar abierto DoP v1.1.
//!
//! Cada palabra de 24 bits lleva 16 bits de DSD bajo un marker que alterna
//! 0x05 y 0xFA entre frames. El DAC detecta esa alternancia y se pasa a modo
//! DSD; si se rompe, interpreta el stream como PCM y suelta ruido fuerte.
//!
//! De ahí sale la regla que la auditoría encontró rota en QBZ: una escritura
//! parcial dejaba fuera los frames restantes y solo escribía un warning, así
//! que la secuencia de markers se desfasaba. Aquí las escrituras parciales se
//! reanudan en el frame exacto donde se quedaron, y lo que se manda en pausas
//! o al final es silencio DSD con markers válidos, nunca ceros de PCM.

/// Frecuencia del portador PCM para un bit rate de DSD: 16 bits de DSD por
/// frame y por canal.
pub const fn carrier_rate(dsd_rate_hz: u32) -> u32 {
    dsd_rate_hz / 16
}

/// El payload de silencio en DSD. Ceros de PCM sacarían al DAC de modo DSD.
const SILENCE: i32 = 0x6969;

/// Empaqueta frames de DoP manteniendo la fase del marker entre llamadas.
#[derive(Debug, Default)]
pub struct DopPacker {
    marker_fa: bool,
}

impl DopPacker {
    pub fn new() -> Self {
        Self::default()
    }

    fn marker(&mut self) -> i32 {
        let m = if self.marker_fa { 0xFA } else { 0x05 };
        self.marker_fa = !self.marker_fa;
        m
    }

    /// Empaqueta DSD planar (2 bytes por canal y por frame) en muestras S32
    /// intercaladas, justificadas a la izquierda y listas para el cable.
    pub fn pack(&mut self, planar: &[&[u8]], out: &mut Vec<i32>) {
        let channels = planar.len();
        let frames = planar.iter().map(|c| c.len()).min().unwrap_or(0) / 2;
        out.reserve(frames * channels);
        for f in 0..frames {
            let marker = self.marker();
            for channel in planar {
                let hi = i32::from(channel[f * 2]);
                let lo = i32::from(channel[f * 2 + 1]);
                out.push(((marker << 16) | (hi << 8) | lo) << 8);
            }
        }
    }

    /// Silencio DSD con markers válidos, para pausas, seeks y relleno final.
    pub fn silence(&mut self, frames: usize, channels: u16, out: &mut Vec<i32>) {
        out.reserve(frames * channels as usize);
        for _ in 0..frames {
            let marker = self.marker();
            for _ in 0..channels {
                out.push(((marker << 16) | SILENCE) << 8);
            }
        }
    }

    /// Fase actual, para comprobar continuidad. `false` = sigue 0x05.
    pub fn next_is_fa(&self) -> bool {
        self.marker_fa
    }
}

/// Lo que un dispositivo hizo con una escritura.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WriteOutcome {
    /// Escribió esos frames; pueden ser menos de los que se le dieron.
    Wrote(usize),
    /// El dispositivo se quedó corto de datos y hubo que recuperarlo. Lo que
    /// no alcanzó a salir se perdió.
    Xrun,
}

/// Entrega palabras de DoP a un dispositivo sin romper la alternancia.
///
/// El escritor no asume que una escritura consuma todo: reintenta el resto
/// desde el frame exacto donde se quedó. Después de un xrun, el hueco se
/// rellena con silencio DSD para que el DAC nunca vea PCM.
pub struct DopWriter {
    channels: u16,
    /// Frames de silencio que se mandan tras recuperar un xrun.
    pub recovery_frames: usize,
}

impl DopWriter {
    pub fn new(channels: u16) -> Self {
        Self { channels, recovery_frames: 1 }
    }

    /// Escribe todos los frames de `words`, reintentando los parciales.
    ///
    /// `sink` recibe las muestras que faltan y regresa qué hizo con ellas.
    /// Regresa cuántos frames se recuperaron con silencio.
    pub fn write_all<S>(&self, packer: &mut DopPacker, words: &[i32], mut sink: S) -> usize
    where
        S: FnMut(&[i32]) -> WriteOutcome,
    {
        let channels = self.channels as usize;
        let mut done = 0;
        let mut recovered = 0;
        while done * channels < words.len() {
            match sink(&words[done * channels..]) {
                WriteOutcome::Wrote(0) => break,
                WriteOutcome::Wrote(frames) => done += frames,
                WriteOutcome::Xrun => {
                    // El silencio avanza la fase igual que los frames que se
                    // perdieron, así que el DAC sigue viendo la alternancia.
                    let mut relleno = Vec::new();
                    packer.silence(self.recovery_frames, self.channels, &mut relleno);
                    sink(&relleno);
                    recovered += self.recovery_frames;
                }
            }
        }
        recovered
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dsd(bytes: &[u8]) -> Vec<u8> {
        bytes.to_vec()
    }

    #[test]
    fn el_portador_de_dsd64_es_176_400() {
        assert_eq!(carrier_rate(2_822_400), 176_400);
        assert_eq!(carrier_rate(5_644_800), 352_800);
    }

    #[test]
    fn los_markers_alternan_y_los_dos_canales_llevan_el_mismo() {
        let left = dsd(&[0xAB, 0xCD, 0x12, 0x34]);
        let der = dsd(&[0x55, 0xAA, 0x9C, 0x63]);
        let mut out = Vec::new();
        DopPacker::new().pack(&[&left, &der], &mut out);
        assert_eq!(out[0], ((0x05 << 16) | 0xABCD) << 8);
        assert_eq!(out[1], ((0x05 << 16) | 0x55AA) << 8);
        assert_eq!(out[2], ((0xFA << 16) | 0x1234) << 8);
        assert_eq!(out[3], ((0xFA << 16) | 0x9C63) << 8);
    }

    #[test]
    fn la_fase_del_marker_sigue_entre_llamadas() {
        let mut packer = DopPacker::new();
        let mut out = Vec::new();
        packer.pack(&[&dsd(&[0, 0]), &dsd(&[0, 0])], &mut out);
        out.clear();
        packer.pack(&[&dsd(&[0, 0]), &dsd(&[0, 0])], &mut out);
        assert_eq!((out[0] >> 24) & 0xFF, 0xFA, "el segundo bloque sigue donde quedó el primero");
    }

    #[test]
    fn el_silencio_lleva_payload_de_dsd_y_no_ceros() {
        let mut out = Vec::new();
        DopPacker::new().silence(1, 2, &mut out);
        assert_eq!(out[0], ((0x05 << 16) | 0x6969) << 8);
        assert_ne!(out[0], 0, "ceros de PCM sacarían al DAC de modo DSD");
    }

    #[test]
    fn una_escritura_parcial_se_reanuda_y_no_pierde_frames() {
        // El hallazgo de la auditoría: QBZ solo avisaba del write parcial y
        // tiraba el resto, y ahí se rompía la alternancia.
        let mut packer = DopPacker::new();
        let mut words = Vec::new();
        packer.pack(&[&dsd(&[1, 2, 3, 4, 5, 6]), &dsd(&[7, 8, 9, 10, 11, 12])], &mut words);
        let written = std::cell::RefCell::new(Vec::new());
        let writer = DopWriter::new(2);
        let mut first = true;
        writer.write_all(&mut packer, &words, |chunk| {
            let frames = if first { 1 } else { chunk.len() / 2 };
            written.borrow_mut().extend_from_slice(&chunk[..frames * 2]);
            if first {
                first = false;
            }
            WriteOutcome::Wrote(frames)
        });
        assert_eq!(written.borrow().len(), 6, "los tres frames estéreo salieron en dos escrituras");
    }

    #[test]
    fn tras_un_xrun_se_rellena_con_silencio_y_la_fase_no_se_rompe() {
        let mut packer = DopPacker::new();
        let mut words = Vec::new();
        packer.pack(&[&dsd(&[1, 2]), &dsd(&[3, 4])], &mut words);
        let mut vistas: Vec<i32> = Vec::new();
        let mut fail = true;
        let recovered = DopWriter::new(2).write_all(&mut packer, &words, |chunk| {
            if fail {
                fail = false;
                return WriteOutcome::Xrun;
            }
            vistas.extend_from_slice(chunk);
            WriteOutcome::Wrote(chunk.len() / 2)
        });
        assert_eq!(recovered, 1);
        assert_eq!(vistas[0], ((0xFA << 16) | 0x6969) << 8, "el relleno continúa la alternancia");
    }
}
