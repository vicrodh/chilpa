//! Remembered network endpoints. Identity and descriptors are durable,
//! locators are replaceable hints and presence lives only in this process.
//! No queue, session or audio output is owned here.
mod controller;
mod document;
mod lan;
mod store;

pub use controller::{Controller, Load, Network, Notices, Outcome, Selection, BROWSE_WINDOW, CHECK_WINDOW, COOLDOWN, EVENTS, PASS_LIMIT, PROBES, PROBE_LIMIT, SEARCH_WINDOW, SELECT_LIMIT};
pub use store::set_aside_path;
pub use document::{parse, render, Stored, Unusable, MAX_BYTES, SCHEMA_VERSION};
pub use lan::Lan;

use crate::{Device, Kind};
use std::time::{Duration, Instant};

/// Endpoints remembered across both protocols, and within one.
pub const MAX_ENDPOINTS: usize = 256;
pub const MAX_PER_PROTOCOL: usize = 128;
pub const MAX_ID: usize = 1024;
pub const MAX_TEXT: usize = 512;
pub const MAX_URL: usize = 4096;
pub const MAX_LOCATORS: usize = 4;
/// How long one answer keeps an endpoint from being asked again.
pub const FRESH: Duration = Duration::from_secs(30);

/// The protocol and the identifier the device gives itself under it: the
/// UDN of a UPnP renderer, the TXT `id` of a Cast receiver. Both fields are
/// compared everywhere; the same text under two protocols is two endpoints.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EndpointKey { pub protocol: Kind, pub stable_id: String }
impl EndpointKey {
    pub fn new(protocol: Kind, stable_id: impl Into<String>) -> Self { Self { protocol, stable_id: stable_id.into() } }
    pub fn of(device: &Device) -> Self { Self::new(device.kind, device.id.clone()) }
}
/// The name a protocol is stored and exchanged under.
pub fn protocol_name(kind: Kind) -> &'static str { match kind { Kind::Dlna => "upnp", Kind::Chromecast => "cast" } }
pub fn protocol_named(name: &str) -> Option<Kind> { match name { "upnp" => Some(Kind::Dlna), "cast" => Some(Kind::Chromecast), _ => None } }

/// Where an endpoint was last found. A hint to ask again, never proof of
/// who answers there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Locator { Description(String), Address(String), Mdns(String) }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Absence { NoAnswer, WrongIdentity, NotRenderer }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Presence { Unknown, Checking, Reachable, Unavailable(Absence) }

#[derive(Clone, Debug)]
pub struct Endpoint {
    pub key: EndpointKey, pub name: String, pub model: String,
    pub locators: Vec<Locator>,
    /// Unix seconds of the last answer. Stored by the day, shown as an age.
    pub last_seen: u64,
    /// The last answer in this process and network: never restored.
    answer: Presence,
    /// The operation counter when a check was asked for.
    asked: Option<u64>,
    checked: Option<Instant>,
    /// The device as its own answer described it, usable to connect.
    device: Option<Device>,
    /// Changes whenever the locators do: an answer for older locators is void.
    version: u64,
}
impl Endpoint {
    /// A check under way only shows while nothing is known: asking again
    /// about an endpoint that answered leaves its row as it was.
    pub fn presence(&self) -> Presence { if self.answer == Presence::Unknown && self.asked.is_some() { Presence::Checking } else { self.answer } }
    pub fn fresh(&self, now: Instant) -> bool { self.checked.is_some_and(|at| now.saturating_duration_since(at) < FRESH) }
    pub fn description(&self) -> Option<&str> { self.locators.iter().find_map(|l| match l { Locator::Description(url) => Some(url.as_str()), _ => None }) }
    pub(super) fn version(&self) -> u64 { self.version }
    pub(super) fn asked(&self) -> Option<u64> { self.asked }
    pub(super) fn checked(&self) -> Option<Instant> { self.checked }
    pub(super) fn answer(&self) -> Presence { self.answer }
    pub(super) fn device(&self) -> Option<&Device> { self.device.as_ref() }
    fn stored(&self) -> Stored { Stored { key: self.key.clone(), name: self.name.clone(), model: self.model.clone(), last_seen: self.last_seen, locators: self.locators.clone() } }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Seen { Updated, Added, Unknown, Full, Invalid }

/// The one inventory. Rows keep a deterministic order: name, then key.
#[derive(Default)]
pub struct Registry { endpoints: Vec<Endpoint>, revision: u64, dirty: bool, versions: u64 }
impl Registry {
    pub fn endpoints(&self) -> &[Endpoint] { &self.endpoints }
    /// Changes only with what a row shows: membership, name, model, presence.
    pub fn revision(&self) -> u64 { self.revision }
    pub fn get(&self, key: &EndpointKey) -> Option<&Endpoint> { self.endpoints.iter().find(|e| &e.key == key) }
    pub(super) fn take_dirty(&mut self) -> bool { std::mem::take(&mut self.dirty) }
    pub(super) fn mark_dirty(&mut self) { self.dirty = true; }
    pub(super) fn stored(&self) -> Vec<Stored> { self.endpoints.iter().map(Endpoint::stored).collect() }
    fn sort(&mut self) { self.endpoints.sort_by(|a, b| (a.name.to_lowercase(), &a.key).cmp(&(b.name.to_lowercase(), &b.key))); }
    fn version(&mut self) -> u64 { self.versions += 1; self.versions }
    /// Applies `change` to one endpoint and counts a revision only when its
    /// row would read differently.
    fn edit(&mut self, key: &EndpointKey, change: impl FnOnce(&mut Endpoint)) -> bool {
        let Some(endpoint) = self.endpoints.iter_mut().find(|e| &e.key == key) else { return false };
        let before = (endpoint.presence(), endpoint.name.clone(), endpoint.model.clone());
        change(endpoint);
        let renamed = before.1 != endpoint.name;
        if before != (endpoint.presence(), endpoint.name.clone(), endpoint.model.clone()) { self.revision += 1; }
        if renamed { self.sort(); }
        true
    }
    /// A device that named itself in its own answer. A known key takes the
    /// new name, model and locators; a new key enters only when `admit`.
    pub(super) fn observe(&mut self, device: Device, fullname: Option<&str>, admit: bool, now: Instant, wall: u64) -> Seen {
        let key = EndpointKey::of(&device);
        if key.stable_id.is_empty() || key.stable_id.len() > MAX_ID || device.description.len() > MAX_URL { return Seen::Invalid; }
        let (name, model) = (clipped(&device.name), clipped(&device.model));
        let mut locators = Vec::new();
        match device.kind {
            Kind::Dlna => locators.push(Locator::Description(device.description.clone())),
            Kind::Chromecast => {
                locators.push(Locator::Address(std::net::SocketAddr::new(device.address.ip(), device.address.port()).to_string()));
                if let Some(fullname) = fullname.filter(|f| !f.is_empty() && f.len() <= MAX_ID) { locators.push(Locator::Mdns(fullname.into())); }
            }
        }
        if self.get(&key).is_none() {
            if !admit { return Seen::Unknown; }
            if self.endpoints.len() >= MAX_ENDPOINTS || self.endpoints.iter().filter(|e| e.key.protocol == key.protocol).count() >= MAX_PER_PROTOCOL { return Seen::Full; }
            let version = self.version();
            self.endpoints.push(Endpoint { key, name, model, locators, last_seen: wall, answer: Presence::Reachable, asked: None, checked: Some(now), device: Some(device), version });
            self.sort(); self.revision += 1; self.dirty = true;
            return Seen::Added;
        }
        let version = self.version();
        let mut durable = false;
        self.edit(&key, |endpoint| {
            if endpoint.locators != locators { endpoint.locators = locators; endpoint.version = version; durable = true; }
            if endpoint.name != name || endpoint.model != model { endpoint.name = name; endpoint.model = model; durable = true; }
            // A probe a second is not a write a second: the date moves by the day.
            if endpoint.last_seen / 86_400 != wall / 86_400 { durable = true; }
            endpoint.last_seen = wall;
            endpoint.answer = Presence::Reachable; endpoint.asked = None; endpoint.checked = Some(now); endpoint.device = Some(device);
        });
        self.dirty |= durable;
        Seen::Updated
    }
    /// What a finished check concluded about an endpoint that did not answer.
    pub(super) fn absent(&mut self, key: &EndpointKey, why: Absence, now: Instant) {
        self.edit(key, |e| { e.answer = Presence::Unavailable(why); e.asked = None; e.checked = Some(now); e.device = None; });
    }
    /// The session already talks to this endpoint: that is its answer.
    pub(super) fn alive(&mut self, key: &EndpointKey, now: Instant) {
        self.edit(key, |e| { e.answer = Presence::Reachable; e.asked = None; e.checked = Some(now); });
    }
    pub(super) fn ask(&mut self, key: &EndpointKey, operation: u64) { self.edit(key, |e| if e.asked.is_none() { e.asked = Some(operation); }); }
    pub(super) fn unask(&mut self, key: &EndpointKey) { self.edit(key, |e| e.asked = None); }
    pub(super) fn forget(&mut self, key: &EndpointKey) -> bool {
        let before = self.endpoints.len();
        self.endpoints.retain(|e| &e.key != key);
        let removed = self.endpoints.len() != before;
        if removed { self.revision += 1; self.dirty = true; }
        removed
    }
    /// Another network, or none: nothing answered here yet.
    pub(super) fn invalidate(&mut self) {
        let keys: Vec<_> = self.endpoints.iter().map(|e| e.key.clone()).collect();
        for key in keys { self.edit(&key, |e| { e.answer = Presence::Unknown; e.asked = None; e.checked = None; e.device = None; }); }
    }
    /// The stored list arrives after the process has been running. What was
    /// found meanwhile is newer and stays; `skip` holds what was forgotten.
    pub(super) fn restore(&mut self, stored: Vec<Stored>, skip: impl Fn(&EndpointKey) -> bool) {
        let found_first = !self.endpoints.is_empty();
        let mut dropped = false;
        for record in stored {
            if skip(&record.key) { dropped = true; continue; }
            if self.get(&record.key).is_some() { continue; }
            // What was found before the list arrived may have taken its room.
            let full = self.endpoints.len() >= MAX_ENDPOINTS || self.endpoints.iter().filter(|e| e.key.protocol == record.key.protocol).count() >= MAX_PER_PROTOCOL;
            if full { dropped = true; continue; }
            let version = self.version();
            self.endpoints.push(Endpoint { key: record.key, name: record.name, model: record.model, locators: record.locators, last_seen: record.last_seen,
                answer: Presence::Unknown, asked: None, checked: None, device: None, version });
            self.revision += 1;
        }
        self.sort();
        // The file is rewritten only if this process changed what it holds.
        self.dirty = found_first || dropped;
    }
}

/// A display text cut on a character boundary. Identifiers are never cut.
fn clipped(text: &str) -> String {
    let mut end = text.len().min(MAX_TEXT);
    while !text.is_char_boundary(end) { end -= 1; }
    text[..end].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn renderer(id: &str, name: &str, url: &str) -> Device {
        Device { id: id.into(), name: name.into(), model: "Model".into(), kind: Kind::Dlna, address: "192.0.2.1:80".parse().unwrap(), description: url.into() }
    }
    fn stored(id: &str, name: &str) -> Stored {
        Stored { key: EndpointKey::new(Kind::Dlna, id), name: name.into(), model: "Stored".into(), last_seen: 5, locators: vec![Locator::Description("http://192.0.2.9/old.xml".into())] }
    }
    #[test]
    fn a_late_restore_keeps_newer_answers_and_leaves_out_the_forgotten() {
        let mut registry = Registry::default();
        let now = Instant::now();
        assert_eq!(registry.observe(renderer("uuid:a", "Found now", "http://192.0.2.1/a.xml"), None, true, now, 100), Seen::Added);
        registry.restore(vec![stored("uuid:a", "Stored name"), stored("uuid:b", "Remembered"), stored("uuid:gone", "Forgotten")], |key| key.stable_id == "uuid:gone");
        let names: Vec<_> = registry.endpoints().iter().map(|e| (e.name.as_str(), e.presence())).collect();
        assert_eq!(names, [("Found now", Presence::Reachable), ("Remembered", Presence::Unknown)], "the stored list must not replace what answered in this process");
        assert_eq!(registry.get(&EndpointKey::new(Kind::Dlna, "uuid:a")).unwrap().description(), Some("http://192.0.2.1/a.xml"));
        assert!(registry.take_dirty(), "what this process found and forgot has to reach the file");
    }
    #[test]
    fn a_restore_alone_changes_nothing_stored_and_never_restores_presence() {
        let mut registry = Registry::default();
        registry.restore(vec![stored("uuid:b", "Remembered")], |_| false);
        assert!(!registry.take_dirty(), "reading the file is not a reason to write it");
        let endpoint = &registry.endpoints()[0];
        assert_eq!(endpoint.presence(), Presence::Unknown);
        assert!(endpoint.device().is_none() && !endpoint.fresh(Instant::now()));
    }
    #[test]
    fn an_equal_answer_keeps_the_revision_and_writes_nothing() {
        let mut registry = Registry::default();
        let now = Instant::now();
        registry.observe(renderer("uuid:a", "Renderer", "http://192.0.2.1/a.xml"), None, true, now, 86_400 * 3 + 10);
        let revision = registry.revision();
        assert!(registry.take_dirty());
        assert_eq!(registry.observe(renderer("uuid:a", "Renderer", "http://192.0.2.1/a.xml"), None, false, now + Duration::from_secs(40), 86_400 * 3 + 50), Seen::Updated);
        assert_eq!(registry.revision(), revision, "the same answer must not rebuild rows");
        assert!(!registry.take_dirty(), "the same answer on the same day must not be written");
        registry.observe(renderer("uuid:a", "Renderer", "http://192.0.2.1/a.xml"), None, false, now, 86_400 * 4 + 1);
        assert!(registry.take_dirty(), "the date of the last answer is stored by the day");
    }
    #[test]
    fn identity_is_protocol_and_identifier_and_limits_reject_without_cutting() {
        let mut registry = Registry::default();
        let now = Instant::now();
        registry.observe(renderer("shared", "Renderer", "http://192.0.2.1/a.xml"), None, true, now, 1);
        let receiver = Device { id: "shared".into(), name: "Receiver".into(), model: "Cast".into(), kind: Kind::Chromecast, address: "192.0.2.1:8009".parse().unwrap(), description: String::new() };
        assert_eq!(registry.observe(receiver, Some("Receiver._googlecast._tcp.local."), true, now, 1), Seen::Added);
        assert_eq!(registry.endpoints().len(), 2, "the same text under two protocols is two endpoints");
        assert_eq!(registry.observe(renderer(&"x".repeat(MAX_ID + 1), "Long", "http://192.0.2.1/l.xml"), None, true, now, 1), Seen::Invalid);
        assert_eq!(registry.observe(renderer("uuid:unknown", "Unknown", "http://192.0.2.1/u.xml"), None, false, now, 1), Seen::Unknown);
        for n in 0..MAX_PER_PROTOCOL { registry.observe(renderer(&format!("uuid:{n}"), "R", &format!("http://192.0.2.1/{n}.xml")), None, true, now, 1); }
        assert_eq!(registry.endpoints().iter().filter(|e| e.key.protocol == Kind::Dlna).count(), MAX_PER_PROTOCOL);
        assert_eq!(registry.observe(renderer("uuid:late", "Late", "http://192.0.2.1/late.xml"), None, true, now, 1), Seen::Full);
        assert_eq!(registry.observe(renderer("shared", "Renamed", "http://192.0.2.7/a.xml"), None, true, now, 1), Seen::Updated, "a known key is updated at the limit");
    }
}
