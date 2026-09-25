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
use chilpa_audio::pack::{duplicate_mono, pack, Samples};
use chilpa_audio::render::{render_paused, FrameSource, Outcome};
use chilpa_audio::seek::SeekSignal;
use chilpa_audio::sink::{AudioSink, PauseSignal, SinkError, SkipSignal, StopSignal};
use chilpa_dsd::{DsdError, DsdSource};
use std::path::Path;

#[derive(Debug)]
pub enum DsdPlayError {
    Read(DsdError),
    /// The device cannot handle the carrier rate.
    Unsupported { carrier_hz: u32 },
    /// The device takes none of the channel counts the track could go out in.
    Channels { requested: u16, offered: Vec<u16> },
    Sink(SinkError),
}

impl std::fmt::Display for DsdPlayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DsdPlayError::Read(e) => write!(f, "{e}"),
            DsdPlayError::Unsupported { carrier_hz } => write!(
                f,
                "{}",
                chilpa_i18n::t_args(
                    "the device does not accept {} Hz, which is what this DSD over DoP asks for",
                    &[&carrier_hz.to_string()],
                )
            ),
            DsdPlayError::Channels { requested, offered } => {
                write!(f, "{}", crate::playback::channels_refused(*requested, offered))
            }
            DsdPlayError::Sink(e) => {
                write!(f, "{}", chilpa_i18n::t_args("device: {}", &[&e.to_string()]))
            }
        }
    }
}

/// True if the file is DSD by its extension: DSF or DSDIFF.
pub(crate) fn is_dsd_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("dsf") || e.eq_ignore_ascii_case("dff"))
}

/// Opens a DSD file, whichever of the two containers it is.
pub(crate) fn open_dsd_file(path: &Path) -> Result<Box<dyn DsdSource>, chilpa_dsd::DsdError> {
    let dsdiff = path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("dff"));
    Ok(match dsdiff {
        true => Box::new(chilpa_dsd::DffReader::open(path)?),
        false => Box::new(chilpa_dsd::DsfReader::open(path)?),
    })
}

/// What a DSD source asks the device for: 24 bit words at the carrier rate.
///
/// DoP is 24 bits -- the marker and sixteen of DSD -- left justified in 32.
/// Asking for `S32` claimed eight bits it does not carry, and a DAC that
/// takes 24 bits in 32 but not 32, which CoreAudio and WASAPI can tell
/// apart, refused it.
pub(crate) fn dop_request(info: &chilpa_dsd::DsdInfo) -> StreamFormat {
    StreamFormat::new(SampleFormat::S24In32, carrier_rate(info.dsd_rate_hz), info.channels)
}

/// Turns DSD blocks into DoP words, in the word the output was opened at.
///
/// The packer is lent, not owned: tracks that share an opening share its
/// marker, so the 0x05 and 0xFA alternation runs on across a join instead
/// of starting over, which some DACs take as the end of DSD.
pub(crate) struct DopFrames<'a> {
    source: &'a mut dyn DsdSource,
    packer: &'a mut DopPacker,
    to: SampleFormat,
    /// A mono DSD track going out in both channels.
    spread: bool,
    skip: Option<SkipSignal>,
    seek: Option<SeekSignal>,
    /// True if this track ended because somebody skipped it.
    pub(crate) skipped: bool,
    /// Bytes of each channel still to drop after a seek: the source lands
    /// on a block or a frame, before the sample asked for.
    discard: usize,
    /// Set by a seek that landed, until the renderer has heard of it.
    moved: bool,
    /// What to add to the DoP frames delivered to get the position in the
    /// track: where the last seek went less what had been delivered by then.
    pub(crate) base: std::sync::Arc<std::sync::atomic::AtomicI64>,
    /// DoP frames the device has taken of this track, as the renderer
    /// reports them.
    pub(crate) delivered: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

impl<'a> DopFrames<'a> {
    pub(crate) fn new(source: &'a mut dyn DsdSource, packer: &'a mut DopPacker, delivery: &Delivery) -> Self {
        DopFrames {
            source,
            packer,
            to: delivery.opened.sample_format,
            spread: delivery.duplicates_mono(),
            skip: None,
            seek: None,
            skipped: false,
            discard: 0,
            moved: false,
            base: std::sync::Arc::new(std::sync::atomic::AtomicI64::new(0)),
            delivered: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    /// Listens to the queue's skip and seek. A skip ends the track; a seek
    /// moves the source, when it can move.
    pub(crate) fn controlled(mut self, skip: SkipSignal, seek: SeekSignal) -> Self {
        self.skip = Some(skip);
        self.seek = Some(seek);
        self
    }
}

impl FrameSource for DopFrames<'_> {
    fn moved(&mut self) -> bool {
        std::mem::take(&mut self.moved)
    }

    fn next_block(&mut self) -> Result<Option<Vec<u8>>, String> {
        // The seek first: taken after a skip, one asked for together with it
        // would outlive this track and land on the next.
        let asked = self.seek.as_ref().and_then(|s| s.take());
        if self.skip.as_ref().is_some_and(|s| s.take()) {
            self.skipped = true;
            return Ok(None);
        }
        if let Some(target) = asked {
            self.seek_to(target)?;
        }
        let mut channels = match self.source.next_block() {
            Ok(Some(c)) => c,
            Ok(None) => return Ok(None),
            Err(e) => return Err(e.to_string()),
        };
        if self.discard > 0 {
            let cut = self.discard.min(channels[0].len());
            for channel in &mut channels {
                channel.drain(..cut.min(channel.len()));
            }
            self.discard -= cut;
            if channels[0].is_empty() {
                return self.next_block();
            }
        }
        let refs: Vec<&[u8]> = channels.iter().map(|c| c.as_slice()).collect();
        let mut words = Vec::new();
        self.packer.pack(&refs, &mut words);
        if words.is_empty() {
            return Ok(None);
        }
        // The packer hands them over left justified in 32 bits, which is
        // `S24In32`, and they are laid out in the word the output opened at
        // by the same packing every PCM file goes through: unchanged in 24
        // in 32 or in 32, three bytes for a DAC with three byte subslots.
        // Writing four bytes whatever had been opened was right only while
        // nothing but a 32 bit word could be opened.
        let samples: Vec<i32> = words.iter().map(|w| w >> 8).collect();
        let samples = if self.spread { duplicate_mono(&samples) } else { samples };
        let mut bytes = Vec::with_capacity(samples.len() * self.to.bytes());
        pack(&Samples::Int { data: &samples, bits: 24 }, self.to, &mut bytes)
            .expect("DoP only opens at a word that holds 24 bits");
        Ok(Some(bytes))
    }
}

impl DopFrames<'_> {
    /// Moves the source to `target`, to the DoP frame: sixteen samples of
    /// each channel, the most one word carries, so the marker and the
    /// channels stay in step.
    fn seek_to(&mut self, target: std::time::Duration) -> Result<(), String> {
        let rate = self.source.info().dsd_rate_hz;
        let wanted = chilpa_audio::seek::frames_at(target, rate) / 16 * 16;
        let Some(landed) = self.source.seek(wanted).map_err(|e| e.to_string())? else {
            // It cannot move, and goes on from where it was.
            return Ok(());
        };
        // Landing before the sample asked for is the rule, and the rest is
        // dropped here; landing after it means the track has no more.
        let at = wanted.max(landed);
        self.discard = ((at - landed) / 8) as usize;
        self.moved = true;
        let delivered = self.delivered.load(std::sync::atomic::Ordering::Relaxed);
        self.base.store((at / 16) as i64 - delivered as i64, std::sync::atomic::Ordering::Relaxed);
        Ok(())
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
    let request = dop_request(&info);

    // Always bit-perfect: converting DoP turns it into noise, so there is no
    // policy to choose here.
    let delivery = match negotiate(request, caps, Policy::BitPerfect) {
        Ok(d) => d,
        Err(NegotiationError::Unsupported { .. })
        | Err(NegotiationError::CapsNotProbed)
        | Err(NegotiationError::Rate { .. }) => return Err(DsdPlayError::Unsupported { carrier_hz }),
        Err(NegotiationError::Channels { requested, offered }) => {
            return Err(DsdPlayError::Channels { requested, offered })
        }
    };

    let mut sink = open_sink(delivery.opened).map_err(DsdPlayError::Sink)?;
    let mut packer = DopPacker::new();
    let mut frames = DopFrames::new(source, &mut packer, &delivery);
    let outcome = render_paused(&mut frames, &mut sink, stop, pause, on_progress);
    // What is still in the device is the last of the track: heard, unless
    // somebody stopped.
    if matches!(outcome, Outcome::Finished { .. }) {
        sink.drain(&|| stop.is_stopped()).map_err(DsdPlayError::Sink)?;
    }
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
    let mut reader = open_dsd_file(path).map_err(DsdPlayError::Read)?;
    play_dsd(reader.as_mut(), caps, stop, pause, open_sink, on_progress)
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
    struct Capture {
        stream: Option<StreamFormat>,
        bytes: Vec<u8>,
    }

    struct Handle<'a>(&'a mut Capture);

    impl AudioSink for Handle<'_> {
        fn format(&self) -> StreamFormat {
            self.0.stream.expect("opened")
        }
        fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
            self.0.bytes.extend_from_slice(frames);
            Ok(frames.len() / self.0.stream.expect("opened").frame_bytes())
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
        let mut cap = Capture::default();
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
        assert!(played.delivery.nothing_was_converted());

        // First word: marker 0x05 and the file's payload, justified.
        let first = i32::from_le_bytes([cap.bytes[0], cap.bytes[1], cap.bytes[2], cap.bytes[3]]);
        assert_eq!(first, ((0x05 << 16) | 0xABCD) << 8);
    }

    /// Plays the test source on a device that takes only `format`, and
    /// hands back what reached it.
    fn played_on(format: SampleFormat) -> (DsdPlayed, Capture) {
        let mut cap = Capture::default();
        let mut f = source();
        let only = DeviceCaps {
            sample_rates: vec![176_400],
            formats: vec![format],
            channels: vec![2],
            probed: true,
        };
        let played = play_dsd(
            &mut f,
            &only,
            &StopSignal::new(),
            &PauseSignal::new(),
            |format| {
                cap.stream = Some(format);
                Ok(Handle(&mut cap))
            },
            &mut |_| {},
        )
        .unwrap_or_else(|e| panic!("DoP did not reach a DAC that takes {format:?}: {e}"));
        (played, cap)
    }

    #[test]
    fn dop_reaches_a_dac_that_only_takes_three_byte_words() {
        // DoP is 24 bits: the marker and sixteen of DSD. A DAC with three
        // byte subslots takes exactly that, and asking for a 32 bit word
        // meant it was refused.
        let (played, cap) = played_on(SampleFormat::S24Packed);
        assert!(played.delivery.nothing_was_converted());
        assert_eq!(cap.bytes.len() % 3, 0, "three bytes a sample");
        assert_eq!(&cap.bytes[..3], &[0xCD, 0xAB, 0x05], "marker 0x05 and the payload, in order");
    }

    #[test]
    fn dop_reaches_a_dac_that_takes_24_bits_in_32_but_not_32() {
        // CoreAudio and WASAPI tell 24-in-32 from 32, and a DAC can offer
        // the first without the second. It is exactly the word DoP is.
        let (played, cap) = played_on(SampleFormat::S24In32);
        assert!(played.delivery.nothing_was_converted());
        let first = i32::from_le_bytes([cap.bytes[0], cap.bytes[1], cap.bytes[2], cap.bytes[3]]);
        assert_eq!(first, ((0x05 << 16) | 0xABCD) << 8);
    }

    #[test]
    fn a_device_that_cannot_reach_the_carrier_is_refused() {
        let mut cap = Capture::default();
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
            other => panic!("it should have been refused: {other:?}"),
        }
        assert!(cap.bytes.is_empty(), "nothing was written to the device");
    }
}
