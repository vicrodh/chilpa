//! Temporary media never imports tracks and closing it releases its queue.
#[path = "support/sacd.rs"]
mod sacd;
use chilpa_api::{open_media, Chilpa, TrackEntry};
use std::time::{Duration, Instant};

#[test]
fn temporary_media_owns_its_queue_and_does_not_survive_closing() {
    let dir = tempfile::tempdir().unwrap();
    for (key, folder) in [("XDG_DATA_HOME", "data"), ("XDG_CONFIG_HOME", "config"), ("XDG_CACHE_HOME", "cache")] {
        std::env::set_var(key, dir.path().join(folder));
    }
    let image = dir.path().join("Album.ISO");
    sacd::image(&image);
    let media = open_media::read("sacd", image.to_str().unwrap()).unwrap();
    assert_eq!(media.tracks.len(), 2);
    assert!(media.tracks.iter().all(|t| t.available));
    let mut api = Chilpa::new();
    api.use_output("file:capture", false);
    let paths: Vec<_> = media.tracks.iter().map(|t| t.path.clone()).collect();
    api.open_media(media);
    let (rows, keys) = api.media_rows("");
    assert_eq!(rows.len(), 2);
    assert!(matches!(&rows[0], TrackEntry::Track(row) if row.number == 1));
    assert!(api.queue_paths().is_empty(), "opening does not start playback");
    let saved = std::fs::read_to_string(chilpa_dirs::data().join("listenlist.json")).unwrap();
    assert!(!saved.contains("Album.ISO"), "temporary media is not restored");
    assert!(api.listen_row_action(&keys[0], "play", ""));
    assert_eq!(api.queue_paths().len(), 2);
    api.close_media();
    let until = Instant::now() + Duration::from_secs(2);
    while Instant::now() < until { api.poll(); std::thread::sleep(Duration::from_millis(10)); }
    assert!(api.queue_paths().is_empty());
    assert!(api.now_playing().path.is_empty(), "the player bar must forget the closed media");
    assert!(!api.now_playing().playing);
    assert_eq!(api.media_json(), "{}");
    assert!(!api.listen_row_action(&keys[0], "play", ""), "closed rows cannot restart playback");
    api.listen_undo();
    assert_eq!(api.media_json(), "{}");
    assert!(!api.listen_row_action(&keys[0], "play", ""));
    assert_eq!(api.reader().unwrap().folder("").total, 0, "opening is not importing");
    assert!(open_media::read("sacd", dir.path().join("missing.iso").to_str().unwrap()).is_err());
    assert!(open_media::read("cd", "").is_err());
    api.open_media(open_media::read("sacd", image.to_str().unwrap()).unwrap());
    let (_, keys) = api.media_rows("");
    assert!(api.listen_row_action(&keys[0], "play", ""));
    let other = api.listen_create("Other", &[image.display().to_string()]);
    api.listen_select(other);
    api.play_tracks(&paths, "file:capture", false);
    api.close_media();
    assert_eq!(api.queue_paths(), paths, "closing media preserves later playback from elsewhere");
    let state: serde_json::Value = serde_json::from_str(&api.listen_json()).unwrap();
    assert!(state["tabs"].as_array().unwrap().iter().any(|t| t["id"] == other.to_string()));
}
