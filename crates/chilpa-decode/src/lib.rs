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
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
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
    delay: u64,
    padding: u32,
    frames: Option<u64>,
    /// El primer bloque, cuando hubo que decodificarlo para saber con
    /// cuántos canales y a qué frecuencia suena el archivo.
    pendiente: Option<Block>,
    /// Bits útiles que declara el archivo. Mandan sobre el ancho del búfer
    /// que entrega el decodificador: symphonia acomoda un FLAC de 16 bits en
    /// un búfer de 32, y quedarse con el 32 haría que el empaquetado creyera
    /// que hay que truncar.
    declared_bits: Option<u8>,
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
        // Un MP4 no siempre declara canales ni frecuencia en el contenedor:
        // los trae el primer bloque decodificado. Por eso, si faltan, se
        // decodifica un bloque de entrada y se guarda para entregarlo luego.
        let declarados = (params.sample_rate, params.channels.map(|c| c.count() as u16));
        // Los bits del archivo mandan: 16 bits salen en S16, 24 en S24In32.
        let sample_format = match params.bits_per_sample.unwrap_or(16) {
            0..=16 => SampleFormat::S16,
            17..=24 => SampleFormat::S24In32,
            _ => SampleFormat::S32,
        };
        let mut decoder = symphonia::default::get_codecs()
            .make(params, &DecoderOptions::default())
            .map_err(|e| DecodeError::Unsupported(e.to_string()))?;
        let delay = u64::from(params.delay.unwrap_or(0));
        let padding = params.padding.unwrap_or(0);
        let frames = params.n_frames;
        let declared_bits = params
            .bits_per_sample
            .map(|b| b.min(32) as u8)
            .or_else(|| alac_bit_depth(params));
        let mut format = format;
        let mut pendiente = None;
        let (sample_rate_hz, channels) = match declarados {
            (Some(rate), Some(channels)) => (rate, channels),
            _ => {
                let (rate, channels, block) =
                    primer_bloque(&mut *format, &mut *decoder, track_id, declared_bits)?;
                pendiente = Some(block);
                (rate, channels)
            }
        };
        Ok(Self {
            format,
            decoder,
            track_id,
            stream: StreamFormat::new(sample_format, sample_rate_hz, channels),
            delay,
            padding,
            frames,
            declared_bits,
            pendiente,
        })
    }

    /// Frames de arranque que metió el codificador y que no son música.
    pub fn encoder_delay(&self) -> u64 {
        self.delay
    }

    /// Frames de cola que metió el codificador.
    pub fn encoder_padding(&self) -> u32 {
        self.padding
    }

    /// Cuántos frames tiene la pista, si el contenedor lo dice.
    pub fn frame_count(&self) -> Option<u64> {
        self.frames
    }

    /// Mueve la lectura lo más cerca posible del frame pedido y regresa el
    /// plan para quedar exactamente ahí: el contenedor casi siempre aterriza
    /// antes, y esos frames se decodifican y se tiran.
    pub fn seek_to_frame(&mut self, frame: u64) -> Result<chilpa_audio::seek::SeekPlan, DecodeError> {
        self.pendiente = None;
        let to = SeekTo::TimeStamp { ts: frame, track_id: self.track_id };
        let landed = self
            .format
            .seek(SeekMode::Accurate, to)
            .map_err(|e| DecodeError::Corrupt(e.to_string()))?;
        // El decodificador arrastra estado del punto anterior.
        self.decoder.reset();
        Ok(chilpa_audio::seek::SeekPlan::after_landing(frame, landed.actual_ts))
    }

    /// El formato que este archivo quiere que se abra en el dispositivo.
    pub fn stream_format(&self) -> StreamFormat {
        self.stream
    }

    /// El siguiente bloque. `None` cuando se acabó el archivo.
    pub fn next_block(&mut self) -> Result<Option<Block>, DecodeError> {
        if let Some(block) = self.pendiente.take() {
            return Ok(Some(block));
        }
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
                Ok(buffer) => return Ok(Some(interleave(&buffer, self.declared_bits))),
                // Un paquete dañado no tumba la pista: se salta.
                Err(SymphoniaError::DecodeError(_)) => continue,
                Err(e) => return Err(DecodeError::Corrupt(e.to_string())),
            }
        }
    }
}

/// ALAC no declara los bits en el contenedor: van en su cookie
/// (ALACSpecificConfig), donde el byte 5 es la profundidad. Sin esto, un ALAC
/// de 16 bits se toma como de 32 y el empaquetado cree que hay que truncar.
fn alac_bit_depth(params: &symphonia::core::codecs::CodecParameters) -> Option<u8> {
    if params.codec != symphonia::core::codecs::CODEC_TYPE_ALAC {
        return None;
    }
    let cookie = params.extra_data.as_deref()?;
    let bits = *cookie.get(5)?;
    (1..=32).contains(&bits).then_some(bits)
}

/// Decodifica hasta el primer bloque con audio y regresa frecuencia, canales
/// y ese bloque, para los contenedores que no lo declaran.
fn primer_bloque(
    format: &mut dyn FormatReader,
    decoder: &mut dyn Decoder,
    track_id: u32,
    declared_bits: Option<u8>,
) -> Result<(u32, u16, Block), DecodeError> {
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(e) => return Err(DecodeError::Unsupported(format!("sin audio legible: {e}"))),
        };
        if packet.track_id() != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(buffer) => {
                let spec = *buffer.spec();
                return Ok((
                    spec.rate,
                    spec.channels.count() as u16,
                    interleave(&buffer, declared_bits),
                ));
            }
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(e) => return Err(DecodeError::Corrupt(e.to_string())),
        }
    }
}

/// Pasa un búfer planar de symphonia a muestras intercaladas, conservando el
/// tipo original.
fn interleave(buffer: &AudioBufferRef<'_>, declared_bits: Option<u8>) -> Block {
    // El decodificador acomoda las muestras justificadas a la izquierda
    // dentro del búfer: un FLAC de 16 bits llega en un búfer de 32 con la
    // muestra corrida 16 lugares. Se baja a su posición real, que es exacto
    // porque los bits de abajo son cero, y así el empaquetado sabe de
    // cuántos bits se trata.
    macro_rules! ints {
        ($buf:expr, $ancho:expr, $conv:expr) => {{
            let bits = declared_bits.unwrap_or($ancho).min($ancho);
            let corrimiento = $ancho - bits;
            let frames = $buf.frames();
            let channels = $buf.spec().channels.count();
            let mut data = Vec::with_capacity(frames * channels);
            for f in 0..frames {
                for c in 0..channels {
                    data.push($conv($buf.chan(c)[f]) >> corrimiento);
                }
            }
            Block::Int { data, bits }
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
