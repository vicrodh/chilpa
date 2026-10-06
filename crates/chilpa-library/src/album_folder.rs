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

/// The name a disc's folder gives the disc: "Disc 2 - Live" names it "Live".
///
/// A box that names its discs usually says so here, and a tag cannot
/// contradict it. The disc has to lead and a separator has to follow it
/// with a space on one side: "Now 75 - CD1" and "CD-Rom Favourites" are
/// titles of records, not discs with a name.
pub fn disc_title(dir: &Path) -> Option<String> {
    let name = dir.file_name()?.to_str()?.trim();
    let (head, tail) = name.char_indices().find_map(|(at, c)| {
        if !matches!(c, '-' | '\u{2013}' | '\u{2014}' | ':') {
            return None;
        }
        let after = at + c.len_utf8();
        let spaced = name[..at].ends_with(char::is_whitespace) || name[after..].starts_with(char::is_whitespace);
        spaced.then(|| (name[..at].trim(), name[after..].trim()))
    })?;
    let disc = disc_folder(Path::new(head))?;
    // The head is the disc and nothing else: its own tail would be a second name.
    let bare = !disc.needs_confirmation && head.chars().last().is_some_and(|c| c.is_ascii_digit());
    (bare && !tail.is_empty()).then(|| tail.to_owned())
}

/// A record's title without the disc it ends in: "Box (Disc 2)", "Box -
/// CD 2" and "Box Disc2" are all "Box". What is left tells a tag that
/// names its disc from one that only numbers it.
pub fn without_disc(title: &str) -> String {
    let trimmed = title.trim();
    for (open, close) in [('(', ')'), ('[', ']')] {
        if let (true, Some(start)) = (trimmed.ends_with(close), trimmed.rfind(open)) {
            if is_disc_designator(trimmed[start + 1..trimmed.len() - 1].trim()) {
                return trimmed[..start].trim().to_owned();
            }
        }
    }
    let tokens: Vec<&str> = trimmed.split_whitespace().filter(|t| !matches!(*t, "-" | "\u{2013}" | "\u{2014}")).collect();
    if let [rest @ .., word, number] = tokens.as_slice() {
        let marker = matches!(word.to_lowercase().as_str(), "disc" | "disk" | "cd");
        // "Disc 2" alone is left with nothing: it never was a title.
        if marker && number.chars().all(|c| c.is_ascii_digit()) {
            return rest.join(" ");
        }
    }
    if let [rest @ .., last] = tokens.as_slice() {
        if !rest.is_empty() && is_disc_designator(last) {
            return rest.join(" ");
        }
    }
    trimmed.to_owned()
}

/// "Disc 2", "disc02", "CD-3": a disc word and its number, nothing else.
fn is_disc_designator(value: &str) -> bool {
    let cleaned: String = value.to_lowercase().chars().filter(char::is_ascii_alphanumeric).collect();
    ["disc", "disk", "cd"].into_iter().any(|word| {
        cleaned.strip_prefix(word).is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()))
    })
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
    fn a_disc_folder_can_name_its_disc() {
        for (name, title) in [
            ("Disc 01 - TV Series Soundtrack #01", "TV Series Soundtrack #01"),
            ("CD1 - Bonus Material", "Bonus Material"),
            ("Disc 2 \u{2013} Live", "Live"),
            ("Disc 3: The Remixes", "The Remixes"),
            ("cd 04 - instrumentals", "instrumentals"),
        ] {
            assert_eq!(disc_title(Path::new(&format!("/music/Box/{name}"))).as_deref(), Some(title), "{name}");
        }
    }

    #[test]
    fn a_folder_that_only_numbers_its_disc_names_nothing() {
        for name in ["Disc 1", "CD2", "Disc 1 -", "Now 75 - CD1", "CD-Rom Favourites", "Disc Jockey Anthems",
            "Greatest Hits - Disc 1 of 2 Collection", "Match of the Day - The Album CD1", "Abbey Road", "CD 1 Mono - Live"] {
            assert_eq!(disc_title(Path::new(&format!("/music/{name}"))), None, "{name}");
        }
    }

    #[test]
    fn a_title_loses_the_disc_it_ends_in() {
        for title in ["Box (Disc 2)", "Box [CD2]", "Box - Disc 2", "Box \u{2013} CD 2", "Box Disc2", "Box disk 03", " Box (disc 1) "] {
            assert_eq!(without_disc(title), "Box", "{title}");
        }
    }

    #[test]
    fn a_title_that_is_not_numbering_a_disc_stays_whole() {
        for title in ["Box", "CD1", "Discovery", "Box (Live)", "Greatest Hits 2", "Box (2008)", "Rheingold"] {
            assert_eq!(without_disc(title), title, "{title}");
        }
        assert_eq!(without_disc("Disc 2"), "", "a disc and its number are no title at all");
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
