//! The device as the engine sees it.
//!
//! Two things the audit found broken in QBZ that here are part of the type,
//! not of the programmer's care:
//!
//! 1. **A failed write is not the end of a track.** In QBZ, if the DAC was
//!    unplugged or jammed, the failure looked exactly like "the track
//!    ended", and the whole queue skipped through without a word.
//! 2. **Stopping never waits for the device.** If the endpoint stopped
//!    sending events, the writer blocked forever and `stop()` deadlocked.
//!    Here every wait has a limit and stopping only raises a flag.

use crate::format::StreamFormat;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// What can go wrong writing to the device. None of these is a track
/// ending normally.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum SinkError {
    /// The device vanished: unplugged, powered off, or taken by someone else.
    DeviceLost,
    /// It ran dry and had to be recovered. Playing can continue.
    Xrun,
    /// It did not answer within the limit. Treated as a loss, not as
    /// infinite patience.
    Timeout(Duration),
    /// It was already closed.
    Closed,
    /// Any other backend failure, with its text.
    Backend(String),
}

/// What the user reads. The variant names reached the window as they
/// were -- "DeviceLost", "Backend(\"...\")" -- through `{:?}`.
impl std::fmt::Display for SinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let said = match self {
            SinkError::DeviceLost => chilpa_i18n::t("the device is gone: unplugged, turned off or taken by another program"),
            SinkError::Xrun => chilpa_i18n::t("the device ran out of samples"),
            SinkError::Timeout(waited) => chilpa_i18n::t_args(
                "the device did not answer in {} ms",
                &[&waited.as_millis().to_string()],
            ),
            SinkError::Closed => chilpa_i18n::t("the device was already closed"),
            // Already a sentence, from whoever failed.
            SinkError::Backend(message) => message.clone(),
        };
        write!(f, "{said}")
    }
}

impl SinkError {
    /// True when it is worth writing more.
    pub fn is_recoverable(&self) -> bool {
        matches!(self, SinkError::Xrun)
    }
}

/// An open output device.
pub trait AudioSink {
    /// The format it actually ended up open at.
    fn format(&self) -> StreamFormat;

    /// Writes frames and returns how many it took. It may take fewer: the
    /// caller retries the rest.
    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError>;

    /// Releases the device. It waits neither for the buffer to empty nor for
    /// the device to answer.
    fn close(&mut self) -> Result<(), SinkError>;

    /// Closes the stream while keeping any exclusive-device lease available
    /// for the stream that immediately replaces it. Backends without a
    /// separate lease simply close normally.
    fn handoff(&mut self) -> Result<(), SinkError> {
        self.close()
    }

    /// Waits until what was written has been played, at a natural end, so
    /// the last of a track is heard before the device is closed: a DAC held
    /// two seconds of it, and closing threw them away.
    ///
    /// It returns as soon as `give_up` says so -- a stop, a skip -- and never
    /// waits much longer than what the device can hold. By default there is
    /// nothing held, and it returns at once.
    fn drain(&mut self, give_up: &dyn Fn() -> bool) -> Result<(), SinkError> {
        let _ = give_up;
        Ok(())
    }

    /// Frames written and not played yet: what the device holds. Progress
    /// counts what was heard, and without this it ran ahead of the music by
    /// all of it -- half a second on a DAC, which synced lyrics show. By
    /// default nothing is held.
    fn held(&self) -> u64 {
        0
    }

    /// Throws away what the device holds and has not played yet. After a
    /// seek or a skip that is the music being left, and playing it out first
    /// put half a second of it between the button and what was asked for.
    /// By default nothing is held.
    fn flush(&mut self) -> Result<(), SinkError> {
        Ok(())
    }

    /// Pauses or resumes at the device, when it knows how.
    ///
    /// By default it does nothing: whoever plays stops writing and that is
    /// enough to stop it. A backend that can really pause implements this
    /// and so keeps the device from running dry.
    fn set_paused(&mut self, _paused: bool) -> Result<(), SinkError> {
        Ok(())
    }

    /// Lets the device go, for another program to use, keeping what is
    /// needed to take it again. What it held is thrown away. The player asks
    /// for it after a pause of a few seconds: a DAC held while nothing plays
    /// is a DAC nobody else can use. By default there is nothing to let go.
    fn suspend(&mut self) -> Result<(), SinkError> {
        Ok(())
    }

    /// Takes the device again after `suspend`, at the same format and as
    /// strictly as the first time. An error means it could not be had back
    /// -- someone else holds it now -- and is never a reason to open
    /// another one.
    fn wake(&mut self) -> Result<(), SinkError> {
        Ok(())
    }
}

impl AudioSink for Box<dyn AudioSink + Send> {
    fn suspend(&mut self) -> Result<(), SinkError> {
        (**self).suspend()
    }

    fn wake(&mut self) -> Result<(), SinkError> {
        (**self).wake()
    }

    fn set_paused(&mut self, paused: bool) -> Result<(), SinkError> {
        (**self).set_paused(paused)
    }

    fn format(&self) -> StreamFormat {
        (**self).format()
    }

    fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
        (**self).write(frames)
    }

    fn close(&mut self) -> Result<(), SinkError> {
        (**self).close()
    }

    fn handoff(&mut self) -> Result<(), SinkError> {
        (**self).handoff()
    }

    fn drain(&mut self, give_up: &dyn Fn() -> bool) -> Result<(), SinkError> {
        (**self).drain(give_up)
    }

    fn flush(&mut self) -> Result<(), SinkError> {
        (**self).flush()
    }

    fn held(&self) -> u64 {
        (**self).held()
    }
}

/// The flag that pauses from another thread.
///
/// Pausing does not lose the place: the delivery loop stops writing and
/// waits, also watching the stop signal so it does not sleep through a stop
/// asked for while paused. A pause that lasts lets the device go (see
/// `AudioSink::suspend`) and takes it back on resume, from what was heard.
#[derive(Clone, Debug, Default)]
pub struct PauseSignal(Arc<AtomicBool>);

impl PauseSignal {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&self, paused: bool) {
        self.0.store(paused, Ordering::Release);
    }

    pub fn is_paused(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// The flag that skips to the next track.
///
/// Skipping is NOT stopping: the current track ends as if it had run out
/// on its own, so the queue advances. Stopping halts the whole queue.
#[derive(Clone, Debug, Default)]
pub struct SkipSignal(Arc<AtomicBool>);

impl SkipSignal {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn skip(&self) {
        self.0.store(true, Ordering::Release);
    }

    /// Takes the request, if there is one.
    pub fn take(&self) -> bool {
        self.0.swap(false, Ordering::AcqRel)
    }
}

/// The flag that stops from another thread.
///
/// Stopping does not send a command down a channel that could fill up, nor
/// take the device's lock: it raises this, and the write loop checks it
/// between blocks.
#[derive(Clone, Debug, Default)]
pub struct StopSignal(Arc<AtomicBool>);

impl StopSignal {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn stop(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_stopped(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    /// Clears the flag for the next track. An old stop order must not kill
    /// what starts afterwards.
    pub fn reset(&self) {
        self.0.store(false, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_xrun_lets_playback_continue() {
        assert!(SinkError::Xrun.is_recoverable());
        assert!(!SinkError::DeviceLost.is_recoverable());
        assert!(!SinkError::Timeout(Duration::from_millis(200)).is_recoverable());
    }

    #[test]
    fn a_skip_is_consumed_exactly_once() {
        let s = SkipSignal::new();
        assert!(!s.take());
        s.skip();
        assert!(s.take());
        assert!(!s.take(), "the request has already been served");
    }

    #[test]
    fn the_pause_goes_on_and_comes_off() {
        let p = PauseSignal::new();
        assert!(!p.is_paused());
        p.set(true);
        assert!(p.is_paused());
        p.set(false);
        assert!(!p.is_paused());
    }

    #[test]
    fn clearing_the_flag_leaves_the_next_track_ready() {
        let signal = StopSignal::new();
        signal.stop();
        signal.reset();
        assert!(!signal.is_stopped());
    }

    #[test]
    fn the_stop_signal_is_visible_from_another_thread() {
        let signal = StopSignal::new();
        let copy = signal.clone();
        assert!(!signal.is_stopped());
        std::thread::spawn(move || copy.stop()).join().unwrap();
        assert!(signal.is_stopped());
    }
}
