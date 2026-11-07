//! About Chilpa: the window that says what this is, which build it is and
//! whom it thanks, the updates of Chilpa and its plugins, and the deck that
//! welcomes a new install and says what is new after an update, as QBZ's
//! About held its What's New. The documents are the facade's (`chilpa_api`);
//! this crate adds the build's stamp and draws them.

pub mod bridge {
    pub mod about;
    pub mod developers;
    pub mod updates;
    pub mod plugin_updates;
    pub mod onboarding;
}

mod log_capture;
pub use log_capture::{start_logging, LogCapture};

/// The build script's choice of commit, compiled here for its tests only.
#[cfg(test)]
#[path = "../build/stamp.rs"]
mod build_stamp;
