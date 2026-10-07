//! The next track, opened while the one before it still plays.
//!
//! Opening a track reads its file: the header, the tags, the cover. On a
//! network share that is a few hundred milliseconds, and with gapless on
//! the device is running all that while on what it holds, which is a third
//! of a second at most. Opened ahead, on a thread of its own, the next
//! track is ready before the one before it ends, and the device never
//! waits. A track that turns out not to be next is dropped unplayed.

use crate::playback::PlayError;
use crate::queue_playback::Opened;
use chilpa_audio::sink::StopSignal;
use std::{path::{Path, PathBuf}, sync::{Arc, atomic::{AtomicBool, Ordering}, mpsc}, time::Duration};

#[derive(Default)]
pub(crate) struct Preload {
    pending: Option<(PathBuf, mpsc::Receiver<Result<Opened, PlayError>>)>,
    busy: Arc<AtomicBool>,
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

    /// The track opened ahead, if it is this one, waiting for it to finish
    /// opening; `None` when another was opened, or none.
    pub fn take(&mut self, path: &Path, stop: &StopSignal) -> Option<Result<Opened, PlayError>> {
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
        preload.request(next.clone(), move |path| open_track(path, &caps, Policy::AllowAnything, DsdMode::Pcm, false));
        let opened = preload.take(&next, &StopSignal::new()).expect("it was asked for").expect("the file opens");
        assert_eq!(opened.about.frames, Some(44_100), "what it says of itself came along");
        assert_eq!(opened.delivery.opened.sample_rate_hz, 44_100);

        // Another track is not the one opened ahead, and the one opened
        // ahead is not kept for later.
        let caps = device();
        preload.request(next.clone(), move |path| open_track(path, &caps, Policy::AllowAnything, DsdMode::Pcm, false));
        assert!(preload.take(&other, &StopSignal::new()).is_none());
        assert!(preload.take(&next, &StopSignal::new()).is_none());
    }

    #[test]
    fn a_track_that_cannot_open_says_so_when_taken() {
        let missing = std::env::temp_dir().join("chilpa-preload-missing.wav");
        let mut preload = Preload::default();
        let caps = device();
        preload.request(missing.clone(), move |path| open_track(path, &caps, Policy::AllowAnything, DsdMode::Pcm, false));
        assert!(matches!(preload.take(&missing, &StopSignal::new()), Some(Err(_))));
    }
}
