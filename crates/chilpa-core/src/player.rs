//! The player: it plays on its own thread and reports what happens.
//!
//! The interface never blocks. It sends an order, goes on with its own
//! business and picks up events as they come. Stopping raises a flag that
//! the delivery loop checks between blocks, so it does not wait on the
//! device.

use chilpa_audio::negotiate::{DeviceCaps, Policy};
use chilpa_audio::render::Outcome;
use chilpa_audio::sink::{AudioSink, SinkError, StopSignal};
use chilpa_audio::format::StreamFormat;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::thread::JoinHandle;

use crate::playback::PlayError;

/// What the player tells whoever controls it.
#[derive(Clone, PartialEq, Debug)]
pub enum PlayerEvent {
    /// It started playing: what format the device was opened with and
    /// whether the delivery is bit-perfect.
    Started {
        path: PathBuf,
        opened: StreamFormat,
        bit_perfect: bool,
        total_frames: Option<u64>,
        /// What the file says about itself, when it could be read.
        tags: Option<chilpa_tags::Tags>,
        /// The cover already extracted to disk, when the file carries one.
        artwork: Option<PathBuf>,
        /// Its place in the queue, and out of how many.
        index: usize,
        queue_len: usize,
        /// For DSD: the multiple that names it (64, 128...). `None` on PCM.
        dsd_multiple: Option<u32>,
    },
    /// Frames delivered so far.
    Progress { frames: u64 },
    /// What the station says is playing.
    StreamTitle { title: String },
    /// It finished naturally.
    Finished { frames: u64 },
    /// It stopped because someone asked.
    Stopped { frames: u64 },
    /// Something failed, in text the interface can show.
    Failed { message: String },
}

enum Order {
    Play { paths: Vec<PathBuf>, device: String, bit_perfect: bool },
    Stop,
    Quit,
}

/// Opens the output device. It is injected so the tests use a fake one and
/// the application uses the real one.
pub type OpenSink = Box<dyn FnMut(&str, StreamFormat) -> Result<Box<dyn AudioSink + Send>, SinkError> + Send>;

/// Measures a device by its name.
pub type ProbeDevice = Box<dyn FnMut(&str) -> Result<DeviceCaps, SinkError> + Send>;

/// The player, as the interface sees it.
pub struct Player {
    orders: Sender<Order>,
    events: Receiver<PlayerEvent>,
    controls: crate::queue_playback::Controls,
    /// The queue that is playing. It is shared with the thread so things
    /// can be added without cutting off what is playing.
    queue: crate::queue_playback::SharedQueue,
    thread: Option<JoinHandle<()>>,
}

impl Player {
    /// Starts the player thread.
    pub fn spawn(mut probe: ProbeDevice, mut open: OpenSink) -> Self {
        let (orders_tx, orders_rx) = channel::<Order>();
        let (events_tx, events_rx) = channel::<PlayerEvent>();
        let controls = crate::queue_playback::Controls::new();
        let controls_thread = controls.clone();
        let queue = crate::queue_playback::SharedQueue::default();
        let queue_thread = queue.clone();
        let stop_thread = controls.stop.clone();
        let pause_thread = controls.pause.clone();
        let skip_thread = controls.skip.clone();

        let thread = std::thread::Builder::new()
            .name("chilpa-player".into())
            .spawn(move || {
                while let Ok(order) = orders_rx.recv() {
                    match order {
                        Order::Quit => break,
                        Order::Stop => {}
                        Order::Play { paths, device, bit_perfect } => {
                            // Every track starts with the flag clear: an old
                            // stop order does not kill the next one.
                            stop_thread.reset();
                            pause_thread.set(false);
                            skip_thread.take();
                            queue_thread.set(paths.clone());
                            let result = play(
                                &queue_thread,
                                &device,
                                bit_perfect,
                                &controls_thread,
                                &mut probe,
                                &mut open,
                                &events_tx,
                            );
                            let event = match result {
                                Ok(Outcome::Finished { frames }) => PlayerEvent::Finished { frames },
                                Ok(Outcome::Stopped { frames }) => PlayerEvent::Stopped { frames },
                                Ok(Outcome::Failed { frames, error }) => PlayerEvent::Failed {
                                    message: chilpa_i18n::t_args(
                    "the device failed after {} frames: {}",
                    &[&frames.to_string(), &format!("{error:?}")],
                ),
                                },
                                Err(e) => PlayerEvent::Failed { message: e.to_string() },
                            };
                            let _ = events_tx.send(event);
                        }
                    }
                }
            })
            .expect("the player thread starts");

        Self { orders: orders_tx, events: events_rx, controls, queue, thread: Some(thread) }
    }

    /// Asks to play a file. Returns immediately.
    pub fn play(&self, path: &Path, device: &str, bit_perfect: bool) {
        self.play_all(&[path.to_path_buf()], device, bit_perfect);
    }

    /// Asks to play several tracks in order, one after another.
    pub fn play_all(&self, paths: &[PathBuf], device: &str, bit_perfect: bool) {
        let _ = self.orders.send(Order::Play {
            paths: paths.to_vec(),
            device: device.to_string(),
            bit_perfect,
        });
    }

    /// Pauses or resumes. The track does not lose its place.
    pub fn set_paused(&self, paused: bool) {
        self.controls.pause.set(paused);
    }

    pub fn is_paused(&self) -> bool {
        self.controls.pause.is_paused()
    }

    /// Skips to the next track in the queue. The current one ends as if it
    /// had finished, so the queue advances instead of stopping.
    pub fn next(&self) {
        self.controls.skip.skip();
    }

    /// Asks to seek to that point of the playing track. The seek is handled
    /// between blocks; asking does not block.
    pub fn seek(&self, position: std::time::Duration) {
        self.controls.seek.request(position);
    }

    /// Changes repeat. It takes effect when the playing track ends.
    pub fn set_repeat(&self, repeat: crate::queue_playback::Repeat) {
        self.controls.repeat.set(repeat);
    }

    pub fn repeat(&self) -> crate::queue_playback::Repeat {
        self.controls.repeat.get()
    }

    /// Adds tracks to the end of the playing queue, without cutting it off.
    ///
    /// Returns how many are left in the queue. If nothing is playing, the
    /// shared queue is empty and the caller decides whether to start it.
    pub fn enqueue(&self, paths: &[PathBuf]) -> usize {
        self.queue.extend(paths)
    }

    /// Removes a track from the playing queue, by its position within what
    /// was given to the player. The one already playing is not removed.
    pub fn remove_from_queue(&self, index: usize) -> bool {
        self.queue.remove(index)
    }

    /// How many tracks the playing queue carries.
    pub fn queue_len(&self) -> usize {
        self.queue.len()
    }

    /// Shuffles the playing queue. It takes effect when the track ends.
    pub fn shuffle(&self, len: usize) {
        self.controls.order.set(crate::queue_playback::shuffled_now(len), len);
    }

    /// Sets a traversal order for the playing queue.
    pub fn set_order(&self, order: Vec<usize>, len: usize) -> bool {
        self.controls.order.set(order, len)
    }

    /// Goes back to the order things were queued in.
    pub fn clear_order(&self) {
        self.controls.order.clear();
    }

    /// Asks to stop. It does not wait on the device.
    pub fn stop(&self) {
        self.controls.stop.stop();
        let _ = self.orders.send(Order::Stop);
    }

    /// Whatever events there are, without blocking.
    pub fn poll(&self) -> Vec<PlayerEvent> {
        let mut out = Vec::new();
        loop {
            match self.events.try_recv() {
                Ok(e) => out.push(e),
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            }
        }
        out
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.controls.stop.stop();
        let _ = self.orders.send(Order::Quit);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn play(
    queue: &crate::queue_playback::SharedQueue,
    device: &str,
    bit_perfect: bool,
    controls: &crate::queue_playback::Controls,
    probe: &mut ProbeDevice,
    open: &mut OpenSink,
    events: &Sender<PlayerEvent>,
) -> Result<Outcome, PlayError> {
    let (stop, pause, skip) = (&controls.stop, &controls.pause, &controls.skip);
    let paths = queue.to_vec();
    let paths = paths.as_slice();
    let caps = probe(device).map_err(PlayError::Sink)?;
    let policy = if bit_perfect { Policy::BitPerfect } else { Policy::AllowAnything };
    let total = paths.len();

    // DSD does not go through the PCM decoder: it goes over DoP, and
    // today it plays one track at a time.
    if let Some(first) = paths.first() {
        if is_url(first) {
            return play_stream(first, device, &caps, policy, stop, pause, open, events);
        }
        if is_cd(first) {
            return play_cd(first, device, &caps, stop, pause, skip, open, events);
        }
        if is_sacd(first) {
            return play_sacd(first, device, &caps, stop, pause, skip, open, events);
        }
        if is_dsd(first) {
            return play_dsd(first, device, &caps, stop, pause, open, events);
        }
    }
    let results = crate::queue_playback::play_queue_with(
        queue,
        &caps,
        policy,
        controls,
        |format| open(device, format),
        &mut |index, path, delivery| {
            // Tags and the cover are read as each track starts, not as it is
            // queued: a long queue must not read hundreds of files before the
            // first one plays.
            let tags = chilpa_tags::read(path).ok().map(|info| info.tags);
            let artwork = chilpa_tags::extract_artwork(path, &crate::artwork_cache()).ok().flatten();
            let total_frames = chilpa_decode::Decoded::open(path).ok().and_then(|d| d.frame_count());
            let _ = events.send(PlayerEvent::Started {
                path: path.to_path_buf(),
                opened: delivery.opened,
                bit_perfect: delivery.is_bit_perfect(),
                total_frames,
                tags,
                artwork,
                index: index,
                queue_len: total,
                dsd_multiple: None,
            });
        },
        &mut |_, frames| {
            let _ = events.send(PlayerEvent::Progress { frames });
        },
    )?;

    // The end of the queue is the end of its last track.
    Ok(results
        .last()
        .map(|r| r.outcome.clone())
        .unwrap_or(Outcome::Finished { frames: 0 }))
}

/// True if what was asked for is a URL and not a file.
fn is_url(path: &Path) -> bool {
    path.to_string_lossy().starts_with("http://") || path.to_string_lossy().starts_with("https://")
}

/// Plays a radio, or any stream over HTTP.
#[allow(clippy::too_many_arguments)]
fn play_stream(
    url: &Path,
    device: &str,
    caps: &chilpa_audio::negotiate::DeviceCaps,
    policy: Policy,
    stop: &StopSignal,
    pause: &chilpa_audio::sink::PauseSignal,
    open: &mut OpenSink,
    events: &Sender<PlayerEvent>,
) -> Result<Outcome, PlayError> {
    let address = url.to_string_lossy().to_string();
    let stream = chilpa_stream::open(&address)
        .map_err(|e| PlayError::Sink(SinkError::Backend(e.to_string())))?;
    let titles = stream.now_playing();
    let station = stream.info().name.clone();

    let path = url.to_path_buf();
    let events = events.clone();
    let tag = chilpa_tags::Tags {
        title: station.clone().or_else(|| Some(address.clone())),
        album: Some("Radio".into()),
        ..chilpa_tags::Tags::default()
    };

    let played = crate::playback::play_stream(
        stream,
        caps,
        policy,
        stop,
        pause,
        |format| open(device, format),
        &mut |delivery| {
            let _ = events.send(PlayerEvent::Started {
                path: path.clone(),
                opened: delivery.opened,
                bit_perfect: delivery.is_bit_perfect(),
                total_frames: None,
                tags: Some(tag.clone()),
                artwork: None,
                index: 0,
                queue_len: 1,
                dsd_multiple: None,
            });
        },
        &mut |frames| {
            // The station title changes with every song; it is published
            // with the progress, which is the only thing that travels often.
            if let Some(title) = titles.get() {
                let _ = events.send(PlayerEvent::StreamTitle { title: title });
            }
            let _ = events.send(PlayerEvent::Progress { frames });
        },
    )?;
    Ok(played.outcome)
}

/// True if the file is a SACD image.
fn is_sacd(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("iso"))
        .unwrap_or(false)
}

/// Plays the tracks of a SACD image's stereo area, in order.
#[allow(clippy::too_many_arguments)]
/// A CD is asked for as `cdda:/dev/sr0`. The prefix is not decoration: a
/// bare device node is indistinguishable from a file, and opening the drive
/// by mistake startles anyone.
fn is_cd(path: &Path) -> bool {
    path.to_string_lossy().starts_with("cdda:")
}

/// The drive a `cdda:` path asks for.
pub fn cd_device_of(path: &Path) -> Option<PathBuf> {
    let s = path.to_string_lossy();
    let node = s.strip_prefix("cdda:")?;
    (!node.is_empty()).then(|| PathBuf::from(node))
}

#[cfg(target_os = "linux")]
fn play_cd(
    path: &Path,
    device: &str,
    caps: &chilpa_audio::negotiate::DeviceCaps,
    stop: &StopSignal,
    pause: &chilpa_audio::sink::PauseSignal,
    skip: &chilpa_audio::sink::SkipSignal,
    open: &mut OpenSink,
    events: &Sender<PlayerEvent>,
) -> Result<Outcome, PlayError> {
    let node = cd_device_of(path)
        .ok_or_else(|| PlayError::Sink(SinkError::Backend(chilpa_i18n::t("the drive is missing"))))?;
    let mut unit = chilpa_cd::linux::LinuxDrive::open(&node)
        .map_err(|e| PlayError::Sink(SinkError::Backend(e.to_string())))?;
    let toc = crate::cd_playback::read_toc(&mut unit)
        .map_err(|e| PlayError::Sink(SinkError::Backend(e.to_string())))?;
    let tracks: Vec<chilpa_cd::Track> = toc.audio_tracks().copied().collect();
    if tracks.is_empty() {
        return Err(PlayError::Sink(SinkError::Backend(
            chilpa_i18n::t("the disc carries no audio tracks"),
        )));
    }

    let total = tracks.len();
    let mut last = Outcome::Finished { frames: 0 };
    for (index, track) in tracks.iter().enumerate() {
        if stop.is_stopped() {
            return Ok(Outcome::Stopped { frames: last.frames() });
        }
        skip.take();
        let tag = chilpa_tags::Tags {
            title: Some(chilpa_i18n::t_args("Track {}", &[&track.number.to_string()])),
            album: Some(chilpa_i18n::t_args("CD in {}", &[&node.display().to_string()])),
            ..chilpa_tags::Tags::default()
        };
        let events = events.clone();
        let path = path.to_path_buf();
        // A CD is 44,100 frames per second, and its sectors are already PCM.
        let duration = Some(u64::from(track.sectors) * chilpa_cd::FRAMES_PER_SECTOR as u64);

        let played = crate::cd_playback::play_track(
            &mut unit,
            track,
            caps,
            stop,
            pause,
            |format| {
                let sink = open(device, format);
                if sink.is_ok() {
                    let _ = events.send(PlayerEvent::Started {
                        path: path.clone(),
                        opened: format,
                        // A CD delivered as is is always bit-perfect: if
                        // the output does not accept 16/44.1, it is not opened.
                        bit_perfect: true,
                        total_frames: duration,
                        tags: Some(tag.clone()),
                        artwork: None,
                        index: index,
                        queue_len: total,
                        dsd_multiple: None,
                    });
                }
                sink
            },
            &mut |frames| {
                let _ = events.send(PlayerEvent::Progress { frames });
            },
        )
        .map_err(|e| PlayError::Sink(SinkError::Backend(e.to_string())))?;

        // An unreadable sector cuts the track, and it is said instead of
        // padding with silence.
        if let Some(problem) = played.read_error {
            let _ = events.send(PlayerEvent::Failed {
                message: chilpa_i18n::t_args(
                    "track {}: {}",
                    &[&track.number.to_string(), &problem],
                ),
            });
        }
        last = played.outcome;
        if !last.may_advance_queue() {
            break;
        }
    }
    Ok(last)
}

#[cfg(not(target_os = "linux"))]
fn play_cd(
    _path: &Path,
    _device: &str,
    _caps: &chilpa_audio::negotiate::DeviceCaps,
    _stop: &StopSignal,
    _pause: &chilpa_audio::sink::PauseSignal,
    _skip: &chilpa_audio::sink::SkipSignal,
    _open: &mut OpenSink,
    _events: &Sender<PlayerEvent>,
) -> Result<Outcome, PlayError> {
    Err(PlayError::Sink(SinkError::Backend(
        chilpa_i18n::t("CDs are not read outside Linux yet"),
    )))
}

fn play_sacd(
    path: &Path,
    device: &str,
    caps: &chilpa_audio::negotiate::DeviceCaps,
    stop: &StopSignal,
    pause: &chilpa_audio::sink::PauseSignal,
    skip: &chilpa_audio::sink::SkipSignal,
    open: &mut OpenSink,
    events: &Sender<PlayerEvent>,
) -> Result<Outcome, PlayError> {
    let image = chilpa_sacd::open(path)
        .map_err(|e| PlayError::Sink(SinkError::Backend(e.to_string())))?;
    let tracks = image.area.tracks.clone();
    if tracks.is_empty() {
        return Err(PlayError::Sink(SinkError::Backend(chilpa_i18n::t("the disc carries no tracks"))));
    }
    let total = tracks.len();
    let mut last = Outcome::Finished { frames: 0 };

    for (index, track) in tracks.iter().enumerate() {
        if stop.is_stopped() {
            return Ok(Outcome::Stopped { frames: last.frames() });
        }
        skip.take();
        // Every track opens the image again: the reader keeps hold of it, so
        // a track that fails does not take the rest of the disc with it.
        let image = chilpa_sacd::open(path)
            .map_err(|e| PlayError::Sink(SinkError::Backend(e.to_string())))?;
        let mut reader = chilpa_sacd::SacdTrackReader::open(image, track)
            .map_err(|e| PlayError::Sink(SinkError::Backend(e.to_string())))?;

        let tag = chilpa_tags::Tags {
            title: Some(chilpa_i18n::t_args("Track {}", &[&track.number.to_string()])),
            album: path.file_stem().map(|s| s.to_string_lossy().to_string()),
            ..chilpa_tags::Tags::default()
        };
        let events = events.clone();
        let path = path.to_path_buf();
        let duration = Some(u64::from(track.duration_frames) * u64::from(chilpa_sacd::DSD64_HZ / 75) / 16);

        let played = crate::dsd_playback::play_dsd(
            &mut reader,
            caps,
            stop,
            pause,
            |format| {
                let sink = open(device, format);
                if sink.is_ok() {
                    let _ = events.send(PlayerEvent::Started {
                        path: path.clone(),
                        opened: format,
                        bit_perfect: true,
                        total_frames: duration,
                        tags: Some(tag.clone()),
                        artwork: None,
                        index: index,
                        queue_len: total,
                        dsd_multiple: Some(64),
                    });
                }
                sink
            },
            &mut |frames| {
                let _ = events.send(PlayerEvent::Progress { frames });
            },
        )
        .map_err(|e| PlayError::Sink(SinkError::Backend(e.to_string())))?;

        last = played.outcome;
        if !last.may_advance_queue() {
            break;
        }
    }
    Ok(last)
}

/// True if the file is DSD by its extension.
fn is_dsd(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("dsf"))
        .unwrap_or(false)
}

/// Plays a DSD file and reports the same things the PCM path does.
#[allow(clippy::too_many_arguments)]
fn play_dsd(
    path: &Path,
    device: &str,
    caps: &chilpa_audio::negotiate::DeviceCaps,
    stop: &StopSignal,
    pause: &chilpa_audio::sink::PauseSignal,
    open: &mut OpenSink,
    events: &Sender<PlayerEvent>,
) -> Result<Outcome, PlayError> {
    let tags = chilpa_tags::read(path).ok().map(|info| info.tags);
    let artwork = chilpa_tags::extract_artwork(path, &crate::artwork_cache()).ok().flatten();
    use chilpa_dsd::DsdSource as _;
    let info = chilpa_dsd::DsfReader::open(path)
        .map(|r| r.info().clone())
        .map_err(|e| PlayError::Sink(SinkError::Backend(e.to_string())))?;

    let owned = path.to_path_buf();
    let events = events.clone();
    let multiple = info.multiple();
    let total_frames = Some(info.samples / 16);

    let played = crate::dsd_playback::play_dsf(
        path,
        caps,
        stop,
        pause,
        |format| {
            let sink = open(device, format);
            if let Ok(_) = &sink {
                let _ = events.send(PlayerEvent::Started {
                    path: owned.clone(),
                    opened: format,
                    bit_perfect: true,
                    total_frames,
                    tags: tags.clone(),
                    artwork: artwork.clone(),
                    index: 0,
                    queue_len: 1,
                    dsd_multiple: Some(multiple),
                });
            }
            sink
        },
        &mut |frames| {
            let _ = events.send(PlayerEvent::Progress { frames });
        },
    )
    .map_err(|e| PlayError::Sink(SinkError::Backend(e.to_string())))?;

    Ok(played.outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cd_path_is_recognized_by_its_prefix() {
        assert!(is_cd(Path::new("cdda:/dev/sr0")));
        // A bare device node is NOT a CD: opening the drive by mistake
        // startles anyone.
        assert!(!is_cd(Path::new("/dev/sr0")));
        assert!(!is_cd(Path::new("/music/one.flac")));
    }

    #[test]
    fn a_cd_path_yields_its_drive() {
        assert_eq!(cd_device_of(Path::new("cdda:/dev/sr0")), Some(PathBuf::from("/dev/sr0")));
        assert_eq!(cd_device_of(Path::new("cdda:")), None, "with no drive there is nothing to open");
        assert_eq!(cd_device_of(Path::new("/dev/sr0")), None);
    }

    use chilpa_audio::format::SampleFormat;
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    struct NullSink {
        stream: StreamFormat,
        written: Arc<AtomicU64>,
    }

    impl AudioSink for NullSink {
        fn format(&self) -> StreamFormat {
            self.stream
        }
        fn write(&mut self, frames: &[u8]) -> Result<usize, SinkError> {
            let n = frames.len() / self.stream.frame_bytes();
            self.written.fetch_add(n as u64, Ordering::Relaxed);
            Ok(n)
        }
        fn close(&mut self) -> Result<(), SinkError> {
            Ok(())
        }
    }

    fn wav(name: &str, frames: u32) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("chilpa-player-{name}.wav"));
        let data = frames * 4;
        let mut f = std::fs::File::create(&p).unwrap();
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
        for i in 0..frames {
            let v = (i % 1000) as i16;
            f.write_all(&v.to_le_bytes()).unwrap();
            f.write_all(&(-v).to_le_bytes()).unwrap();
        }
        p
    }

    fn player(written: Arc<AtomicU64>) -> Player {
        Player::spawn(
            Box::new(|_| {
                Ok(DeviceCaps {
                    sample_rates: vec![44_100],
                    formats: vec![SampleFormat::S16],
                    channels: vec![2],
                    probed: true,
                })
            }),
            Box::new(move |_, stream| {
                Ok(Box::new(NullSink { stream, written: Arc::clone(&written) }) as Box<dyn AudioSink + Send>)
            }),
        )
    }

    fn wait<F: Fn(&[PlayerEvent]) -> bool>(p: &Player, hasta: F) -> Vec<PlayerEvent> {
        let limit = Instant::now() + Duration::from_secs(5);
        let mut all = Vec::new();
        while Instant::now() < limit {
            all.extend(p.poll());
            if hasta(&all) {
                return all;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        all
    }

    #[test]
    fn playing_does_not_block_and_reports_progress_and_end() {
        let written = Arc::new(AtomicU64::new(0));
        let p = player(Arc::clone(&written));
        let file = wav("basic", 5_000);

        let antes = Instant::now();
        p.play(&file, "fake", true);
        assert!(antes.elapsed() < Duration::from_millis(100), "play() returns immediately");

        let events = wait(&p, |e| e.iter().any(|x| matches!(x, PlayerEvent::Finished { .. })));
        assert!(events.iter().any(|e| matches!(e, PlayerEvent::Started { .. })));
        assert!(events.iter().any(|e| matches!(e, PlayerEvent::Progress { .. })));
        assert!(matches!(events.last(), Some(PlayerEvent::Finished { frames: 5_000 })));
        assert_eq!(written.load(Ordering::Relaxed), 5_000);
        let _ = std::fs::remove_file(file);
    }

    #[test]
    fn a_queue_plays_in_order_and_reports_each_track_position() {
        let written = Arc::new(AtomicU64::new(0));
        let p = player(Arc::clone(&written));
        let a = wav("cola-a", 1_000);
        let b = wav("cola-b", 2_000);
        p.play_all(&[a.clone(), b.clone()], "fake", true);

        let events = wait(&p, |e| e.iter().any(|x| matches!(x, PlayerEvent::Finished { .. })));
        let startups: Vec<(usize, usize)> = events
            .iter()
            .filter_map(|e| match e {
                PlayerEvent::Started { index, queue_len, .. } => Some((*index, *queue_len)),
                _ => None,
            })
            .collect();
        assert_eq!(startups, vec![(0, 2), (1, 2)], "both tracks, in order");
        assert_eq!(written.load(Ordering::Relaxed), 3_000, "every frame went out");
        let _ = std::fs::remove_file(a);
        let _ = std::fs::remove_file(b);
    }

    #[test]
    fn skipping_advances_the_queue_instead_of_stopping_it() {
        let written = Arc::new(AtomicU64::new(0));
        let p = player(Arc::clone(&written));
        // Long tracks: without skipping, the test would take a while to reach the end.
        let a = wav("skip-a", 400_000);
        let b = wav("skip-b", 1_000);
        p.play_all(&[a.clone(), b.clone()], "fake", true);

        // As soon as the first one starts, a skip is asked for. The events
        // are gathered from both waits: each one consumes what it picked up.
        let mut events =
            wait(&p, |e| e.iter().any(|x| matches!(x, PlayerEvent::Started { .. })));
        p.next();
        events.extend(wait(&p, |e| {
            e.iter().any(|x| matches!(x, PlayerEvent::Finished { .. }))
        }));

        let startups = events
            .iter()
            .filter(|e| matches!(e, PlayerEvent::Started { .. }))
            .count();
        assert!(startups >= 2, "the second track started: {startups} startups");
        assert!(
            written.load(Ordering::Relaxed) < 401_000,
            "the first one did not play whole"
        );
        let _ = std::fs::remove_file(a);
        let _ = std::fs::remove_file(b);
    }

    #[test]
    fn a_missing_file_is_reported_and_the_player_stays_alive() {
        let p = player(Arc::new(AtomicU64::new(0)));
        p.play(Path::new("/does/not/exist.flac"), "fake", true);
        let events = wait(&p, |e| e.iter().any(|x| matches!(x, PlayerEvent::Failed { .. })));
        assert!(matches!(events.last(), Some(PlayerEvent::Failed { .. })));

        let file = wav("after-failing", 1_000);
        p.play(&file, "fake", true);
        let events = wait(&p, |e| e.iter().any(|x| matches!(x, PlayerEvent::Finished { .. })));
        assert!(events.iter().any(|e| matches!(e, PlayerEvent::Finished { .. })), "it still accepts orders");
        let _ = std::fs::remove_file(file);
    }
}
