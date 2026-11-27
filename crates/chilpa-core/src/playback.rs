//! Playing a local file from start to finish.
//!
//! It is the vertical slice that ties everything together: decode without
//! touching the integers, measure the device, negotiate an exact format and
//! deliver without mistaking a failure for the end of a track.

use chilpa_audio::format::StreamFormat;
use chilpa_audio::negotiate::{negotiate, Delivery, NegotiationError, Policy};
use chilpa_audio::pack::{duplicate_mono, pack, pack_pcm_reference, PackError, Samples};
use chilpa_audio::render::{FrameSource, Outcome};
use chilpa_audio::sink::{AudioSink, SinkError, StopSignal};
use chilpa_decode::{Block, DecodeError, Decoded};
use std::path::Path;

#[derive(Debug)]
pub enum PlayError {
    Decode(DecodeError),
    Negotiation(NegotiationError),
    ChannelOutput { path: std::path::PathBuf, index: usize, requested: u16, offered: Vec<u16> },
    Pack(PackError),
    Sink(SinkError),
    /// The track's own data would not read: a one-bit file, a disc image or
    /// a server's item that did not open. Not the device.
    Unreadable(String),
}

impl PlayError {
    pub(crate) fn at_entry(self, path: &Path, index: usize) -> Self {
        match self {
            Self::Negotiation(NegotiationError::Channels { requested, offered }) if (3..=8).contains(&requested) && offered.contains(&2) =>
                Self::ChannelOutput { path:path.to_path_buf(),index,requested,offered },
            other => other,
        }
    }
    /// The track is at fault, not the output: a queue passes over it instead
    /// of ending. Negotiation, packing and the device are the output's.
    pub fn is_track(&self) -> bool {
        matches!(self, PlayError::Decode(_) | PlayError::Unreadable(_))
    }
}

/// How a track ended once what went wrong is known. The renderer only hears
/// that the source stopped giving blocks and calls it `Unreadable`; when the
/// reason was a block the output could not take (`packing`), the failure is
/// the output's, and the queue stops on it instead of passing the track over.
pub(crate) fn output_fault(outcome: Outcome, packing: Option<PackError>) -> Outcome {
    match (outcome, packing) {
        (Outcome::Unreadable { frames, .. }, Some(error)) => {
            Outcome::Failed { frames, error: SinkError::Backend(PlayError::Pack(error).to_string()) }
        }
        (outcome, _) => outcome,
    }
}

impl std::fmt::Display for PlayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlayError::Decode(e) => write!(f, "{e}"),
            PlayError::ChannelOutput { requested, offered, .. } => write!(f, "{}", channels_refused(*requested,offered)),
            PlayError::Negotiation(NegotiationError::CapsNotProbed) => write!(
                f,
                "{}",
                chilpa_i18n::t(
                    "the device was not measured; without that bit-perfect is not promised"
                )
            ),
            PlayError::Negotiation(NegotiationError::Unsupported { requested, .. }) => write!(
                f,
                "{}",
                chilpa_i18n::t_args(
                    "the device does not accept {} at {} Hz, and the policy does not allow \
                     converting",
                    &[
                        &format!("{:?}", requested.sample_format),
                        &requested.sample_rate_hz.to_string(),
                    ],
                )
            ),
            PlayError::Negotiation(NegotiationError::Channels { requested, offered }) => {
                write!(f, "{}", channels_refused(*requested, offered))
            }
            PlayError::Negotiation(NegotiationError::Rate { requested_hz, offered }) => {
                let offered: Vec<String> = offered.iter().map(u32::to_string).collect();
                write!(
                    f,
                    "{}",
                    chilpa_i18n::t_args(
                        "the device does not play {} Hz (it plays {}), and Chilpa does not convert \
                         rates",
                        &[&requested_hz.to_string(), &offered.join(", ")],
                    )
                )
            }
            PlayError::Pack(PackError::WouldTruncate { from_bits, to_bits }) => write!(
                f,
                "{}",
                chilpa_i18n::t_args(
                    "the file carries {} bits and the device only takes {}",
                    &[&from_bits.to_string(), &to_bits.to_string()],
                )
            ),
            PlayError::Pack(PackError::NativeDsdNeedsDsdSource) => write!(
                f,
                "{}",
                chilpa_i18n::t("PCM samples cannot be written to a native DSD stream")
            ),
            PlayError::Sink(e) => {
                write!(f, "{}", chilpa_i18n::t_args("device: {}", &[&e.to_string()]))
            }
            PlayError::Unreadable(why) => write!(f, "{why}"),
        }
    }
}

/// What is said when the device takes none of the channel counts a track
/// could go out in. The format and the rate are not the problem then, and
/// the message used to name only those.
pub(crate) fn channels_refused(requested: u16, offered: &[u16]) -> String {
    let offered: Vec<String> = offered.iter().map(u16::to_string).collect();
    chilpa_i18n::t_args(
        "the device takes {} channels and this track has {}",
        &[&offered.join(", "), &requested.to_string()],
    )
}

/// Bridge between the decoder and the device: every block is packed in the
/// format the output was opened with.
struct Frames<'a> {
    decoded: &'a mut Decoded,
    opened: StreamFormat,
    /// A mono track going out in both channels.
    spread: bool,
    /// The first packing error, to report it instead of swallowing it.
    error: Option<PackError>,
    opus_trim: Option<chilpa_audio::gapless::Trimmer<f32>>,
    /// What the blocks are packed in when the renderer scales and converts
    /// them on their way out, as a queue's track; none packs for the device.
    gain_format: Option<StreamFormat>,
}

impl<'a> Frames<'a> {
    fn new(decoded: &'a mut Decoded, delivery: &Delivery) -> Self {
        let opus_trim = (decoded.codec() == "opus").then(||
            chilpa_audio::gapless::Trimmer::new(decoded.encoder_delay(), decoded.encoder_padding(), decoded.stream_format().channels));
        Self { decoded, opened: delivery.opened, spread: delivery.duplicates_mono(), error: None, opus_trim, gain_format: None }
    }

    /// Packed in `format` for the renderer to scale before it converts.
    fn scaled_in(mut self, format: StreamFormat) -> Self {
        self.gain_format = Some(format);
        self
    }
}

/// The format a PCM track is scaled in before it meets the output: float
/// for a float source, so a lowered volume is neither clipped nor rounded
/// before it is applied; the source's own integers when the output opened
/// at float or is narrower than a Vorbis reference; the output's otherwise.
pub(crate) fn gain_format(decoded: &Decoded, opened: StreamFormat) -> StreamFormat {
    use chilpa_audio::format::SampleFormat::F32;
    let original = decoded.stream_format().sample_format;
    let own = original == F32
        || (original.is_integer() && (opened.sample_format == F32
            || (decoded.codec() == "vorbis" && original.valid_bits() > opened.sample_format.valid_bits())));
    StreamFormat { sample_format: if own { original } else { opened.sample_format }, ..opened }
}

pub(crate) fn prepare_channels(decoded: &mut Decoded, caps: &chilpa_audio::negotiate::DeviceCaps, stereo: bool, fallback: bool) -> Result<(),PlayError> {
    let channels=decoded.stream_format().channels;
    if stereo || (fallback && channels > 2 && !caps.channels.contains(&channels) && caps.channels.contains(&2)) {
        decoded.downmix_to_stereo().map_err(PlayError::Decode)?;
    }
    Ok(())
}

pub(crate) fn negotiate_decoded(decoded: &Decoded, caps: &chilpa_audio::negotiate::DeviceCaps, policy: Policy) -> Result<Delivery, NegotiationError> {
    let mut delivery = match negotiate(decoded.stream_format(), caps, policy) {
        Err(NegotiationError::Unsupported { .. }) if decoded.codec() == "vorbis" && policy == Policy::BitPerfect => {
            // A narrow or float-only output still plays, with its conversion
            // recorded against the fixed signed 32-bit decoder reference.
            negotiate(decoded.stream_format(), caps, Policy::AllowAnything)
        }
        other => other,
    }?;
    let source = decoded.source_stream_format();
    if source.channels != decoded.stream_format().channels {
        delivery.requested = source;
        delivery.transforms.insert(0, chilpa_audio::negotiate::Transform::Downmixed {
            from_channels: source.channels, to_channels: decoded.stream_format().channels,
        });
    }
    Ok(delivery)
}

pub(crate) fn pack_decoded(decoded: &Decoded, samples: &Samples<'_>, to: chilpa_audio::format::SampleFormat, out: &mut Vec<u8>) -> Result<(), PackError> {
    if decoded.codec() == "vorbis" { pack_pcm_reference(samples, to, out) }
    else { pack(samples, to, out) }
}

pub(crate) fn signal_bits(decoded: &Decoded, delivery: &Delivery) -> Option<u8> {
    // A Vorbis output reference has 32 bits, but the encoded source has no
    // integer depth. The wire verdict still checks all reference bits.
    if decoded.codec() == "vorbis" { None } else { delivery.bits_carried() }
}

impl FrameSource for Frames<'_> {
    fn pcm_format(&self) -> Option<StreamFormat> { self.gain_format }

    fn next_block(&mut self) -> Result<Option<Vec<u8>>, String> {
        loop {
            let block = match self.decoded.next_block() {
                Ok(Some(b)) => b,
                Ok(None) => return Ok(None),
                Err(e) => return Err(e.to_string()),
            };
            let block = match (block, self.opus_trim.as_mut()) {
                (Block::Float(data), Some(trim)) => Block::Float(trim.push(&data)),
                (block, _) => block,
            };
            let block = match block {
                Block::Int { data, bits } if self.spread => Block::Int { data: duplicate_mono(&data), bits },
                Block::Float(data) if self.spread => Block::Float(duplicate_mono(&data)),
                block => block,
            };
            let mut bytes = Vec::new();
            let to = self.gain_format.map_or(self.opened.sample_format, |format| format.sample_format);
            let packed = match &block {
                // Kept as decoded, past full scale too, as a queue's track
                // keeps it: the volume brings the overs back before the
                // conversion to the device would clip them.
                Block::Float(data) if self.gain_format.is_some() && to == chilpa_audio::format::SampleFormat::F32 => {
                    bytes.extend(data.iter().flat_map(|sample| sample.to_le_bytes()));
                    Ok(())
                }
                Block::Float(data) => pack_decoded(self.decoded, &Samples::Float(data), to, &mut bytes),
                Block::Int { data, bits } => pack_decoded(self.decoded, &Samples::Int { data, bits: *bits }, to, &mut bytes),
            };
            match packed {
                Ok(()) if bytes.is_empty() => continue,
                Ok(()) => return Ok(Some(bytes)),
                Err(e) => {
                    self.error = Some(e);
                    return Ok(None);
                }
            }
        }
    }
}

/// What happened while playing.
pub struct Played {
    pub delivery: Delivery,
    pub outcome: Outcome,
    /// How many frames the track had, when the container says so.
    pub total_frames: Option<u64>,
}

/// Reproduce un stream de red.
///
/// There is no duration and no seeking: a stream plays until it is cut off
/// or until someone stops it.
///
/// `on_delivery` is called with the delivery already negotiated, before the
/// device is opened: it is the only moment when it is known whether the
/// stream goes bit-perfect, because a radio has no end at which to say so.
#[allow(clippy::too_many_arguments)]
pub fn play_stream<F, K>(
    stream: chilpa_stream::HttpStream,
    caps: &chilpa_audio::negotiate::DeviceCaps,
    policy: Policy,
    stop: &StopSignal,
    pause: &chilpa_audio::sink::PauseSignal,
    dsp: Option<chilpa_audio::dsp::Control>,
    open_sink: F,
    on_delivery: &mut dyn FnMut(&Delivery),
    on_progress: &mut dyn FnMut(u64),
) -> Result<Played, PlayError>
where
    F: FnOnce(StreamFormat) -> Result<K, SinkError>,
    K: AudioSink,
{
    play_stream_told(stream, caps, policy, stop, pause, false, false, dsp, None, crate::attempts::Touched::default(), crate::playback_events::Report::default(), open_sink,
        &mut |delivery, _| on_delivery(delivery), on_progress).map(|(played, ..)| played)
}

/// The player bar's volume as a stream takes it: the factor, read on every
/// block, and the flag that says whether it changed any sample.
pub(crate) struct StreamVolume {
    pub level: crate::queue_playback::Level,
    pub active: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn play_stream_told<F, K>(
    stream: chilpa_stream::HttpStream,
    caps: &chilpa_audio::negotiate::DeviceCaps,
    policy: Policy,
    stop: &StopSignal,
    pause: &chilpa_audio::sink::PauseSignal,
    stereo: bool,
    fallback: bool,
    dsp: Option<chilpa_audio::dsp::Control>,
    volume: Option<StreamVolume>,
    touched: crate::attempts::Touched,
    report: crate::playback_events::Report,
    open_sink: F,
    on_delivery: &mut dyn FnMut(&Delivery, Option<u8>),
    on_progress: &mut dyn FnMut(u64),
) -> Result<(Played, u64, crate::attempts::How), PlayError>
where
    F: FnOnce(StreamFormat) -> Result<K, SinkError>,
    K: AudioSink,
{
    let mut decoded = Decoded::open_stream(stream).map_err(PlayError::Decode)?;
    prepare_channels(&mut decoded,caps,stereo,fallback)?;
    let mut delivery =
        negotiate_decoded(&decoded, caps, policy).map_err(PlayError::Negotiation)?;
    if dsp.as_ref().is_some_and(|control| control.requested()) {
        delivery.transforms.push(chilpa_audio::negotiate::Transform::DspApplied);
    }
    // The volume scales a stream as it scales a file, off the bit-perfect
    // policy only, and is said in the delivery when it changes anything.
    let volume = volume.filter(|_| policy != Policy::BitPerfect);
    if volume.as_ref().is_some_and(|volume| volume.level.get() != 1.0) {
        delivery.transforms.push(chilpa_audio::negotiate::Transform::VolumeApplied);
    }
    // Opened before it is told, as a queue's track is: what is told names
    // the output, and that is only known once one opened.
    let opened = open_sink(delivery.opened).map_err(PlayError::Sink)?;
    on_delivery(&delivery, signal_bits(&decoded, &delivery));
    let mut sink = crate::playback_events::Measured::reporting(opened, report.clone());
    sink.begin_attempt(1, None);
    // Scaled where a queue's track is, before the conversion to the device,
    // so a file and a stream sound the same at the same volume.
    let scaled = gain_format(&decoded, delivery.opened);
    let spread = delivery.duplicates_mono();
    let mut frames = Frames::new(&mut decoded, &delivery).scaled_in(scaled);
    // Marked as it happens, so a stream that fails or unwinds afterwards
    // still closes with what was done to it.
    let dsp_witness = crate::attempts::DspWitness { dsp: dsp.clone(), touched: touched.clone() };
    let mut processor = dsp.map(|control| chilpa_audio::dsp::Processor::new(control, sink.format()));
    let mut level = crate::loudness::level::Leveler::volume_only(volume.as_ref().map(|volume| volume.level.clone()));
    let mut gain = |bytes: &mut [u8]| {
        let changed = level.packed(bytes, scaled, spread);
        if changed {
            touched.mark();
        }
        if let Some(volume) = &volume {
            volume.active.store(changed, std::sync::atomic::Ordering::Release);
        }
    };
    let gain = volume.is_some().then_some(&mut gain as &mut dyn FnMut(&mut [u8]));
    let outcome = chilpa_audio::render::render_with_volume(&mut frames, &mut sink, stop, pause, processor.as_mut(), gain, on_progress);
    drop(dsp_witness);
    sink.end(match &outcome {
        Outcome::Finished { .. } => "finished",
        Outcome::Stopped { .. } => "stopped",
        _ => "error",
    });
    if let Some(e) = frames.error {
        return Err(PlayError::Pack(e));
    }
    // What is still in the device is the last of the track: heard, unless
    // somebody stopped.
    if matches!(outcome, Outcome::Finished { .. }) {
        sink.drain(&|| stop.is_stopped()).map_err(PlayError::Sink)?;
    }
    sink.close().map_err(PlayError::Sink)?;
    // How it ended, raised by what became of the audio it still had in the
    // device: thrown away by a stop, it was stopped.
    let mut how = crate::attempts::How::of(&outcome, false);
    let retired = report.take().into_iter().next();
    match retired.map(|retired| retired.lost) {
        Some(crate::playback_events::Lost::Error) => how = how.max(crate::attempts::How::Failed),
        Some(crate::playback_events::Lost::Cut) if stop.is_stopped() => how = how.max(crate::attempts::How::Stopped),
        _ => {}
    }
    let heard = retired.map_or(0, |retired| retired.played);
    Ok((Played { delivery, outcome, total_frames: None }, heard, how))
}

/// Plays a file on a device, with the given policy.
///
/// `open_sink` receives the negotiated format and hands back the opened
/// device; that way the core does not depend on a concrete backend and the
/// tests use a fake one.
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

/// The same, reporting progress in frames while it plays.
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
        negotiate_decoded(&decoded, caps, policy).map_err(PlayError::Negotiation)?;
    let mut sink = open_sink(delivery.opened).map_err(PlayError::Sink)?;
    let mut frames = Frames::new(&mut decoded, &delivery);
    let outcome = chilpa_audio::render::render_with(&mut frames, &mut sink, stop, on_progress);
    if let Some(e) = frames.error {
        return Err(PlayError::Pack(e));
    }
    // What is still in the device is the last of the track: heard, unless
    // somebody stopped.
    if matches!(outcome, Outcome::Finished { .. }) {
        sink.drain(&|| stop.is_stopped()).map_err(PlayError::Sink)?;
    }
    sink.close().map_err(PlayError::Sink)?;
    Ok(Played { delivery, outcome, total_frames })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_device_error_reads_as_a_sentence_and_not_as_code() {
        // The window said "device: DeviceLost" and "device: Backend(\"...\")".
        let lost = PlayError::Sink(SinkError::DeviceLost).to_string();
        assert!(!lost.contains("DeviceLost"), "{lost}");
        let backend = PlayError::Sink(SinkError::Backend("the graph is gone".into())).to_string();
        assert!(backend.ends_with("the graph is gone") && !backend.contains("Backend"), "{backend}");
    }
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
    fn a_file_that_does_not_exist_is_reported_as_such() {
        let r = play_file(
            Path::new("/does/not/exist.flac"),
            &caps(),
            Policy::BitPerfect,
            &StopSignal::new(),
            |_| Ok(NullSink),
        );
        assert!(matches!(r, Err(PlayError::Decode(DecodeError::Open(_)))));
    }
}
