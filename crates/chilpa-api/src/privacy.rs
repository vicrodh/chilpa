//! I keep optional services and offline operation under one network policy.

use chilpa_core::{network, settings::Settings};

pub(crate) fn policy(saved: &Settings) -> network::Policy {
    let enabled = |key| saved.window.get(key).is_none_or(|value| value != "false");
    network::Policy {
        offline: saved.window.get("integrations.offline").is_some_and(|value| value == "true"),
        proxy: enabled("integrations.proxy.enabled"),
        musicbrainz: enabled("integrations.musicbrainz.enabled"),
        lyrics: enabled("integrations.lyrics.enabled"),
        lyrics_service: enabled("integrations.lyrics.service"),
    }
}

/// Qt's image loader uses the same policy as the engine's HTTP clients.
pub fn network_allowed(address: &str) -> bool { network::current().allows(address) }

impl crate::Chilpa {
    pub fn offline(&self) -> bool { network::current().offline }

    pub(crate) fn privacy_changed(&mut self, previous: network::Policy, saved: &Settings) {
        let next = policy(saved);
        if previous == next { return; }
        network::configure(next);
        self.artist_scene.close();
        self.artist_network.clear();
        self.catalog.changed();
        if previous.offline != next.offline {
            if next.offline {
                self.appimage_updates_cancel();
                self.cast_offline();
                if crate::source_opener::of_plugin(&self.now.path).is_some()
                    || self.now.path.starts_with("http://") || self.now.path.starts_with("https://")
                    || chilpa_models::RemoteEntry::parse(&self.now.path).is_some() { self.stop(); }
            }
            self.natives.network_changed(self.catalog.clone());
        }
    }
}
