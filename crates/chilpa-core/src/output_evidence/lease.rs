//! I exclude explicit checks from live openings without blocking an audio thread.
use chilpa_audio::{format::StreamFormat,sink::{AudioSink,SinkError}};
use std::sync::atomic::{AtomicUsize,Ordering};
const CHECK: usize = usize::MAX;
static ACTIVE: AtomicUsize = AtomicUsize::new(0);
pub(super) struct Check;
impl Drop for Check { fn drop(&mut self) { ACTIVE.store(0,Ordering::Release); } }
pub(super) fn check() -> Option<Check> {
    ACTIVE.compare_exchange(0,CHECK,Ordering::AcqRel,Ordering::Acquire).ok().map(|_| Check)
}
pub(crate) struct Playback;
impl Drop for Playback { fn drop(&mut self) { ACTIVE.fetch_sub(1,Ordering::Release); } }
pub(crate) fn playback() -> Result<Playback,SinkError> {
    ACTIVE.fetch_update(Ordering::AcqRel,Ordering::Acquire,|n| (n < CHECK-1).then(||n+1))
        .map(|_| Playback).map_err(|_| SinkError::Backend(chilpa_i18n::t("An audio output check is running. Try playback again when it finishes.")))
}
pub(crate) fn wrap(sink:Box<dyn AudioSink+Send>,lease:Playback) -> Box<dyn AudioSink+Send> {
    Box::new(Leased {sink,lease:Some(lease)})
}
struct Leased { sink:Box<dyn AudioSink+Send>,lease:Option<Playback> }
impl AudioSink for Leased {
    fn format(&self) -> StreamFormat { self.sink.format() }
    fn write(&mut self,bytes:&[u8]) -> Result<usize,SinkError> { self.sink.write(bytes) }
    fn close(&mut self) -> Result<(),SinkError> { let result=self.sink.close(); if result.is_ok() { self.lease.take(); } result }
    fn handoff(&mut self) -> Result<(),SinkError> { self.sink.handoff() }
    fn set_dop(&mut self,dop:bool) { self.sink.set_dop(dop); }
    fn position_changed(&mut self,position:Option<u64>) { self.sink.position_changed(position); }
    fn drain(&mut self,give_up:&dyn Fn()->bool) -> Result<(),SinkError> { self.sink.drain(give_up) }
    fn drain_observed(&mut self,give_up:&dyn Fn()->bool,held:&dyn Fn(u64)) -> Result<(),SinkError> { self.sink.drain_observed(give_up,held) }
    fn held(&self) -> u64 { self.sink.held() }
    fn flush(&mut self) -> Result<(),SinkError> { self.sink.flush() }
    fn set_paused(&mut self,paused:bool) -> Result<(),SinkError> { self.sink.set_paused(paused) }
    fn suspend(&mut self) -> Result<(),SinkError> { self.sink.suspend() }
    fn wake(&mut self) -> Result<(),SinkError> { self.sink.wake() }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A device that says what it holds while it drains.
    struct Reporting;

    impl AudioSink for Reporting {
        fn format(&self) -> StreamFormat {
            StreamFormat::new(chilpa_audio::format::SampleFormat::S16, 44_100, 2)
        }
        fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
            Ok(frames.len() / 4)
        }
        fn close(&mut self) -> Result<(), SinkError> {
            Ok(())
        }
        fn drain_observed(&mut self, give_up: &dyn Fn() -> bool, held: &dyn Fn(u64)) -> Result<(), SinkError> {
            held(7);
            let _ = give_up();
            Ok(())
        }
    }

    #[test]
    fn a_leased_sink_hands_on_what_the_device_says_it_holds_while_draining() {
        let mut leased = wrap(Box::new(Reporting), playback().expect("no check is running"));
        let told = std::cell::RefCell::new(Vec::new());
        leased.drain_observed(&|| false, &|held| told.borrow_mut().push(held)).unwrap();
        assert_eq!(*told.borrow(), [7]);
    }
}
