//! What a file's place says about it, for what its tags leave out.
//!
//! A file whose tags could not be read, or that has none, came into the
//! library under its file name with no album, and a track with no album is
//! on no album screen: the user saw the folder on disk and not the record in
//! Chilpa. Most collections are laid out the same way --
//! `Artist/Year - Album/NN - Title.ext`, with a disc folder now and then --
//! so the path says most of what is missing. It only fills what the tags do
//! not say; a tag always wins.

use crate::album_folder::disc_folder;
use std::path::Path;

/// What a path says. Every field may be missing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FromPath {
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub year: Option<u32>,
    pub disc: Option<u16>,
    pub track: Option<u16>,
    pub title: Option<String>,
    /// The artist a file name gives before its title: "05. The Cure - High".
    pub artist: Option<String>,
}

/// Reads `file`'s place under `root`. Folders at or above `root` say nothing:
/// "DSD" or "Flac" is where a library keeps things, not who made them.
pub fn from_path(file: &Path, root: Option<&Path>) -> FromPath {
    let mut out = FromPath::default();
    let below_root = |dir: &Path| root.is_none_or(|root| dir.starts_with(root) && dir != root);
    let Some(mut album_dir) = file.parent().filter(|dir| below_root(dir)) else {
        return named_file(file, out);
    };
    // A disc of a set: its number, and the record is the folder above.
    if let Some(disc) = disc_folder(album_dir).filter(|disc| !disc.needs_confirmation) {
        out.disc = Some(disc.number);
        match album_dir.parent().filter(|dir| below_root(dir)) {
            Some(parent) => album_dir = parent,
            None => return named_file(file, out),
        }
    }
    let artist_dir = album_dir.parent().filter(|dir| below_root(dir)).and_then(name_of).filter(|name| !is_index(name));
    if let Some(name) = name_of(album_dir) {
        let (album, artist, year) = album_name(&name, artist_dir.as_deref());
        out.album = album;
        out.year = year;
        out.album_artist = artist.or(artist_dir);
    }
    named_file(file, out)
}

/// The file name's number, artist and title.
fn named_file(file: &Path, mut out: FromPath) -> FromPath {
    let Some(stem) = file.file_stem().and_then(|s| s.to_str()) else { return out };
    let (track, rest) = leading_number(stem);
    out.track = track;
    let rest = rest.trim();
    // "The Cure - High": the artist when it is the record's, or when the
    // record names nobody; otherwise the dash is part of the title.
    let title = match rest.split_once(" - ") {
        Some((who, what)) if !who.trim().is_empty() && !what.trim().is_empty() => {
            let same = out.album_artist.as_deref().is_some_and(|artist| plain(artist) == plain(who));
            if same || out.album_artist.is_none() {
                out.artist = Some(who.trim().to_string());
                what.trim()
            } else {
                rest
            }
        }
        _ => rest,
    };
    out.title = (!title.is_empty()).then(|| title.to_string());
    out
}

/// "02 - My Sweet Lord", "05. Title", "7_Title": the number and the rest.
/// A name that is only a number, or a number that is plainly a year, is a
/// title.
fn leading_number(stem: &str) -> (Option<u16>, &str) {
    let digits = stem.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 || digits > 3 {
        return (None, stem);
    }
    let rest = &stem[digits..];
    let after = rest.trim_start_matches([' ', '.', '-', '_', ')']);
    if after.is_empty() || after.len() == rest.len() {
        return (None, stem);
    }
    (stem[..digits].parse().ok().filter(|n| *n > 0), after)
}

/// A record folder's name, read for its title, its artist and its year:
/// "1970 - All Things Must Pass", "(1992) Wish [FLAC 16-44]", "Paul
/// McCartney & Wings - Band On The Run (Japan-1973)".
fn album_name(name: &str, artist_dir: Option<&str>) -> (Option<String>, Option<String>, Option<u32>) {
    let mut year = None;
    let mut rest = name.trim();
    // A year in front, bare or bracketed.
    if let Some((found, after)) = leading_year(rest) {
        year = Some(found);
        rest = after;
    }
    // "Artist - Album": the artist, unless it is the year again.
    let mut artist = None;
    if let Some((who, what)) = rest.split_once(" - ") {
        if let Some((found, _)) = leading_year(who).filter(|(_, after)| after.is_empty()) {
            year = year.or(Some(found));
            rest = what;
        } else if !who.trim().is_empty() && !what.trim().is_empty() {
            let named_again = artist_dir.is_some_and(|dir| plain(dir) == plain(who));
            if !named_again {
                artist = Some(who.trim().to_string());
            }
            rest = what;
        }
    }
    // What edition and format the folder is: "[FLAC 16-44]", "(SACD-ISO)",
    // "(1973)". A bracket that only says words is the title's own, as in
    // "(pronounced 'leh-'nerd 'skin-'nerd)".
    // Of the years in them, the leftmost is the record's: "(1973) [MFSL
    // 2012]" is a 1973 record pressed again in 2012.
    let leading = year.is_some();
    let mut title = rest.trim().to_string();
    while let Some((head, group)) = trailing_group(&title) {
        let is_square = group.starts_with('[') || group.starts_with('{');
        if !is_square && !says_edition(group) {
            break;
        }
        if let Some(found) = year_in(group).filter(|_| !leading) {
            year = Some(found);
        }
        title = head.trim_end().to_string();
    }
    let title = title.trim().trim_end_matches(['-', ' ']).to_string();
    ((!title.is_empty()).then_some(title), artist, year)
}

fn leading_year(text: &str) -> Option<(u32, &str)> {
    let text = text.trim_start();
    let (inner, after) = match text.chars().next()? {
        '(' | '[' => {
            let close = text.find([')', ']'])?;
            (&text[1..close], &text[close + 1..])
        }
        _ => (text.get(..4)?, &text[4..]),
    };
    let year = inner.trim().parse::<u32>().ok().filter(|y| (1900..=2100).contains(y))?;
    // The year is the whole of what it is: "1970 - ", "1970. ", "(1970) ".
    if inner.trim().len() != 4 || after.starts_with(|c: char| c.is_ascii_alphanumeric()) {
        return None;
    }
    Some((year, after.trim_start_matches([' ', '.', '-', '_'])))
}

/// The last bracketed group of `text` and what is before it.
fn trailing_group(text: &str) -> Option<(&str, &str)> {
    let text = text.trim_end();
    let close = text.chars().last()?;
    let open = match close {
        ')' => '(',
        ']' => '[',
        '}' => '{',
        _ => return None,
    };
    let at = text.rfind(open)?;
    let head = &text[..at];
    (!head.trim().is_empty()).then(|| (head, &text[at..]))
}

/// A round bracket that names an edition or a format, not part of a title.
fn says_edition(group: &str) -> bool {
    let lower = group.to_lowercase();
    lower.chars().any(|c| c.is_ascii_digit())
        || ["flac", "sacd", "iso", "dsd", "dsf", "dff", "mp3", "wav", "hi-res", "hires", "khz", "bit", "vinyl", "web"]
            .iter()
            .any(|word| lower.contains(word))
}

fn year_in(text: &str) -> Option<u32> {
    let bytes = text.as_bytes();
    (0..bytes.len().saturating_sub(3)).find_map(|at| {
        let window = text.get(at..at + 4)?;
        let alone = |i: usize| bytes.get(i).is_none_or(|b| !b.is_ascii_digit());
        let starts_clean = at == 0 || alone(at - 1);
        (starts_clean && alone(at + 4)).then(|| window.parse::<u32>().ok()).flatten().filter(|y| (1900..=2100).contains(y))
    })
}

/// "G", "#", "0-9": a folder that sorts artists, not one. "U2" is one.
fn is_index(name: &str) -> bool {
    let name = name.trim();
    name.chars().count() == 1 || name.eq_ignore_ascii_case("0-9")
}

fn name_of(dir: &Path) -> Option<String> {
    dir.file_name().and_then(|n| n.to_str()).map(|n| n.trim().to_string()).filter(|n| !n.is_empty())
}

/// Letters and digits only, lower case: how two names are told the same.
fn plain(name: &str) -> String {
    name.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(path: &str) -> FromPath {
        from_path(Path::new(path), Some(Path::new("/music/DSD")))
    }

    #[test]
    fn year_then_album_under_the_artist() {
        let got = read("/music/DSD/G/George Harrison/1970 - All Things Must Pass/02 - My Sweet Lord.dsf");
        assert_eq!(got.album.as_deref(), Some("All Things Must Pass"));
        assert_eq!(got.album_artist.as_deref(), Some("George Harrison"));
        assert_eq!(got.year, Some(1970));
        assert_eq!((got.track, got.title.as_deref()), (Some(2), Some("My Sweet Lord")));
        assert_eq!(got.disc, None);
    }

    #[test]
    fn a_bracketed_year_and_the_format_after_the_title() {
        let got = read("/music/DSD/C/The Cure/(1992) Wish [FLAC 16-44]/05. The Cure - Wendy Time (Remastered 2022).flac");
        assert_eq!(got.album.as_deref(), Some("Wish"));
        assert_eq!(got.year, Some(1992));
        assert_eq!(got.album_artist.as_deref(), Some("The Cure"));
        assert_eq!(got.track, Some(5));
        assert_eq!(got.title.as_deref(), Some("Wendy Time (Remastered 2022)"), "the artist before the title is not the title");
        assert_eq!(got.artist.as_deref(), Some("The Cure"));
    }

    #[test]
    fn the_artist_in_the_folder_name_over_the_folder_above() {
        let got = read("/music/DSD/W/The Wings/Paul McCartney & Wings - Band On The Run (Japan-1973)/03 - Jet.dff");
        assert_eq!(got.album.as_deref(), Some("Band On The Run"));
        assert_eq!(got.album_artist.as_deref(), Some("Paul McCartney & Wings"));
        assert_eq!(got.year, Some(1973));
    }

    #[test]
    fn a_title_in_brackets_stays_and_the_edition_goes() {
        let got = read("/music/DSD/L/Lynyrd Skynyrd/Lynyrd Skynyrd - (pronounced 'leh-'nerd 'skin-'nerd) (1973) [MFSL 2012] (SACD-ISO)/8.dsf");
        assert_eq!(got.album.as_deref(), Some("(pronounced 'leh-'nerd 'skin-'nerd)"));
        assert_eq!(got.album_artist.as_deref(), Some("Lynyrd Skynyrd"));
        assert_eq!(got.year, Some(1973));
        assert_eq!((got.track, got.title.as_deref()), (None, Some("8")), "a bare number is a title");
    }

    #[test]
    fn a_disc_folder_numbers_the_disc_and_the_record_is_above() {
        let got = read("/music/DSD/S/Soundtracks/Castlevania OST [FLAC+log]/DISC2/04 - Bloody Tears.flac");
        assert_eq!(got.disc, Some(2));
        assert_eq!(got.album.as_deref(), Some("Castlevania OST"));
        assert_eq!(got.album_artist.as_deref(), Some("Soundtracks"));
        assert_eq!(got.track, Some(4));
    }

    #[test]
    fn a_two_letter_artist_is_an_artist() {
        let got = read("/music/DSD/U/U2/1987 - The Joshua Tree/03 - With Or Without You.dsf");
        assert_eq!(got.album_artist.as_deref(), Some("U2"));
    }

    #[test]
    fn the_root_and_index_folders_name_nobody() {
        let got = read("/music/DSD/G/Loose Album/01 - One.dsf");
        assert_eq!(got.album.as_deref(), Some("Loose Album"));
        assert_eq!(got.album_artist, None, "\"G\" sorts artists");
        let got = read("/music/DSD/Straight Under The Root/01 - One.dsf");
        assert_eq!(got.album_artist, None, "the root is no artist");
        let got = read("/music/DSD/01 - Alone.dsf");
        assert_eq!((got.album, got.track, got.title.as_deref()), (None, Some(1), Some("Alone")));
    }

    #[test]
    fn a_dash_in_a_title_is_kept_when_it_names_someone_else() {
        let got = read("/music/DSD/Q/Queen/1975 - A Night at the Opera/11 - Bohemian Rhapsody - Remastered.flac");
        assert_eq!(got.title.as_deref(), Some("Bohemian Rhapsody - Remastered"));
        assert_eq!(got.artist, None);
    }
}
