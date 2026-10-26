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
    fn held(&self) -> u64 { self.sink.held() }
    fn flush(&mut self) -> Result<(),SinkError> { self.sink.flush() }
    fn set_paused(&mut self,paused:bool) -> Result<(),SinkError> { self.sink.set_paused(paused) }
    fn suspend(&mut self) -> Result<(),SinkError> { self.sink.suspend() }
    fn wake(&mut self) -> Result<(),SinkError> { self.sink.wake() }
}
