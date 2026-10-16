//! Writing what changed into a copy of each file, and moving the copies in.
//!
//! Nothing is written over an original. Each file is copied beside itself,
//! the copy is written and read back, and only when every copy of the
//! record says what it should are they moved over their originals. A
//! failure before that leaves every original as it was.

use super::inspect::{inspect, Refusal};
use super::read::{of_tag, Snapshot};
use lofty::config::WriteOptions;
use lofty::file::{AudioFile, TaggedFile, TaggedFileExt};
use lofty::id3::v2::Id3v2Tag;
use lofty::picture::{MimeType, Picture, PictureType};
use lofty::prelude::ItemKey;
use lofty::probe::Probe;
use lofty::tag::items::Timestamp;
use lofty::tag::{Accessor, ItemValue, Tag, TagExt, TagItem, TagType};
use std::path::{Path, PathBuf};

/// What to do with one field of one file.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Change<T> {
    Keep,
    Set(T),
    Clear,
}

impl<T> Change<T> {
    pub fn is_keep(&self) -> bool {
        matches!(self, Change::Keep)
    }
}

/// Which ID3 a file whose canonical tag is ID3v2 is written as.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Id3Version {
    #[default]
    V24,
    /// For players that do not read 2.4.
    V23,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Options {
    pub id3: Id3Version,
    /// Writes the same changes into the other writable layers a file already
    /// carries. ID3v1 is never one of them: thirty bytes of Latin-1 would
    /// cut a title short without saying so.
    pub secondary_layers: bool,
}

/// A front cover, already checked by whoever hands it in.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Cover {
    pub bytes: Vec<u8>,
    /// "image/jpeg" or "image/png".
    pub mime: &'static str,
}

/// Every change to one file. A field left at `Keep` is not touched.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FileChanges {
    pub path: PathBuf,
    pub title: Change<String>,
    pub artist_credit: Change<String>,
    pub artists: Change<Vec<String>>,
    pub album: Change<String>,
    pub album_artist: Change<String>,
    pub album_artists: Change<Vec<String>>,
    pub compilation: Change<bool>,
    pub date: Change<String>,
    pub genres: Change<Vec<String>>,
    pub catalog_number: Change<String>,
    pub label: Change<String>,
    pub track_number: Change<u32>,
    pub disc_number: Change<u32>,
    pub disc_subtitle: Change<String>,
    pub work: Change<String>,
    pub composers: Change<Vec<String>>,
    pub performers: Change<Vec<String>>,
    pub musicbrainz_release_id: Change<String>,
    pub musicbrainz_release_group_id: Change<String>,
    pub musicbrainz_album_artist_ids: Change<Vec<String>>,
    pub musicbrainz_recording_id: Change<String>,
    pub musicbrainz_track_id: Change<String>,
    pub musicbrainz_artist_ids: Change<Vec<String>>,
    pub cover: Change<Cover>,
}

/// The fields that hold one text, by the name the editor gives them and the
/// key they are written under. Title, album and artist are not here: they
/// go through the tag's own accessors.
const TEXTS: [(&str, ItemKey); 9] = [
    ("album_artist", ItemKey::AlbumArtist),
    ("catalog_number", ItemKey::CatalogNumber),
    ("label", ItemKey::Label),
    ("disc_subtitle", ItemKey::SetSubtitle),
    ("work", ItemKey::Work),
    ("musicbrainz_release_id", ItemKey::MusicBrainzReleaseId),
    ("musicbrainz_release_group_id", ItemKey::MusicBrainzReleaseGroupId),
    ("musicbrainz_recording_id", ItemKey::MusicBrainzRecordingId),
    ("musicbrainz_track_id", ItemKey::MusicBrainzTrackId),
];

/// The fields that hold several.
const LISTS: [(&str, ItemKey); 7] = [
    ("artists", ItemKey::TrackArtists),
    ("album_artists", ItemKey::AlbumArtists),
    ("genres", ItemKey::Genre),
    ("composers", ItemKey::Composer),
    ("performers", ItemKey::Performer),
    ("musicbrainz_album_artist_ids", ItemKey::MusicBrainzReleaseArtistId),
    ("musicbrainz_artist_ids", ItemKey::MusicBrainzArtistId),
];

/// The fields every tag that can be written has a place for.
const ALWAYS: [&str; 7] = ["title", "artist_credit", "album", "date", "track_number", "disc_number", "cover"];

impl FileChanges {
    /// Every field at `Keep`.
    pub fn none(path: PathBuf) -> Self {
        Self {
            path,
            title: Change::Keep,
            artist_credit: Change::Keep,
            artists: Change::Keep,
            album: Change::Keep,
            album_artist: Change::Keep,
            album_artists: Change::Keep,
            compilation: Change::Keep,
            date: Change::Keep,
            genres: Change::Keep,
            catalog_number: Change::Keep,
            label: Change::Keep,
            track_number: Change::Keep,
            disc_number: Change::Keep,
            disc_subtitle: Change::Keep,
            work: Change::Keep,
            composers: Change::Keep,
            performers: Change::Keep,
            musicbrainz_release_id: Change::Keep,
            musicbrainz_release_group_id: Change::Keep,
            musicbrainz_album_artist_ids: Change::Keep,
            musicbrainz_recording_id: Change::Keep,
            musicbrainz_track_id: Change::Keep,
            musicbrainz_artist_ids: Change::Keep,
            cover: Change::Keep,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.title.is_keep()
            && self.artist_credit.is_keep()
            && self.album.is_keep()
            && self.date.is_keep()
            && self.compilation.is_keep()
            && self.track_number.is_keep()
            && self.disc_number.is_keep()
            && self.cover.is_keep()
            && TEXTS.iter().all(|(name, _)| self.text(name).is_keep())
            && LISTS.iter().all(|(name, _)| self.list(name).is_keep())
    }

    /// The change to one of `TEXTS`.
    fn text(&self, name: &str) -> &Change<String> {
        match name {
            "album_artist" => &self.album_artist,
            "catalog_number" => &self.catalog_number,
            "label" => &self.label,
            "disc_subtitle" => &self.disc_subtitle,
            "work" => &self.work,
            "musicbrainz_release_id" => &self.musicbrainz_release_id,
            "musicbrainz_release_group_id" => &self.musicbrainz_release_group_id,
            "musicbrainz_recording_id" => &self.musicbrainz_recording_id,
            _ => &self.musicbrainz_track_id,
        }
    }

    /// The change to one of `LISTS`.
    fn list(&self, name: &str) -> &Change<Vec<String>> {
        match name {
            "artists" => &self.artists,
            "album_artists" => &self.album_artists,
            "genres" => &self.genres,
            "composers" => &self.composers,
            "performers" => &self.performers,
            "musicbrainz_album_artist_ids" => &self.musicbrainz_album_artist_ids,
            _ => &self.musicbrainz_artist_ids,
        }
    }
}

#[derive(Debug)]
pub enum WriteError {
    Refused { path: PathBuf, why: Refusal },
    Copy { path: PathBuf, error: std::io::Error },
    Write { path: PathBuf, reason: String },
    /// Written, read back, and it does not say what was asked.
    Verify { path: PathBuf, field: &'static str },
    /// The audio came out different: duration, rate, depth or channels.
    Audio { path: PathBuf },
    /// The canonical tag has no place for that field.
    NoPlace { path: PathBuf, field: &'static str },
    /// A copy could not be moved over its original. `replaced` are the
    /// originals that already were.
    Replace { path: PathBuf, error: std::io::Error, replaced: Vec<PathBuf> },
}

/// Copies written and checked, waiting beside their originals.
pub struct Staged {
    /// The copy, the file it replaces -- a link's target, not the link --
    /// and the path it was asked for by, in the order they came.
    pairs: Vec<(PathBuf, PathBuf, PathBuf)>,
}

impl Staged {
    pub fn len(&self) -> usize {
        self.pairs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }

    /// Moves every copy over its original, in the order given to `stage`.
    /// Returns the originals replaced.
    pub fn commit(mut self) -> Result<Vec<PathBuf>, WriteError> {
        let mut replaced = Vec::with_capacity(self.pairs.len());
        while !self.pairs.is_empty() {
            let (copy, target, original) = self.pairs.remove(0);
            if let Err(error) = std::fs::rename(&copy, &target) {
                let _ = std::fs::remove_file(&copy);
                // The copies still waiting go when `self` is dropped.
                return Err(WriteError::Replace { path: original, error, replaced });
            }
            replaced.push(original);
        }
        Ok(replaced)
    }
}

impl Drop for Staged {
    fn drop(&mut self) {
        for (copy, _, _) in &self.pairs {
            let _ = std::fs::remove_file(copy);
        }
    }
}

/// Where a file's copy waits: beside it, under a name that is not audio, so
/// a scan that finds one left behind does not take it for a track.
///
/// Built from the name as the system has it, not as text: two names that
/// are not valid UTF-8 would otherwise come out as the same copy.
fn copy_path(original: &Path) -> PathBuf {
    let mut name = std::ffi::OsString::from(".");
    name.push(original.file_name().unwrap_or_default());
    name.push(".chilpa-tmp");
    original.with_file_name(name)
}

/// A copy is named for what it is and not for what it holds, so its kind is
/// read from its bytes.
fn read_copy(path: &Path) -> Result<TaggedFile, String> {
    Probe::open(path)
        .map_err(|e| e.to_string())?
        .guess_file_type()
        .map_err(|e| e.to_string())?
        .read()
        .map_err(|e| e.to_string())
}

/// What the audio is, as far as a tag write could get it wrong.
fn sound(file: &TaggedFile) -> (u128, Option<u32>, Option<u8>, Option<u8>) {
    let p = file.properties();
    (p.duration().as_millis(), p.sample_rate(), p.bit_depth(), p.channels())
}

/// The same audio. The length is given a tenth of a second: where a format
/// has no count of samples it is worked out from the file's size, which a
/// tag changes.
fn same_sound(a: (u128, Option<u32>, Option<u8>, Option<u8>), b: (u128, Option<u32>, Option<u8>, Option<u8>)) -> bool {
    a.0.abs_diff(b.0) <= 100 && a.1 == b.1 && a.2 == b.2 && a.3 == b.3
}

/// A date as the editor hands it in: `1984`, `1984-09`, `1984-09-03`.
fn timestamp(date: &str) -> Option<Timestamp> {
    let mut parts = date.split('-');
    let year: u16 = parts.next()?.parse().ok()?;
    let month = match parts.next() {
        Some(m) => Some(m.parse::<u8>().ok()?),
        None => None,
    };
    let day = match parts.next() {
        Some(d) => Some(d.parse::<u8>().ok()?),
        None => None,
    };
    if parts.next().is_some() {
        return None;
    }
    Some(Timestamp { year, month, day, ..Timestamp::default() })
}

/// The new front cover goes first. A reader that shows the first picture
/// it finds, and there are many, shows this one.
fn put_cover(tag: &mut Tag, cover: &Cover) {
    let others: Vec<Picture> =
        tag.pictures().iter().filter(|p| p.pic_type() != PictureType::CoverFront).cloned().collect();
    while tag.picture_count() > 0 {
        tag.remove_picture(0);
    }
    let kind = if cover.mime == "image/png" { MimeType::Png } else { MimeType::Jpeg };
    tag.push_picture(
        Picture::unchecked(cover.bytes.clone()).pic_type(PictureType::CoverFront).mime_type(kind).build(),
    );
    for other in others {
        tag.push_picture(other);
    }
}

/// Lays the changes over one tag.
///
/// `strict` is for the canonical tag: a field it has no place for stops the
/// write and is named. Another layer takes what it can and skips the rest.
fn apply(tag: &mut Tag, changes: &FileChanges, strict: bool) -> Result<(), &'static str> {
    match &changes.title {
        Change::Keep => {}
        Change::Set(v) => tag.set_title(v.clone()),
        Change::Clear => tag.remove_title(),
    }
    match &changes.album {
        Change::Keep => {}
        Change::Set(v) => tag.set_album(v.clone()),
        Change::Clear => tag.remove_album(),
    }
    match &changes.artist_credit {
        Change::Keep => {}
        // One value: the credit as it is shown. The names that make it up
        // are `artists`.
        Change::Set(v) => {
            tag.remove_key(ItemKey::TrackArtist);
            tag.set_artist(v.clone());
        }
        Change::Clear => tag.remove_key(ItemKey::TrackArtist),
    }
    for (name, key) in TEXTS {
        match changes.text(name) {
            Change::Keep => {}
            Change::Set(v) => {
                if !tag.insert_text(key, v.clone()) && strict {
                    return Err(name);
                }
            }
            Change::Clear => tag.remove_key(key),
        }
    }
    for (name, key) in LISTS {
        match changes.list(name) {
            Change::Keep => {}
            Change::Set(values) => {
                tag.remove_key(key);
                for v in values {
                    if !tag.push(TagItem::new(key, ItemValue::Text(v.clone()))) && strict {
                        return Err(name);
                    }
                }
            }
            Change::Clear => tag.remove_key(key),
        }
    }
    match &changes.compilation {
        Change::Keep => {}
        Change::Set(true) => {
            if !tag.insert_text(ItemKey::FlagCompilation, "1".to_string()) && strict {
                return Err("compilation");
            }
        }
        // A record that is not a compilation says nothing about it.
        Change::Set(false) | Change::Clear => tag.remove_key(ItemKey::FlagCompilation),
    }
    match &changes.date {
        Change::Keep => {}
        Change::Set(v) => match timestamp(v) {
            Some(stamp) => tag.set_date(stamp),
            None if strict => return Err("date"),
            None => {}
        },
        Change::Clear => tag.remove_date(),
    }
    match changes.track_number {
        Change::Keep => {}
        Change::Set(n) => tag.set_track(n),
        Change::Clear => tag.remove_track(),
    }
    match changes.disc_number {
        Change::Keep => {}
        Change::Set(n) => tag.set_disk(n),
        Change::Clear => tag.remove_disk(),
    }
    match &changes.cover {
        Change::Keep => {}
        Change::Set(cover) => put_cover(tag, cover),
        Change::Clear => tag.remove_picture_type(PictureType::CoverFront),
    }
    Ok(())
}

/// What one of `TEXTS` reads as.
fn said_text<'a>(said: &'a Snapshot, name: &str) -> Option<&'a str> {
    match name {
        "album_artist" => said.album_artist.as_deref(),
        "catalog_number" => said.catalog_number.as_deref(),
        "label" => said.label.as_deref(),
        "disc_subtitle" => said.disc_subtitle.as_deref(),
        "work" => said.work.as_deref(),
        "musicbrainz_release_id" => said.musicbrainz_release_id.as_deref(),
        "musicbrainz_release_group_id" => said.musicbrainz_release_group_id.as_deref(),
        "musicbrainz_recording_id" => said.musicbrainz_recording_id.as_deref(),
        _ => said.musicbrainz_track_id.as_deref(),
    }
}

/// What one of `LISTS` reads as.
fn said_list<'a>(said: &'a Snapshot, name: &str) -> &'a [String] {
    match name {
        "artists" => &said.artists,
        "album_artists" => &said.album_artists,
        "genres" => &said.genres,
        "composers" => &said.composers,
        "performers" => &said.performers,
        "musicbrainz_album_artist_ids" => &said.musicbrainz_album_artist_ids,
        _ => &said.musicbrainz_artist_ids,
    }
}

/// What the copy says now against what it was told to say. Only the fields
/// that were changed are looked at. `Err` names the first that differs.
fn verify(said: &Snapshot, first_picture: Option<&[u8]>, changes: &FileChanges) -> Result<(), &'static str> {
    let text = |change: &Change<String>, now: Option<&str>| match change {
        Change::Keep => true,
        Change::Set(v) => now == Some(v.as_str()),
        Change::Clear => now.is_none(),
    };
    let number = |change: Change<u32>, now: Option<u32>| match change {
        Change::Keep => true,
        Change::Set(n) => now == Some(n),
        Change::Clear => now.is_none(),
    };
    if !text(&changes.title, said.title.as_deref()) {
        return Err("title");
    }
    if !text(&changes.album, said.album.as_deref()) {
        return Err("album");
    }
    let credit = match &changes.artist_credit {
        Change::Keep => true,
        Change::Set(v) => said.artist_credit.len() == 1 && said.artist_credit[0] == *v,
        Change::Clear => said.artist_credit.is_empty(),
    };
    if !credit {
        return Err("artist_credit");
    }
    for (name, _) in TEXTS {
        if !text(changes.text(name), said_text(said, name)) {
            return Err(name);
        }
    }
    for (name, _) in LISTS {
        let same = match changes.list(name) {
            Change::Keep => true,
            Change::Set(values) => said_list(said, name) == values.as_slice(),
            Change::Clear => said_list(said, name).is_empty(),
        };
        if !same {
            return Err(name);
        }
    }
    let compilation = match changes.compilation {
        Change::Keep => true,
        Change::Set(true) => said.compilation == Some(true),
        Change::Set(false) | Change::Clear => said.compilation.is_none(),
    };
    if !compilation {
        return Err("compilation");
    }
    if !text(&changes.date, said.date.as_deref()) {
        return Err("date");
    }
    if !number(changes.track_number.clone(), said.track_number) {
        return Err("track_number");
    }
    if !number(changes.disc_number.clone(), said.disc_number) {
        return Err("disc_number");
    }
    let cover = match &changes.cover {
        Change::Keep => true,
        Change::Set(c) => first_picture == Some(c.bytes.as_slice()),
        Change::Clear => !said.front_cover,
    };
    if !cover {
        return Err("cover");
    }
    Ok(())
}

/// One file: copied, written, read back.
fn one(changes: &FileChanges, copy: &Path, options: Options) -> Result<(), WriteError> {
    let path = &changes.path;
    let failed = |reason: String| WriteError::Write { path: path.clone(), reason };

    let _ = std::fs::remove_file(copy);
    std::fs::copy(path, copy).map_err(|error| WriteError::Copy { path: path.clone(), error })?;

    let audio = {
        let before = lofty::read_from_path(path).map_err(|e| failed(e.to_string()))?;
        sound(&before)
    };

    let mut file = read_copy(copy).map_err(&failed)?;
    let primary = file.primary_tag_type();
    if file.tag(primary).is_none() {
        // A file whose only tag is of another kind is read through that one.
        // The canonical tag starts as its copy, or what was not changed
        // would stop being said the moment the canonical one exists.
        let mut born = file.first_tag().cloned().unwrap_or_else(|| Tag::new(primary));
        born.re_map(primary);
        file.insert_tag(born);
    }
    let others: Vec<TagType> = if options.secondary_layers {
        file.tags()
            .iter()
            .map(Tag::tag_type)
            .filter(|kind| *kind != primary && *kind != TagType::Id3v1 && file.tag_support(*kind).is_writable())
            .collect()
    } else {
        Vec::new()
    };
    match file.tag_mut(primary) {
        Some(tag) => apply(tag, changes, true).map_err(|field| WriteError::NoPlace { path: path.clone(), field })?,
        None => return Err(failed("the canonical tag could not be made".to_string())),
    }
    // Lofty 0.25 maps WORK to ID3's TXXX:WORK on read, but its generic
    // writer classifies it as a four-character text frame and refuses the
    // whole MP3. Let the generic writer handle the other fields, then put
    // this one back through Lofty's native ID3 frame writer.
    let (id3_work, id3_subtitle) = if primary == TagType::Id3v2 {
        file.tag_mut(primary).and_then(|tag| {
            let work = tag.get_strings(ItemKey::Work).next().map(str::to_string);
            let subtitle = tag.get_strings(ItemKey::SetSubtitle).next().map(str::to_string);
            tag.remove_key(ItemKey::Work);
            Some((work, subtitle))
        }).unwrap_or_default()
    } else { (None, None) };
    for kind in others {
        if let Some(tag) = file.tag_mut(kind) {
            let _ = apply(tag, changes, false);
        }
    }

    let mut write = WriteOptions::new();
    write.use_id3v23(options.id3 == Id3Version::V23);
    // A title that does not fit the encoding fails the write. It is never
    // turned into question marks.
    write.lossy_text_encoding(false);
    file.save_to_path(copy, write).map_err(|e| failed(e.to_string()))?;
    drop(file);
    if primary == TagType::Id3v2 && (id3_work.is_some() || !changes.work.is_keep()
        || options.id3 == Id3Version::V23 && (id3_subtitle.is_some() || !changes.disc_subtitle.is_keep())) {
        let fresh = read_copy(copy).map_err(&failed)?;
        let mut id3 = fresh.tag(primary).cloned().map(Id3v2Tag::from).unwrap_or_default();
        id3.remove_user_text("WORK");
        if let Some(work) = id3_work { id3.insert_user_text("WORK".to_string(), work); }
        if options.id3 == Id3Version::V23 {
            // TSST is a v2.4 frame. In v2.3 a user text frame named TSST
            // keeps the subtitle legal and maps back to SetSubtitle.
            id3.remove_user_text("TSST");
            if let Some(subtitle) = id3_subtitle { id3.insert_user_text("TSST".to_string(), subtitle); }
        }
        let mut write = WriteOptions::new();
        write.use_id3v23(options.id3 == Id3Version::V23);
        write.lossy_text_encoding(false);
        id3.save_to_path(copy, write).map_err(|e| failed(e.to_string()))?;
    }

    let after = read_copy(copy).map_err(&failed)?;
    if !same_sound(sound(&after), audio) {
        return Err(WriteError::Audio { path: path.clone() });
    }
    let tag = after.tag(after.primary_tag_type());
    let said = tag.map(of_tag).unwrap_or_default();
    let picture = tag.and_then(|t| t.pictures().first()).map(Picture::data);
    verify(&said, picture, changes).map_err(|field| WriteError::Verify { path: path.clone(), field })?;

    // On disk before it is moved over the original: a copy still in a cache
    // when the power goes is an original replaced by half a file.
    // Opened for writing: flushing a file needs that on Windows.
    std::fs::OpenOptions::new().write(true).open(copy).and_then(|f| f.sync_all()).map_err(|e| failed(e.to_string()))
}

/// Writes every file's changes into a copy beside it, and checks each copy.
///
/// Nothing is replaced: that is `Staged::commit`. On any error every copy
/// made so far is removed, and no original was touched.
pub fn stage(
    files: &[FileChanges],
    options: Options,
    on_progress: &mut dyn FnMut(usize, usize),
) -> Result<Staged, WriteError> {
    let files: Vec<&FileChanges> = files.iter().filter(|f| !f.is_empty()).collect();
    // A link is written through: the copy goes beside the file it points to
    // and replaces that file, and the link stays a link. Two paths to one
    // file are the same file twice.
    let targets: Vec<PathBuf> = files.iter().map(|f| std::fs::canonicalize(&f.path).unwrap_or_else(|_| f.path.clone())).collect();
    for (i, file) in files.iter().enumerate() {
        if targets[..i].contains(&targets[i]) {
            return Err(WriteError::Write { path: file.path.clone(), reason: "the same file twice".to_string() });
        }
    }
    // Every file is asked before the first is copied: what can be known
    // beforehand is not found out halfway.
    let paths: Vec<PathBuf> = files.iter().map(|f| f.path.clone()).collect();
    let layers = inspect(&paths);
    for (path, refusal) in paths.iter().zip(&layers.refusals) {
        if let Some(why) = refusal {
            return Err(WriteError::Refused { path: path.clone(), why: *why });
        }
    }

    let total = files.len();
    let mut staged = Staged { pairs: Vec::with_capacity(total) };
    for (done, (changes, target)) in files.iter().zip(targets).enumerate() {
        let copy = copy_path(&target);
        // `staged` owns the copy from here: an error below drops it, and
        // dropping it removes every copy it holds.
        staged.pairs.push((copy.clone(), target, changes.path.clone()));
        one(changes, &copy, options)?;
        on_progress(done + 1, total);
    }
    Ok(staged)
}

/// The fields a file's canonical tag has a place for, by the names the
/// editor gives them. What is not here goes to the sidecar.
pub fn placed(path: &Path) -> Result<Vec<&'static str>, WriteError> {
    let file = lofty::read_from_path(path)
        .map_err(|e| WriteError::Write { path: path.to_path_buf(), reason: e.to_string() })?;
    let kind = file.primary_tag_type();
    let mut out: Vec<&'static str> = ALWAYS.to_vec();
    for (name, key) in TEXTS.into_iter().chain(LISTS) {
        if key.map_key(kind).is_some() {
            out.push(name);
        }
    }
    if ItemKey::FlagCompilation.map_key(kind).is_some() {
        out.push("compilation");
    }
    Ok(out)
}
