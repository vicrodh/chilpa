//! Playing an audio CD.
//!
//! A CD is not decoded: its sectors are already 16 bit PCM at 44,100 Hz.
//! That is why this is the shortest path of all: read sectors and deliver
//! them.
//!
//! The drive is opened once per track. If a sector cannot be read, the
//! track is cut off there and it is said; it is never padded with silence.

use chilpa_audio::format::StreamFormat;
use chilpa_audio::negotiate::{negotiate, DeviceCaps, Delivery, Policy};
use chilpa_audio::render::{render_paused, Outcome};
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
    let mut lector = TrackReader::new(drive, track);
    let outcome = render_paused(&mut lector, &mut sink, stop, pause, on_progress);
    let read_error = lector.error.as_ref().map(|e| e.to_string());
    sink.close().map_err(CdPlayError::Sink)?;
    Ok(CdPlayed { delivery, outcome, read_error })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chilpa_audio::format::SampleFormat;
    use chilpa_cd::{TocEntry, SECTOR_BYTES};

    struct Disco {
        fail_at: Option<u32>,
    }

    impl CdDevice for Disco {
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
                return Err(CdError::Read { lsn, why: "error de C2".into() });
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
        let mut disco = Disco { fail_at: None };
        let toc = read_toc(&mut disco).unwrap();
        let track = *toc.track(1).unwrap();
        let mut sink = NullSink(CDDA_FORMAT, 0);
        let played = play_track(
            &mut disco,
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

    #[test]
    fn an_unreadable_sector_cuts_the_track_and_it_is_said() {
        let mut disco = Disco { fail_at: Some(75) };
        let toc = read_toc(&mut disco).unwrap();
        let track = *toc.track(1).unwrap();
        let mut sink = NullSink(CDDA_FORMAT, 0);
        let played = play_track(
            &mut disco,
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
