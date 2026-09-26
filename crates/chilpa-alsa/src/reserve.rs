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
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use dbus::blocking::stdintf::org_freedesktop_dbus::RequestNameReply;
use dbus::blocking::Connection;

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
    match card_of(device).filter(|_| asking()) {
        Some(card) => Reservation::take(card).map_err(chilpa_audio::sink::SinkError::Backend),
        None => Ok(None),
    }
}

/// A card held for Chilpa. Dropping it gives the card back.
pub struct Reservation {
    bus: Connection,
    name: String,
}

impl Reservation {
    /// Takes card `card`, asking whoever holds it to let go. None when
    /// there is no session bus: then there is nobody to ask, and the card is
    /// opened as it is.
    pub fn take(card: u32) -> Result<Option<Self>, String> {
        if let Some((_, parked)) = PARKED.lock().ok().and_then(|mut p| p.as_mut()?.remove(&card)) {
            return Ok(Some(parked));
        }
        let Ok(bus) = Connection::new_session() else { return Ok(None) };
        let name = format!("org.freedesktop.ReserveDevice1.Audio{card}");
        let refused = || chilpa_i18n::t_args("the sound server would not let go of card {}", &[&card.to_string()]);
        if bus.request_name(name.as_str(), false, false, true).map_err(|e| e.to_string())? != RequestNameReply::PrimaryOwner {
            let path = format!("/org/freedesktop/ReserveDevice1/Audio{card}");
            let owner = bus.with_proxy(name.as_str(), path, PATIENCE);
            let (released,): (bool,) = owner
                .method_call("org.freedesktop.ReserveDevice1", "RequestRelease", (PRIORITY,))
                .map_err(|e| e.to_string())?;
            if !released
                || bus.request_name(name.as_str(), false, true, true).map_err(|e| e.to_string())?
                    != RequestNameReply::PrimaryOwner
            {
                return Err(refused());
            }
        }
        Ok(Some(Self { bus, name }))
    }

    /// Keeps it for the opening that follows a measurement. If none comes,
    /// it goes back by itself.
    pub fn park(self, card: u32) {
        let turn = TURN.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut parked) = PARKED.lock() {
            parked.get_or_insert_with(HashMap::new).insert(card, (turn, self));
        }
        std::thread::spawn(move || {
            std::thread::sleep(PATIENCE);
            if let Ok(mut parked) = PARKED.lock() {
                if let Some(p) = parked.as_mut().filter(|p| p.get(&card).is_some_and(|(t, _)| *t == turn)) {
                    p.remove(&card);
                }
            }
        });
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        let _ = self.bus.release_name(self.name.as_str());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(owner(&name), Some(held.bus.unique_name().to_string()), "Chilpa has the card");
        drop(held);
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while before.is_some() && owner(&name).is_none() {
            assert!(std::time::Instant::now() < deadline, "the server never took the card back");
            std::thread::sleep(Duration::from_millis(100));
        }
        eprintln!("before: {before:?}, after: {:?}", owner(&name));
    }
}
