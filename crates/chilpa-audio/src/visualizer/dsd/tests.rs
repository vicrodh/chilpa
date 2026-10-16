use super::*;
use crate::format::StreamFormat;
use crate::sink::{AudioSink, SinkError};
use crate::visualizer::{Controls, Frame, Producer, Sink, GONIOMETER, OSCILLOSCOPE};
use std::sync::{Arc, Mutex};
use std::time::Instant;

struct Device {
    format: StreamFormat,
    accepted: Arc<Mutex<Vec<u8>>>,
    limit: Option<usize>,
    fail: bool,
}

impl AudioSink for Device {
    fn format(&self) -> StreamFormat { self.format }
    fn write(&mut self, bytes: &[u8]) -> Result<usize, SinkError> {
        if self.fail { return Err(SinkError::DeviceLost); }
        let frames = (bytes.len() / self.format.frame_bytes()).min(self.limit.unwrap_or(usize::MAX));
        self.accepted.lock().unwrap().extend_from_slice(&bytes[..frames * self.format.frame_bytes()]);
        Ok(frames)
    }
    fn close(&mut self) -> Result<(), SinkError> { Ok(()) }
}

fn device(format: StreamFormat) -> Device {
    Device { format, accepted: Arc::default(), limit: None, fail: false }
}

fn encodings(rate: u32) -> Vec<Encoding> {
    [
        (SampleFormat::DsdU32Be, false, 32),
        (SampleFormat::DsdU32Le, false, 32),
        (SampleFormat::S24In32, true, 16),
        (SampleFormat::S32, true, 16),
        (SampleFormat::S24Packed, true, 16),
    ].into_iter().map(|(format, dop, bits)| Encoding {
        format: StreamFormat::new(format, rate / bits, 2), dop,
    }).collect()
}

fn tone(rate: u32, pcm_frames: usize) -> Vec<Vec<u8>> {
    let bytes = pcm_frames * (rate / OUTPUT_RATE / 8) as usize;
    [1000.0, 2300.0].into_iter().map(|frequency| {
        let mut error = 0.0;
        (0..bytes).map(|byte| {
            let mut packed = 0u8;
            for bit in 0..8 {
                let phase = std::f64::consts::TAU * frequency * (byte * 8 + bit) as f64 / rate as f64;
                error += 0.4 * phase.sin();
                let high = error >= 0.0;
                error -= if high { 1.0 } else { -1.0 };
                packed = (packed << 1) | u8::from(high);
            }
            packed
        }).collect()
    }).collect()
}

fn pack(planar: &[Vec<u8>], encoding: Encoding) -> Vec<u8> {
    let per_word = if encoding.dop { 2 } else { 4 };
    let mut bytes = Vec::new();
    for word in 0..planar[0].len() / per_word {
        for channel in planar {
            let source = &channel[word * per_word..(word + 1) * per_word];
            let marker = if word % 2 == 0 { 0x05 } else { 0xfa };
            match (encoding.format.sample_format, encoding.dop) {
                (SampleFormat::DsdU32Be, false) => bytes.extend_from_slice(source),
                (SampleFormat::DsdU32Le, false) => bytes.extend(source.iter().rev()),
                (SampleFormat::S24In32 | SampleFormat::S32, true) => bytes.extend([0, source[1], source[0], marker]),
                (SampleFormat::S24Packed, true) => bytes.extend([source[1], source[0], marker]),
                _ => unreachable!(),
            }
        }
    }
    bytes
}

fn assert_tone(snapshot: &Snapshot) {
    assert_eq!(snapshot.sample_rate_hz, OUTPUT_RATE);
    for (channel, frequency) in [1000.0, 2300.0].into_iter().enumerate() {
        let samples: Vec<f64> = snapshot.samples.chunks_exact(2).skip(512)
            .map(|frame| f64::from(frame[channel])).collect();
        let rms = (samples.iter().map(|sample| sample * sample).sum::<f64>() / samples.len() as f64).sqrt();
        let crossings = samples.windows(2).filter(|pair| pair[0] < 0.0 && pair[1] >= 0.0).count();
        let measured = crossings as f64 * f64::from(OUTPUT_RATE) / samples.len() as f64;
        assert!((rms - 0.4 * f64::from(GAIN) / 2f64.sqrt()).abs() < 0.008, "RMS {rms}");
        assert!((measured - frequency).abs() < 40.0, "frequency {measured}, expected {frequency}");
    }
}

#[test]
fn all_dsd_rates_and_explicit_carriers_preserve_stereo_frequency_and_rms() {
    for multiplier in [1, 2, 4, 8] {
        let rate = 2_822_400 * multiplier;
        let planar = tone(rate, 4096);
        for encoding in encodings(rate) {
            let bytes = pack(&planar, encoding);
            let tap = Tap::new();
            let dev = device(encoding.format);
            let delivered = dev.accepted.clone();
            let mut sink = tap.wrap(dev);
            sink.set_dop(encoding.dop);
            let mut sidechain = Sidechain::default();
            let frame_bytes = encoding.format.frame_bytes();
            // Non-power-of-two writes exercise phase continuity at every stage.
            for chunk in bytes.chunks(251 * frame_bytes) {
                sink.write(chunk).unwrap();
                sidechain.poll(&tap);
            }
            let snapshot = Snapshot { samples: sidechain.history.clone(), channels: 2,
                sample_rate_hz: OUTPUT_RATE, peaks: vec![], rms: vec![] };
            assert_tone(&snapshot);
            assert_eq!(*delivered.lock().unwrap(), bytes);
            assert!(tap.snapshot().samples.is_empty(), "carrier words never become PCM amplitudes");
            assert!(sidechain.poll(&tap).snapshot.is_none(), "old raw windows are consumed once");
        }
    }
}

#[test]
fn idle_and_ultrasonic_one_bit_patterns_do_not_alias_into_pcm() {
    for multiplier in [1, 2, 4, 8] {
        let rate = 2_822_400 * multiplier;
        for pattern in [0x69, 0xaa, 0xcc] {
            let mut decimator = Decimator::new(rate, 2).unwrap();
            let bytes = (rate / OUTPUT_RATE / 8) as usize * 2048;
            let output = decimator.process(&vec![vec![pattern; bytes]; 2]);
            for channel in output {
                let rms = (channel[512..].iter().map(|sample| sample * sample).sum::<f32>() / (channel.len() - 512) as f32).sqrt();
                assert!(rms < 0.001, "{rate} Hz, pattern {pattern:#x}, RMS {rms}");
            }
        }
    }
}

#[test]
fn partial_zero_and_failed_writes_copy_only_accepted_bytes_without_changing_them() {
    for encoding in encodings(2_822_400) {
        let tap = Tap::new();
        let mut dev = device(encoding.format);
        dev.limit = Some(3);
        let accepted = dev.accepted.clone();
        let mut sink = tap.wrap(dev);
        sink.set_dop(encoding.dop);
        let bytes = pack(&tone(2_822_400, 32), encoding);
        assert_eq!(sink.write(&bytes).unwrap(), 3);
        let batch = tap.drain_dsd();
        assert_eq!(batch.bytes, bytes[..3 * encoding.format.frame_bytes()]);
        assert_eq!(batch.bytes, *accepted.lock().unwrap());
        // Independent zero and failed sinks verify that no rejected bytes enter the ring.
        for fail in [false, true] {
            let mut dev = device(encoding.format);
            dev.limit = Some(0);
            dev.fail = fail;
            let mut sink = tap.wrap(dev);
            sink.set_dop(encoding.dop);
            let result = sink.write(&bytes);
            if fail { assert_eq!(result, Err(SinkError::DeviceLost)); }
            else { assert_eq!(result, Ok(0)); }
            assert!(tap.drain_dsd().bytes.is_empty());
        }
    }
}

#[test]
fn seek_content_and_format_changes_reset_filter_and_history() {
    let encoding = encodings(2_822_400)[0];
    let tap = Tap::new();
    let mut sink = tap.wrap(device(encoding.format));
    let mut sidechain = Sidechain::default();
    sink.write(&pack(&tone(2_822_400, 2048), encoding)).unwrap();
    assert!(sidechain.poll(&tap).snapshot.is_some());
    sink.position_changed(Some(100));
    let update = sidechain.poll(&tap);
    assert!(update.reset);
    assert!(sidechain.history.is_empty());
    sink.write(&pack(&vec![vec![0x69; 4096]; 2], encoding)).unwrap();
    let after_seek = sidechain.poll(&tap).snapshot.unwrap();
    let mut fresh = Sidechain::default();
    let other = Tap::new();
    let mut other_sink = other.wrap(device(encoding.format));
    other_sink.write(&pack(&vec![vec![0x69; 4096]; 2], encoding)).unwrap();
    assert_eq!(after_seek, fresh.poll(&other).snapshot.unwrap());
    sink.set_dop(false);
    assert!(sidechain.poll(&tap).reset, "a new track also resets a reused native stream");
    let carrier = encodings(2_822_400)[2];
    let mut sink = tap.wrap(device(carrier.format));
    assert!(!sidechain.poll(&tap).active, "a PCM carrier remains PCM until explicitly tagged");
    sink.set_dop(true);
    let update = sidechain.poll(&tap);
    assert!(update.reset && update.active);
    assert!(sidechain.history.is_empty());
}

#[derive(Default)]
struct Frames(Mutex<Vec<Frame>>);
impl Sink for Frames {
    fn submit(&self, frame: Frame) { self.0.lock().unwrap().push(frame); }
}

#[test]
fn producer_delivers_dsd_to_every_analyzer_and_scope_only_while_active() {
    let encoding = encodings(2_822_400)[2];
    let tap = Tap::new();
    let mut sink = tap.wrap(device(encoding.format));
    sink.set_dop(true);
    let bytes = pack(&tone(2_822_400, 4096), encoding);
    let controls = Controls::new();
    controls.set_scopes(GONIOMETER | OSCILLOSCOPE);
    let frames = Frames::default();
    let mut producer = Producer::new();
    sink.write(&bytes).unwrap();
    producer.produce(&tap, &controls, &frames);
    assert!(frames.0.lock().unwrap().is_empty());
    controls.set_enabled(true);
    producer.produce(&tap, &controls, &frames);
    let delivered = std::mem::take(&mut *frames.0.lock().unwrap());
    assert!(delivered.iter().any(|frame| matches!(frame, Frame::Wave(values) if values.iter().any(|x| x.abs() > 0.1))));
    assert!(delivered.iter().any(|frame| matches!(frame, Frame::Bars(values) if values.iter().any(|x| *x > 0.05))));
    assert!(delivered.iter().any(|frame| matches!(frame, Frame::Energy(values) if values.iter().any(|x| *x > 0.0))));
    assert!(delivered.iter().any(|frame| matches!(frame, Frame::Spectral(values) if values.iter().any(|x| *x > 0.0))));
    assert!(delivered.iter().any(|frame| matches!(frame, Frame::Goniometer { points, .. } if points.iter().any(|x| x.abs() > 0.01))));
    assert!(delivered.iter().any(|frame| matches!(frame, Frame::Oscilloscope(values) if values.iter().any(|x| x.abs() > 0.01))));
    producer.produce(&tap, &controls, &frames);
    assert!(frames.0.lock().unwrap().is_empty(), "no samples means no repeated DSD analysis");
    controls.set_paused(true);
    sink.write(&bytes).unwrap();
    producer.produce(&tap, &controls, &frames);
    assert!(frames.0.lock().unwrap().is_empty());
}

#[test]
fn dsd512_writer_and_worker_cost_are_measured_separately() {
    let encoding = encodings(22_579_200)[0];
    let tap = Tap::new();
    let mut sink = tap.wrap(device(encoding.format));
    let bytes = pack(&tone(22_579_200, 8820), encoding);
    let started = Instant::now();
    sink.write(&bytes).unwrap();
    let write = started.elapsed();
    let started = Instant::now();
    let mut sidechain = Sidechain::default();
    assert!(sidechain.poll(&tap).snapshot.is_some());
    eprintln!("DSD512 100 ms: accepted-byte copy {write:?}; worker FIR {:?}", started.elapsed());
}
