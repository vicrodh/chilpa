//! A record of several discs opened from Open media shows a header before
//! each disc: the record it is of, which disc, and how many tracks.
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

/// What a list shows of each entry: a header's label and count, or a
/// track's title.
fn shown(rows: &[TrackEntry]) -> Vec<String> {
    rows.iter()
        .map(|row| match row {
            TrackEntry::Header(header) => format!("[{} | {}]", header.label, header.meta),
            TrackEntry::Track(track) => track.title.clone(),
        })
        .collect()
}

#[test]
fn opened_media_of_several_discs_has_a_header_before_each() {
    let dir = tempfile::tempdir().unwrap();
    for (key, folder) in [("XDG_DATA_HOME", "data"), ("XDG_CONFIG_HOME", "config"), ("XDG_CACHE_HOME", "cache")] {
        std::env::set_var(key, dir.path().join(folder));
    }
    let folder = dir.path().join("Two discs");
    std::fs::create_dir_all(&folder).unwrap();
    for name in ["a.wav", "b.wav", "c.wav", "d.wav", "e.wav"] {
        wave(&folder.join(name));
    }

    // The files carry no tags; the test states what their tags would.
    let mut media = open_media::read("folder", folder.to_str().unwrap()).unwrap();
    assert_eq!(media.tracks.len(), 5);
    for (track, (title, disc, number)) in media.tracks.iter_mut().zip([("One", 1, 1), ("Two", 1, 2), ("Three", 1, 3), ("Four", 2, 1), ("Five", 2, 2)]) {
        track.title = title.to_string();
        track.album = "Two discs".to_string();
        track.disc = disc;
        track.number = number;
    }
    let single = media.tracks[..3].to_vec();

    let mut api = Chilpa::new();
    api.use_output("file:capture", false);
    api.open_media(media);
    let (rows, keys) = api.media_rows("");
    assert_eq!(shown(&rows), ["[Two discs | Disc 1 \u{b7} 3 tracks]", "One", "Two", "Three", "[Two discs | Disc 2 \u{b7} 2 tracks]", "Four", "Five"]);
    assert!(rows.iter().all(|row| !matches!(row, TrackEntry::Header(header) if !header.record)), "each stands for a record's disc");
    // A header has no key; every track keeps its own.
    assert_eq!(keys.len(), rows.len());
    assert_eq!(keys.iter().map(String::is_empty).collect::<Vec<_>>(), [true, false, false, false, true, false, false]);

    // The filter keeps the disc of what it leaves.
    let (rows, keys) = api.media_rows("four");
    assert_eq!(shown(&rows), ["[Two discs | Disc 2 \u{b7} 1 track]", "Four"]);
    assert_eq!(keys.len(), 2);

    // A track still plays from its row.
    let (_, keys) = api.media_rows("");
    assert!(api.listen_row_action(&keys[5], "play", ""));

    // One disc alone carries no header.
    api.open_media(open_media::OpenedMedia { title: "One disc".into(), tracks: single, about: Vec::new() });
    let (rows, keys) = api.media_rows("");
    assert_eq!(shown(&rows), ["One", "Two", "Three"]);
    assert!(keys.iter().all(|key| !key.is_empty()));
}
