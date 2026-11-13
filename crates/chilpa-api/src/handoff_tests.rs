//! The listen and the mark of a relaunch across a hand-off, driven by the
//! events as the player says them, each with its play.
//!
//! The listen is set up as a track's start leaves it, and a play picking it
//! up on another output is the call that start makes: an event that starts a
//! track carries the route of a real delivery, which these have none of.

use chilpa_core::player::PlayerEvent;
use std::path::PathBuf;

use crate::listening::{ListenFilter, ListenMetrics};
use crate::{Chilpa, InProgress, Relaunch};

const RATE: u32 = 44_100;

/// A facade with a queue in front of it and `play` playing `path`, heard
/// from `started_at`: a moment no other test logs under.
fn playing(play: u64, path: &str, started_at: i64) -> Chilpa {
    let mut api = Chilpa::with_listed(Vec::new(), Vec::new());
    api.listen.active = 1;
    open_listen(&mut api, play, path, started_at);
    api
}

fn open_listen(api: &mut Chilpa, play: u64, path: &str, started_at: i64) {
    api.in_progress = Some(InProgress {
        context: Default::default(),
        path: PathBuf::from(path),
        play,
        resuming: false,
        carried_ms: 0,
        started_at,
        sample_rate_hz: RATE,
        bit_depth: Some(16),
        bit_perfect: false,
        duration_ms: 600_000,
        played_frames: 0,
        listen: None,
        scrobbled: false,
    });
    api.now.playing = true;
    api.launched_from(0, play);
}

/// The window asks for what plays to be replaced: the mark, then the launch.
fn relaunch(api: &mut Chilpa, why: Relaunch, new: u64) {
    api.mark_relaunch(why);
    api.launched_from(0, new);
}

fn say(api: &mut Chilpa, play: u64, event: PlayerEvent) {
    api.apply_player_event(play, event, &mut false);
}

fn seconds(count: u64) -> u64 {
    count * u64::from(RATE)
}

fn logged(api: &mut Chilpa, started_at: i64) -> ListenMetrics {
    api.listening_summary(&ListenFilter { from: Some(started_at), until: Some(started_at + 1), ..Default::default() }).unwrap()
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
}

#[test]
fn a_play_that_never_started_leaves_the_mark_of_another_alone() {
    let (never, heard, next, at) = (21, 22, 23, 1_000_000_021);
    let mut api = playing(heard, "/music/superseded.flac", at);
    say(&mut api, heard, PlayerEvent::Progress { frames: seconds(2), played: seconds(2) });
    relaunch(&mut api, Relaunch::Output, next);
    // A play asked for earlier and never taken says it ended, late.
    say(&mut api, never, PlayerEvent::Stopped { frames: 0 });
    assert_eq!(api.relaunching, Some((Relaunch::Output, heard)), "the mark is still the heard play's");
    say(&mut api, heard, PlayerEvent::Stopped { frames: seconds(2) });
    let listen = api.in_progress.as_ref().expect("the listen waits to be picked up");
    assert!(listen.resuming);
    assert_eq!(listen.played_frames, seconds(2));
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
    assert_eq!(api.relaunching, Some((Relaunch::Output, a)), "replacing a play that opened no listen leaves the mark with the one that did");
    say(&mut api, a, PlayerEvent::Stopped { frames: seconds(3) });
    say(&mut api, b, PlayerEvent::Stopped { frames: 0 });
    assert!(api.resume_listen(std::path::Path::new(path), RATE, Some(16), false, c), "the third play picks the listen up");
    say(&mut api, c, PlayerEvent::Progress { frames: seconds(2), played: seconds(2) });
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
    assert!(api.resume_listen(std::path::Path::new(path), RATE, Some(16), false, c), "the third play picks the listen up");
    say(&mut api, c, PlayerEvent::Progress { frames: seconds(2), played: seconds(2) });
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
    assert_eq!(api.relaunching, Some((Relaunch::Track, a)), "the newest reason, for the play whose listen is open");
    say(&mut api, a, PlayerEvent::Stopped { frames: seconds(3) });
    say(&mut api, b, PlayerEvent::Stopped { frames: 0 });
    let logged = logged(&mut api, at);
    assert_eq!((logged.events, logged.skips), (1, 1), "left by choice: {logged:?}");
    assert_eq!(logged.listened_ms, 3_000);
    assert!(api.in_progress.is_none(), "nothing is left for the new track to pick up");
}
