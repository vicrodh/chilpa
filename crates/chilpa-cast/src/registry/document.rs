//! The stored form of the registry: `network-endpoints.json`, schema 1.
//! Presence, sessions and tokens are never written.
use super::{protocol_name, protocol_named, EndpointKey, Locator, MAX_ENDPOINTS, MAX_ID, MAX_LOCATORS, MAX_PER_PROTOCOL, MAX_TEXT, MAX_URL};
use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_BYTES: usize = 1024 * 1024;

/// One endpoint as the file holds it, already validated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stored { pub key: EndpointKey, pub name: String, pub model: String, pub last_seen: u64, pub locators: Vec<Locator> }

/// Why a file is left as it is instead of being read or replaced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unusable { Damaged, Newer, Unreadable }

#[derive(Deserialize)]
struct Header { schema_version: u32 }
#[derive(Serialize, Deserialize)]
struct Document { schema_version: u32, endpoints: Vec<Record> }
#[derive(Serialize, Deserialize)]
struct Record {
    protocol: String, stable_id: String, name: String, model: String,
    #[serde(default)] last_seen: u64,
    #[serde(default)] locators: Vec<Place>,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Place { Description { url: String }, Address { address: String }, Mdns { fullname: String } }

/// Reads a whole document or none of it: a field over its limit, a repeated
/// key or an unknown protocol makes the file damaged, never shorter.
pub fn parse(bytes: &[u8]) -> Result<Vec<Stored>, Unusable> {
    if bytes.len() > MAX_BYTES { return Err(Unusable::Damaged); }
    let header: Header = serde_json::from_slice(bytes).map_err(|_| Unusable::Damaged)?;
    if header.schema_version > SCHEMA_VERSION { return Err(Unusable::Newer); }
    if header.schema_version != SCHEMA_VERSION { return Err(Unusable::Damaged); }
    let document: Document = serde_json::from_slice(bytes).map_err(|_| Unusable::Damaged)?;
    if document.endpoints.len() > MAX_ENDPOINTS { return Err(Unusable::Damaged); }
    let mut seen = std::collections::HashSet::new();
    let mut stored = Vec::with_capacity(document.endpoints.len());
    for record in document.endpoints {
        let protocol = protocol_named(&record.protocol).ok_or(Unusable::Damaged)?;
        let sized = !record.stable_id.is_empty() && record.stable_id.len() <= MAX_ID && record.name.len() <= MAX_TEXT
            && record.model.len() <= MAX_TEXT && record.locators.len() <= MAX_LOCATORS;
        if !sized { return Err(Unusable::Damaged); }
        let key = EndpointKey::new(protocol, record.stable_id);
        if !seen.insert(key.clone()) { return Err(Unusable::Damaged); }
        let mut locators = Vec::with_capacity(record.locators.len());
        for place in record.locators {
            let (locator, limit) = match place {
                Place::Description { url } => (Locator::Description(url), MAX_URL),
                Place::Address { address } => (Locator::Address(address), MAX_TEXT),
                Place::Mdns { fullname } => (Locator::Mdns(fullname), MAX_ID),
            };
            let text = match &locator { Locator::Description(text) | Locator::Address(text) | Locator::Mdns(text) => text };
            if text.is_empty() || text.len() > limit { return Err(Unusable::Damaged); }
            locators.push(locator);
        }
        stored.push(Stored { key, name: record.name, model: record.model, last_seen: record.last_seen, locators });
    }
    for protocol in [crate::Kind::Dlna, crate::Kind::Chromecast] {
        if stored.iter().filter(|s| s.key.protocol == protocol).count() > MAX_PER_PROTOCOL { return Err(Unusable::Damaged); }
    }
    Ok(stored)
}

/// The bytes to store, or nothing when they would pass the size limit: the
/// size wins over the count, and the last valid file stays in place.
pub fn render(stored: &[Stored]) -> Option<Vec<u8>> {
    let endpoints = stored.iter().map(|s| Record {
        protocol: protocol_name(s.key.protocol).into(), stable_id: s.key.stable_id.clone(), name: s.name.clone(), model: s.model.clone(), last_seen: s.last_seen,
        locators: s.locators.iter().map(|l| match l {
            Locator::Description(url) => Place::Description { url: url.clone() },
            Locator::Address(address) => Place::Address { address: address.clone() },
            Locator::Mdns(fullname) => Place::Mdns { fullname: fullname.clone() },
        }).collect(),
    }).collect();
    let bytes = serde_json::to_vec_pretty(&Document { schema_version: SCHEMA_VERSION, endpoints }).ok()?;
    (bytes.len() <= MAX_BYTES).then_some(bytes)
}
