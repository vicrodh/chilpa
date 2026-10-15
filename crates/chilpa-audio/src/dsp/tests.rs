use super::*;
use crate::format::{SampleFormat, StreamFormat};

fn preset() -> Preset {
    Preset { filters: vec![Filter { shape: Shape::Peak, frequency: 1000.0, gain_db: 6.0, q: 1.0 }], ..Preset::default() }
}
fn bytes(samples: &[f32]) -> Vec<u8> { samples.iter().flat_map(|s| s.to_le_bytes()).collect() }
fn floats(bytes: &[u8]) -> Vec<f32> { bytes.chunks_exact(4).map(|b| f32::from_le_bytes(b.try_into().unwrap())).collect() }
fn processor(p: Option<Preset>, channels: u16) -> (Control, Processor) {
    let control = Control::default();
    control.set(p).unwrap();
    let engine = Processor::new(control.clone(), StreamFormat::new(SampleFormat::F32, 48_000, channels));
    (control, engine)
}

#[test]
fn bypass_and_identity_preserve_every_packed_bit() {
    for format in [SampleFormat::S16, SampleFormat::S24Packed, SampleFormat::S24In32, SampleFormat::S32, SampleFormat::F32] {
        for preset in [None, Some(Preset::default())] {
            let control = Control::default(); control.set(preset).unwrap();
            let mut engine = Processor::new(control.clone(), StreamFormat::new(format, 48_000, 2));
            let mut data: Vec<_> = (0..format.bytes()*400).map(|i| (i*47) as u8).collect();
            let original = data.clone();
            engine.process(&mut data);
            assert_eq!(data, original, "{format:?}");
            assert!(!control.touched());
        }
    }
}

#[test]
fn raw_native_dsd_is_never_processed() {
    for format in [SampleFormat::DsdU32Be, SampleFormat::DsdU32Le] {
        let control = Control::default(); control.set(Some(preset())).unwrap();
        let mut engine = Processor::new(control.clone(), StreamFormat::new(format, 176_400, 2));
        let mut data = vec![0x69; 8192]; let original = data.clone();
        engine.process(&mut data);
        assert_eq!(data, original); assert!(!control.touched());
    }
}

#[test]
fn peak_filter_matches_its_center_gain() {
    for gain_db in [-12.0, -3.0, 0.0, 6.0, 12.0] {
        let mut filter = biquad::Biquad::new(&Filter { shape: Shape::Peak, frequency: 1000.0, gain_db, q: 1.0 }, 48_000);
        let mut input_power = 0.0; let mut output_power = 0.0;
        for i in 0..48_000 {
            let x = (2.0*std::f64::consts::PI*1000.0*i as f64/48_000.0).sin()*0.1;
            let y = filter.tick(x);
            if i > 24_000 { input_power += x*x; output_power += y*y; }
        }
        let measured = 10.0*(output_power/input_power).log10();
        assert!((measured-gain_db).abs() < 0.01, "{gain_db}: {measured}");
    }
}

#[test]
fn extreme_valid_filters_stay_finite_at_every_supported_rate() {
    for rate in [8000, 44_100, 48_000, 96_000, 192_000, 384_000, 768_000] {
        for shape in [Shape::Peak, Shape::LowShelf, Shape::HighShelf, Shape::HighPass, Shape::LowPass, Shape::Notch] {
            for frequency in [10.0, 1000.0, 24_000.0] {
                for q in [0.2, 12.0] {
                    let mut filter = biquad::Biquad::new(&Filter { shape, frequency, gain_db: 12.0, q }, rate);
                    for i in 0..12_000 { assert!(filter.tick(if i == 0 { 1.0 } else { 0.0 }).is_finite()); }
                }
            }
        }
    }
}

#[test]
fn channels_have_separate_filter_histories_and_crossfeed_is_stereo_only() {
    let (_, mut engine) = processor(Some(preset()), 2);
    let samples: Vec<_> = (0..10_000).flat_map(|i| [if i == 2000 { 0.5 } else { 0.0 }, 0.0]).collect();
    let mut block = bytes(&samples); engine.process(&mut block);
    assert!(floats(&block).chunks_exact(2).all(|frame| frame[1] == 0.0));
    let p = Preset { crossfeed: 0.3, ..Preset::default() };
    let (_, mut engine) = processor(Some(p), 2);
    let mut block = bytes(&vec![0.0; 2000]); engine.process(&mut block);
    let mut block = bytes(&[0.5, 0.0]); engine.process(&mut block);
    assert!(floats(&block)[1] > 0.0);
}

#[test]
fn delay_balance_and_mono_have_the_documented_effect() {
    let p = Preset { delay_left_ms: 1.0, ..Preset::default() };
    let (_, mut engine) = processor(Some(p), 2);
    engine.process(&mut bytes(&vec![0.0; 2000]));
    let mut input = vec![0.0; 200]; input[0] = 0.5; input[1] = 0.5;
    let mut block = bytes(&input); engine.process(&mut block);
    let output = floats(&block);
    assert_eq!(output[0], 0.0); assert!(output[1] > 0.4); assert!(output[96] > 0.4);
    let (_, mut engine) = processor(Some(Preset { width: 0.0, ..Preset::default() }), 2);
    engine.process(&mut bytes(&vec![0.0; 2000]));
    let mut block = bytes(&[0.5, 0.0]); engine.process(&mut block);
    let output = floats(&block); assert_eq!(output[0], output[1]);
}

#[test]
fn live_bypass_finishes_its_transition_and_returns_to_original_bytes() {
    let (control, mut engine) = processor(Some(preset()), 2);
    engine.process(&mut bytes(&vec![0.5; 4000])); assert!(control.touched());
    control.set(None).unwrap();
    engine.process(&mut bytes(&vec![0.5; 4000]));
    let original = bytes(&vec![0.5; 400]); let mut block = original.clone();
    engine.process(&mut block); assert_eq!(block, original); assert!(!control.active());
    assert!(control.touched(), "a processed track stays processed in its verdict");
}

#[test]
fn invalid_parameters_cannot_replace_a_working_preset() {
    let (control, mut engine) = processor(Some(preset()), 2);
    for bad in [Preset { preamp_db: f64::NAN, ..preset() }, Preset { width: 2.0, ..preset() },
        Preset { filters: vec![preset().filters[0].clone(); 11], ..preset() }] {
        assert!(control.set(Some(bad)).is_err());
    }
    engine.process(&mut bytes(&vec![0.2; 2000])); assert!(control.active());
}

#[test]
fn packed_pcm_round_trips_and_sample_guard_never_wraps() {
    for format in [SampleFormat::S16, SampleFormat::S24Packed, SampleFormat::S24In32, SampleFormat::S32, SampleFormat::F32] {
        for value in [-2.0, -1.0, -0.5, 0.0, 0.5, 1.0, 2.0, f64::NAN] {
            let mut data = vec![0; format.bytes()]; wire::write(&mut data, format, value);
            let out = wire::read(&data, format);
            assert!(out.is_finite() && (-1.0..=1.0).contains(&out));
            if value.is_finite() { assert!((out-value.clamp(-1.0,1.0)).abs() < 0.00004); }
            if format == SampleFormat::S24In32 { assert_eq!(data[0], 0); }
        }
    }
}

#[test]
fn compression_reduces_loud_passages_without_makeup_gain() {
    let (_, mut engine) = processor(Some(Preset { compressor_ratio: 4.0, compressor_threshold_db: -24.0, ..Preset::default() }), 2);
    let mut block = bytes(&vec![0.8; 96_000]); engine.process(&mut block);
    assert!(floats(&block).last().unwrap().abs() < 0.2);
}

#[test]
fn automatic_headroom_follows_the_combined_response_at_the_playback_rate() {
    let p = Preset { filters: vec![
        Filter { shape: Shape::Peak, frequency: 100.0, gain_db: 6.0, q: 3.0 },
        Filter { shape: Shape::Peak, frequency: 10_000.0, gain_db: 6.0, q: 3.0 },
    ], ..Preset::default() };
    let gain = p.preamp_for_rate(48_000);
    assert!((-7.1..=-7.0).contains(&gain), "unrelated boosts need about 7 dB, not 13: {gain}");
    let low_rate = Preset { filters: vec![p.filters[1].clone()], ..Preset::default() };
    assert_eq!(low_rate.preamp_for_rate(8000), -1.0, "an unavailable treble band does not reduce a low-rate stream");
}
