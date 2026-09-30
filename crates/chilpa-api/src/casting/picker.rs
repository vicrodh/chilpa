//! The output menu is built beside the session, as with the track action menu.
use crate::Chilpa;
use chilpa_cast::Kind;
use chilpa_i18n as i18n;
use serde_json::{json, Value};

impl Chilpa {
    pub fn cast_json(&self) -> String {
        let (protocol, icon) = match self.casting.selected.as_ref().map(|d| &d.kind) {
            Some(Kind::Dlna) => ("DLNA / UPnP", "dlna"), Some(Kind::Chromecast) => ("Chromecast", "cast"), None => ("", ""),
        };
        let state = if !self.casting.connected && !self.casting.error.is_empty() { i18n::t("Connection error") }
            else if !self.casting.active() { i18n::t("Disconnected") }
            else if !self.casting.connected { i18n::t("Connecting…") }
            else if self.casting.loading { i18n::t("Loading…") }
            else if self.now.paused { i18n::t("Paused") }
            else if self.now.playing { i18n::t("Playing") } else { i18n::t("Connected") };
        json!({"entries": self.cast_entries(&state), "searching": self.casting.discovery.is_some(),
            "active": self.casting.active(), "name": self.cast_destination().unwrap_or_default(),
            "protocol": protocol, "icon": icon, "state": state, "error": self.casting.error}).to_string()
    }

    fn cast_entries(&self, state: &str) -> Vec<Value> {
        let (device, route) = &self.casting.local_output;
        let device = if device.is_empty() { i18n::t("This computer") } else { device.clone() };
        let mut entries = vec![json!({"act": "local", "icon": "local", "text": device,
            "detail": format!("{} · {route}", i18n::t("This computer")), "checked": !self.casting.active()})];
        for (kind, title, icon) in [(Kind::Chromecast, "Chromecast", "cast"), (Kind::Dlna, "DLNA / UPnP", "dlna")] {
            entries.push(json!({"separator": true}));
            entries.push(json!({"text": title, "note": true}));
            let mut devices: Vec<_> = self.casting.devices.iter().filter(|d| d.kind == kind).collect();
            // Discovery may miss an already connected receiver for a scan.
            if let Some(selected) = self.casting.selected.as_ref().filter(|d| d.kind == kind) {
                if !devices.iter().any(|d| d.id == selected.id) { devices.push(selected); }
            }
            if devices.is_empty() {
                entries.push(json!({"text": if self.casting.discovery.is_some() { i18n::t("Searching for devices…") } else { i18n::t("No devices found") }, "note": true}));
            }
            for device in devices {
                let selected = self.casting.selected.as_ref().is_some_and(|d| d.id == device.id && d.kind == device.kind);
                entries.push(json!({"act": "connect", "id": device.id, "icon": icon, "text": device.name,
                    "detail": if selected { state } else { device.model.as_str() }, "checked": selected}));
            }
        }
        if !self.casting.error.is_empty() {
            entries.push(json!({"separator": true}));
            entries.push(json!({"text": self.casting.error, "note": true}));
        }
        entries
    }
}
