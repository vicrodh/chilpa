use super::*;
use std::time::Duration;

#[derive(Clone, Copy)]
enum SourceCall { Moved, Heard, Position, Ended }

struct ChecksLock<'a> {
    cushion: &'a Cushion,
    call: SourceCall,
}

impl ChecksLock<'_> {
    fn check(&self) {
        assert!(self.cushion.ahead.try_lock().is_ok(), "source called with ahead locked");
        self.cushion.finish();
    }
}

impl FrameSource for ChecksLock<'_> {
    fn next_block(&mut self) -> Result<Option<Vec<u8>>, String> {
        Ok(if matches!(self.call, SourceCall::Ended) { None } else { Some(vec![0; 32]) })
    }
    fn moved(&mut self) -> bool {
        if matches!(self.call, SourceCall::Moved) { self.check(); }
        true
    }
    fn heard_at_move(&self) -> Option<u64> {
        if matches!(self.call, SourceCall::Heard) { self.check(); }
        None
    }
    fn position_at_move(&self) -> Option<u64> {
        if matches!(self.call, SourceCall::Position) { self.check(); }
        None
    }
    fn ended_early(&self) -> bool {
        self.check();
        false
    }
}

#[test]
fn source_callbacks_run_without_the_cushion_lock() {
    for call in [SourceCall::Moved, SourceCall::Heard, SourceCall::Position, SourceCall::Ended] {
        let cushion = Cushion::new(32);
        let mut source = ChecksLock { cushion: &cushion, call };
        cushion.read(&mut source, &StopSignal::new());
    }
}

struct PanicsIn(SourceCall);

struct Device;
impl AudioSink for Device {
    fn format(&self) -> crate::format::StreamFormat {
        crate::format::StreamFormat::new(crate::format::SampleFormat::S16, 44_100, 2)
    }
    fn write(&mut self, bytes: &[u8]) -> Result<usize, SinkError> { Ok(bytes.len() / 4) }
    fn close(&mut self) -> Result<(), SinkError> { Ok(()) }
}

impl FrameSource for PanicsIn {
    fn next_block(&mut self) -> Result<Option<Vec<u8>>, String> {
        Ok(if matches!(self.0, SourceCall::Ended) { None } else { Some(vec![0; 32]) })
    }
    fn moved(&mut self) -> bool { panic!("source moved panicked"); }
    fn ended_early(&self) -> bool { panic!("source ended_early panicked"); }
}

fn callback_panic(call: SourceCall) {
    let (done, seen) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(|| {
            render(&mut PanicsIn(call), &mut Device, &StopSignal::new());
        });
        let _ = done.send(result.is_err());
    });
    assert!(seen.recv_timeout(Duration::from_secs(5)).expect("render did not unwind"));
}

#[test]
fn a_reader_that_panics_in_moved_returns_before_the_deadline() {
    callback_panic(SourceCall::Moved);
}

#[test]
fn a_reader_that_panics_in_ended_early_returns_before_the_deadline() {
    callback_panic(SourceCall::Ended);
}
