//! Keeps the machine awake while music plays: through logind, or, inside a
//! Flatpak sandbox, where the system bus is out of reach, through the
//! desktop's Inhibit portal.
//!
//! logind: while playing, an inhibitor lock is held (`Inhibit("sleep:idle",
//! ..., "block")`) so the system neither suspends nor goes idle mid-track.
//! The lock is a file descriptor: logind lets it go the moment it is
//! closed, so a pause, a stop, the end of the process or a crash all
//! release it. There is no call to undo it.
//!
//! The portal: `Inhibit("", Suspend | Idle, {reason})` answers with a
//! request, and closing that request (`Request.Close`) lets the inhibition
//! go. The portal also lets it go when the connection that asked leaves the
//! bus, so the end of the process or a crash release it as well.
//!
//! Each keeps a connection of its own, made the first time it is needed:
//! logind's on the system bus, unlike MPRIS, and the portal's on the
//! session bus. Every call gives up after the portals' call timeout.

use crate::portal::{Bus, CALL_TIMEOUT, DESKTOP, DESKTOP_PATH};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use zbus::zvariant::{OwnedFd, OwnedObjectPath, Value};

const LOGIN1_DEST: &str = "org.freedesktop.login1";
const LOGIN1_PATH: &str = "/org/freedesktop/login1";
const LOGIN1_IFACE: &str = "org.freedesktop.login1.Manager";
const INHIBIT_IFACE: &str = "org.freedesktop.portal.Inhibit";
const REQUEST_IFACE: &str = "org.freedesktop.portal.Request";
/// Of the portal's flags (Logout 1, UserSwitch 2, Suspend 4, Idle 8), the
/// two that logind's `sleep:idle` covers.
const SUSPEND_AND_IDLE: u32 = 4 | 8;
const REASON: &str = "Music playback in progress";

/// After a failure that may pass, how long to wait before trying again.
/// Every play asks for the lock, and without a wait a machine with no
/// logind session wrote the same warning down every couple of seconds.
const RETRY_BACKOFF: Duration = Duration::from_secs(300);

/// Why the last `Inhibit` attempt did not produce a lock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Denial {
    /// A permanent policy answer for this process (polkit wants an
    /// interactive auth we can never give — a daemon with no logind
    /// session; the portal refuses it). Never retried.
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
    if text.contains("InteractiveAuthorizationRequired") || text.contains("AccessDenied") || text.contains("NotAllowed") {
        Denial::Policy
    } else {
        Denial::Transient
    }
}

/// Who is asked to keep the machine awake.
#[derive(Clone, Debug)]
pub(crate) enum Backend {
    Logind,
    Portal(Bus),
}

impl Backend {
    /// The portal inside Flatpak, logind everywhere else.
    pub(crate) fn for_this_system() -> Self {
        if chilpa_dirs::sandbox() == chilpa_dirs::Sandbox::Flatpak { Self::Portal(Bus::Session) } else { Self::Logind }
    }

    fn name(&self) -> &'static str {
        match self {
            Self::Logind => "login1",
            Self::Portal(_) => "the Inhibit portal",
        }
    }
}

/// What holds the machine awake: logind's descriptor, or the portal's
/// request.
enum Held {
    /// Never read: it is held to be closed.
    Lock { _descriptor: OwnedFd },
    Request(OwnedObjectPath),
}

/// Holds the inhibition while playing. Owned by the MPRIS update loop
/// (single-threaded); acquire/release are driven by playback-state updates.
pub struct SleepInhibitor {
    backend: Backend,
    /// Lazy connection; `None` until first acquire (or if the bus is
    /// unreachable — then we retry on the next transition).
    conn: Option<zbus::Connection>,
    /// `Some` while Playing. Dropping logind's descriptor closes it, which
    /// is how logind releases the lock; the portal's request is closed.
    held: Option<Held>,
    /// Last failed attempt and why — gates the next attempt (see
    /// `should_attempt`).
    last_failure: Option<(Denial, Instant)>,
}

impl SleepInhibitor {
    pub fn new() -> Self {
        Self::with(Backend::for_this_system())
    }

    pub(crate) fn with(backend: Backend) -> Self {
        Self { backend, conn: None, held: None, last_failure: None }
    }

    /// Drive the inhibitor from a playback transition: acquire on Playing,
    /// release on Paused/Stopped. Idempotent — repeated Playing updates keep
    /// the existing lock. Failures are logged and non-fatal (playback must
    /// never depend on logind or the portal being present).
    pub async fn set_playing(&mut self, playing: bool) {
        if playing {
            self.acquire().await;
        } else {
            self.release().await;
        }
    }

    async fn connection(&mut self) -> zbus::Result<zbus::Connection> {
        if let Some(conn) = &self.conn {
            return Ok(conn.clone());
        }
        let conn = match &self.backend {
            Backend::Logind => zbus::connection::Builder::system()?.method_timeout(CALL_TIMEOUT).build().await?,
            Backend::Portal(bus) => bus.connect_async().await?,
        };
        self.conn = Some(conn.clone());
        Ok(conn)
    }

    async fn acquire(&mut self) {
        if self.held.is_some() {
            return;
        }
        if !should_attempt(self.last_failure, Instant::now()) {
            return;
        }
        let conn = match self.connection().await {
            Ok(conn) => conn,
            Err(e) => {
                eprintln!(
                    "chilpa: inhibit: the bus of {} is unavailable, cannot inhibit sleep (retry in {}s): {e}",
                    self.backend.name(),
                    RETRY_BACKOFF.as_secs()
                );
                self.last_failure = Some((Denial::Transient, Instant::now()));
                return;
            }
        };
        let held = match &self.backend {
            Backend::Logind => conn
                .call_method(Some(LOGIN1_DEST), LOGIN1_PATH, Some(LOGIN1_IFACE), "Inhibit", &("sleep:idle", "Chilpa", REASON, "block"))
                .await
                .and_then(|msg| msg.body().deserialize::<OwnedFd>().map(|fd| Held::Lock { _descriptor: fd })),
            Backend::Portal(_) => {
                let options = HashMap::from([("reason", Value::from(REASON))]);
                conn.call_method(Some(DESKTOP), DESKTOP_PATH, Some(INHIBIT_IFACE), "Inhibit", &("", SUSPEND_AND_IDLE, options))
                    .await
                    .and_then(|msg| msg.body().deserialize::<OwnedObjectPath>().map(Held::Request))
            }
        };
        match held {
            Ok(held) => {
                self.held = Some(held);
                self.last_failure = None;
            }
            Err(e) => {
                let denial = classify(&e);
                match denial {
                    Denial::Policy => eprintln!(
                        "chilpa: inhibit: {} refuses to keep the machine awake for this process ({e}); \
                         giving up — playback is unaffected",
                        self.backend.name()
                    ),
                    Denial::Transient => eprintln!(
                        "chilpa: inhibit: the Inhibit call to {} failed (retry in {}s): {e}",
                        self.backend.name(),
                        RETRY_BACKOFF.as_secs()
                    ),
                }
                self.last_failure = Some((denial, Instant::now()));
            }
        }
    }

    async fn release(&mut self) {
        // Closing logind's descriptor is what lets its lock go.
        let Some(Held::Request(request)) = self.held.take() else { return };
        let Some(conn) = &self.conn else { return };
        if let Err(e) = conn.call_method(Some(DESKTOP), request.as_str(), Some(REQUEST_IFACE), "Close", &()).await {
            eprintln!("chilpa: inhibit: the portal's inhibition could not be closed ({e}); it ends with Chilpa");
        }
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

    const REQUEST: &str = "/org/freedesktop/portal/desktop/request/1_1/chilpa";

    fn portal(bus: &crate::fake_bus::PrivateBus, refuse: bool) -> crate::fake_bus::FakeService {
        crate::fake_bus::FakeService::serve(bus, DESKTOP, move |conn, call| {
            let header = call.header();
            let _ = match header.member().map(|member| member.as_str()) {
                Some("Inhibit") if refuse => conn.reply_dbus_error(&header, zbus::fdo::Error::AccessDenied("no".into())),
                Some("Inhibit") => conn.reply(&header, &OwnedObjectPath::try_from(REQUEST).unwrap()),
                Some("Close") => conn.reply(&header, &()),
                _ => Ok(()),
            };
        })
    }

    #[test]
    fn the_portal_holds_suspend_and_idle_while_playing_and_closes_its_request_after() {
        let bus = crate::fake_bus::PrivateBus::start();
        let fake = portal(&bus, false);
        let mut inhibitor = SleepInhibitor::with(Backend::Portal(bus.bus()));
        pollster::block_on(inhibitor.set_playing(true));
        pollster::block_on(inhibitor.set_playing(true));
        assert_eq!(fake.members(), ["Inhibit"], "a second play keeps the inhibition it has");
        pollster::block_on(inhibitor.set_playing(false));
        pollster::block_on(inhibitor.set_playing(false));
        assert_eq!(fake.members(), ["Inhibit", "Close"], "a second pause has nothing to close");
        let calls = fake.calls();
        let (window, flags, options): (String, u32, HashMap<String, zbus::zvariant::OwnedValue>) = calls[0].body().deserialize().unwrap();
        assert_eq!((window.as_str(), flags), ("", 12));
        assert_eq!(String::try_from(options["reason"].clone()).unwrap(), REASON);
        assert_eq!(calls[0].header().interface().unwrap().as_str(), INHIBIT_IFACE);
        assert_eq!(calls[1].header().path().unwrap().as_str(), REQUEST);
        assert_eq!(calls[1].header().interface().unwrap().as_str(), REQUEST_IFACE);
    }

    #[test]
    fn a_portal_that_refuses_is_not_asked_again() {
        let bus = crate::fake_bus::PrivateBus::start();
        let fake = portal(&bus, true);
        let mut inhibitor = SleepInhibitor::with(Backend::Portal(bus.bus()));
        for playing in [true, false, true] {
            pollster::block_on(inhibitor.set_playing(playing));
        }
        assert_eq!(fake.members(), ["Inhibit"]);
    }
}
