//! The loop that carries samples to the device and says how it ended.
//!
//! The point of this module is that **the end of a track has a type**. In
//! QBZ a failed write went down the same path as a natural end, so an
//! unplugged DAC looked like "it is over" and the whole queue skipped
//! through in silence. Here only [`Outcome::Finished`] lets the queue
//! advance; everything else is told to the user.

use crate::sink::{AudioSink, PauseSignal, SinkError, StopSignal};

/// Where the bytes going to the device come from.
pub trait FrameSource {
    /// The next block, already in the device's format. `None` = the track
    /// ended naturally.
    fn next_block(&mut self) -> Option<Vec<u8>>;
}

/// How a track ended.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Outcome {
    /// The source ran out and everything of its was delivered. It is the
    /// ONLY ending that allows moving to the next track.
    Finished { frames: u64 },
    /// Somebody asked to stop.
    Stopped { frames: u64 },
    /// The device failed. The queue does not advance and the user finds out.
    Failed { frames: u64, error: SinkError },
}

impl Outcome {
    pub fn may_advance_queue(&self) -> bool {
        matches!(self, Outcome::Finished { .. })
    }

    pub fn frames(&self) -> u64 {
        match self {
            Outcome::Finished { frames } | Outcome::Stopped { frames } | Outcome::Failed { frames, .. } => *frames,
        }
    }
}

/// How many xruns in a row are tolerated before giving the device up.
const MAX_CONSECUTIVE_XRUNS: u32 = 8;

/// Delivers everything the source gives and returns how it ended.
///
/// It checks the stop signal between blocks, so stopping does not depend on
/// the device answering. It retries partial writes, tolerates a few xruns
/// in a row, and when the device is lost or takes too long it does not fake
/// a natural end.
pub fn render<S, K>(source: &mut S, sink: &mut K, stop: &StopSignal) -> Outcome
where
    S: FrameSource,
    K: AudioSink,
{
    render_with(source, sink, stop, &mut |_| {})
}

/// The same, reporting how many frames have gone out.
///
/// The report comes from the delivery loop, so the position the interface
/// sees is what really went over the wire and not a clock's count. It is
/// called once per accepted write, and never from the system's audio
/// thread.
pub fn render_with<S, K>(
    source: &mut S,
    sink: &mut K,
    stop: &StopSignal,
    on_progress: &mut dyn FnMut(u64),
) -> Outcome
where
    S: FrameSource,
    K: AudioSink,
{
    render_paused(source, sink, stop, &PauseSignal::new(), on_progress)
}

/// The same, minding the pause as well.
///
/// While paused the loop stops writing and waits, and tells the device in
/// case it can really pause. It keeps watching the stop signal, so stopping
/// from a pause returns at once.
pub fn render_paused<S, K>(
    source: &mut S,
    sink: &mut K,
    stop: &StopSignal,
    pause: &PauseSignal,
    on_progress: &mut dyn FnMut(u64),
) -> Outcome
where
    S: FrameSource,
    K: AudioSink,
{
    let frame_bytes = sink.format().frame_bytes();
    let mut frames: u64 = 0;
    let mut xruns = 0;

    while !stop.is_stopped() {
        let Some(block) = source.next_block() else {
            return Outcome::Finished { frames };
        };
        let mut offset = 0;
        let mut warned = false;
        while offset < block.len() {
            if stop.is_stopped() {
                return Outcome::Stopped { frames };
            }
            if pause.is_paused() {
                if !warned {
                    let _ = sink.set_paused(true);
                    warned = true;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
                continue;
            }
            if warned {
                let _ = sink.set_paused(false);
                warned = false;
            }
            match sink.write(&block[offset..]) {
                Ok(0) => break,
                Ok(written) => {
                    xruns = 0;
                    offset += written * frame_bytes;
                    frames += written as u64;
                    on_progress(frames);
                }
                Err(e) if e.is_recoverable() => {
                    xruns += 1;
                    if xruns >= MAX_CONSECUTIVE_XRUNS {
                        return Outcome::Failed { frames, error: SinkError::DeviceLost };
                    }
                }
                Err(error) => return Outcome::Failed { frames, error },
            }
        }
    }
    Outcome::Stopped { frames }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::{SampleFormat, StreamFormat};
    use std::time::Duration;

    #[test]
    fn paused_nothing_is_written_and_resuming_picks_up_where_it_was() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let pause = PauseSignal::new();
        pause.set(true);
        let mut source = Blocks(vec![block(8)]);
        let mut sink = FakeSink::new(100);
        let ready = Arc::new(AtomicBool::new(false));

        std::thread::scope(|s| {
            let pause_thread = pause.clone();
            let ready_thread = Arc::clone(&ready);
            s.spawn(move || {
                // While paused, nothing must go out.
                std::thread::sleep(Duration::from_millis(60));
                ready_thread.store(true, Ordering::Release);
                pause_thread.set(false);
            });
            let out = render_paused(&mut source, &mut sink, &StopSignal::new(), &pause, &mut |_| {});
            assert!(ready.load(Ordering::Acquire), "no escribió antes de que se quitara la pausa");
            assert_eq!(out, Outcome::Finished { frames: 8 });
        });
    }

    #[test]
    fn stopping_from_a_pause_returns_at_once() {
        let pause = PauseSignal::new();
        pause.set(true);
        let stop = StopSignal::new();
        stop.stop();
        let mut source = Blocks(vec![block(8)]);
        let mut sink = FakeSink::new(100);
        let out = render_paused(&mut source, &mut sink, &stop, &pause, &mut |_| {});
        assert_eq!(out, Outcome::Stopped { frames: 0 });
    }

    #[test]
    fn progress_reports_the_frames_that_actually_went_out() {
        let mut source = Blocks(vec![block(10), block(6)]);
        let mut sink = FakeSink::new(4);
        let mut warnings = Vec::new();
        let out = render_with(&mut source, &mut sink, &StopSignal::new(), &mut |f| warnings.push(f));
        assert_eq!(out, Outcome::Finished { frames: 16 });
        assert_eq!(warnings.last(), Some(&16));
        assert!(warnings.windows(2).all(|p| p[0] < p[1]), "el progreso solo avanza");
    }

    struct Blocks(Vec<Vec<u8>>);

    impl FrameSource for Blocks {
        fn next_block(&mut self) -> Option<Vec<u8>> {
            if self.0.is_empty() {
                None
            } else {
                Some(self.0.remove(0))
            }
        }
    }

    /// A fake device: it takes `accept` frames per write and fails on write
    /// number `fail_at` with `error`.
    struct FakeSink {
        accept: usize,
        fail_at: Option<(usize, SinkError)>,
        writes: usize,
        received: usize,
        closed: bool,
    }

    impl FakeSink {
        fn new(accept: usize) -> Self {
            Self { accept, fail_at: None, writes: 0, received: 0, closed: false }
        }

        fn failing(accept: usize, en: usize, error: SinkError) -> Self {
            Self { fail_at: Some((en, error)), ..Self::new(accept) }
        }
    }

    impl AudioSink for FakeSink {
        fn format(&self) -> StreamFormat {
            StreamFormat::new(SampleFormat::S16, 44_100, 2)
        }

        fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
            self.writes += 1;
            if let Some((en, error)) = &self.fail_at {
                if self.writes == *en {
                    return Err(error.clone());
                }
            }
            let fits = (frames.len() / 4).min(self.accept);
            self.received += fits;
            Ok(fits)
        }

        fn close(&mut self) -> Result<(), SinkError> {
            self.closed = true;
            Ok(())
        }
    }

    fn block(frames: usize) -> Vec<u8> {
        vec![0u8; frames * 4]
    }

    #[test]
    fn a_finished_track_lets_the_queue_advance() {
        let mut source = Blocks(vec![block(10), block(10)]);
        let mut sink = FakeSink::new(100);
        let out = render(&mut source, &mut sink, &StopSignal::new());
        assert_eq!(out, Outcome::Finished { frames: 20 });
        assert!(out.may_advance_queue());
    }

    #[test]
    fn partial_writes_are_retried_until_everything_is_delivered() {
        let mut source = Blocks(vec![block(10)]);
        let mut sink = FakeSink::new(3);
        let out = render(&mut source, &mut sink, &StopSignal::new());
        assert_eq!(out, Outcome::Finished { frames: 10 });
        assert_eq!(sink.received, 10);
        assert!(sink.writes > 1, "se necesitaron varias escrituras");
    }

    #[test]
    fn a_lost_device_is_not_mistaken_for_end_of_track() {
        // The finding: in QBZ this advanced the whole queue without a word.
        let mut source = Blocks(vec![block(10), block(10)]);
        let mut sink = FakeSink::failing(100, 1, SinkError::DeviceLost);
        let out = render(&mut source, &mut sink, &StopSignal::new());
        assert_eq!(out, Outcome::Failed { frames: 0, error: SinkError::DeviceLost });
        assert!(!out.may_advance_queue(), "la cola NO avanza");
    }

    #[test]
    fn a_timeout_is_a_failure_too_and_does_not_wait_forever() {
        let mut source = Blocks(vec![block(4)]);
        let wait = Duration::from_millis(200);
        let mut sink = FakeSink::failing(100, 1, SinkError::Timeout(wait));
        let out = render(&mut source, &mut sink, &StopSignal::new());
        assert_eq!(out, Outcome::Failed { frames: 0, error: SinkError::Timeout(wait) });
    }

    #[test]
    fn a_lone_xrun_is_absorbed_and_the_track_finishes() {
        let mut source = Blocks(vec![block(6)]);
        let mut sink = FakeSink::failing(100, 1, SinkError::Xrun);
        assert_eq!(render(&mut source, &mut sink, &StopSignal::new()), Outcome::Finished { frames: 6 });
    }

    #[test]
    fn many_xruns_in_a_row_give_the_device_up_for_lost() {
        struct AlwaysXrun;
        impl AudioSink for AlwaysXrun {
            fn format(&self) -> StreamFormat {
                StreamFormat::new(SampleFormat::S16, 44_100, 2)
            }
            fn write(&mut self, _: &[u8]) -> Result<usize, SinkError> {
                Err(SinkError::Xrun)
            }
            fn close(&mut self) -> Result<(), SinkError> {
                Ok(())
            }
        }
        let mut source = Blocks(vec![block(4)]);
        let out = render(&mut source, &mut AlwaysXrun, &StopSignal::new());
        assert_eq!(out, Outcome::Failed { frames: 0, error: SinkError::DeviceLost });
    }

    #[test]
    fn stopping_does_not_wait_for_the_device() {
        // The signal is already set before starting: the loop sees it
        // between blocks and returns without asking the device anything.
        let stop = StopSignal::new();
        stop.stop();
        let mut source = Blocks(vec![block(10)]);
        let mut sink = FakeSink::new(1);
        assert_eq!(render(&mut source, &mut sink, &stop), Outcome::Stopped { frames: 0 });
        assert_eq!(sink.writes, 0, "no se le escribió nada al dispositivo");
    }

    #[test]
    fn stopping_mid_track_returns_between_writes() {
        struct StopsOnTheThird(StopSignal, usize);
        impl AudioSink for StopsOnTheThird {
            fn format(&self) -> StreamFormat {
                StreamFormat::new(SampleFormat::S16, 44_100, 2)
            }
            fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
                self.1 += 1;
                if self.1 == 3 {
                    self.0.stop();
                }
                Ok((frames.len() / 4).min(2))
            }
            fn close(&mut self) -> Result<(), SinkError> {
                Ok(())
            }
        }
        let stop = StopSignal::new();
        let mut source = Blocks(vec![block(20)]);
        let mut sink = StopsOnTheThird(stop.clone(), 0);
        assert_eq!(render(&mut source, &mut sink, &stop), Outcome::Stopped { frames: 6 });
    }
}
