//! Settings own the optional metadata engine and its private installation.
mod managed;
mod packages;
pub(crate) use managed::Managed;
use crate::{Chilpa, SettingsRow};

pub(crate) const ENGINE: &str = "metadata.engine";
pub(crate) const PYTHON: &str = "metadata.beetsPython";
const INSTALL: &str = "metadata.beets.install";
const CHECK: &str = "metadata.beets.check";
const STATUS: &str = "metadata.beets.status";

impl Chilpa {
    pub(crate) fn beets_python(&self) -> String {
        self.preference(PYTHON).filter(|value| !value.is_empty())
            .or_else(|| packages::installed().map(|installation| installation.python().to_string_lossy().into_owned()))
            .unwrap_or_default()
    }
    pub(crate) fn beets_runtime_busy(&self) -> bool {
        self.beets_runtime.as_ref().is_some_and(Managed::busy)
    }
    pub fn settings_working(&self, section: &str) -> bool {
        section == "library" && self.beets_runtime_busy()
    }
    pub(crate) fn advance_beets_runtime(&mut self) {
        let Some(result) = self.beets_runtime.as_mut().and_then(Managed::poll) else { return };
        if result {
            // A completed private installation becomes the default. A failed
            // or cancelled attempt leaves the previous preference intact.
            if let Err(error) = self.remember_preference_checked(PYTHON, "") {
                self.now.error = error;
            }
        }
    }
    fn beets_manager(&mut self) -> &mut Managed {
        self.beets_runtime.get_or_insert_with(Managed::new)
    }
    pub(crate) fn beets_setting(&mut self, key: &str, value: &str) -> bool {
        let accepted = match key {
            ENGINE => self.metadata_set_engine(value),
            PYTHON => self.metadata_set_beets_runtime(value),
            _ => return false,
        };
        if !accepted { self.now.error = chilpa_i18n::t("The beets setting could not be changed while a task is running."); }
        true
    }
    pub(crate) fn beets_maintenance(&mut self, key: &str) -> Option<String> {
        match key {
            INSTALL => {
                if self.beets_runtime_busy() {
                    self.beets_manager().cancel();
                } else if self.offline() {
                    return Some(chilpa_core::network::blocked());
                } else {
                    self.metadata_cancel_lookup();
                    let lease = self.metadata_beets_lease();
                    let previous = self.beets_python();
                    self.beets_manager().install(previous, lease);
                }
            }
            CHECK => {
                if self.beets_runtime_busy() { self.beets_manager().cancel(); }
                else {
                    let python = self.beets_python();
                    self.beets_manager().check(python);
                }
            }
            _ => return None,
        }
        Some(String::new())
    }
    pub(crate) fn beets_settings_row(&mut self, row: &mut SettingsRow) -> bool {
        if !matches!(row.key.as_str(), ENGINE | PYTHON | INSTALL | CHECK | STATUS) { return false; }
        let busy = self.beets_runtime_busy();
        match row.key.as_str() {
            ENGINE => {
                row.value = self.preference(ENGINE).unwrap_or_else(|| "native".into());
                row.options = vec!["Chilpa".into(), "beets".into()];
                row.values = vec!["native".into(), "beets".into()];
                if busy { row.control = "fixed".into(); }
            }
            PYTHON => {
                row.value = self.preference(PYTHON).unwrap_or_default();
                row.options = vec![chilpa_i18n::t("All files") + " (*)"];
                if !row.value.is_empty() { row.description = row.value.clone(); }
                if busy { row.control = "fileBusy".into(); }
            }
            STATUS => {
                let python = self.beets_python();
                row.kind = 2;
                row.control = "none".into();
                row.value = self.beets_manager().description(&python);
                row.description = python;
            }
            CHECK => {
                if busy && !self.beets_manager().view().installing { row.action = chilpa_i18n::t("Cancel check"); }
                else if busy || self.beets_python().is_empty() { row.control = "buttonBusy".into(); }
            }
            INSTALL => {
                let offline = self.offline();
                let view = self.beets_manager().view();
                row.description = view.message;
                if !packages::supported() || offline && !busy || busy && !view.installing {
                    row.control = "buttonBusy".into();
                    if offline { row.description = chilpa_core::network::blocked(); }
                } else {
                    row.action = if busy { chilpa_i18n::t("Cancel") } else if packages::installed().is_some() { chilpa_i18n::t("Reinstall") } else { chilpa_i18n::t("Download and install") };
                    row.value = if busy { view.progress.to_string() } else { String::new() };
                }
            }
            _ => (),
        }
        true
    }
}

#[cfg(test)]
mod tests;
