//! Playing an audio CD.
//!
//! A CD is not decoded: its sectors are already 16 bit PCM at 44,100 Hz.
//! That is why this is the shortest path of all: read sectors and deliver
//! them.
//!
//! A disc plays through one opening of the output, its tracks one after
//! another, as a CD player plays it: they are one stream on the disc, and
//! closing the device between two of them put a gap where the disc has
//! none. If a sector cannot be read, its track is cut off there and it is
//! said; it is never padded with silence, and the disc goes on.

use chilpa_audio::format::{SampleFormat, StreamFormat};
use chilpa_audio::negotiate::{negotiate, DeviceCaps, Delivery, Policy};
use chilpa_audio::pack::{pack, Samples};
use chilpa_audio::render::{render_paused, FrameSource, Outcome};
use chilpa_audio::sink::{AudioSink, SinkError, SkipSignal};
use chilpa_audio::seek::SeekSignal;
use chilpa_cd::{CdDevice, CdError, Toc, Track, TrackReader, CDDA_FORMAT};

use crate::queue_playback::{Controls, Progress};
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::Arc;

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
                write!(f, "{}", chilpa_i18n::t_args("device: {}", &[&e.to_string()]))
            }
        }
    }
}

/// Reads whatever disc is in the drive.
pub fn read_toc(drive: &mut dyn CdDevice) -> Result<Toc, CdPlayError> {
    chilpa_cd::read_toc(drive).map_err(CdPlayError::Drive)
}

/// The disc's sectors in the word the output was opened with.
///
/// A sector is 16 bit pairs, and handing it as it is to an output opened at
/// 32 bits reads two samples as one: half as many frames, each one noise,
/// at twice the speed. A DAC that only takes 32 bit words gets each sample on
/// top of the word with zeros below, which keeps its value, through the same
/// packing every decoded file goes through.
///
/// It also listens to the controls a track obeys while it plays. "Next"
/// used to be dropped until the track had ended by itself, and a seek never
/// reached a CD.
struct LaidOut<'r, 'd> {
    reader: &'r mut TrackReader<'d>,
    to: SampleFormat,
    skip: SkipSignal,
    seek: SeekSignal,
    /// True if this track ended because somebody skipped it.
    skipped: bool,
    /// Frames of the next block to drop, to land inside a sector.
    discard: usize,
    /// Set by a seek, until the renderer has heard of it.
    moved: bool,
    /// Position less frames delivered, and frames delivered, as the queue
    /// counts them for a file.
    base: Arc<AtomicI64>,
    delivered: Arc<AtomicU64>,
}

impl FrameSource for LaidOut<'_, '_> {
    fn ended_early(&self) -> bool {
        self.skipped
    }

    fn moved(&mut self) -> bool {
        std::mem::take(&mut self.moved)
    }

    /// A sector that cannot be read ends the track here and is reported by
    /// the reader itself, beside the outcome.
    fn next_block(&mut self) -> Result<Option<Vec<u8>>, String> {
        if self.skip.take() {
            self.skipped = true;
            return Ok(None);
        }
        if let Some(target) = self.seek.take() {
            let frame = chilpa_audio::seek::frames_at(target, CDDA_FORMAT.sample_rate_hz);
            let per_sector = chilpa_cd::FRAMES_PER_SECTOR as u64;
            self.reader.seek_to_sector(u32::try_from(frame / per_sector).unwrap_or(u32::MAX));
            self.discard = (frame % per_sector) as usize;
            self.moved = true;
            let delivered = self.delivered.load(Ordering::Relaxed);
            self.base.store(frame as i64 - delivered as i64, Ordering::Relaxed);
        }
        let Some(mut block) = self.reader.next_block()? else { return Ok(None) };
        if self.discard > 0 {
            let bytes = CDDA_FORMAT.frame_bytes();
            block.drain(..(self.discard * bytes).min(block.len()));
            self.discard = 0;
        }
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

/// Plays `tracks` off the disc, one after another through one opening of
/// the output.
///
/// `on_track` hears of each track as it starts, with how the delivery turned
/// out; `on_progress` where it is; `on_read_error` of a sector that could not
/// be read, which ends that track and not the disc. The outcome is the last
/// track's.
#[allow(clippy::too_many_arguments)]
pub fn play_disc<F, K>(
    drive: &mut dyn CdDevice,
    tracks: &[Track],
    caps: &DeviceCaps,
    controls: &Controls,
    open_sink: F,
    on_track: &mut dyn FnMut(usize, &Delivery),
    on_progress: &mut dyn FnMut(usize, Progress),
    on_read_error: &mut dyn FnMut(usize, String),
) -> Result<Outcome, CdPlayError>
where
    F: FnOnce(StreamFormat) -> Result<K, SinkError>,
    K: AudioSink,
{
    let delivery =
        negotiate(CDDA_FORMAT, caps, Policy::BitPerfect).map_err(|_| CdPlayError::Unsupported)?;
    let mut sink = open_sink(delivery.opened).map_err(CdPlayError::Sink)?;
    let mut last = Outcome::Finished { frames: 0 };
    // Whether the last track ended by itself: then what is still in the
    // device is its end, and it is heard before the device is closed.
    let mut ended_by_itself = false;
    // A skipped track's rest is still in the device, ahead of the next.
    let mut skipped_before = false;
    for (index, track) in tracks.iter().enumerate() {
        if controls.stop.is_stopped() {
            last = Outcome::Stopped { frames: 0 };
            ended_by_itself = false;
            break;
        }
        if skipped_before {
            sink.flush().map_err(CdPlayError::Sink)?;
        }
        on_track(index, &delivery);
        let base = Arc::new(AtomicI64::new(0));
        let delivered = Arc::new(AtomicU64::new(0));
        let mut reader = TrackReader::new(drive, track);
        let (outcome, skipped) = {
            let mut laid = LaidOut {
                reader: &mut reader,
                to: delivery.opened.sample_format,
                skip: controls.skip.clone(),
                seek: controls.seek.clone(),
                skipped: false,
                discard: 0,
                moved: false,
                base: Arc::clone(&base),
                delivered: Arc::clone(&delivered),
            };
            let outcome = render_paused(&mut laid, &mut sink, &controls.stop, &controls.pause, &mut |f| {
                delivered.store(f, Ordering::Relaxed);
                let at = f as i64 + base.load(Ordering::Relaxed);
                on_progress(index, Progress { position: at.max(0) as u64, delivered: f });
            });
            (outcome, laid.skipped)
        };
        if let Some(problem) = reader.error.as_ref() {
            on_read_error(index, problem.to_string());
        }
        ended_by_itself = matches!(outcome, Outcome::Finished { .. }) && !skipped;
        skipped_before = skipped;
        last = outcome;
        if !last.may_advance_queue() {
            break;
        }
    }
    if ended_by_itself {
        sink.drain(&|| controls.stop.is_stopped()).map_err(CdPlayError::Sink)?;
    }
    sink.close().map_err(CdPlayError::Sink)?;
    Ok(last)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chilpa_audio::format::SampleFormat;
    use chilpa_cd::{TocEntry, SECTOR_BYTES};

    /// Two tracks of 100 sectors, silence, and a sector that fails if asked.
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
            if self.fail_at.is_some_and(|at| (lsn..lsn + sectors).contains(&at)) {
                return Err(CdError::Read { lsn, why: "C2 error".into() });
            }
            out.clear();
            out.resize(sectors as usize * SECTOR_BYTES, 0);
            Ok(())
        }
    }

    /// Counts what it is handed and how often it was drained or flushed.
    struct Counting {
        format: StreamFormat,
        frames: u64,
        drains: usize,
        flushes: usize,
    }

    impl AudioSink for &mut Counting {
        fn format(&self) -> StreamFormat {
            self.format
        }
        fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
            let n = frames.len() / self.format.frame_bytes();
            self.frames += n as u64;
            Ok(n)
        }
        fn drain(&mut self, _: &dyn Fn() -> bool) -> Result<(), SinkError> {
            self.drains += 1;
            Ok(())
        }
        fn flush(&mut self) -> Result<(), SinkError> {
            self.flushes += 1;
            Ok(())
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

    /// Plays the whole disc; gives back the tracks started, the frames each
    /// sent, the read errors and the outcome.
    fn play(
        disc: &mut dyn CdDevice,
        sink: &mut Counting,
        controls: &Controls,
        on_progress: &mut dyn FnMut(usize, Progress),
    ) -> (Vec<usize>, Vec<(usize, String)>, Outcome) {
        let toc = read_toc(disc).unwrap();
        let tracks: Vec<Track> = toc.audio_tracks().copied().collect();
        let (mut started, mut errors) = (Vec::new(), Vec::new());
        let outcome = play_disc(
            disc,
            &tracks,
            &caps(),
            controls,
            |format| {
                sink.format = format;
                Ok(&mut *sink)
            },
            &mut |index, delivery| {
                assert!(delivery.nothing_was_converted());
                started.push(index);
            },
            on_progress,
            &mut |index, why| errors.push((index, why)),
        )
        .unwrap();
        (started, errors, outcome)
    }

    fn counting() -> Counting {
        Counting { format: CDDA_FORMAT, frames: 0, drains: 0, flushes: 0 }
    }

    #[test]
    fn a_disc_plays_through_one_opening_and_is_heard_to_its_end() {
        // Opening the device is FnOnce: one opening for the disc is the
        // type's to keep. A track at a time, it was one per track, with the
        // gap of closing and opening between songs the disc runs into.
        let mut sink = counting();
        let (started, errors, outcome) = play(&mut Disc { fail_at: None }, &mut sink, &Controls::new(), &mut |_, _| {});
        assert_eq!(started, [0, 1]);
        assert!(errors.is_empty());
        assert!(outcome.may_advance_queue());
        assert_eq!(sink.frames, 200 * 588, "both tracks' sectors, one after the other");
        assert_eq!(sink.drains, 1, "the end of the disc is waited for, and only the end");
        assert_eq!(sink.flushes, 0, "and nothing is thrown away on the way");
    }

    #[test]
    fn next_moves_to_the_next_track_while_one_plays() {
        // "Next" was dropped until the track ended by itself.
        let controls = Controls::new();
        let asked = controls.skip.clone();
        let mut sink = counting();
        let mut first = Vec::new();
        let (started, _, _) = play(&mut Disc { fail_at: None }, &mut sink, &controls, &mut |index, progress| {
            if index == 0 {
                first.push(progress.delivered);
                asked.skip();
            }
        });
        assert_eq!(started, [0, 1]);
        assert!(first.last().is_some_and(|f| *f < 100 * 588), "the first track stopped short: {:?}", first.last());
        assert_eq!(sink.frames, first.last().unwrap() + 100 * 588, "and the second played whole");
        assert_eq!(sink.flushes, 1, "what was left of the first did not play ahead of the second");
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

    fn record(caps: &DeviceCaps, controls: &Controls, positions: &mut Vec<u64>) -> Recorder {
        let mut disc = Patterned;
        let tracks: Vec<Track> = read_toc(&mut disc).unwrap().audio_tracks().copied().collect();
        let mut sink = Recorder { opened: None, bytes: Vec::new() };
        play_disc(
            &mut disc,
            &tracks,
            caps,
            controls,
            |format| {
                sink.opened = Some(format);
                Ok(&mut sink)
            },
            &mut |_, _| {},
            &mut |_, progress| positions.push(progress.position),
            &mut |_, _| {},
        )
        .expect("the disc plays");
        sink
    }

    #[test]
    fn a_seek_on_a_cd_lands_on_the_sample_asked_for() {
        // A seek never reached a CD. Sector 3 and a hundred frames into it.
        let controls = Controls::new();
        let frame = 3 * 588 + 100;
        controls.seek.request(chilpa_audio::seek::position_of(frame, 44_100));
        let mut positions = Vec::new();
        let sink = record(&caps(), &controls, &mut positions);
        let expected: Vec<u8> = (0..10).flat_map(sector).skip(frame as usize * 4).collect();
        assert_eq!(sink.bytes, expected, "from that sample to the end, and nothing before it");
        assert!(positions.first().is_some_and(|p| *p > frame), "the position counts from the seek: {:?}", positions.first());
        assert_eq!(positions.last(), Some(&(10 * 588)));
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
        let sink = record(&only_32, &Controls::new(), &mut Vec::new());
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
    fn an_unreadable_sector_cuts_its_track_says_so_and_the_disc_goes_on() {
        let mut sink = counting();
        let (started, errors, _) = play(&mut Disc { fail_at: Some(75) }, &mut sink, &Controls::new(), &mut |_, _| {});
        assert_eq!(started, [0, 1], "the second track still played");
        assert_eq!(errors.len(), 1, "the error was reported: {errors:?}");
        assert_eq!(errors[0].0, 0, "against the first track");
        assert!(sink.frames < 200 * 588, "the first track did not come out whole");
    }
}
