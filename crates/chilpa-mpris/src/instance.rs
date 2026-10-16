//! One Chilpa at a time.
//!
//! The first launch owns the application's name on the session bus. A
//! second one finds it taken, asks the first to show its window and ends.
//! There is no lock file to go stale: the bus lets the name go when its
//! owner's connection closes, however the process ended.
//!
//! When the bus cannot be asked, the launch goes on: two windows are a
//! smaller harm than none.

use crate::APP_ID;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::Duration;
use zbus::blocking::{connection::Builder, Connection};
use zbus::fdo::{RequestNameFlags, RequestNameReply};

const OBJECT_PATH: &str = "/app/chilpa/Chilpa";
const INTERFACE: &str = "app.chilpa.Chilpa.SingleInstance";
/// Everything asked of the bus at launch has this long, all told.
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);
/// An address nothing answers at: what a bus that hung is replaced with,
/// for this launch, so the window's toolkit does not hang on it too.
const UNAVAILABLE_BUS: &str = "unix:path=/dev/null";
/// Set to launch beside a Chilpa that is already open, for a test run in a
/// profile of its own.
const ALLOW_SECOND: &str = "CHILPA_SECOND_INSTANCE";

/// The connection that owns the name, kept for as long as the process lives.
static OWNER: OnceLock<Connection> = OnceLock::new();
/// A later launch asked for the window and nobody has shown it yet.
static PRESENT: AtomicBool = AtomicBool::new(false);

struct Instance;

#[zbus::interface(name = "app.chilpa.Chilpa.SingleInstance")]
impl Instance {
    fn present(&self) {
        PRESENT.store(true, Ordering::SeqCst);
    }
}

/// Whether a later launch asked for the window since the last time this was
/// read. The window reads it once it exists, so a launch that came early is
/// not lost.
pub fn take_present() -> bool {
    PRESENT.swap(false, Ordering::SeqCst)
}

/// Takes the name, or hands over to whoever has it. `false` means another
/// Chilpa is open and was asked to show itself: this launch should end.
pub fn acquire() -> bool {
    if std::env::var_os(ALLOW_SECOND).is_some() {
        return true;
    }
    let (tx, rx) = std::sync::mpsc::channel();
    let asked = std::thread::Builder::new().name("chilpa-instance".into()).spawn(move || {
        let _ = tx.send(probe());
    });
    if asked.is_err() {
        return true;
    }
    match rx.recv_timeout(PROBE_TIMEOUT) {
        Ok(Ok(Some(owner))) => {
            let _ = OWNER.set(owner);
            true
        }
        Ok(Ok(None)) => false,
        Ok(Err(e)) => {
            eprintln!("chilpa: could not ask the session bus for another instance ({e}); starting anyway");
            true
        }
        Err(_) => {
            eprintln!("chilpa: the session bus did not answer in {} s; starting without it", PROBE_TIMEOUT.as_secs());
            // SAFETY: nothing else runs yet; this is the first thing the
            // process does, before any thread of its own reads the
            // environment.
            unsafe { std::env::set_var("DBUS_SESSION_BUS_ADDRESS", UNAVAILABLE_BUS) };
            true
        }
    }
}

/// `Some` with the connection when this process now owns the name, `None`
/// when another one does and was told to show itself.
fn probe() -> zbus::Result<Option<Connection>> {
    let conn = Builder::session()?.serve_at(OBJECT_PATH, Instance)?.build()?;
    match conn.request_name_with_flags(APP_ID, RequestNameFlags::DoNotQueue.into()) {
        Ok(RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner) => return Ok(Some(conn)),
        Ok(_) | Err(zbus::Error::NameTaken) => {}
        Err(e) => return Err(e),
    }
    if conn.call_method(Some(APP_ID), OBJECT_PATH, Some(INTERFACE), "Present", &()).is_err() {
        // An owner that does not answer to this still has media controls.
        let player = format!("org.mpris.MediaPlayer2.{APP_ID}");
        conn.call_method(Some(player.as_str()), "/org/mpris/MediaPlayer2", Some("org.mpris.MediaPlayer2"), "Raise", &())?;
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_to_show_the_window_is_kept_until_it_is_read() {
        assert!(!take_present());
        Instance.present();
        assert!(take_present(), "kept for a window that did not exist yet");
        assert!(!take_present(), "and read once");
    }
}
