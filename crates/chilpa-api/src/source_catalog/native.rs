//! A process plugin's answers, taken into the catalog.
//!
//! A process plugin answers in the shape of a library: its home's shelves
//! in one call, a page of 500 favorites, a record with its tracks. Each
//! answer becomes the same cards, tracks and shelves a compatibility
//! source's listings become, so the same pages draw them. The plugin is
//! asked on the catalog's native threads, never under the sandbox lock.

use super::store::{Card, CardKind, Format, Shelf, Store};
use super::{now, split, Catalog};
use crate::natives::Natives;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;

/// How long a plugin has to answer one question.
const ASKED: Duration = Duration::from_secs(30);
/// A library tab is read in pages of this many, up to this many pages.
const PAGE: u64 = 500;
const PAGES: u64 = 20;

fn text(item: &Value, key: &str) -> String {
    item.get(key).and_then(Value::as_str).unwrap_or_default().trim().to_string()
}

fn number(item: &Value, key: &str) -> Option<u64> {
    item.get(key).and_then(Value::as_u64)
}

fn web(item: &Value) -> String {
    Some(text(item, "image")).filter(|a| a.starts_with("https://") || a.starts_with("http://")).unwrap_or_default()
}

fn format(item: &Value) -> Option<Format> {
    let quality = item.get("quality")?;
    let bits = quality.get("bits").and_then(Value::as_u64).and_then(|b| u8::try_from(b).ok()).filter(|b| *b > 0);
    let rate = quality.get("rate").and_then(Value::as_u64).and_then(|r| u32::try_from(r).ok()).filter(|r| *r > 0);
    let codec = quality.get("codec").and_then(Value::as_str).unwrap_or_default().to_ascii_lowercase();
    (!codec.is_empty() || bits.is_some() || rate.is_some()).then_some(Format { codec, bits, rate })
}

impl Store {
    /// A record, a playlist or an artist the plugin listed. Its `ref` is
    /// its identity, so it keeps its id between listings and runs.
    pub fn take_item(&mut self, plugin: &str, item: &Value, now: u64) -> Option<i64> {
        let kind = match item.get("kind").and_then(Value::as_str)? {
            "album" => CardKind::Album,
            "playlist" => CardKind::Playlist,
            "artist" => CardKind::Artist,
            _ => return None,
        };
        let reference = text(item, "ref");
        if reference.is_empty() {
            return None;
        }
        let id = self.card_id(plugin, &reference);
        let card = self.cards.get_mut(&id).expect("interned");
        card.kind = kind;
        card.title = text(item, "title");
        card.artist = text(item, "artist");
        card.year = number(item, "year").and_then(|y| i32::try_from(y).ok()).or(card.year);
        let genre = text(item, "genre");
        if !genre.is_empty() {
            card.genre = genre;
        }
        let image = web(item);
        if !image.is_empty() && image != card.image {
            card.image = image;
            card.art.clear();
        }
        card.format = format(item).or(card.format.take());
        card.seen_at = now;
        Some(id)
    }

    /// A track the plugin listed; under a record, that record's. A track
    /// listed on its own still names its record, so "Go to album" finds it.
    pub fn take_item_track(&mut self, plugin: &str, item: &Value, under: Option<&Card>, now: u64) -> Option<i64> {
        let reference = text(item, "ref");
        if reference.is_empty() {
            return None;
        }
        let album_id = match under.filter(|c| c.kind == CardKind::Album) {
            Some(card) => Some(card.id),
            None => {
                let album = text(item, "album_ref");
                (!album.is_empty()).then(|| {
                    let record = json!({"kind": "album", "ref": album, "title": item.get("album"), "artist": item.get("artist"),
                        "image": item.get("image"), "year": item.get("year")});
                    // A record already read keeps what it said of itself.
                    if self.has_read(plugin, &album) { self.card_id(plugin, &album) } else { self.take_item(plugin, &record, now).unwrap_or_default() }
                })
            }
        };
        let path = crate::source_opener::entry_of(plugin, &reference);
        let id = self.track_id(&path);
        let track = self.tracks.get_mut(&id).expect("interned");
        track.plugin = plugin.into();
        track.title = text(item, "title");
        track.artist = Some(text(item, "artist")).filter(|a| !a.is_empty())
            .or_else(|| under.filter(|c| c.kind == CardKind::Album).map(|c| c.artist.clone())).unwrap_or_default();
        track.album = Some(text(item, "album")).filter(|a| !a.is_empty())
            .or_else(|| under.filter(|c| c.kind == CardKind::Album).map(|c| c.title.clone())).unwrap_or_default();
        track.album_id = album_id.unwrap_or(track.album_id);
        track.disc = number(item, "disc_number").and_then(|n| u16::try_from(n).ok()).unwrap_or(0);
        track.number = number(item, "track_number").and_then(|n| u16::try_from(n).ok()).unwrap_or(0);
        track.duration_ms = number(item, "duration_ms").unwrap_or(0);
        track.format = format(item).or(track.format.take());
        let image = web(item);
        track.image = if image.is_empty() { under.map(|c| c.image.clone()).unwrap_or_default() } else { image };
        track.playable = true;
        Some(id)
    }
}

/// Reads a listing of a process plugin into its shelf.
pub(super) fn read_shelf(catalog: &Catalog, natives: &Arc<Natives>, key: &str) -> Result<(), String> {
    let (plugin, address) = split(key);
    remember_domains(catalog, natives, plugin);
    if address.is_empty() || address == "home" || address == "root" || address.starts_with("route:discover:") {
        return read_home(catalog, natives, plugin);
    }
    if let Some(kind) = address.strip_prefix("section:") {
        return read_library(catalog, natives, plugin, key, kind);
    }
    if let Some(card) = address.strip_prefix("card:") {
        return read_artist(catalog, natives, plugin, key, card.parse().unwrap_or_default());
    }
    Err(chilpa_i18n::t("this part of the source is not there now"))
}

/// Every shelf of the plugin's home, in one call: each becomes a route of
/// its Discover page.
fn read_home(catalog: &Catalog, natives: &Arc<Natives>, plugin: &str) -> Result<(), String> {
    // Every route of Discover asks for the home: the first reads it, and
    // the rest, waiting here, find it fresh.
    let reading = {
        static READING: std::sync::Mutex<Option<std::collections::HashMap<String, Arc<std::sync::Mutex<()>>>>> = std::sync::Mutex::new(None);
        let mut reading = READING.lock().unwrap_or_else(|e| e.into_inner());
        reading.get_or_insert_with(Default::default).entry(plugin.to_string()).or_default().clone()
    };
    let _one = reading.lock().unwrap_or_else(|e| e.into_inner());
    let fresh = catalog.lock().store.shelves.get(&format!("{plugin} home")).is_some_and(|s| s.epoch > 0 && now().saturating_sub(s.read_at) < 30);
    if fresh {
        return Ok(());
    }
    let home = natives.call(plugin, "home", json!({}), ASKED)?;
    let shelves = home.get("shelves").and_then(Value::as_array).cloned().unwrap_or_default();
    let stamp = now();
    let mut titles = Vec::new();
    let mut inner = catalog.lock();
    for (at, found) in shelves.iter().enumerate() {
        let title = text(found, "title");
        let mut shelf = Shelf { title: title.clone(), read_at: stamp, epoch: 1, ..Shelf::default() };
        for item in found.get("items").and_then(Value::as_array).into_iter().flatten() {
            if item.get("kind").and_then(Value::as_str) == Some("track") {
                shelf.tracks.extend(inner.store.take_item_track(plugin, item, None, stamp));
            } else {
                shelf.cards.extend(inner.store.take_item(plugin, item, stamp));
            }
        }
        let key = format!("{plugin} route:discover:{at}");
        inner.shelf_errors.remove(&key);
        inner.store.shelves.insert(key, shelf);
        titles.push(title);
    }
    for key in [format!("{plugin} home"), format!("{plugin} root"), plugin.to_string()] {
        inner.store.shelves.insert(key, Shelf { read_at: stamp, epoch: 1, ..Shelf::default() });
    }
    inner.dirty = true;
    drop(inner);
    natives.set_discover(plugin, titles);
    Ok(())
}

/// One tab of the library, every page of it: albums, tracks, artists or
/// playlists the listener keeps in the service.
fn read_library(catalog: &Catalog, natives: &Arc<Natives>, plugin: &str, key: &str, kind: &str) -> Result<(), String> {
    let kind = if kind == "library" { "albums" } else { kind };
    let mut items = Vec::new();
    for page in 0..PAGES {
        let answer = natives.call(plugin, "library.list", json!({"kind": kind, "offset": page * PAGE, "limit": PAGE}), ASKED)?;
        let found = answer.get("items").and_then(Value::as_array).cloned().unwrap_or_default();
        let total = number(&answer, "total").unwrap_or(0);
        let short = (found.len() as u64) < PAGE;
        items.extend(found);
        if short || items.len() as u64 >= total {
            break;
        }
    }
    let stamp = now();
    let mut inner = catalog.lock();
    let mut shelf = Shelf { read_at: stamp, epoch: 1, ..Shelf::default() };
    for item in &items {
        if kind == "tracks" {
            shelf.tracks.extend(inner.store.take_item_track(plugin, item, None, stamp));
        } else if let Some(id) = inner.store.take_item(plugin, item, stamp) {
            if let Some(card) = inner.store.cards.get_mut(&id) {
                card.seen_in = key.to_string();
            }
            shelf.cards.push(id);
            shelf.membership.insert(id, "favorites".into());
        }
    }
    let mut seen = std::collections::HashSet::new();
    shelf.cards.retain(|id| seen.insert(*id));
    inner.shelf_errors.remove(key);
    inner.store.shelves.insert(key.to_string(), shelf);
    inner.dirty = true;
    Ok(())
}

/// An artist's page: its records.
fn read_artist(catalog: &Catalog, natives: &Arc<Natives>, plugin: &str, key: &str, id: i64) -> Result<(), String> {
    let reference = catalog.lock().store.cards.get(&id).map(|c| c.identity.clone())
        .ok_or_else(|| chilpa_i18n::t("this part of the source is not there now"))?;
    let answer = natives.call(plugin, "artist.get", json!({"ref": reference}), ASKED)?;
    let stamp = now();
    let mut inner = catalog.lock();
    if let Some(artist) = answer.get("artist") {
        inner.store.take_item(plugin, artist, stamp);
    }
    let title = inner.store.cards.get(&id).map(|c| c.title.clone()).unwrap_or_default();
    let mut shelf = Shelf { title, read_at: stamp, epoch: 1, ..Shelf::default() };
    for found in answer.get("shelves").and_then(Value::as_array).into_iter().flatten() {
        for item in found.get("items").and_then(Value::as_array).into_iter().flatten() {
            shelf.cards.extend(inner.store.take_item(plugin, item, stamp));
        }
    }
    let mut seen = std::collections::HashSet::new();
    shelf.cards.retain(|id| seen.insert(*id));
    inner.shelf_errors.remove(key);
    inner.store.shelves.insert(key.to_string(), shelf);
    inner.dirty = true;
    Ok(())
}

/// A record's or a playlist's tracks, in one call.
pub(super) fn read_tracks(catalog: &Catalog, natives: &Arc<Natives>, id: i64) -> Result<(), String> {
    let (plugin, reference, kind) = catalog.lock().store.cards.get(&id).map(|c| (c.plugin.clone(), c.identity.clone(), c.kind))
        .ok_or_else(|| chilpa_i18n::t("this part of the source is not there now"))?;
    remember_domains(catalog, natives, &plugin);
    let (method, holder) = match kind {
        CardKind::Album => ("album.get", "album"),
        CardKind::Playlist => ("playlist.get", "playlist"),
        CardKind::Artist => return Err(chilpa_i18n::t("this part of the source is not there now")),
    };
    let answer = natives.call(&plugin, method, json!({"ref": reference, "offset": 0, "limit": 10_000}), ASKED)?;
    let stamp = now();
    let mut inner = catalog.lock();
    if let Some(itself) = answer.get(holder) {
        inner.store.take_item(&plugin, itself, stamp);
    }
    let under = inner.store.cards.get(&id).cloned().unwrap_or_default();
    let tracks: Vec<i64> = answer.get("tracks").and_then(Value::as_array).into_iter().flatten()
        .filter_map(|item| inner.store.take_item_track(&plugin, item, Some(&under), stamp)).collect();
    let best = tracks.iter().filter_map(|t| inner.store.tracks.get(t)?.format.clone())
        .max_by_key(|f: &Format| (f.quality().tier.rank(), f.bits, f.rate));
    let images: Vec<String> = if kind == CardKind::Playlist {
        tracks.iter().filter_map(|t| inner.store.tracks.get(t)).map(|t| t.image.clone()).collect()
    } else {
        Vec::new()
    };
    if let Some(card) = inner.store.cards.get_mut(&id) {
        card.tracks = Some(tracks);
        card.read_at = now();
        if card.format.is_none() { card.format = best; }
    }
    inner.card_errors.remove(&id);
    inner.dirty = true;
    for image in images {
        catalog.want_cover(&mut inner, &image);
    }
    drop(inner);
    // Play is the likely next click: the first track's address is asked
    // for now, and the plugin keeps it until it expires.
    if let Some(first) = answer.get("tracks").and_then(Value::as_array).and_then(|t| t.first()).map(|t| text(t, "ref")).filter(|r| !r.is_empty()) {
        let (natives, plugin) = (natives.clone(), plugin.clone());
        let _ = std::thread::Builder::new().name("chilpa-preload".into()).spawn(move || {
            let _ = natives.call(&plugin, "track.resolve", json!({"ref": first, "purpose": "preload",
                "output": {"max_rate": 192_000, "max_bits": 24}}), ASKED);
        });
    }
    Ok(())
}

/// A process plugin's covers are fetched within the domains its manifest
/// names, like any plugin's.
fn remember_domains(catalog: &Catalog, natives: &Natives, plugin: &str) {
    let domains = natives.domains(plugin);
    catalog.lock().domains.insert(plugin.to_string(), domains);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_record_and_its_tracks_from_items() {
        let mut store = Store::default();
        let record = json!({"kind": "album", "ref": "album:x1", "title": "Record", "artist": "Band", "year": 1999,
            "image": "https://i/l.jpg", "quality": {"codec": "flac", "bits": 24, "rate": 96000}});
        let id = store.take_item("p", &record, 10).unwrap();
        assert_eq!(store.take_item("p", &record, 11), Some(id), "the same ref is the same card");
        let card = store.cards[&id].clone();
        assert_eq!((card.title.as_str(), card.artist.as_str(), card.year), ("Record", "Band", Some(1999)));
        assert_eq!(card.format, Some(Format { codec: "flac".into(), bits: Some(24), rate: Some(96000) }));
        let track = store.take_item_track("p", &json!({"ref": "track:1", "title": "Song", "duration_ms": 1000, "track_number": 2}), Some(&card), 10).unwrap();
        let track = &store.tracks[&track];
        assert_eq!((track.artist.as_str(), track.album.as_str(), track.album_id, track.number), ("Band", "Record", id, 2));
        assert!(track.playable);
        assert_eq!(track.image, "https://i/l.jpg", "its record's cover");
    }

    #[test]
    fn a_track_on_its_own_still_leads_to_its_record() {
        let mut store = Store::default();
        let track = store.take_item_track("p", &json!({"ref": "track:1", "title": "Song", "artist": "Band",
            "album": "Record", "album_ref": "album:x1", "image": "https://i/s.jpg"}), None, 10).unwrap();
        let album = store.tracks[&track].album_id;
        assert!(album < 0, "a card was made for its record");
        assert_eq!(store.cards[&album].title, "Record");
        assert_eq!(store.cards[&album].identity, "album:x1");
    }
}
