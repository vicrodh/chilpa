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

fn merge_metadata(target: &mut Value, item: &Value) {
    if !target.is_object() { *target = json!({}); }
    if let (Some(target), Some(item)) = (target.as_object_mut(), item.as_object()) {
        for (key, value) in item { if !value.is_null() { target.insert(key.clone(), value.clone()); } }
    }
}

fn number(item: &Value, key: &str) -> Option<u64> {
    item.get(key).and_then(Value::as_u64)
}

fn web(item: &Value) -> String {
    Some(text(item, "image")).filter(|a| a.starts_with("https://") || a.starts_with("http://")).unwrap_or_default()
}

pub(super) fn format(item: &Value) -> Option<Format> {
    let quality = item.get("quality")?;
    let bits = quality.get("bits").and_then(Value::as_u64).and_then(|b| u8::try_from(b).ok()).filter(|b| *b > 0);
    let rate = quality.get("rate").and_then(Value::as_u64).and_then(|r| u32::try_from(r).ok()).filter(|r| *r > 0);
    let codec = quality.get("codec").and_then(Value::as_str).unwrap_or_default().to_ascii_lowercase();
    let advertised_hires = quality.get("tier").and_then(Value::as_str) == Some("hi_res");
    (!codec.is_empty() || bits.is_some() || rate.is_some()).then_some(Format { codec, bits, rate, advertised_hires })
}

impl Store {
    /// A record, a playlist or an artist the plugin listed. Its `ref` is
    /// its identity, so it keeps its id between listings and runs.
    pub fn take_item(&mut self, plugin: &str, item: &Value, now: u64) -> Option<i64> {
        let kind = match item.get("kind").and_then(Value::as_str)? {
            "album" => CardKind::Album,
            "playlist" => CardKind::Playlist,
            "artist" => CardKind::Artist,
            "label" => CardKind::Label,
            _ => return None,
        };
        let reference = text(item, "ref");
        if reference.is_empty() {
            return None;
        }
        let id = self.card_id(plugin, &reference);
        let card = self.cards.get_mut(&id).expect("interned");
        if kind == CardKind::Playlist && ["updated_at", "tracks", "editable"].iter().any(|key| item.get(key).is_some_and(|value| card.metadata.get(key) != Some(value))) {
            card.read_at = 0;
        }
        card.kind = kind;
        merge_metadata(&mut card.metadata, item);
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
                        "image": item.get("image"), "year": item.get("year"), "artist_ref": item.get("artist_ref")});
                    // A record already read keeps what it said of itself.
                    if self.has_read(plugin, &album) { self.card_id(plugin, &album) } else { self.take_item(plugin, &record, now).unwrap_or_default() }
                })
            }
        };
        let path = crate::source_opener::entry_of(plugin, &reference);
        let id = self.track_id(&path);
        let track = self.tracks.get_mut(&id).expect("interned");
        track.plugin = plugin.into();
        merge_metadata(&mut track.metadata, item);
        // A record's song is its record's artist's unless it names its own.
        if track.metadata["artist_ref"].as_str().is_none_or(str::is_empty) {
            let named = under.filter(|c| c.kind == CardKind::Album).and_then(|c| c.metadata["artist_ref"].as_str()).filter(|r| !r.is_empty());
            if let (Some(reference), Some(fields)) = (named, track.metadata.as_object_mut()) {
                fields.insert("artist_ref".into(), json!(reference));
            }
        }
        track.title = text(item, "title");
        track.artist = Some(text(item, "artist")).filter(|a| !a.is_empty())
            .or_else(|| under.filter(|c| c.kind == CardKind::Album).map(|c| c.artist.clone())).unwrap_or_default();
        track.album = Some(text(item, "album")).filter(|a| !a.is_empty())
            .or_else(|| under.filter(|c| c.kind == CardKind::Album).map(|c| c.title.clone())).unwrap_or_default();
        track.album_id = album_id.unwrap_or(track.album_id);
        track.disc = number(item, "disc_number").and_then(|n| u16::try_from(n).ok()).unwrap_or(track.disc);
        track.number = number(item, "track_number").and_then(|n| u16::try_from(n).ok()).unwrap_or(track.number);
        track.duration_ms = number(item, "duration_ms").unwrap_or(0);
        let incoming = format(item);
        if !track.format.as_ref().is_some_and(|f| f.rate.is_some() && f.bits.is_some()) || incoming.as_ref().is_some_and(|f| f.rate.is_some() && f.bits.is_some()) {
            track.format = incoming.or(track.format.take());
        }
        let image = web(item);
        track.image = if image.is_empty() { under.map(|c| c.image.clone()).unwrap_or_default() } else { image };
        track.playable = item["available"].as_bool().unwrap_or(true);
        Some(id)
    }
}

/// Reads a listing of a process plugin into its shelf.
pub(super) fn read_shelf(catalog: &Catalog, natives: &Arc<Natives>, key: &str) -> Result<(), String> {
    let (plugin, address) = split(key);
    remember_domains(catalog, natives, plugin);
    if let Some(name) = address.strip_prefix("matching-artist-v3:").or_else(|| address.strip_prefix("matching-artist-v2:")) {
        return read_matching_artist(catalog, natives, plugin, key, name);
    }
    if let Some(reference) = address.strip_prefix("native:") {
        return super::pages::read(catalog, natives, plugin, reference);
    }
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
    if !natives.is_native(&plugin) { return Err("The plugin was disabled.".into()); }
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
    let mut folders = Value::Null;
    let refresh = catalog.stored_shelf(key).is_some_and(|shelf| shelf.read_at == 0);
    for page in 0..PAGES {
        let answer = natives.call(plugin, "library.list", json!({"kind": kind, "offset": page * PAGE, "limit": PAGE,"refresh":refresh && page == 0}), ASKED)?;
        if answer["folders"].is_array() { folders = answer["folders"].clone(); }
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
    if !natives.is_native(&plugin) { return Err("The plugin was disabled.".into()); }
    let mut shelf = Shelf { read_at: stamp, epoch: 1, document: json!({"folders":folders}), ..Shelf::default() };
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
    if !natives.is_native(&plugin) { return Err("The plugin was disabled.".into()); }
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

pub(super) fn same_primary_artist(album: &Value, artist: &Value) -> bool {
    let reference = text(album,"artist_ref");
    let expected = text(artist,"ref");
    if !reference.is_empty() && !expected.is_empty() { return reference == expected; }
    let name = text(album,"artist");
    !name.trim().is_empty() && name.trim().to_lowercase() == text(artist,"title").trim().to_lowercase()
}

fn read_matching_artist(catalog: &Catalog, natives: &Arc<Natives>, plugin: &str, key: &str, name: &str) -> Result<(), String> {
    let search = natives.call(plugin, "search", json!({"text":name,"limit":20}), ASKED)?;
    let artist = search["artists"].as_array().into_iter().flatten()
        .find(|item| text(item, "title").trim().to_lowercase() == name.trim().to_lowercase());
    if natives.has_pages(plugin) {
        if let Some(reference) = artist.and_then(|a| a["ref"].as_str()) {
            super::pages::read(catalog, natives, plugin, reference)?;
            let mut inner = catalog.lock();
            inner.store.shelves.insert(key.into(), Shelf { title: name.into(), read_at: now(), epoch: 1,
                document: json!({"resolved_ref": reference}), ..Default::default() });
            inner.shelf_errors.remove(key); inner.dirty = true;
            return Ok(());
        }
    }
    let mut items = Vec::new();
    if let Some(artist) = artist {
        let answer = natives.call(plugin, "artist.get", json!({"ref":artist["ref"]}), ASKED)?;
        items.extend(answer["albums"].as_array().into_iter().flatten().filter(|item| same_primary_artist(item,artist)).cloned());
        for shelf in answer["shelves"].as_array().into_iter().flatten() {
            items.extend(shelf["items"].as_array().into_iter().flatten().filter(|item| item["kind"] == "album" && same_primary_artist(item,artist)).cloned());
        }
    }
    let stamp = now();
    let mut inner = catalog.lock();
    if !natives.is_native(plugin) { return Err("The plugin was disabled.".into()); }
    let mut shelf = Shelf { title:name.into(), read_at:stamp, epoch:1, ..Default::default() };
    let mut seen = std::collections::HashSet::new();
    for item in items { if let Some(id) = inner.store.take_item(plugin, &item, stamp) { if seen.insert(id) { shelf.cards.push(id); } } }
    inner.shelf_errors.remove(key);
    inner.store.shelves.insert(key.into(), shelf);
    inner.dirty = true;
    Ok(())
}

/// A record's or a playlist's tracks, in one call.
/// One track by its entry, from `track.get`: the item a shelf would carry,
/// taken in with its record, so a list of mine can name it, show its cover
/// and lead to its record and artist.
pub(super) fn read_track(catalog: &Catalog, natives: &Arc<Natives>, path: &str) -> Result<(), String> {
    let (plugin, reference) = crate::source_opener::of_plugin(path).ok_or_else(|| "not a source's track".to_string())?;
    remember_domains(catalog, natives, &plugin);
    let answer = natives.call(&plugin, "track.get", json!({"ref": reference}), ASKED)?;
    let item = requested_track(&answer, &reference)?;
    let stamp = now();
    let mut inner = catalog.lock();
    if !natives.is_native(&plugin) { return Err("The plugin was disabled.".into()); }
    inner.store.take_item_track(&plugin, &item, None, stamp);
    inner.dirty = true;
    Ok(())
}

fn requested_track(answer: &Value, reference: &str) -> Result<Value, String> {
    let mut item = answer.get("track").filter(|t| t.is_object()).unwrap_or(answer).clone();
    if item["ref"].as_str().is_none_or(str::is_empty) {
        return Err("the source said nothing of the track".into());
    }
    // I retain the requested occurrence, including its opaque playback context.
    item["ref"] = json!(reference);
    Ok(item)
}

pub(super) fn read_tracks(catalog: &Catalog, natives: &Arc<Natives>, id: i64) -> Result<(), String> {
    let (plugin, reference, kind) = catalog.lock().store.cards.get(&id).map(|c| (c.plugin.clone(), c.identity.clone(), c.kind))
        .ok_or_else(|| chilpa_i18n::t("this part of the source is not there now"))?;
    remember_domains(catalog, natives, &plugin);
    let (method, holder) = match kind {
        CardKind::Album => ("album.get", "album"),
        CardKind::Playlist => ("playlist.get", "playlist"),
        CardKind::Artist | CardKind::Label => ("page.get", "header"),
    };
    let answer = natives.call(&plugin, method, json!({"ref": reference, "offset": 0, "limit": 10_000}), ASKED)?;
    let stamp = now();
    let mut inner = catalog.lock();
    if !natives.is_native(&plugin) { return Err("The plugin was disabled.".into()); }
    if let Some(itself) = answer.get(holder) {
        inner.store.take_item(&plugin, itself, stamp);
    }
    let under = inner.store.cards.get(&id).cloned().unwrap_or_default();
    let tracks: Vec<i64> = playable_rows(&answer).iter()
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
        card.metadata["available"] = json!(answer["availability"] != "unavailable" && answer[holder]["available"] != false);
        card.read_at = now();
        if card.format.is_none() { card.format = best; }
    }
    inner.card_errors.remove(&id);
    inner.dirty = true;
    for image in images {
        catalog.want_cover(&mut inner, &image);
    }
    drop(inner);
    if let Some(itself) = answer.get(holder).filter(|v| v["shelves"].is_array()) {
        super::pages::ingest_document(catalog, &plugin, &reference, itself);
    }
    Ok(())
}

fn playable_rows(answer: &Value) -> Vec<Value> {
    answer["tracks"].as_array().cloned().or_else(|| answer["shelves"].as_array()?.iter().find(|s| s["kind"] == "track")?["items"].as_array().cloned()).unwrap_or_default()
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
    fn restored_occurrences_keep_the_requested_path_after_metadata_recovery() {
        let reference = "track:7@context:9";
        let answer = json!({"track":{"ref":"track:7","title":"Song","kind":"track"}});
        let item = requested_track(&answer, reference).unwrap();
        let mut store = Store::default();
        store.take_item_track("fixture", &item, None, 1).unwrap();
        let path = crate::source_opener::entry_of("fixture", reference);
        assert_eq!(store.track_by_path(&path).unwrap().title, "Song");
        let mut restored: Store = serde_json::from_value(serde_json::to_value(store).unwrap()).unwrap();
        restored.index();
        assert_eq!(restored.track_by_path(&path).unwrap().metadata["ref"], reference);
        assert!(requested_track(&json!({"ref":""}), reference).is_err());
        assert!(requested_track(&json!({}), reference).is_err());
    }

    #[test]
    fn same_artist_excludes_appearances_and_recommendations() {
        let artist = json!({"ref":"artist:9","title":"The Band"});
        assert!(same_primary_artist(&json!({"artist_ref":"artist:9","artist":"The Band and Friends"}),&artist));
        assert!(same_primary_artist(&json!({"artist":" THE BAND "}),&artist));
        assert!(!same_primary_artist(&json!({"artist_ref":"artist:7","artist":"Unrelated Artist"}),&artist));
        assert!(!same_primary_artist(&json!({"artist_ref":"artist:7","artist":"The Band"}),&artist));
        assert!(!same_primary_artist(&json!({"title":"The Band tribute"}),&artist));
    }

    #[test]
    fn catalog_refresh_keeps_known_delivery_quality() {
        let mut store = Store::default();
        let generic = json!({"ref":"track:1","quality":{"codec":"flac"}});
        let detailed = json!({"ref":"track:1","quality":{"codec":"wav","bits":24,"rate":96000}});
        let id = store.take_item_track("p",&detailed,None,1).unwrap();
        store.take_item_track("p",&generic,None,2);
        assert_eq!(store.tracks[&id].format.as_ref().unwrap().quality().short(),"WAV 24/96");
        let updated = json!({"ref":"track:1","quality":{"codec":"wav","bits":16,"rate":44100}});
        store.take_item_track("p",&updated,None,3);
        assert_eq!(store.tracks[&id].format.as_ref().unwrap().quality().short(),"WAV 16/44.1");
    }

    #[test]
    fn a_changed_playlist_revision_refreshes_entries_without_changing_identity() {
        let mut store = Store::default();
        let mut item = json!({"kind":"playlist","ref":"list:1","title":"List","tracks":2,"updated_at":10});
        let id = store.take_item("fixture", &item, 1).unwrap();
        store.cards.get_mut(&id).unwrap().tracks = Some(vec![-2,-3]);
        store.take_item("fixture", &item, 2);
        assert!(store.cards[&id].tracks.is_some(), "an unchanged listing keeps the tracks");
        item["updated_at"] = json!(11);
        assert_eq!(store.take_item("fixture", &item, 3), Some(id));
        assert_eq!(store.cards[&id].tracks.as_deref(), Some(&[-2, -3][..]));
        assert_eq!(store.cards[&id].read_at, 0);
        store.cards.get_mut(&id).unwrap().tracks = Some(vec![-2,-3]);
        item["editable"] = json!(true);
        store.take_item("fixture", &item, 4);
        assert_eq!(store.cards[&id].tracks.as_deref(), Some(&[-2, -3][..]), "I retain visible rows until the refresh answers");
        assert_eq!(store.cards[&id].read_at, 0);
    }

    #[test]
    fn unavailable_tracks_stay_in_the_listing_and_can_recover() {
        let mut store = Store::default();
        let mut item = json!({"ref":"track:1","title":"Song","available":false});
        let id = store.take_item_track("fixture", &item, None, 1).unwrap();
        assert!(!store.tracks[&id].playable);
        assert_eq!(store.tracks[&id].title, "Song");
        item["available"] = json!(true);
        assert_eq!(store.take_item_track("fixture", &item, None, 2), Some(id));
        assert!(store.tracks[&id].playable);
    }

    #[test]
    fn an_artist_or_label_plays_its_primary_track_section() {
        let answer = json!({"shelves":[{"kind":"album","items":[{"ref":"album:1"}]},
            {"kind":"track","items":[{"ref":"track:7"}]},{"kind":"track","items":[{"ref":"track:8"}]}]});
        assert_eq!(playable_rows(&answer),vec![json!({"ref":"track:7"})]);
        assert_eq!(playable_rows(&json!({"tracks":[{"ref":"track:9"}]})),vec![json!({"ref":"track:9"})]);
    }
    #[test]
    fn a_record_and_its_tracks_from_items() {
        let mut store = Store::default();
        let record = json!({"kind": "album", "ref": "album:x1", "title": "Record", "artist": "Band", "year": 1999,
            "image": "https://i/l.jpg", "quality": {"codec": "flac", "bits": 24, "rate": 96000}});
        let id = store.take_item("p", &record, 10).unwrap();
        assert_eq!(store.take_item("p", &record, 11), Some(id), "the same ref is the same card");
        let card = store.cards[&id].clone();
        assert_eq!((card.title.as_str(), card.artist.as_str(), card.year), ("Record", "Band", Some(1999)));
        assert_eq!(card.format, Some(Format { codec: "flac".into(), bits: Some(24), rate: Some(96000), advertised_hires: false }));
        let track = store.take_item_track("p", &json!({"ref": "track:1", "title": "Song", "duration_ms": 1000, "track_number": 2}), Some(&card), 10).unwrap();
        let track = &store.tracks[&track];
        assert_eq!((track.artist.as_str(), track.album.as_str(), track.album_id, track.number), ("Band", "Record", id, 2));
        assert!(track.playable);
        assert_eq!(track.image, "https://i/l.jpg", "its record's cover");
    }

    #[test]
    fn partial_pages_keep_album_track_and_disc_numbers() {
        let catalog = Catalog::new();
        let ids = {
            let mut inner = catalog.lock();
            let record = json!({"kind":"album","ref":"album:1","title":"Record"});
            let album = inner.store.take_item("fixture", &record, 1).unwrap();
            let card = inner.store.cards[&album].clone();
            [("track:1", 3, 1), ("track:2", 1, 2)].map(|(reference, number, disc)| {
                inner.store.take_item_track("fixture", &json!({"kind":"track","ref":reference,
                    "title":"Song","track_number":number,"disc_number":disc}), Some(&card), 1).unwrap()
            })
        };
        super::super::pages::ingest_document(&catalog, "fixture", "artist:1", &json!({"shelves":[{
            "id":"tracks","kind":"track","items":[
                {"kind":"track","ref":"track:1","title":"Song"},
                {"kind":"track","ref":"track:2","title":"Song","track_number":null,"disc_number":null}
            ]}]}));
        let mut inner = catalog.lock();
        for (id, expected) in ids.into_iter().zip([(3, 1), (1, 2)]) {
            let track = &inner.store.tracks[&id];
            let row = super::super::view::row_of(track, String::new());
            assert_eq!((row.number, row.disc), expected);
        }
        let invalid = json!({"ref":"track:1","track_number":65536,"disc_number":"unknown"});
        inner.store.take_item_track("fixture", &invalid, None, 2);
        assert_eq!((inner.store.tracks[&ids[0]].number, inner.store.tracks[&ids[0]].disc), (3, 1));
        let updated = json!({"ref":"track:1","track_number":4,"disc_number":2});
        assert_eq!(inner.store.take_item_track("fixture", &updated, None, 3), Some(ids[0]));
        let row = super::super::view::row_of(&inner.store.tracks[&ids[0]], String::new());
        assert_eq!((row.number, row.disc), (4, 2));
        let unknown = inner.store.take_item_track("fixture", &json!({"ref":"track:3"}), None, 4).unwrap();
        assert_eq!((inner.store.tracks[&unknown].number, inner.store.tracks[&unknown].disc), (0, 0));
    }

    #[test]
    fn advertised_high_resolution_needs_no_invented_bits_or_rate() {
        let mut store = Store::default();
        let id = store.take_item("p", &json!({"kind":"album","ref":"album:1",
            "quality":{"codec":"flac","tier":"hi_res"}}), 1).unwrap();
        let quality = store.cards[&id].format.as_ref().unwrap().quality();
        assert_eq!(quality.tier, crate::quality::Tier::HiRes);
        assert_eq!(quality.short(), "FLAC Hi-Res");
        assert_eq!((quality.bits, quality.rate_hz), (None, None));
        let track = store.take_item_track("p", &json!({"ref":"track:1",
            "quality":{"codec":"flac","tier":"hi_res"}}), None, 1).unwrap();
        let catalog = Catalog::new();
        let path = store.tracks[&track].path.clone();
        catalog.lock().store = store;
        catalog.measured(&path, Some("flac"), Some(16), 44100);
        let quality = catalog.lock().store.tracks[&track].format.as_ref().unwrap().quality();
        assert_eq!(quality.tier, crate::quality::Tier::Lossless);
        assert_eq!(quality.short(), "FLAC 16/44.1");
    }

    #[test]
    fn a_track_on_its_own_still_leads_to_its_record() {
        let mut store = Store::default();
        let track = store.take_item_track("p", &json!({"ref": "track:1", "title": "Song", "artist": "Band",
            "album": "Record", "album_ref": "album:x1", "artist_ref": "artist:9", "image": "https://i/s.jpg"}), None, 10).unwrap();
        let album = store.tracks[&track].album_id;
        assert!(album < 0, "a card was made for its record");
        assert_eq!(store.cards[&album].title, "Record");
        assert_eq!(store.cards[&album].identity, "album:x1");
        assert_eq!(store.cards[&album].metadata["artist_ref"], "artist:9");
    }

    #[test]
    fn a_records_song_leads_to_its_records_artist_unless_it_names_its_own() {
        let mut store = Store::default();
        let album = store.take_item("p", &json!({"kind": "album", "ref": "album:x1", "title": "Record", "artist": "Band", "artist_ref": "artist:9"}), 10).unwrap();
        let record = store.cards[&album].clone();
        let plain = store.take_item_track("p", &json!({"ref": "track:1", "title": "Song"}), Some(&record), 10).unwrap();
        assert_eq!(store.tracks[&plain].metadata["artist_ref"], "artist:9");
        let guest = store.take_item_track("p", &json!({"ref": "track:2", "title": "Duet", "artist_ref": "artist:3"}), Some(&record), 10).unwrap();
        assert_eq!(store.tracks[&guest].metadata["artist_ref"], "artist:3");
    }
}
