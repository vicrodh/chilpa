//! What the Mac's media controls and Chilpa say to each other, apart from the
//! system, so the choices are tested wherever the workspace is.
use std::time::Duration;

/// What a media key, the Control Center widget or the lock screen asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Play,
    Pause,
    Toggle,
    Next,
    Previous,
    Stop,
    /// A move by that many microseconds, back when negative.
    SeekBy(i64),
    /// A move to that many microseconds from the start.
    SetPosition(i64),
}

/// What plays, as the widget shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Track {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: Option<Duration>,
    /// A `file://` or `https://` address the system can read.
    pub art_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Playing,
    Paused,
    Stopped,
}

/// Where a seek asked by `command` lands, in seconds, inside a track `total`
/// seconds long that is `at` seconds in. Nothing for another command, or when
/// the length is not known: there is nowhere to jump to.
pub fn seek_target(command: Command, at: f64, total: f64) -> Option<f64> {
    if total <= 0.0 {
        return None;
    }
    let to = match command {
        Command::SeekBy(micros) => at + micros as f64 / 1e6,
        Command::SetPosition(micros) => micros as f64 / 1e6,
        _ => return None,
    };
    Some(to.clamp(0.0, total))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_seek_by_moves_from_where_it_is_and_stays_inside_the_track() {
        assert_eq!(seek_target(Command::SeekBy(5_000_000), 10.0, 100.0), Some(15.0));
        assert_eq!(seek_target(Command::SeekBy(-30_000_000), 10.0, 100.0), Some(0.0));
        assert_eq!(seek_target(Command::SeekBy(500_000_000), 10.0, 100.0), Some(100.0));
    }

    #[test]
    fn a_set_position_is_absolute() {
        assert_eq!(seek_target(Command::SetPosition(42_500_000), 10.0, 100.0), Some(42.5));
    }

    #[test]
    fn without_a_length_or_a_seek_there_is_no_target() {
        assert_eq!(seek_target(Command::SeekBy(1), 0.0, 0.0), None);
        assert_eq!(seek_target(Command::Next, 1.0, 10.0), None);
    }
}
