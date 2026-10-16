//! The MPRIS server, on `mpris-server`.
//!
//! `mpris-server` lets the `DesktopEntry` property be published, and that
//! is the only thing GNOME Shell reads to find the application's icon for
//! its media widget. `mpris:artUrl` is the cover, a separate matter.
//!
//! The server runs on a thread of its own; what the player says arrives
//! over a channel, and what the desktop asks goes out through a callback.

use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};

use mpris_server::zbus::{self, fdo};
use mpris_server::{
    LoopStatus, Metadata, PlaybackRate, PlaybackStatus as MprisStatus, PlayerInterface, Property,
    RootInterface, Server, Signal, Time, TrackId, Volume,
};

use crate::inhibit::SleepInhibitor;
use crate::types::{MediaEvent, PlaybackStatus, TrackMeta};
use crate::APP_ID;

const IDENTITY: &str = "Chilpa";

/// Monotonic counter so each track gets a distinct `mpris:trackid` object path
/// (helps clients detect track changes).
static TRACK_SEQ: AtomicU64 = AtomicU64::new(1);

type EventCb = Arc<dyn Fn(MediaEvent) + Send + Sync>;

/// Shared, mutable now-playing state. Read by the MPRIS getter methods (on the
/// zbus task) and written by the update loop (on the same runtime). Never held
/// across an `.await`.
struct State {
    metadata: Metadata,
    status: MprisStatus,
    volume: Volume,
    /// The last position we were TOLD, and when we were told it.
    ///
    /// MPRIS `Position` is read on demand, so what matters is the answer at
    /// READ time, not at push time. Storing the instant alongside lets the
    /// getter extrapolate — `position + (now - stamped)` while Playing — which
    /// is exact to the microsecond no matter how often the app pushes. Without
    /// it the answer is only as fresh as the last push, and the app would have
    /// to choose between a stale clock and a faster poll it does not want.
    position: Time,
    stamped: std::time::Instant,
}

/// Update commands sent from the app to the server thread.
enum Update {
    Metadata(Metadata),
    Playback {
        status: MprisStatus,
        position: Option<Time>,
    },
    Volume(Volume),
    /// Position ONLY — stores, emits nothing. MPRIS excludes `Position` from
    /// `PropertiesChanged` by design, so there is no signal to send; the point
    /// is purely that the next on-demand read is not stale.
    Position(Time),
    /// A DISCONTINUOUS jump. Stores like `Position` and additionally emits the
    /// `Seeked` signal, which is the ONE thing that tells an already-open
    /// client to stop extrapolating and re-read. Without it a widget keeps
    /// counting from wherever it was until it happens to poll again.
    Seeked(Time),
}

/// The cloneable handle returned to the app. Pushing state is a non-blocking
/// channel send from any thread/context.
pub struct Handle {
    tx: async_channel::Sender<Update>,
}

impl Handle {
    pub fn set_metadata(&self, meta: &TrackMeta) {
        let _ = self.tx.try_send(Update::Metadata(build_metadata(meta)));
    }

    pub fn set_playback(&self, status: PlaybackStatus, position: Option<std::time::Duration>) {
        let _ = self.tx.try_send(Update::Playback {
            status: map_status(status),
            position: position.map(|d| Time::from_micros(d.as_micros() as i64)),
        });
    }

    pub fn set_volume(&self, vol: f64) {
        let _ = self.tx.try_send(Update::Volume(vol.clamp(0.0, 1.0)));
    }

    pub fn set_position(&self, position: std::time::Duration) {
        let _ = self
            .tx
            .try_send(Update::Position(Time::from_micros(position.as_micros() as i64)));
    }

    pub fn seeked(&self, position: std::time::Duration) {
        let _ = self
            .tx
            .try_send(Update::Seeked(Time::from_micros(position.as_micros() as i64)));
    }
}

fn map_status(s: PlaybackStatus) -> MprisStatus {
    match s {
        PlaybackStatus::Playing => MprisStatus::Playing,
        PlaybackStatus::Paused => MprisStatus::Paused,
        PlaybackStatus::Stopped => MprisStatus::Stopped,
    }
}

fn build_metadata(meta: &TrackMeta) -> Metadata {
    let seq = TRACK_SEQ.fetch_add(1, Ordering::Relaxed);
    let trackid = TrackId::try_from(format!("/app/chilpa/Chilpa/track/{seq}"))
        .unwrap_or(TrackId::NO_TRACK);

    let mut b = Metadata::builder().trackid(trackid).title(meta.title.clone());
    if !meta.artist.is_empty() {
        b = b.artist([meta.artist.clone()]);
    }
    if !meta.album.is_empty() {
        b = b.album(meta.album.clone());
    }
    if let Some(d) = meta.duration {
        b = b.length(Time::from_micros(d.as_micros() as i64));
    }
    if let Some(url) = &meta.art_url {
        b = b.art_url(url.clone());
    }
    if let Some(url) = &meta.url {
        b = b.url(url.clone());
    }
    b.build()
}

/// The MPRIS interface implementation. Getters read shared `State`; the action
/// methods forward to the app via `on_event`.
struct Mpris {
    on_event: EventCb,
    state: Arc<Mutex<State>>,
}

impl Mpris {
    fn emit(&self, ev: MediaEvent) {
        (self.on_event)(ev);
    }
}

impl RootInterface for Mpris {
    async fn raise(&self) -> fdo::Result<()> {
        self.emit(MediaEvent::Raise);
        Ok(())
    }
    async fn quit(&self) -> fdo::Result<()> {
        self.emit(MediaEvent::Quit);
        Ok(())
    }
    async fn can_quit(&self) -> fdo::Result<bool> {
        Ok(true)
    }
    async fn fullscreen(&self) -> fdo::Result<bool> {
        Ok(false)
    }
    async fn set_fullscreen(&self, _fullscreen: bool) -> zbus::Result<()> {
        Ok(())
    }
    async fn can_set_fullscreen(&self) -> fdo::Result<bool> {
        Ok(false)
    }
    async fn can_raise(&self) -> fdo::Result<bool> {
        Ok(true)
    }
    async fn has_track_list(&self) -> fdo::Result<bool> {
        Ok(false)
    }
    async fn identity(&self) -> fdo::Result<String> {
        Ok(IDENTITY.to_string())
    }
    /// The GNOME app-icon fix: GNOME resolves the icon from this property.
    async fn desktop_entry(&self) -> fdo::Result<String> {
        Ok(APP_ID.to_string())
    }
    async fn supported_uri_schemes(&self) -> fdo::Result<Vec<String>> {
        Ok(vec![])
    }
    async fn supported_mime_types(&self) -> fdo::Result<Vec<String>> {
        Ok(vec![])
    }
}

impl PlayerInterface for Mpris {
    async fn next(&self) -> fdo::Result<()> {
        self.emit(MediaEvent::Next);
        Ok(())
    }
    async fn previous(&self) -> fdo::Result<()> {
        self.emit(MediaEvent::Previous);
        Ok(())
    }
    async fn pause(&self) -> fdo::Result<()> {
        self.emit(MediaEvent::Pause);
        Ok(())
    }
    async fn play_pause(&self) -> fdo::Result<()> {
        self.emit(MediaEvent::Toggle);
        Ok(())
    }
    async fn stop(&self) -> fdo::Result<()> {
        self.emit(MediaEvent::Stop);
        Ok(())
    }
    async fn play(&self) -> fdo::Result<()> {
        self.emit(MediaEvent::Play);
        Ok(())
    }
    async fn seek(&self, offset: Time) -> fdo::Result<()> {
        self.emit(MediaEvent::SeekBy(offset.as_micros()));
        Ok(())
    }
    async fn set_position(&self, _track_id: TrackId, position: Time) -> fdo::Result<()> {
        self.emit(MediaEvent::SetPosition(position.as_micros()));
        Ok(())
    }
    async fn open_uri(&self, _uri: String) -> fdo::Result<()> {
        Ok(())
    }
    async fn playback_status(&self) -> fdo::Result<MprisStatus> {
        Ok(self.state.lock().unwrap().status)
    }
    async fn loop_status(&self) -> fdo::Result<LoopStatus> {
        Ok(LoopStatus::None)
    }
    async fn set_loop_status(&self, _loop_status: LoopStatus) -> zbus::Result<()> {
        Ok(())
    }
    async fn rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(1.0)
    }
    async fn set_rate(&self, _rate: PlaybackRate) -> zbus::Result<()> {
        Ok(())
    }
    async fn shuffle(&self) -> fdo::Result<bool> {
        Ok(false)
    }
    async fn set_shuffle(&self, _shuffle: bool) -> zbus::Result<()> {
        Ok(())
    }
    async fn metadata(&self) -> fdo::Result<Metadata> {
        Ok(self.state.lock().unwrap().metadata.clone())
    }
    async fn volume(&self) -> fdo::Result<Volume> {
        Ok(self.state.lock().unwrap().volume)
    }
    async fn set_volume(&self, volume: Volume) -> zbus::Result<()> {
        self.emit(MediaEvent::SetVolume(volume.clamp(0.0, 1.0)));
        Ok(())
    }
    async fn position(&self) -> fdo::Result<Time> {
        let st = self.state.lock().unwrap();
        // Extrapolate while playing; a paused or stopped player has not moved
        // since it was stamped.
        if st.status == MprisStatus::Playing {
            let elapsed = st.stamped.elapsed().as_micros() as i64;
            Ok(Time::from_micros(st.position.as_micros() + elapsed))
        } else {
            Ok(st.position)
        }
    }
    async fn minimum_rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(1.0)
    }
    async fn maximum_rate(&self) -> fdo::Result<PlaybackRate> {
        Ok(1.0)
    }
    async fn can_go_next(&self) -> fdo::Result<bool> {
        Ok(true)
    }
    async fn can_go_previous(&self) -> fdo::Result<bool> {
        Ok(true)
    }
    async fn can_play(&self) -> fdo::Result<bool> {
        Ok(true)
    }
    async fn can_pause(&self) -> fdo::Result<bool> {
        Ok(true)
    }
    async fn can_seek(&self) -> fdo::Result<bool> {
        Ok(true)
    }
    async fn can_control(&self) -> fdo::Result<bool> {
        Ok(true)
    }
}

async fn apply(server: &Server<Mpris>, state: &Arc<Mutex<State>>, update: Update) {
    match update {
        Update::Metadata(m) => {
            state.lock().unwrap().metadata = m.clone();
            let _ = server.properties_changed([Property::Metadata(m)]).await;
        }
        Update::Playback { status, position } => {
            {
                let mut st = state.lock().unwrap();
                st.status = status;
                if let Some(p) = position {
                    st.position = p;
                    st.stamped = std::time::Instant::now();
                } else {
                    // A status change with no position still re-anchors: the
                    // extrapolation base must not keep running across a pause.
                    st.position = Time::from_micros(
                        st.position.as_micros()
                            + if st.status == MprisStatus::Playing {
                                st.stamped.elapsed().as_micros() as i64
                            } else {
                                0
                            },
                    );
                    st.stamped = std::time::Instant::now();
                }
            }
            let _ = server
                .properties_changed([Property::PlaybackStatus(status)])
                .await;
        }
        Update::Volume(v) => {
            state.lock().unwrap().volume = v;
            let _ = server.properties_changed([Property::Volume(v)]).await;
        }
        Update::Position(p) => {
            let mut st = state.lock().unwrap();
            st.position = p;
            st.stamped = std::time::Instant::now();
        }
        Update::Seeked(p) => {
            {
                let mut st = state.lock().unwrap();
                st.position = p;
                st.stamped = std::time::Instant::now();
            }
            let _ = server.emit(Signal::Seeked { position: p }).await;
        }
    }
}

/// Starts the MPRIS server on a thread of its own. `None` if the thread
/// cannot start. The name is registered on that thread: a failure there is
/// written down, and the player goes on without media controls.
pub fn spawn(on_event: impl Fn(MediaEvent) + Send + Sync + 'static) -> Option<Handle> {
    let (tx, rx) = async_channel::unbounded::<Update>();
    let on_event: EventCb = Arc::new(on_event);

    let spawned = std::thread::Builder::new().name("chilpa-mpris".into()).spawn(move || {
        pollster::block_on(async move {
            let state = Arc::new(Mutex::new(State {
                metadata: Metadata::new(),
                status: MprisStatus::Stopped,
                volume: 1.0,
                position: Time::ZERO,
                stamped: std::time::Instant::now(),
            }));
            let imp = Mpris { on_event, state: state.clone() };
            let server = match Server::new(APP_ID, imp).await {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("chilpa: mpris: org.mpris.MediaPlayer2.{APP_ID} could not be registered: {e}");
                    return;
                }
            };
            // Held while playing, let go when paused or stopped. It rides
            // on the same updates the server gets, so it cannot disagree
            // with what the desktop's widget shows.
            let mut inhibitor = SleepInhibitor::new();
            while let Ok(update) = rx.recv().await {
                if let Update::Playback { status, .. } = &update {
                    inhibitor.set_playing(matches!(status, MprisStatus::Playing)).await;
                }
                apply(&server, &state, update).await;
            }
        });
    });

    match spawned {
        Ok(_) => Some(Handle { tx }),
        Err(e) => {
            eprintln!("chilpa: mpris: its thread could not start: {e}");
            None
        }
    }
}
