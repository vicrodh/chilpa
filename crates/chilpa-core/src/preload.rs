//! The next track, opened while the one before it still plays.
//!
//! Opening a track reads its file: the header, the tags, the cover. On a
//! network share that is a few hundred milliseconds, and with gapless on
//! the device is running all that while on what it holds, which is a third
//! of a second at most. Opened ahead, on a thread of its own, the next
//! track is ready before the one before it ends, and the device never
//! waits. A track that turns out not to be next is dropped unplayed.
//!
//! A track over the network is not opened the moment the one before it
//! starts: opening it is a round trip to its source, which took 3.8 to 6.1 s
//! for a plugin in the owner's trace (2026-10-09), and in those first
//! seconds the track that plays is filling its own cushion from the same
//! place. It is opened once that one has played a while ([`at`]).

use crate::playback::PlayError;
use crate::queue_playback::Opened;
use chilpa_audio::sink::StopSignal;
use std::{path::{Path, PathBuf}, sync::{Arc, atomic::{AtomicBool, Ordering}, mpsc}, time::Duration};

/// What opens a track ahead, on the preload's thread.
type Open = Box<dyn FnOnce(&Path) -> Result<Opened, PlayError> + Send>;

/// How long a track plays before the next one, over the network, is opened.
const AFTER: Duration = Duration::from_secs(15);

/// When the next track is opened, in frames the device has taken of the
/// one before it: `None`, at once, for a file; for a track over the network
/// after [`AFTER`], or halfway through a track shorter than twice that.
pub(crate) fn at(remote: bool, rate: u32, length_seconds: Option<f64>) -> Option<u64> {
    if !remote {
        return None;
    }
    let after = AFTER.as_secs_f64();
    let seconds = length_seconds.map_or(after, |length| after.min(length / 2.0));
    Some((seconds * f64::from(rate)) as u64)
}

#[derive(Default)]
pub(crate) struct Preload {
    pending: Option<(PathBuf, mpsc::Receiver<Result<Opened, PlayError>>)>,
    busy: Arc<AtomicBool>,
    /// A track to open once the one before it has played this many frames.
    later: Option<(PathBuf, u64, Open)>,
}

impl Preload {
    /// Opens that track on a thread of its own, with `open`, unless it is
    /// the one already being opened or another one still is.
    pub fn request<F>(&mut self, path: PathBuf, open: F)
    where
        F: FnOnce(&Path) -> Result<Opened, PlayError> + Send + 'static,
    {
        if self.pending.as_ref().is_some_and(|(current, _)| *current == path) { return; }
        if self.busy.swap(true, Ordering::AcqRel) { return; }
        let (tx, rx) = mpsc::channel();
        let busy = self.busy.clone();
        let next = path.clone();
        if std::thread::Builder::new().name("chilpa-preload".into()).spawn(move || {
            let started = std::time::Instant::now();
            let opened = open(&next);
            chilpa_audio::diagnostics::trace("queue", "preloaded", format_args!("after={:?} path={next:?} ok={}", started.elapsed(), opened.is_ok()));
            busy.store(false, Ordering::Release);
            let _ = tx.send(opened);
        }).is_ok() {
            self.pending = Some((path, rx));
        } else { self.busy.store(false, Ordering::Release); }
    }

    /// Opens that track now, or, given a count of frames, once the one
    /// before it has played that many ([`Self::reached`]).
    pub fn request_at<F>(&mut self, path: PathBuf, at: Option<u64>, open: F)
    where
        F: FnOnce(&Path) -> Result<Opened, PlayError> + Send + 'static,
    {
        match at {
            None => self.request(path, open),
            Some(frames) => self.later = Some((path, frames, Box::new(open))),
        }
    }

    /// The track that plays has gone out this far: the one waiting to be
    /// opened is opened when it is far enough.
    pub fn reached(&mut self, delivered: u64) {
        if self.later.as_ref().is_some_and(|(_, at, _)| delivered >= *at) {
            if let Some((path, _, open)) = self.later.take() {
                self.request(path, open);
            }
        }
    }

    /// The track opened ahead, if it is this one, waiting for it to finish
    /// opening; `None` when another was opened, or none. A track still
    /// waiting to be opened is forgotten: the queue has moved on.
    pub fn take(&mut self, path: &Path, stop: &StopSignal) -> Option<Result<Opened, PlayError>> {
        self.later = None;
        let (expected, rx) = self.pending.take()?;
        if expected != path { return None; }
        while !stop.is_stopped() {
            match rx.recv_timeout(Duration::from_millis(20)) {
                Ok(opened) => return Some(opened),
                Err(mpsc::RecvTimeoutError::Timeout) => {},
                Err(mpsc::RecvTimeoutError::Disconnected) => return None,
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queue_playback::{open_track, DsdMode};
    use chilpa_audio::format::SampleFormat;
    use chilpa_audio::negotiate::{DeviceCaps, Policy};
    use std::io::Write;

    /// A second of silence, 16 bits, stereo, as a WAV.
    fn wave(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("chilpa-preload-{name}.wav"));
        let data: u32 = 44_100 * 4;
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"RIFF").unwrap();
        f.write_all(&(36 + data).to_le_bytes()).unwrap();
        f.write_all(b"WAVEfmt ").unwrap();
        f.write_all(&16u32.to_le_bytes()).unwrap();
        f.write_all(&1u16.to_le_bytes()).unwrap();
        f.write_all(&2u16.to_le_bytes()).unwrap();
        f.write_all(&44_100u32.to_le_bytes()).unwrap();
        f.write_all(&176_400u32.to_le_bytes()).unwrap();
        f.write_all(&4u16.to_le_bytes()).unwrap();
        f.write_all(&16u16.to_le_bytes()).unwrap();
        f.write_all(b"data").unwrap();
        f.write_all(&data.to_le_bytes()).unwrap();
        f.write_all(&vec![0u8; data as usize]).unwrap();
        path
    }

    fn device() -> DeviceCaps {
        DeviceCaps { sample_rates: vec![44_100], formats: vec![SampleFormat::S16], channels: vec![2], probed: true }
    }

    #[test]
    fn the_next_track_is_opened_ahead_and_handed_over_whole() {
        let (next, other) = (wave("next"), wave("other"));
        let mut preload = Preload::default();
        let caps = device();
        preload.request(next.clone(), move |path| open_track(path, &caps, Policy::AllowAnything, DsdMode::Pcm, None));
        let opened = preload.take(&next, &StopSignal::new()).expect("it was asked for").expect("the file opens");
        assert_eq!(opened.about.frames, Some(44_100), "what it says of itself came along");
        assert_eq!(opened.delivery.opened.sample_rate_hz, 44_100);

        // Another track is not the one opened ahead, and the one opened
        // ahead is not kept for later.
        let caps = device();
        preload.request(next.clone(), move |path| open_track(path, &caps, Policy::AllowAnything, DsdMode::Pcm, None));
        assert!(preload.take(&other, &StopSignal::new()).is_none());
        assert!(preload.take(&next, &StopSignal::new()).is_none());
    }

    /// A file opens at once; a track over the network fifteen seconds in,
    /// or halfway through one shorter than thirty.
    #[test]
    fn the_next_track_over_the_network_is_opened_once_this_one_plays_a_while() {
        assert_eq!(at(false, 44_100, Some(200.0)), None);
        assert_eq!(at(true, 44_100, Some(200.0)), Some(15 * 44_100));
        assert_eq!(at(true, 192_000, None), Some(15 * 192_000));
        assert_eq!(at(true, 44_100, Some(20.0)), Some(10 * 44_100));
    }

    /// Asked for later, a track is not opened before its time, is opened
    /// once it comes, and is forgotten when the queue moves on first.
    #[test]
    fn a_track_asked_for_later_opens_when_the_one_before_gets_there() {
        let next = wave("later");
        let mut preload = Preload::default();
        let caps = device();
        preload.request_at(next.clone(), Some(1_000), move |path| open_track(path, &caps, Policy::AllowAnything, DsdMode::Pcm, None));
        preload.reached(999);
        assert!(preload.pending.is_none(), "not before its time");
        preload.reached(1_000);
        assert!(preload.take(&next, &StopSignal::new()).expect("opened once it came").is_ok());

        let caps = device();
        preload.request_at(next.clone(), Some(1_000), move |path| open_track(path, &caps, Policy::AllowAnything, DsdMode::Pcm, None));
        assert!(preload.take(&next, &StopSignal::new()).is_none(), "taken before its time: opened by the queue instead");
        preload.reached(5_000);
        assert!(preload.pending.is_none(), "and forgotten");
    }

    #[test]
    fn a_track_that_cannot_open_says_so_when_taken() {
        let missing = std::env::temp_dir().join("chilpa-preload-missing.wav");
        let mut preload = Preload::default();
        let caps = device();
        preload.request(missing.clone(), move |path| open_track(path, &caps, Policy::AllowAnything, DsdMode::Pcm, None));
        assert!(matches!(preload.take(&missing, &StopSignal::new()), Some(Err(_))));
    }
}
