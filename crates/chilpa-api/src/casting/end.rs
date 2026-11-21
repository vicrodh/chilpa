use chilpa_cast::{State, Status};
use std::time::{Duration, Instant};

/// How close the renderer must get to the end before a DLNA STOPPED counts
/// as the end of the track rather than a hiccup.
const END_GUARD_SECS: f64 = 5.0;
/// Below this observed maximum the renderer's RelTime is unreliable (plenty
/// never implement GetPositionInfo) and the near-end guard is skipped.
const POSITION_SIGNAL_MIN_SECS: f64 = 1.0;
/// A guard must never wedge the queue: a persistent STOPPED ends the track.
const PREMATURE_STOP_POLLS_MAX: u32 = 4;
/// Polls a renderer may sit inside the end guard with a frozen RelTime and
/// no STOPPED before the track is treated as ended.
const END_STALL_POLLS_MAX: u32 = 5;

pub(super) struct End {
    started: Instant, seen_playing: bool, maximum: f64, previous: f64, stopped: u8, frozen: u8, duration: f64,
    renderer: Renderer,
}
/// The reference player's DLNA end rule, judged on the transport state word.
#[derive(Default)]
struct Renderer { saw_playing: bool, max_position: f64, premature_stop_polls: u32, stall_position: f64, stall_polls: u32, ended: bool }
impl Renderer {
    fn tick(&mut self, state: &str, position: f64, duration: f64) -> bool {
        if state == "PLAYING" { self.saw_playing = true; self.max_position = self.max_position.max(position); }
        let stopped = matches!(state, "STOPPED" | "NO_MEDIA_PRESENT");
        // The guard only makes sense with a usable position signal: renderers
        // whose RelTime never moves are taken at their STOPPED.
        let position_reliable = self.max_position > POSITION_SIGNAL_MIN_SECS;
        let near_end = duration <= 0.0 || !position_reliable || self.max_position >= duration - END_GUARD_SECS;
        if stopped && self.saw_playing && !near_end {
            self.premature_stop_polls += 1;
            chilpa_core::trace_playback("cast", "premature_stop", format_args!("polls={}/{PREMATURE_STOP_POLLS_MAX} state={state} max_position={:.1} duration={duration:.1}", self.premature_stop_polls, self.max_position));
        } else if !stopped { self.premature_stop_polls = 0; }
        let persistent_stop = self.premature_stop_polls >= PREMATURE_STOP_POLLS_MAX;
        // A renderer that reaches the last seconds and freezes its RelTime
        // without ever reporting STOPPED would park the queue forever. Only
        // with a real position signal, and never on a pause.
        let inside_end_guard = position_reliable && duration > 0.0 && self.max_position >= duration - END_GUARD_SECS;
        let frozen = (position - self.stall_position).abs() < 0.5;
        self.stall_position = position;
        let stalled_at_end = if !inside_end_guard || !frozen || state == "PAUSED_PLAYBACK" { self.stall_polls = 0; false } else {
            self.stall_polls = self.stall_polls.saturating_add(1);
            self.stall_polls >= END_STALL_POLLS_MAX
        };
        if stalled_at_end && self.saw_playing && !self.ended {
            chilpa_core::trace_playback("cast", "end_stall", format_args!("polls={} state={state} position={position:.1} duration={duration:.1}", self.stall_polls));
        }
        let ended = ((stopped && (near_end || persistent_stop)) || stalled_at_end) && self.saw_playing && !self.ended;
        if state == "PLAYING" { self.ended = false; } else if ended { self.ended = true; }
        ended
    }
}
impl End {
    pub fn new(duration: f64) -> Self { Self { started: Instant::now(), seen_playing: false, maximum: 0.0, previous: 0.0, stopped: 0, frozen: 0, duration, renderer: Renderer::default() } }
    /// A seek restarts the Cast rule. The DLNA rule keeps its maximum across
    /// a seek, as the reference player does.
    pub fn seek(&mut self) {
        let (seen_playing, renderer) = (self.seen_playing, std::mem::take(&mut self.renderer));
        *self = Self::new(self.duration);
        self.seen_playing = seen_playing;
        self.renderer = renderer;
    }
    pub fn tick(&mut self, status: &Status) -> bool {
        if let Some(state) = status.transport.as_deref() {
            // The renderer's duration when it reports one, else the catalog's.
            let duration = status.duration.filter(|d| *d > 0.0).unwrap_or(self.duration);
            return self.renderer.tick(state, status.position.unwrap_or(0.0), duration);
        }
        if status.state == State::Finished { return true; }
        if let Some(duration) = status.duration.filter(|d| *d > 0.0) { self.duration = duration; }
        if status.state == State::Playing { self.seen_playing = true; }
        let position = status.position.unwrap_or(0.0);
        self.maximum = self.maximum.max(position);
        let near = self.duration > 0.0 && self.maximum >= (self.duration - 5.0).max(1.0);
        if status.state == State::Paused || status.state == State::Buffering { self.frozen = 0; self.stopped = 0; return false; }
        self.stopped = if status.state == State::Stopped { self.stopped.saturating_add(1) } else { 0 };
        self.frozen = if near && (position - self.previous).abs() < 0.5 { self.frozen.saturating_add(1) } else { 0 };
        self.previous = position;
        self.seen_playing && self.started.elapsed() >= Duration::from_secs(3) && ((near && self.stopped >= 2) || self.stopped >= 4 || self.frozen >= 5)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn dlna(state: &str, position: f64) -> Status { Status { position: Some(position), duration: Some(100.0), transport: Some(state.into()), ..Default::default() } }
    #[test]
    fn transient_stop_pause_and_frozen_end_are_distinct() {
        let mut end = End::new(100.0); end.started = Instant::now() - Duration::from_secs(10);
        assert!(!end.tick(&Status { state: State::Stopped, ..Default::default() }));
        assert!(!end.tick(&Status { state: State::Playing, position: Some(96.0), ..Default::default() }));
        assert!(!end.tick(&Status { state: State::Stopped, ..Default::default() }));
        for _ in 0..10 { assert!(!end.tick(&Status { state: State::Paused, position: Some(96.0), ..Default::default() })); }
        let mut ended = false;
        for _ in 0..6 { ended |= end.tick(&Status { state: State::Playing, position: Some(96.0), ..Default::default() }); }
        assert!(ended);
    }
    #[test]
    fn a_dlna_stop_near_the_end_ends_the_track_on_the_first_poll() {
        let mut end = End::new(100.0);
        assert!(!end.tick(&dlna("STOPPED", 0.0)), "nothing was seen playing yet");
        assert!(!end.tick(&dlna("PLAYING", 96.0)));
        assert!(end.tick(&dlna("STOPPED", 0.0)));
        assert!(!end.tick(&dlna("STOPPED", 0.0)), "the end is reported once");
    }
    #[test]
    fn a_premature_dlna_stop_waits_four_polls() {
        let mut end = End::new(100.0);
        assert!(!end.tick(&dlna("PLAYING", 30.0)));
        for _ in 0..3 { assert!(!end.tick(&dlna("STOPPED", 30.0))); }
        assert!(end.tick(&dlna("STOPPED", 30.0)));
        let mut resumed = End::new(100.0);
        assert!(!resumed.tick(&dlna("PLAYING", 30.0)));
        for _ in 0..3 { assert!(!resumed.tick(&dlna("STOPPED", 30.0))); }
        assert!(!resumed.tick(&dlna("PLAYING", 31.0)), "playing again clears the count");
        for _ in 0..3 { assert!(!resumed.tick(&dlna("STOPPED", 31.0))); }
    }
    #[test]
    fn a_frozen_dlna_end_without_stop_ends_after_five_polls_but_a_pause_does_not() {
        let mut end = End::new(100.0);
        assert!(!end.tick(&dlna("PLAYING", 97.0)));
        for _ in 0..10 { assert!(!end.tick(&dlna("PAUSED_PLAYBACK", 97.0))); }
        for _ in 0..4 { assert!(!end.tick(&dlna("UNKNOWN", 97.0))); }
        assert!(end.tick(&dlna("UNKNOWN", 97.0)));
    }
    #[test]
    fn without_a_position_signal_a_dlna_stop_is_taken_at_once() {
        let mut end = End::new(100.0);
        assert!(!end.tick(&dlna("PLAYING", 0.0)));
        assert!(end.tick(&dlna("STOPPED", 0.0)));
    }
}
