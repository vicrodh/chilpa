//! Opus uses the player's audible timeline, including seeks and held padding.
use chilpa_audio::{
    format::{SampleFormat, StreamFormat},
    negotiate::{DeviceCaps, Policy},
    sink::{AudioSink, SinkError},
};
use chilpa_core::queue_playback::{Controls, SharedQueue, play_queue_with};
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
    time::Duration,
};

#[derive(Clone)]
struct Capture {
    stream: StreamFormat,
    samples: Rc<RefCell<Vec<f32>>>,
    boundary: Rc<RefCell<Option<(u64, usize)>>>,
}
impl AudioSink for Capture {
    fn format(&self) -> StreamFormat {
        self.stream
    }
    fn write(&mut self, bytes: &[u8]) -> Result<usize, SinkError> {
        self.samples.borrow_mut().extend(
            bytes
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes(b.try_into().unwrap())),
        );
        // Let the read-ahead worker observe an in-flight seek.
        std::thread::sleep(Duration::from_millis(1));
        Ok(bytes.len() / self.stream.frame_bytes())
    }
    fn position_changed(&mut self, position: Option<u64>) {
        if let Some(position) = position {
            *self.boundary.borrow_mut() = Some((
                position,
                self.samples.borrow().len() / usize::from(self.stream.channels),
            ));
        }
    }
    fn close(&mut self) -> Result<(), SinkError> {
        Ok(())
    }
}
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../chilpa-decode/tests/fixtures/opus")
        .join(name)
}
fn capture(
    path: PathBuf,
    channels: u16,
    start: Option<u64>,
    during: Option<u64>,
) -> (Vec<f32>, Option<(u64, usize)>) {
    let stream = StreamFormat::new(SampleFormat::F32, 48_000, channels);
    let samples = Rc::new(RefCell::new(Vec::new()));
    let boundary = Rc::new(RefCell::new(None));
    let sink = Capture {
        stream,
        samples: samples.clone(),
        boundary: boundary.clone(),
    };
    let controls = Controls::new();
    if let Some(frame) = start {
        controls
            .seek
            .request(Duration::from_nanos(frame * 1_000_000_000 / 48_000));
    }
    let caps = DeviceCaps {
        sample_rates: vec![48_000],
        formats: vec![SampleFormat::F32],
        channels: vec![channels],
        probed: true,
    };
    let mut requested = false;
    play_queue_with(
        &SharedQueue::new(vec![path]),
        &caps,
        Policy::BitPerfect,
        &controls,
        |_| Ok(sink.clone()),
        &mut |_, _, _| {},
        &mut |_, progress| {
            if let Some(target) = during {
                if !requested && progress.position >= 24_000 {
                    controls
                        .seek
                        .request(Duration::from_nanos(target * 1_000_000_000 / 48_000));
                    requested = true;
                }
            }
        },
    )
    .unwrap();
    let output = samples.borrow().clone();
    let moved = *boundary.borrow();
    (output, moved)
}
fn same(actual: &[f32], expected: &[f32], context: &str) {
    assert_eq!(
        actual.len(),
        expected.len(),
        "{context}: audible sample count"
    );
    let error = actual
        .iter()
        .zip(expected)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0f32, f32::max);
    assert!(error < 0.00002, "{context}: audible sample error {error}");
}
#[test]
fn opus_queue_seeks_deliver_the_requested_audible_frames() {
    let path = fixture("channel-markers.opus");
    let (baseline, _) = capture(path.clone(), 6, None, None);
    assert_eq!(baseline.len(), 96_000 * 6);
    for target in [0u64, 48_000, 72_000] {
        let (actual, _) = capture(path.clone(), 6, Some(target), None);
        same(
            &actual,
            &baseline[target as usize * 6..],
            &format!("startup target={target}"),
        );
    }
    for target in [0u64, 72_000] {
        let (actual, boundary) = capture(path.clone(), 6, None, Some(target));
        // The device receives this notification immediately before new audio,
        // after retiring blocks that were already in flight at the request.
        let (position, boundary) = boundary.expect("the sink acknowledged the in-flight seek");
        assert_eq!(position, target);
        same(
            &actual[boundary * 6..],
            &baseline[target as usize * 6..],
            &format!("in-flight target={target}, boundary={boundary}"),
        );
    }
}
#[test]
fn short_opus_queue_output_matches_the_reference_samples() {
    for (name, channels) in [("short-stereo", 2), ("code3-32", 1), ("code3-48", 1)] {
        let (actual, _) = capture(fixture(&format!("{name}.opus")), channels, None, None);
        let expected: Vec<f32> = std::fs::read(fixture(&format!("{name}.f32")))
            .unwrap()
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
            .collect();
        same(&actual, &expected, name);
    }
}

fn direct_playback(forward: bool) {
    use chilpa_audio::sink::{PauseSignal, StopSignal};
    for (name, channels) in [
        ("short-stereo.opus", 2),
        ("large-pages.opus", 6),
        ("code3-48.opus", 1),
    ] {
        let path = fixture(name);
        let (expected, _) = capture(path.clone(), channels, None, None);
        let stream = StreamFormat::new(SampleFormat::F32, 48_000, channels);
        let caps = DeviceCaps {
            sample_rates: vec![48_000],
            formats: vec![SampleFormat::F32],
            channels: vec![channels],
            probed: true,
        };
        let samples = Rc::new(RefCell::new(Vec::new()));
        let sink = Capture {
            stream,
            samples: samples.clone(),
            boundary: Rc::new(RefCell::new(None)),
        };
        if forward {
            let input = chilpa_stream::HttpStream::new(
                Box::new(std::io::Cursor::new(std::fs::read(&path).unwrap())),
                chilpa_stream::StreamInfo {
                    content_type: Some("audio/ogg".into()),
                    ..Default::default()
                },
                chilpa_stream::NowPlaying::new(),
            );
            chilpa_core::playback::play_stream(
                input,
                &caps,
                Policy::BitPerfect,
                &StopSignal::new(),
                &PauseSignal::default(),
                None,
                |_| Ok(sink.clone()),
                &mut |_| {},
                &mut |_| {},
            )
            .unwrap();
        } else {
            chilpa_core::playback::play_file(
                &path,
                &caps,
                Policy::BitPerfect,
                &StopSignal::new(),
                |_| Ok(sink.clone()),
            )
            .unwrap();
        }
        same(
            &samples.borrow(),
            &expected,
            &format!("{name}: forward={forward}"),
        );
    }
}
#[test]
fn direct_opus_streams_trim_priming_before_packing() {
    direct_playback(true)
}
#[test]
fn direct_opus_files_trim_priming_and_padding_before_packing() {
    direct_playback(false)
}
