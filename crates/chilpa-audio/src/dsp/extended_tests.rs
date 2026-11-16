use super::*;
use crate::format::{SampleFormat, StreamFormat};

fn run(preset: Preset, input: &[f32]) -> (Vec<f32>, Control) {
    let control = Control::default();
    control.set(Some(preset)).unwrap();
    let mut processor = Processor::new(control.clone(), StreamFormat::new(SampleFormat::F32, 48_000, 2));
    processor.process(&mut vec![0; 4800 * 8]);
    let mut bytes: Vec<_> = input.iter().flat_map(|sample| sample.to_le_bytes()).collect();
    processor.process(&mut bytes);
    (bytes.chunks_exact(4).map(|word| f32::from_le_bytes(word.try_into().unwrap())).collect(), control)
}

#[test]
fn legacy_presets_have_neutral_optional_stages_and_enabled_filters() {
    let old = r#"{"name":"Legacy","filters":[{"shape":"peak","frequency":1000,"gain_db":0,"q":1}]}"#;
    let preset: Preset = serde_json::from_str(old).unwrap();
    preset.validate().unwrap();
    assert!(preset.is_identity());
    assert!(preset.filters[0].enabled);
    assert_eq!(preset.filters[0].channel, Channel::All);
    let input = [0.25, -0.5, 0.0, 0.125];
    assert_eq!(run(preset, &input).0, input);
}

#[test]
fn independent_tone_preserves_all_correction_filters_and_reserves_headroom() {
    let filters = vec![Filter { frequency: 1000.0, gain_db: -1.0, ..Filter::default() }; MAX_FILTERS];
    let mut preset = Preset { filters: filters.clone(), ..Preset::default() };
    preset.tone.bass_db = 8.0;
    preset.tone.treble_db = 4.0;
    preset.validate().unwrap();
    assert_eq!(preset.filters, filters);
    assert!(preset.response_db(&[25.0], 48_000)[0] > 7.0);
    assert!(preset.preamp_for_rate(48_000) < -8.9);
    preset.eq_enabled = false;
    assert!(preset.response_db(&[25.0], 48_000)[0] > 7.0);
    preset.tone.enabled = false;
    assert!(preset.is_identity());
    let serialized = serde_json::to_string(&preset).unwrap();
    assert_eq!(serde_json::from_str::<Preset>(&serialized).unwrap(), preset);
}

#[test]
fn routed_disabled_and_bandpass_filters_have_the_declared_response() {
    let preset = Preset { filters: vec![Filter { frequency: 1000.0, gain_db: -12.0,
        channel: Channel::Left, ..Filter::default() }], auto_headroom: false, ..Preset::default() };
    assert!((preset.channel_response_db(&[1000.0], 48_000, 0)[0] + 12.0).abs() < 1e-8);
    assert!(preset.channel_response_db(&[1000.0], 48_000, 1)[0].abs() < 1e-8);
    let input: Vec<f32> = (0..48_000).flat_map(|i| {
        let x = (std::f64::consts::TAU * 1000.0 * i as f64 / 48_000.0).sin() as f32 * 0.2;
        [x, x]
    }).collect();
    let (output, _) = run(preset, &input);
    assert!(output.chunks_exact(2).zip(input.chunks_exact(2)).all(|(y, x)| y[1] == x[1]));
    let band = Preset { filters: vec![Filter { shape: Shape::BandPass, ..Filter::default() }], ..Preset::default() };
    assert!(band.response_db(&[1000.0], 48_000)[0].abs() < 1e-8);
    assert!(band.response_db(&[20.0], 48_000)[0] < -20.0);
}

#[test]
fn independent_limiter_caps_transients_and_reports_real_reduction() {
    let preset = Preset { auto_headroom: false, preamp_db: 18.0,
        limiter: Limiter { enabled: true, ceiling_db: -3.0, ..Limiter::default() }, ..Preset::default() };
    let input: Vec<f32> = (0..5000).flat_map(|i| [if i % 71 == 0 { 0.95 } else { 0.02 }; 2]).collect();
    let (output, control) = run(preset, &input);
    let ceiling = 10_f32.powf(-3.0 / 20.0);
    assert!(output.iter().all(|sample| sample.abs() <= ceiling + 1e-6));
    assert_eq!(control.clipped_samples(), 0);
    assert!(control.meters()[3] > 15.0);
}

#[test]
fn expanded_dynamics_validate_and_makeup_changes_the_processed_signal() {
    let mut preset = Preset { auto_headroom: false, compressor_ratio: 1.0, ..Preset::default() };
    preset.dynamics.makeup_db = 6.0;
    preset.validate().unwrap();
    let (output, _) = run(preset.clone(), &vec![0.1; 4000]);
    assert!((output.last().unwrap() - 0.199526).abs() < 1e-5);
    preset.dynamics.attack_ms = 0.0;
    assert!(preset.validate().is_err());
    preset.dynamics.attack_ms = 120.0;
    preset.dynamics.expander_ratio = 10.0;
    preset.dynamics.knee_db = 20.0;
    preset.compressor_ratio = 10.0;
    preset.compressor_threshold_db = -90.0;
    preset.validate().unwrap();
}

#[test]
fn pending_updates_coalesce_and_bypass_retires_processing_banks() {
    let control = Control::default();
    let mut processor = Processor::new(control.clone(), StreamFormat::new(SampleFormat::F32, 48_000, 2));
    for gain in [1.0, 2.0, 3.0] {
        let mut preset = Preset::default(); preset.tone.bass_db = gain;
        control.set(Some(preset)).unwrap();
    }
    processor.process(&mut vec![0; 4800 * 8]);
    assert!(control.active());
    control.set(None).unwrap();
    processor.process(&mut vec![0; 4800 * 8]);
    processor.process(&mut vec![0; 4800 * 8]);
    control.collect();
    assert!(!control.active());
    assert!(control.0.mailbox.lock().unwrap().retired.is_none());
}

#[test]
fn automatic_headroom_covers_surround_channels_without_stereo_cancellation() {
    let mut preset = Preset {filters: vec![
        Filter {frequency:1000.0, gain_db:18.0, ..Filter::default()},
        Filter {frequency:1000.0, gain_db:-18.0, channel:Channel::Left, ..Filter::default()},
        Filter {frequency:1000.0, gain_db:-18.0, channel:Channel::Right, ..Filter::default()},
    ], ..Preset::default()};
    assert!(preset.preamp_for_channels(48_000, 32) <= -18.99);
    assert!(preset.preamp_for_channels(48_000, 2) > -1.01);
    let control = Control::default(); control.set(Some(preset.clone())).unwrap();
    let mut processor = Processor::new(control.clone(), StreamFormat::new(SampleFormat::F32, 48_000, 32));
    let samples: Vec<f32> = (0..6000).flat_map(|index| {
        let sample = (std::f64::consts::TAU * 1000.0 * index as f64 / 48_000.0).sin() as f32 * 0.5;
        [sample;32]
    }).collect();
    let mut block: Vec<_> = samples.iter().flat_map(|sample| sample.to_le_bytes()).collect();
    processor.process(&mut block);
    assert_eq!(control.clipped_samples(), 0);
    assert!(control.applied_preamp_db().unwrap() <= -18.99);
    preset.filters[0].enabled = false;
    assert!(preset.preamp_for_channels(48_000, 32).is_finite());
}

#[test]
fn sample_limiter_caps_startup_and_keeps_the_last_frame_without_warmup() {
    let preset = Preset {auto_headroom:false, limiter:Limiter {enabled:true, ceiling_db:-3.0, ..Limiter::default()}, ..Preset::default()};
    let control = Control::default(); control.set(Some(preset.clone())).unwrap();
    let mut processor = Processor::new(control.clone(), StreamFormat::new(SampleFormat::F32, 48_000, 2));
    let mut samples = vec![0.0_f32;4096]; samples[0] = 0.95; samples[4094] = 0.95; samples[4095] = 0.95;
    let mut bytes: Vec<_> = samples.iter().flat_map(|sample| sample.to_le_bytes()).collect();
    processor.process(&mut bytes);
    let output: Vec<_> = bytes.chunks_exact(4).map(|word| f32::from_le_bytes(word.try_into().unwrap())).collect();
    let ceiling = 10_f32.powf(-3.0 / 20.0);
    assert!(output.iter().all(|sample| sample.abs() <= ceiling + 1e-6));
    assert!(output[0] > 0.7 && output[4095] > 0.7, "neither startup nor EOF may lose an impulse");
    let mut quieter = preset; quieter.limiter.ceiling_db = -12.0;
    control.set(Some(quieter)).unwrap();
    let mut bytes: Vec<_> = [0.95_f32,0.95].iter().flat_map(|sample| sample.to_le_bytes()).collect();
    processor.process(&mut bytes);
    assert!(bytes.chunks_exact(4).all(|word| f32::from_le_bytes(word.try_into().unwrap()) <= 10_f32.powf(-12.0/20.0) + 1e-6));
}

#[test]
fn metadata_and_filter_order_preserve_live_delay_and_dynamics_histories() {
    let mut preset = Preset {auto_headroom:false, delay_left_ms:10.0, delay_right_ms:10.0,
        filters:vec![Filter {frequency:1000.0, gain_db:-1.0, ..Filter::default()}, Filter {frequency:2000.0,gain_db:-1.0,..Filter::default()}],
        compressor_ratio:2.0, limiter:Limiter {enabled:true, ..Limiter::default()}, ..Preset::default()};
    let control = Control::default(); control.set(Some(preset.clone())).unwrap();
    let mut processor = Processor::new(control.clone(), StreamFormat::new(SampleFormat::F32,48_000,2));
    let block = || vec![0.4_f32;8192].into_iter().flat_map(f32::to_le_bytes).collect::<Vec<_>>();
    processor.process(&mut block());
    let mut before = block(); processor.process(&mut before);
    preset.name = "Renamed".into(); preset.filters[0].locked = true; preset.filters[0].color = 3; preset.filters.reverse();
    control.set(Some(preset)).unwrap();
    let mut after = block(); processor.process(&mut after);
    let first = f32::from_le_bytes(before[before.len()-4..].try_into().unwrap());
    assert!(after.chunks_exact(4).all(|word| (f32::from_le_bytes(word.try_into().unwrap()) - first).abs() < 1e-5));
}

#[test]
fn gate_expander_and_sidechain_hpf_process_only_their_declared_stage() {
    let mut preset = Preset {auto_headroom:false, ..Preset::default()};
    preset.dynamics.gate_enabled = true; preset.dynamics.gate_threshold_db = -20.0; preset.dynamics.release_ms = 1.0;
    let (gated, _) = run(preset.clone(), &vec![0.01;4800]);
    assert!(gated.last().unwrap().abs() < 1e-6);
    preset.dynamics.gate_enabled = false; preset.dynamics.expander_ratio = 4.0;
    let (expanded, _) = run(preset.clone(), &vec![0.01;4800]);
    assert!(expanded.last().unwrap().abs() > 0.0 && expanded.last().unwrap().abs() < 1e-4);
    preset.dynamics.expander_ratio = 1.0; preset.dynamics.sidechain_hpf_hz = 100.0;
    preset.compressor_ratio = 4.0; preset.compressor_threshold_db = -24.0;
    let (filtered_detector, _) = run(preset.clone(), &vec![0.5;96_000]);
    preset.dynamics.sidechain_hpf_hz = 0.0;
    let (normal_detector, _) = run(preset, &vec![0.5;96_000]);
    assert!((filtered_detector.last().unwrap() - 0.5).abs() < 1e-5);
    assert!(normal_detector.last().unwrap().abs() < 0.2);
}
