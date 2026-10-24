//! What a file manager's "Open with" brings opens as temporary media: a
//! file by itself, a folder with what is under it, both together, and
//! nothing that is not music.
use chilpa_api::open_media;

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

#[test]
fn a_launch_opens_its_files_and_folders_and_skips_what_is_not_music() {
    let dir = tempfile::tempdir().unwrap();
    for (key, folder) in [("XDG_DATA_HOME", "data"), ("XDG_CONFIG_HOME", "config"), ("XDG_CACHE_HOME", "cache")] {
        std::env::set_var(key, dir.path().join(folder));
    }
    let folder = dir.path().join("Album");
    std::fs::create_dir_all(&folder).unwrap();
    wave(&folder.join("01.wav"));
    wave(&folder.join("02.wav"));
    let single = dir.path().join("Single.wav");
    wave(&single);
    let notes = dir.path().join("notes.txt");
    std::fs::write(&notes, "not music").unwrap();

    let one = open_media::read("launch", single.to_str().unwrap()).unwrap();
    assert_eq!(one.tracks.len(), 1, "a file opens by itself, not its folder");

    let both = format!("{}\n{}\n{}", folder.display(), single.display(), notes.display());
    let media = open_media::read("launch", &both).unwrap();
    assert_eq!(media.tracks.len(), 3, "the folder's two and the file, not the text");

    let error = open_media::read("launch", notes.to_str().unwrap()).err().unwrap();
    assert!(!error.is_empty(), "nothing playable says so");
}
