use super::*;
use chilpa_audio::{format::StreamFormat, sink::{AudioSink, SinkError}};
use chilpa_core::{launch::LaunchSpec, player::Player};
use std::{path::Path, sync::{atomic::{AtomicU64, AtomicUsize, Ordering}, mpsc::{channel, Receiver}}, time::Instant};

#[derive(Default)]
struct Heard(AtomicU64);
impl chilpa_core::playback_events::Observer for Heard {
    fn update(&self, frames: u64, _: u32, _: &'static str, _: Option<&'static str>) {
        self.0.fetch_max(frames, Ordering::AcqRel);
    }
}

struct Output { format: StreamFormat, written: Arc<AtomicU64> }
impl AudioSink for Output {
    fn format(&self) -> StreamFormat { self.format }
    fn write(&mut self, bytes: &[u8]) -> Result<usize, SinkError> {
        let frames = bytes.len() / self.format.frame_bytes();
        self.written.fetch_add(frames as u64, Ordering::Release);
        Ok(frames)
    }
    fn close(&mut self) -> Result<(), SinkError> { Ok(()) }
}

fn player(opened: Arc<Mutex<Vec<String>>>, written: Arc<AtomicU64>, gate: Option<Receiver<()>>) -> Player {
    Player::spawn(
        Box::new(move |_| {
            if let Some(gate) = &gate { gate.recv_timeout(Duration::from_secs(10)).unwrap(); }
            Ok(chilpa_audio::capture::capabilities())
        }),
        Box::new(move |device, format, _| {
            opened.lock().unwrap().push(device.to_string());
            Ok(Box::new(Output { format, written: written.clone() }) as Box<dyn AudioSink + Send>)
        }),
    )
}

fn wave(path: &Path) {
    let frames = 4_410u32;
    let data = frames * 4;
    let mut bytes = b"RIFF".to_vec();
    bytes.extend_from_slice(&(36 + data).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&44_100u32.to_le_bytes());
    bytes.extend_from_slice(&176_400u32.to_le_bytes());
    bytes.extend_from_slice(&4u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data.to_le_bytes());
    bytes.resize(bytes.len() + data as usize, 0);
    std::fs::write(path, bytes).unwrap();
}

fn until(mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ready() {
        assert!(Instant::now() < deadline, "the controlled playback transition did not finish");
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn an_address_resolved_late_is_consumed_by_the_recreated_player_without_marking_its_play() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("resolved.wav");
    wave(&file);
    let mut api = crate::Chilpa::with_listed(Vec::new(), Vec::new());
    api.install_openers();
    let openers = api.installed_openers();
    let entry = RemoteEntry::new(SourceId::new(PROVIDER, "app.example.shows"), "ep-1");
    let path = entry.to_entry();
    let (entered, seen) = channel();
    let (release, released) = channel();
    let released = Mutex::new(released);
    let calls = Arc::new(AtomicUsize::new(0));
    let located_calls = calls.clone();
    let heard = Arc::new(Heard::default());
    let observed = heard.clone();
    let resolved = file.clone();
    let expected = entry.clone();
    *openers.locator.lock().unwrap() = Some(Arc::new(move |entry| {
        assert_eq!(entry, &expected);
        assert_eq!(located_calls.fetch_add(1, Ordering::AcqRel), 0, "the successor must consume the cached result");
        entered.send(()).unwrap();
        released.lock().unwrap().recv_timeout(Duration::from_secs(10)).unwrap();
        Ok(Located {
            normalization: None, playback: Some(observed.clone()), extension: Some("wav".into()), server: String::new(),
            media: chilpa_core::remote::Media::File { file: std::fs::File::open(&resolved).unwrap(), lease: Arc::new(()) },
        })
    }));
    let opened = Arc::new(Mutex::new(Vec::new()));
    let written = Arc::new(AtomicU64::new(0));
    api.player = Some(player(opened.clone(), written.clone(), None));
    api.replace_queue(vec![file.display().to_string()]);
    api.launch(LaunchSpec { paths: vec![file], device: "file:before".into(), paused: true, ..Default::default() }, 0, crate::Relaunch::Track);
    until(|| api.read_player_events());
    let old = api.plays.current().unwrap().id;
    assert_eq!(api.audio_output.route.as_ref().unwrap().device, "file:before");
    openers.resolve_ahead(path.clone());
    seen.recv_timeout(Duration::from_secs(10)).unwrap();
    api.retire_player(crate::plays::Leaving::Drained);
    assert!(api.player.is_none() && api.plays.is_empty());

    let (start, started) = channel();
    api.player = Some(player(opened.clone(), written.clone(), Some(started)));
    api.replace_queue(vec![path.clone()]);
    api.launch(LaunchSpec { paths: vec![path.clone().into()], device: "file:after".into(), paused: true, ..Default::default() }, 0, crate::Relaunch::Track);
    let new = api.plays.current().unwrap().id;
    assert_ne!(new, old);
    api.poll();
    let before = api.now.clone();
    release.send(()).unwrap();
    until(|| openers.ahead.lock().unwrap().contains_key(&path));
    api.poll();
    assert_eq!(api.now, before, "a late resolution changes no window state");
    assert!(!api.marked_at(0));
    assert_eq!(api.plays.current().unwrap().id, new);
    assert!(!api.plays.current().unwrap().started);
    assert_eq!(opened.lock().unwrap().as_slice(), ["file:before"]);

    start.send(()).unwrap();
    until(|| api.read_player_events());
    assert!(api.plays.current().unwrap().started, "the successor emitted Started");
    assert_eq!(api.now.path, path);
    assert_eq!(api.audio_output.route.as_ref().unwrap().device, "file:after");
    api.player.as_ref().unwrap().set_paused(false);
    until(|| { api.read_player_events(); api.plays.is_empty() });
    assert_eq!(opened.lock().unwrap().as_slice(), ["file:before", "file:after"]);
    assert_eq!(written.load(Ordering::Acquire), 4_410, "only the successor delivered audio");
    assert_eq!(heard.0.load(Ordering::Acquire), 4_410, "the observer came from the cached resource");
    assert_eq!(calls.load(Ordering::Acquire), 1);
    assert!(openers.take_ahead(&entry).is_none(), "the real opener consumed it once");
    assert!(!api.marked_at(0) && api.now.error.is_empty());
    api.retire_player(crate::plays::Leaving::Drained);
    api.install_openers();
}
