//! What is opened from Open media shows in its queue as its page shows it:
//! with the title, the artist and the length its file states, not a row
//! made from its name.
use chilpa_api::{open_media, Chilpa, TrackEntry};

/// A second of silence, 16 bits, mono, with no tags.
fn wave(path: &std::path::Path) {
    let frames: u32 = 44_100;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + frames * 2).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&44_100u32.to_le_bytes());
    bytes.extend_from_slice(&88_200u32.to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&(frames * 2).to_le_bytes());
    bytes.resize(bytes.len() + (frames * 2) as usize, 0);
    std::fs::write(path, bytes).unwrap();
}

fn tracks(rows: Vec<TrackEntry>) -> Vec<chilpa_api::TrackRow> {
    rows.into_iter().filter_map(|row| if let TrackEntry::Track(track) = row { Some(track) } else { None }).collect()
}

#[test]
fn the_queue_of_opened_media_carries_what_its_files_state() {
    let dir = tempfile::tempdir().unwrap();
    for (key, folder) in [("XDG_DATA_HOME", "data"), ("XDG_CONFIG_HOME", "config"), ("XDG_CACHE_HOME", "cache")] {
        std::env::set_var(key, dir.path().join(folder));
    }
    let folder = dir.path().join("Loose");
    std::fs::create_dir_all(&folder).unwrap();
    wave(&folder.join("01.wav"));
    wave(&folder.join("02.wav"));

    let media = open_media::read("folder", folder.to_str().unwrap()).unwrap();
    assert_eq!(media.tracks.len(), 2);
    assert!(media.tracks.iter().all(|t| t.duration_ms >= 990), "the page reads each file's length");
    let mut api = Chilpa::new();
    api.use_output("file:capture", false);
    api.open_media(media);
    let state: serde_json::Value = serde_json::from_str(&api.media_json()).unwrap();
    let buffer: u64 = state["buffer"].as_str().unwrap().parse().unwrap();

    // The queues view, on the media's own queue.
    let (rows, _) = api.listen_rows(buffer, "", true);
    let rows = tracks(rows);
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|t| t.duration_ms >= 990), "the queue says each track's length: {:?}", rows.iter().map(|t| t.duration_ms).collect::<Vec<_>>());

    // The side queue, once it plays.
    let (_, keys) = api.media_rows("");
    assert!(api.listen_row_action(&keys[0], "play", ""));
    let queued = api.queue_rows();
    assert_eq!(queued.len(), 2);
    assert!(queued.iter().all(|t| t.duration_ms >= 990), "and so does the side queue");

    // Closed, its rows are nobody's again.
    api.close_media();
    assert!(api.queue_rows().is_empty());
}
