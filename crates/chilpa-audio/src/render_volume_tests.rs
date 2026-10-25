use super::*;
use crate::format::{SampleFormat, StreamFormat};
use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

struct Sink {
    gain: Rc<Cell<f32>>,
    samples: Vec<i16>,
    writes: usize,
    partial: bool,
    flushes: usize,
    largest_write: usize,
}

impl AudioSink for Sink {
    fn format(&self) -> StreamFormat { StreamFormat::new(SampleFormat::S16, 8_000, 1) }
    fn write(&mut self, data: &[u8]) -> Result<usize, SinkError> {
        self.writes += 1;
        self.largest_write = self.largest_write.max(data.len() / 2);
        if self.partial && self.writes == 1 {
            self.gain.set(0.0);
            return Ok(0);
        }
        let count = if self.partial { 1 } else { data.len() / 2 };
        self.samples.extend(data[..count * 2].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])));
        self.gain.set(match self.writes { 1 => 0.0, 2 => 0.5, _ => 1.0 });
        Ok(count)
    }
    fn flush(&mut self) -> Result<(), SinkError> { self.flushes += 1; Ok(()) }
    fn close(&mut self) -> Result<(), SinkError> { Ok(()) }
}

fn queued(partial: bool, seek: bool) -> Sink {
    let cushion = Cushion::new(32_000);
    {
        let mut ahead = cushion.lock();
        for index in 0..4 {
            let block = 10_000i16.to_le_bytes().repeat(if partial { 1 } else { 4_000 });
            ahead.bytes += block.len();
            let moved = (seek && index == 2).then_some((Some(8_000), Some(80_000)));
            ahead.blocks.push_back((block, moved));
        }
        ahead.end = Some(Ok(()));
        assert_eq!(ahead.blocks.len(), 4, "all audio is queued before the volume changes");
    }
    let gain = Rc::new(Cell::new(1.0));
    let mut sink = Sink { gain: gain.clone(), samples: Vec::new(), writes: 0, partial, flushes: 0, largest_write: 0 };
    let outcome = deliver(&cushion, &mut sink, &StopSignal::new(), &PauseSignal::new(), &mut |_| {}, 2,
        Instant::now(), None, Some(&mut |bytes| crate::dsp::volume::scale(bytes, StreamFormat::new(SampleFormat::S16, 8_000, 1), || gain.get())), None);
    assert_eq!(outcome, Outcome::Finished { frames: if partial { 4 } else { 16_000 } });
    sink
}

#[test]
fn a_full_two_second_queue_uses_the_volume_at_consumption() {
    let sink = queued(false, false);
    assert_eq!(sink.largest_write, 40, "prepared audio is bounded to 5 ms at 8 kHz");
    for (block, expected) in sink.samples.chunks_exact(40).zip([10_000, 0, 5_000, 10_000]) {
        assert!(block.iter().all(|sample| *sample == expected));
    }
}

#[test]
fn a_full_device_retries_without_processing_the_same_samples_twice() {
    let sink = queued(true, false);
    assert_eq!(sink.samples, [10_000, 5_000, 10_000, 10_000]);
}

#[test]
fn a_seek_keeps_the_current_gain_and_flushes_the_device() {
    let sink = queued(false, true);
    assert_eq!(sink.flushes, 1);
    assert!(sink.samples[8_000..12_000].iter().all(|sample| *sample == 10_000));
    assert!(sink.samples[12_000..].iter().all(|sample| *sample == 10_000));
}

#[test]
fn gain_precedes_dynamics_and_retries_do_not_advance_either_stage() {
    let cushion = Cushion::new(1600);
    let original = 20_000i16.to_le_bytes().repeat(800);
    {
        let mut ahead = cushion.lock();
        ahead.blocks.push_back((original.clone(), None));
        ahead.bytes = original.len();
        ahead.end = Some(Ok(()));
    }
    let format = StreamFormat::new(SampleFormat::S16, 8_000, 1);
    let processor = || {
        let control = crate::dsp::Control::default();
        let mut preset = crate::dsp::Preset::default();
        preset.compressor_threshold_db = -24.0;
        preset.compressor_ratio = 6.0;
        control.set(Some(preset)).unwrap();
        let mut processor = crate::dsp::Processor::new(control, format);
        processor.process(&mut vec![0; 160]);
        processor
    };
    let mut expected = original.clone();
    crate::dsp::volume::scale(&mut expected, format, || 0.25);
    processor().process(&mut expected);
    let mut wrong_order = original;
    processor().process(&mut wrong_order);
    crate::dsp::volume::scale(&mut wrong_order, format, || 0.25);
    assert_ne!(expected, wrong_order, "compression distinguishes gain before and after DSP");
    let mut sink = Sink { gain: Rc::new(Cell::new(1.0)), samples: Vec::new(), writes: 0, partial: true, flushes: 0, largest_write: 0 };
    let mut applications = 0;
    let outcome = deliver(&cushion, &mut sink, &StopSignal::new(), &PauseSignal::new(), &mut |_| {}, 2,
        Instant::now(), Some(&mut processor()), Some(&mut |bytes| {
            applications += 1;
            crate::dsp::volume::scale(bytes, format, || 0.25);
        }), None);
    assert_eq!(outcome, Outcome::Finished { frames: 800 });
    assert_eq!(applications, 20, "one gain application per 5 ms despite a full device and partial writes");
    let expected: Vec<i16> = expected.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
    assert_eq!(sink.samples, expected, "DSP state advances exactly once per sample");
}

#[test]
fn floating_headroom_survives_until_gain_before_integer_conversion() {
    struct Floats(bool);
    impl FrameSource for Floats {
        fn pcm_format(&self) -> Option<StreamFormat> {
            Some(StreamFormat::new(SampleFormat::F32, 8_000, 1))
        }
        fn next_block(&mut self) -> Result<Option<Vec<u8>>, String> {
            if std::mem::replace(&mut self.0, true) { return Ok(None); }
            Ok(Some([1.5f32, -1.5].into_iter().flat_map(f32::to_le_bytes).collect()))
        }
    }
    let mut sink = Sink { gain: Rc::new(Cell::new(1.0)), samples: Vec::new(), writes: 0,
        partial: true, flushes: 0, largest_write: 0 };
    let outcome = render_with_volume(&mut Floats(false), &mut sink, &StopSignal::new(), &PauseSignal::new(), None,
        Some(&mut |bytes| crate::dsp::volume::scale(bytes, StreamFormat::new(SampleFormat::F32, 8_000, 1), || 0.5)), &mut |_| {});
    assert_eq!(outcome, Outcome::Finished { frames: 2 });
    assert_eq!(sink.samples, [24_576, -24_576], "over-unity floats are attenuated before packing to integer PCM");
}

#[test]
fn queued_integer_precision_survives_gain_narrowing_and_partial_retries() {
    struct NarrowSink { samples: Vec<i16>, gain: Rc<Cell<f32>>, full: bool }
    impl AudioSink for NarrowSink {
        fn format(&self) -> StreamFormat { StreamFormat::new(SampleFormat::S16, 8_000, 1) }
        fn write(&mut self, bytes: &[u8]) -> Result<usize, SinkError> {
            if !std::mem::replace(&mut self.full, true) { return Ok(0); }
            self.samples.push(i16::from_le_bytes(bytes[..2].try_into().unwrap()));
            self.gain.set(if self.samples.len() == 40 { 0.0 } else if self.samples.len() >= 80 { 0.5 } else { self.gain.get() });
            Ok(1)
        }
        fn flush(&mut self) -> Result<(), SinkError> { Ok(()) }
        fn close(&mut self) -> Result<(), SinkError> { Ok(()) }
    }
    let format = StreamFormat::new(SampleFormat::S32, 8_000, 1);
    let cushion = Cushion::new(64_000);
    {
        let mut ahead = cushion.lock();
        let block: Vec<_> = [19_693_568i32, -19_693_568].into_iter().flat_map(i32::to_le_bytes).cycle().take(64_000).collect();
        ahead.bytes = block.len();
        ahead.blocks.push_back((block, None));
        ahead.end = Some(Ok(()));
    }
    let gain = Rc::new(Cell::new(0.5));
    let mut sink = NarrowSink { samples: Vec::new(), gain: gain.clone(), full: false };
    let mut applications = 0;
    let outcome = deliver(&cushion, &mut sink, &StopSignal::new(), &PauseSignal::new(), &mut |_| {}, 4,
        Instant::now(), None, Some(&mut |bytes| {
            applications += 1;
            crate::dsp::volume::scale(bytes, format, || gain.get());
        }), Some(format));
    assert_eq!(outcome, Outcome::Finished { frames: 16_000 });
    assert_eq!(applications, 400, "partial and zero writes never apply gain twice");
    for (index, sample) in sink.samples.into_iter().enumerate() {
        let expected = if (40..80).contains(&index) { 0 } else if index % 2 == 0 { 150 } else { -150 };
        assert_eq!(sample, expected, "original integer precision reaches gain at frame {index}");
    }
}
