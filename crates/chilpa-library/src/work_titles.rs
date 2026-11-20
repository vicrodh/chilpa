//! Work titles as classical releases print them. A part names its work
//! first, then the part: "Symphony No. 5 in C minor, Op. 67: I. Allegro".
//! Only that printed convention is read; nothing is looked up or guessed
//! from a title without a part marker.

/// Words that open a part when a number or a Roman numeral follows them.
const PART_WORDS: &[&str] = &[
    "no", "nr", "nº", "n°", "num", "number", "act", "akt", "acte", "atto", "acto", "aufzug",
    "scene", "szene", "scène", "scena", "escena", "part", "teil", "partie", "parte",
    "movement", "satz", "book", "buch", "livre", "libro", "tableau", "bild", "variation", "var",
];

/// Catalogue prefixes that identify a classical work by number.
const NUMBERED_PREFIXES: &[&str] = &[
    "op", "opus", "bwv", "k", "kv", "hob", "d", "rv", "hwv", "wwv", "buxwv", "twv", "woo", "s",
    "sz", "l", "jlb", "wq", "h", "b", "z", "p", "t", "fwv", "swv", "mwv", "anh", "bb", "ms", "trv",
];

fn is_letter_numeral(word: &str) -> bool {
    !word.is_empty() && word.len() <= 7 && word.chars().all(|c| matches!(c, 'I' | 'V' | 'X' | 'L'))
}

fn numeral(word: &str) -> bool {
    is_letter_numeral(word) || (!word.is_empty() && word.len() <= 3 && word.chars().all(|c| c.is_ascii_digit()))
}

/// Whether `text` starts with a part marker: "I. ", "IV) ", "12. ",
/// "No. 2", "Act I", "Scene 3".
fn opens_part(text: &str) -> bool {
    let head: String = text.chars().take_while(|c| c.is_alphanumeric() || matches!(c, 'º' | '°')).collect();
    let rest = &text[head.len()..];
    if numeral(&head) {
        // A bare numeral needs its punctuation: "I." or "3)", not "I Want".
        return rest.starts_with(['.', ')', ':']) || rest.starts_with(" -") || rest.starts_with(" –");
    }
    let lower = head.to_lowercase();
    if !PART_WORDS.contains(&lower.as_str()) { return false; }
    let after = rest.trim_start_matches('.').trim_start();
    let next: String = after.chars().take_while(|c| c.is_alphanumeric()).collect();
    numeral(&next)
}

const SEPARATORS: &[&str] = &[": ", " - ", " – ", " — "];

/// The work a part title names before its first part marker.
pub fn work_of_part(title: &str) -> Option<&str> {
    let mut best: Option<usize> = None;
    for separator in SEPARATORS {
        let mut from = 0;
        while let Some(at) = title[from..].find(separator) {
            let at = from + at;
            let part = title[at + separator.len()..].trim_start();
            if opens_part(part) {
                best = Some(best.map_or(at, |seen| seen.min(at)));
                break;
            }
            from = at + separator.len();
        }
    }
    let work = title[..best?].trim().trim_end_matches([',', ';']).trim();
    (work.chars().filter(|c| c.is_alphabetic()).count() >= 3).then_some(work)
}

fn folded_words(text: &str) -> String {
    text.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// The work a track's title names, without a leading composer name some
/// releases print ("Wagner: Das Rheingold: Prelude").
pub fn work_from_title(title: &str, composers: &[String]) -> Option<String> {
    if composers.iter().all(|name| name.trim().is_empty()) { return None; }
    let mut work = work_of_part(title)?;
    if let Some((first, rest)) = work.split_once(": ") {
        let first = folded_words(first);
        let names = composers.iter().flat_map(|name| [folded_words(name), folded_words(name.split_whitespace().last().unwrap_or(""))]);
        if !first.is_empty() && names.into_iter().any(|name| name == first) { work = rest.trim(); }
    }
    Some(work.to_string())
}

/// A title without the qualifiers a release adds to it: "(Live in …)",
/// "[Remastered]", " - Live".
fn without_qualifiers(text: &str) -> &str {
    let mut text = text.trim();
    loop {
        let before = text;
        for (open, close) in [('(', ')'), ('[', ']')] {
            if text.ends_with(close) {
                if let Some(at) = text.rfind(open).filter(|at| *at > 0) { text = text[..at].trim_end(); }
            }
        }
        if let Some(at) = text.rfind(" - ").filter(|at| *at > 0) {
            let tail = text[at + 3..].to_lowercase();
            if ["live", "remaster", "version", "mix", "edit", "demo", "acoustic", "mono", "stereo"].iter().any(|word| tail.contains(word)) {
                text = text[..at].trim_end();
            }
        }
        if text == before { return text; }
    }
}

/// Whether a work carries a catalogue number: "Op. 67", "BWV 887", "K. 618".
pub fn has_catalogue(work: &str) -> bool {
    let catalogue = |name: &str| NUMBERED_PREFIXES.contains(&name.trim_end_matches('.').to_lowercase().as_str());
    let number = |text: &str| text.chars().next().is_some_and(|c| c.is_ascii_digit() || matches!(c, 'I' | 'V' | 'X'));
    let words: Vec<&str> = work.split(|c: char| c.is_whitespace() || matches!(c, ',' | '(' | ')')).filter(|w| !w.is_empty()).collect();
    words.iter().any(|word| word.split_once('.').is_some_and(|(name, rest)| catalogue(name) && number(rest)))
        || words.windows(2).any(|pair| catalogue(pair[0]) && number(pair[1]))
}

/// Whether a work only repeats the track's own title, as every song of a
/// popular-music release does when its recording is linked to its work.
pub fn repeats_title(work: &str, title: &str) -> bool {
    let work = folded_words(without_qualifiers(work));
    !work.is_empty() && work == folded_words(without_qualifiers(title))
}

/// The work a lookup proposes for a track, or `None` when it adds nothing:
/// a part's work becomes the whole work, and a song's own title is dropped.
pub fn proposed_work(work: &str, title: &str) -> Option<String> {
    let work = work.trim();
    let whole = work_of_part(work).unwrap_or(work);
    if whole.is_empty() || repeats_title(whole, title) && !has_catalogue(whole) { return None; }
    Some(whole.to_string())
}

#[cfg(test)]
mod tests;
