//! The Mac's Now Playing: the Control Center widget, the media keys and the
//! lock screen, through MediaRemote. `souvlaki` wraps it, as in QBZ.
//!
//! The commands arrive on the application's run loop, which in a Qt
//! application is the one the window lives on; without that loop running
//! none arrives. What plays is pushed from any thread.

use crate::media::{Command, Status, Track};
use souvlaki::{MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig, SeekDirection};
use std::time::Duration;

/// A seek key with no distance moves this far, as QBZ's does.
const SEEK_STEP_MICROS: i64 = 5_000_000;

pub struct NowPlaying {
    controls: MediaControls,
}

fn signed(direction: SeekDirection, by: Duration) -> i64 {
    let micros = i64::try_from(by.as_micros()).unwrap_or(i64::MAX);
    match direction {
        SeekDirection::Forward => micros,
        SeekDirection::Backward => -micros,
    }
}

fn command_of(event: MediaControlEvent) -> Option<Command> {
    Some(match event {
        MediaControlEvent::Play => Command::Play,
        MediaControlEvent::Pause => Command::Pause,
        MediaControlEvent::Toggle => Command::Toggle,
        MediaControlEvent::Next => Command::Next,
        MediaControlEvent::Previous => Command::Previous,
        MediaControlEvent::Stop => Command::Stop,
        MediaControlEvent::Seek(direction) => Command::SeekBy(signed(direction, Duration::from_micros(SEEK_STEP_MICROS as u64))),
        MediaControlEvent::SeekBy(direction, by) => Command::SeekBy(signed(direction, by)),
        MediaControlEvent::SetPosition(MediaPosition(at)) => Command::SetPosition(i64::try_from(at.as_micros()).unwrap_or(i64::MAX)),
        _ => return None,
    })
}

/// Registers with the system. `on_command` is called from the run loop; None
/// when the system would not take it, and Chilpa goes on without.
pub fn start(on_command: impl Fn(Command) + Send + 'static) -> Option<NowPlaying> {
    let config = PlatformConfig { dbus_name: "app.chilpa.Chilpa", display_name: "Chilpa", hwnd: None };
    let mut controls = match MediaControls::new(config) {
        Ok(controls) => controls,
        Err(error) => {
            eprintln!("now playing: the system would not take it: {error:?}");
            return None;
        }
    };
    let attached = controls.attach(move |event| {
        if let Some(command) = command_of(event) {
            on_command(command);
        }
    });
    if let Err(error) = attached {
        eprintln!("now playing: the commands could not be attached: {error:?}");
        return None;
    }
    Some(NowPlaying { controls })
}

impl NowPlaying {
    /// What plays now.
    pub fn set_track(&mut self, track: &Track) {
        let _ = self.controls.set_metadata(MediaMetadata {
            title: Some(&track.title),
            artist: Some(&track.artist),
            album: Some(&track.album),
            cover_url: track.art_url.as_deref(),
            duration: track.duration,
        });
    }

    /// Nothing plays.
    pub fn clear_track(&mut self) {
        let _ = self.controls.set_metadata(MediaMetadata::default());
    }

    /// The state and where in the track it is; the system carries on from there.
    pub fn set_playback(&mut self, status: Status, position: Option<Duration>) {
        let progress = position.map(MediaPosition);
        let _ = self.controls.set_playback(match status {
            Status::Playing => MediaPlayback::Playing { progress },
            Status::Paused => MediaPlayback::Paused { progress },
            Status::Stopped => MediaPlayback::Stopped,
        });
    }
}
