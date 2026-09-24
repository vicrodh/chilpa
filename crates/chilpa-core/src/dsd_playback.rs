//! Playing DSD over DoP.
//!
//! DoP puts the DSD bits inside 24 bit PCM words, under an alternating
//! marker. To the device it is an ordinary 24 bit stream at the carrier
//! rate; to the DAC it is DSD. That is why the path is the usual one:
//! negotiate, pack and deliver.
//!
//! The one difference, and it is the important one: if the device cannot
//! open that exact rate, nothing is converted. Resampled DoP is not degraded
//! DSD, it is noise.

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
    /// The device cannot handle the carrier rate.
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

/// Turns DSD blocks into DoP words ready for the cable.
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
        // The words already come left justified inside 32 bits: they go to
        // the cable as they are, without touching a bit.
        let mut bytes = Vec::with_capacity(words.len() * 4);
        for word in words {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        let _ = self.channels;
        Some(bytes)
    }
}

/// What happened while playing DSD.
#[derive(Debug)]
pub struct DsdPlayed {
    pub delivery: Delivery,
    pub outcome: Outcome,
    /// The rate of the PCM carrier that was used.
    pub carrier_hz: u32,
}

/// Plays a DSD source over DoP.
///
/// `open_sink` receives the negotiated format, just as on the PCM path.
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

    // Always bit-perfect: converting DoP turns it into noise, so there is no
    // policy to choose here.
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

/// Opens a DSF file and plays it over DoP.
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

    /// A fake DSD source with known blocks.
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

        // First word: marker 0x05 and the file's payload, justified.
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
