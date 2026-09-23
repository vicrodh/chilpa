//! Decodificación de archivos locales a muestras nativas.
//!
//! La regla del ADR sobre bit-perfect empieza aquí: si el archivo trae
//! enteros, salen enteros. Convertir a `f32` para volver a escalar es lo que
//! movía un LSB en QBZ, así que este crate entrega
//! [`chilpa_audio::Samples::Int`] siempre que el códec lo permita y solo usa
//! flotantes cuando la fuente de verdad es flotante.

use chilpa_audio::format::{SampleFormat, StreamFormat};
use std::fs::File;
use std::path::Path;
use symphonia::core::audio::{AudioBufferRef, Signal};
use symphonia::core::codecs::{Decoder, DecoderOptions};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

#[derive(Debug)]
pub enum DecodeError {
    Open(std::io::Error),
    Unsupported(String),
    Corrupt(String),
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::Open(e) => write!(f, "no se pudo abrir: {e}"),
            DecodeError::Unsupported(s) => write!(f, "formato no soportado: {s}"),
            DecodeError::Corrupt(s) => write!(f, "archivo dañado: {s}"),
        }
    }
}

/// Un bloque decodificado, en el tipo en el que venía.
pub enum Block {
    /// Enteros intercalados, justificados a la derecha, con `bits` útiles.
    Int { data: Vec<i32>, bits: u8 },
    Float(Vec<f32>),
}

impl Block {
    pub fn frames(&self, channels: u16) -> usize {
        let len = match self {
            Block::Int { data, .. } => data.len(),
            Block::Float(data) => data.len(),
        };
        len / channels.max(1) as usize
    }
}

/// Un archivo abierto, listo para entregar bloques.
pub struct Decoded {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    track_id: u32,
    stream: StreamFormat,
}

impl Decoded {
    /// Abre un archivo y prepara su decodificador.
    pub fn open(path: &Path) -> Result<Self, DecodeError> {
        let file = File::open(path).map_err(DecodeError::Open)?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());
        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }
        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
            .map_err(|e| DecodeError::Unsupported(e.to_string()))?;
        let format = probed.format;
        let track = format
            .default_track()
            .ok_or_else(|| DecodeError::Unsupported("el archivo no trae pistas".into()))?;
        let track_id = track.id;
        let params = &track.codec_params;
        let sample_rate_hz = params
            .sample_rate
            .ok_or_else(|| DecodeError::Unsupported("sin frecuencia de muestreo".into()))?;
        let channels = params
            .channels
            .ok_or_else(|| DecodeError::Unsupported("sin canales".into()))?
            .count() as u16;
        // Los bits del archivo mandan: 16 bits salen en S16, 24 en S24In32.
        let sample_format = match params.bits_per_sample.unwrap_or(16) {
            0..=16 => SampleFormat::S16,
            17..=24 => SampleFormat::S24In32,
            _ => SampleFormat::S32,
        };
        let decoder = symphonia::default::get_codecs()
            .make(params, &DecoderOptions::default())
            .map_err(|e| DecodeError::Unsupported(e.to_string()))?;
        Ok(Self {
            format,
            decoder,
            track_id,
            stream: StreamFormat::new(sample_format, sample_rate_hz, channels),
        })
    }

    /// El formato que este archivo quiere que se abra en el dispositivo.
    pub fn stream_format(&self) -> StreamFormat {
        self.stream
    }

    /// El siguiente bloque. `None` cuando se acabó el archivo.
    pub fn next_block(&mut self) -> Result<Option<Block>, DecodeError> {
        loop {
            let packet = match self.format.next_packet() {
                Ok(p) => p,
                Err(SymphoniaError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    return Ok(None)
                }
                Err(SymphoniaError::ResetRequired) => return Ok(None),
                Err(e) => return Err(DecodeError::Corrupt(e.to_string())),
            };
            if packet.track_id() != self.track_id {
                continue;
            }
            match self.decoder.decode(&packet) {
                Ok(buffer) => return Ok(Some(interleave(&buffer))),
                // Un paquete dañado no tumba la pista: se salta.
                Err(SymphoniaError::DecodeError(_)) => continue,
                Err(e) => return Err(DecodeError::Corrupt(e.to_string())),
            }
        }
    }
}

/// Pasa un búfer planar de symphonia a muestras intercaladas, conservando el
/// tipo original.
fn interleave(buffer: &AudioBufferRef<'_>) -> Block {
    macro_rules! ints {
        ($buf:expr, $bits:expr, $conv:expr) => {{
            let frames = $buf.frames();
            let channels = $buf.spec().channels.count();
            let mut data = Vec::with_capacity(frames * channels);
            for f in 0..frames {
                for c in 0..channels {
                    data.push($conv($buf.chan(c)[f]));
                }
            }
            Block::Int { data, bits: $bits }
        }};
    }
    match buffer {
        AudioBufferRef::S16(b) => ints!(b, 16, |v: i16| i32::from(v)),
        AudioBufferRef::S24(b) => ints!(b, 24, |v: symphonia::core::sample::i24| v.inner()),
        AudioBufferRef::S32(b) => ints!(b, 32, |v: i32| v),
        AudioBufferRef::U8(b) => ints!(b, 8, |v: u8| i32::from(v) - 128),
        AudioBufferRef::F32(b) => {
            let frames = b.frames();
            let channels = b.spec().channels.count();
            let mut data = Vec::with_capacity(frames * channels);
            for f in 0..frames {
                for c in 0..channels {
                    data.push(b.chan(c)[f]);
                }
            }
            Block::Float(data)
        }
        other => {
            // Los formatos que faltan se convierten a f32 una sola vez, y el
            // camino ya no se anuncia como bit-perfect.
            let mut owned = other.make_equivalent::<f32>();
            other.convert(&mut owned);
            let frames = owned.frames();
            let channels = owned.spec().channels.count();
            let mut data = Vec::with_capacity(frames * channels);
            for f in 0..frames {
                for c in 0..channels {
                    data.push(owned.chan(c)[f]);
                }
            }
            Block::Float(data)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_archivo_que_no_existe_falla_al_abrir() {
        let r = Decoded::open(Path::new("/no/existe.flac"));
        assert!(matches!(r, Err(DecodeError::Open(_))));
    }

    #[test]
    fn los_frames_se_cuentan_por_canal() {
        let b = Block::Int { data: vec![0; 10], bits: 16 };
        assert_eq!(b.frames(2), 5);
    }
}
