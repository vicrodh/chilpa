//! The Library's stack of conditions as the window shows it: chips that say
//! each condition in words, and the edits a click makes on them. What
//! travels between the window and here is the text of `store::query`, and
//! a chip is one condition of its top level: all of them have to hold, or
//! with "any" one is enough.
//!
//! A genre typed close to one the library has is read as that one too:
//! "hiphop", "Hip Hop" and "hip-hop" are one name, a long name one or two
//! letters off ("Electronika") finds the one that is there, and a name
//! MusicBrainz knows a genre by finds the genre under its other names
//! ("dnb" finds "Drum & Bass"), from a table generated from its genres.

use chilpa_i18n as i18n;
use chilpa_library::store::query::{parse, Field, Node, Values};
use std::collections::HashMap;

/// MusicBrainz's genres that go by other names: the name, then each alias,
/// a tab apart. Written by `tools/genres/musicbrainz-aliases`.
const ALIASES: &str = include_str!("../data/genre-aliases.tsv");

/// The names each genre goes by, as keys, and which genre a key is.
pub(crate) struct Aliases {
    genre_of: HashMap<String, usize>,
    names: Vec<Vec<String>>,
}

impl Aliases {
    pub(crate) fn read(text: &str) -> Self {
        let mut aliases = Self { genre_of: HashMap::new(), names: Vec::new() };
        for line in text.lines() {
            let mut keys: Vec<String> = line.split('\t').map(genre_key).filter(|key| !key.is_empty()).collect();
            // "hip hop" and "hip-hop" are one key already.
            let mut seen = std::collections::HashSet::new();
            keys.retain(|key| seen.insert(key.clone()));
            if keys.len() < 2 { continue; }
            let at = aliases.names.len();
            for key in &keys { aliases.genre_of.entry(key.clone()).or_insert(at); }
            aliases.names.push(keys);
        }
        aliases
    }

    /// Every name the genre of `key` goes by, `key` among them.
    fn names_of<'a>(&'a self, key: &'a str) -> Vec<&'a str> {
        match self.genre_of.get(key) {
            Some(at) => self.names[*at].iter().map(String::as_str).collect(),
            None => vec![key],
        }
    }
}

fn bundled() -> &'static Aliases {
    static BUNDLED: std::sync::OnceLock<Aliases> = std::sync::OnceLock::new();
    BUNDLED.get_or_init(|| Aliases::read(ALIASES))
}

/// `:filter genre:jazz -label:verve` stacks conditions on the Library;
/// `:filter` alone takes them away.
pub(crate) const COMMANDS: &[crate::commands::Command] = &[
    crate::commands::command("filter", "library.filter", "[query]"),
];

/// One condition of the stack, as a chip.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct QueryChip {
    /// What the chip says: "Genre: Jazz, Soul".
    pub label: String,
    /// The condition as text, which the command line takes back.
    pub text: String,
    /// It keeps out what it names.
    pub negated: bool,
}

/// A query as typed, read: nothing for an empty text, or why it cannot be read.
pub fn read(text: &str) -> Result<Option<Node>, String> {
    if text.trim().is_empty() { return Ok(None); }
    parse(text).map(Some).map_err(|error| i18n::t_args(i18n::mark("The filter cannot be read: {}"), &[&i18n::t(error.what)]))
}

fn split(text: &str) -> (bool, Vec<Node>) {
    match read(text) {
        Ok(Some(Node::All(nodes))) => (false, nodes),
        Ok(Some(Node::Any(nodes))) => (true, nodes),
        Ok(Some(node)) => (false, vec![node]),
        _ => (false, Vec::new()),
    }
}

fn join(any: bool, mut chips: Vec<Node>) -> String {
    match chips.len() {
        0 => String::new(),
        1 => chips.pop().expect("one").to_string(),
        _ => if any { Node::Any(chips) } else { Node::All(chips) }.to_string(),
    }
}

fn field_label(field: Field) -> String {
    i18n::t(match field {
        Field::Year => "Year", Field::Genre => "Genre", Field::Label => "Label", Field::Composer => "Composer",
        Field::Artist => "Artist", Field::Album => "Album", Field::Title => "Title", Field::Rating => "Rating",
        Field::Added => "Added", Field::Text => "Words",
    })
}

fn chip(node: &Node) -> QueryChip {
    let (negated, inner) = match node { Node::Not(inner) => (true, inner.as_ref()), other => (false, other) };
    let label = match inner {
        Node::Is(field, values) => {
            let said = match values {
                Values::Any(list) => list.join(", "),
                Values::Range(Some(from), Some(to)) => format!("{from}–{to}"),
                Values::Range(Some(from), None) => format!("≥ {from}"),
                Values::Range(None, Some(to)) => format!("≤ {to}"),
                Values::Range(None, None) => String::new(),
                Values::Within(seconds) => {
                    let days = seconds / 86_400;
                    i18n::tf("last {} day", "last {} days", days, &[&days.to_string()])
                }
                Values::Missing => i18n::t("None"),
            };
            format!("{}: {said}", field_label(*field))
        }
        other => other.to_string(),
    };
    QueryChip { label, text: node.to_string(), negated }
}

/// The chips of a query, and whether one of them is enough.
pub fn query_chips(text: &str) -> (bool, Vec<QueryChip>) {
    let (any, nodes) = split(text);
    (any, nodes.iter().map(chip).collect())
}

/// The query without its chip `at`.
pub fn query_without(text: &str, at: usize) -> String {
    let (any, mut nodes) = split(text);
    if at < nodes.len() { nodes.remove(at); }
    join(any, nodes)
}

/// The query with its chip `at` turned around: what it kept, it keeps out.
pub fn query_flipped(text: &str, at: usize) -> String {
    let (any, mut nodes) = split(text);
    if let Some(node) = nodes.get_mut(at) {
        let taken = std::mem::replace(node, Node::All(Vec::new()));
        *node = match taken { Node::Not(inner) => *inner, other => Node::Not(Box::new(other)) };
    }
    join(any, nodes)
}

/// The query with its chips joined by "all" or by "any".
pub fn query_joined(text: &str, any: bool) -> String {
    join(any, split(text).1)
}

/// The query that also keeps out a value of a column: `column` is one of the
/// Library's ("year", "genre", "label", "composer", "rating", "artist").
/// A value already kept out of that column joins its chip.
pub fn query_excluding(text: &str, column: &str, value: &str) -> String {
    let field = match column {
        "year" => Field::Year, "genre" => Field::Genre, "label" => Field::Label, "composer" => Field::Composer,
        "rating" => Field::Rating, "artist" => Field::Artist, _ => return text.to_string(),
    };
    let (any, mut nodes) = split(text);
    let held = nodes.iter_mut().find_map(|node| match node {
        Node::Not(inner) => match inner.as_mut() { Node::Is(f, Values::Any(list)) if *f == field => Some(list), _ => None },
        _ => None,
    });
    match held {
        Some(list) => if !list.iter().any(|v| v.eq_ignore_ascii_case(value)) { list.push(value.to_string()) },
        None => nodes.push(Node::Not(Box::new(Node::Is(field, Values::Any(vec![value.to_string()]))))),
    }
    // Keeping something out of every chip that has to hold is "all"; a
    // stack of "any" keeps its own way.
    join(any && nodes.len() > 1, nodes)
}

/// A name folded to what is left of it: lower case, letters and digits, "&"
/// as "and".
pub(crate) fn genre_key(name: &str) -> String {
    name.to_lowercase().replace('&', "and").chars().filter(|c| c.is_alphanumeric()).collect()
}

/// How many letters apart two words are, past `most` counted as `most + 1`.
fn distance(a: &[char], b: &[char], most: usize) -> usize {
    if a.len().abs_diff(b.len()) > most { return most + 1; }
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut row = vec![i + 1; b.len() + 1];
        for (j, cb) in b.iter().enumerate() {
            row[j + 1] = (previous[j] + usize::from(ca != cb)).min(previous[j + 1] + 1).min(row[j] + 1);
        }
        if row.iter().min().is_some_and(|least| *least > most) { return most + 1; }
        previous = row;
    }
    previous[b.len()]
}

/// Whether the query names a genre anywhere.
pub(crate) fn names_genres(node: &Node) -> bool {
    match node {
        Node::All(nodes) | Node::Any(nodes) => nodes.iter().any(names_genres),
        Node::Not(inner) => names_genres(inner),
        Node::Is(field, values) => *field == Field::Genre && matches!(values, Values::Any(_)),
    }
}

/// The query with every genre it names that `known` does not have read
/// also as the known ones close to it.
pub(crate) fn widen_genres(node: &Node, known: &[String]) -> Node {
    widen_genres_with(node, known, bundled())
}

pub(crate) fn widen_genres_with(node: &Node, known: &[String], aliases: &Aliases) -> Node {
    match node {
        Node::All(nodes) => Node::All(nodes.iter().map(|n| widen_genres_with(n, known, aliases)).collect()),
        Node::Any(nodes) => Node::Any(nodes.iter().map(|n| widen_genres_with(n, known, aliases)).collect()),
        Node::Not(inner) => Node::Not(Box::new(widen_genres_with(inner, known, aliases))),
        Node::Is(Field::Genre, Values::Any(names)) => {
            let mut widened = names.clone();
            for name in names.iter().filter(|name| !known.iter().any(|k| k.eq_ignore_ascii_case(name))) {
                let typed = genre_key(name);
                let same = aliases.names_of(&typed);
                let key: Vec<char> = typed.chars().collect();
                let most = if key.len() > 6 { 2 } else { 1 };
                for candidate in known {
                    let theirs = genre_key(candidate);
                    let other: Vec<char> = theirs.chars().collect();
                    let close = key.len() >= 3 && other.len() >= 3 && distance(&key, &other, most) <= most;
                    if (same.contains(&theirs.as_str()) || close) && !widened.contains(candidate) {
                        widened.push(candidate.clone());
                    }
                }
            }
            Node::Is(Field::Genre, Values::Any(widened))
        }
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_query_is_chips_that_a_click_edits() {
        let text = "genre:jazz,soul -label:\"blue note\" year:1955..1965";
        let (any, chips) = query_chips(text);
        assert!(!any);
        assert_eq!(chips.iter().map(|c| (c.label.as_str(), c.negated)).collect::<Vec<_>>(),
            [("Genre: jazz, soul", false), ("Label: blue note", true), ("Year: 1955–1965", false)]);
        assert_eq!(query_without(text, 1), "genre:jazz,soul year:1955..1965");
        assert_eq!(query_flipped(text, 1), "genre:jazz,soul label:\"blue note\" year:1955..1965");
        assert_eq!(query_flipped(text, 0), "-genre:jazz,soul -label:\"blue note\" year:1955..1965");
        assert_eq!(query_joined(text, true), "genre:jazz,soul | -label:\"blue note\" | year:1955..1965");
        assert_eq!(query_joined(&query_joined(text, true), false), text);
        assert_eq!(query_excluding(text, "label", "Verve"), "genre:jazz,soul -label:\"blue note\",Verve year:1955..1965",
            "a second label kept out joins the first");
        assert_eq!(query_excluding("", "genre", "Pop"), "-genre:Pop");
        assert_eq!(query_excluding("", "album", "12"), "", "a record is chosen in its column, not kept out by id");
        assert_eq!(query_chips("genre:(").1, Vec::new(), "a query that cannot be read has no chips");
    }

    #[test]
    fn a_query_that_cannot_be_read_says_why() {
        assert_eq!(read(""), Ok(None));
        assert!(read("(genre:jazz").unwrap_err().contains("a parenthesis is not closed"));
    }

    #[test]
    fn a_genre_typed_close_to_one_there_is_finds_it() {
        let known: Vec<String> = ["Hip-Hop", "Electronica", "Pop", "Bop", "R&B"].map(String::from).to_vec();
        let widened = |name: &str| match widen_genres(&Node::Is(Field::Genre, Values::Any(vec![name.into()])), &known) {
            Node::Is(_, Values::Any(list)) => list,
            other => panic!("{other:?}"),
        };
        assert_eq!(widened("hiphop"), ["hiphop", "Hip-Hop"]);
        assert_eq!(widened("Electronika"), ["Electronika", "Electronica"]);
        assert_eq!(widened("r and b"), ["r and b", "R&B"]);
        assert_eq!(widened("pop"), ["pop"], "a genre that is there is only itself");
        assert_eq!(widened("Polka"), ["Polka"], "nothing close, nothing added");
    }

    #[test]
    fn a_name_musicbrainz_knows_a_genre_by_finds_the_genre() {
        let aliases = Aliases::read("drum and bass\tdnb\td&b\tdrum n bass\nhip hop\thip-hop\trap music\nlonely\n");
        let known: Vec<String> = ["Drum & Bass", "Hip Hop", "Jungle"].map(String::from).to_vec();
        let widened = |name: &str| match widen_genres_with(&Node::Is(Field::Genre, Values::Any(vec![name.into()])), &known, &aliases) {
            Node::Is(_, Values::Any(list)) => list,
            other => panic!("{other:?}"),
        };
        assert_eq!(widened("dnb"), ["dnb", "Drum & Bass"]);
        assert_eq!(widened("Drum n Bass"), ["Drum n Bass", "Drum & Bass"]);
        assert_eq!(widened("rap music"), ["rap music", "Hip Hop"]);
        assert_eq!(widened("jungle"), ["jungle"], "a genre that is there is only itself");
        assert!(Aliases::read("2 tone\t2-tone\ttwo-tone\n").names == [["2tone", "twotone"]], "names that fold alike are one");
        assert!(Aliases::read("ska\tSKA\n").names.is_empty(), "a genre whose aliases fold to its name adds nothing");
        assert!(bundled().names.iter().all(|names| names.len() >= 2), "the bundled table reads");
    }
}
