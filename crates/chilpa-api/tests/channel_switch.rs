//! Changing channel policy retires the active stream and its preloaded successor.
use chilpa_api::Chilpa;
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

fn silence(directory: &Path, channels: u16) -> String {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../chilpa-decode/tests/fixtures/channels/layout-{channels}.wav"
    ));
    let mut wave = std::fs::read(source).unwrap();
    assert_eq!(&wave[60..64], b"data");
    let size = 12 * 48000 * u32::from(channels) * 2;
    wave.truncate(68);
    wave[4..8].copy_from_slice(&(size + 60).to_le_bytes());
    wave[64..68].copy_from_slice(&size.to_le_bytes());
    wave.resize(68 + size as usize, 0);
    let path = directory.join(format!("silence-{channels}.wav"));
    std::fs::write(&path, wave).unwrap();
    path.to_str().unwrap().into()
}
fn wait_route(api: &mut Chilpa, path: &str, source: u64, output: u64, after: u64) -> Value {
    let limit = Instant::now() + Duration::from_secs(10);
    let mut last = Value::Null;
    while Instant::now() < limit {
        api.poll();
        if let Some((_, document)) = api.audio_output_doc(u64::MAX) {
            last = serde_json::from_str(&document).unwrap();
            let route = &last["now"]["route"];
            if api.now_playing().path == path
                && api.now_playing().playing
                && route["revision"]
                    .as_u64()
                    .is_some_and(|value| value > after)
                && route["delivery"]["requested"]["channels"] == source
                && route["delivery"]["opened"]["channels"] == output
            {
                assert!(
                    api.now_playing().error.is_empty(),
                    "{}",
                    api.now_playing().error
                );
                return route.clone();
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!(
        "missing {source}->{output} route for {path}: {last}; {}",
        api.now_playing().error
    );
}
#[test]
fn switching_while_playing_preserves_position_and_reopens_the_next_track_with_the_new_mode() {
    let directory = tempfile::tempdir().unwrap();
    for (name, path) in chilpa_dirs::profile_environment(directory.path()) {
        std::env::set_var(name, path);
    }
    std::env::set_var("CHILPA_DEV", "1");
    std::env::set_var("CHILPA_CAPTURE_REALTIME", "1");
    let paths = [silence(directory.path(), 6), silence(directory.path(), 8)];
    let mut api = Chilpa::new();
    api.remember("file:capture", false);
    assert_eq!(api.play_tracks(&paths, "file:capture", false).len(), 2);
    assert!(api.audio_output_command("open", ""));
    let first = wait_route(&mut api, &paths[0], 6, 6, 0);
    let limit = Instant::now() + Duration::from_secs(5);
    while api.poll().seconds < 0.5 && Instant::now() < limit {
        std::thread::sleep(Duration::from_millis(20));
    }
    let position = api.now_playing().seconds;
    assert!(position >= 0.5);
    assert!(api.adjust("audio.channelMode", "stereo"));
    let stereo = wait_route(
        &mut api,
        &paths[0],
        6,
        2,
        first["revision"].as_u64().unwrap(),
    );
    assert!(
        api.now_playing().seconds >= position - 0.1,
        "switching must preserve the current position"
    );
    assert!(api.now_playing().seconds < position + 2.0);
    assert!(!api.now_playing().bit_perfect);
    assert_eq!(api.queue_paths(), paths);
    assert!(api.adjust("audio.channelMode", "stereo"));
    assert_eq!(api.now_playing().path, paths[0]);
    api.next_track();
    let next = wait_route(
        &mut api,
        &paths[1],
        8,
        2,
        stereo["revision"].as_u64().unwrap(),
    );
    assert!(api.adjust("audio.channelMode", "original"));
    wait_route(
        &mut api,
        &paths[1],
        8,
        8,
        next["revision"].as_u64().unwrap(),
    );
    assert_eq!(api.queue_paths(), paths);
    api.stop();
    assert!(api.audio_output_command("close", ""));
}
