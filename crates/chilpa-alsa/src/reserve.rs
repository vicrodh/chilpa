//! Asking the sound server for a card before opening it, over the session
//! bus (`org.freedesktop.ReserveDevice1`).
//!
//! PipeWire and PulseAudio hold every card they manage, so opening `hw:`
//! while they run fails with "busy". The reservation protocol is how
//! programs ask each other for one: whoever owns the bus name
//! `org.freedesktop.ReserveDevice1.Audio<N>` has card N, and another asks
//! the owner to release it, with a priority. The server closes the card,
//! lets go of the name, and takes the card back when the name is free
//! again -- which is what Chilpa does when it closes the card, or lets go of
//! it in a long pause.
//!
//! Measuring a card comes right before opening it. What the measurement
//! took is parked for the opening to pick up rather than handed back in
//! between, so the server does not reopen the card only to be asked for it
//! again a moment later; if nothing picks it up, it goes back by itself.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::thread::JoinHandle;
use std::time::Duration;

use dbus::blocking::stdintf::org_freedesktop_dbus::RequestNameReply;
use dbus::blocking::SyncConnection;
use dbus::channel::MatchingReceiver;
use dbus::message::MatchRule;
use dbus_crossroads::Crossroads;

static ASKING: AtomicBool = AtomicBool::new(false);

/// What a measurement took and nobody opened yet, by card, with the turn it
/// was parked on: a timer only takes back the one it was started for.
static PARKED: Mutex<Option<HashMap<u32, (u64, Reservation)>>> = Mutex::new(None);
static TURN: AtomicU64 = AtomicU64::new(0);

/// Above the servers, which hold cards at 0 (PulseAudio) or below
/// (WirePlumber, -20): a server asked at this priority lets go.
const PRIORITY: i32 = 5;

/// How long the owner has to answer, and how long a parked card waits.
const PATIENCE: Duration = Duration::from_secs(3);

/// Whether cards are asked for before they are opened.
pub fn set_asking(on: bool) {
    chilpa_audio::diagnostics::trace("reserve", "setting_changed", format_args!("asking={on}"));
    ASKING.store(on, Ordering::Relaxed);
}

pub fn asking() -> bool {
    ASKING.load(Ordering::Relaxed)
}

/// The card an ALSA name ends at: `hw:1,0`, `plughw:1,0` and
/// `sysdefault:CARD=1` are card 1.
pub fn card_of(device: &str) -> Option<u32> {
    if let Some(card) = device.strip_prefix("sysdefault:CARD=") {
        return card.parse().ok();
    }
    let rest = device.strip_prefix("hw:").or_else(|| device.strip_prefix("plughw:"))?;
    rest.split(',').next()?.parse().ok()
}

/// Takes the card an ALSA name ends at, when cards are being asked for.
/// None when they are not, or the name is not a numbered card, or there is
/// nobody to ask.
pub fn hold(device: &str) -> Result<Option<Reservation>, chilpa_audio::sink::SinkError> {
    let card = card_of(device);
    let asking = asking();
    chilpa_audio::diagnostics::trace(
        "reserve",
        "hold_requested",
        format_args!("device={device:?} card={card:?} asking={asking}"),
    );
    match card.filter(|_| asking) {
        Some(card) => Reservation::take(card).map_err(chilpa_audio::sink::SinkError::Backend),
        None => {
            chilpa_audio::diagnostics::trace("reserve", "hold_skipped", format_args!("device={device:?}"));
            Ok(None)
        }
    }
}

/// A card held for Chilpa. Dropping it gives the card back.
pub struct Reservation {
    _bus: Arc<SyncConnection>,
    name: String,
    _server: Server,
}

struct Server {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

struct ReservationInfo {
    device: String,
}

impl Reservation {
    /// Takes card `card`, asking whoever holds it to let go. None when
    /// there is no session bus: then there is nobody to ask, and the card is
    /// opened as it is.
    pub fn take(card: u32) -> Result<Option<Self>, String> {
        if let Some((_, parked)) = PARKED.lock().ok().and_then(|mut p| p.as_mut()?.remove(&card)) {
            chilpa_audio::diagnostics::trace("reserve", "parked_claimed", format_args!("card={card}"));
            return Ok(Some(parked));
        }
        let started = std::time::Instant::now();
        chilpa_audio::diagnostics::trace("reserve", "take_started", format_args!("card={card}"));
        let Ok(bus) = SyncConnection::new_session() else {
            chilpa_audio::diagnostics::trace(
                "reserve",
                "session_bus_missing",
                format_args!("after={:?} card={card}", started.elapsed()),
            );
            return Ok(None);
        };
        let name = format!("org.freedesktop.ReserveDevice1.Audio{card}");
        let refused = || chilpa_i18n::t_args("the sound server would not let go of card {}", &[&card.to_string()]);
        // Be ready to answer on the reservation object before acquiring its
        // well-known name.  WirePlumber asks the new owner immediately; a
        // name that appears before its object exists is treated as a broken
        // participant and is not watched for a later hand-back.
        let bus = Arc::new(bus);
        let stop = Arc::new(AtomicBool::new(false));
        let server = Server { worker: Some(serve(&bus, &stop, card)), stop };
        // Queue before asking the current owner to release.  That ordering is
        // the protocol's hand-off: D-Bus promotes us atomically when the
        // server lets go.  A do-not-queue request followed by
        // `replace_existing` has a small but real race in which WirePlumber
        // reacquires the name first and we then steal it without a second
        // RequestRelease.  WirePlumber consequently believes it still owns
        // the card and does not try again when Chilpa stops.
        let request_started = std::time::Instant::now();
        let first = match bus.request_name(name.as_str(), false, false, false) {
            Ok(reply) => reply,
            Err(error) => {
                chilpa_audio::diagnostics::trace(
                    "reserve",
                    "name_request_failed",
                    format_args!("after={:?} card={card} stage=queue error={error}", request_started.elapsed()),
                );
                return Err(error.to_string());
            }
        };
        chilpa_audio::diagnostics::trace(
            "reserve",
            "name_requested",
            format_args!("after={:?} card={card} reply={first:?}", request_started.elapsed()),
        );
        if !matches!(first, RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner) {
            let path = format!("/org/freedesktop/ReserveDevice1/Audio{card}");
            let owner = bus.with_proxy(name.as_str(), path, PATIENCE);
            let release_started = std::time::Instant::now();
            chilpa_audio::diagnostics::trace(
                "reserve",
                "release_requested",
                format_args!("card={card} priority={PRIORITY}"),
            );
            let (released,): (bool,) = match owner
                .method_call("org.freedesktop.ReserveDevice1", "RequestRelease", (PRIORITY,))
            {
                Ok(reply) => reply,
                Err(error) => {
                    chilpa_audio::diagnostics::trace(
                        "reserve",
                        "release_failed",
                        format_args!("after={:?} card={card} error={error}", release_started.elapsed()),
                    );
                    return Err(error.to_string());
                }
            };
            chilpa_audio::diagnostics::trace(
                "reserve",
                "release_answered",
                format_args!("after={:?} card={card} released={released}", release_started.elapsed()),
            );
            let second_started = std::time::Instant::now();
            let second = match bus.request_name(name.as_str(), false, false, false) {
                Ok(reply) => reply,
                Err(error) => {
                    chilpa_audio::diagnostics::trace(
                        "reserve",
                        "name_request_failed",
                        format_args!("after={:?} card={card} stage=claim error={error}", second_started.elapsed()),
                    );
                    return Err(error.to_string());
                }
            };
            chilpa_audio::diagnostics::trace(
                "reserve",
                "name_claimed",
                format_args!("after={:?} card={card} reply={second:?}", second_started.elapsed()),
            );
            if !released || !matches!(second, RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner) {
                // Disconnecting this request's private connection below also
                // removes a queued name request. Do not issue synchronous
                // ReleaseName here: that is the call that stalled playback
                // teardown for seconds on an unhealthy session bus.
                chilpa_audio::diagnostics::trace(
                    "reserve",
                    "take_refused",
                    format_args!("after={:?} card={card} released={released} reply={second:?}", started.elapsed()),
                );
                return Err(refused());
            }
        }
        chilpa_audio::diagnostics::trace(
            "reserve",
            "take_finished",
            format_args!("after={:?} card={card}", started.elapsed()),
        );
        Ok(Some(Self { _bus: bus, name, _server: server }))
    }

    /// Keeps it for the opening that follows a measurement. If none comes,
    /// it goes back by itself.
    pub fn park(self, card: u32) {
        let turn = TURN.fetch_add(1, Ordering::Relaxed);
        chilpa_audio::diagnostics::trace("reserve", "parked", format_args!("card={card} turn={turn}"));
        if let Ok(mut parked) = PARKED.lock() {
            parked.get_or_insert_with(HashMap::new).insert(card, (turn, self));
        }
        std::thread::spawn(move || {
            std::thread::sleep(PATIENCE);
            if let Ok(mut parked) = PARKED.lock() {
                if let Some(p) = parked.as_mut().filter(|p| p.get(&card).is_some_and(|(t, _)| *t == turn)) {
                    chilpa_audio::diagnostics::trace("reserve", "park_expired", format_args!("card={card} turn={turn}"));
                    p.remove(&card);
                }
            }
        });
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        // A synchronous ReleaseName call can wait on a wedged session-bus
        // peer for many seconds. Closing this reservation's connection has
        // exactly the same ownership semantics and cannot strand a queued
        // request; the server worker is stopped below and the final Arc drop
        // disconnects from D-Bus.
        chilpa_audio::diagnostics::trace(
            "reserve",
            "release_by_disconnect",
            format_args!("name={:?}", self.name),
        );
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        chilpa_audio::diagnostics::trace("reserve", "server_stopping", format_args!(""));
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        chilpa_audio::diagnostics::trace("reserve", "server_stopped", format_args!(""));
    }
}

/// Publishes the other half of a reservation: not only the well-known bus
/// name, but the object the protocol says its owner implements.  In
/// particular WirePlumber watches this object before queuing to take the
/// card back.  Returning false to another release request is deliberate:
/// the PCM and this reservation must be relinquished together by the sink,
/// never one while the other is still open.
fn serve(bus: &Arc<SyncConnection>, stop: &Arc<AtomicBool>, card: u32) -> JoinHandle<()> {
    let mut crossroads = Crossroads::new();
    let reservation = crossroads.register("org.freedesktop.ReserveDevice1", |interface| {
        interface.method(
            "RequestRelease",
            ("priority",),
            ("result",),
            |_, _: &mut ReservationInfo, (_priority,): (i32,)| Ok((false,)),
        );
        interface.property::<i32, _>("Priority").get(|_, _| Ok(PRIORITY));
        interface.property::<String, _>("ApplicationName").get(|_, _| Ok("Chilpa".to_string()));
        interface
            .property::<String, _>("ApplicationDeviceName")
            .get(|_, info| Ok(info.device.clone()));
    });
    let path = format!("/org/freedesktop/ReserveDevice1/Audio{card}");
    crossroads.insert(path, &[reservation], ReservationInfo { device: format!("ALSA card {card}") });
    let crossroads = Mutex::new(crossroads);
    bus.start_receive(
        MatchRule::new_method_call(),
        Box::new(move |message, connection| {
            if let Ok(mut crossroads) = crossroads.lock() {
                let _ = crossroads.handle_message(message, connection);
            }
            true
        }),
    );

    let bus = Arc::clone(bus);
    let stop = Arc::clone(stop);
    std::thread::spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            let _ = bus.process(Duration::from_millis(10));
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use dbus::blocking::Connection;

    #[test]
    fn a_card_is_read_out_of_either_name() {
        assert_eq!(card_of("hw:1,0"), Some(1));
        assert_eq!(card_of("plughw:12,3"), Some(12));
        assert_eq!(card_of("sysdefault:CARD=1"), Some(1));
        assert_eq!(card_of("default"), None);
        assert_eq!(card_of("hw:CARD=ZH3,DEV=0"), None, "only numbered cards are asked for");
    }

    /// Who owns a bus name, or None when nobody does.
    fn owner(name: &str) -> Option<String> {
        let bus = Connection::new_session().ok()?;
        let dbus = bus.with_proxy("org.freedesktop.DBus", "/org/freedesktop/DBus", PATIENCE);
        dbus.method_call("org.freedesktop.DBus", "GetNameOwner", (name,)).ok().map(|(o,): (String,)| o)
    }

    #[test]
    #[ignore = "takes a real card from the running sound server; plays nothing"]
    fn the_server_lets_go_of_the_card_and_takes_it_back() {
        let card: u32 = std::env::var("CHILPA_RESERVE_CARD").ok().and_then(|c| c.parse().ok()).unwrap_or(1);
        let name = format!("org.freedesktop.ReserveDevice1.Audio{card}");
        let before = owner(&name);
        let held = Reservation::take(card).expect("the server lets go").expect("a session bus");
        assert_eq!(owner(&name), Some(held._bus.unique_name().to_string()), "Chilpa has the card");
        // The former owner immediately asks the newly published object for
        // the priority.  Let that handshake finish before simulating stop.
        std::thread::sleep(Duration::from_millis(100));
        drop(held);
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while before.is_some() && owner(&name).is_none() {
            assert!(std::time::Instant::now() < deadline, "the server never took the card back");
            std::thread::sleep(Duration::from_millis(100));
        }
        eprintln!("before: {before:?}, after: {:?}", owner(&name));
    }
}
