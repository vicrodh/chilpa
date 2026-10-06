//! The desktop around the window: its media controls and the launch of a
//! second Chilpa.
//!
//! What plays goes out to the desktop's widget and media keys, and what
//! they ask comes back, on every poll. The window is asked only for what
//! the facade cannot do: show itself, and close.

use crate::Chilpa;
#[cfg(target_os = "linux")]
use crate::NowPlaying;
#[cfg(target_os = "linux")]
use chilpa_mpris::{MediaEvent, PlaybackStatus, TrackMeta};
use std::collections::VecDeque;
#[cfg(target_os = "linux")]
use std::time::Duration;

/// What the desktop asks of the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowRequest {
    /// Come to the front: the widget was clicked, or Chilpa was launched
    /// again.
    Raise,
    Quit,
}

#[derive(Default)]
pub(crate) struct Desktop {
    #[cfg(target_os = "linux")]
    link: Option<(chilpa_mpris::Handle, async_channel::Receiver<MediaEvent>)>,
    /// What the desktop was last told, to tell it only what changed.
    #[cfg(target_os = "linux")]
    told: (Option<TrackMeta>, Option<PlaybackStatus>),
    /// A jump the desktop has not heard of yet, in seconds.
    pub(crate) seeked: Option<f64>,
    requests: VecDeque<WindowRequest>,
}

/// The cover as an address the desktop can open.
#[cfg(any(target_os = "linux", test))]
fn art_url(path: &str) -> Option<String> {
    if path.is_empty() {
        None
    } else if path.contains("://") {
        Some(path.to_owned())
    } else {
        url::Url::from_file_path(path).ok().map(String::from)
    }
}

#[cfg(target_os = "linux")]
fn meta_of(now: &NowPlaying) -> TrackMeta {
    TrackMeta {
        title: now.title.clone(),
        artist: now.artist.clone(),
        album: now.album.clone(),
        duration: (now.total_seconds > 0.0).then(|| Duration::from_secs_f64(now.total_seconds)),
        art_url: art_url(&now.artwork_path),
        url: None,
    }
}

impl Chilpa {
    /// Whether this is the only Chilpa open. A second launch asks the first
    /// to show its window and is told `false`: it should end there, before
    /// it opens anything.
    pub fn only_instance() -> bool {
        #[cfg(target_os = "linux")]
        return chilpa_mpris::instance::acquire();
        #[cfg(not(target_os = "linux"))]
        true
    }

    /// Puts Chilpa on the desktop's media controls, where there are any.
    pub fn start_media_controls(&mut self) {
        #[cfg(target_os = "linux")]
        {
            let (tx, rx) = async_channel::unbounded();
            let handle = chilpa_mpris::spawn(move |event| {
                let _ = tx.try_send(event);
            });
            self.desktop.link = handle.map(|handle| (handle, rx));
        }
    }

    /// What the desktop asks of the window, one request at a time.
    pub fn window_request(&mut self) -> Option<WindowRequest> {
        #[cfg(target_os = "linux")]
        if chilpa_mpris::instance::take_present() {
            self.desktop.requests.push_back(WindowRequest::Raise);
        }
        self.desktop.requests.pop_front()
    }

    /// Does what the desktop asked and tells it what plays now.
    pub(crate) fn advance_desktop(&mut self) {
        #[cfg(target_os = "linux")]
        {
            let Some((_, asked)) = &self.desktop.link else { return };
            let events: Vec<_> = std::iter::from_fn(|| asked.try_recv().ok()).collect();
            for event in events {
                self.desktop_asked(event);
            }
            let status = match (self.now.playing, self.now.paused) {
                (false, _) => PlaybackStatus::Stopped,
                (true, true) => PlaybackStatus::Paused,
                (true, false) => PlaybackStatus::Playing,
            };
            let meta = self.now.playing.then(|| meta_of(&self.now));
            let position = Duration::from_secs_f64(self.now.seconds.max(0.0));
            let Desktop { link: Some((handle, _)), told, seeked, .. } = &mut self.desktop else { return };
            let another = meta != told.0;
            if another {
                if let Some(meta) = &meta {
                    handle.set_metadata(meta);
                }
                told.0 = meta;
            }
            if another || told.1 != Some(status) {
                handle.set_playback(status, (status != PlaybackStatus::Stopped).then_some(position));
                told.1 = Some(status);
                *seeked = None;
            } else if let Some(to) = seeked.take() {
                handle.seeked(Duration::from_secs_f64(to.max(0.0)));
            } else if status == PlaybackStatus::Playing {
                handle.set_position(position);
            }
        }
    }

    #[cfg(target_os = "linux")]
    fn desktop_asked(&mut self, event: MediaEvent) {
        let sounding = self.now.playing && !self.now.paused;
        match event {
            // Both arrive unasked too: a desktop pauses a paused player
            // before it suspends.
            MediaEvent::Play if !sounding => self.set_paused(false),
            MediaEvent::Pause if sounding => self.set_paused(true),
            MediaEvent::Play | MediaEvent::Pause => {}
            MediaEvent::Toggle => self.set_paused(sounding),
            MediaEvent::Next => self.next_track(),
            MediaEvent::Previous => {
                let (device, bit_perfect) = self.chosen_output();
                self.previous_track(&device, bit_perfect);
            }
            MediaEvent::Stop => self.stop(),
            MediaEvent::SeekBy(micros) => self.desktop_seek(self.now.seconds + micros as f64 / 1e6),
            MediaEvent::SetPosition(micros) => self.desktop_seek(micros as f64 / 1e6),
            MediaEvent::SetVolume(volume) => self.set_player_volume((volume.clamp(0.0, 1.0) * 100.0).round() as u8),
            MediaEvent::Raise => self.desktop.requests.push_back(WindowRequest::Raise),
            MediaEvent::Quit => self.desktop.requests.push_back(WindowRequest::Quit),
        }
    }

    /// A jump asked from outside, kept inside the track. Without a known
    /// length there is nowhere to jump to.
    #[cfg(target_os = "linux")]
    fn desktop_seek(&mut self, to: f64) {
        if self.now.total_seconds > 0.0 {
            self.seek(to.clamp(0.0, self.now.total_seconds));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::art_url;

    #[test]
    fn a_cover_on_disk_goes_out_as_a_file_address() {
        assert_eq!(art_url(""), None, "no cover, no property");
        assert_eq!(art_url("/covers/a b#1.jpg").as_deref(), Some("file:///covers/a%20b%231.jpg"));
        assert_eq!(art_url("https://covers.example/1.jpg").as_deref(), Some("https://covers.example/1.jpg"));
        assert_eq!(art_url("covers/1.jpg"), None, "a path that is not whole is not an address");
    }
}
