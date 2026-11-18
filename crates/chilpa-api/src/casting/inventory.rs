//! The remembered renderers beside the session. Opening the picker, checking
//! and searching never connect; only a choice made by hand does.
use crate::Chilpa;
use chilpa_cast::{registry::{protocol_named, Controller, EndpointKey, Lan, Network, Outcome, Selection}, Device, Kind};
use std::{sync::Arc, time::{Instant, SystemTime}};

pub(crate) struct Inventory(Controller);
impl Inventory {
    fn over(network: Arc<dyn Network>) -> Self { Self(Controller::new(Some(chilpa_dirs::data().join("network-endpoints.json")), network)) }
}
impl Default for Inventory { fn default() -> Self { Self::over(Arc::new(Lan)) } }
impl std::ops::Deref for Inventory { type Target = Controller; fn deref(&self) -> &Controller { &self.0 } }
impl std::ops::DerefMut for Inventory { fn deref_mut(&mut self) -> &mut Controller { &mut self.0 } }

/// A renderer as the output details name it.
#[derive(Clone)]
pub(crate) struct Remote { pub kind: Kind, pub id: String, pub name: String, pub model: String }

impl super::Casting {
    /// Every remembered renderer, and the connected one when it is not among them.
    pub(crate) fn inspected_remotes(&self) -> Vec<Remote> {
        let mut remotes: Vec<_> = self.inventory.registry().endpoints().iter()
            .map(|e| Remote { kind: e.key.protocol, id: e.key.stable_id.clone(), name: e.name.clone(), model: e.model.clone() }).collect();
        if let Some(d) = self.selected.as_ref().filter(|d| !remotes.iter().any(|r| r.kind == d.kind && r.id == d.id)) {
            remotes.push(Remote { kind: d.kind, id: d.id.clone(), name: d.name.clone(), model: d.model.clone() });
        }
        remotes
    }
}

impl Chilpa {
    /// Replaces the network the registry asks, for tests that hold answers back.
    #[doc(hidden)]
    pub fn cast_inventory_over(&mut self, network: Arc<dyn Network>) { self.casting.inventory = Inventory::over(network); }
    /// What the registry has in flight, for tests and measurements.
    #[doc(hidden)]
    pub fn cast_inventory_load(&self) -> chilpa_cast::registry::Load { self.casting.inventory.load() }

    /// One action of the output menu. The menu only says which entry was chosen.
    pub fn cast_act(&mut self, act: &str, protocol: &str, id: &str) {
        match act {
            "local" => self.cast_disconnect(),
            "connect" => self.cast_connect(protocol, id),
            "discover" => self.cast_discover(),
            "forget-pick" => self.casting.forgetting = true,
            "forget-done" => self.casting.forgetting = false,
            "forget" => { self.cast_forget(protocol, id); self.casting.forgetting = false; }
            _ => {}
        }
    }
    pub fn cast_picker(&mut self, open: bool) {
        self.casting.picker = open;
        self.casting.forgetting = false;
        if open { self.name_local_output(); }
        let offline = self.offline();
        if open && offline { self.casting.error = chilpa_core::network::blocked(); }
        self.casting.inventory.set_online(!offline);
        self.casting.inventory.set_open(open, Instant::now());
    }
    /// The search asked for by hand: it finds new renderers and where known ones went.
    pub fn cast_discover(&mut self) {
        if self.offline() { self.casting.error = chilpa_core::network::blocked(); return; }
        self.casting.inventory.set_online(true);
        self.casting.inventory.discover(Instant::now());
    }
    /// Connects the renderer with this protocol and identifier. One that has
    /// not just answered is asked first; the local player stays until it does.
    pub fn cast_connect(&mut self, protocol: &str, id: &str) {
        let Some(kind) = protocol_named(protocol) else { return };
        if self.offline() { self.casting.error = chilpa_core::network::blocked(); return; }
        let key = EndpointKey::new(kind, id);
        if self.casting.selected.as_ref().is_some_and(|d| d.kind == kind && d.id == id) && self.casting.error.is_empty() {
            // Choosing the connected renderer again withdraws a choice still waiting.
            self.casting.inventory.deselect();
            return;
        }
        match self.casting.inventory.select(&key, Instant::now()) {
            Selection::Ready(device) => self.cast_connect_device(device),
            Selection::Pending => self.casting.error.clear(),
            Selection::Unknown => {}
        }
    }
    pub fn cast_forget(&mut self, protocol: &str, id: &str) {
        let Some(kind) = protocol_named(protocol) else { return };
        if self.casting.selected.as_ref().is_some_and(|d| d.kind == kind && d.id == id) {
            self.casting.error = chilpa_i18n::t("Disconnect before forgetting this device.");
            return;
        }
        self.casting.inventory.forget(&EndpointKey::new(kind, id));
    }
    pub(crate) fn cast_online(&mut self) { self.casting.inventory.set_online(true); }
    pub(super) fn cast_session_on(&mut self, device: Option<&Device>) {
        self.casting.inventory.deselect();
        self.casting.inventory.set_active(device.map(EndpointKey::of));
    }
    pub(super) fn poll_inventory(&mut self) {
        for outcome in self.casting.inventory.pump(Instant::now(), SystemTime::now()) {
            match outcome {
                Outcome::Connect(device) => self.cast_connect_device(device),
                Outcome::Unreachable(_) => if !self.offline() { self.casting.error = chilpa_cast::Error::Network.to_string(); },
            }
        }
    }
}
