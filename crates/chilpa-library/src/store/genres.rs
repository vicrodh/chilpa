//! What a genre tag says, as genres.
//!
//! Nothing agrees on how a genre tag is written. One tagger writes a field
//! per genre, another one field with ";" or "," between them, a third pastes
//! a catalog's page as it came: "Folk, World, & CountryStyle:Heavy Metal".
//! Filed as they come, each spelling is its own genre, and the same three
//! genres in another order are a fourth. Reading them is done here, once,
//! for a file, a sidecar, a server and what is already stored.

use std::collections::HashSet;
use std::sync::OnceLock;

/// Genre names that a catalog publishes with a separator inside them:
/// "Funk / Soul", "Hip-Hop/Rap", "Christmas: Rock". A tag that states one
/// of them states one genre.
const WHOLE: &str = include_str!("../../data/genres-whole.txt");

/// The genres somebody else already lists: MusicBrainz's, which are in the
/// public domain (`licenses/genre-lists.txt`). What "/" and ":" may split a
/// tag into is checked against them, so nothing is split on a guess.
const KNOWN: &str = include_str!("../../data/genres-known.txt");

/// What a catalog's page puts before its values, pasted into the tag with
/// them. Matched with its capital anywhere, since it comes glued to the word
/// before ("CountryStyle:"), and in any case only at the start of a word:
/// "Freestyle: ..." is a genre.
const LABELS: &[&str] = &["Styles:", "Style:", "Genres:", "Genre:"];

/// The one whole name that has a comma in it, kept through the split at
/// commas under a name no tag carries.
const WITH_A_COMMA: &str = "Folk, World, & Country";
const KEPT: &str = "\u{1}";

/// The genres a tag states, one each, as they will be filed.
///
/// In this order, which is where the programs that read tags agree and
/// where the catalogs' own names allow it:
///
/// 1. a field is split at ";", "|" and line ends, and a catalog's labels
///    are taken out;
/// 2. a name a catalog publishes whole stays whole;
/// 3. what is left is split at commas;
/// 4. "/" and ":" split only when every genre that comes out is one that
///    MusicBrainz lists: "Death/Doom Metal" is Death Metal and Doom Metal,
///    and "Progressive/Industrial/Atmospheric Metal" stays as it was
///    written, because nobody lists an Atmospheric Metal.
///
/// Each genre loses the spaces and the loose punctuation around it. Repeats
/// are dropped whatever their case, keeping the first spelling.
pub fn split_genres(genres: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut keep = |genre: String| {
        if !genre.is_empty() && !out.iter().any(|g| g.to_lowercase() == genre.to_lowercase()) {
            out.push(genre);
        }
    };
    for field in genres {
        for stated in without_labels(field).split([';', '|', '\0', '\n', '\r']) {
            if let Some(whole) = whole(&tidy(stated)) {
                keep(whole);
                continue;
            }
            for part in replace_ignoring_case(stated, WITH_A_COMMA, KEPT).split(',') {
                let part = tidy(part);
                if part == KEPT {
                    keep(WITH_A_COMMA.to_string());
                } else if let Some(whole) = whole(&part) {
                    keep(whole);
                } else {
                    several(&part.replace(KEPT, "")).into_iter().for_each(&mut keep);
                }
            }
        }
    }
    out
}

/// The name as its catalog spells it, when the tag states one of the whole
/// names, however it is cased or spaced around its separator.
fn whole(stated: &str) -> Option<String> {
    static NAMES: OnceLock<Vec<(String, &'static str)>> = OnceLock::new();
    let names = NAMES.get_or_init(|| WHOLE.lines().map(str::trim).filter(|l| !l.is_empty()).map(|l| (squeezed(l), l)).collect());
    let stated = squeezed(stated);
    names.iter().find(|(name, _)| *name == stated).map(|(_, spelled)| spelled.to_string())
}

/// A name without its case and without the spaces around a separator:
/// "Funk/Soul" and "funk / soul" are the same name.
fn squeezed(name: &str) -> String {
    let spaced = name.to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ");
    spaced.replace(" /", "/").replace("/ ", "/").replace(" :", ":").replace(": ", ":")
}

fn known(genre: &str) -> bool {
    static NAMES: OnceLock<HashSet<String>> = OnceLock::new();
    NAMES.get_or_init(|| KNOWN.lines().map(plain).filter(|l| !l.is_empty()).collect()).contains(&plain(genre))
}

/// A genre as lists are compared: "Avant-Garde Metal", "avant garde metal"
/// and "Rock & Roll", "rock and roll" are each one name.
fn plain(genre: &str) -> String {
    let lowered = genre.to_lowercase().replace('-', " ").replace('&', " and ");
    lowered.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// What one stated genre is when it has "/" or ":" in it: the genres it
/// names, if every one of them is known, and itself if not.
fn several(stated: &str) -> Vec<String> {
    if !stated.contains(['/', ':']) {
        return vec![stated.to_string()];
    }
    let parts: Vec<String> = stated.split(['/', ':']).map(tidy).collect();
    if parts.len() < 2 || parts.iter().any(String::is_empty) {
        return vec![tidy(stated.trim_matches([' ', '/', ':']))];
    }
    // The word they share first: "Gothic/Doom Metal" is Gothic Metal, and
    // "Gothic" alone is a genre too, which the tag did not state.
    let (last, before) = parts.split_last().expect("two parts or more");
    let words: Vec<&str> = last.split(' ').collect();
    for shared in 1..=2 {
        if words.len() > shared {
            let head = words[words.len() - shared..].join(" ");
            let mut named: Vec<String> = before.iter().map(|part| format!("{part} {head}")).collect();
            named.push(last.clone());
            if named.iter().all(|genre| known(genre)) {
                return named;
            }
        }
    }
    if parts.iter().all(|genre| known(genre)) {
        return parts;
    }
    vec![stated.to_string()]
}

/// The field with each label turned into a separator.
fn without_labels(field: &str) -> String {
    let mut out = String::with_capacity(field.len());
    let mut rest = field;
    'scan: while !rest.is_empty() {
        for label in LABELS {
            let Some(head) = rest.get(..label.len()) else { continue };
            let starts_a_word = out.chars().next_back().is_none_or(|c| !c.is_alphabetic());
            if head == *label || (starts_a_word && head.eq_ignore_ascii_case(label)) {
                out.push(';');
                rest = &rest[label.len()..];
                continue 'scan;
            }
        }
        let mut chars = rest.chars();
        if let Some(c) = chars.next() {
            out.push(c);
        }
        rest = chars.as_str();
    }
    out
}

/// One genre without what surrounded it: spaces of any kind run together,
/// and no quotes, dashes, ampersands or stops left at either end.
fn tidy(part: &str) -> String {
    let joined = part.split_whitespace().collect::<Vec<_>>().join(" ");
    let loose = |c: char| c.is_whitespace() || matches!(c, '&' | ':' | '-' | '\u{2013}' | '\u{2014}' | '/' | '\\' | '"' | '\'' | '\u{201c}' | '\u{201d}' | '+' | '*' | '=');
    let kept = joined.trim_matches(loose);
    // "and Country", what is left of a list that ended "..., and Country".
    let kept = match kept.get(..4) {
        Some(head) if head.eq_ignore_ascii_case("and ") => kept[4..].trim_start(),
        _ => kept,
    };
    kept.to_string()
}

fn replace_ignoring_case(text: &str, what: &str, with: &str) -> String {
    let (lower, what) = (text.to_lowercase(), what.to_lowercase());
    // Lowering can change a letter's length; then the places do not match.
    if lower.len() != text.len() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while let Some(found) = lower[at..].find(&what) {
        out.push_str(&text[at..at + found]);
        out.push_str(with);
        at += found + what.len();
    }
    out.push_str(&text[at..]);
    out
}

#[cfg(test)]
mod tests {
    use super::split_genres;

    fn of(tag: &str) -> Vec<String> {
        split_genres(&[tag.to_string()])
    }

    #[test]
    fn a_joined_genre_field_is_several_genres() {
        let g = split_genres(&["Alternative Metal; Alternative Rock".into(), "Grunge|Heavy Metal|grunge".into()]);
        assert_eq!(g, ["Alternative Metal", "Alternative Rock", "Grunge", "Heavy Metal"]);
    }

    #[test]
    fn a_name_a_catalog_publishes_whole_is_one_genre() {
        assert_eq!(of("R&B/Soul"), ["R&B/Soul"]);
        assert_eq!(of("Hip-Hop/Rap; Pop/Rock"), ["Hip-Hop/Rap", "Pop/Rock"]);
        assert_eq!(of("Funk / Soul"), ["Funk / Soul"]);
        assert_eq!(of("funk/soul, Jazz"), ["Funk / Soul", "Jazz"], "as its catalog spells it");
        assert_eq!(of("Christmas: Rock"), ["Christmas: Rock"]);
        assert_eq!(of("Singer/Songwriter"), ["Singer/Songwriter"]);
    }

    #[test]
    fn a_slash_splits_into_genres_that_are_known_and_into_nothing_else() {
        // The genres of my library that had one, as they were measured.
        assert_eq!(of("Death/Doom Metal"), ["Death Metal", "Doom Metal"]);
        assert_eq!(of("Death/Thrash Metal"), ["Death Metal", "Thrash Metal"]);
        assert_eq!(of("Doom / Death Metal"), ["Doom Metal", "Death Metal"]);
        assert_eq!(of("Doom/Gothic Metal"), ["Doom Metal", "Gothic Metal"]);
        assert_eq!(of("Power/Progressive Metal"), ["Power Metal", "Progressive Metal"]);
        assert_eq!(of("Gothic Metal / Rock"), ["Gothic Metal", "Rock"]);
        assert_eq!(of("Hard Rock / Alternative Rock"), ["Hard Rock", "Alternative Rock"]);
        assert_eq!(of("Progressive/Industrial/Atmospheric Metal"), ["Progressive/Industrial/Atmospheric Metal"], "nobody lists an Atmospheric Metal");
        assert_eq!(of("Brutal/Technical Death Metal"), ["Brutal Death Metal", "Technical Death Metal"], "two words shared");
        assert_eq!(of("Zork/Blorb"), ["Zork/Blorb"], "nothing known, nothing split");
    }

    #[test]
    fn the_word_two_genres_share_is_tried_before_the_parts_as_written() {
        // "Gothic", "Progressive" and "Psychedelic" are genres too, and not
        // what these tags state.
        assert_eq!(of("Gothic/Doom Metal"), ["Gothic Metal", "Doom Metal"]);
        assert_eq!(of("Progressive/Avant-Garde Metal"), ["Progressive Metal", "Avant-Garde Metal"]);
        assert_eq!(of("Psychedelic/Progressive Rock"), ["Psychedelic Rock", "Progressive Rock"]);
    }

    #[test]
    fn a_list_with_commas_is_the_same_genres_in_any_order() {
        assert_eq!(of("Rock, Metal, Pop"), ["Rock", "Metal", "Pop"]);
        assert_eq!(of("Pop,Rock"), ["Pop", "Rock"]);
        assert_eq!(of("grunge, alternative rock"), ["grunge", "alternative rock"]);
        assert_eq!(of("Pop, Punk - New Wave, Rock"), ["Pop", "Punk - New Wave", "Rock"]);
    }

    #[test]
    fn a_page_pasted_into_the_tag_is_read_for_its_genres() {
        assert_eq!(
            of("Heavy Metal; Ambient; Electronic; Jazz; Rock; Folk; Prog Rock; World; & CountryStyle:Heavy Metal"),
            ["Heavy Metal", "Ambient", "Electronic", "Jazz", "Rock", "Folk", "Prog Rock", "World", "Country"]
        );
        assert_eq!(of("Stage & ScreenStyle:"), ["Stage & Screen"]);
        assert_eq!(of("Genre: Rock Style: Hard Rock, Blues Rock"), ["Rock", "Hard Rock", "Blues Rock"]);
        assert_eq!(of("genre:Jazz"), ["Jazz"]);
        assert_eq!(of("Freestyle: Miami Bass"), ["Freestyle", "Miami Bass"], "a genre that ends like a label is a genre");
    }

    #[test]
    fn a_genre_under_another_is_both() {
        assert_eq!(of("Metal: Heavy Metal"), ["Metal", "Heavy Metal"]);
        assert_eq!(of("Disc 2: Live"), ["Disc 2: Live"], "and what is not two genres is left as written");
    }

    #[test]
    fn a_genre_named_with_commas_stays_whole() {
        assert_eq!(of("Folk, World, & Country"), ["Folk, World, & Country"]);
        assert_eq!(of("Rock, folk, world, & country, Pop"), ["Rock", "Folk, World, & Country", "Pop"]);
    }

    #[test]
    fn what_surrounds_a_genre_is_not_part_of_it() {
        assert_eq!(of("  Rock \t;\"Blues\"; - Jazz -;Hip   Hop"), ["Rock", "Blues", "Jazz", "Hip Hop"]);
        assert_eq!(of("Soul, and Funk"), ["Soul", "Funk"]);
        assert_eq!(of("R&B; Drum & Bass; Rock & Roll"), ["R&B", "Drum & Bass", "Rock & Roll"]);
        assert_eq!(of("Alt. Rock"), ["Alt. Rock"]);
        assert_eq!(of("Andean"), ["Andean"], "a genre that starts like \"and\" is a genre");
    }

    #[test]
    fn a_tag_that_says_nothing_has_no_genres() {
        assert!(of("").is_empty());
        assert!(of(" ; , | : & ").is_empty());
        assert!(of("Style:").is_empty());
    }

    #[test]
    fn letters_outside_ascii_are_read_like_any_other() {
        assert_eq!(of("Électronique, Techno"), ["Électronique", "Techno"]);
        assert_eq!(of("Alternatif et Indé, Pop, ROCK, rock"), ["Alternatif et Indé", "Pop", "ROCK"]);
        assert_eq!(of("İstanbul Folk, World, & Country"), ["İstanbul Folk", "World", "Country"]);
    }

    #[test]
    fn reading_what_was_read_changes_nothing() {
        for tag in ["Heavy Metal; Folk; World; & CountryStyle:Heavy Metal", "Rock, folk, world, & country, Pop", "Metal: Heavy Metal", "R&B/Soul", "Death/Doom Metal", "funk/soul", "Progressive/Industrial/Atmospheric Metal", "Christmas: Rock"] {
            let once = of(tag);
            let again: Vec<String> = once.iter().flat_map(|g| of(g)).collect();
            assert_eq!(once, again, "{tag}");
        }
    }
}
