//! The selected channel policy reaches normal and preloaded queue tracks.
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
fn fixture(channels: usize, suffix: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../chilpa-decode/tests/fixtures/channels/layout-{channels}{suffix}"
    ))
}
#[test]
fn original_is_default_and_explicit_stereo_also_applies_to_the_preloaded_next_track() {
    let profile = tempfile::tempdir().unwrap();
    for (name, path) in chilpa_dirs::profile_environment(profile.path()) {
        std::env::set_var(name, path);
    }
    for (stereo,fallback,supported) in [(false,false,true),(true,false,true),(false,true,true),(false,true,false)] {
        let mixed=stereo || !supported;
        let controls = Controls::default();
        assert!(
            !controls.stereo_downmix.get(),
            "default must preserve multichannel output"
        );
        controls.stereo_downmix.set(stereo);
        controls.stereo_fallback.set(fallback);
        let queue = SharedQueue::new(vec![fixture(6, ".wav"), fixture(8, ".wav")]);
        let shared = Rc::new(RefCell::new(Captured::default()));
        let captured = shared.clone();
        let mut deliveries = Vec::new();
        let caps = DeviceCaps {
            sample_rates: vec![48000],
            formats: vec![SampleFormat::S16, SampleFormat::F32],
            channels: if supported { vec![2,6,8] } else { vec![2] },
            probed: true,
        };
        let results = play_queue_with(
            &queue,
            &caps,
            Policy::BitPerfect,
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
        assert_eq!(finished(&results).count(), 2);
        assert_eq!(deliveries.len(), 2);
        for (delivery, source) in deliveries.iter().zip([6, 8]) {
            assert_eq!(delivery.requested.channels, source);
            assert_eq!(delivery.opened.channels, if mixed { 2 } else { source });
            assert_eq!(delivery.nothing_was_converted(), !mixed);
            assert_eq!(
                delivery.transforms.contains(&Transform::Downmixed {
                    from_channels: source,
                    to_channels: 2
                }),
                mixed
            );
        }
        let log = shared.borrow();
        if mixed {
            assert_eq!(
                log.formats.len(),
                1,
                "converted tracks share one stereo stream"
            );
            let actual: Vec<_> = log
                .bytes
                .chunks_exact(4)
                .map(|v| f32::from_le_bytes(v.try_into().unwrap()))
                .collect();
            let expected: Vec<_> = [6, 8]
                .into_iter()
                .flat_map(|channels| std::fs::read(fixture(channels, "-stereo.f32")).unwrap())
                .collect::<Vec<_>>()
                .chunks_exact(4)
                .map(|v| f32::from_le_bytes(v.try_into().unwrap()))
                .collect();
            assert_eq!(actual.len(), 4096 * 2 * 2);
            assert_eq!(actual.len(), expected.len());
            let peak = actual
                .iter()
                .zip(expected)
                .map(|(a, b)| (a - b).abs())
                .fold(0.0f32, f32::max);
            assert!(peak < 0.000001, "stereo signal error {peak}");
        } else {
            assert_eq!(
                log.formats.iter().map(|f| f.channels).collect::<Vec<_>>(),
                [6, 8]
            );
            assert_eq!(log.bytes.len(), 4096 * (6 + 8) * 2);
        }
    }
}
