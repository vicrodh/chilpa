//! What the player says, applied to the listens of the play that said it
//! and, while that play is the one in front of the window, to what the
//! window shows.

use std::path::Path;

use chilpa_core::attempts::How;
use chilpa_core::player::PlayerEvent;

use crate::plays::face::Face;
use crate::{channels, lyrics_service, now_unix, Chilpa, InProgress, Quality};

impl Chilpa {
    /// Reads what the player has said since the last time and applies it.
    /// Answers whether a track started in front of the window.
    pub(crate) fn read_player_events(&mut self) -> bool {
        let mut started = false;
        let events = self.player.as_ref().map(|p| p.poll_tagged()).unwrap_or_default();
        for (play, event) in events {
            self.apply_player_event(play, event, &mut started);
        }
        started
    }

    /// Applies one event the player said, by the play that said it. The
    /// listens are that play's own and are kept whatever became of it; the
    /// window, the marks and the queue are touched only while it is the play
    /// in front of the window. `started` is raised when a track started
    /// there.
    pub(crate) fn apply_player_event(&mut self, id: u64, event: PlayerEvent, started: &mut bool) {
        let Some(play) = self.plays.get(id) else {
            chilpa_core::trace_playback("facade", "event_of_no_play", format_args!("play={id}"));
            return;
        };
        let (fate, from, had_started) = (play.fate, play.from, play.started);
        let shown = self.play_shown(id);
        match event {
            PlayerEvent::Started { path, opened, route, channels: music_channels, bits, bit_perfect, total_frames, tags, artwork, attempt, index, queue_len, dsd_multiple, dsd_passthrough } => {
                let rate = opened.sample_rate_hz;
                // One face for the window and for the listen.
                let face = self.face_of(id, &path, tags.as_ref(), artwork.as_deref());
                if shown {
                    *started = true;
                    if let Some(play) = self.plays.get_mut(id) {
                        play.started = true;
                    }
                    self.show_started(&path, &face, opened, route, music_channels, bits, bit_perfect, total_frames, index, queue_len, dsd_multiple, dsd_passthrough);
                } else {
                    chilpa_core::trace_playback("facade", "started_behind_the_window", format_args!("play={id} fate={fate:?} index={index}"));
                }
                self.listen_started(id, attempt, &path, tags.as_ref(), &face, rate, bits, bit_perfect, total_frames, shown);
            }
            PlayerEvent::StreamTitle { title } if shown => {
                // On a radio the title is sent by the station and
                // changes with every song. A station that was browsed
                // keeps its name beside the song, and gets it back when
                // the station sends an empty title between songs.
                match self.station_on() {
                    Some(station) if title.trim().is_empty() => self.now.title = station,
                    Some(station) => {
                        self.song_changed(&title);
                        self.now.title = title;
                        self.now.artist = station;
                    }
                    None => self.now.title = title,
                }
            }
            PlayerEvent::StreamTitle { .. } => {}
            PlayerEvent::Progress { frames, played } => {
                if let Some(listen) = self.plays.get_mut(id).and_then(|play| play.ledger.progress(played)) {
                    crate::scrobble::if_heard(self.scrobbler.as_ref(), listen);
                }
                if shown {
                    self.now.frames = frames;
                    if self.sample_rate > 0 {
                        self.now.seconds = frames as f64 / f64::from(self.sample_rate);
                    }
                    self.now.lyrics_line = self.line_at(self.now.seconds);
                }
            }
            // A track was passed over and the queue went on. One that broke
            // partway had started: its listen is cut short, whenever it is
            // closed.
            PlayerEvent::Skipped { attempt, index, path, message } => {
                if let Some(play) = self.plays.get_mut(id) {
                    play.ledger.known(attempt, How::Broken);
                }
                chilpa_core::trace_playback("facade", "unavailable_passed", format_args!("play={id} index={index} shown={shown} why={message:?}"));
                if shown {
                    self.player_skipped(index, &path, message);
                }
            }
            // A disc's track cut by a sector that would not read, while the
            // disc goes on: said, and the track's listen is cut short.
            PlayerEvent::Notice { attempt, message } => {
                if let Some(play) = self.plays.get_mut(id) {
                    play.ledger.known(attempt, How::Broken);
                }
                if shown {
                    self.now.error = message;
                }
            }
            // A track that started is over: how it ended and what of it left
            // the device settle its listen.
            PlayerEvent::Closed { attempt, heard, how, .. } => {
                if let Some(listen) = self.plays.get_mut(id).and_then(|play| play.ledger.listen_mut(attempt)) {
                    listen.played_frames = heard;
                    crate::scrobble::if_heard(self.scrobbler.as_ref(), listen);
                }
                let written = self.plays.get_mut(id).map(|play| play.ledger.closed(attempt, heard, how, fate)).unwrap_or_default();
                for written in written {
                    self.record(written, shown);
                }
            }
            PlayerEvent::Finished { frames } => {
                if shown { self.reset_channel_consent(); }
                self.settle_play(id, shown);
                if shown {
                    self.now.frames = frames;
                    if self.sample_rate > 0 {
                        self.now.seconds = frames as f64 / f64::from(self.sample_rate);
                    }
                    self.keep_place(true);
                    self.now.playing = false;
                    self.finish_queue();
                }
            }
            PlayerEvent::Stopped { frames } => {
                if shown {
                    self.reset_channel_consent();
                    // A play that never started says it stopped with nothing
                    // played: that is not where the window was.
                    if had_started {
                        self.now.frames = frames;
                    }
                    self.now.playing = false;
                    self.stop_after = None;
                    self.sleep_until = None;
                }
                self.settle_play(id, shown);
            }
            PlayerEvent::ChannelConversionRequired { output,path,index,requested,offered } => {
                if shown && !self.offer_channel_conversion(output,&path,index,requested,&offered) {
                    self.now.playing=false;
                    self.now.error=chilpa_i18n::t("Stereo conversion is not available for this output.");
                }
                self.settle_play(id,shown);
            }
            PlayerEvent::Failed { message } => {
                if shown {
                    self.reset_channel_consent();
                    self.player_failed(message, from, had_started);
                }
                self.settle_play(id, shown);
            }
        }
    }

    /// A track started in the play `id`: the listen carried in from another
    /// output goes on if it is this track's, and a new one is opened
    /// otherwise, named by the track's `face` whether the play is in front
    /// of the window or not. Only one in front of it is told to the services
    /// as what plays now.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn listen_started(&mut self, id: u64, attempt: u64, path: &Path, tags: Option<&chilpa_tags::Tags>, face: &Face, rate: u32, bits: Option<u8>, bit_perfect: bool, total_frames: Option<u64>, shown: bool) {
        let Some(play) = self.plays.get_mut(id) else { return };
        let began = play.ledger.started(attempt, path);
        if began.resumed {
            if let Some(listen) = play.ledger.listen_mut(attempt) {
                listen.picked_up(rate, bits, bit_perfect);
            }
            return;
        }
        // It was carried here for a track that did not start.
        if let Some(written) = began.written {
            self.record(written, shown);
        }
        let started_at = now_unix();
        let duration_ms = match total_frames {
            Some(frames) if rate > 0 => frames * 1000 / u64::from(rate),
            _ => 0,
        };
        let mut context = self.context_at_start(path, tags, started_at, face.cover.clone());
        // Empty or path-derived fields keep the captured library/tag identity.
        for (saved, sourced) in [&mut context.title, &mut context.artist, &mut context.album].into_iter().zip(face.sourced()) {
            if !sourced.is_empty() {
                *saved = sourced.to_string();
            }
        }
        let listen = crate::scrobble::listen_of(tags, &context.title, &context.artist, &context.album, duration_ms, started_at);
        if let (true, Some(listen)) = (shown, &listen) {
            self.announce(listen);
        }
        self.home.changed();
        let Some(play) = self.plays.get_mut(id) else { return };
        play.ledger.open(attempt, InProgress {
            context,
            path: path.to_path_buf(),
            carried_ms: 0,
            started_at,
            sample_rate_hz: rate,
            // I log signal bits, not the DAC's wider transport word.
            bit_depth: bits,
            bit_perfect,
            duration_ms,
            played_frames: 0,
            listen,
            scrobbled: false,
        });
    }

    /// A track started in front of the window: what the window shows of it.
    #[allow(clippy::too_many_arguments)]
    fn show_started(&mut self, path: &Path, face: &Face, opened: chilpa_core::output_evidence::StreamFormat, route: chilpa_core::output_evidence::Route, music_channels: u16, bits: Option<u8>, bit_perfect: bool, total_frames: Option<u64>, index: usize, queue_len: usize, dsd_multiple: Option<u32>, dsd_passthrough: bool) {
        let requested_rate = route.delivery.requested.sample_rate_hz;
        self.audio_output.route = Some(route);
        self.dsp.passthrough = dsd_passthrough;
        // What was playing until now is left where it was.
        self.keep_place(true);
        self.sample_rate = opened.sample_rate_hz;
        self.now.path = path.display().to_string();
        // What came from a source already carries its card: without it the
        // window would show the raw URL, which is all the decoder knows.
        self.now.artwork_path = face.cover.clone();
        self.now.title = face.title.clone();
        self.now.artist = face.artist.clone();
        self.now.album = face.album.clone();
        self.now.ai_generated = face.ai_generated;
        self.now.live = face.live;
        self.load_album_context();
        if dsd_multiple.is_none() && crate::source_opener::of_plugin(&self.now.path).is_some() {
            // What a source's track turned out to be, measured, for its row.
            self.catalog.measured(&self.now.path, chilpa_core::remote::opened_codec(&self.now.path), bits, opened.sample_rate_hz);
        }
        self.now.playing = true;
        self.now.bit_perfect = bit_perfect;
        // On DSD the rate that names it is the one-bit source
        // rate, not the DoP carrier or native U32 frame rate.
        let rate = match dsd_multiple {
            Some(multiple) => multiple * 44_100,
            None => requested_rate,
        };
        let codec = self.remote_codec(&self.now.path.clone());
        self.now.quality = Quality::of_track(&self.now.path, codec.as_deref(), bits, Some(rate));
        self.load_lyrics(path, lyrics_service::length_ms(total_frames, opened.sample_rate_hz));
        self.load_analysis(path);
        // The player counts over what it was given; the view
        // counts over the whole queue.
        self.now.index = self.base + index;
        self.now.queue_len = if self.queue.is_empty() { queue_len } else { self.queue.len() };
        self.look_past_unavailable();
        self.now.total_seconds = match (total_frames, opened.sample_rate_hz) {
            (Some(f), rate) if rate > 0 => f as f64 / f64::from(rate),
            _ => 0.0,
        };
        // The channels are the music's: a mono track that goes
        // out in both channels of a stereo DAC is still mono.
        self.now.format = match (dsd_multiple, opened.sample_format.is_dsd(), dsd_passthrough) {
            (Some(multiple), true, _) => chilpa_i18n::t_args("DSD{} · Native at {} Hz · {}", &[&multiple.to_string(), &opened.sample_rate_hz.to_string(), &channels(music_channels)]),
            // On DSD what matters is the DSD rate; the DoP
            // carrier is stated separately, because it is the
            // format the device was opened with.
            (Some(multiple), false, true) => chilpa_i18n::t_args("DSD{} · DoP at {} Hz · {}", &[&multiple.to_string(), &opened.sample_rate_hz.to_string(), &channels(music_channels)]),
            (Some(multiple), false, false) => chilpa_i18n::t_args("DSD{} · PCM at {} Hz · {}", &[&multiple.to_string(), &opened.sample_rate_hz.to_string(), &channels(music_channels)]),
            (None, _, _) => match bits {
                Some(bits) => format!("{} Hz · {} bits · {}", opened.sample_rate_hz, bits, channels(music_channels)),
                // A lossy track has no bit depth, and the word it
                // was rounded into is not one: "32 bits" on an
                // MP3 reads as more than it is.
                None => format!("{} Hz · {}", opened.sample_rate_hz, channels(music_channels)),
            },
        };
    }
}
