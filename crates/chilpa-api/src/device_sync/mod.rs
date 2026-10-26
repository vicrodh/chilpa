//! I expose device profiles, Library selections and bounded background jobs to every frontend.
mod catalog;
mod selection;
mod service;
mod work;
mod view;
pub use catalog::{Catalog, Pick, Profile, Root, Store};
pub use selection::OptionRow;
use serde::Serialize;
pub use service::{Service, Status};
pub use work::{ChangeRow, Destination};

#[derive(Clone, Debug, Serialize)]
pub struct UiError {
    pub message: String,
    pub detail: String,
    pub recovery: bool,
}
pub(crate) fn failure(message: &str, detail: impl std::fmt::Display) -> UiError {
    UiError {
        message: chilpa_i18n::t(message),
        detail: detail.to_string(),
        recovery: false,
    }
}
impl From<chilpa_sync::Error> for UiError {
    fn from(error: chilpa_sync::Error) -> Self {
        use chilpa_sync::{Error, TransportProblem as Problem};
        let message = match &error {
            Error::Cancelled => {
                chilpa_i18n::mark("Device sync was cancelled. Its progress is saved for recovery.")
            }
            Error::Space { .. } => chilpa_i18n::mark(
                "There is not enough space for this selection and its temporary files.",
            ),
            Error::Transport {
                problem: Problem::Disconnected | Problem::Timeout,
                ..
            } => chilpa_i18n::mark("The player is not responding. Reconnect it and try again."),
            Error::Transport {
                problem: Problem::Busy,
                ..
            } => chilpa_i18n::mark(
                "The player is busy. Close the other file transfer application and try again.",
            ),
            Error::Transport {
                problem: Problem::Permission,
                ..
            } => chilpa_i18n::mark("I do not have permission to access this USB device."),
            Error::Conflict(_) => {
                chilpa_i18n::mark("The device review needs attention before continuing.")
            }
            _ => chilpa_i18n::mark(
                "I could not finish this device operation. I kept its recovery information.",
            ),
        };
        failure(message, error)
    }
}
pub(crate) fn bytes(value: u64) -> String {
    for (size, unit) in [
        (1u64 << 40, "TiB"),
        (1 << 30, "GiB"),
        (1 << 20, "MiB"),
        (1 << 10, "KiB"),
    ] {
        if value >= size {
            return format!("{:.1} {unit}", value as f64 / size as f64);
        }
    }
    format!("{value} B")
}

impl crate::Chilpa {
    /// I initialize the main connection before device workers open read-only snapshots.
    pub fn open_device_library(&mut self) -> Result<(), UiError> {
        self.library().map(|_| ()).ok_or_else(|| {
            failure(
                chilpa_i18n::mark("Open the Library before choosing music for a device."),
                "",
            )
        })
    }
}
