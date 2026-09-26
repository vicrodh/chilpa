//! Sending what is listened to, as it is listened to, to ListenBrainz.
//!
//! One thread of its own does the network, so the window never waits on
//! it: the facade hands it what started playing and what counted as a
//! listen, and reads back a status for Settings.
//!
//! A listen that cannot go out now -- no network, the server down, its rate
//! limit -- is kept on disk and sent later, several to a request, oldest
//! first, waiting longer after each failure. What is playing now is never
//! kept: by the time it could be sent it is not playing any more.
//!
//! The token is sealed with the same key the plugins' credentials use. The
//! QBZ it comes from kept it in the clear in a database.

use crate::listenbrainz::{self, Failure, Kind, Listen, BATCH};
use chilpa_secrets::SecretBox;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The most listens kept waiting. Past it the oldest go: a year offline at
/// forty listens a day still fits.
const KEPT: usize = 20_000;
/// The longest wait between two tries.
const MOST_WAIT: Duration = Duration::from_secs(30 * 60);
/// How often the thread wakes up on its own to send what is waiting.
const TICK: Duration = Duration::from_secs(5 * 60);

impl crate::Chilpa {
    /// Starts sending listens to ListenBrainz, when an account is connected.
    /// The window calls it once; nothing else does, so a test never opens
    /// the keyring or the network.
    pub fn start_scrobbling(&mut self) {
        if self.scrobbler.is_none() {
            self.scrobbler = Some(Scrobbler::start(chilpa_dirs::data().join("listenbrainz"), Self::plugins_dir()));
        }
    }

    /// Where ListenBrainz stands, for Settings.
    pub fn listenbrainz(&self) -> ListenBrainzStatus {
        self.scrobbler.as_ref().map(|s| s.status()).unwrap_or_default()
    }

    /// Checks a user token and, if it is good, keeps it and starts sending.
    pub fn connect_listenbrainz(&mut self, token: &str) {
        self.start_scrobbling();
        if let Some(s) = &self.scrobbler {
            s.connect(token);
        }
    }

    pub fn disconnect_listenbrainz(&mut self) {
        if let Some(s) = &self.scrobbler {
            s.disconnect();
        }
    }

    pub fn enable_listenbrainz(&mut self, on: bool) {
        if let Some(s) = &self.scrobbler {
            s.enable(on);
        }
    }

    /// The listen a track that just started would be, told to ListenBrainz
    /// as what is playing now. None when the tags do not name it.
    pub(crate) fn announce(
        &self,
        tags: Option<&chilpa_tags::Tags>,
        duration_ms: u64,
        started_at: i64,
    ) -> Option<Listen> {
        let t = tags?;
        let listen = Listen {
            artist: t.artist_line(),
            track: self.now.title.clone(),
            release: self.now.album.clone(),
            duration_ms,
            track_number: t.track_number,
            recording_mbid: t.musicbrainz_recording_id.clone().unwrap_or_default(),
            isrc: t.isrc.clone().unwrap_or_default(),
            listened_at: started_at,
        };
        if listen.artist.is_empty() || listen.track.is_empty() {
            return None;
        }
        if let Some(s) = &self.scrobbler {
            s.playing(listen.clone());
        }
        Some(listen)
    }

    /// Sends the playing track's listen once enough of it has been heard:
    /// now, filed under when it started, and not when it ends -- a crash or
    /// a closed window after the mark would lose it otherwise.
    pub(crate) fn scrobble_if_heard(&mut self) {
        let Some(current) = self.in_progress.as_mut() else { return };
        if current.scrobbled || current.sample_rate_hz == 0 {
            return;
        }
        let played_ms = current.played_frames * 1000 / u64::from(current.sample_rate_hz);
        if counts(played_ms, current.duration_ms) {
            current.scrobbled = true;
            if let (Some(s), Some(listen)) = (&self.scrobbler, &current.listen) {
                s.listened(listen.clone());
            }
        }
    }
}

/// What Settings shows about ListenBrainz.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ListenBrainzStatus {
    pub enabled: bool,
    /// Who the token belongs to; empty when there is none.
    pub user: String,
    /// A token is being checked.
    pub checking: bool,
    /// Listens kept until they can be sent.
    pub pending: usize,
    /// The last thing that went wrong, for the person to read.
    pub error: String,
}

enum Order {
    Playing(Listen),
    Listened(Listen),
    Connect(String),
    Disconnect,
    Enable(bool),
}

/// The handle the facade keeps.
pub struct Scrobbler {
    orders: Sender<Order>,
    status: Arc<Mutex<ListenBrainzStatus>>,
}

impl Scrobbler {
    /// Starts the thread. `dir` holds the waiting listens and the state;
    /// `secrets` is where the key that seals the token lives.
    pub fn start(dir: PathBuf, secrets: PathBuf) -> Self {
        let (orders, inbox) = mpsc::channel();
        let status = Arc::new(Mutex::new(ListenBrainzStatus::default()));
        let shared = Arc::clone(&status);
        let _ = std::thread::Builder::new()
            .name("chilpa-scrobble".into())
            .spawn(move || Worker::open(dir, &secrets, shared).run(inbox));
        Self { orders, status }
    }

    pub fn status(&self) -> ListenBrainzStatus {
        self.status.lock().map(|s| s.clone()).unwrap_or_default()
    }

    pub fn playing(&self, listen: Listen) {
        let _ = self.orders.send(Order::Playing(listen));
    }

    pub fn listened(&self, listen: Listen) {
        let _ = self.orders.send(Order::Listened(listen));
    }

    /// Checks a token and, if it is good, keeps it and turns sending on.
    pub fn connect(&self, token: &str) {
        if let Ok(mut s) = self.status.lock() {
            s.checking = true;
            s.error.clear();
        }
        let _ = self.orders.send(Order::Connect(token.trim().to_string()));
    }

    /// Forgets the token. What is waiting stays, for the next one.
    pub fn disconnect(&self) {
        let _ = self.orders.send(Order::Disconnect);
    }

    pub fn enable(&self, on: bool) {
        let _ = self.orders.send(Order::Enable(on));
    }
}

/// What survives a restart, besides the token.
#[derive(Default, Serialize, Deserialize)]
struct Saved {
    enabled: bool,
    user: String,
}

/// The two calls that go out to the network, apart so a test can answer
/// them.
struct Net {
    validate: fn(&str) -> Result<String, Failure>,
    submit: fn(&str, Kind, &[Listen]) -> Result<(), Failure>,
}

const LISTENBRAINZ: Net = Net { validate: listenbrainz::validate, submit: listenbrainz::submit };

struct Worker {
    net: Net,
    dir: PathBuf,
    secrets: Option<SecretBox>,
    token: Option<String>,
    saved: Saved,
    pending: Vec<Listen>,
    /// When sending may be tried again, after a failure.
    retry_at: Option<Instant>,
    wait: Duration,
    status: Arc<Mutex<ListenBrainzStatus>>,
}

impl Worker {
    fn open(dir: PathBuf, secrets_dir: &Path, status: Arc<Mutex<ListenBrainzStatus>>) -> Self {
        let secrets = SecretBox::open("chilpa", secrets_dir).ok();
        let token = std::fs::read(dir.join("token.sealed"))
            .ok()
            .and_then(|sealed| secrets.as_ref()?.open_sealed(&sealed).ok())
            .and_then(|plain| String::from_utf8(plain).ok());
        let saved = read_json(&dir.join("state.json")).unwrap_or_default();
        let pending = read_json(&dir.join("pending.json")).unwrap_or_default();
        let worker = Self {
            net: LISTENBRAINZ,
            dir,
            secrets,
            token,
            saved,
            pending,
            retry_at: None,
            wait: Duration::from_secs(60),
            status,
        };
        worker.publish(String::new());
        worker
    }

    fn run(mut self, inbox: Receiver<Order>) {
        // Whatever an earlier run left waiting goes first.
        self.flush();
        loop {
            match inbox.recv_timeout(TICK) {
                Ok(order) => self.handle(order),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            self.flush();
        }
    }

    fn sending(&self) -> Option<String> {
        self.token.clone().filter(|_| self.saved.enabled)
    }

    fn handle(&mut self, order: Order) {
        match order {
            Order::Playing(listen) => {
                if let Some(token) = self.sending() {
                    // Best effort: a failure here is not worth keeping.
                    let _ = (self.net.submit)(&token, Kind::PlayingNow, &[listen]);
                }
            }
            Order::Listened(listen) => {
                if self.sending().is_none() {
                    return;
                }
                self.pending.push(listen);
                if self.pending.len() > KEPT {
                    let extra = self.pending.len() - KEPT;
                    self.pending.drain(..extra);
                }
                self.save_pending();
                self.publish(String::new());
            }
            Order::Connect(token) => match (self.net.validate)(&token) {
                Ok(user) => {
                    let sealed = self.secrets.as_ref().and_then(|s| s.seal(token.as_bytes()).ok());
                    let kept = sealed.is_some_and(|bytes| {
                        std::fs::create_dir_all(&self.dir).is_ok()
                            && std::fs::write(self.dir.join("token.sealed"), bytes).is_ok()
                    });
                    self.token = Some(token);
                    self.saved = Saved { enabled: true, user };
                    self.save_state();
                    self.retry_at = None;
                    let note =
                        if kept { String::new() } else { chilpa_i18n::t("the token could not be kept for next time") };
                    self.checked(note);
                }
                Err(Failure::Token) => self.checked(chilpa_i18n::t("ListenBrainz did not accept that token")),
                Err(_) => self.checked(chilpa_i18n::t("ListenBrainz could not be reached; try again")),
            },
            Order::Disconnect => {
                self.token = None;
                let _ = std::fs::remove_file(self.dir.join("token.sealed"));
                self.saved.user.clear();
                self.save_state();
                self.publish(String::new());
            }
            Order::Enable(on) => {
                self.saved.enabled = on;
                self.save_state();
                self.publish(String::new());
            }
        }
    }

    /// Sends what is waiting, oldest first, until it is all gone or the
    /// other side says to stop.
    fn flush(&mut self) {
        while !self.pending.is_empty() {
            let Some(token) = self.sending() else { return };
            if self.retry_at.is_some_and(|at| Instant::now() < at) {
                return;
            }
            let n = self.pending.len().min(BATCH);
            let kind = if n == 1 { Kind::Single } else { Kind::Import };
            match (self.net.submit)(&token, kind, &self.pending[..n]) {
                Ok(()) => {
                    self.pending.drain(..n);
                    self.wait = Duration::from_secs(60);
                    self.retry_at = None;
                    self.save_pending();
                    self.publish(String::new());
                }
                Err(Failure::Later(seconds)) => {
                    let wait = self.wait.max(Duration::from_secs(seconds));
                    self.retry_at = Some(Instant::now() + wait);
                    self.wait = (wait * 2).min(MOST_WAIT);
                    return;
                }
                Err(Failure::Token) => {
                    // Kept, not sent: a new token sends them.
                    self.token = None;
                    self.publish(chilpa_i18n::t("ListenBrainz no longer accepts the token; connect again"));
                    return;
                }
                Err(Failure::Refused(why)) if n > 1 => {
                    // One bad listen refuses the whole import: send them one
                    // at a time, so only that one is lost.
                    let batch: Vec<Listen> = self.pending.drain(..n).collect();
                    let mut kept = Vec::new();
                    for listen in batch {
                        match (self.net.submit)(&token, Kind::Single, std::slice::from_ref(&listen)) {
                            Ok(()) | Err(Failure::Refused(_)) => {}
                            Err(_) => kept.push(listen),
                        }
                    }
                    kept.append(&mut self.pending);
                    self.pending = kept;
                    self.save_pending();
                    self.publish(why);
                }
                Err(Failure::Refused(why)) => {
                    self.pending.remove(0);
                    self.save_pending();
                    self.publish(why);
                }
            }
        }
    }

    /// Updates the status. Whether a token is being checked is left as it
    /// is: the facade raises it when it asks, and only the answer lowers it.
    fn publish(&self, error: String) {
        if let Ok(mut s) = self.status.lock() {
            *s = ListenBrainzStatus {
                enabled: self.saved.enabled,
                user: if self.token.is_some() { self.saved.user.clone() } else { String::new() },
                checking: s.checking,
                pending: self.pending.len(),
                error,
            };
        }
    }

    /// The answer to a token check.
    fn checked(&self, error: String) {
        if let Ok(mut s) = self.status.lock() {
            s.checking = false;
        }
        self.publish(error);
    }

    fn save_pending(&self) {
        write_json(&self.dir.join("pending.json"), &self.pending);
    }

    fn save_state(&self) {
        write_json(&self.dir.join("state.json"), &self.saved);
    }
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

/// Written beside and moved over, so a crash mid write leaves the old file
/// and not half of the new one.
fn write_json<T: Serialize>(path: &Path, value: &T) {
    let Ok(bytes) = serde_json::to_vec(value) else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let next = path.with_extension("json.new");
    if std::fs::write(&next, bytes).is_ok() {
        let _ = std::fs::rename(&next, path);
    }
}

/// Whether a listen that has played for `played_ms` of `duration_ms` counts:
/// half of it or four minutes, whichever comes first, and never a track of
/// half a minute or less, or of unknown length (ListenBrainz's own rule).
pub(crate) fn counts(played_ms: u64, duration_ms: u64) -> bool {
    duration_ms > 30_000 && played_ms >= (duration_ms / 2).min(240_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_listen_counts_at_half_or_four_minutes() {
        assert!(!counts(89_000, 180_000));
        assert!(counts(90_000, 180_000), "half of three minutes");
        assert!(counts(240_000, 1_200_000), "four minutes of twenty");
        assert!(!counts(239_000, 1_200_000));
        assert!(!counts(30_000, 30_000), "half a minute is not enough to count");
        assert!(!counts(500_000, 0), "a length nobody knows never counts");
    }

    fn worker(dir: &str, submit: fn(&str, Kind, &[Listen]) -> Result<(), Failure>, waiting: usize) -> Worker {
        let dir = std::env::temp_dir().join(dir);
        let _ = std::fs::remove_dir_all(&dir);
        Worker {
            net: Net { validate: |_| Ok("someone".into()), submit },
            dir,
            secrets: None,
            token: Some("token".into()),
            saved: Saved { enabled: true, user: "someone".into() },
            pending: (0..waiting)
                .map(|i| Listen { artist: "A".into(), track: format!("T{i}"), listened_at: i as i64, ..Listen::default() })
                .collect(),
            retry_at: None,
            wait: Duration::from_secs(60),
            status: Arc::new(Mutex::new(ListenBrainzStatus::default())),
        }
    }

    #[test]
    fn what_waits_goes_out_a_hundred_at_a_time_oldest_first() {
        static SIZES: Mutex<Vec<(usize, i64)>> = Mutex::new(Vec::new());
        let mut w = worker("chilpa-scrobble-batches", |_, _, l| {
            SIZES.lock().unwrap().push((l.len(), l[0].listened_at));
            Ok(())
        }, 150);
        w.flush();
        assert!(w.pending.is_empty());
        assert_eq!(*SIZES.lock().unwrap(), [(100, 0), (50, 100)]);
        assert_eq!(w.status.lock().unwrap().pending, 0);
    }

    #[test]
    fn a_server_that_says_later_is_left_alone_until_then() {
        let mut w = worker("chilpa-scrobble-later", |_, _, _| Err(Failure::Later(120)), 3);
        w.flush();
        assert_eq!(w.pending.len(), 3, "kept");
        let wait = w.retry_at.unwrap() - Instant::now();
        assert!(wait > Duration::from_secs(110), "it asked for two minutes");
        // Before then, nothing is tried.
        w.net.submit = |_, _, _| panic!("tried before it was time");
        w.flush();
    }

    #[test]
    fn one_refused_listen_does_not_take_the_rest_with_it() {
        // An import with T1 in it is refused; T1 alone is refused; the rest go.
        let mut w = worker("chilpa-scrobble-refused", |_, _, l| {
            if l.iter().any(|x| x.track == "T1") { Err(Failure::Refused("bad".into())) } else { Ok(()) }
        }, 3);
        w.flush();
        assert!(w.pending.is_empty(), "T0 and T2 went; T1 was dropped");
        assert_eq!(w.status.lock().unwrap().error, "bad");
    }

    #[test]
    fn a_token_that_stopped_working_keeps_the_listens_for_the_next() {
        let mut w = worker("chilpa-scrobble-token", |_, _, _| Err(Failure::Token), 2);
        w.flush();
        assert_eq!(w.pending.len(), 2);
        assert!(w.token.is_none());
        assert!(w.status.lock().unwrap().user.is_empty(), "shown as not connected");
    }

    #[test]
    fn what_waits_survives_a_restart_and_a_crash_mid_write() {
        let dir = std::env::temp_dir().join("chilpa-scrobble-pending");
        let _ = std::fs::remove_dir_all(&dir);
        let listens = vec![Listen { artist: "A".into(), track: "T".into(), listened_at: 1, ..Listen::default() }];
        write_json(&dir.join("pending.json"), &listens);
        // A write that died before the move leaves the last good file.
        std::fs::write(dir.join("pending.json.new"), b"{half").unwrap();
        let back: Vec<Listen> = read_json(&dir.join("pending.json")).unwrap();
        assert_eq!(back, listens);
        let _ = std::fs::remove_dir_all(dir);
    }
}
