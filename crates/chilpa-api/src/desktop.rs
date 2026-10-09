//! The desktop around the window: its media controls, its tray icon and
//! the launch of a second Chilpa.
//!
//! What plays goes out to the desktop's widget, its media keys and the
//! tray's tooltip, and what they ask comes back, on every poll. The window
//! is asked only for what the facade cannot do: show itself, hide, and
//! close.

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
    /// Show if hidden, hide if in sight: the tray icon was clicked.
    Toggle,
    Quit,
    /// Quit, asking first when that is wanted: the tray's Quit.
    QuitAsked,
}

#[derive(Default)]
pub(crate) struct Desktop {
    #[cfg(target_os = "linux")]
    link: Option<(chilpa_mpris::Handle, async_channel::Receiver<MediaEvent>)>,
    /// Where the tray sends what it is asked, the same way as the controls.
    #[cfg(target_os = "linux")]
    tell: Option<async_channel::Sender<MediaEvent>>,
    #[cfg(target_os = "linux")]
    tray: Option<chilpa_mpris::tray::Handle>,
    /// What the tray's tooltip was last told.
    #[cfg(target_os = "linux")]
    tray_said: (String, String),
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

    /// Puts Chilpa on the desktop's media controls, where there are any,
    /// and its icon on the panel, if that is wanted.
    pub fn start_media_controls(&mut self) {
        #[cfg(target_os = "linux")]
        {
            let (tx, rx) = async_channel::unbounded();
            let tell = tx.clone();
            let handle = chilpa_mpris::spawn(move |event| {
                let _ = tell.try_send(event);
            });
            self.desktop.link = handle.map(|handle| (handle, rx));
            self.desktop.tell = Some(tx);
            if self.tray_wanted() {
                self.start_tray();
            }
        }
    }

    /// Whether the icon is on the panel now.
    pub fn tray_present(&self) -> bool {
        #[cfg(target_os = "linux")]
        return self.desktop.tray.is_some();
        #[cfg(not(target_os = "linux"))]
        false
    }

    /// Puts the icon on the panel, if it is not there already.
    pub(crate) fn start_tray(&mut self) {
        #[cfg(target_os = "linux")]
        {
            if self.desktop.tray.is_some() {
                return;
            }
            let Some(tell) = self.desktop.tell.clone() else { return };
            let glyph = crate::tray::glyph_of(&self.tray_glyph(), chilpa_mpris::tray::prefers_dark());
            let said = crate::tray::said(self.now.playing.then_some(&self.now), self.now.playing && !self.now.paused);
            let labels = tray_labels();
            self.desktop.tray = chilpa_mpris::tray::spawn(glyph, labels, chilpa_mpris::tray::Said { title: said.0.clone(), lines: said.1.clone() }, move |event| {
                let _ = tell.try_send(event);
            });
            self.desktop.tray_said = said;
        }
    }

    /// Takes the icon off the panel.
    pub(crate) fn stop_tray(&mut self) {
        #[cfg(target_os = "linux")]
        {
            self.desktop.tray = None;
        }
    }

    /// Shows the glyph the preference asks for now.
    pub(crate) fn recolor_tray(&mut self) {
        #[cfg(target_os = "linux")]
        if let Some(tray) = &self.desktop.tray {
            tray.recolor(crate::tray::glyph_of(&self.tray_glyph(), chilpa_mpris::tray::prefers_dark()));
        }
    }

    /// The tray's menu in the language of the moment.
    pub fn relabel_tray(&mut self) {
        #[cfg(target_os = "linux")]
        if let Some(tray) = &self.desktop.tray {
            tray.relabel(tray_labels());
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
            if let Some(tray) = &self.desktop.tray {
                let said = crate::tray::said(self.now.playing.then_some(&self.now), status == PlaybackStatus::Playing);
                if said != self.desktop.tray_said {
                    tray.say(chilpa_mpris::tray::Said { title: said.0.clone(), lines: said.1.clone() });
                    self.desktop.tray_said = said;
                }
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
            MediaEvent::ToggleWindow => self.desktop.requests.push_back(WindowRequest::Toggle),
            MediaEvent::VolumeBy(notches) => {
                let now = self.player_volume().percent;
                self.set_player_volume(crate::tray::volume_after(now, notches));
            }
            MediaEvent::Quit => self.desktop.requests.push_back(WindowRequest::Quit),
            MediaEvent::QuitAsked => self.desktop.requests.push_back(WindowRequest::QuitAsked),
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

/// The tray's menu, as it reads now.
#[cfg(target_os = "linux")]
fn tray_labels() -> chilpa_mpris::tray::Labels {
    chilpa_mpris::tray::Labels {
        play_pause: chilpa_i18n::t("Play/Pause"),
        next: chilpa_i18n::t("Next Track"),
        previous: chilpa_i18n::t("Previous Track"),
        show_hide: chilpa_i18n::t("Show/Hide Window"),
        quit: chilpa_i18n::t("Quit Chilpa"),
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
