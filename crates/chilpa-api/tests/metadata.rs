//! The metadata editor, end to end.
//!
//! One test on purpose: it points the library, the settings and the cache
//! at a temporary directory, and those belong to the whole process.
//!
//! Nothing here goes out to the network: the editor is handed one that
//! answers what it was loaded with. And nothing plays.

use chilpa_api::Chilpa;
use chilpa_meta::{Http, Reply};
use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// A network that answers what it was loaded with, and writes down what it
/// was asked.
#[derive(Default)]
struct Canned {
    replies: Mutex<VecDeque<(u16, Vec<u8>)>>,
    asked: Mutex<Vec<String>>,
}

impl Http for Canned {
    fn get(&self, url: &str, _: &str, _: Duration, _: usize) -> Result<Reply, String> {
        self.asked.lock().unwrap().push(url.to_string());
        match self.replies.lock().unwrap().pop_front() {
            Some((status, body)) => Ok(Reply { status, headers: BTreeMap::new(), body }),
            None => Err("nothing was loaded".into()),
        }
    }
}

fn tagged(target: &Path, title: &str, track: u32, album_artist: bool) -> bool {
    let _ = std::fs::create_dir_all(target.parent().unwrap());
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-y", "-loglevel", "error", "-f", "lavfi", "-i", "sine=frequency=440:duration=1", "-c:a", "flac"])
        .args(["-metadata", &format!("title={title}")])
        .args(["-metadata", "artist=Someone"])
        .args(["-metadata", "album=A record"])
        .args(["-metadata", "genre=Rock"])
        .args(["-metadata", "date=1990-05-01"])
        .args(["-metadata", &format!("track={track}")]);
    if album_artist {
        cmd.args(["-metadata", "album_artist=Someone"]);
    }
    cmd.arg(target).output().map(|o| o.status.success()).unwrap_or(false)
}

/// Every file under a folder that is not one of the editor's own, with
/// its bytes.
fn audio(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in std::fs::read_dir(next).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|e| e == "flac") {
                out.push((path.clone(), std::fs::read(&path).unwrap()));
            }
        }
    }
    out.sort();
    out
}

/// Polls until the editor is in that state.
fn until(chilpa: &mut Chilpa, state: &str) {
    let limit = Instant::now() + Duration::from_secs(20);
    while Instant::now() < limit {
        if chilpa.metadata_poll().state == state {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("the editor never got to \"{state}\": {:?}", chilpa.metadata_view());
}

/// Polls until a search, a load or a cover has come back.
fn until_idle(chilpa: &mut Chilpa) {
    let limit = Instant::now() + Duration::from_secs(20);
    while Instant::now() < limit {
        let view = chilpa.metadata_poll();
        if !view.lookup.searching && !view.lookup.loading && !view.artwork.searching && !view.artwork.loading {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("what was asked for never came back");
}

fn album_titled(chilpa: &mut Chilpa, title: &str) -> i64 {
    chilpa.albums_sorted(chilpa_api::AlbumSort::Title, 100, 0).into_iter().find(|a| a.title == title).map(|a| a.id).unwrap_or(0)
}

fn field<'a>(view: &'a chilpa_api::MetadataView, name: &str) -> &'a str {
    view.album.iter().find(|f| f.name == name).map(|f| f.value.as_str()).unwrap_or("")
}

#[test]
fn the_editor_end_to_end() {
    let dir = std::env::temp_dir().join("chilpa-api-metadata");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("XDG_DATA_HOME", dir.join("data"));
    std::env::set_var("XDG_CONFIG_HOME", dir.join("config"));
    std::env::set_var("XDG_CACHE_HOME", dir.join("cache"));

    let music = dir.join("music");
    let made = tagged(&music.join("A record/CD1/01.flac"), "One", 1, false)
        && tagged(&music.join("A record/CD1/02.flac"), "Two", 2, false)
        && tagged(&music.join("A record/CD2/01.flac"), "Three", 1, false);
    if !made {
        eprintln!("no ffmpeg: skipped");
        return;
    }

    let mut chilpa = Chilpa::new();
    let network = Arc::new(Canned::default());
    chilpa.metadata_network(network.clone());
    chilpa.scan(&music);
    let album = album_titled(&mut chilpa, "A record");
    assert_ne!(album, 0, "the record was scanned");

    // a_record_with_nothing_present_does_not_open
    assert!(!chilpa.metadata_open(987_654, 0));
    assert!(!chilpa.metadata_view().error.is_empty());
    assert_eq!(chilpa.metadata_view().state, "closed");

    // It opens saying what its files say.
    assert!(chilpa.metadata_open(album, 0));
    until(&mut chilpa, "ready");
    let view = chilpa.metadata_view().clone();
    assert_eq!(field(&view, "title"), "A record");
    assert_eq!(field(&view, "album_artist"), "Someone", "worked out by the scan: the files name nobody");
    assert_eq!(field(&view, "date"), "1990-05-01");
    assert_eq!(view.tracks.len(), 3);
    assert!(!view.dirty, "nobody has changed anything");
    assert!(view.save.files_allowed);
    assert_eq!(view.layers.files, 3);

    // there_is_nothing_to_save
    assert!(!chilpa.metadata_save());
    assert!(!chilpa.metadata_view().error.is_empty());

    // saving_to_a_sidecar_leaves_every_file_byte_for_byte
    // a_record_in_two_folders_gets_a_sidecar_in_each
    let before = audio(&music);
    let first = view.tracks[0].id;
    assert!(chilpa.metadata_set_album("title", "A record, corrected"));
    assert!(chilpa.metadata_set_album("genres", "Blues; Soul"));
    assert!(chilpa.metadata_set_track(first, "title", "One, corrected"));
    assert!(chilpa.metadata_view().dirty);
    assert!(chilpa.metadata_save());
    until(&mut chilpa, "closed");
    let after = chilpa.metadata_view().clone();
    assert!(after.error.is_empty(), "{}", after.error);
    assert!(!after.notice.is_empty());
    assert_eq!(audio(&music), before, "not one byte of audio changed");
    assert!(music.join("A record/CD1/.chilpa.json").is_file());
    assert!(music.join("A record/CD2/.chilpa.json").is_file());

    // saving_to_a_sidecar_changes_what_the_library_says
    // a_record_renamed_stays_open_under_the_same_id
    assert_eq!(after.album_id, album);
    assert_eq!(album_titled(&mut chilpa, "A record, corrected"), album);
    assert_eq!(album_titled(&mut chilpa, "A record"), 0);
    let rows = chilpa.album_track_rows(album);
    assert_eq!(rows.len(), 3);
    assert!(rows.iter().any(|r| r.title == "One, corrected"));
    assert_eq!(chilpa.album_detail(album).map(|d| d.genre), Some("Blues".to_string()));

    // a_scan_after_a_save_changes_nothing
    let told: Vec<(String, String)> = rows.iter().map(|r| (r.title.clone(), r.artist.clone())).collect();
    // Written again with the bytes they had: the same files with another
    // date, so the scan reads every one of them and not just their rows.
    for (path, bytes) in &before {
        std::fs::write(path, bytes).unwrap();
    }
    chilpa.scan(&music);
    assert_eq!(album_titled(&mut chilpa, "A record, corrected"), album, "the scan finds the record the edit left");
    let again: Vec<(String, String)> = chilpa.album_track_rows(album).iter().map(|r| (r.title.clone(), r.artist.clone())).collect();
    assert_eq!(again, told);

    // a_save_is_refused_while_a_scan_runs
    assert!(chilpa.metadata_open(album, first));
    until(&mut chilpa, "ready");
    assert_eq!(chilpa.metadata_view().focus, 0, "it opens on the track it was asked for");
    assert!(chilpa.metadata_view().album.iter().any(|f| f.name == "title" && f.overridden));
    chilpa.metadata_set_track(first, "title", "While scanning");
    chilpa.start_scan(&music);
    assert!(!chilpa.metadata_save());
    assert!(!chilpa.metadata_view().error.is_empty());
    let limit = Instant::now() + Duration::from_secs(20);
    while chilpa.scan_status().running && Instant::now() < limit {
        std::thread::sleep(Duration::from_millis(10));
    }

    // saving_to_the_files_empties_the_sidecar_of_what_they_now_say
    assert!(chilpa.metadata_set_mode("files"));
    chilpa.metadata_set_track(first, "title", "One, in the file");
    assert!(chilpa.metadata_save());
    until(&mut chilpa, "closed");
    assert!(chilpa.metadata_view().error.is_empty(), "{}", chilpa.metadata_view().error);
    assert!(!music.join("A record/CD1/.chilpa.json").exists(), "the files say it all now");
    assert!(!music.join("A record/CD2/.chilpa.json").exists());
    let said = chilpa_tags::read(&music.join("A record/CD1/01.flac")).unwrap().tags;
    assert_eq!(said.title.as_deref(), Some("One, in the file"));
    assert_eq!(said.album.as_deref(), Some("A record, corrected"));
    assert_eq!(said.album_artist.as_deref(), Some("Someone"), "the artist the scan worked out is written down");
    assert_eq!(said.genres, ["Blues", "Soul"]);
    assert_eq!(said.date.as_deref(), Some("1990-05-01"), "the date nobody touched is whole");
    assert_eq!(said.track_number, Some(1));
    let second = chilpa_tags::read(&music.join("A record/CD1/02.flac")).unwrap().tags;
    assert_eq!(second.title.as_deref(), Some("Two"));
    assert_eq!(std::fs::read_dir(music.join("A record/CD1")).unwrap().count(), 2, "and nothing was left beside them");

    // And again the scan changes nothing, reading the files as they are now.
    chilpa.scan(&music);
    assert_eq!(album_titled(&mut chilpa, "A record, corrected"), album);
    assert!(chilpa.album_track_rows(album).iter().any(|r| r.title == "One, in the file"));

    // closing_mid_search_drops_what_comes_back, a_second_search_drops_the_first
    network.replies.lock().unwrap().push_back((200, br#"{"releases":[{"id":"3d3207f9-c4c2-425c-a364-f359b4320805","title":"First answer","score":90}]}"#.to_vec()));
    network.replies.lock().unwrap().push_back((200, br#"{"releases":[{"id":"60a20bc8-acac-3cbb-99f9-ce458317233a","title":"Second answer","score":80}]}"#.to_vec()));
    assert!(chilpa.metadata_open(album, 0));
    until(&mut chilpa, "ready");
    assert!(chilpa.metadata_search());
    assert!(chilpa.metadata_search());
    until_idle(&mut chilpa);
    let hits = chilpa.metadata_view().lookup.hits.clone();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].title, "Second answer", "the first came back to nobody");

    // with_lookups_turned_off_nothing_goes_out
    let asked = network.asked.lock().unwrap().len();
    chilpa.remember_preference("library.metaSrc", "embedded");
    assert!(!chilpa.metadata_search());
    assert!(!chilpa.metadata_load(0));
    assert!(!chilpa.metadata_search_artwork());
    assert_eq!(network.asked.lock().unwrap().len(), asked, "not one request");
    chilpa.remember_preference("library.metaSrc", "musicbrainz");

    // a_cover_saved_to_a_sidecar_leaves_cover_jpg_alone
    std::fs::write(music.join("A record/CD1/cover.jpg"), b"the folder's own").unwrap();
    let picked = dir.join("picked.png");
    image_of(&picked);
    assert!(chilpa.metadata_set_mode("sidecar"));
    assert!(chilpa.metadata_artwork_from_file(&picked));
    until_idle(&mut chilpa);
    let cover = chilpa.metadata_view().cover.clone();
    assert!(cover.waiting, "{}", chilpa.metadata_view().error);
    assert_eq!((cover.width, cover.height, cover.from), (16, 16, "file"));
    assert!(chilpa.metadata_view().dirty, "a cover in wait is something to save");
    assert!(chilpa.metadata_save());
    until(&mut chilpa, "closed");
    assert!(chilpa.metadata_view().error.is_empty(), "{}", chilpa.metadata_view().error);
    assert_eq!(std::fs::read(music.join("A record/CD1/cover.jpg")).unwrap(), b"the folder's own");
    assert_eq!(std::fs::read(music.join("A record/CD1/.chilpa.cover.png")).unwrap(), std::fs::read(&picked).unwrap(), "byte for byte");
    let shown = chilpa.album(album).map(|a| a.artwork_path).unwrap_or_default();
    assert!(shown.ends_with(".chilpa.cover.png"), "{shown}");

    // closing_leaves_nothing_in_the_folder
    let kept = std::fs::read_dir(dir.join("cache/chilpa/editor")).map(|d| d.count()).unwrap_or(0);
    assert_eq!(kept, 0);

    let _ = std::fs::remove_dir_all(dir);
}

/// A 16 by 16 PNG, written where it is asked.
fn image_of(path: &Path) {
    image::RgbImage::from_fn(16, 16, |x, y| image::Rgb([(x * 16) as u8, (y * 16) as u8, 90])).save(path).unwrap();
}
