//! Output incompatibility must wait for a choice instead of skipping remote tracks.
use chilpa_audio::{
    format::{SampleFormat, StreamFormat},
    negotiate::{DeviceCaps, Policy},
    sink::{AudioSink, SinkError},
};
use chilpa_core::{
    queue_playback::{Controls, SharedQueue, play_queue_with},
    remote::{Described, Located, Media, RemoteOpener},
};
use chilpa_models::{RemoteEntry, SourceId};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};
static SERIAL: Mutex<()> = Mutex::new(());
struct Cached {
    opened: AtomicUsize,
}
impl RemoteOpener for Cached {
    fn locate(&self, _entry: &RemoteEntry) -> Result<Located, String> {
        self.opened.fetch_add(1, Ordering::Relaxed);
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../chilpa-decode/tests/fixtures/channels/layout-6.wav");
        Ok(Located {
            normalization: None,
            media: Media::File {
                file: std::fs::File::open(path).unwrap(),
                lease: Arc::new(()),
            },
            extension: Some("wav".into()),
            server: "Fixture".into(),
            playback: None,
        })
    }
    fn describe(&self, _entry: &RemoteEntry) -> Described {
        Described::default()
    }
}
struct Silent {
    format: StreamFormat,
}
impl AudioSink for Silent {
    fn format(&self) -> StreamFormat {
        self.format
    }
    fn write(&mut self, bytes: &[u8]) -> Result<usize, SinkError> {
        Ok(bytes.len() / self.format.frame_bytes())
    }
    fn close(&mut self) -> Result<(), SinkError> {
        Ok(())
    }
}
#[test]
fn an_incompatible_output_stops_at_the_remote_entry_that_needs_a_choice() {
    let _serial = SERIAL.lock().unwrap_or_else(|error|error.into_inner());
    let profile = tempfile::tempdir().unwrap();
    for (name, path) in chilpa_dirs::profile_environment(profile.path()) {
        std::env::set_var(name, path);
    }
    let opener = Arc::new(Cached {
        opened: AtomicUsize::new(0),
    });
    chilpa_core::remote::install(opener.clone());
    let entries: Vec<_> = ["first", "second"]
        .into_iter()
        .map(|id| PathBuf::from(RemoteEntry::new(SourceId::new("fixture", "one"), id).to_entry()))
        .collect();
    let queue = SharedQueue::new(entries);
    for channels in [vec![2], vec![8], vec![1]] {
        opener.opened.store(0, Ordering::Relaxed);
        let caps = DeviceCaps {
            sample_rates: vec![48000],
            formats: vec![SampleFormat::F32, SampleFormat::S16],
            channels,
            probed: true,
        };
        let result = play_queue_with(
            &queue,
            &caps,
            Policy::AllowAnything,
            &Controls::default(),
            |format| Ok(Silent { format }),
            &mut |_, _, _| panic!("an incompatible output cannot start without a choice"),
            &mut |_, _| {},
        );
        assert!(result.is_err());
        assert_eq!(
            opener.opened.load(Ordering::Relaxed),
            1,
            "output negotiation must not walk past the track awaiting a conversion choice"
        );
    }
}

#[derive(Default)]
struct Buffered {
    frames: u64,
    drains: usize,
    closes: usize,
}
struct BufferedSink {
    format: StreamFormat,
    state: Arc<Mutex<Buffered>>,
}
impl AudioSink for BufferedSink {
    fn format(&self) -> StreamFormat {
        self.format
    }
    fn write(&mut self, bytes: &[u8]) -> Result<usize, SinkError> {
        let frames = bytes.len() / self.format.frame_bytes();
        self.state.lock().unwrap().frames += frames as u64;
        Ok(frames)
    }
    fn held(&self) -> u64 {
        self.state.lock().unwrap().frames
    }
    fn drain(&mut self, _give_up: &dyn Fn() -> bool) -> Result<(), SinkError> {
        let mut state = self.state.lock().unwrap();
        state.frames = 0;
        state.drains += 1;
        Ok(())
    }
    fn close(&mut self) -> Result<(), SinkError> {
        self.state.lock().unwrap().closes += 1;
        Ok(())
    }
}
#[test]
fn a_conversion_choice_drains_the_previous_track_before_closing_its_sink() {
    use std::io::Write;
    let _serial = SERIAL.lock().unwrap_or_else(|error|error.into_inner());
    let profile = tempfile::tempdir().unwrap();
    for (name, path) in chilpa_dirs::profile_environment(profile.path()) {
        std::env::set_var(name, path);
    }
    let mut wave = std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../chilpa-decode/tests/fixtures/channels/layout-6.wav"),
    )
    .unwrap();
    wave[22..24].copy_from_slice(&2u16.to_le_bytes());
    wave[28..32].copy_from_slice(&192000u32.to_le_bytes());
    wave[32..34].copy_from_slice(&4u16.to_le_bytes());
    wave[40..44].copy_from_slice(&3u32.to_le_bytes());
    wave[4..8].copy_from_slice(&2108u32.to_le_bytes());
    wave[64..68].copy_from_slice(&2048u32.to_le_bytes());
    wave.truncate(68);
    wave.resize(2116, 0);
    let mut stereo = tempfile::NamedTempFile::new().unwrap();
    stereo.write_all(&wave).unwrap();
    chilpa_core::remote::install(Arc::new(Cached {
        opened: AtomicUsize::new(0),
    }));
    let remote =
        PathBuf::from(RemoteEntry::new(SourceId::new("fixture", "one"), "next").to_entry());
    let queue = SharedQueue::new(vec![stereo.path().to_path_buf(), remote]);
    let caps = DeviceCaps {
        sample_rates: vec![48000],
        formats: vec![SampleFormat::F32, SampleFormat::S16],
        channels: vec![2],
        probed: true,
    };
    let state = Arc::new(Mutex::new(Buffered::default()));
    let sink_state = state.clone();
    let result = play_queue_with(
        &queue,
        &caps,
        Policy::AllowAnything,
        &Controls::default(),
        move |format| {
            Ok(BufferedSink {
                format,
                state: sink_state.clone(),
            })
        },
        &mut |_, _, _| {},
        &mut |_, _| {},
    );
    assert!(result.is_err());
    let state = state.lock().unwrap();
    assert_eq!(
        state.drains, 1,
        "the previous track must drain before offering conversion"
    );
    assert_eq!(state.frames, 0);
    assert_eq!(state.closes, 1);
}
