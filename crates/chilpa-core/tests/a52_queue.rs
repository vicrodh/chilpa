//! Consecutive compressed programs retain the sink and deliver every packet.
//! This does not verify MP4 edit-list trimming or audible gapless boundaries.
use chilpa_audio::{
    format::{SampleFormat, StreamFormat},
    negotiate::{DeviceCaps, Policy, Transform},
    sink::{AudioSink, SinkError},
};
use chilpa_core::queue_playback::{Controls, SharedQueue, finished, play_queue_with};
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
};
#[derive(Default)]
struct Captured {
    formats: Vec<StreamFormat>,
    bytes: Vec<u8>,
}
struct Capture {
    format: StreamFormat,
    shared: Rc<RefCell<Captured>>,
}
impl AudioSink for Capture {
    fn format(&self) -> StreamFormat {
        self.format
    }
    fn write(&mut self, bytes: &[u8]) -> Result<usize, SinkError> {
        let frames = (bytes.len() / self.format.frame_bytes()).min(256);
        self.shared
            .borrow_mut()
            .bytes
            .extend_from_slice(&bytes[..frames * self.format.frame_bytes()]);
        Ok(frames)
    }
    fn close(&mut self) -> Result<(), SinkError> {
        Ok(())
    }
}
fn fixture(codec: &str, extension: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../chilpa-decode/tests/fixtures/a52/{codec}-six-channel.{extension}"
    ))
}
#[test]
fn consecutive_programs_reach_the_sink_with_original_or_stereo_channels() {
    let profile = tempfile::tempdir().unwrap();
    for (name, path) in chilpa_dirs::profile_environment(profile.path()) {
        std::env::set_var(name, path);
    }
    for codec in ["ac3", "eac3"] {
        for stereo in [false, true] {
            let controls = Controls::default();
            controls.stereo_downmix.set(stereo);
            let path = fixture(codec, "m4a");
            let queue = SharedQueue::new(vec![path.clone(), path]);
            let shared = Rc::new(RefCell::new(Captured::default()));
            let captured = shared.clone();
            let caps = DeviceCaps {
                sample_rates: vec![48000],
                formats: vec![SampleFormat::F32],
                channels: vec![2, 6],
                probed: true,
            };
            let mut deliveries = Vec::new();
            let results = play_queue_with(
                &queue,
                &caps,
                Policy::AllowAnything,
                &controls,
                move |format| {
                    captured.borrow_mut().formats.push(format);
                    Ok(Capture {
                        format,
                        shared: captured.clone(),
                    })
                },
                &mut |_, _, delivery| deliveries.push(delivery.clone()),
                &mut |_, _| {},
            )
            .unwrap();
            assert_eq!(finished(&results).count(), 2, "{codec}");
            assert_eq!(deliveries.len(), 2);
            for delivery in deliveries {
                assert_eq!(delivery.requested.channels, 6);
                assert_eq!(delivery.opened.channels, if stereo { 2 } else { 6 });
                assert_eq!(
                    delivery.transforms.contains(&Transform::Downmixed {
                        from_channels: 6,
                        to_channels: 2,
                    }),
                    stereo
                );
            }
            let capture = shared.borrow();
            assert_eq!(
                capture.formats.len(),
                1,
                "{codec}: consecutive programs must retain the sink"
            );
            let mut decoder = chilpa_decode::Decoded::open(&fixture(codec, "m4a")).unwrap();
            if stereo {
                decoder.downmix_to_stereo().unwrap();
            }
            let mut source = Vec::new();
            while let Some(block) = decoder.next_block().unwrap() {
                let chilpa_decode::Block::Float(block) = block else {
                    panic!("lossy source must retain float samples")
                };
                for sample in block {
                    source.extend(sample.to_le_bytes());
                }
            }
            assert_eq!(
                capture.bytes.len(),
                source.len() * 2,
                "{codec}: both final packets must reach the sink"
            );
            for delivered in capture.bytes.chunks_exact(source.len()) {
                assert_eq!(
                    delivered, source,
                    "{codec}, stereo={stereo}: the queue must preserve every source PCM bit"
                );
            }
        }
    }
}
