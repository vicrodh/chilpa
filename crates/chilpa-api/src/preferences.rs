//! What the window remembers between runs: its layout, its own
//! preferences by dotted key, and the last folder opened.

pub(crate) const COMMANDS: &[crate::commands::Command] = &[
    crate::commands::command("set", "settings.set", "key value"),
];

use crate::Chilpa;

impl Chilpa {
    /// Remembers the chosen window layout.
    pub fn remember_layout(&mut self, layout: &str) {
        let mut s = chilpa_core::settings::load();
        s.layout = layout.to_string();
        let _ = chilpa_core::settings::save(&s);
    }

    /// A preference the window keeps for itself, by its dotted key
    /// (`appearance.density`). `None` when it was never chosen, so the
    /// window's own default applies.
    pub fn preference(&self, key: &str) -> Option<String> {
        chilpa_core::settings::load().window.get(key).cloned()
    }

    /// Remembers one of the window's preferences. The key has to be dotted:
    /// a plain word is one of the engine's own and is not the window's to
    /// write.
    pub fn remember_preference(&mut self, key: &str, value: &str) {
        if !key.contains('.') {
            return;
        }
        let mut s = chilpa_core::settings::load();
        s.window.insert(key.to_string(), value.to_string());
        let _ = chilpa_core::settings::save(&s);
        self.lyrics_preference_changed(key);
    }

    /// Remembers the last folder opened.
    pub fn remember_folder(&mut self, folder: &str) {
        let mut s = chilpa_core::settings::load();
        s.last_folder = folder.to_string();
        let _ = chilpa_core::settings::save(&s);
    }
}
