//! PCM reaches the device processed exactly once, including partial writes.
use chilpa_audio::{dsp::{Preset, Filter, Shape}, format::{SampleFormat, StreamFormat},
    negotiate::{DeviceCaps, Policy, Transform}, sink::{AudioSink, SinkError}};
use chilpa_core::queue_playback::{play_queue_with, Controls, SharedQueue};
use std::{io::Write, sync::{Arc, Mutex}};

struct Keep { samples: Arc<Mutex<Vec<i16>>>, controls: Controls, bypass_at: Option<usize> }
impl AudioSink for Keep {
    fn format(&self) -> StreamFormat { StreamFormat::new(SampleFormat::S16, 48_000, 2) }
    fn write(&mut self, bytes: &[u8]) -> Result<usize, SinkError> {
        // A deliberately partial device write must not run the EQ again.
        let frames = (bytes.len()/4).min(37);
        let mut samples = self.samples.lock().unwrap();
        samples.extend(bytes[..frames*4].chunks_exact(2).map(|b| i16::from_le_bytes(b.try_into().unwrap())));
        if self.bypass_at.is_some_and(|at| samples.len() >= at) { self.controls.dsp.set(None).unwrap(); self.bypass_at = None; }
        Ok(frames)
    }
    fn close(&mut self) -> Result<(), SinkError> { Ok(()) }
}

fn play(preset: Option<Preset>, bypass_at: Option<usize>) -> (Vec<i16>, bool, bool) {
    play_tracks(preset, bypass_at, 1)
}
fn play_tracks(preset: Option<Preset>, bypass_at: Option<usize>, tracks: usize) -> (Vec<i16>, bool, bool) {
    let file = tempfile::NamedTempFile::with_suffix(".wav").unwrap();
    let frames = 24_000u32;
    let mut f = std::fs::File::create(file.path()).unwrap();
    f.write_all(b"RIFF").unwrap(); f.write_all(&(36+frames*4).to_le_bytes()).unwrap(); f.write_all(b"WAVEfmt ").unwrap();
    f.write_all(&16u32.to_le_bytes()).unwrap(); f.write_all(&1u16.to_le_bytes()).unwrap(); f.write_all(&2u16.to_le_bytes()).unwrap();
    f.write_all(&48_000u32.to_le_bytes()).unwrap(); f.write_all(&192_000u32.to_le_bytes()).unwrap(); f.write_all(&4u16.to_le_bytes()).unwrap();
    f.write_all(&16u16.to_le_bytes()).unwrap(); f.write_all(b"data").unwrap(); f.write_all(&(frames*4).to_le_bytes()).unwrap();
    for _ in 0..frames*2 { f.write_all(&10_000i16.to_le_bytes()).unwrap(); } drop(f);
    let controls = Controls::new(); controls.dsp.set(preset).unwrap();
    let caps = DeviceCaps { sample_rates: vec![48_000], formats: vec![SampleFormat::S16], channels: vec![2], probed: true };
    let queue = SharedQueue::new(vec![file.path().into(); tracks]);
    let samples = Arc::new(Mutex::new(Vec::new())); let kept = samples.clone();
    let mut said = false;
    play_queue_with(&queue, &caps, Policy::AllowAnything, &controls,
        |_| Ok(Keep { samples: kept.clone(), controls: controls.clone(), bypass_at }),
        &mut |_, _, delivery| said = delivery.transforms.contains(&Transform::DspApplied), &mut |_, _| {}).unwrap();
    let result = samples.lock().unwrap().clone(); (result, said, controls.dsp.touched())
}

#[test]
fn bypass_and_flat_deliver_identical_pcm_with_partial_writes() {
    for preset in [None, Some(Preset::default())] {
        let (samples, said, touched) = play(preset, None);
        assert_eq!(samples.len(), 48_000); assert!(samples.iter().all(|s| *s == 10_000)); assert!(!said && !touched);
    }
}
#[test]
fn selected_processing_changes_pcm_and_marks_the_delivery() {
    let (samples, said, touched) = play(Some(Preset { preamp_db: -6.0, ..Preset::default() }), None);
    assert_eq!(samples.len(), 48_000); assert!(said && touched);
    assert!(samples[4000..].iter().all(|s| (5010..=5013).contains(s)), "partial writes do not multiply gain twice");
}
#[test]
fn live_bypass_takes_effect_after_read_ahead_without_restarting_the_track() {
    let (samples, said, touched) = play(Some(Preset { preamp_db: -6.0, filters: vec![Filter { shape: Shape::Peak, frequency: 1000.0, gain_db: -2.0, q: 1.0 }], ..Preset::default() }), Some(8000));
    assert!(said && touched); assert_eq!(samples.len(), 48_000);
    assert!(samples[30_000..].iter().all(|s| *s == 10_000));
}

#[test]
fn gapless_tracks_keep_the_processor_and_do_not_repeat_the_startup_fade() {
    let (samples, said, touched) = play_tracks(Some(Preset { preamp_db: -6.0, ..Preset::default() }), None, 2);
    assert_eq!(samples.len(), 96_000); assert!(said && touched);
    assert!(samples[4000..].iter().all(|s| (5010..=5013).contains(s)), "the boundary keeps the same gain and history");
}
