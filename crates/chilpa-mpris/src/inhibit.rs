//! Keeps the machine awake while music plays, through logind.
//!
//! While playing, an inhibitor lock is held (`Inhibit("sleep:idle", ...,
//! "block")`) so the system neither suspends nor goes idle mid-track. The
//! lock is a file descriptor: logind lets it go the moment it is closed, so
//! a pause, a stop, the end of the process or a crash all release it. There
//! is no call to undo it.
//!
//! logind lives on the system bus, unlike MPRIS, so this keeps a connection
//! of its own, made the first time it is needed.

use mpris_server::zbus::{self, zvariant::OwnedFd};
use std::time::{Duration, Instant};

const LOGIN1_DEST: &str = "org.freedesktop.login1";
const LOGIN1_PATH: &str = "/org/freedesktop/login1";
const LOGIN1_IFACE: &str = "org.freedesktop.login1.Manager";

/// After a failure that may pass, how long to wait before trying again.
/// Every play asks for the lock, and without a wait a machine with no
/// logind session wrote the same warning down every couple of seconds.
const RETRY_BACKOFF: Duration = Duration::from_secs(300);

/// Why the last `Inhibit` attempt did not produce a lock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Denial {
    /// A permanent policy answer for this process (polkit wants an
    /// interactive auth we can never give — a daemon with no logind
    /// session). Never retried.
    Policy,
    /// Anything else (bus hiccup, logind restarting). Retried after
    /// `RETRY_BACKOFF`.
    Transient,
}

/// Pure retry decision, so the backoff can be unit-tested without a bus.
fn should_attempt(last: Option<(Denial, Instant)>, now: Instant) -> bool {
    match last {
        None => true,
        Some((Denial::Policy, _)) => false,
        Some((Denial::Transient, at)) => now.duration_since(at) >= RETRY_BACKOFF,
    }
}

fn classify(error: &zbus::Error) -> Denial {
    let text = error.to_string();
    if text.contains("InteractiveAuthorizationRequired") || text.contains("AccessDenied") {
        Denial::Policy
    } else {
        Denial::Transient
    }
}

/// Holds the inhibitor fd while playing. Owned by the MPRIS update loop
/// (single-threaded); acquire/release are driven by playback-state updates.
pub struct SleepInhibitor {
    /// Lazy system-bus connection; `None` until first acquire (or if the
    /// system bus is unreachable — then we retry on the next transition).
    conn: Option<zbus::Connection>,
    /// The inhibitor lock. `Some` while Playing; dropping it closes the fd,
    /// which is how logind releases the lock.
    fd: Option<OwnedFd>,
    /// Last failed attempt and why — gates the next attempt (see
    /// `should_attempt`).
    last_failure: Option<(Denial, Instant)>,
}

impl SleepInhibitor {
    pub fn new() -> Self {
        Self {
            conn: None,
            fd: None,
            last_failure: None,
        }
    }

    /// Drive the inhibitor from a playback transition: acquire on Playing,
    /// release on Paused/Stopped. Idempotent — repeated Playing updates keep
    /// the existing lock. Failures are logged and non-fatal (playback must
    /// never depend on logind being present, e.g. non-systemd distros).
    pub async fn set_playing(&mut self, playing: bool) {
        if playing {
            self.acquire().await;
        } else {
            self.release();
        }
    }

    async fn acquire(&mut self) {
        if self.fd.is_some() {
            return;
        }
        if !should_attempt(self.last_failure, Instant::now()) {
            return;
        }
        let conn = match &self.conn {
            Some(c) => c.clone(),
            None => match zbus::Connection::system().await {
                Ok(c) => {
                    self.conn = Some(c.clone());
                    c
                }
                Err(e) => {
                    eprintln!(
                        "chilpa: inhibit: system bus unavailable, cannot inhibit sleep (retry in {}s): {e}",
                        RETRY_BACKOFF.as_secs()
                    );
                    self.last_failure = Some((Denial::Transient, Instant::now()));
                    return;
                }
            },
        };
        let reply = conn
            .call_method(
                Some(LOGIN1_DEST),
                LOGIN1_PATH,
                Some(LOGIN1_IFACE),
                "Inhibit",
                &("sleep:idle", "Chilpa", "Music playback in progress", "block"),
            )
            .await;
        match reply {
            Ok(msg) => match msg.body().deserialize::<OwnedFd>() {
                Ok(fd) => {
                    self.fd = Some(fd);
                    self.last_failure = None;
                }
                Err(e) => {
                    eprintln!("chilpa: inhibit: bad Inhibit reply (expected fd): {e}");
                    self.last_failure = Some((Denial::Transient, Instant::now()));
                }
            },
            Err(e) => {
                let denial = classify(&e);
                match denial {
                    Denial::Policy => eprintln!(
                        "chilpa: inhibit: login1 refuses a sleep:idle inhibitor for this process ({e}); \
                         giving up — playback is unaffected"
                    ),
                    Denial::Transient => eprintln!(
                        "chilpa: inhibit: login1 Inhibit call failed (retry in {}s): {e}",
                        RETRY_BACKOFF.as_secs()
                    ),
                }
                self.last_failure = Some((denial, Instant::now()));
            }
        }
    }

    fn release(&mut self) {
        // Closing the descriptor is what lets the lock go.
        self.fd = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_attempt_is_always_made() {
        assert!(should_attempt(None, Instant::now()));
    }

    #[test]
    fn policy_denial_is_never_retried() {
        let at = Instant::now();
        assert!(!should_attempt(Some((Denial::Policy, at)), at));
        assert!(!should_attempt(Some((Denial::Policy, at)), at + RETRY_BACKOFF * 10));
    }

    #[test]
    fn transient_failure_waits_out_the_backoff() {
        let at = Instant::now();
        assert!(!should_attempt(Some((Denial::Transient, at)), at));
        assert!(!should_attempt(Some((Denial::Transient, at)), at + RETRY_BACKOFF / 2));
        assert!(should_attempt(Some((Denial::Transient, at)), at + RETRY_BACKOFF));
    }
}
