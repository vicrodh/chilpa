//! One Chilpa at a time.
//!
//! The first launch owns the application's name on the session bus. A
//! second one finds it taken, asks the first to show its window and ends.
//! There is no lock file to go stale: the bus lets the name go when its
//! owner's connection closes, however the process ended.
//!
//! A launch that carries paths (the file manager's "Open with") hands them
//! to the first one the same way, and it opens them as temporary media.
//!
//! When the bus cannot be asked, the launch goes on: two windows are a
//! smaller harm than none.

use crate::APP_ID;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
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
/// Paths a launch asked to open, in the order they came, not yet taken.
static OPENED: Mutex<Vec<String>> = Mutex::new(Vec::new());

struct Instance;

#[zbus::interface(name = "app.chilpa.Chilpa.SingleInstance")]
impl Instance {
    fn present(&self) {
        PRESENT.store(true, Ordering::SeqCst);
    }

    /// A later launch with paths to open: they wait for the window, which
    /// is shown too.
    fn open(&self, paths: Vec<String>) {
        keep_opened(paths);
        PRESENT.store(true, Ordering::SeqCst);
    }
}

fn keep_opened(paths: Vec<String>) {
    if let Ok(mut kept) = OPENED.lock() {
        kept.extend(paths);
    }
}

/// The paths launches asked to open since the last time this was read.
pub fn take_opened() -> Vec<String> {
    OPENED.lock().map(|mut kept| std::mem::take(&mut *kept)).unwrap_or_default()
}

/// Whether a later launch asked for the window since the last time this was
/// read. The window reads it once it exists, so a launch that came early is
/// not lost.
pub fn take_present() -> bool {
    PRESENT.swap(false, Ordering::SeqCst)
}

/// Takes the name, or hands over to whoever has it. `false` means another
/// Chilpa is open and was asked to show itself, and to open `paths`: this
/// launch should end. When this one stays, `paths` are its own to open.
pub fn acquire(paths: &[String]) -> bool {
    if std::env::var_os(ALLOW_SECOND).is_some() {
        keep_opened(paths.to_vec());
        return true;
    }
    let (tx, rx) = std::sync::mpsc::channel();
    let handed = paths.to_vec();
    let asked = std::thread::Builder::new().name("chilpa-instance".into()).spawn(move || {
        let _ = tx.send(probe(&handed));
    });
    if asked.is_err() {
        keep_opened(paths.to_vec());
        return true;
    }
    match rx.recv_timeout(PROBE_TIMEOUT) {
        Ok(Ok(Some(owner))) => {
            let _ = OWNER.set(owner);
            keep_opened(paths.to_vec());
            true
        }
        Ok(Ok(None)) => false,
        Ok(Err(e)) => {
            eprintln!("chilpa: could not ask the session bus for another instance ({e}); starting anyway");
            keep_opened(paths.to_vec());
            true
        }
        Err(_) => {
            keep_opened(paths.to_vec());
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
fn probe(paths: &[String]) -> zbus::Result<Option<Connection>> {
    let conn = Builder::session()?.serve_at(OBJECT_PATH, Instance)?.build()?;
    match conn.request_name_with_flags(APP_ID, RequestNameFlags::DoNotQueue.into()) {
        Ok(RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner) => return Ok(Some(conn)),
        Ok(_) | Err(zbus::Error::NameTaken) => {}
        Err(e) => return Err(e),
    }
    let asked = if paths.is_empty() {
        conn.call_method(Some(APP_ID), OBJECT_PATH, Some(INTERFACE), "Present", &())
    } else {
        conn.call_method(Some(APP_ID), OBJECT_PATH, Some(INTERFACE), "Open", &(paths,))
    };
    if asked.is_err() {
        // An owner that does not answer to this still has media controls.
        let player = format!("org.mpris.MediaPlayer2.{APP_ID}");
        conn.call_method(Some(player.as_str()), "/org/mpris/MediaPlayer2", Some("org.mpris.MediaPlayer2"), "Raise", &())?;
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tests share this module's statics, so they take turns.
    static TURN: Mutex<()> = Mutex::new(());

    #[test]
    fn a_request_to_show_the_window_is_kept_until_it_is_read() {
        let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
        assert!(!take_present());
        Instance.present();
        assert!(take_present(), "kept for a window that did not exist yet");
        assert!(!take_present(), "and read once");
    }

    #[test]
    fn paths_a_later_launch_hands_over_wait_in_order_until_they_are_taken() {
        let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
        take_opened();
        Instance.open(vec!["/music/a.flac".into(), "/music/b".into()]);
        Instance.open(vec!["/music/c.flac".into()]);
        assert_eq!(take_opened(), ["/music/a.flac", "/music/b", "/music/c.flac"]);
        assert!(take_opened().is_empty(), "and are taken once");
        assert!(take_present(), "the window is shown for them");
    }
}
