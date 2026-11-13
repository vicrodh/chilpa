//! What the player says, applied to what the window shows and to the listen
//! that is open.

use chilpa_core::player::PlayerEvent;

use crate::{channels, lyrics_service, now_unix, Chilpa, InProgress, Quality, Relaunch};

impl Chilpa {
    /// Applies one event the player said, by the play that said it.
    pub(crate) fn apply_player_event(&mut self, generation: u64, event: PlayerEvent, started: &mut bool) {
        if self.listen.active == 0 || self.from_replaced_play(generation, &event) {
            return;
        }
        self.player_event(generation, event, started);
    }

    /// Applies one event of the play in front of the window. `started` is
    /// raised when a track started. The listen that is open and the mark of
    /// a relaunch each belong to one play: what another play says changes
    /// neither (`queue/replaced.rs`).
    fn player_event(&mut self, generation: u64, event: PlayerEvent, started: &mut bool) {
    match event {
        PlayerEvent::Started { path, opened, route, channels: music_channels, bits, bit_perfect, total_frames, tags, artwork, index, queue_len, dsd_multiple, dsd_passthrough } => {
            let requested_rate = route.delivery.requested.sample_rate_hz;
            self.audio_output.route = Some(route);
            self.dsp.passthrough = dsd_passthrough;
            // What was playing until now is left where it was.
            self.keep_place(true);
            *started = true;
            self.sample_rate = opened.sample_rate_hz;
            self.now.path = path.display().to_string();
            let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            self.now.artwork_path = artwork.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
            match &tags {
                Some(t) => {
                    self.now.title = t.title.clone().unwrap_or(name);
                    self.now.artist = t.artist_line();
                    self.now.album = t.album.clone().unwrap_or_default();
                }
                None => {
                    self.now.title = name;
                    self.now.artist = String::new();
                    self.now.album = String::new();
                }
            }
            // What came from a source already carries its card from
            // the search. Without this the window would show the raw
            // URL, which is all the decoder knows about it.
            self.name_from_source(&path);
            self.now.live = self.name_from_entry(&path);
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
            self.load_lyrics(&path, lyrics_service::length_ms(total_frames, opened.sample_rate_hz));
            self.load_analysis(&path);
            // The player counts over what it was given; the view
            // counts over the whole queue.
            self.now.index = self.base + index;
            self.now.queue_len = if self.queue.is_empty() { queue_len } else { self.queue.len() };
            self.look_past_unavailable();
            // Every start closes the previous one: in a queue, the
            // track that starts is the proof the one before it
            // finished.
            let started_at = now_unix();
            let duration_ms = match (total_frames, opened.sample_rate_hz) {
                (Some(f), rate) if rate > 0 => f * 1000 / u64::from(rate),
                _ => 0,
            };
            if !self.resume_listen(&path, opened.sample_rate_hz, bits, bit_perfect, generation) {
                self.log_play(false);
                let listen = self.announce(tags.as_ref(), duration_ms, started_at);
                let context = self.context_at_start(&path, tags.as_ref(), started_at);
                self.home.changed();
                self.in_progress = Some(InProgress {
                    context,
                    path: path.clone(),
                    play: generation,
                    resuming: false,
                    carried_ms: 0,
                    started_at,
                    sample_rate_hz: opened.sample_rate_hz,
                    // I log signal bits, not the DAC's wider transport word.
                    bit_depth: bits,
                    bit_perfect,
                    duration_ms,
                    played_frames: 0,
                    listen,
                    scrobbled: false,
                });
            }
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
        PlayerEvent::StreamTitle { title } => {
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
        PlayerEvent::Progress { frames, played } => {
            self.now.frames = frames;
            if self.owns_listen(generation) {
                self.set_played(played);
            }
            if self.sample_rate > 0 {
                self.now.seconds = frames as f64 / f64::from(self.sample_rate);
            }
            self.now.lyrics_line = self.line_at(self.now.seconds);
        }
        PlayerEvent::Finished { frames } => {
            // What the renderer counted: what went out, not where
            // the track was.
            if self.owns_listen(generation) {
                self.set_played(frames);
            }
            self.now.frames = frames;
            if self.sample_rate > 0 {
                self.now.seconds = frames as f64 / f64::from(self.sample_rate);
            }
            self.keep_place(true);
            self.now.playing = false;
            self.finish_queue();
        }
        PlayerEvent::Stopped { frames } => {
            // A play that never started says it stopped with nothing played:
            // that is not the count of a listen another play left open to be
            // picked up, nor where the window was.
            if self.owns_listen(generation) {
                self.set_played(frames);
                self.now.frames = frames;
            }
            // A stop we asked for in order to start again is not the
            // listener stopping. Saying `playing = false` here would
            // blink the window to stopped on its way to the new
            // device, and the play that is already queued behind this
            // event is what decides what happens next.
            match self.take_relaunch(generation) {
                // Going back: the track that was left is a skip,
                // because somebody chose to leave it.
                Some(Relaunch::Track) => self.log_play(true),
                // I carry the delivered time into the next output segment.
                Some(Relaunch::Output) => {
                    if let Some(current) = self.in_progress.as_mut() { current.resuming = true; }
                }
                None => {
                    self.now.playing = false;
                    self.stop_after = None;
                    self.sleep_until = None;
                    // Stopping mid track counts as a skip.
                    self.log_play(true);
                }
            }
        }
        PlayerEvent::Failed { message } => self.player_failed(message),
        // A disc's track cut by a sector that would not read: handled as the
        // failure it used to be said as, while the disc goes on.
        PlayerEvent::Notice { message, .. } => self.player_failed(message),
        PlayerEvent::Skipped { generation, index, path, message, played } => self.player_skipped(generation, index, &path, message, played),
    }
    }
}
