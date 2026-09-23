//! Reproducir un archivo local de principio a fin.
//!
//! Es la rebanada vertical que amarra todo: decodificar sin tocar los
//! enteros, medir el dispositivo, negociar un formato exacto y entregar sin
//! confundir una falla con un fin de pista.

use chilpa_audio::format::StreamFormat;
use chilpa_audio::negotiate::{negotiate, Delivery, NegotiationError, Policy};
use chilpa_audio::pack::{pack, PackError, Samples};
use chilpa_audio::render::{FrameSource, Outcome};
use chilpa_audio::sink::{AudioSink, SinkError, StopSignal};
use chilpa_decode::{Block, DecodeError, Decoded};
use std::path::Path;

#[derive(Debug)]
pub enum PlayError {
    Decode(DecodeError),
    Negotiation(NegotiationError),
    Pack(PackError),
    Sink(SinkError),
}

impl std::fmt::Display for PlayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlayError::Decode(e) => write!(f, "{e}"),
            PlayError::Negotiation(NegotiationError::CapsNotProbed) => {
                write!(f, "no se midió el dispositivo; sin eso no se promete bit-perfect")
            }
            PlayError::Negotiation(NegotiationError::Unsupported { requested, .. }) => write!(
                f,
                "el dispositivo no acepta {:?} a {} Hz y la política no deja convertir",
                requested.sample_format, requested.sample_rate_hz
            ),
            PlayError::Pack(PackError::WouldTruncate { from_bits, to_bits }) => {
                write!(f, "el archivo trae {from_bits} bits y el dispositivo solo toma {to_bits}")
            }
            PlayError::Sink(e) => write!(f, "dispositivo: {e:?}"),
        }
    }
}

/// Puente entre el decodificador y el dispositivo: cada bloque se empaqueta
/// en el formato con el que se abrió la salida.
struct Frames<'a> {
    decoded: &'a mut Decoded,
    opened: StreamFormat,
    /// El primer error de empaquetado, para reportarlo en vez de callarlo.
    error: Option<PackError>,
}

impl FrameSource for Frames<'_> {
    fn next_block(&mut self) -> Option<Vec<u8>> {
        loop {
            let block = match self.decoded.next_block() {
                Ok(Some(b)) => b,
                Ok(None) | Err(_) => return None,
            };
            let samples = match &block {
                Block::Int { data, bits } => Samples::Int { data, bits: *bits },
                Block::Float(data) => Samples::Float(data),
            };
            let mut bytes = Vec::new();
            match pack(&samples, self.opened.sample_format, &mut bytes) {
                Ok(()) if bytes.is_empty() => continue,
                Ok(()) => return Some(bytes),
                Err(e) => {
                    self.error = Some(e);
                    return None;
                }
            }
        }
    }
}

/// Lo que pasó al reproducir.
pub struct Played {
    pub delivery: Delivery,
    pub outcome: Outcome,
    /// Cuántos frames tenía la pista, cuando el contenedor lo dice.
    pub total_frames: Option<u64>,
}

/// Reproduce un stream de red.
///
/// No hay duración ni búsqueda: un stream se oye hasta que se corta o hasta
/// que alguien lo para.
///
/// `on_delivery` se llama con la entrega ya negociada, antes de abrir el
/// dispositivo: es el único momento en que se sabe si el stream va
/// bit-perfect, porque en una radio no hay un final donde decirlo después.
#[allow(clippy::too_many_arguments)]
pub fn play_stream<F, K>(
    stream: chilpa_stream::HttpStream,
    caps: &chilpa_audio::negotiate::DeviceCaps,
    policy: Policy,
    stop: &StopSignal,
    pause: &chilpa_audio::sink::PauseSignal,
    open_sink: F,
    on_delivery: &mut dyn FnMut(&Delivery),
    on_progress: &mut dyn FnMut(u64),
) -> Result<Played, PlayError>
where
    F: FnOnce(StreamFormat) -> Result<K, SinkError>,
    K: AudioSink,
{
    let mut decoded = Decoded::open_stream(stream).map_err(PlayError::Decode)?;
    let delivery =
        negotiate(decoded.stream_format(), caps, policy).map_err(PlayError::Negotiation)?;
    on_delivery(&delivery);
    let mut sink = open_sink(delivery.opened).map_err(PlayError::Sink)?;
    let mut frames = Frames { decoded: &mut decoded, opened: delivery.opened, error: None };
    let outcome =
        chilpa_audio::render::render_paused(&mut frames, &mut sink, stop, pause, on_progress);
    if let Some(e) = frames.error {
        return Err(PlayError::Pack(e));
    }
    sink.close().map_err(PlayError::Sink)?;
    Ok(Played { delivery, outcome, total_frames: None })
}

/// Reproduce un archivo en un dispositivo, con la política dada.
///
/// `open_sink` recibe el formato negociado y entrega el dispositivo abierto;
/// así el core no depende de un backend concreto y las pruebas usan uno
/// falso.
pub fn play_file<F, K>(
    path: &Path,
    caps: &chilpa_audio::negotiate::DeviceCaps,
    policy: Policy,
    stop: &StopSignal,
    open_sink: F,
) -> Result<Played, PlayError>
where
    F: FnOnce(StreamFormat) -> Result<K, SinkError>,
    K: AudioSink,
{
    play_file_with(path, caps, policy, stop, open_sink, &mut |_| {})
}

/// Lo mismo, avisando el progreso en frames mientras suena.
pub fn play_file_with<F, K>(
    path: &Path,
    caps: &chilpa_audio::negotiate::DeviceCaps,
    policy: Policy,
    stop: &StopSignal,
    open_sink: F,
    on_progress: &mut dyn FnMut(u64),
) -> Result<Played, PlayError>
where
    F: FnOnce(StreamFormat) -> Result<K, SinkError>,
    K: AudioSink,
{
    let mut decoded = Decoded::open(path).map_err(PlayError::Decode)?;
    let total_frames = decoded.frame_count();
    let delivery =
        negotiate(decoded.stream_format(), caps, policy).map_err(PlayError::Negotiation)?;
    let mut sink = open_sink(delivery.opened).map_err(PlayError::Sink)?;
    let mut frames = Frames { decoded: &mut decoded, opened: delivery.opened, error: None };
    let outcome = chilpa_audio::render::render_with(&mut frames, &mut sink, stop, on_progress);
    if let Some(e) = frames.error {
        return Err(PlayError::Pack(e));
    }
    sink.close().map_err(PlayError::Sink)?;
    Ok(Played { delivery, outcome, total_frames })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chilpa_audio::format::SampleFormat;
    use chilpa_audio::negotiate::DeviceCaps;

    fn caps() -> DeviceCaps {
        DeviceCaps {
            sample_rates: vec![44_100],
            formats: vec![SampleFormat::S16],
            channels: vec![2],
            probed: true,
        }
    }

    struct NullSink;

    impl AudioSink for NullSink {
        fn format(&self) -> StreamFormat {
            StreamFormat::new(SampleFormat::S16, 44_100, 2)
        }
        fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
            Ok(frames.len() / 4)
        }
        fn close(&mut self) -> Result<(), SinkError> {
            Ok(())
        }
    }

    #[test]
    fn un_archivo_que_no_existe_se_reporta_como_tal() {
        let r = play_file(
            Path::new("/no/existe.flac"),
            &caps(),
            Policy::BitPerfect,
            &StopSignal::new(),
            |_| Ok(NullSink),
        );
        assert!(matches!(r, Err(PlayError::Decode(DecodeError::Open(_)))));
    }
}
