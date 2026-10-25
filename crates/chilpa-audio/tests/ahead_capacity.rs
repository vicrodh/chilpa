//! I measure the real read-ahead queue with a blocked software sink.
use chilpa_audio::{format::{SampleFormat, StreamFormat}, render::{FrameSource, render_with_volume}, sink::{AudioSink, PauseSignal, SinkError, StopSignal}};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::time::Duration;
struct Source { format: StreamFormat, seconds: u64, frames: usize, allocations: mpsc::Sender<(usize, usize)> }
impl FrameSource for Source {
    fn pcm_format(&self) -> Option<StreamFormat> { Some(self.format) }
    fn ahead(&self) -> Duration { Duration::from_secs(self.seconds) }
    fn next_block(&mut self) -> Result<Option<Vec<u8>>, String> {
        let bytes = vec![0; self.frames * self.format.frame_bytes()];
        self.allocations.send((bytes.len(), bytes.capacity())).map_err(|e| e.to_string())?;
        Ok(Some(bytes))
    }
}
struct Sink { format: StreamFormat, released: Arc<(Mutex<bool>, Condvar)> }
impl AudioSink for Sink {
    fn format(&self) -> StreamFormat { self.format }
    fn write(&mut self, bytes: &[u8]) -> Result<usize, SinkError> {
        let (lock, changed) = &*self.released;
        let ready = lock.lock().unwrap();
        drop(changed.wait_timeout_while(ready, Duration::from_secs(10), |ready| !*ready).unwrap());
        Ok(bytes.len() / self.format.frame_bytes())
    }
    fn flush(&mut self) -> Result<(), SinkError> { Ok(()) }
    fn close(&mut self) -> Result<(), SinkError> { Ok(()) }
}
fn held(rate: u32, seconds: u64, block_frames: usize, float: bool) -> usize {
    let format = StreamFormat::new(if float { SampleFormat::F32 } else { SampleFormat::S16 }, rate, 2);
    let (sender, allocations) = mpsc::channel();
    let mut source = Source { format, seconds, frames: block_frames, allocations: sender };
    let released = Arc::new((Mutex::new(false), Condvar::new()));
    let mut sink = Sink { format: StreamFormat::new(SampleFormat::S16, rate, 2), released: released.clone() };
    let stop = StopSignal::new();
    let expected = (rate as usize * seconds as usize).div_ceil(block_frames) + 1;
    let observed = std::thread::scope(|threads| {
        let stop_render = stop.clone();
        let renderer = threads.spawn(move || render_with_volume(&mut source, &mut sink, &stop_render,
            &PauseSignal::new(), None, None, &mut |_| {}));
        let mut rows = Vec::new();
        for _ in 0..expected {
            match allocations.recv_timeout(Duration::from_secs(5)) { Ok(row) => rows.push(row), Err(_) => break }
        }
        let extra = allocations.recv_timeout(Duration::from_millis(100)).ok();
        stop.stop();
        *released.0.lock().unwrap() = true;
        released.1.notify_all();
        renderer.join().unwrap();
        assert_eq!(rows.len(), expected, "one active block plus the full bounded queue");
        assert!(extra.is_none(), "a blocked sink stops the producer after the queue fills");
        rows
    });
    let bytes = observed.iter().map(|row| row.0).sum::<usize>();
    let capacity = observed.iter().map(|row| row.1).sum::<usize>();
    assert_eq!(bytes, expected * block_frames * format.frame_bytes());
    assert_eq!(capacity, bytes, "the synthetic source reserves its exact payload");
    let queued_frames = (expected - 1) * block_frames;
    assert!(queued_frames >= rate as usize * seconds as usize);
    assert!(queued_frames < rate as usize * seconds as usize + block_frames);
    eprintln!("ahead_capacity rate={rate} seconds={seconds} float={float} block_frames={block_frames} active_and_queue_bytes={bytes} capacity={capacity}");
    capacity
}
#[test]
fn float_headroom_preserves_seconds_with_an_explicit_capacity_increment() {
    for rate in [44_100, 192_000, 384_000] {
        for seconds in [2, 10] {
            for block in [1024, 4093] {
                let baseline = held(rate, seconds, block, false);
                let candidate = held(rate, seconds, block, true);
                assert_eq!(candidate, baseline * 2, "F32 doubles S16 payload at equal retained duration");
            }
        }
    }
}
