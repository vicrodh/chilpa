//! A box that names its discs and gives each its own cover shows them:
//! on its page in the library and when it is opened without being added.
//! A set with one cover for all, kept above its discs, shows that one on
//! the record and none on its discs.
use chilpa_api::{open_media, Chilpa, GroupHeader, TrackEntry};
use std::path::Path;
use std::process::Command;

fn tagged(target: &Path, album: &str, title: &str, disc: u32, track: u32) -> bool {
    let _ = std::fs::create_dir_all(target.parent().unwrap());
    Command::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-f", "lavfi", "-i", "sine=frequency=440:duration=1", "-c:a", "flac"])
        .args(["-metadata", &format!("title={title}"), "-metadata", "artist=Someone", "-metadata", "album_artist=Someone"])
        .args(["-metadata", &format!("album={album}"), "-metadata", "date=1990-05-01"])
        .args(["-metadata", &format!("disc={disc}"), "-metadata", &format!("track={track}")])
        .arg(target)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// A square PNG of that many pixels a side, written where it is asked.
/// Two covers of the same size in bytes count as one picture to the
/// scan, so each disc's differs in size.
fn image_of(path: &Path, side: u32) {
    image::RgbImage::from_fn(side, side, |x, y| image::Rgb([(x * 7) as u8, (y * 5) as u8, 90])).save(path).unwrap();
}

fn headers(rows: &[TrackEntry]) -> Vec<&GroupHeader> {
    rows.iter().filter_map(|row| if let TrackEntry::Header(header) = row { Some(header) } else { None }).collect()
}

fn album_titled(chilpa: &mut Chilpa, title: &str) -> i64 {
    chilpa.albums_sorted(chilpa_api::AlbumSort::Title, 100, 0).into_iter().find(|a| a.title == title).map(|a| a.id).unwrap_or(0)
}

#[test]
fn a_box_shows_each_disc_with_its_name_and_its_cover() {
    let dir = tempfile::tempdir().unwrap();
    for (key, folder) in [("XDG_DATA_HOME", "data"), ("XDG_CONFIG_HOME", "config"), ("XDG_CACHE_HOME", "cache")] {
        std::env::set_var(key, dir.path().join(folder));
    }
    let music = dir.path().join("music");
    let (overture, finale) = (music.join("Box/Disc 1 - Overture"), music.join("Box/Disc 2 - Finale"));
    let made = tagged(&overture.join("01.flac"), "Box", "One", 1, 1)
        && tagged(&overture.join("02.flac"), "Box", "Two", 1, 2)
        && tagged(&finale.join("01.flac"), "Box", "Three", 2, 1)
        // A set that only numbers its discs and keeps one cover above them.
        && tagged(&music.join("Plain/CD1/01.flac"), "Plain", "Four", 1, 1)
        && tagged(&music.join("Plain/CD2/01.flac"), "Plain", "Five", 2, 1);
    if !made {
        eprintln!("no ffmpeg: skipped");
        return;
    }
    image_of(&overture.join("cover.png"), 16);
    image_of(&finale.join("cover.png"), 24);
    image_of(&music.join("Plain/cover.png"), 20);

    let mut chilpa = Chilpa::new();
    chilpa.use_output("file:capture", false);
    chilpa.scan(&music);

    // Its page in the library: the divider names the disc and carries its cover.
    let album = album_titled(&mut chilpa, "Box");
    assert!(album > 0, "the two discs are one record");
    let rows = chilpa.album_track_entries(album);
    let said = headers(&rows);
    assert_eq!(said.iter().map(|h| (h.label.as_str(), h.meta.as_str())).collect::<Vec<_>>(),
        [("Disc 1 \u{2014} Overture", "2 songs"), ("Disc 2 \u{2014} Finale", "1 song")]);
    assert!(said[0].art.ends_with("Disc 1 - Overture/cover.png"), "{:?}", said[0].art);
    assert!(said[1].art.ends_with("Disc 2 - Finale/cover.png"), "{:?}", said[1].art);
    assert!(said.iter().all(|h| !h.record), "a divider, not a record of its own");

    // One cover for the whole set: the record has it, the discs repeat nothing.
    let plain = album_titled(&mut chilpa, "Plain");
    assert!(plain > 0);
    let cover = chilpa.albums_sorted(chilpa_api::AlbumSort::Title, 100, 0).into_iter().find(|a| a.id == plain).unwrap().artwork_path;
    assert!(cover.ends_with("Plain/cover.png"), "the set's cover, kept above its discs: {cover:?}");
    let rows = chilpa.album_track_entries(plain);
    assert_eq!(headers(&rows).iter().map(|h| (h.label.as_str(), h.art.as_str())).collect::<Vec<_>>(), [("Disc 1", ""), ("Disc 2", "")]);

    // Opened without being added: each disc is a block with its own title,
    // artist, year and cover.
    let media = open_media::read("folder", music.join("Box").to_str().unwrap()).unwrap();
    assert_eq!(media.title, "Box");
    chilpa.open_media(media);
    let (rows, keys) = chilpa.media_rows("");
    let said = headers(&rows);
    assert_eq!(said.iter().map(|h| (h.label.as_str(), h.artist.as_str(), h.meta.as_str())).collect::<Vec<_>>(), [
        ("Box \u{2014} Overture", "Someone", "1990 \u{b7} Disc 1 \u{b7} 2 tracks"),
        ("Box \u{2014} Finale", "Someone", "1990 \u{b7} Disc 2 \u{b7} 1 track"),
    ]);
    assert!(said[0].art.ends_with("Disc 1 - Overture/cover.png") && said[1].art.ends_with("Disc 2 - Finale/cover.png"));
    assert!(said.iter().all(|h| h.record));
    assert_eq!(keys.iter().filter(|key| key.is_empty()).count(), 2);

    // The set with one cover: its blocks fall back to it.
    let media = open_media::read("folder", music.join("Plain").to_str().unwrap()).unwrap();
    chilpa.open_media(media);
    let (rows, _) = chilpa.media_rows("");
    let said = headers(&rows);
    assert_eq!(said.iter().map(|h| h.label.as_str()).collect::<Vec<_>>(), ["Plain", "Plain"]);
    assert!(said.iter().all(|h| h.art.ends_with("Plain/cover.png")), "{:?}", said.iter().map(|h| &h.art).collect::<Vec<_>>());
}
