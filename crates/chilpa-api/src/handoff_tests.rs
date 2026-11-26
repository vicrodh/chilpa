//! The listens of a play across a hand-off, driven by the events as the
//! player says them, each with its play.
//!
//! A play is set up as its launch leaves it and a listen as a track's start
//! leaves it: an event that starts a track carries the route of a real
//! delivery, which these have none of, so a start is the call that event
//! makes for the listen.

use chilpa_core::attempts::How;
use chilpa_core::player::PlayerEvent;
use std::path::{Path, PathBuf};

use crate::listening::{ListenFilter, ListenMetrics};
use crate::plays::{Fate, Replaced};
use crate::{Chilpa, InProgress, Relaunch};

const RATE: u32 = 44_100;

/// A facade with a queue in front of it and nothing playing.
fn facade() -> Chilpa {
    let mut api = Chilpa::with_listed(Vec::new(), Vec::new());
    api.listen.active = 1;
    api
}

/// A facade with `play` playing `path` as its first track, heard from
/// `started_at`: a moment no other test logs under.
fn playing(play: u64, path: &str, started_at: i64) -> Chilpa {
    let mut api = facade();
    api.launched(play, 0, Relaunch::Track);
    open(&mut api, play, path, started_at);
    api
}

/// The first track of `play` started at `started_at`, in front of the window.
fn open(api: &mut Chilpa, play: u64, path: &str, started_at: i64) {
    let listen = InProgress {
        context: Default::default(),
        path: PathBuf::from(path),
        carried_ms: 0,
        started_at,
        sample_rate_hz: RATE,
        bit_depth: Some(16),
        bit_perfect: false,
        duration_ms: 600_000,
        played_frames: 0,
        listen: None,
        scrobbled: false,
    };
    let play = api.plays.get_mut(play).unwrap();
    play.started = true;
    play.ledger.open(1, listen);
    api.now.playing = true;
}

/// The window asks for what plays to be replaced, and the launch goes out.
fn relaunch(api: &mut Chilpa, why: Relaunch, new: u64) {
    api.launched(new, 0, why);
}

/// A track starts in `play`, behind the window or in front of it alike: what
/// its start does to the listens.
fn start(api: &mut Chilpa, play: u64, path: &str) {
    let face = api.face_of(play, Path::new(path), None, None);
    api.listen_started(play, 1, Path::new(path), None, &face, RATE, Some(16), false, Some(seconds(600)), false);
}

fn say(api: &mut Chilpa, play: u64, event: PlayerEvent) {
    api.apply_player_event(play, event, &mut false);
}

fn seconds(count: u64) -> u64 {
    count * u64::from(RATE)
}

fn closed(heard: u64, how: How) -> PlayerEvent {
    PlayerEvent::Closed { attempt: 1, index: 0, heard, how }
}

fn logged(api: &mut Chilpa, started_at: i64) -> ListenMetrics {
    api.listening_summary(&ListenFilter { from: Some(started_at), until: Some(started_at + 1), ..Default::default() }).unwrap()
}

/// The listen that waits in `play` to be picked up.
fn waiting(api: &mut Chilpa, play: u64) -> Option<u64> {
    api.plays.get_mut(play).and_then(|play| play.ledger.waiting_mut()).map(|listen| listen.played_frames)
}

#[test]
fn a_move_stopped_before_its_play_starts_keeps_what_was_heard() {
    let (a, b, at) = (11, 12, 1_000_000_011);
    let mut api = playing(a, "/music/moved-then-stopped.flac", at);
    say(&mut api, a, PlayerEvent::Progress { frames: seconds(3), played: seconds(3) });
    relaunch(&mut api, Relaunch::Output, b);
    // Stop, before the new output's play started: both end, in order.
    say(&mut api, a, PlayerEvent::Stopped { frames: seconds(3) });
    say(&mut api, b, PlayerEvent::Stopped { frames: 0 });
    let logged = logged(&mut api, at);
    assert_eq!((logged.events, logged.skips), (1, 1), "one listen, cut short: {logged:?}");
    assert_eq!(logged.listened_ms, 3_000, "with what was heard before the move");
    assert!(!api.now.playing, "and the window shows it stopped");
    assert!(api.plays.is_empty(), "both plays are settled");
}

#[test]
fn a_play_that_never_started_leaves_the_listen_of_another_alone() {
    let (never, heard, next, at) = (21, 22, 23, 1_000_000_021);
    let mut api = facade();
    // A play asked for and replaced before it was taken.
    api.launched(never, 0, Relaunch::Track);
    api.launched(heard, 0, Relaunch::Track);
    open(&mut api, heard, "/music/superseded.flac", at);
    say(&mut api, heard, PlayerEvent::Progress { frames: seconds(2), played: seconds(2) });
    relaunch(&mut api, Relaunch::Output, next);
    // The play that was never taken says it ended, with nothing played.
    say(&mut api, never, PlayerEvent::Stopped { frames: 0 });
    assert_eq!(api.plays.get(heard).map(|play| play.fate), Some(Fate::Replaced(Replaced::Output { by: next })), "what became of the heard play is its own");
    say(&mut api, heard, PlayerEvent::Stopped { frames: seconds(2) });
    assert_eq!(waiting(&mut api, next), Some(seconds(2)), "the listen waits in the play that took the output over");
    assert!(api.now.playing, "and none of it stopped the window");
}

#[test]
fn two_moves_in_a_row_are_one_listen() {
    let (a, b, c, at) = (31, 32, 33, 1_000_000_031);
    let path = "/music/moved-twice.flac";
    let mut api = playing(a, path, at);
    say(&mut api, a, PlayerEvent::Progress { frames: seconds(3), played: seconds(3) });
    relaunch(&mut api, Relaunch::Output, b);
    // Moved again before anything of the first move was heard of.
    relaunch(&mut api, Relaunch::Output, c);
    say(&mut api, a, PlayerEvent::Stopped { frames: seconds(3) });
    assert_eq!(waiting(&mut api, b), Some(seconds(3)), "handed to the play that took the output");
    say(&mut api, b, PlayerEvent::Stopped { frames: 0 });
    assert_eq!(waiting(&mut api, c), Some(seconds(3)), "which never started and hands it on");
    start(&mut api, c, path);
    say(&mut api, c, PlayerEvent::Progress { frames: seconds(2), played: seconds(2) });
    say(&mut api, c, closed(seconds(2), How::Completed));
    say(&mut api, c, PlayerEvent::Finished { frames: seconds(2) });
    let logged = logged(&mut api, at);
    assert_eq!((logged.events, logged.skips), (1, 0), "one listen, played to its end: {logged:?}");
    assert_eq!(logged.listened_ms, 5_000, "what was heard on both outputs");
}

#[test]
fn two_moves_with_the_first_ending_read_in_between_are_one_listen() {
    let (a, b, c, at) = (41, 42, 43, 1_000_000_041);
    let path = "/music/moved-twice-slowly.flac";
    let mut api = playing(a, path, at);
    say(&mut api, a, PlayerEvent::Progress { frames: seconds(3), played: seconds(3) });
    relaunch(&mut api, Relaunch::Output, b);
    say(&mut api, a, PlayerEvent::Stopped { frames: seconds(3) });
    relaunch(&mut api, Relaunch::Output, c);
    say(&mut api, b, PlayerEvent::Stopped { frames: 0 });
    start(&mut api, c, path);
    say(&mut api, c, PlayerEvent::Progress { frames: seconds(2), played: seconds(2) });
    say(&mut api, c, closed(seconds(2), How::Completed));
    say(&mut api, c, PlayerEvent::Finished { frames: seconds(2) });
    let logged = logged(&mut api, at);
    assert_eq!((logged.events, logged.skips), (1, 0), "one listen, played to its end: {logged:?}");
    assert_eq!(logged.listened_ms, 5_000);
}

#[test]
fn a_move_followed_by_another_track_leaves_the_first_as_cut_short() {
    let (a, b, c, at) = (51, 52, 53, 1_000_000_051);
    let mut api = playing(a, "/music/moved-then-left.flac", at);
    say(&mut api, a, PlayerEvent::Progress { frames: seconds(3), played: seconds(3) });
    relaunch(&mut api, Relaunch::Output, b);
    // The listener picks another track before the move started playing.
    relaunch(&mut api, Relaunch::Track, c);
    say(&mut api, a, PlayerEvent::Stopped { frames: seconds(3) });
    say(&mut api, b, PlayerEvent::Stopped { frames: 0 });
    let logged = logged(&mut api, at);
    assert_eq!((logged.events, logged.skips), (1, 1), "left by choice: {logged:?}");
    assert_eq!(logged.listened_ms, 3_000);
    assert_eq!(waiting(&mut api, c), None, "nothing is left for the new track to pick up");
}

#[test]
fn a_closed_track_is_written_at_once_with_what_left_the_device() {
    let (a, at) = (61, 1_000_000_061);
    let mut api = playing(a, "/music/closed.flac", at);
    say(&mut api, a, PlayerEvent::Progress { frames: seconds(3), played: seconds(3) });
    // The last progress read was short of what the device went on to play.
    say(&mut api, a, closed(seconds(5), How::Completed));
    let logged = logged(&mut api, at);
    assert_eq!((logged.events, logged.skips), (1, 0), "written as it is closed, played through: {logged:?}");
    assert_eq!(logged.listened_ms, 5_000, "with the device's count");
    assert!(!api.plays.listening());
}

#[test]
fn a_track_closed_as_broken_is_cut_short_whatever_its_play_says_after() {
    let (a, at) = (71, 1_000_000_071);
    let mut api = playing(a, "/music/closed-broken.flac", at);
    say(&mut api, a, PlayerEvent::Progress { frames: seconds(3), played: seconds(3) });
    say(&mut api, a, closed(seconds(3), How::Broken));
    say(&mut api, a, PlayerEvent::Finished { frames: seconds(3) });
    let logged = logged(&mut api, at);
    assert_eq!((logged.events, logged.skips), (1, 1), "cut short, though the play ran to its end: {logged:?}");
}

#[test]
fn the_close_of_another_track_or_of_another_play_leaves_the_listen_open() {
    let (a, other, at) = (81, 82, 1_000_000_081);
    let mut api = playing(a, "/music/closed-other.flac", at);
    say(&mut api, a, PlayerEvent::Progress { frames: seconds(3), played: seconds(3) });
    say(&mut api, a, PlayerEvent::Closed { attempt: 2, index: 1, heard: seconds(9), how: How::Failed });
    say(&mut api, other, closed(seconds(9), How::Failed));
    let listen = api.plays.get(a).and_then(|play| play.ledger.newest()).expect("still open");
    assert_eq!(listen.played_frames, seconds(3));
    assert_eq!(logged(&mut api, at).events, 0);
}

#[test]
fn a_track_stopped_for_a_move_is_kept_with_what_left_the_device() {
    let (a, b, at) = (91, 92, 1_000_000_091);
    let mut api = playing(a, "/music/closed-moved.flac", at);
    say(&mut api, a, PlayerEvent::Progress { frames: seconds(3), played: seconds(3) });
    relaunch(&mut api, Relaunch::Output, b);
    // Three seconds were handed to the device; the move threw one away.
    say(&mut api, a, closed(seconds(2), How::Stopped));
    say(&mut api, a, PlayerEvent::Stopped { frames: seconds(3) });
    assert_eq!(waiting(&mut api, b), Some(seconds(2)), "what was heard, not what was handed over, waits to be picked up");
    assert!(api.plays.get(a).is_none(), "and the old play is settled");
}

#[test]
fn a_track_stopped_by_the_listener_is_cut_short_as_it_is_closed() {
    let (a, at) = (101, 1_000_000_101);
    let mut api = playing(a, "/music/closed-stopped.flac", at);
    say(&mut api, a, PlayerEvent::Progress { frames: seconds(3), played: seconds(3) });
    say(&mut api, a, closed(seconds(2), How::Stopped));
    say(&mut api, a, PlayerEvent::Stopped { frames: seconds(3) });
    let logged = logged(&mut api, at);
    assert_eq!((logged.events, logged.skips), (1, 1), "{logged:?}");
    assert_eq!(logged.listened_ms, 2_000);
    assert!(!api.now.playing);
}

#[test]
fn what_a_play_says_after_nothing_was_left_to_play_does_not_reach_the_window() {
    let (a, at) = (111, 1_000_000_111);
    let mut api = playing(a, "/music/ran-out.flac", at);
    // Another queue was asked for and nothing of it could be played.
    api.end_unplayable(0);
    assert!(!api.now.playing);
    // What the old play had said before it was told to stop is read now.
    say(&mut api, a, PlayerEvent::Progress { frames: seconds(5), played: seconds(5) });
    assert_eq!(api.now.frames, 0, "the window shows a queue that ran out, not where the old play was");
    say(&mut api, a, closed(seconds(5), How::Stopped));
    say(&mut api, a, PlayerEvent::Stopped { frames: seconds(5) });
    assert!(!api.now.playing);
    let logged = logged(&mut api, at);
    assert_eq!((logged.events, logged.skips), (1, 1), "its listen is its own all the same, cut short: {logged:?}");
}

#[test]
fn a_listen_moved_to_an_output_where_its_track_did_not_start_is_cut_short() {
    let (a, b, at) = (121, 122, 1_000_000_121);
    let mut api = playing(a, "/music/moved-and-lost.flac", at);
    say(&mut api, a, PlayerEvent::Progress { frames: seconds(3), played: seconds(3) });
    relaunch(&mut api, Relaunch::Output, b);
    say(&mut api, a, PlayerEvent::Stopped { frames: seconds(3) });
    // The track would not open on the new output and the queue went on:
    // what starts there is another track.
    start(&mut api, b, "/music/the-next-one.flac");
    let logged = logged(&mut api, at);
    assert_eq!((logged.events, logged.skips), (1, 1), "it never went on: {logged:?}");
    assert_eq!(logged.listened_ms, 3_000);
}

#[test]
fn a_track_of_a_disc_cut_by_a_bad_sector_says_so_and_the_disc_plays_on() {
    let (a, at) = (131, 1_000_000_131);
    let mut api = playing(a, "/music/disc-track.flac", at);
    say(&mut api, a, PlayerEvent::Progress { frames: seconds(3), played: seconds(3) });
    say(&mut api, a, PlayerEvent::Notice { attempt: 1, message: "a sector would not read".into() });
    assert!(api.now.playing, "the disc goes on: the window does not blink to stopped");
    assert_eq!(api.now.error, "a sector would not read");
    say(&mut api, a, closed(seconds(3), How::Completed));
    let logged = logged(&mut api, at);
    assert_eq!((logged.events, logged.skips), (1, 1), "the cut track is a listen cut short: {logged:?}");
}

#[test]
fn a_track_that_started_in_a_replaced_play_is_a_listen_of_that_play() {
    let (a, b, at) = (141, 142, 1_000_000_141);
    let mut api = playing(a, "/music/first-of-the-old.flac", at);
    relaunch(&mut api, Relaunch::Track, b);
    open(&mut api, b, "/music/first-of-the-new.flac", at + 100);
    // Read after the new play went out: the old one's track ended and its
    // next one started, was heard for a second and was cut by the new play.
    say(&mut api, a, closed(seconds(4), How::Completed));
    let first = logged(&mut api, at);
    assert_eq!((first.events, first.skips), (1, 0), "the track that ended played through: {first:?}");
    // The second one starts now: its listen is filed under this moment.
    let recent = ListenFilter { from: Some(crate::now_unix() - 1), ..Default::default() };
    let before = api.listening_summary(&recent).unwrap();
    start(&mut api, a, "/music/second-of-the-old.flac");
    say(&mut api, a, PlayerEvent::Progress { frames: seconds(1), played: seconds(1) });
    say(&mut api, a, PlayerEvent::Closed { attempt: 1, index: 1, heard: seconds(1), how: How::Stopped });
    say(&mut api, a, PlayerEvent::Stopped { frames: seconds(1) });
    let after = api.listening_summary(&recent).unwrap();
    assert_eq!((after.events - before.events, after.skips - before.skips), (1, 1), "the one cut off by the new play is the old play's too, cut short: {after:?}");
    let new = api.plays.get(b).and_then(|play| play.ledger.newest()).expect("the new play's listen is untouched");
    assert_eq!(new.played_frames, 0);
    assert!(api.now.playing, "and the window still shows the new play");
}

#[test]
fn a_failure_of_a_replaced_play_is_not_said_of_the_one_that_replaced_it() {
    let (a, b, at) = (151, 152, 1_000_000_151);
    let mut api = playing(a, "/music/failed-behind.flac", at);
    say(&mut api, a, PlayerEvent::Progress { frames: seconds(2), played: seconds(2) });
    relaunch(&mut api, Relaunch::Track, b);
    say(&mut api, a, closed(seconds(2), How::Failed));
    say(&mut api, a, PlayerEvent::Failed { message: "the device failed".into() });
    assert!(api.now.playing && api.now.error.is_empty(), "the window is the new play's: {:?}", api.now.error);
    let logged = logged(&mut api, at);
    assert_eq!((logged.events, logged.skips), (1, 1), "{logged:?}");
}

#[test]
fn a_queue_replaced_under_a_play_takes_the_play_off_the_window_at_once() {
    let (a, at) = (161, 1_000_000_161);
    let mut api = playing(a, "/music/rows-replaced.flac", at);
    say(&mut api, a, PlayerEvent::Progress { frames: seconds(2), played: seconds(2) });
    assert_eq!(api.now.frames, seconds(2));
    // Other rows take the queue's place, and the launch that should follow
    // is refused or has not gone out yet.
    api.replace_queue(vec!["/music/another-queue.flac".into()]);
    assert!(!api.now.playing, "nothing plays until a launch says so");
    // What the old play said before it was told to stop is read now: every
    // position it names is another row's.
    say(&mut api, a, PlayerEvent::Progress { frames: seconds(5), played: seconds(5) });
    assert_eq!(api.now.frames, seconds(2), "the window does not follow it");
    say(&mut api, a, closed(seconds(5), How::Failed));
    say(&mut api, a, PlayerEvent::Failed { message: "the device failed".into() });
    assert!(api.now.error.is_empty(), "nor is its failure said of the new queue: {}", api.now.error);
    let logged = logged(&mut api, at);
    assert_eq!((logged.events, logged.skips), (1, 1), "its listen is written all the same: {logged:?}");
}

/// A WAV of that many seconds of silence.
fn silence(path: &Path, seconds: u32) {
    let data = 44_100 * 4 * seconds;
    let mut wave = b"RIFF".to_vec();
    wave.extend((36 + data).to_le_bytes());
    wave.extend(b"WAVEfmt ");
    wave.extend(16u32.to_le_bytes());
    wave.extend(1u16.to_le_bytes());
    wave.extend(2u16.to_le_bytes());
    wave.extend(44_100u32.to_le_bytes());
    wave.extend(176_400u32.to_le_bytes());
    wave.extend(4u16.to_le_bytes());
    wave.extend(16u16.to_le_bytes());
    wave.extend(b"data");
    wave.extend(data.to_le_bytes());
    wave.resize(wave.len() + data as usize, 0);
    std::fs::write(path, wave).unwrap();
}

#[test]
fn a_purge_reads_the_last_of_the_play_it_ends() {
    std::env::set_var("CHILPA_CAPTURE_REALTIME", "1");
    let folder = tempfile::tempdir().unwrap();
    let (kept, gone) = (folder.path().join("kept.wav"), folder.path().join("gone.wav"));
    silence(&kept, 10);
    silence(&gone, 10);
    let (kept, gone) = (kept.display().to_string(), gone.display().to_string());
    let mut api = facade();
    api.use_output("file:capture", false);
    api.play_tracks(&[kept, gone.clone()], "file:capture", false);
    let limit = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while api.poll().seconds < 0.3 && std::time::Instant::now() < limit {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(api.now.seconds >= 0.3, "the first track plays");
    let recent = ListenFilter { from: Some(crate::now_unix() - 10), ..Default::default() };
    let before = api.listening_summary(&recent).unwrap();
    // The second track's source is removed: the queue is rebuilt without it
    // and the player made again. The play that was cut says so before it goes.
    api.purge_queue(|path| path == gone);
    let after = api.listening_summary(&recent).unwrap();
    assert_eq!((after.events - before.events, after.skips - before.skips), (1, 1), "the stretch that was cut is a listen cut short, written as the purge returns: {after:?}");
    assert!(api.plays.current().is_some(), "and the track that stayed plays on as a play of its own");
    api.stop();
    std::env::remove_var("CHILPA_CAPTURE_REALTIME");
}

/// Leaves the player as a hand-over to a renderer does, with `last` being
/// what the player had said and nobody had read.
fn abandon(api: &mut Chilpa, last: Vec<(u64, PlayerEvent)>) {
    api.plays.retire(Fate::Abandoned);
    api.leave_plays(last);
    assert!(api.plays.is_empty(), "every play is settled as the player leaves");
}

fn broke(play_path: &str) -> PlayerEvent {
    PlayerEvent::Skipped { attempt: 1, index: 0, path: PathBuf::from(play_path), message: "it broke".into() }
}

#[test]
fn a_track_playing_as_its_player_leaves_is_a_transfer_with_or_without_its_close() {
    // The stop's close arrives after the cut: the last progress stands.
    let (a, at) = (171, 1_000_000_171);
    let mut api = playing(a, "/music/handed-over.flac", at);
    say(&mut api, a, PlayerEvent::Progress { frames: seconds(3), played: seconds(3) });
    abandon(&mut api, Vec::new());
    let late = logged(&mut api, at);
    assert_eq!((late.events, late.skips, late.listened_ms), (1, 0, 3_000), "{late:?}");
    // The close is read in the cut: the device's count stands.
    let (b, at) = (172, 1_000_000_172);
    let mut api = playing(b, "/music/handed-over-closed.flac", at);
    say(&mut api, b, PlayerEvent::Progress { frames: seconds(3), played: seconds(3) });
    abandon(&mut api, vec![(b, closed(seconds(2), How::Stopped)), (b, PlayerEvent::Stopped { frames: seconds(3) })]);
    let read = logged(&mut api, at);
    assert_eq!((read.events, read.skips, read.listened_ms), (1, 0, 2_000), "{read:?}");
}

#[test]
fn a_track_known_to_have_broken_before_its_player_leaves_is_cut_short_either_way() {
    for (play, at, close_in_the_cut) in [(181, 1_000_000_181, true), (182, 1_000_000_182, false)] {
        let path = "/music/broke-then-handed-over.flac";
        let mut api = playing(play, path, at);
        say(&mut api, play, PlayerEvent::Progress { frames: seconds(3), played: seconds(3) });
        // The last track broke and said so; its last audio is still playing
        // out when the player is handed over, which cancels it.
        say(&mut api, play, broke(path));
        let last = if close_in_the_cut { vec![(play, closed(seconds(3), How::Broken))] } else { Vec::new() };
        abandon(&mut api, last);
        let logged = logged(&mut api, at);
        assert_eq!((logged.events, logged.skips), (1, 1), "close read in the cut: {close_in_the_cut}: {logged:?}");
    }
}

#[test]
fn what_a_player_says_after_it_left_is_lost_and_the_listen_is_a_transfer() {
    // The track breaks, or the device fails, in the moment after the cut.
    let (a, at) = (191, 1_000_000_191);
    let mut api = playing(a, "/music/broke-after-the-cut.flac", at);
    say(&mut api, a, PlayerEvent::Progress { frames: seconds(3), played: seconds(3) });
    abandon(&mut api, Vec::new());
    say(&mut api, a, closed(seconds(3), How::Failed));
    say(&mut api, a, PlayerEvent::Failed { message: "the device failed".into() });
    let logged = logged(&mut api, at);
    assert_eq!((logged.events, logged.skips), (1, 0), "the declared loss: {logged:?}");
    assert!(api.now.error.is_empty(), "and nothing of it reaches the window");
}

#[test]
fn a_failure_or_a_stop_read_before_the_player_leaves_is_written_as_without_leaving() {
    let (a, at) = (201, 1_000_000_201);
    let mut api = playing(a, "/music/failed-then-handed-over.flac", at);
    say(&mut api, a, PlayerEvent::Progress { frames: seconds(3), played: seconds(3) });
    say(&mut api, a, closed(seconds(3), How::Failed));
    abandon(&mut api, vec![(a, PlayerEvent::Failed { message: "the device failed".into() })]);
    let failed = logged(&mut api, at);
    assert_eq!((failed.events, failed.skips), (1, 1), "{failed:?}");
    // Stopped by the listener, and its ending not read yet, when the player leaves.
    let (b, at) = (202, 1_000_000_202);
    let mut api = playing(b, "/music/stopped-then-handed-over.flac", at);
    say(&mut api, b, PlayerEvent::Progress { frames: seconds(3), played: seconds(3) });
    api.stop();
    abandon(&mut api, Vec::new());
    let stopped = logged(&mut api, at);
    assert_eq!((stopped.events, stopped.skips), (1, 1), "a stop asked for before is a stop: {stopped:?}");
}

#[test]
fn a_listen_on_its_way_to_another_output_as_the_player_leaves_is_a_transfer() {
    let (a, b, at) = (211, 212, 1_000_000_211);
    let mut api = playing(a, "/music/moving-then-handed-over.flac", at);
    say(&mut api, a, PlayerEvent::Progress { frames: seconds(3), played: seconds(3) });
    relaunch(&mut api, Relaunch::Output, b);
    abandon(&mut api, Vec::new());
    let logged = logged(&mut api, at);
    assert_eq!((logged.events, logged.skips, logged.listened_ms), (1, 0, 3_000), "{logged:?}");
}

/// The player is taken out of the facade in one place.
#[test]
fn nothing_else_empties_the_player() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut pending = vec![root.clone()];
    let mut found = Vec::new();
    while let Some(folder) = pending.pop() {
        for entry in std::fs::read_dir(&folder).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            let name = path.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/");
            if !name.ends_with(".rs") || name.ends_with("_tests.rs") || name.ends_with("/tests.rs") || name == "plays.rs" {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            let code = text.split("#[cfg(test)]").next().unwrap_or_default();
            if ["self.player.take()", "self.player = None", "mem::take(&mut self.player)"].iter().any(|taken| code.contains(taken)) {
                found.push(name);
            }
        }
    }
    assert!(found.is_empty(), "the player is emptied outside plays.rs: {found:?}");
}

#[test]
fn a_track_that_starts_behind_the_window_is_named_by_the_card_of_its_own_play() {
    let (a, b) = (221, 222);
    let mut api = facade();
    let address = "https://radio.example/a";
    let card = |title: &str, album: &str| crate::search::Hit { title: title.into(), artist: "Card artist".into(), album: album.into(), ..Default::default() };
    api.search.playing = Some((address.into(), card("Card of the first", "Its album")));
    api.launched(a, 0, Relaunch::Track);
    // The listener picks something else: the card in front is another one.
    api.search.playing = Some(("https://radio.example/b".into(), card("Card of the second", "Another album")));
    api.launched(b, 0, Relaunch::Track);
    // The first play's track had started before it was replaced; it is read now.
    let tags = chilpa_tags::Tags {
        title: Some("Tagged title".into()), artists: vec!["Tagged artist".into()], album: Some("Tagged album".into()),
        ..Default::default()
    };
    let face = api.face_of(a, Path::new(address), Some(&tags), None);
    api.listen_started(a, 1, Path::new(address), Some(&tags), &face, RATE, Some(16), false, None, false);
    let listen = api.plays.get(a).and_then(|play| play.ledger.newest()).and_then(|listen| listen.listen.clone()).expect("a listen the services can be told of");
    let at = 1_810_000_221;
    api.plays.get_mut(a).unwrap().ledger.listen_mut(1).unwrap().started_at = at;
    say(&mut api, a, closed(seconds(3), How::Stopped));
    let history = history_names(&mut api, at);
    assert_eq!((listen.track.as_str(), listen.artist.as_str(), listen.release.as_str()), ("Card of the first", "Card artist", "Its album"));
    assert_eq!(history, ["Card of the first", "Card artist", "Its album"], "the retired play persists the same identity");
}

#[test]
fn a_close_that_crosses_the_scrobble_threshold_is_counted_before_parking() {
    let mut api = playing(801, "/carried.wav", 1_810_000_801);
    say(&mut api, 801, PlayerEvent::Progress { frames: seconds(239), played: seconds(239) });
    assert!(!api.plays.get(801).unwrap().ledger.newest().unwrap().scrobbled);
    relaunch(&mut api, Relaunch::Output, 802);
    say(&mut api, 801, closed(seconds(240), How::Stopped));
    let parked = api.plays.get_mut(801).unwrap().ledger.waiting_mut().unwrap();
    assert_eq!(parked.heard_ms(), 240_000);
    assert!(parked.scrobbled, "the final count must scrobble before any successor event arrives");
    say(&mut api, 801, PlayerEvent::Stopped { frames: seconds(240) });
    assert!(api.plays.get_mut(802).unwrap().ledger.waiting_mut().unwrap().scrobbled);
    start(&mut api, 802, "/carried.wav");
    assert!(api.plays.get(802).unwrap().ledger.newest().unwrap().scrobbled, "resuming must retain the once-only marker");
}

fn history_names(api: &mut Chilpa, started_at: i64) -> Vec<String> {
    use crate::listening::{ListenDimension, ListenOrder};
    let filter = ListenFilter { from: Some(started_at), until: Some(started_at + 1), ..Default::default() };
    [ListenDimension::Track, ListenDimension::Artist, ListenDimension::Album].into_iter().map(|dimension| {
        let buckets = api.listening_buckets(&filter, dimension, ListenOrder::Key, 10).unwrap();
        assert_eq!(buckets.len(), 1, "one persisted listen per dimension: {buckets:?}");
        buckets[0].label.clone()
    }).collect()
}

#[test]
fn a_url_without_a_station_name_logs_the_identity_shown_in_the_window() {
    use chilpa_core::output_evidence::{Delivery, Route, SampleFormat, StreamFormat};
    let mut api = facade();
    let path = "https://radio.example/unnamed";
    api.search.playing = Some((path.into(), crate::search::Hit {
        title: "Named by its card".into(), artist: "Card artist".into(), album: "Card album".into(), ..Default::default()
    }));
    api.launched(811, 0, Relaunch::Track);
    let opened = StreamFormat::new(SampleFormat::S16, RATE, 2);
    let tags = chilpa_tags::Tags { title: Some(path.into()), album: Some("Radio".into()), ..Default::default() };
    say(&mut api, 811, PlayerEvent::Started {
        path: path.into(), opened,
        route: Route { revision: 1, device: "file:capture".into(), delivery: Delivery { requested: opened, opened, transforms: Vec::new() },
            planned_processing: Vec::new(), exclusive_requested: false, observation: Default::default() },
        channels: 2, bits: Some(16), bit_perfect: false, total_frames: None, tags: Some(tags), artwork: None,
        attempt: 1, index: 0, queue_len: 1, dsd_multiple: None, dsd_passthrough: false,
    });
    let visible = [api.now.title.clone(), api.now.artist.clone(), api.now.album.clone()];
    assert_eq!(visible, ["Named by its card", "Card artist", "Card album"]);
    let at = 1_810_000_811;
    let listen = api.plays.get_mut(811).unwrap().ledger.listen_mut(1).unwrap();
    listen.started_at = at;
    let scrobble = listen.listen.clone();
    say(&mut api, 811, closed(seconds(3), How::Completed));
    assert_eq!(history_names(&mut api, at), visible, "the saved row must not fall back to URL/Radio");
    let scrobble = scrobble.expect("a card with an artist supplies the scrobble identity without tag artists");
    assert_eq!([scrobble.track, scrobble.artist, scrobble.release], visible);
}

fn station_history(play: u64, name: Option<&str>) {
    let mut api = facade();
    let address = "https://radio.example/stream";
    let title = name.unwrap_or(address);
    let tags = chilpa_tags::Tags { title: Some(title.into()), album: Some("Radio".into()), ..Default::default() };
    api.launched(play, 0, Relaunch::Track);
    let face = api.face_of(play, Path::new(address), Some(&tags), None);
    assert_eq!([face.title.as_str(), face.artist.as_str(), face.album.as_str()], [title, "", "Radio"]);
    api.listen_started(play, 1, Path::new(address), Some(&tags), &face, RATE, Some(16), false, None, false);
    let at = 1_810_001_000 + play as i64;
    let listen = api.plays.get_mut(play).unwrap().ledger.listen_mut(1).unwrap();
    listen.started_at = at;
    assert!(listen.listen.is_none(), "a station without an artist is not a scrobble");
    assert_eq!([listen.context.title.as_str(), listen.context.artist.as_str(), listen.context.album.as_str()], [title, "", "Radio"]);
    say(&mut api, play, closed(seconds(3), How::Completed));
    let filter = ListenFilter { from: Some(at), until: Some(at + 1), ..Default::default() };
    let (rows, _) = api.library().unwrap().listening_history(&filter, "", 0, 10).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!([rows[0].title.as_str(), rows[0].artist.as_str(), rows[0].album.as_str()], [title, "", "Radio"]);
}

#[test]
fn a_url_with_a_station_name_keeps_that_name_in_history() {
    station_history(812, Some("The station"));
}

#[test]
fn a_url_without_a_station_name_or_card_keeps_its_current_identity() {
    station_history(813, None);
}

fn local_identity(play: u64, tags: Option<chilpa_tags::Tags>, card: Option<crate::search::Hit>, visible: [&str; 3], expected: [&str; 3]) {
    let mut api = facade();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(format!("history-{play}.wav")).display().to_string();
    let lib = api.library().unwrap();
    let source = lib.source_id("local", "default").unwrap();
    let db = lib.connection();
    db.execute("INSERT INTO albums(title, album_artist) VALUES ('Library album', 'Library artist')", []).unwrap();
    let album = db.last_insert_rowid();
    db.execute("INSERT INTO tracks(source_id, external_id, title, artist, album_id, added_at) VALUES (?1, ?2, 'Library title', 'Library artist', ?3, 0)",
        rusqlite::params![source, path, album]).unwrap();
    api.search.playing = card.map(|card| (path.clone(), card));
    api.launched(play, 0, Relaunch::Track);
    let face = api.face_of(play, Path::new(&path), tags.as_ref(), None);
    assert_eq!([face.title.as_str(), face.artist.as_str(), face.album.as_str()], visible, "what the window shows");
    api.listen_started(play, 1, Path::new(&path), tags.as_ref(), &face, RATE, Some(16), false, None, false);
    let at = 1_810_002_000 + play as i64;
    let listen = api.plays.get_mut(play).unwrap().ledger.listen_mut(1).unwrap();
    listen.started_at = at;
    let scrobble = listen.listen.as_ref().expect("the captured identity names the scrobble");
    assert_eq!([scrobble.track.as_str(), scrobble.artist.as_str(), scrobble.release.as_str()], expected);
    assert_eq!([listen.context.title.as_str(), listen.context.artist.as_str(), listen.context.album.as_str()], expected);
    say(&mut api, play, closed(seconds(3), How::Completed));
    assert_eq!(history_names(&mut api, at), expected);
}

#[test]
fn a_file_without_tags_is_named_by_its_library_row_everywhere() {
    local_identity(814, None, None, ["Library title", "Library artist", "Library album"], ["Library title", "Library artist", "Library album"]);
}

#[test]
fn empty_face_fields_keep_the_library_snapshot_per_field() {
    let tags = chilpa_tags::Tags { title: Some("Tagged title".into()), artists: vec!["Tagged artist".into()], album: Some("Tagged album".into()), ..Default::default() };
    let card = crate::search::Hit { artist: "Card artist".into(), ..Default::default() };
    local_identity(815, Some(tags), Some(card), ["", "Card artist", ""], ["Library title", "Card artist", "Library album"]);
}

#[test]
fn the_library_row_names_a_held_file_over_its_tags() {
    // The row carries what a sidecar lays over the file, as the lists show it.
    let tags = chilpa_tags::Tags { title: Some("Tagged title".into()), artists: vec!["Tagged artist".into()], album: Some("Tagged album".into()), ..Default::default() };
    local_identity(816, Some(tags), None, ["Library title", "Library artist", "Library album"], ["Library title", "Library artist", "Library album"]);
}
