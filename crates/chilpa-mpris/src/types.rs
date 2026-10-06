//! What goes to the desktop's media controls and what comes back.

use std::time::Duration;

/// What is playing, as the desktop shows it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrackMeta {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: Option<Duration>,
    /// `mpris:artUrl`: the cover of the track, as a file or web address.
    /// Not the application's icon, which the desktop finds through the
    /// `DesktopEntry` property.
    pub art_url: Option<String>,
    /// `xesam:url`: a public address of the track, when it has one.
    pub url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackStatus {
    Playing,
    Paused,
    Stopped,
}

/// What the desktop asks for: media keys, the shell's widget, `playerctl`.
/// Every time is in microseconds.
#[derive(Debug, Clone)]
pub enum MediaEvent {
    Play,
    Pause,
    Toggle,
    Next,
    Previous,
    Stop,
    /// A move by that much, back or forth (MPRIS `Seek`).
    SeekBy(i64),
    /// A move to that moment (MPRIS `SetPosition`).
    SetPosition(i64),
    /// A volume from 0.0 to 1.0.
    SetVolume(f64),
    Raise,
    Quit,
}
