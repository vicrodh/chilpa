//! One service's thread: its account, and the listens waiting to go out.
//!
//! A listen that cannot go out now -- no network, the server down, its rate
//! limit -- is kept on disk and sent later, several to a request, oldest
//! first, waiting longer after each failure. What is playing now is never
//! kept: by the time it could be sent it is not playing any more.
//!
//! The credential is sealed with the same key the plugins' credentials use.
//! The QBZ this comes from kept it in the clear in a database.

use super::{Failure, Kind, Listen, ScrobbleStatus};
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
/// How long an account being connected is waited for, once its page opened.
const CONNECTING: Duration = Duration::from_secs(5 * 60);

/// What one service does, apart from waiting and keeping.
#[derive(Clone, Copy)]
pub(crate) struct Service {
    /// For the person to read.
    pub label: &'static str,
    /// Listens in one request.
    pub batch: usize,
    /// For a service whose account is allowed on its own site: asks for a
    /// request token and the page where it is allowed.
    pub begin: Option<fn() -> Result<(String, String), Failure>>,
    /// Turns what is being connected with -- a pasted token, an allowed
    /// request token -- into the account's user and what is kept to send.
    pub check: fn(&str) -> Result<(String, String), Failure>,
    pub submit: fn(&str, Kind, &[Listen]) -> Result<(), Failure>,
}

pub(crate) enum Order {
    Playing(Listen),
    Listened(Listen),
    /// Connect with a pasted token, or -- empty -- start connecting on the
    /// service's own site.
    Connect(String),
    Disconnect,
    Enable(bool),
}

/// Starts a service's thread. `dir` holds what waits and the state;
/// `secrets` is where the key that seals the credential lives.
pub(crate) fn start(service: Service, dir: PathBuf, secrets: PathBuf) -> (Sender<Order>, Arc<Mutex<ScrobbleStatus>>) {
    let (orders, inbox) = mpsc::channel();
    let status = Arc::new(Mutex::new(ScrobbleStatus::default()));
    let shared = Arc::clone(&status);
    let _ = std::thread::Builder::new()
        .name("chilpa-scrobble".into())
        .spawn(move || Worker::open(service, dir, &secrets, shared).run(inbox));
    (orders, status)
}

/// What survives a restart, besides the credential.
#[derive(Default, Serialize, Deserialize)]
struct Saved {
    enabled: bool,
    user: String,
}

/// An account being connected on the service's own site.
struct Connecting {
    token: String,
    until: Instant,
}

pub(super) struct Worker {
    service: Service,
    dir: PathBuf,
    secrets: Option<SecretBox>,
    credential: Option<String>,
    saved: Saved,
    pending: Vec<Listen>,
    connecting: Option<Connecting>,
    /// When sending may be tried again, after a failure.
    retry_at: Option<Instant>,
    wait: Duration,
    status: Arc<Mutex<ScrobbleStatus>>,
}

impl Worker {
    fn open(service: Service, dir: PathBuf, secrets_dir: &Path, status: Arc<Mutex<ScrobbleStatus>>) -> Self {
        let secrets = SecretBox::open("chilpa", secrets_dir).map_err(|e| eprintln!("chilpa-scrobble: no box: step={} ({e})", e.kind())).ok();
        let credential = std::fs::read(dir.join("credential.sealed"))
            .ok()
            .and_then(|sealed| {
                let secrets = secrets.as_ref()?;
                let opened = secrets.open_sealed(&sealed);
                opened.map_err(|e| eprintln!("chilpa-scrobble: {} sign-in not opened: step={} backend={:?}", service.label, e.kind(), secrets.backend())).ok()
            })
            .and_then(|plain| String::from_utf8(plain).ok());
        let saved = read_json(&dir.join("state.json")).unwrap_or_default();
        let pending = read_json(&dir.join("pending.json")).unwrap_or_default();
        let worker = Self {
            service,
            dir,
            secrets,
            credential,
            saved,
            pending,
            connecting: None,
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
            // While an account is being allowed, it is asked about often.
            let tick = if self.connecting.is_some() { Duration::from_secs(3) } else { TICK };
            match inbox.recv_timeout(tick) {
                Ok(order) => self.handle(order),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            self.poll_connecting();
            self.flush();
        }
    }

    fn sending(&self) -> Option<String> {
        self.credential.clone().filter(|_| self.saved.enabled)
    }

    pub(super) fn handle(&mut self, order: Order) {
        match order {
            Order::Playing(listen) => {
                if let Some(credential) = self.sending() {
                    // Best effort: a failure here is not worth keeping.
                    let _ = (self.service.submit)(&credential, Kind::PlayingNow, &[listen]);
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
            Order::Connect(token) if token.is_empty() => {
                let Some(begin) = self.service.begin else { return self.checked(String::new()) };
                match begin() {
                    Ok((token, link)) => {
                        self.connecting = Some(Connecting { token, until: Instant::now() + CONNECTING });
                        if let Ok(mut s) = self.status.lock() {
                            s.link = link;
                        }
                    }
                    Err(Failure::Refused(why)) => self.checked(why),
                    Err(_) => self.checked(chilpa_i18n::t_args("{} could not be reached; try again", &[self.service.label])),
                }
            }
            Order::Connect(token) => self.check(&token),
            Order::Disconnect => {
                self.credential = None;
                self.connecting = None;
                let _ = std::fs::remove_file(self.dir.join("credential.sealed"));
                self.saved.user.clear();
                self.save_state();
                self.checked(String::new());
            }
            Order::Enable(on) => {
                self.saved.enabled = on;
                self.save_state();
                self.publish(String::new());
            }
        }
    }

    /// Asks whether the account being connected was allowed yet.
    fn poll_connecting(&mut self) {
        let Some(c) = &self.connecting else { return };
        if Instant::now() > c.until {
            self.connecting = None;
            self.checked(chilpa_i18n::t_args("{} was not allowed in time; connect again", &[self.service.label]));
            return;
        }
        let token = c.token.clone();
        match (self.service.check)(&token) {
            // Not allowed yet, or the network: ask again on the next tick.
            Err(Failure::Later(_)) => {}
            // A request token answers once: what came back is the answer,
            // and asking again with it would fail.
            answer => self.accept(answer),
        }
    }

    fn check(&mut self, token: &str) {
        let answer = (self.service.check)(token);
        self.accept(answer);
    }

    fn accept(&mut self, answer: Result<(String, String), Failure>) {
        self.connecting = None;
        match answer {
            Ok((user, credential)) => {
                let sealed = self.secrets.as_ref().and_then(|s| s.seal(credential.as_bytes()).ok());
                let kept = sealed.is_some_and(|bytes| {
                    std::fs::create_dir_all(&self.dir).is_ok()
                        && std::fs::write(self.dir.join("credential.sealed"), bytes).is_ok()
                });
                self.credential = Some(credential);
                self.saved = Saved { enabled: true, user };
                self.save_state();
                self.retry_at = None;
                let note = if kept { String::new() } else { chilpa_i18n::t("the account could not be kept for next time") };
                self.checked(note);
            }
            Err(Failure::Token) => {
                self.checked(chilpa_i18n::t_args("{} did not accept the account", &[self.service.label]))
            }
            Err(_) => self.checked(chilpa_i18n::t_args("{} could not be reached; try again", &[self.service.label])),
        }
    }

    /// Sends what is waiting, oldest first, until it is all gone or the
    /// other side says to stop.
    pub(super) fn flush(&mut self) {
        while !self.pending.is_empty() {
            let Some(credential) = self.sending() else { return };
            if self.retry_at.is_some_and(|at| Instant::now() < at) {
                return;
            }
            let n = self.pending.len().min(self.service.batch);
            let kind = if n == 1 { Kind::Single } else { Kind::Import };
            match (self.service.submit)(&credential, kind, &self.pending[..n]) {
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
                    // Kept, not sent: a new account sends them.
                    self.credential = None;
                    let why = chilpa_i18n::t_args("{} no longer accepts the account; connect again", &[self.service.label]);
                    self.publish(why);
                    return;
                }
                Err(Failure::Refused(why)) if n > 1 => {
                    // One bad listen refuses the whole batch: send them one
                    // at a time, so only that one is lost.
                    let batch: Vec<Listen> = self.pending.drain(..n).collect();
                    let mut kept = Vec::new();
                    for listen in batch {
                        match (self.service.submit)(&credential, Kind::Single, std::slice::from_ref(&listen)) {
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

    /// Updates the status. Whether an account is being connected is left as
    /// it is: the facade raises it when it asks, and only the answer lowers
    /// it.
    fn publish(&self, error: String) {
        if let Ok(mut s) = self.status.lock() {
            *s = ScrobbleStatus {
                enabled: self.saved.enabled,
                user: if self.credential.is_some() { self.saved.user.clone() } else { String::new() },
                checking: s.checking,
                link: std::mem::take(&mut s.link),
                pending: self.pending.len(),
                error,
            };
        }
    }

    /// The answer to connecting an account.
    fn checked(&self, error: String) {
        if let Ok(mut s) = self.status.lock() {
            s.checking = false;
            s.link.clear();
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

#[cfg(test)]
mod tests {
    use super::*;

    fn service(submit: fn(&str, Kind, &[Listen]) -> Result<(), Failure>) -> Service {
        Service { label: "Test", batch: 100, begin: None, check: |t| Ok(("someone".into(), t.to_string())), submit }
    }

    fn worker(dir: &str, submit: fn(&str, Kind, &[Listen]) -> Result<(), Failure>, waiting: usize) -> Worker {
        let dir = std::env::temp_dir().join(dir);
        let _ = std::fs::remove_dir_all(&dir);
        Worker {
            service: service(submit),
            dir,
            secrets: None,
            credential: Some("token".into()),
            saved: Saved { enabled: true, user: "someone".into() },
            pending: (0..waiting)
                .map(|i| Listen { artist: "A".into(), track: format!("T{i}"), listened_at: i as i64, ..Listen::default() })
                .collect(),
            connecting: None,
            retry_at: None,
            wait: Duration::from_secs(60),
            status: Arc::new(Mutex::new(ScrobbleStatus::default())),
        }
    }

    #[test]
    fn what_waits_goes_out_a_batch_at_a_time_oldest_first() {
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
        w.service.submit = |_, _, _| panic!("tried before it was time");
        w.flush();
    }

    #[test]
    fn one_refused_listen_does_not_take_the_rest_with_it() {
        // A batch with T1 in it is refused; T1 alone is refused; the rest go.
        let mut w = worker("chilpa-scrobble-refused", |_, _, l| {
            if l.iter().any(|x| x.track == "T1") { Err(Failure::Refused("bad".into())) } else { Ok(()) }
        }, 3);
        w.flush();
        assert!(w.pending.is_empty(), "T0 and T2 went; T1 was dropped");
        assert_eq!(w.status.lock().unwrap().error, "bad");
    }

    #[test]
    fn an_account_that_stopped_working_keeps_the_listens_for_the_next() {
        let mut w = worker("chilpa-scrobble-token", |_, _, _| Err(Failure::Token), 2);
        w.flush();
        assert_eq!(w.pending.len(), 2);
        assert!(w.credential.is_none());
        assert!(w.status.lock().unwrap().user.is_empty(), "shown as not connected");
    }

    #[test]
    fn an_account_allowed_on_its_site_is_kept_once_allowed() {
        static ASKED: Mutex<u32> = Mutex::new(0);
        let mut w = worker("chilpa-scrobble-allow", |_, _, _| Ok(()), 0);
        w.credential = None;
        w.service.begin = Some(|| Ok(("request".into(), "https://example.org/allow".into())));
        // Not allowed the first time it is asked, allowed the second, and
        // the token does not answer a third: it is used up.
        w.service.check = |token| {
            let mut n = ASKED.lock().unwrap();
            *n += 1;
            match *n {
                1 => Err(Failure::Later(3)),
                2 => Ok(("someone".into(), format!("session-for-{token}"))),
                _ => Err(Failure::Token),
            }
        };
        w.handle(Order::Connect(String::new()));
        assert_eq!(w.status.lock().unwrap().link, "https://example.org/allow", "the page to allow it on");
        w.poll_connecting();
        assert!(w.credential.is_none(), "not allowed yet");
        w.poll_connecting();
        assert_eq!(w.credential.as_deref(), Some("session-for-request"));
        let s = w.status.lock().unwrap().clone();
        assert_eq!((s.user.as_str(), s.checking, s.link.as_str()), ("someone", false, ""));
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
