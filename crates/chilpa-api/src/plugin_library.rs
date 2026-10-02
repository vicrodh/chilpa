//! Explicitly selected plugin collections share the ordinary library model.
use crate::{plugins::Plugins, source_opener};
use chilpa_plugin::{messages::EntryKind, presentation::{Entity, Presentation, SourceDocument}};
use serde_json::{json, Value};
use std::{collections::HashSet, sync::{Arc, Mutex}, time::{Duration, Instant}};

fn call(plugins: &Arc<Mutex<Plugins>>, id: &str, operation: &str, input: Value) -> Result<Vec<u8>, String> {
    plugins.lock().map_err(|_| "The plugin host is unavailable.")?
        .call_waiting(id, operation, &serde_json::to_vec(&input).map_err(|_| "Invalid library request.")?)
}

pub(crate) fn import(plugins: Arc<Mutex<Plugins>>, id: &str, name: &str, at: &str) -> Result<usize, String> {
    let tracks = collect(id, at, |operation, input| call(&plugins, id, operation, input))?;
    let library = chilpa_core::open_library().map_err(|_| "The library could not be opened.")?;
    save(&library, id, name, &tracks)?;
    let domains = plugins.lock().ok().and_then(|plugins| plugins.worker_domains(id)).unwrap_or_default();
    if let Ok(Some(source)) = library.find_source(source_opener::PROVIDER, id) {
        let started = Instant::now();
        for (album, address) in library.pending_server_covers(source, 10000).unwrap_or_default() {
            if started.elapsed() > Duration::from_secs(30) { break; }
            if let Some(path) = crate::pictures::keep_approved(&address, &domains) {
                let _ = library.set_server_cover(album, Some(&path));
            }
        }
    }
    Ok(tracks.len())
}

fn collect(id: &str, at: &str, mut request: impl FnMut(&str, Value) -> Result<Vec<u8>, String>) -> Result<Vec<chilpa_library::ServerTrack>, String> {
    let started = Instant::now();
    let mut pending = vec![(at.to_owned(), None, 0_u8, String::new())];
    let (mut visited, mut identities, mut tracks) = (HashSet::new(), HashSet::new(), Vec::new());
    let mut pages = 0;
    while let Some((at, cursor, depth, parent_album)) = pending.pop() {
        pages += 1;
        if pages > 250 || started.elapsed() > Duration::from_secs(300) {
            return Err("This collection exceeds the import limit. Choose a smaller collection.".into());
        }
        if !visited.insert((at.clone(), cursor.clone())) { return Err("The collection repeats a page.".into()); }
        let bytes = request(chilpa_plugin::op::BROWSE, json!({"at":at,"text":"","cursor":cursor,"language":"en","picks":{}}))?;
        let document: SourceDocument = serde_json::from_slice(&bytes).map_err(|_| "Invalid collection response.")?;
        if let Some(next) = document.listing.next.clone() {
            pending.push((at, Some(next), depth, parent_album.clone()));
        }
        let has_recordings = document.listing.entries.iter().any(|entry| entry.playable && entry.kind != EntryKind::Folder);
        for entry in document.listing.entries {
            let mut display = document.presentation.get(&entry.id).cloned().unwrap_or_default();
            if entry.kind == EntryKind::Folder {
                // Unknown folders may execute actions rather than enumerate music.
                // A recording list can also advertise related collections or disc duplicates.
                if has_recordings || !matches!(display.entity, Entity::Album | Entity::Playlist) { continue; }
                if depth >= 5 { return Err("The collection is too deeply nested.".into()); }
                let album = if display.entity == Entity::Album { entry.title.clone() } else { parent_album.clone() };
                pending.push((entry.id, None, depth + 1, album));
                continue;
            }
            if !entry.playable || !identities.insert(entry.id.clone()) { continue; }
            // A metadata read never resolves a stream or consumes a media URL.
            if !display.metadata.contains_key("album") || !display.metadata.contains_key("artist") {
                for _ in 0..2 {
                    let bytes = request("source.metadata", json!({"id":entry.id}))?;
                    let reply: Value = serde_json::from_slice(&bytes).map_err(|_| "Invalid recording metadata.")?;
                    if let Ok(extra) = serde_json::from_value::<Presentation>(reply["presentation"].clone()) {
                        display.metadata.extend(extra.metadata);
                        if !extra.title.is_empty() { display.title = extra.title; }
                        if extra.advertised_quality.sample_rate_hz.is_some() { display.advertised_quality = extra.advertised_quality; }
                    }
                    if display.metadata.contains_key("album") && display.metadata.contains_key("artist") { break; }
                }
            }
            let field = |key: &str| display.metadata.get(key).cloned().unwrap_or_default();
            let album = if field("album").is_empty() { parent_album.clone() } else { field("album") };
            let quality = &display.advertised_quality;
            if tracks.len() >= 10000 {
                return Err("This collection exceeds the import limit. Choose a smaller collection.".into());
            }
            tracks.push(chilpa_library::ServerTrack {
                external_id: source_opener::entry_of(id, &entry.id),
                title: if display.title.is_empty() { entry.title } else { display.title.clone() },
                artist: field("artist"), artists: Vec::new(), album, album_artist: field("artist"), album_key: String::new(),
                cover: entry.image_url, disc_number: u16::try_from(display.disc_number).ok().filter(|n| *n > 0),
                disc_total: u16::try_from(display.disc_count).ok().filter(|n| *n > 0),
                track_number: u16::try_from(display.track_number).ok().filter(|n| *n > 0),
                year: (display.year > 0).then_some(display.year), genres: field("genre").split(';').filter(|s| !s.is_empty()).map(str::to_owned).collect(),
                duration_ms: entry.duration_ms, codec: (!quality.format.is_empty()).then(|| quality.format.clone()),
                bit_depth: quality.bit_depth.and_then(|n| u8::try_from(n).ok()), sample_rate_hz: quality.sample_rate_hz,
                channels: quality.channels, isrc: display.metadata.get("isrc").cloned(),
                recording_mbid: display.metadata.get("musicbrainz_recording_id").cloned(), revision: None, added_at: None,
            });
        }
    }
    if tracks.is_empty() { return Err("This collection contains no importable recordings.".into()); }
    Ok(tracks)
}

fn save(library: &chilpa_library::Library, id: &str, name: &str, tracks: &[chilpa_library::ServerTrack]) -> Result<(), String> {
    let source = library.source_id(source_opener::PROVIDER, id).map_err(|_| "The source could not be saved.")?;
    library.set_source_label(source, name).map_err(|_| "The source name could not be saved.")?;
    let icon = crate::plugin_icons::image_url(&crate::Chilpa::plugins_dir(), id);
    library.set_source_icon(source, &icon).map_err(|_| "The source icon could not be saved.")?;
    library.save_server_tracks(source, crate::now_unix(), tracks).map_err(|_| "The collection could not be saved.")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn importing_never_enters_unknown_actions_or_related_collections() {
        let tracks = collect("org.example.guest", "collection", |operation, input| {
            assert_eq!(operation, chilpa_plugin::op::BROWSE);
            assert_eq!(input["at"], "collection");
            Ok(serde_json::to_vec(&json!({"entries":[
                {"id":"remove","title":"Remove unavailable collection","kind":"folder","playable":false},
                {"id":"related","title":"Related release","kind":"folder","playable":false},
                {"id":"guest://1","title":"Recording","kind":"item","playable":true}],
                "presentation":{"related":{"entity":"album"},"guest://1":{"metadata":{"artist":"Artist","album":"Release"}}}
            })).unwrap())
        }).unwrap();
        assert_eq!(tracks.len(), 1);
    }
    #[test]
    fn imported_music_keeps_its_source_identity_and_appears_in_the_shared_library() {
        let tracks = collect("org.example.guest", "collection", |operation, input| {
            assert_eq!(operation, chilpa_plugin::op::BROWSE);
            let response = if input["at"] == "collection" {
                json!({"entries":[{"id":"album","title":"A release","kind":"folder","playable":false}],
                    "presentation":{"album":{"entity":"album"}}})
            } else {
                json!({"entries":[{"id":"guest://17","title":"A recording","kind":"item","playable":true,"duration_ms":180000}],
                    "presentation":{"guest://17":{"entity":"track","title":"A recording","track_number":1,
                        "metadata":{"artist":"An artist","album":"A release"},"advertised_quality":{"format":"flac","sample_rate_hz":96000,"bit_depth":24}}}})
            };
            Ok(serde_json::to_vec(&response).unwrap())
        }).unwrap();
        let library = chilpa_library::Library::in_memory().unwrap();
        save(&library, "org.example.guest", "Guest source", &tracks).unwrap();
        assert_eq!(library.track_count(false).unwrap(), 1);
        let path = source_opener::entry_of("org.example.guest", "guest://17");
        let card = library.server_track_card(&path).unwrap().unwrap();
        assert_eq!((card.0.as_str(), card.1.as_str(), card.2.as_str()), ("A recording", "An artist", "A release"));
        let source = library.find_source(source_opener::PROVIDER, "org.example.guest").unwrap().unwrap();
        assert_eq!(library.imported_source_state(source).unwrap(), (1, true));
        library.show_imported_source(source, false).unwrap();
        assert_eq!(library.track_count(false).unwrap(), 0);
        assert!(library.server_track_card(&path).unwrap().is_some());
    }
}
