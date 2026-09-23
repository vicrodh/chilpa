//! Reproducir DSD por DoP.
//!
//! DoP mete los bits de DSD dentro de palabras PCM de 24 bits, bajo un marker
//! que alterna. Para el dispositivo es un stream normal de 24 bits a la
//! frecuencia portadora; para el DAC es DSD. Por eso la ruta es la misma de
//! siempre: negociar, empaquetar y entregar.
//!
//! La única diferencia, y es la importante: si el dispositivo no puede abrir
//! esa frecuencia exacta, NO se convierte nada. Un DoP resampleado no es DSD
//! degradado, es ruido.

use chilpa_audio::dop::{carrier_rate, DopPacker};
use chilpa_audio::format::{SampleFormat, StreamFormat};
use chilpa_audio::negotiate::{negotiate, DeviceCaps, Delivery, NegotiationError, Policy};
use chilpa_audio::render::{render_paused, FrameSource, Outcome};
use chilpa_audio::sink::{AudioSink, PauseSignal, SinkError, StopSignal};
use chilpa_dsd::{DsdError, DsdSource};
use std::path::Path;

#[derive(Debug)]
pub enum DsdPlayError {
    Read(DsdError),
    /// El dispositivo no puede con la frecuencia portadora.
    Unsupported { carrier_hz: u32 },
    Sink(SinkError),
}

impl std::fmt::Display for DsdPlayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DsdPlayError::Read(e) => write!(f, "{e}"),
            DsdPlayError::Unsupported { carrier_hz } => write!(
                f,
                "el dispositivo no acepta {carrier_hz} Hz, que es lo que pide este DSD por DoP"
            ),
            DsdPlayError::Sink(e) => write!(f, "dispositivo: {e:?}"),
        }
    }
}

/// Convierte bloques de DSD en palabras DoP listas para el cable.
struct DopFrames<'a> {
    source: &'a mut dyn DsdSource,
    packer: DopPacker,
    channels: u16,
}

impl FrameSource for DopFrames<'_> {
    fn next_block(&mut self) -> Option<Vec<u8>> {
        let channels = match self.source.next_block() {
            Ok(Some(c)) => c,
            Ok(None) | Err(_) => return None,
        };
        let refs: Vec<&[u8]> = channels.iter().map(|c| c.as_slice()).collect();
        let mut words = Vec::new();
        self.packer.pack(&refs, &mut words);
        if words.is_empty() {
            return None;
        }
        // Las palabras ya vienen justificadas a la izquierda dentro de 32
        // bits: al cable van tal cual, sin tocar un bit.
        let mut bytes = Vec::with_capacity(words.len() * 4);
        for word in words {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        let _ = self.channels;
        Some(bytes)
    }
}

/// Lo que pasó al reproducir DSD.
#[derive(Debug)]
pub struct DsdPlayed {
    pub delivery: Delivery,
    pub outcome: Outcome,
    /// La frecuencia del portador PCM que se usó.
    pub carrier_hz: u32,
}

/// Reproduce una fuente de DSD por DoP.
///
/// `open_sink` recibe el formato negociado, igual que en la ruta de PCM.
pub fn play_dsd<F, K>(
    source: &mut dyn DsdSource,
    caps: &DeviceCaps,
    stop: &StopSignal,
    pause: &PauseSignal,
    open_sink: F,
    on_progress: &mut dyn FnMut(u64),
) -> Result<DsdPlayed, DsdPlayError>
where
    F: FnOnce(StreamFormat) -> Result<K, SinkError>,
    K: AudioSink,
{

    let info = source.info().clone();
    let carrier_hz = carrier_rate(info.dsd_rate_hz);
    let request = StreamFormat::new(SampleFormat::S32, carrier_hz, info.channels);

    // Bit-perfect siempre: convertir DoP lo vuelve ruido, así que aquí no hay
    // política que elegir.
    let delivery = match negotiate(request, caps, Policy::BitPerfect) {
        Ok(d) => d,
        Err(NegotiationError::Unsupported { .. }) | Err(NegotiationError::CapsNotProbed) => {
            return Err(DsdPlayError::Unsupported { carrier_hz })
        }
    };

    let mut sink = open_sink(delivery.opened).map_err(DsdPlayError::Sink)?;
    let mut frames = DopFrames { source, packer: DopPacker::new(), channels: info.channels };
    let outcome = render_paused(&mut frames, &mut sink, stop, pause, on_progress);
    sink.close().map_err(DsdPlayError::Sink)?;
    Ok(DsdPlayed { delivery, outcome, carrier_hz })
}

/// Abre un archivo DSF y lo reproduce por DoP.
pub fn play_dsf<F, K>(
    path: &Path,
    caps: &DeviceCaps,
    stop: &StopSignal,
    pause: &PauseSignal,
    open_sink: F,
    on_progress: &mut dyn FnMut(u64),
) -> Result<DsdPlayed, DsdPlayError>
where
    F: FnOnce(StreamFormat) -> Result<K, SinkError>,
    K: AudioSink,
{
    let mut reader = chilpa_dsd::DsfReader::open(path).map_err(DsdPlayError::Read)?;
    play_dsd(&mut reader, caps, stop, pause, open_sink, on_progress)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chilpa_dsd::DsdInfo;

    /// Una fuente de DSD de mentiras con bloques conocidos.
    struct TestSource {
        info: DsdInfo,
        blocks: Vec<Vec<Vec<u8>>>,
    }

    impl DsdSource for TestSource {
        fn info(&self) -> &DsdInfo {
            &self.info
        }
        fn next_block(&mut self) -> Result<Option<Vec<Vec<u8>>>, DsdError> {
            Ok(if self.blocks.is_empty() { None } else { Some(self.blocks.remove(0)) })
        }
    }

    #[derive(Default)]
    struct Capturador {
        stream: Option<StreamFormat>,
        bytes: Vec<u8>,
    }

    struct Handle<'a>(&'a mut Capturador);

    impl AudioSink for Handle<'_> {
        fn format(&self) -> StreamFormat {
            self.0.stream.expect("abierto")
        }
        fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
            self.0.bytes.extend_from_slice(frames);
            Ok(frames.len() / self.0.stream.expect("abierto").frame_bytes())
        }
        fn close(&mut self) -> Result<(), SinkError> {
            Ok(())
        }
    }

    fn caps(rates: &[u32]) -> DeviceCaps {
        DeviceCaps {
            sample_rates: rates.to_vec(),
            formats: vec![SampleFormat::S32],
            channels: vec![2],
            probed: true,
        }
    }

    fn source() -> TestSource {
        TestSource {
            info: DsdInfo { dsd_rate_hz: 2_822_400, channels: 2, samples: 32 },
            blocks: vec![vec![vec![0xAB, 0xCD, 0x12, 0x34], vec![0x55, 0xAA, 0x9C, 0x63]]],
        }
    }

    #[test]
    fn dsd64_opens_at_176_400_and_the_bits_come_out_intact() {
        let mut cap = Capturador::default();
        let mut f = source();
        let played = play_dsd(
            &mut f,
            &caps(&[176_400]),
            &StopSignal::new(),
            &PauseSignal::new(),
            |format| {
                cap.stream = Some(format);
                Ok(Handle(&mut cap))
            },
            &mut |_| {},
        )
        .unwrap();

        assert_eq!(played.carrier_hz, 176_400);
        assert_eq!(played.delivery.opened.sample_rate_hz, 176_400);
        assert!(played.delivery.is_bit_perfect());

        // Primera palabra: marker 0x05 y la carga del archivo, justificada.
        let first = i32::from_le_bytes([cap.bytes[0], cap.bytes[1], cap.bytes[2], cap.bytes[3]]);
        assert_eq!(first, ((0x05 << 16) | 0xABCD) << 8);
    }

    #[test]
    fn a_device_that_cannot_reach_the_carrier_is_refused() {
        let mut cap = Capturador::default();
        let mut f = source();
        let r = play_dsd(
            &mut f,
            &caps(&[44_100, 48_000]),
            &StopSignal::new(),
            &PauseSignal::new(),
            |format| {
                cap.stream = Some(format);
                Ok(Handle(&mut cap))
            },
            &mut |_| {},
        );
        match r {
            Err(DsdPlayError::Unsupported { carrier_hz }) => assert_eq!(carrier_hz, 176_400),
            other => panic!("debía rechazarse: {other:?}"),
        }
        assert!(cap.bytes.is_empty(), "no se le escribió nada al dispositivo");
    }
}
