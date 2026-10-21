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

mod focus;
pub(crate) mod refine;
mod notes;
pub use notes::EngineNote;
pub use refine::{check, refine, refine_edit};
pub(crate) use focus::facet_filter;
pub use focus::{focus_rows, with_minimum, with_missing, with_range, with_value, with_within, FocusRow, FocusRows, ValueState};

use chilpa_i18n as i18n;
use chilpa_library::store::query::{parse_grouped as parse, Field, Node, Values};
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

/// What `:help filter` shows under its usage: one line for each thing a
/// query can say, in the order a listener learns them.
pub(crate) const EXAMPLES: &[(&str, &str)] = &[
    (":filter genre:jazz", chilpa_i18n::mark("Records of that genre")),
    (":filter genre:jazz,soul", chilpa_i18n::mark("Either genre: a comma means or")),
    (":filter -label:\"Blue Note\"", chilpa_i18n::mark("Leave a label out: a minus means not")),
    (":filter year:1985", chilpa_i18n::mark("Records from that year")),
    (":filter year:1980,1985,1990", chilpa_i18n::mark("Any of those years")),
    (":filter year:1980..1989", chilpa_i18n::mark("From one year to another, both included")),
    (":filter year:1990..", chilpa_i18n::mark("From that year on")),
    (":filter year:..1969", chilpa_i18n::mark("Up to that year")),
    (":filter year:none", chilpa_i18n::mark("No year; genre, label and composer take none too")),
    (":filter artist:\"Bill Evans\"", chilpa_i18n::mark("Quotes keep a name with spaces or commas whole")),
    (":filter composer:monk", chilpa_i18n::mark("Records with that composer")),
    (":filter album:blue title:train", chilpa_i18n::mark("Words in the record title and in the track title")),
    (":filter rating:4..", chilpa_i18n::mark("Four stars or more; rating:0 has none")),
    (":filter plays:0", chilpa_i18n::mark("Never played; plays:1.. means played at least once")),
    (":filter type:single,ep", chilpa_i18n::mark("Singles or EPs, using the release type grouping")),
    (":filter quality:cd,hires", chilpa_i18n::mark("Lossless or Hi-Res records; quality also takes lossy or dsd")),
    (":filter added:30d", chilpa_i18n::mark("Added in the last 30 days: d, w, m or y")),
    (":filter genre:rock year:1970..1979", chilpa_i18n::mark("Side by side, all must hold")),
    (":filter genre:jazz | composer:monk", chilpa_i18n::mark("A bar or OR: either is enough")),
    (":filter (genre:jazz | genre:blues) -year:1990..", chilpa_i18n::mark("Parentheses group; NOT works like the minus")),
    (":filter y:1960..1969 g:soul", chilpa_i18n::mark("Short names: y g l c a al t r p ty q")),
    (":filter iron maiden", chilpa_i18n::mark("Words alone look in titles, artists and records")),
    (r#":filter words:"A. Smith" genre=:dnb"#, chilpa_i18n::mark("Keep words literal and match a genre without expanding it")),
    (":filter", chilpa_i18n::mark("Clear the filter")),
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
        Field::Year => "Year", Field::Genre | Field::ExactGenre => "Genre", Field::Label => "Label", Field::Composer => "Composer",
        Field::Artist => "Artist", Field::Album => "Album", Field::Title => "Title", Field::Rating => "Rating",
        Field::Added => "Added", Field::Text | Field::Words => "Words",
        Field::Plays => i18n::mark("Plays"), Field::Type => i18n::mark("Release type"), Field::Quality => i18n::mark("Quality"),
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
        Node::Not(inner) | Node::Group(inner) => names_genres(inner),
        Node::Is(field, values) => *field == Field::Genre && matches!(values, Values::Any(_)),
    }
}

/// The query with every genre it names that `known` does not have read
/// also as the known ones close to it.
pub(crate) fn widen_genres(node: &Node, known: &[String]) -> Node {
    widen_genres_with(node, known, bundled())
}

pub(crate) fn widen_genres_with(node: &Node, known: &[String], aliases: &Aliases) -> Node {
    notes::widen(node, known, aliases, &mut Vec::new(), &[])
}

/// Whether the query has words to look for anywhere.
pub(crate) fn names_words(node: &Node) -> bool {
    match node {
        Node::All(nodes) | Node::Any(nodes) => nodes.iter().any(names_words),
        Node::Not(inner) | Node::Group(inner) => names_words(inner),
        Node::Is(field, _) => *field == Field::Text,
    }
}

/// Words that, together, are a name the library has, for an artist, a
/// composer, a genre or a label, are read as that name, the way a click on
/// it in its column picks it: `A. Smith` finds the composer "A. Smith" and
/// "A Smith". Words that name nothing stay words.
pub(crate) fn resolve_names(node: &Node, known: &[(Field, Vec<String>)]) -> Node {
    notes::resolve(node, known, &mut Vec::new(), &[])
}

/// The names a query's words may be in the library: its artists,
/// composers, genres and labels, as their columns take them.
pub(crate) fn library_names(lib: &chilpa_library::Library) -> Vec<(Field, Vec<String>)> {
    fn keys<E>(entries: Result<Vec<chilpa_library::FacetEntry>, E>) -> Vec<String> {
        entries.map(|entries| entries.into_iter().map(|entry| entry.key).collect()).unwrap_or_default()
    }
    let all = chilpa_library::Filter::default();
    vec![(Field::Artist, keys(lib.facet_artists(&all))), (Field::Composer, keys(lib.facet_composers(&all))),
        (Field::Genre, keys(lib.facet_genres(&all))), (Field::Label, keys(lib.facet_labels(&all)))]
}

impl crate::Chilpa {
    /// I retain the typed words so Refine can explain and undo name resolution.
    /// Reader resolves them when applying the query, using the same vocabulary.
    pub fn query_with_names(&mut self, text: &str) -> String { text.to_string() }
}

/// The names `words` say together, in each field that has them.
fn named(words: &[&Node], known: &[(Field, Vec<String>)]) -> Option<Node> {
    let phrase: Vec<&str> = words.iter().flat_map(|node| match node {
        Node::Is(_, Values::Any(words)) => words.iter().map(String::as_str).collect(),
        _ => Vec::new(),
    }).collect();
    let key = genre_key(&phrase.join(" "));
    if key.is_empty() { return None; }
    let mut found: Vec<Node> = known.iter().filter_map(|(field, names)| {
        let same: Vec<String> = names.iter().filter(|name| genre_key(name) == key).cloned().collect();
        (!same.is_empty()).then(|| Node::Is(*field, Values::Any(same)))
    }).collect();
    match found.len() { 0 => None, 1 => found.pop(), _ => Some(Node::Any(found)) }
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
    fn words_that_are_a_name_the_library_has_are_read_as_that_name() {
        let known = vec![(Field::Composer, vec!["A Smith".to_string(), "A. Smith".into(), "Adrian Smith".into()]),
            (Field::Artist, vec!["Iron Maiden".to_string()]), (Field::Genre, vec!["Heavy Metal".to_string()])];
        let resolved = |text: &str| resolve_names(&read(text).unwrap().unwrap(), &known).to_string();
        assert_eq!(resolved("A. Smith"), "composer:\"A Smith\",\"A. Smith\"", "both spellings, as the column picks them");
        assert_eq!(resolved("\"A. Smith\""), "composer:\"A Smith\",\"A. Smith\"");
        assert_eq!(resolved("year:1990..1999 iron maiden"), "year:1990..1999 artist:\"Iron Maiden\"");
        assert_eq!(resolved("heavy metal | jazz"), "genre:\"Heavy Metal\" | jazz", "each side of an or on its own");
        assert_eq!(resolved("smith"), "smith", "a part of a name stays words");
        assert!(names_words(&read("genre:jazz smith").unwrap().unwrap()));
        assert!(!names_words(&read("genre:jazz").unwrap().unwrap()));
    }

    #[test]
    fn an_applied_query_keeps_the_words_its_notes_explain() {
        let lib = chilpa_library::Library::in_memory().unwrap();
        let source = lib.source_id("local", "default").unwrap();
        let album = lib.source_edition(source, "record", "Record", "Band", Some(2000)).unwrap();
        for (id, title) in [(1, "One"), (2, "Two")] {
            lib.connection().execute("INSERT INTO tracks(id, source_id, external_id, title, artist, album_id, duration_ms, added_at)
                VALUES (?1, ?2, ?3, ?4, 'Band', ?5, 1000, 1)", rusqlite::params![id, source, format!("/{id}.flac"), title, album]).unwrap();
        }
        lib.set_track_composers(1, &["A. Smith".to_string()]).unwrap();
        lib.set_track_composers(2, &["A Smith".to_string()]).unwrap();
        let mut api = crate::Chilpa::new();
        api.library = Some(lib);
        let text=api.query_with_names("A. Smith");
        assert_eq!(text,"A. Smith");
        let reader=crate::Reader::of(api.library.take().unwrap());
        assert_eq!(reader.refine_notes(&text).len(),1);
        let filter=chilpa_library::Filter {query:read(&text).unwrap(),..Default::default()};
        assert_eq!(reader.refresh(&filter,crate::TrackOrder::Artist,|| true).unwrap().1.totals.tracks,2);
        assert_eq!(api.query_with_names("genre:jazz"), "genre:jazz", "no words, nothing to read");
        assert_eq!(api.query_with_names("(genre:"), "(genre:", "what cannot be read is left to say why");
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
