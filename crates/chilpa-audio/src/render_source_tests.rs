use super::*;
use crate::format::{SampleFormat, StreamFormat};
use crate::tap::Tap;
use crate::sink::SourcePcm;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

struct Device {
    format: StreamFormat,
    accepted: Rc<RefCell<Vec<u8>>>,
    replies: VecDeque<Result<usize, SinkError>>,
    held: u64,
}

impl AudioSink for Device {
    fn format(&self) -> StreamFormat { self.format }
    fn write(&mut self, bytes: &[u8]) -> Result<usize, SinkError> {
        let frames = bytes.len() / self.format.frame_bytes();
        let accepted = self.replies.pop_front().unwrap_or(Ok(frames))?.min(frames);
        self.accepted.borrow_mut().extend_from_slice(&bytes[..accepted * self.format.frame_bytes()]);
        Ok(accepted)
    }
    fn held(&self) -> u64 { self.held }
    fn close(&mut self) -> Result<(), SinkError> { Ok(()) }
}

fn queued(blocks: Vec<(Vec<u8>, bool)>) -> Cushion {
    let cushion = Cushion::new(1024 * 1024);
    {
        let mut ahead = cushion.lock();
        for (block, moved) in blocks {
            ahead.bytes += block.len();
            ahead.blocks.push_back((block, moved.then_some((None, Some(0)))));
        }
        ahead.end = Some(Ok(()));
    }
    cushion
}

fn input(samples: &[f32]) -> Vec<u8> { samples.iter().flat_map(|value| value.to_le_bytes()).collect() }

struct Captured {
    outcome: Outcome,
    source: Vec<f32>,
    output: Vec<u8>,
    output_peak: f32,
    tap_frames: u64,
}

fn run(blocks: Vec<(Vec<u8>, bool)>, volume: f32, replies: Vec<Result<usize, SinkError>>, held: u64, visible: bool) -> Captured {
    let format = StreamFormat::new(SampleFormat::S16, 8_000, 1);
    let source_format = StreamFormat::new(SampleFormat::F32, 8_000, 1);
    let accepted = Rc::new(RefCell::new(Vec::new()));
    let tap = Tap::new();
    tap.set_visualizing(visible);
    let mut sink = tap.wrap(Device { format, accepted: accepted.clone(), replies: replies.into(), held });
    let mut gain = |bytes: &mut [u8]| crate::dsp::volume::scale(bytes, source_format, || volume);
    let outcome = deliver(&queued(blocks), &mut sink, &StopSignal::new(), &PauseSignal::new(), &mut |_| {},
        source_format.frame_bytes(), std::time::Instant::now(), None, Some(&mut gain), Some(source_format));
    let mut source = vec![0.0; 16];
    let rate = tap.visualizer_snapshot(&mut source);
    assert_eq!(rate, if visible { 8_000 } else { 0 });
    let snapshot = tap.snapshot();
    let result = Captured { outcome, source, output: accepted.borrow().clone(),
        output_peak: snapshot.peaks.into_iter().fold(0.0, f32::max), tap_frames: tap.frames() };
    result
}

#[test]
fn file_analysis_is_identical_at_full_quiet_and_muted_volume() {
    let samples = [0.1, 0.2, -0.3, 0.4, -0.5, 0.6, -0.7, 0.8];
    let reference = run(vec![(input(&samples), false)], 1.0, vec![], 0, true);
    assert_eq!(reference.source, samples.into_iter().flat_map(|sample| [sample, sample]).collect::<Vec<_>>());
    for volume in [0.000729, 0.0, 0.970299] {
        let captured = run(vec![(input(&samples), false)], volume, vec![], 0, true);
        assert_eq!(captured.outcome, Outcome::Finished { frames: 8 });
        assert_eq!(captured.source, reference.source, "visuals follow source PCM, including at mute");
        assert_eq!(captured.tap_frames, 8);
        if volume == 0.0 {
            assert!(captured.output.iter().all(|byte| *byte == 0));
            assert_eq!(captured.output_peak, 0.0, "output meters still describe the DAC bytes");
        } else {
            assert!((captured.output_peak - 0.8 * volume).abs() < 0.00004);
        }
        let hidden = run(vec![(input(&samples), false)], volume, vec![], 0, false);
        assert_eq!(captured.output, hidden.output, "visible source analysis never changes delivered audio");
    }
}

#[test]
fn full_partial_and_recoverable_writes_commit_each_source_frame_once() {
    let samples = [0.1, 0.2, -0.3, 0.4, -0.5, 0.6, -0.7, 0.8];
    let full = run(vec![(input(&samples), false)], 0.0, vec![], 0, true);
    let partial = run(vec![(input(&samples), false)], 0.0,
        vec![Ok(0), Err(SinkError::Xrun), Ok(1), Ok(2), Ok(0), Ok(1)], 0, true);
    assert_eq!(partial.outcome, full.outcome);
    assert_eq!(partial.source, full.source, "retrying must neither duplicate nor skip source samples");
    assert_eq!(partial.output, full.output);
    let failed = run(vec![(input(&samples), false)], 0.0, vec![Ok(2), Err(SinkError::DeviceLost)], 0, true);
    assert!(matches!(failed.outcome, Outcome::Failed { frames: 2, error: SinkError::DeviceLost }));
    assert_eq!(failed.tap_frames, 2);
    assert_eq!(&failed.source[12..], &[0.1, 0.1, 0.2, 0.2]);
    assert!(failed.source[..12].iter().all(|value| *value == 0.0), "unaccepted source frames are never shown");
}

#[test]
fn source_history_follows_device_delay_and_discards_audio_before_seek() {
    let samples = [0.1, 0.2, -0.3, 0.4, -0.5, 0.6, -0.7, 0.8];
    let delayed = run(vec![(input(&samples), false)], 0.0, vec![Ok(3), Ok(2)], 3, true);
    assert!(delayed.source[..6].iter().all(|value| *value == 0.0));
    assert_eq!(&delayed.source[6..], &[0.1, 0.1, 0.2, 0.2, -0.3, -0.3, 0.4, 0.4, -0.5, -0.5]);
    let moved = run(vec![(input(&samples), false), (input(&[0.9, -0.9]), true)], 0.0, vec![], 0, true);
    assert_eq!(&moved.source[12..], &[0.9, 0.9, -0.9, -0.9]);
    assert!(moved.source[..12].iter().all(|value| *value == 0.0), "seek cannot reuse the previous source window");
}

#[test]
fn source_level_does_not_jump_when_volume_changes_between_chunks() {
    let format = StreamFormat::new(SampleFormat::S16, 8_000, 1);
    let source_format = StreamFormat::new(SampleFormat::F32, 8_000, 1);
    let tap = Tap::new();
    tap.set_visualizing(true);
    let accepted = Rc::new(RefCell::new(Vec::new()));
    let mut sink = tap.wrap(Device { format, accepted: accepted.clone(), replies: VecDeque::new(), held: 0 });
    let mut step = 0;
    let gains = [1.0, 0.000729, 0.0, 0.970299];
    let mut gain = |bytes: &mut [u8]| {
        crate::dsp::volume::scale(bytes, source_format, || gains[step]);
        step += 1;
    };
    let result = deliver(&queued(vec![(input(&[0.5; 160]), false)]), &mut sink, &StopSignal::new(),
        &PauseSignal::new(), &mut |_| {}, source_format.frame_bytes(), std::time::Instant::now(),
        None, Some(&mut gain), Some(source_format));
    assert_eq!(result, Outcome::Finished { frames: 160 });
    assert_eq!(step, 4, "the normal audio chunk and gain cadence stays unchanged");
    let mut source = [0.0; 320];
    tap.visualizer_snapshot(&mut source);
    assert!(source.iter().all(|sample| *sample == 0.5));
    for (chunk, gain) in accepted.borrow().chunks_exact(80).zip(gains) {
        assert!(chunk.chunks_exact(2).all(|bytes| {
            let sample = f32::from(i16::from_le_bytes([bytes[0], bytes[1]])) / 32768.0;
            (sample - 0.5 * gain).abs() < 0.00004
        }));
    }
}

#[test]
fn hidden_source_history_resumes_empty_and_resets_with_the_output() {
    let format = StreamFormat::new(SampleFormat::F32, 8_000, 1);
    let tap = Tap::new();
    let mut sink = tap.wrap(Device { format, accepted: Rc::default(), replies: VecDeque::new(), held: 0 });
    let mut source = [9.0; 4];
    sink.write_with_source(&[0; 8], Some(SourcePcm { bytes: &input(&[0.2, 0.4]), format })).unwrap();
    assert_eq!(tap.visualizer_snapshot(&mut source), 0);
    tap.set_visualizing(true);
    assert_eq!(tap.visualizer_snapshot(&mut source), 0);
    sink.write_with_source(&[0; 8], Some(SourcePcm { bytes: &input(&[0.2, 0.4]), format })).unwrap();
    assert_eq!(tap.visualizer_snapshot(&mut source), 8_000);
    assert_eq!(source, [0.2, 0.2, 0.4, 0.4]);
    tap.set_visualizing(false);
    tap.set_visualizing(true);
    assert_eq!(tap.visualizer_snapshot(&mut source), 0);
    for reset in [0, 1, 2, 3] {
        sink.write_with_source(&[0; 8], Some(SourcePcm { bytes: &input(&[0.7, 0.9]), format })).unwrap();
        assert_eq!(tap.visualizer_snapshot(&mut source), 8_000);
        match reset {
            0 => sink.flush().unwrap(),
            1 => sink.position_changed(Some(0)),
            2 => sink.suspend().unwrap(),
            _ => sink.close().unwrap(),
        }
        assert_eq!(tap.visualizer_snapshot(&mut source), 0, "old source audio survived reset {reset}");
    }
}

#[test]
fn raw_dsd_and_explicit_dop_keep_their_existing_sidechain() {
    for (format, dop) in [(StreamFormat::new(SampleFormat::DsdU32Be, 88_200, 2), false),
        (StreamFormat::new(SampleFormat::S24In32, 176_400, 2), true)] {
        let tap = Tap::new();
        tap.set_visualizing(true);
        let accepted = Rc::new(RefCell::new(Vec::new()));
        let mut sink = tap.wrap(Device { format, accepted: accepted.clone(), replies: VecDeque::new(), held: 0 });
        sink.set_dop(dop);
        assert!(!sink.wants_source_pcm(), "one-bit carriers must not enter the PCM source branch");
        let bytes = vec![0x69; format.frame_bytes() * 32];
        let result = deliver(&queued(vec![(bytes.clone(), false)]), &mut sink, &StopSignal::new(),
            &PauseSignal::new(), &mut |_| {}, format.frame_bytes(), std::time::Instant::now(), None, None, None);
        assert_eq!(result, Outcome::Finished { frames: 32 });
        assert_eq!(*accepted.borrow(), bytes);
        assert!(tap.snapshot().samples.is_empty());
        assert_eq!(tap.drain_dsd().bytes, bytes);
    }
}

#[test]
fn analysis_uses_one_existing_delay_query_and_never_substitutes_output_for_missing_source() {
    use std::cell::Cell;
    struct Counted { inner: Device, queries: Rc<Cell<usize>> }
    impl AudioSink for Counted {
        fn format(&self) -> StreamFormat { self.inner.format() }
        fn write(&mut self, bytes: &[u8]) -> Result<usize, SinkError> { self.inner.write(bytes) }
        fn close(&mut self) -> Result<(), SinkError> { Ok(()) }
        fn held(&self) -> u64 { self.queries.set(self.queries.get() + 1); 0 }
    }
    let format = StreamFormat::new(SampleFormat::F32, 8_000, 1);
    let queries = Rc::new(Cell::new(0));
    let tap = Tap::new();
    let mut sink = tap.wrap(Counted { inner: Device { format, accepted: Rc::default(), replies: VecDeque::new(), held: 0 }, queries: queries.clone() });
    assert!(!sink.wants_source_pcm());
    tap.set_visualizing(true);
    assert!(sink.wants_source_pcm());
    // Visibility changed after the chunk was prepared: there is no saved source.
    sink.write_with_source(&input(&[0.0001, 0.0002]), None).unwrap();
    assert_eq!(queries.get(), 1);
    let mut source = [9.0; 4];
    assert_eq!(tap.visualizer_snapshot(&mut source), 0);
    sink.write_with_source(&input(&[0.0001, 0.0002]), Some(SourcePcm { bytes: &input(&[0.3, 0.4]), format })).unwrap();
    assert_eq!(queries.get(), 2, "source capture reuses the output tap delay query");
    assert_eq!(tap.visualizer_snapshot(&mut source), 8_000);
    assert_eq!(source, [0.3, 0.3, 0.4, 0.4]);
}

#[test]
fn matching_integer_pcm_without_gain_keeps_full_block_dsp_and_output_bytes() {
    let format = StreamFormat::new(SampleFormat::S16, 8_000, 1);
    // Larger than source scratch: visibility must not split DSP processing.
    let original = 20_000i16.to_le_bytes().repeat(10_000);
    let processor = || {
        let control = crate::dsp::Control::default();
        let mut preset = crate::dsp::Preset::default();
        preset.compressor_threshold_db = -24.0;
        preset.compressor_ratio = 6.0;
        control.set(Some(preset)).unwrap();
        crate::dsp::Processor::new(control, format)
    };
    let mut expected = original.clone();
    processor().process(&mut expected);
    for visible in [false, true] {
        let tap = Tap::new();
        tap.set_visualizing(visible);
        let accepted = Rc::new(RefCell::new(Vec::new()));
        let mut sink = tap.wrap(Device { format, accepted: accepted.clone(), replies: VecDeque::new(), held: 0 });
        let result = deliver(&queued(vec![(original.clone(), false)]), &mut sink, &StopSignal::new(),
            &PauseSignal::new(), &mut |_| {}, format.frame_bytes(), std::time::Instant::now(),
            Some(&mut processor()), None, Some(format));
        assert_eq!(result, Outcome::Finished { frames: 10_000 });
        assert_eq!(*accepted.borrow(), expected, "analysis preserves the original full-block DSP result");
    }
}

#[test]
fn source_pcm_survives_boxed_sink_forwarding() {
    struct SilentDevice;
    impl AudioSink for SilentDevice {
        fn format(&self) -> StreamFormat { StreamFormat::new(SampleFormat::S16, 8_000, 1) }
        fn write(&mut self, bytes: &[u8]) -> Result<usize, SinkError> { Ok(bytes.len() / 2) }
        fn close(&mut self) -> Result<(), SinkError> { Ok(()) }
    }
    let tap = Tap::new();
    tap.set_visualizing(true);
    let mut sink: Box<dyn AudioSink + Send> = Box::new(tap.wrap(SilentDevice));
    assert!(sink.wants_source_pcm());
    let format = StreamFormat::new(SampleFormat::F32, 8_000, 1);
    sink.write_with_source(&[0; 4], Some(SourcePcm { bytes: &input(&[0.4, 0.6]), format })).unwrap();
    let mut out = [0.0; 4];
    assert_eq!(tap.visualizer_snapshot(&mut out), 8_000);
    assert_eq!(out, [0.4, 0.4, 0.6, 0.6]);
}
