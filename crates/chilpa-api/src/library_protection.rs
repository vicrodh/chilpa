//! Protect my library (Settings > Library): one switch, off by default,
//! that keeps Chilpa from changing the music files and the servers, so an
//! accident cannot.
//!
//! On, every place that writes a file's tags or artwork, deletes from a
//! server, changes a server's playlist or syncs into a library folder
//! refuses, and the window greys those actions out. Hearts, stars and
//! their sync, adding sources and folders, Chilpa's own playlists and the
//! caches are untouched. Its child lets metadata edits go to sidecars: a
//! `.chilpa.json` beside the music, or Chilpa's own copy for a record on a
//! server.

use crate::settings_rows::SettingsRow;
use crate::Chilpa;

pub const ENABLED: &str = "library.protect";
pub const SIDECARS: &str = "library.protect.sidecars";

const REFUSED: &str = chilpa_i18n::mark("Protect my library is on. Turn it off in Settings > Library to change your files or servers.");

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Protection {
    pub(crate) on: bool,
    /// Sidecars are allowed while it is on.
    pub(crate) sidecars: bool,
}

impl Protection {
    /// As kept, read from the preferences so a worker answers as the
    /// window does.
    pub(crate) fn kept() -> Self {
        let window = chilpa_core::settings::load().window;
        let on = |key: &str| window.get(key).is_some_and(|value| value == "true");
        Protection { on: on(ENABLED), sidecars: on(SIDECARS) }
    }

    /// Why a write to a file or a server is refused; empty when it is not.
    pub(crate) fn writes_refused(self) -> String {
        if self.on { chilpa_i18n::t(REFUSED) } else { String::new() }
    }

    /// Why a sidecar is refused; empty when it is not.
    pub(crate) fn sidecar_refused(self) -> String {
        if self.on && !self.sidecars { chilpa_i18n::t(REFUSED) } else { String::new() }
    }
}

impl Chilpa {
    pub fn library_protected(&self) -> bool {
        Protection::kept().on
    }

    /// Sidecars are allowed while the library is protected.
    pub fn protected_sidecars(&self) -> bool {
        Protection::kept().sidecars
    }

    pub(crate) fn library_protection_setting(&mut self, key: &str, value: &str) -> bool {
        if !matches!(key, ENABLED | SIDECARS) || !matches!(value, "true" | "false") {
            return false;
        }
        self.remember_preference_checked(key, value).is_ok()
    }

    pub(crate) fn library_protection_row(&self, row: &mut SettingsRow) -> bool {
        let protection = Protection::kept();
        match row.key.as_str() {
            ENABLED => row.value = protection.on.to_string(),
            SIDECARS => {
                row.value = protection.sidecars.to_string();
                if !protection.on {
                    row.control = "fixed".into();
                }
            }
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests;
