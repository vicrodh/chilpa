//! Library work runs on broker workers, never on the window thread.
use chilpa_library::{Library, PlaylistEdit, PlaylistEntry, PlaylistTrackMetadata};
use serde_json::{json, Value};
use std::collections::HashSet;
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn writer() -> Result<Library, String> {
    Library::open_existing(&Library::default_path()).map_err(error)
}
pub(super) fn search(input: &Value) -> Result<Value, String> {
    let library = Library::open_read_only(&Library::default_path()).map_err(error)?;
    let text = input["text"].as_str().unwrap_or_default();
    if text.len() > 1000 {
        return Err("The search text is too long.".into());
    }
    let found = library
        .import_candidates(
            text,
            input["isrc"].as_str().unwrap_or_default(),
            input["limit"].as_u64().unwrap_or(20).clamp(1, 100) as usize,
        )
        .map_err(error)?;
    Ok(
        json!({"tracks":found.into_iter().map(|r|json!({"kind":"track","source":"local","ref":r.id.to_string(),"title":r.title,"artist":r.artist,"album":r.album,"duration_ms":r.duration_ms,"isrc":r.isrc,"available":true,"quality":{"codec":r.codec,"bits":r.bits,"rate":r.rate}})).collect::<Vec<_>>()}),
    )
}
pub(super) fn create(input: &Value, allowed: &HashSet<String>) -> Result<Value, String> {
    let library = writer()?;
    create_in(&library, input, allowed)
}
fn create_in(library: &Library, input: &Value, allowed: &HashSet<String>) -> Result<Value, String> {
    let tracks = input["tracks"]
        .as_array()
        .filter(|v| !v.is_empty() && v.len() <= 10000)
        .ok_or("A local import needs between one and 10000 tracks.")?;
    library
        .connection()
        .execute_batch("BEGIN IMMEDIATE")
        .map_err(error)?;
    let outcome = (|| {
        let mut entries: Vec<PlaylistEntry> = Vec::new();
        for item in tracks {
            let source = item["source"].as_str().unwrap_or_default();
            let reference = item["ref"]
                .as_str()
                .filter(|r| !r.is_empty())
                .ok_or("A track has no reference.")?;
            if source == "local" {
                let id = reference
                    .parse::<i64>()
                    .map_err(|_| "Invalid local track reference.")?;
                if !library.is_available_local_track(id).map_err(error)? {
                    return Err("A local track is no longer available.".into());
                }
                entries.extend(library.playlist_entries_for_tracks(&[id]).map_err(error)?);
            } else {
                if input["target"] != "mixed" || !allowed.contains(source) {
                    return Err("This track's source was not selected for the import.".into());
                }
                let source_id = library
                    .source_id(crate::source_opener::PROVIDER, source)
                    .map_err(error)?;
                let metadata = PlaylistTrackMetadata {
                    external_id: crate::source_opener::entry_of(source, reference),
                    title: item["title"].as_str().unwrap_or_default().into(),
                    artist: item["artist"].as_str().unwrap_or_default().into(),
                    album: item["album"].as_str().unwrap_or_default().into(),
                    duration_ms: item["duration_ms"].as_u64().unwrap_or(0),
                    codec: item["quality"]["codec"].as_str().map(str::to_owned),
                    sample_rate_hz: item["quality"]["rate"]
                        .as_u64()
                        .and_then(|n| u32::try_from(n).ok()),
                    bit_depth: item["quality"]["bits"]
                        .as_u64()
                        .and_then(|n| u16::try_from(n).ok()),
                    isrc: item["isrc"].as_str().map(str::to_owned),
                    ..Default::default()
                };
                entries.extend(
                    library
                        .cache_playlist_tracks(source_id, &[metadata])
                        .map_err(error)?,
                );
            }
        }
        let source = library.source_id("local", "default").map_err(error)?;
        let edit = PlaylistEdit {
            name: input["title"].as_str().unwrap_or_default().into(),
            description: input["description"].as_str().unwrap_or_default().into(),
            folder_id: input["folder"].as_i64().filter(|n| *n > 0),
            ..Default::default()
        };
        let id = library.create_playlist_with(source, &edit).map_err(error)?;
        library.set_playlist_entries(id, &entries).map_err(error)?;
        Ok(json!({"ref":id.to_string(),"local_id":id,"added":entries.len()}))
    })();
    match outcome {
        Ok(value) => {
            library
                .connection()
                .execute_batch("COMMIT")
                .map_err(error)?;
            Ok(value)
        }
        Err(e) => {
            let _ = library.connection().execute_batch("ROLLBACK");
            Err(e)
        }
    }
}
pub(super) fn remember_remote(input: &Value, reference: &str) -> Result<(), String> {
    let library = writer()?;
    let source = library
        .source_id(
            crate::source_opener::PROVIDER,
            input["target"].as_str().unwrap_or_default(),
        )
        .map_err(error)?;
    let id = library
        .sync_source_playlist(
            source,
            reference,
            input["title"].as_str().unwrap_or_default(),
            input["description"].as_str().unwrap_or_default(),
            None,
        )
        .map_err(error)?;
    library
        .move_playlist_to_folder(id, input["folder"].as_i64().filter(|n| *n > 0))
        .map_err(error)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mixed_creation_is_atomic_and_local_only_refuses_remote_tracks() {
        let library = Library::in_memory().unwrap();
        let input = json!({"target":"mixed","title":"Mixed","tracks":[{"source":"source-a","ref":"track:1","title":"First"},{"source":"source-b","ref":"track:2","title":"Second"}]});
        let allowed = ["source-a".into(), "source-b".into()].into_iter().collect();
        let result = create_in(&library, &input, &allowed).unwrap();
        let id = result["local_id"].as_i64().unwrap();
        assert_eq!(library.playlist_entries(id).unwrap().len(), 2);
        let mut invalid = input.clone();
        invalid["target"] = json!("local");
        assert!(create_in(&library, &invalid, &allowed).is_err());
        assert_eq!(library.playlists().unwrap().len(), 1);
        assert!(create_in(&library, &input, &HashSet::new()).is_err());
        assert_eq!(library.playlists().unwrap().len(), 1);
    }
}
