//! The discs of a record: what each is called and which cover is its own.
//!
//! A box set is one record, so its discs share its title and its cover,
//! and the only thing that told disc 1 from disc 2 was the number. A box
//! that names its discs and gives each a cover says so in its folders and
//! its files; this is where that is read, for a record's page and for
//! media opened without being added.

use crate::browse::GroupHeader;
use crate::{TrackEntry, TrackRow};
use chilpa_library::album_folder::{album_folder, disc_title, without_disc};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// The file a track is in: itself, or the disc image it is a track of.
fn file_of(path: &str) -> &Path {
    Path::new(path.strip_prefix("sacd:").map_or(path, |image| image.rsplit_once('#').map_or(image, |(file, _)| file)))
}

/// The folder a track's file sits in.
fn folder_of(path: &str) -> PathBuf {
    file_of(path).parent().unwrap_or(Path::new("")).to_owned()
}

/// The folder of the record a track belongs to: its own, or the one above
/// when its own is one disc of a set.
pub(crate) fn record_folder(path: &str) -> String {
    album_folder(file_of(path))
}

/// What a disc is called besides its number, most trustworthy first: the
/// name its folder gives it ("Disc 2 - Live"), which a tag cannot
/// contradict; the album its tags name, when that says something the
/// record's title does not ("Box (Disc 2)" only numbers it); nothing.
pub(crate) fn named(path: &str, tagged: &str, record: &str) -> String {
    if let Some(title) = disc_title(&folder_of(path)) {
        return title;
    }
    let (tag, record) = (without_disc(tagged), without_disc(record));
    if tag.is_empty() || tag.to_lowercase() == record.to_lowercase() { String::new() } else { tag }
}

/// One disc of a record, as its header shows it.
pub(crate) struct Disc {
    pub number: u16,
    pub title: String,
    pub art: String,
}

/// Covers are shown only when every disc has one and no two are the same:
/// a box with one cover for all says nothing by repeating it.
pub(crate) fn only_distinct_art(discs: &mut [Disc]) {
    let mut seen = BTreeSet::new();
    if !discs.iter().all(|disc| !disc.art.is_empty() && seen.insert(disc.art.as_str())) {
        discs.iter_mut().for_each(|disc| disc.art.clear());
    }
}

/// Whether those tracks, in disc order, span more than one disc.
pub(crate) fn several(rows: &[TrackRow]) -> bool {
    rows.iter().any(|track| track.disc.max(1) != rows[0].disc.max(1))
}

/// "Disc 2", or "Disc 2 — Das Rheingold" when the disc has a name.
fn label(number: u16, title: &str) -> String {
    let disc = chilpa_i18n::t_args("Disc {}", &[&number.to_string()]);
    if title.is_empty() { disc } else { format!("{disc} \u{2014} {title}") }
}

/// Those tracks, in disc order, with a header before each disc: its
/// number, its name and its cover when it has them, and how many of its
/// songs follow.
pub(crate) fn with_headers(rows: Vec<TrackRow>, discs: &[Disc]) -> Vec<TrackEntry> {
    let mut entries = Vec::with_capacity(rows.len() + discs.len());
    let mut current = None;
    for (index, track) in rows.iter().enumerate() {
        let number = track.disc.max(1);
        if current != Some(number) {
            current = Some(number);
            let songs = rows[index..].iter().take_while(|row| row.disc.max(1) == number).count();
            let disc = discs.iter().find(|disc| disc.number == number);
            entries.push(TrackEntry::Header(GroupHeader {
                label: label(number, disc.map_or("", |disc| &disc.title)),
                meta: chilpa_i18n::tf("{} song", "{} songs", songs as i64, &[&songs.to_string()]),
                art: disc.map(|disc| disc.art.clone()).unwrap_or_default(),
                ..GroupHeader::default()
            }));
        }
        entries.push(TrackEntry::Track(track.clone()));
    }
    entries
}

/// What a file says besides what its row shows, read when media is opened.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct About {
    pub album_artist: String,
    pub year: Option<u32>,
    /// The cover in the file's own folder; empty when there is none.
    pub own_cover: String,
}

/// What tells one block of opened media from the next: the record's
/// folder, its title without the disc, and the disc.
pub(crate) fn block_of(row: &TrackRow) -> (String, String, u16) {
    (record_folder(&row.path), without_disc(&row.album).to_lowercase(), row.disc.max(1))
}

/// The header of a block of opened media: a record, or one disc of it,
/// with its own title, artist and cover.
///
/// `shown` is how many of its tracks the list shows; `disc_said` is whether
/// the media holds more than one disc number, because on an ordinary
/// record "Disc 1" tells nothing apart.
pub(crate) fn block_header(first: &TrackRow, about: Option<&About>, shown: usize, disc_said: bool) -> GroupHeader {
    let record = match without_disc(&first.album) {
        title if title.is_empty() => Path::new(&record_folder(&first.path)).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        title => title,
    };
    let disc = named(&first.path, &first.album, &record);
    let tracks = chilpa_i18n::tf("{} track", "{} tracks", shown as i64, &[&shown.to_string()]);
    let mut meta: Vec<String> = about.and_then(|a| a.year).filter(|year| *year > 0).map(|year| year.to_string()).into_iter().collect();
    if disc_said {
        meta.push(chilpa_i18n::t_args("Disc {}", &[&first.disc.max(1).to_string()]));
    }
    meta.push(tracks);
    GroupHeader {
        label: if disc.is_empty() { record } else { format!("{record} \u{2014} {disc}") },
        artist: about.map(|a| a.album_artist.clone()).filter(|artist| !artist.is_empty()).unwrap_or_else(|| first.artist.clone()),
        meta: meta.join(" \u{00b7} "),
        // The disc's own cover first: a track's is its record's when the
        // disc has none of its own.
        art: about.map(|a| a.own_cover.clone()).filter(|cover| !cover.is_empty()).unwrap_or_else(|| first.artwork_path.clone()),
        record: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_disc_is_named_by_its_folder_before_its_tag() {
        assert_eq!(named("/music/Box/Disc 2 - Live/01.flac", "Box", "Box"), "Live");
        assert_eq!(named("/music/Box/Disc 2 - Live/01.flac", "Something Else", "Box"), "Live");
        assert_eq!(named("sacd:/music/Box/CD3: Encores/image.iso#4", "Box", "Box"), "Encores");
    }

    #[test]
    fn a_tag_names_the_disc_only_when_it_says_more_than_the_record() {
        assert_eq!(named("/music/Box/CD2/01.flac", "Das Rheingold", "Der Ring"), "Das Rheingold");
        assert_eq!(named("/music/Box/CD2/01.flac", "Box (Disc 2)", "Box"), "");
        assert_eq!(named("/music/Box/CD2/01.flac", "box", "Box"), "");
        assert_eq!(named("/music/Box/CD2/01.flac", "", "Box"), "");
    }

    #[test]
    fn covers_show_only_when_every_disc_has_its_own() {
        let disc = |number, art: &str| Disc { number, title: String::new(), art: art.into() };
        let mut own = [disc(1, "/a.jpg"), disc(2, "/b.jpg")];
        only_distinct_art(&mut own);
        assert_eq!(own.iter().map(|d| d.art.as_str()).collect::<Vec<_>>(), ["/a.jpg", "/b.jpg"]);
        for mut shared in [[disc(1, "/a.jpg"), disc(2, "/a.jpg")], [disc(1, "/a.jpg"), disc(2, "")]] {
            only_distinct_art(&mut shared);
            assert!(shared.iter().all(|d| d.art.is_empty()));
        }
    }
}
