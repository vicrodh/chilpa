//! Real typed player events, with no hardware, accounts or network.
use super::*;
use chilpa_audio::{
    format::{SampleFormat, StreamFormat},
    negotiate::DeviceCaps,
    sink::{AudioSink, SinkError},
};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Instant,
};
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
fn facade(multichannel: bool) -> (Chilpa, Arc<Mutex<Vec<u16>>>) {
    let formats = Arc::new(Mutex::new(Vec::new()));
    let captured = formats.clone();
    let mut api = Chilpa::with_listed(Vec::new(), Vec::new());
    api.player = Some(chilpa_core::player::Player::spawn(
        Box::new(move |_| {
            Ok(DeviceCaps {
                sample_rates: vec![48000],
                formats: vec![SampleFormat::F32, SampleFormat::S16],
                channels: if multichannel { vec![2, 6, 8] } else { vec![2] },
                probed: true,
            })
        }),
        Box::new(move |_, format, _| {
            captured.lock().unwrap().push(format.channels);
            Ok(Box::new(Silent { format }) as Box<dyn AudioSink + Send>)
        }),
    ));
    (api, formats)
}
fn paths() -> Vec<String> {
    [6, 8]
        .into_iter()
        .map(|channels| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join(format!(
                    "../chilpa-decode/tests/fixtures/channels/layout-{channels}.wav"
                ))
                .to_string_lossy()
                .into_owned()
        })
        .collect()
}
fn wait(api: &mut Chilpa, done: impl Fn(&Chilpa) -> bool) {
    let until = Instant::now() + Duration::from_secs(10);
    while Instant::now() < until {
        api.poll();
        if done(api) {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("channel policy did not settle: {}", api.now.error)
}
fn token(api: &mut Chilpa) -> String {
    wait(api, |api| api.audio_output.channels.pending.is_some());
    serde_json::from_str::<serde_json::Value>(&api.channel_offer_doc()).unwrap()["token"]
        .as_str()
        .unwrap()
        .into()
}
#[test]
fn conversion_requires_a_current_choice_and_remembers_only_the_selected_output() {
    const CHILD: &str = "CHILPA_CHANNEL_OFFER_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let profile = tempfile::tempdir().unwrap();
        let child=std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact","outputs::channel_offer::tests::conversion_requires_a_current_choice_and_remembers_only_the_selected_output","--nocapture"])
            .envs(crate::test_support::profile_environment(profile.path())).env(CHILD,"1").output().unwrap();
        assert!(
            child.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&child.stdout),
            String::from_utf8_lossy(&child.stderr)
        );
        return;
    }
    let paths = paths();
    let output = "file:consent-stereo-a";
    let (mut api, captured) = facade(false);
    api.remember(output, false);
    api.play_tracks(&paths, output, false);
    let first = token(&mut api);
    assert!(captured.lock().unwrap().is_empty());
    assert!(api.now.error.is_empty());
    assert_eq!(api.queue, paths);
    assert!(api.answer_channel_offer(&first, false, false));
    assert!(!api.now.playing);
    api.play_tracks(&paths, output, false);
    let next = token(&mut api);
    assert!(!api.answer_channel_offer(&first, true, true));
    assert!(!api.remembered_stereo_fallback(output));
    assert!(api.answer_channel_offer(&next, true, false));
    wait(&mut api, |api| {
        !api.now.playing && api.plays.current().is_none()
    });
    assert_eq!(*captured.lock().unwrap(), [2]);
    assert!(!api.remembered_stereo_fallback(output));
    assert!(!api.stereo_downmix_enabled());
    api.play_tracks(&paths, output, false);
    let next = token(&mut api);
    assert!(api.answer_channel_offer(&next, true, true));
    wait(&mut api, |api| {
        !api.now.playing && api.plays.current().is_none()
    });
    assert!(api.remembered_stereo_fallback(output));
    drop(api);
    let (mut api, captured) = facade(true);
    api.play_tracks(&paths, output, false);
    wait(&mut api, |api| {
        !api.now.playing && api.plays.current().is_none()
    });
    assert_eq!(
        *captured.lock().unwrap(),
        [6, 8],
        "remembered conversion cannot force stereo on a compatible output"
    );
    api.stop();
    api.remember("file:consent-stereo-b", false);
    drop(api);
    let (mut api, captured) = facade(false);
    api.play_tracks(&paths, "file:consent-stereo-b", false);
    let previous = token(&mut api);
    assert!(captured.lock().unwrap().is_empty());
    api.stop();
    assert!(!api.answer_channel_offer(&previous, true, true));
    api.play_tracks(&paths, "file:consent-stereo-b", false);
    let previous = token(&mut api);
    api.rows_epoch += 1;
    assert!(!api.answer_channel_offer(&previous, true, true));
    assert_eq!(api.channel_offer_doc(), "{}");
    api.play_tracks(&paths, "file:consent-stereo-b", false);
    let previous = token(&mut api);
    api.remember("file:consent-stereo-c", false);
    assert!(!api.answer_channel_offer(&previous, true, true));
    api.remember(output, false);
    assert!(api.adjust(super::super::channel_mode::CHANNEL_FALLBACK, "false"));
    assert!(!api.remembered_stereo_fallback(output));
    api.stop();
    drop(api);
    let (mut api, _) = facade(false);
    let output = "file:consent-removed";
    api.remember(output, false);
    let mut wave = std::fs::read(&paths[0]).unwrap();
    wave[22..24].copy_from_slice(&2u16.to_le_bytes());
    wave[28..32].copy_from_slice(&192000u32.to_le_bytes());
    wave[32..34].copy_from_slice(&4u16.to_le_bytes());
    wave[40..44].copy_from_slice(&3u32.to_le_bytes());
    wave[4..8].copy_from_slice(&2108u32.to_le_bytes());
    wave[64..68].copy_from_slice(&2048u32.to_le_bytes());
    wave.truncate(68);
    wave.resize(2116, 0);
    let stereo = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(stereo.path(), wave).unwrap();
    let mut queue = vec![stereo.path().to_string_lossy().into_owned()];
    queue.extend(paths);
    api.play_tracks(&queue, output, false);
    let current = token(&mut api);
    assert_eq!(api.now.index, 0);
    assert!(api.remove_from_queue(1));
    assert!(
        !api.answer_channel_offer(&current, true, true),
        "a withdrawn occurrence cannot authorize another track"
    );
    assert!(!api.remembered_stereo_fallback(output));
}
