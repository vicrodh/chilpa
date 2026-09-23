//! Una salida que escribe a un archivo en vez de a un DAC.
//!
//! Sirve para dos cosas: probar la ruta completa sin sacar sonido, y
//! comprobar el bit-perfect contra lo que de verdad salió. No resamplea ni
//! convierte nada: acepta el formato que le den y guarda los bytes tal cual.

use crate::format::StreamFormat;
use crate::negotiate::DeviceCaps;
use crate::sink::{AudioSink, SinkError};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

/// El identificador con el que se pide esta salida.
pub const CAPTURE_ID: &str = "capture";

/// Capacidades de la captura: acepta cualquier formato entero, así que
/// nunca obliga a convertir.
pub fn capabilities() -> DeviceCaps {
    use crate::format::SampleFormat::*;
    DeviceCaps {
        sample_rates: vec![
            44_100, 48_000, 88_200, 96_000, 176_400, 192_000, 352_800, 384_000, 705_600, 768_000,
        ],
        formats: vec![S16, S24Packed, S24In32, S32],
        channels: vec![1, 2, 4, 6, 8],
        // Medidas de verdad: esta salida sabe exactamente lo que acepta.
        probed: true,
    }
}

/// Guarda lo entregado en un archivo crudo.
pub struct CaptureSink {
    file: BufWriter<File>,
    stream: StreamFormat,
    frames: u64,
}

impl CaptureSink {
    pub fn create(path: &Path, stream: StreamFormat) -> Result<Self, SinkError> {
        let file = File::create(path).map_err(|e| SinkError::Backend(e.to_string()))?;
        Ok(Self { file: BufWriter::new(file), stream, frames: 0 })
    }

    pub fn frames(&self) -> u64 {
        self.frames
    }
}

impl AudioSink for CaptureSink {
    fn format(&self) -> StreamFormat {
        self.stream
    }

    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
        self.file.write_all(frames).map_err(|e| SinkError::Backend(e.to_string()))?;
        let n = frames.len() / self.stream.frame_bytes();
        self.frames += n as u64;
        Ok(n)
    }

    fn close(&mut self) -> Result<(), SinkError> {
        self.file.flush().map_err(|e| SinkError::Backend(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::SampleFormat;

    #[test]
    fn lo_entregado_queda_en_el_archivo_tal_cual() {
        let mut path = std::env::temp_dir();
        path.push("chilpa-capture-test.raw");
        let stream = StreamFormat::new(SampleFormat::S16, 44_100, 2);
        let mut sink = CaptureSink::create(&path, stream).unwrap();
        let data: Vec<u8> = (0..40u8).collect();
        assert_eq!(sink.write(&data).unwrap(), 10);
        sink.close().unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), data);
        assert_eq!(sink.frames(), 10);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn la_captura_acepta_lo_que_le_den_y_se_declara_medida() {
        let caps = capabilities();
        assert!(caps.probed, "sin esto no se podría prometer bit-perfect");
        assert!(caps.accepts(&StreamFormat::new(SampleFormat::S24In32, 192_000, 2)));
    }
}
