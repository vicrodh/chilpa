//! What a genre tag says, as genres.
//!
//! Nothing agrees on how a genre tag is written. One tagger writes a field
//! per genre, another one field with ";" or "," between them, a third pastes
//! a catalog's page as it came: "Folk, World, & CountryStyle:Heavy Metal".
//! Filed as they come, each spelling is its own genre, and the same three
//! genres in another order are a fourth. Reading them is done here, once,
//! for a file, a sidecar, a server and what is already stored.

/// Genres whose own name has what otherwise separates two.
const WHOLE: &[&str] = &["Folk, World, & Country"];

/// What a catalog's page puts before its values, pasted into the tag with
/// them. Matched with its capital anywhere, since it comes glued to the word
/// before ("CountryStyle:"), and in any case only at the start of a word:
/// "Freestyle: ..." is a genre.
const LABELS: &[&str] = &["Styles:", "Style:", "Genres:", "Genre:"];

/// The genres a tag states, one each, as they will be filed.
///
/// A field is split at ";", "|", ",", ":" and line ends, and at a catalog's
/// labels. "/" is left alone: "R&B/Soul" names one genre as often as two,
/// and "Death/Doom Metal" split would file a "Death". What is left of each
/// one loses the spaces and the loose punctuation around it. Repeats are
/// dropped whatever their case, keeping the first spelling.
pub fn split_genres(genres: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for field in genres {
        let mut field = without_labels(field);
        // Kept whole through the split, under a name no tag carries.
        for (place, whole) in WHOLE.iter().enumerate() {
            field = replace_ignoring_case(&field, whole, &format!("\u{1}{place}\u{1}"));
        }
        for part in field.split([';', '|', ',', ':', '\0', '\n', '\r']) {
            let part = tidy(part);
            let part = match part.strip_prefix('\u{1}').and_then(|p| p.strip_suffix('\u{1}')).and_then(|p| p.parse::<usize>().ok()) {
                Some(place) if place < WHOLE.len() => WHOLE[place].to_string(),
                _ => part.replace('\u{1}', ""),
            };
            if !part.is_empty() && !out.iter().any(|g| g.to_lowercase() == part.to_lowercase()) {
                out.push(part);
            }
        }
    }
    out
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
    let loose = |c: char| c.is_whitespace() || matches!(c, '&' | '-' | '\u{2013}' | '\u{2014}' | '/' | '\\' | '"' | '\'' | '\u{201c}' | '\u{201d}' | '+' | '*' | '=');
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
        assert_eq!(of("R&B/Soul"), ["R&B/Soul"], "a slash is left alone");
        assert_eq!(of("Death/Doom Metal"), ["Death/Doom Metal"]);
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
        for tag in ["Heavy Metal; Folk; World; & CountryStyle:Heavy Metal", "Rock, folk, world, & country, Pop", "Metal: Heavy Metal", "R&B/Soul"] {
            let once = of(tag);
            let again: Vec<String> = once.iter().flat_map(|g| of(g)).collect();
            assert_eq!(once, again, "{tag}");
        }
    }
}
