//! The output menu is built beside the session, as with the track action menu.
use crate::Chilpa;
use chilpa_cast::{registry::{protocol_name, EndpointKey, Presence, Unusable}, Kind};
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
        json!({"entries": self.cast_entries(&state), "footer":[{"act":"inspect","text":i18n::t("Output details…")}], "searching": self.casting.inventory.searching(),
            "active": self.casting.active(), "name": self.cast_destination().unwrap_or_default(),
            "protocol": protocol, "icon": icon, "state": state, "error": self.casting.error}).to_string()
    }

    fn cast_entries(&self, state: &str) -> Vec<Value> {
        let inventory = &self.casting.inventory;
        let chosen = |kind: Kind, id: &str| self.casting.selected.as_ref().is_some_and(|d| d.kind == kind && d.id == id);
        let named = |name: &str, model: &str, id: &str| [name, model, id].into_iter().find(|text| !text.is_empty()).unwrap_or_default().to_string();
        let groups = [(Kind::Chromecast, "Chromecast", "cast"), (Kind::Dlna, "DLNA / UPnP", "dlna")];
        if self.casting.forgetting {
            // The same menu, listing what can be forgotten. The renderer in
            // use stays, and its row says why.
            let mut entries = vec![json!({"text": i18n::t("Forget a device"), "note": true})];
            for (kind, _, icon) in groups {
                for endpoint in inventory.registry().endpoints().iter().filter(|e| e.key.protocol == kind) {
                    let active = chosen(kind, &endpoint.key.stable_id);
                    entries.push(json!({"act": "forget", "protocol": protocol_name(kind), "id": endpoint.key.stable_id, "icon": icon,
                        "text": named(&endpoint.name, &endpoint.model, &endpoint.key.stable_id),
                        "detail": if active { i18n::t("Disconnect before forgetting this device.") } else { endpoint.model.clone() }, "enabled": !active, "keepOpen": true}));
                }
            }
            entries.push(json!({"separator": true}));
            entries.push(json!({"act": "forget-done", "text": i18n::t("Back"), "keepOpen": true}));
            return entries;
        }
        let (device, route) = &self.casting.local_output;
        let device = if device.is_empty() { i18n::t("This computer") } else { device.clone() };
        let mut entries = vec![json!({"act": "local", "icon": "local", "text": device,
            "detail": format!("{} · {route}", i18n::t("This computer")), "checked": !self.casting.active()})];
        for (kind, title, icon) in groups {
            entries.push(json!({"separator": true}));
            entries.push(json!({"text": title, "note": true}));
            let mut rows = 0;
            for endpoint in inventory.registry().endpoints().iter().filter(|e| e.key.protocol == kind) {
                let selected = chosen(kind, &endpoint.key.stable_id);
                let detail = if selected { state.to_string() } else if inventory.pending() == Some(&endpoint.key) { i18n::t("Checking…") } else {
                    // A renderer that answered needs no label; one that did
                    // not is told apart from one nobody asked yet.
                    let presence = match endpoint.presence() {
                        Presence::Unknown => i18n::t("Not checked yet"), Presence::Checking => i18n::t("Checking…"),
                        Presence::Reachable => String::new(), Presence::Unavailable(_) => i18n::t("Unavailable"),
                    };
                    [endpoint.model.as_str(), presence.as_str()].into_iter().filter(|text| !text.is_empty()).collect::<Vec<_>>().join(" · ")
                };
                entries.push(json!({"act": "connect", "protocol": protocol_name(kind), "id": endpoint.key.stable_id, "icon": icon,
                    "text": named(&endpoint.name, &endpoint.model, &endpoint.key.stable_id), "detail": detail, "checked": selected}));
                rows += 1;
            }
            // A session on a renderer the registry does not hold still shows.
            if let Some(d) = self.casting.selected.as_ref().filter(|d| d.kind == kind && inventory.registry().get(&EndpointKey::of(d)).is_none()) {
                entries.push(json!({"act": "connect", "protocol": protocol_name(kind), "id": d.id, "icon": icon, "text": d.name, "detail": state, "checked": true}));
                rows += 1;
            }
            if rows == 0 {
                let text = if inventory.loading() { i18n::t("Loading…") } else if inventory.searching() { i18n::t("Searching for devices…") } else { i18n::t("No devices found") };
                entries.push(json!({"text": text, "note": true}));
            }
        }
        entries.push(json!({"separator": true}));
        let searching = inventory.searching();
        entries.push(json!({"act": "discover", "text": if searching { i18n::t("Searching for devices…") } else { i18n::t("Search for devices") }, "enabled": !searching && !self.offline(), "keepOpen": true}));
        if !inventory.registry().endpoints().is_empty() { entries.push(json!({"act": "forget-pick", "text": i18n::t("Forget a device…"), "keepOpen": true})); }
        let notices = inventory.notices();
        let told = [
            (notices.kept == Some(Unusable::Damaged), i18n::mark("The saved device list is damaged and was kept. Search for devices to start a new list.")),
            (notices.kept == Some(Unusable::Newer), i18n::mark("The saved device list comes from a newer Chilpa. Changes will not be saved.")),
            (notices.kept == Some(Unusable::Unreadable), i18n::mark("The saved device list could not be read. Changes will not be saved.")),
            (notices.unsaved, i18n::mark("The device list could not be saved.")),
            (notices.full, i18n::mark("The device list is full. Forget a device to add another.")),
            (notices.search_failed, i18n::mark("The search for devices failed.")),
        ];
        let mut notes: Vec<_> = told.into_iter().filter(|(shown, _)| *shown).map(|(_, text)| i18n::t(text)).collect();
        if !self.casting.error.is_empty() { notes.push(self.casting.error.clone()); }
        if !notes.is_empty() { entries.push(json!({"separator": true})); }
        entries.extend(notes.into_iter().map(|text| json!({"text": text, "note": true})));
        entries
    }
}
