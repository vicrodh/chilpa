//! Playing an audio CD.
//!
//! A CD is not decoded: its sectors are already 16 bit PCM at 44,100 Hz.
//! That is why this is the shortest path of all: read sectors and deliver
//! them.
//!
//! The drive is opened once per track. If a sector cannot be read, the
//! track is cut off there and it is said; it is never padded with silence.

use chilpa_audio::format::{SampleFormat, StreamFormat};
use chilpa_audio::negotiate::{negotiate, DeviceCaps, Delivery, Policy};
use chilpa_audio::pack::{pack, Samples};
use chilpa_audio::render::{render_paused, FrameSource, Outcome};
use chilpa_audio::sink::{AudioSink, PauseSignal, SinkError, StopSignal};
use chilpa_cd::{CdDevice, CdError, Toc, Track, TrackReader, CDDA_FORMAT};

#[derive(Debug)]
pub enum CdPlayError {
    Drive(CdError),
    /// The output device does not accept a CD's format.
    Unsupported,
    Sink(SinkError),
}

impl std::fmt::Display for CdPlayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CdPlayError::Drive(e) => write!(f, "{e}"),
            CdPlayError::Unsupported => write!(
                f,
                "{}",
                chilpa_i18n::t(
                    "the output does not accept 16 bit at 44,100 Hz, which is what a CD carries"
                )
            ),
            CdPlayError::Sink(e) => {
                write!(f, "{}", chilpa_i18n::t_args("device: {}", &[&format!("{e:?}")]))
            }
        }
    }
}

/// Reads whatever disc is in the drive.
pub fn read_toc(drive: &mut dyn CdDevice) -> Result<Toc, CdPlayError> {
    chilpa_cd::read_toc(drive).map_err(CdPlayError::Drive)
}

/// What happened while playing a track off the disc.
#[derive(Debug)]
pub struct CdPlayed {
    pub delivery: Delivery,
    pub outcome: Outcome,
    /// If a sector could not be read, it is said here.
    pub read_error: Option<String>,
}

/// The disc's sectors in the word the output was opened with.
///
/// A sector is 16 bit pairs, and handing it as it is to an output opened at
/// 32 bits reads two samples as one: half as many frames, each one noise,
/// at twice the speed. A DAC that only takes 32 bit words gets each sample on
/// top of the word with zeros below, which keeps its value, through the same
/// packing every decoded file goes through.
struct LaidOut<'r, 'd> {
    reader: &'r mut TrackReader<'d>,
    to: SampleFormat,
}

impl FrameSource for LaidOut<'_, '_> {
    /// A sector that cannot be read ends the track here and is reported by
    /// the reader itself, beside the outcome.
    fn next_block(&mut self) -> Result<Option<Vec<u8>>, String> {
        let Some(block) = self.reader.next_block()? else { return Ok(None) };
        if self.to == CDDA_FORMAT.sample_format {
            return Ok(Some(block));
        }
        let samples: Vec<i32> = block
            .chunks_exact(2)
            .map(|c| i32::from(i16::from_le_bytes([c[0], c[1]])))
            .collect();
        let mut out = Vec::with_capacity(samples.len() * self.to.bytes());
        // Bit-perfect only ever opens a CD at its own format or at a wider
        // integer one, and sixteen bits widened cannot be truncated.
        pack(&Samples::Int { data: &samples, bits: 16 }, self.to, &mut out)
            .expect("a CD is only ever widened");
        Ok(Some(out))
    }
}

/// Plays a track off the disc.
pub fn play_track<F, K>(
    drive: &mut dyn CdDevice,
    track: &Track,
    caps: &DeviceCaps,
    stop: &StopSignal,
    pause: &PauseSignal,
    open_sink: F,
    on_progress: &mut dyn FnMut(u64),
) -> Result<CdPlayed, CdPlayError>
where
    F: FnOnce(StreamFormat) -> Result<K, SinkError>,
    K: AudioSink,
{
    let delivery =
        negotiate(CDDA_FORMAT, caps, Policy::BitPerfect).map_err(|_| CdPlayError::Unsupported)?;
    let mut sink = open_sink(delivery.opened).map_err(CdPlayError::Sink)?;
    let mut reader = TrackReader::new(drive, track);
    let mut laid = LaidOut { reader: &mut reader, to: delivery.opened.sample_format };
    let outcome = render_paused(&mut laid, &mut sink, stop, pause, on_progress);
    let read_error = reader.error.as_ref().map(|e| e.to_string());
    sink.close().map_err(CdPlayError::Sink)?;
    Ok(CdPlayed { delivery, outcome, read_error })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chilpa_audio::format::SampleFormat;
    use chilpa_cd::{TocEntry, SECTOR_BYTES};

    struct Disc {
        fail_at: Option<u32>,
    }

    impl CdDevice for Disc {
        fn read_toc(&mut self) -> Result<(Vec<TocEntry>, u32), CdError> {
            Ok((
                vec![
                    TocEntry { number: 1, start_lsn: 0, is_audio: true, session: 1 },
                    TocEntry { number: 2, start_lsn: 100, is_audio: true, session: 1 },
                ],
                200,
            ))
        }
        fn read_audio(&mut self, lsn: u32, sectors: u32, out: &mut Vec<u8>) -> Result<(), CdError> {
            if self.fail_at == Some(lsn) {
                return Err(CdError::Read { lsn, why: "C2 error".into() });
            }
            out.clear();
            out.resize(sectors as usize * SECTOR_BYTES, 0);
            Ok(())
        }
    }

    struct NullSink(StreamFormat, u64);

    impl AudioSink for &mut NullSink {
        fn format(&self) -> StreamFormat {
            self.0
        }
        fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
            let n = frames.len() / self.0.frame_bytes();
            self.1 += n as u64;
            Ok(n)
        }
        fn close(&mut self) -> Result<(), SinkError> {
            Ok(())
        }
    }

    fn caps() -> DeviceCaps {
        DeviceCaps {
            sample_rates: vec![44_100],
            formats: vec![SampleFormat::S16],
            channels: vec![2],
            probed: true,
        }
    }

    #[test]
    fn a_track_off_the_disc_comes_out_whole_and_bit_perfect() {
        let mut disc = Disc { fail_at: None };
        let toc = read_toc(&mut disc).unwrap();
        let track = *toc.track(1).unwrap();
        let mut sink = NullSink(CDDA_FORMAT, 0);
        let played = play_track(
            &mut disc,
            &track,
            &caps(),
            &StopSignal::new(),
            &PauseSignal::new(),
            |_| Ok(&mut sink),
            &mut |_| {},
        )
        .unwrap();

        assert!(played.delivery.nothing_was_converted());
        assert!(played.outcome.may_advance_queue());
        assert!(played.read_error.is_none());
        assert_eq!(played.outcome.frames(), 100 * 588, "the track's 100 sectors");
    }

    /// A disc whose every sample can be told apart from its neighbours.
    struct Patterned;

    /// What sector `lsn` carries on the patterned disc.
    fn sector(lsn: u32) -> Vec<u8> {
        (0..SECTOR_BYTES).map(|i| (lsn as usize * 7 + i * 13) as u8).collect()
    }

    impl CdDevice for Patterned {
        fn read_toc(&mut self) -> Result<(Vec<TocEntry>, u32), CdError> {
            Ok((vec![TocEntry { number: 1, start_lsn: 0, is_audio: true, session: 1 }], 10))
        }
        fn read_audio(&mut self, lsn: u32, sectors: u32, out: &mut Vec<u8>) -> Result<(), CdError> {
            out.clear();
            for s in 0..sectors {
                out.extend_from_slice(&sector(lsn + s));
            }
            Ok(())
        }
    }

    /// A device that keeps what it is handed, in the format it was opened
    /// with rather than the one the test expects.
    struct Recorder {
        opened: Option<StreamFormat>,
        bytes: Vec<u8>,
    }

    impl AudioSink for &mut Recorder {
        fn format(&self) -> StreamFormat {
            self.opened.expect("opened")
        }
        fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
            self.bytes.extend_from_slice(frames);
            Ok(frames.len() / self.opened.expect("opened").frame_bytes())
        }
        fn close(&mut self) -> Result<(), SinkError> {
            Ok(())
        }
    }

    #[test]
    fn a_cd_reaches_a_dac_that_only_takes_32_bit_words_with_every_sample_intact() {
        // A USB DAC publishing only `S32_LE`. The disc's 16 bit samples have
        // to arrive laid out in the wider word, not as 16 bit pairs written
        // into a 32 bit stream, which would play at twice the speed as noise.
        let only_32 = DeviceCaps {
            sample_rates: vec![44_100],
            formats: vec![SampleFormat::S24In32, SampleFormat::S32],
            channels: vec![2],
            probed: true,
        };
        let mut disc = Patterned;
        let toc = read_toc(&mut disc).unwrap();
        let track = *toc.track(1).unwrap();
        let mut sink = Recorder { opened: None, bytes: Vec::new() };
        let played = play_track(
            &mut disc,
            &track,
            &only_32,
            &StopSignal::new(),
            &PauseSignal::new(),
            |format| {
                sink.opened = Some(format);
                Ok(&mut sink)
            },
            &mut |_| {},
        )
        .expect("a CD plays on a DAC that takes wider words");

        assert!(played.delivery.nothing_was_converted());
        assert_eq!(played.outcome.frames(), 10 * 588, "the track's 10 sectors, no more, no fewer");
        let original: Vec<i16> = (0..10)
            .flat_map(sector)
            .collect::<Vec<u8>>()
            .chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]))
            .collect();
        let words: Vec<i32> = sink
            .bytes
            .chunks_exact(4)
            .map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        assert_eq!(words.len(), original.len(), "one word per sample");
        for (i, (&word, &sample)) in words.iter().zip(&original).enumerate() {
            assert_eq!(word >> 16, i32::from(sample), "sample {i} is on top of the word");
            assert_eq!(word & 0xffff, 0, "sample {i} has nothing but zeros below it");
        }
    }

    #[test]
    fn an_unreadable_sector_cuts_the_track_and_it_is_said() {
        let mut disc = Disc { fail_at: Some(75) };
        let toc = read_toc(&mut disc).unwrap();
        let track = *toc.track(1).unwrap();
        let mut sink = NullSink(CDDA_FORMAT, 0);
        let played = play_track(
            &mut disc,
            &track,
            &caps(),
            &StopSignal::new(),
            &PauseSignal::new(),
            |_| Ok(&mut sink),
            &mut |_| {},
        )
        .unwrap();
        assert!(played.read_error.is_some(), "the read error is reported");
        assert!(played.outcome.frames() < 100 * 588, "the whole track did not come out");
    }
}
