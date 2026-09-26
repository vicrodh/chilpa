//! Which folder a record's files are grouped by when they do not name their
//! album artist.
//!
//! The folder they sit in, unless that folder is one disc of a set -- "CD1",
//! "Disc 2 - Live" -- in which case it is the folder above: the two discs of
//! a compilation are one record. A folder whose name only ends in a disc,
//! like "Now 75 - CD1", is a record of its own and not a disc of one.

use std::path::Path;

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
fn is_disc_folder(dir: &Path) -> bool {
    let Some(name) = dir.file_name().and_then(|n| n.to_str()) else { return false };
    let lower = name.trim().to_lowercase();
    // Longest first, so "disco" is not read as "disc" and an "o".
    let Some(rest) = ["disco", "disc", "disk", "cd"].iter().find_map(|w| lower.strip_prefix(w)) else {
        return false;
    };
    let rest = rest.trim_start_matches([' ', '_', '-', '.']);
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 {
        return false;
    }
    let tail = rest[digits..].trim_start();
    tail.is_empty() || tail.starts_with(['-', '–', ':', '(', '['])
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
}
