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
    Pack(PackError),
    Sink(SinkError),
}

impl std::fmt::Display for PlayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlayError::Decode(e) => write!(f, "{e}"),
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
}

pub(crate) fn negotiate_decoded(decoded: &Decoded, caps: &chilpa_audio::negotiate::DeviceCaps, policy: Policy) -> Result<Delivery, NegotiationError> {
    match negotiate(decoded.stream_format(), caps, policy) {
        Err(NegotiationError::Unsupported { .. }) if decoded.codec() == "vorbis" && policy == Policy::BitPerfect => {
            // A narrow or float-only output still plays, with its conversion
            // recorded against the fixed signed 32-bit decoder reference.
            negotiate(decoded.stream_format(), caps, Policy::AllowAnything)
        }
        other => other,
    }
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
    fn next_block(&mut self) -> Result<Option<Vec<u8>>, String> {
        loop {
            let block = match self.decoded.next_block() {
                Ok(Some(b)) => b,
                Ok(None) => return Ok(None),
                Err(e) => return Err(e.to_string()),
            };
            let block = match block {
                Block::Int { data, bits } if self.spread => Block::Int { data: duplicate_mono(&data), bits },
                Block::Float(data) if self.spread => Block::Float(duplicate_mono(&data)),
                block => block,
            };
            let samples = match &block {
                Block::Int { data, bits } => Samples::Int { data, bits: *bits },
                Block::Float(data) => Samples::Float(data),
            };
            let mut bytes = Vec::new();
            match pack_decoded(self.decoded, &samples, self.opened.sample_format, &mut bytes) {
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
    play_stream_told(stream, caps, policy, stop, pause, dsp, open_sink,
        &mut |delivery, _| on_delivery(delivery), on_progress)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn play_stream_told<F, K>(
    stream: chilpa_stream::HttpStream,
    caps: &chilpa_audio::negotiate::DeviceCaps,
    policy: Policy,
    stop: &StopSignal,
    pause: &chilpa_audio::sink::PauseSignal,
    dsp: Option<chilpa_audio::dsp::Control>,
    open_sink: F,
    on_delivery: &mut dyn FnMut(&Delivery, Option<u8>),
    on_progress: &mut dyn FnMut(u64),
) -> Result<Played, PlayError>
where
    F: FnOnce(StreamFormat) -> Result<K, SinkError>,
    K: AudioSink,
{
    let mut decoded = Decoded::open_stream(stream).map_err(PlayError::Decode)?;
    let delivery =
        negotiate_decoded(&decoded, caps, policy).map_err(PlayError::Negotiation)?;
    on_delivery(&delivery, signal_bits(&decoded, &delivery));
    let mut sink = open_sink(delivery.opened).map_err(PlayError::Sink)?;
    let mut frames = Frames {
        decoded: &mut decoded,
        opened: delivery.opened,
        spread: delivery.duplicates_mono(),
        error: None,
    };
    let outcome =
        chilpa_audio::render::render_processed(&mut frames, &mut sink, stop, pause, dsp, on_progress);
    if let Some(e) = frames.error {
        return Err(PlayError::Pack(e));
    }
    // What is still in the device is the last of the track: heard, unless
    // somebody stopped.
    if matches!(outcome, Outcome::Finished { .. }) {
        sink.drain(&|| stop.is_stopped()).map_err(PlayError::Sink)?;
    }
    sink.close().map_err(PlayError::Sink)?;
    Ok(Played { delivery, outcome, total_frames: None })
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
    let mut frames = Frames {
        decoded: &mut decoded,
        opened: delivery.opened,
        spread: delivery.duplicates_mono(),
        error: None,
    };
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
