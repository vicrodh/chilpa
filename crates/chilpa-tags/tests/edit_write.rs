//! Writing tags into real files.
//!
//! The files are one second of a tone, made with ffmpeg, and an MP3 made by
//! hand: ffmpeg is not always built with an MP3 encoder, and the tags are
//! what is under test.

use chilpa_tags::edit::{inspect, placed, snapshot, stage, Change, Cover, FileChanges, Id3Version, Options, Refusal, WriteError};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Extension and ffmpeg's encoder.
const FORMATS: &[(&str, &str)] =
    &[("flac", "flac"), ("m4a", "aac"), ("ogg", "libvorbis"), ("wav", "pcm_s16le"), ("aiff", "pcm_s16be")];

const TAGS: &[(&str, &str)] = &[
    ("title", "A song"),
    ("artist", "Someone"),
    ("album", "A record"),
    ("date", "2019-03-15"),
    ("genre", "Rock"),
    ("track", "3"),
];

fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("chilpa-edit-write-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// One second of a tone in that format, carrying those tags. `None` when
/// ffmpeg is missing or cannot encode it.
fn tone(dir: &Path, name: &str, format: &(&str, &str), tags: &[(&str, &str)]) -> Option<PathBuf> {
    let target = dir.join(format!("{name}.{}", format.0));
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-y", "-loglevel", "error", "-f", "lavfi", "-i", "sine=frequency=440:duration=1", "-c:a", format.1]);
    for (key, value) in tags {
        cmd.arg("-metadata").arg(format!("{key}={value}"));
    }
    if format.0 == "aiff" {
        cmd.args(["-write_id3v2", "1"]);
    }
    cmd.arg(&target);
    cmd.output().ok()?.status.success().then_some(target)
}

/// Forty frames of silence, 128 kbps at 44.1 kHz: an MP3 with no tag.
fn mp3(dir: &Path, name: &str) -> PathBuf {
    const FRAME: usize = 417;
    let mut bytes = Vec::with_capacity(FRAME * 40);
    for _ in 0..40 {
        let mut frame = vec![0u8; FRAME];
        frame[..4].copy_from_slice(&[0xFF, 0xFB, 0x90, 0x00]);
        bytes.extend_from_slice(&frame);
    }
    let target = dir.join(format!("{name}.mp3"));
    std::fs::write(&target, bytes).unwrap();
    target
}

/// Every format that could be made, each as a file with `TAGS`.
fn every_format(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = FORMATS.iter().filter_map(|f| tone(dir, f.0, f, TAGS)).collect();
    let made = mp3(dir, "mp3");
    // The MP3 gets its tags the way any file does: by being written.
    let mut seed = FileChanges::none(made.clone());
    seed.title = Change::Set("A song".into());
    seed.artist_credit = Change::Set("Someone".into());
    seed.album = Change::Set("A record".into());
    seed.date = Change::Set("2019-03-15".into());
    seed.genres = Change::Set(vec!["Rock".into()]);
    seed.track_number = Change::Set(3);
    write(&[seed]).expect("the MP3 made by hand takes tags");
    out.push(made);
    out
}

fn write(files: &[FileChanges]) -> Result<Vec<PathBuf>, WriteError> {
    stage(files, Options::default(), &mut |_, _| {})?.commit()
}

fn state(path: &Path) -> (u64, std::time::SystemTime) {
    let meta = std::fs::metadata(path).unwrap();
    (meta.len(), meta.modified().unwrap())
}

/// The copies a write leaves waiting in a folder.
fn copies(dir: &Path) -> usize {
    std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".chilpa-tmp"))
        .count()
}

/// A cover: one pixel, PNG. 70 bytes.
const FRONT: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0xDA, 0x63, 0x38, 0x21, 0xA7, 0xF1,
    0x1F, 0x00, 0x04, 0xCE, 0x02, 0x0E, 0x62, 0xCF, 0x94, 0xA5, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

/// Another picture, to be there before the cover: two by two, PNG. 74 bytes.
const BACK: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x02, 0x08, 0x06, 0x00, 0x00, 0x00, 0x72, 0xB6, 0x0D,
    0x24, 0x00, 0x00, 0x00, 0x11, 0x49, 0x44, 0x41, 0x54, 0x78, 0xDA, 0x63, 0xE0, 0x12, 0x91, 0xFB,
    0x0F, 0xC2, 0x0C, 0x30, 0x06, 0x00, 0x26, 0x8C, 0x04, 0xED, 0xDF, 0x27, 0xD8, 0x9D, 0x00, 0x00,
    0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

fn cover() -> Cover {
    Cover { bytes: FRONT.to_vec(), mime: "image/png" }
}

#[test]
fn a_field_left_alone_is_not_rewritten() {
    let dir = folder("alone");
    for path in every_format(&dir) {
        let before = snapshot(&path).unwrap();
        let mut changes = FileChanges::none(path.clone());
        changes.title = Change::Set("Another title".into());
        write(&[changes]).unwrap_or_else(|e| panic!("{}: {e:?}", path.display()));

        let after = snapshot(&path).unwrap();
        assert_eq!(after.title.as_deref(), Some("Another title"), "{}", path.display());
        assert_eq!(after.date, before.date, "{}: the date", path.display());
        assert_eq!(after.genres, before.genres, "{}: the genres", path.display());
        assert_eq!(after.artist_credit, before.artist_credit, "{}: the artist", path.display());
        assert_eq!(after.album, before.album, "{}: the album", path.display());
        assert_eq!(after.track_number, before.track_number, "{}: the number", path.display());
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_full_date_survives_a_save_that_did_not_touch_it() {
    let dir = folder("date-kept");
    for path in every_format(&dir) {
        let mut changes = FileChanges::none(path.clone());
        changes.genres = Change::Set(vec!["Blues".into()]);
        write(&[changes]).unwrap();
        assert_eq!(snapshot(&path).unwrap().date.as_deref(), Some("2019-03-15"), "{}", path.display());
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_full_date_is_written_whole() {
    let dir = folder("date-written");
    for path in every_format(&dir) {
        let mut changes = FileChanges::none(path.clone());
        changes.date = Change::Set("1984-09-03".into());
        write(&[changes]).unwrap();
        assert_eq!(snapshot(&path).unwrap().date.as_deref(), Some("1984-09-03"), "{}", path.display());
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn three_genres_written_are_three_genres_read() {
    let dir = folder("genres");
    let genres: Vec<String> = vec!["Rock".into(), "Blues".into(), "Soul".into()];
    for path in every_format(&dir) {
        let mut changes = FileChanges::none(path.clone());
        changes.genres = Change::Set(genres.clone());
        write(&[changes]).unwrap_or_else(|e| panic!("{}: {e:?}", path.display()));
        assert_eq!(snapshot(&path).unwrap().genres, genres, "{}", path.display());
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn three_genres_in_id3v2_3_either_come_back_or_nothing_is_replaced() {
    // What lofty does with several values in 2.3 is not something I have
    // seen. Either answer is safe; this says which one it is.
    let dir = folder("genres-23");
    let path = mp3(&dir, "a");
    let before = state(&path);
    let genres: Vec<String> = vec!["Rock".into(), "Blues".into(), "Soul".into()];
    let mut changes = FileChanges::none(path.clone());
    changes.genres = Change::Set(genres.clone());
    let options = Options { id3: Id3Version::V23, secondary_layers: false };
    match stage(&[changes], options, &mut |_, _| {}) {
        Ok(staged) => {
            staged.commit().unwrap();
            assert_eq!(snapshot(&path).unwrap().genres, genres);
        }
        Err(WriteError::Verify { field, .. }) => {
            assert_eq!(field, "genres");
            assert_eq!(state(&path), before, "and the original is as it was");
            assert_eq!(copies(&dir), 0);
        }
        Err(other) => panic!("{other:?}"),
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn clearing_a_number_removes_it() {
    let dir = folder("numbers");
    for path in every_format(&dir) {
        let mut changes = FileChanges::none(path.clone());
        changes.track_number = Change::Clear;
        changes.disc_number = Change::Set(2);
        write(&[changes]).unwrap();
        let said = snapshot(&path).unwrap();
        assert_eq!(said.track_number, None, "{}", path.display());
        assert_eq!(said.disc_number, Some(2), "{}", path.display());
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_artist_credit_ends_as_one_value() {
    let dir = folder("credit");
    let Some(path) = tone(&dir, "a", &FORMATS[0], TAGS) else {
        eprintln!("no ffmpeg: skipped");
        return;
    };
    // Two values first, the way some taggers write a duet.
    let mut two = FileChanges::none(path.clone());
    two.artists = Change::Set(vec!["Alpha".into(), "Beta".into()]);
    write(&[two]).unwrap();
    assert_eq!(snapshot(&path).unwrap().artists, ["Alpha", "Beta"], "they stay two");

    let mut credit = FileChanges::none(path.clone());
    credit.artist_credit = Change::Set("Alpha feat. Beta".into());
    write(&[credit]).unwrap();
    let said = snapshot(&path).unwrap();
    assert_eq!(said.artist_credit, ["Alpha feat. Beta"]);
    assert_eq!(said.artists, ["Alpha", "Beta"], "and the names were not touched");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn changing_the_album_artist_leaves_every_track_artist_alone() {
    let dir = folder("album-artist");
    for path in every_format(&dir) {
        let mut changes = FileChanges::none(path.clone());
        changes.album_artist = Change::Set("Various Artists".into());
        write(&[changes]).unwrap();
        let said = snapshot(&path).unwrap();
        assert_eq!(said.album_artist.as_deref(), Some("Various Artists"), "{}", path.display());
        assert_eq!(said.artist_credit, ["Someone"], "{}", path.display());
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_new_cover_is_the_first_picture() {
    use lofty::config::WriteOptions;
    use lofty::file::{AudioFile, TaggedFileExt};
    use lofty::picture::{MimeType, Picture, PictureType};

    let dir = folder("cover");
    let Some(path) = tone(&dir, "a", &FORMATS[0], TAGS) else {
        eprintln!("no ffmpeg: skipped");
        return;
    };
    // A picture of another kind is there first.
    {
        let mut file = lofty::read_from_path(&path).unwrap();
        let tag = file.primary_tag_mut().unwrap();
        tag.push_picture(Picture::unchecked(BACK.to_vec()).pic_type(PictureType::Other).mime_type(MimeType::Png).build());
        file.save_to_path(&path, WriteOptions::new()).unwrap();
    }

    let mut changes = FileChanges::none(path.clone());
    changes.cover = Change::Set(cover());
    write(&[changes]).unwrap();

    let file = lofty::read_from_path(&path).unwrap();
    let pictures = file.primary_tag().unwrap().pictures();
    assert_eq!(pictures.len(), 2);
    assert_eq!(pictures[0].data(), FRONT, "the new one goes first");
    assert_eq!(pictures[0].pic_type(), PictureType::CoverFront);
    assert_eq!(pictures[1].data(), BACK, "and the other is kept");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn nothing_is_replaced_until_commit() {
    let dir = folder("until-commit");
    let Some(path) = tone(&dir, "a", &FORMATS[0], TAGS) else {
        eprintln!("no ffmpeg: skipped");
        return;
    };
    let before = state(&path);
    let mut changes = FileChanges::none(path.clone());
    changes.title = Change::Set("Another".into());
    let staged = stage(&[changes], Options::default(), &mut |_, _| {}).unwrap();
    assert_eq!(state(&path), before, "the original is as it was");
    assert_eq!(copies(&dir), 1, "and its copy waits beside it");
    drop(staged);
    assert_eq!(copies(&dir), 0, "dropping what was staged removes the copies");
    assert_eq!(snapshot(&path).unwrap().title.as_deref(), Some("A song"));
    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(target_family = "unix")]
#[test]
fn a_failure_in_the_third_file_leaves_no_copy_behind() {
    use std::os::unix::fs::PermissionsExt;
    let dir = folder("third");
    let made: Vec<PathBuf> = ["a", "b", "c"].iter().filter_map(|n| tone(&dir, n, &FORMATS[0], TAGS)).collect();
    if made.len() != 3 {
        eprintln!("no ffmpeg: skipped");
        return;
    }
    std::fs::set_permissions(&made[2], std::fs::Permissions::from_mode(0o444)).unwrap();
    if std::fs::OpenOptions::new().write(true).open(&made[2]).is_ok() {
        eprintln!("running as someone who can write anywhere: skipped");
        return;
    }
    let before: Vec<_> = made.iter().map(|p| state(p)).collect();
    let files: Vec<FileChanges> = made
        .iter()
        .map(|p| {
            let mut c = FileChanges::none(p.clone());
            c.title = Change::Set("Another".into());
            c
        })
        .collect();
    let result = stage(&files, Options::default(), &mut |_, _| {});
    assert!(matches!(result, Err(WriteError::Refused { why: Refusal::ReadOnly, .. })));
    assert_eq!(made.iter().map(|p| state(p)).collect::<Vec<_>>(), before, "no original was touched");
    assert_eq!(copies(&dir), 0);
    std::fs::set_permissions(&made[2], std::fs::Permissions::from_mode(0o644)).unwrap();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn commit_replaces_every_original() {
    let dir = folder("commit");
    let made: Vec<PathBuf> = ["a", "b"].iter().filter_map(|n| tone(&dir, n, &FORMATS[0], TAGS)).collect();
    if made.len() != 2 {
        eprintln!("no ffmpeg: skipped");
        return;
    }
    let files: Vec<FileChanges> = made
        .iter()
        .map(|p| {
            let mut c = FileChanges::none(p.clone());
            c.album = Change::Set("Another record".into());
            c
        })
        .collect();
    let mut seen = Vec::new();
    let replaced = stage(&files, Options::default(), &mut |done, total| seen.push((done, total))).unwrap().commit().unwrap();
    assert_eq!(replaced, made);
    assert_eq!(seen, [(1, 2), (2, 2)]);
    for path in &made {
        assert_eq!(snapshot(path).unwrap().album.as_deref(), Some("Another record"));
    }
    assert_eq!(copies(&dir), 0);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_audio_is_the_same_after_a_write() {
    let dir = folder("audio");
    for path in every_format(&dir) {
        let before = chilpa_tags::read(&path).unwrap();
        let mut changes = FileChanges::none(path.clone());
        changes.title = Change::Set("A much longer title than the one that was there before".into());
        changes.cover = Change::Set(cover());
        write(&[changes]).unwrap_or_else(|e| panic!("{}: {e:?}", path.display()));
        let after = chilpa_tags::read(&path).unwrap();
        assert_eq!(after.sample_rate_hz, before.sample_rate_hz, "{}", path.display());
        assert_eq!(after.bit_depth, before.bit_depth, "{}", path.display());
        assert_eq!(after.channels, before.channels, "{}", path.display());
        assert!(after.duration.as_millis().abs_diff(before.duration.as_millis()) <= 100, "{}", path.display());
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_file_read_through_another_tag_keeps_what_was_not_changed() {
    // A WAV from ffmpeg carries its tags as RIFF INFO, and its canonical tag
    // is ID3v2. Writing one field makes the canonical tag exist; what the
    // other tag said has to be in it.
    let dir = folder("other-tag");
    let Some(path) = tone(&dir, "a", &("wav", "pcm_s16le"), TAGS) else {
        eprintln!("no ffmpeg: skipped");
        return;
    };
    let before = snapshot(&path).unwrap();
    assert_eq!(before.title.as_deref(), Some("A song"), "it is read, through whichever tag");
    let mut changes = FileChanges::none(path.clone());
    changes.album = Change::Set("Another record".into());
    write(&[changes]).unwrap();
    let after = snapshot(&path).unwrap();
    assert_eq!(after.album.as_deref(), Some("Another record"));
    assert_eq!(after.title, before.title, "what was not changed is still said");
    assert_eq!(after.artist_credit, before.artist_credit);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn id3v1_is_never_written() {
    use lofty::file::TaggedFileExt;
    use lofty::tag::{Accessor, TagType};

    let dir = folder("id3v1");
    let path = mp3(&dir, "a");
    // An ID3v1 tag at the end: 128 bytes that start with "TAG", then the
    // title in thirty.
    let mut v1 = vec![0u8; 128];
    v1[..3].copy_from_slice(b"TAG");
    v1[3..10].copy_from_slice(b"Old one");
    let mut bytes = std::fs::read(&path).unwrap();
    bytes.extend_from_slice(&v1);
    std::fs::write(&path, bytes).unwrap();

    let mut changes = FileChanges::none(path.clone());
    changes.title = Change::Set("New one".into());
    let options = Options { id3: Id3Version::V24, secondary_layers: true };
    stage(&[changes], options, &mut |_, _| {}).unwrap().commit().unwrap();

    let file = lofty::read_from_path(&path).unwrap();
    let old = file.tag(TagType::Id3v1).expect("the old tag is still there");
    assert_eq!(old.title().as_deref(), Some("Old one"), "and says what it said");
    assert_eq!(snapshot(&path).unwrap().title.as_deref(), Some("New one"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_dsf_is_refused_before_anything_is_copied() {
    let dir = folder("dsf");
    let path = dir.join("a.dsf");
    std::fs::write(&path, b"not really a dsf").unwrap();
    let mut changes = FileChanges::none(path.clone());
    changes.title = Change::Set("Another".into());
    let result = stage(&[changes], Options::default(), &mut |_, _| {});
    assert!(matches!(result, Err(WriteError::Refused { why: Refusal::Dsd, .. })));
    assert_eq!(copies(&dir), 0);
    assert_eq!(inspect(&[path]).refusals, vec![Some(Refusal::Dsd)]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_same_file_twice_is_refused() {
    let dir = folder("twice");
    let path = mp3(&dir, "a");
    let mut changes = FileChanges::none(path);
    changes.title = Change::Set("Another".into());
    let result = stage(&[changes.clone(), changes], Options::default(), &mut |_, _| {});
    assert!(matches!(result, Err(WriteError::Write { .. })));
    assert_eq!(copies(&dir), 0);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn every_format_has_a_place_for_what_every_tag_carries() {
    let dir = folder("placed");
    for path in every_format(&dir) {
        let has = placed(&path).unwrap();
        for name in ["title", "artist_credit", "album", "date", "track_number", "disc_number", "cover", "genres", "album_artist"] {
            assert!(has.contains(&name), "{}: {name}", path.display());
        }
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_change_with_nothing_in_it_writes_nothing() {
    let dir = folder("nothing");
    let path = mp3(&dir, "a");
    let before = state(&path);
    let staged = stage(&[FileChanges::none(path.clone())], Options::default(), &mut |_, _| {}).unwrap();
    assert!(staged.is_empty());
    assert_eq!(staged.commit().unwrap(), Vec::<PathBuf>::new());
    assert_eq!(state(&path), before);
    let _ = std::fs::remove_dir_all(dir);
}

/// An MP3 with its canonical tag and an APE tag, both saying the same.
fn two_layers(dir: &Path) -> PathBuf {
    use lofty::config::WriteOptions;
    use lofty::file::{AudioFile, TaggedFileExt};
    use lofty::tag::{Accessor, Tag, TagType};

    let path = mp3(dir, "a");
    let mut file = lofty::read_from_path(&path).unwrap();
    for kind in [TagType::Id3v2, TagType::Ape] {
        let mut tag = Tag::new(kind);
        tag.set_title("Old one".to_string());
        tag.set_genre("Rock".to_string());
        file.insert_tag(tag);
    }
    file.save_to_path(&path, WriteOptions::new()).unwrap();
    path
}

#[test]
fn secondary_layers_take_only_what_changed() {
    use lofty::file::TaggedFileExt;
    use lofty::tag::{Accessor, TagType};

    let dir = folder("layers-on");
    let path = two_layers(&dir);
    let mut changes = FileChanges::none(path.clone());
    changes.title = Change::Set("New one".into());
    let options = Options { id3: Id3Version::V24, secondary_layers: true };
    stage(&[changes], options, &mut |_, _| {}).unwrap().commit().unwrap();

    let file = lofty::read_from_path(&path).unwrap();
    let canonical = file.tag(TagType::Id3v2).expect("the canonical tag");
    assert_eq!(canonical.title().as_deref(), Some("New one"));
    assert_eq!(canonical.genre().as_deref(), Some("Rock"));
    let other = file.tag(TagType::Ape).expect("the other layer is still there");
    assert_eq!(other.title().as_deref(), Some("New one"), "it took what changed");
    assert_eq!(other.genre().as_deref(), Some("Rock"), "and kept what did not");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn other_layers_are_left_alone_unless_asked() {
    use lofty::file::TaggedFileExt;
    use lofty::tag::{Accessor, TagType};

    let dir = folder("layers-off");
    let path = two_layers(&dir);
    let mut changes = FileChanges::none(path.clone());
    changes.title = Change::Set("New one".into());
    write(&[changes]).unwrap();

    let file = lofty::read_from_path(&path).unwrap();
    assert_eq!(file.tag(TagType::Id3v2).expect("the canonical tag").title().as_deref(), Some("New one"));
    let other = file.tag(TagType::Ape).expect("the other layer is still there");
    assert_eq!(other.title().as_deref(), Some("Old one"), "and says what it said");
    assert_eq!(inspect(&[path]).conflicting, 1, "which the editor is told");
    let _ = std::fs::remove_dir_all(dir);
}
