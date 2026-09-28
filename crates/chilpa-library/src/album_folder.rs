//! Which folder a record's files are grouped by when they do not name their
//! album artist.
//!
//! The folder they sit in, unless that folder is one disc of a set -- "CD1",
//! "Disc 2 - Live" -- in which case it is the folder above: the two discs of
//! a compilation are one record. A folder whose name only ends in a disc,
//! like "Now 75 - CD1", is a record of its own and not a disc of one.

use std::path::Path;

/// A folder name that may identify one disc of a set.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct DiscFolder {
    pub number: u16,
    pub style: &'static str,
    /// A subtitle separated only by whitespace needs another piece of
    /// evidence before the folder may be skipped.
    pub needs_confirmation: bool,
}

/// The album artist of a record whose tracks are by more than one artist and
/// whose files do not say whose it is. The name MusicBrainz gives the same
/// thing.
pub const VARIOUS_ARTISTS: &str = "Various Artists";

/// The folder a file's record is grouped by.
pub fn album_folder(file: &Path) -> String {
    let mut dir = file.parent();
    // Two levels at most: "CD1" inside "Disc 1" is as deep as sets go.
    for _ in 0..2 {
        match dir {
            Some(d) if is_disc_folder(d) => dir = d.parent(),
            _ => break,
        }
    }
    dir.map(|d| d.display().to_string()).unwrap_or_default()
}

/// A folder named for one disc of a set: a disc word, its number, and at
/// most a subtitle after a separator.
pub(crate) fn disc_folder(dir: &Path) -> Option<DiscFolder> {
    let Some(name) = dir.file_name().and_then(|n| n.to_str()) else { return None };
    let lower = name.trim().to_lowercase();
    // Longest first, so "disco" is not read as "disc" and an "o".
    let Some((style, rest)) = ["disco", "disc", "disk", "cd"]
        .into_iter()
        .find_map(|word| lower.strip_prefix(word).map(|rest| (word, rest)))
    else {
        return None;
    };
    let rest = rest.trim_start_matches([' ', '_', '-', '.']);
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let number = rest[..digits].parse::<u16>().ok().filter(|number| *number > 0)?;
    let raw_tail = &rest[digits..];
    let tail = raw_tail.trim_start();
    if tail.is_empty() {
        return Some(DiscFolder { number, style, needs_confirmation: false });
    }
    if tail.starts_with(['-', '–', ':', '(', '[']) {
        return Some(DiscFolder { number, style, needs_confirmation: false });
    }
    raw_tail.starts_with(char::is_whitespace).then_some(DiscFolder {
        number,
        style,
        needs_confirmation: true,
    })
}

fn is_disc_folder(dir: &Path) -> bool {
    disc_folder(dir).is_some_and(|disc| !disc.needs_confirmation)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(path: &str) -> String {
        album_folder(Path::new(path))
    }

    #[test]
    fn a_file_belongs_to_the_folder_it_sits_in() {
        assert_eq!(folder("/music/OST/01.flac"), "/music/OST");
    }

    #[test]
    fn the_discs_of_a_set_are_one_record() {
        for disc in ["CD1", "CD 2", "cd01", "Disc 1", "DISC2", "Disk 3", "Disco 1", "Disc 2 - Live", "CD1 (Bonus)"] {
            assert_eq!(folder(&format!("/music/Box/{disc}/01.flac")), "/music/Box", "{disc}");
        }
    }

    #[test]
    fn a_name_that_only_ends_in_a_disc_is_a_record_of_its_own() {
        for name in ["Now 75 - CD1", "Discovery", "CDs", "Disco Inferno", "Relaxation Disc1"] {
            let path = format!("/music/{name}/01.flac");
            assert_eq!(folder(&path), format!("/music/{name}"), "{name}");
        }
    }

    #[test]
    fn a_space_only_subtitle_is_a_candidate_that_needs_confirmation() {
        for name in ["CD 1 Mono", "Disc 5  live! 1978-1999"] {
            let disc = disc_folder(Path::new(name)).unwrap();
            assert!(disc.needs_confirmation, "{name}");
        }
        assert!(!disc_folder(Path::new("CD 01")).unwrap().needs_confirmation);
    }
}
