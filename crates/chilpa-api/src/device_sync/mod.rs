//! I expose device profiles, Library selections and bounded background jobs to every frontend.
mod catalog;
mod drafts;
mod episodes;
pub use episodes::Episode;
mod selection;
mod service;
mod work;
mod view;
mod library;
mod browser;
pub use catalog::{Catalog, Pick, Profile, Root, Store};
pub use selection::OptionRow;
use serde::Serialize;
pub use service::{Service, Status};
pub use work::{ChangeRow, Destination};

pub(crate) const SHOW_IN_SIDEBAR: &str = "navigation.devices";

impl crate::Chilpa {
    pub(crate) fn device_sync_setting(&mut self, key: &str, value: &str) -> bool {
        if key != SHOW_IN_SIDEBAR || !matches!(value, "true" | "false") { return false; }
        self.remember_preference(key, value);
        true
    }
}

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

/// I leave external albums out before reading local tracks or staging device work.
pub fn device_albums_available(ids: &[i64]) -> bool { ids.iter().any(|id| *id > 0) }
impl crate::Chilpa {
    pub fn device_album_tracks(&mut self, albums: &[i64]) -> Vec<i64> {
        let mut ids = Vec::new();
        for album in albums.iter().copied().filter(|id| *id > 0) {
            ids.extend(self.album_track_rows(album).into_iter().map(|track| track.id));
            if ids.len() > chilpa_sync::MAX_ASSETS { break; }
        }
        if albums.iter().any(|id| *id < 0) {
            self.now.error = chilpa_i18n::t("Albums from streaming services will not be synced.");
        }
        ids
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn mixed_albums_export_only_local_tracks_and_report_omissions() {
        let directory = tempfile::tempdir().unwrap();
        let library = chilpa_library::Library::open(&directory.path().join("library.sqlite3")).unwrap();
        let source = library.source_id("local", "default").unwrap();
        library.connection().execute_batch("INSERT INTO albums(id,title) VALUES (7,'First'),(9,'Second')").unwrap();
        library.connection().execute("INSERT INTO tracks(id,source_id,external_id,title,album_id,disc_number,track_number,added_at) VALUES (81,?1,'one.flac','One',7,1,1,1),(91,?1,'two.flac','Two',9,1,1,1)", [source]).unwrap();
        let mut app = crate::Chilpa::new();
        app.library = Some(library);
        assert!(super::device_albums_available(&[7,-9]));
        assert!(!super::device_albums_available(&[-7,-9]));
        assert_eq!(app.device_album_tracks(&[7, -7, 9]), [81,91]);
        assert!(!app.now.error.is_empty());
        assert_eq!(crate::album_action_targets(0, &[7,9]), [7,9]);
        assert_eq!(crate::album_action_targets(7, &[7,9]), [7,9]);
        assert_eq!(crate::album_action_targets(8, &[7,9]), [8]);
    }
}
