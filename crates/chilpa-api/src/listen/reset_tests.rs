use super::*;
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

#[test]
fn accepting_a_buffer_switch_clears_the_timer_before_the_old_play_settles() {
    let mut api = Chilpa::with_listed(Vec::new(), Vec::new());
    let old = api.listen_create("Old", &["/old.wav".into()]);
    api.install_buffer(&api.buffer_snapshot(old).unwrap());
    api.launched(7001, 0, crate::Relaunch::Track);
    let next = api.listen_create("Next", &["/next.wav".into()]);
    api.sleep_until = Some(Instant::now());
    assert!(api.listen_play(next, ""));
    assert!(!api.plays.is_empty(), "the old play still owes its terminal");
    api.poll();
    assert_eq!(api.listen.pending.map(|pending| pending.0), Some(next), "the expired old timer did not cancel the pending switch");
    assert!(api.sleep_until.is_none());
}

#[test]
fn replacing_restored_rows_stays_stopped_when_no_output_can_launch_them() {
    let mut api = Chilpa::with_listed(Vec::new(), Vec::new());
    api.remember("", false);
    api.device_lock_setting(crate::outputs::device_lock::LOCK, "false");
    let old = api.listen_create("Restored", &["/restored.wav".into()]);
    api.install_buffer(&api.buffer_snapshot(old).unwrap());
    assert!(api.now.playing && api.now.paused && api.plays.is_empty());
    api.replace_queue(vec!["/replacement.wav".into()]);
    api.spawn_from(0, "", false, None, crate::Relaunch::Track);
    assert!(api.plays.is_empty() && api.player.is_none(), "there was no output to launch");
    assert!(!api.now.error.is_empty());
    assert!(!api.now.playing, "replacing restored rows is synchronously stopped");
}

#[test]
fn facade_stop_keeps_plugin_updates_blocked_until_a_running_launch_settles() {
    let mut api = Chilpa::with_listed(Vec::new(), Vec::new());
    let (entered, seen) = channel();
    let (release, released) = channel();
    api.player = Some(chilpa_core::player::Player::spawn(
        Box::new(move |_| {
            entered.send(()).unwrap();
            released.recv_timeout(Duration::from_secs(5)).unwrap();
            chilpa_core::probe_device("file:capture")
        }),
        Box::new(|_, _, _| panic!("a stopped launch must not open an output")),
    ));
    api.launch(chilpa_core::launch::LaunchSpec {
        paths: vec!["/pending.wav".into()], device: "fake".into(), ..Default::default()
    }, 0, crate::Relaunch::Track);
    api.now.playing = true;
    api.now.paused = false;
    seen.recv_timeout(Duration::from_secs(5)).unwrap();
    api.stop();
    let active_while_unsettled = api.plugin_playback_active();
    assert!(!api.now.playing && !api.now.paused && !api.plays.is_empty());
    release.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !api.plays.is_empty() {
        assert!(Instant::now() < deadline, "the stopped launch did not settle");
        api.read_player_events();
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(active_while_unsettled, "Stop must not release the plugin while the worker still uses it");
    assert!(!api.plugin_playback_active());
}
