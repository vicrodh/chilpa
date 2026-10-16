//! Chilpa on a Linux desktop's session bus: the MPRIS media controls, the
//! tray icon, the lock that keeps the machine awake while music plays, and
//! the name that makes a second launch hand over to the first.
//!
//! Nothing here knows the player: it is told what plays and says what was
//! asked. The facade does the rest.
#![cfg(target_os = "linux")]

mod inhibit;
pub mod instance;
mod server;
pub mod tray;
mod types;
pub mod wayland_window;

pub use server::{spawn, Handle};
pub use types::{MediaEvent, PlaybackStatus, TrackMeta};

/// The application's name on the bus and the name of its desktop entry.
pub const APP_ID: &str = "app.chilpa.Chilpa";
