//! Conditions stacked with AND, OR and NOT, and the SQL they make.
//!
//! A query is a tree: every condition of a group (`All`), any of them
//! (`Any`), the opposite of one (`Not`), and at the leaves a field and the
//! values it holds. The text a command line takes and the chips a screen
//! shows are two ways of writing the same tree (`parse`, `Display`).
//!
//! Compiled, every value is a `?` parameter and every name of a column comes
//! from this file, never from the text. Each leaf is a set of track ids read
//! from an index (`t.id IN (SELECT ...)`), not a lookup per track: on 34,000
//! tracks one genre was 20 ms that way and 125 ms as a correlated EXISTS. An
//! `Any` whose members are all leaves is one `UNION` of their sets, which
//! keeps each branch on its own index (6 ms against 86 ms for the OR). Words
//! go to the full-text index: "love" in 5.5 ms, against 121 ms as LIKE.

mod parse;
mod buckets;
mod fold;
pub use fold::fold;
pub(crate) use buckets::register;
#[cfg(test)]
mod tests;

pub use parse::{parse, parse_grouped, ParseError};

use rusqlite::types::ToSql;

/// What a condition looks at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Field {
    Year,
    Genre,
    Label,
    Composer,
    /// Who the record is credited to or who plays on it.
    Artist,
    /// Words of the record's title.
    Album,
    /// Words of the track's title.
    Title,
    /// The record's own stars, 0 for none.
    Rating,
    /// Completed listens, as the track list counts them.
    Plays,
    /// The record's release kind.
    Type,
    /// The record's best quality tier.
    Quality,
    /// When the track came into the library.
    Added,
    /// Words anywhere in the title, the artist or the record.
    Text,
    /// Explicit words, never resolved to a library name.
    Words,
    /// A genre without alias or spelling expansion.
    ExactGenre,
}

impl Field {
    pub(crate) const NAMES: &'static [(&'static str, Field)] = &[
        ("year", Field::Year), ("y", Field::Year), ("genre", Field::Genre), ("g", Field::Genre),
        ("label", Field::Label), ("l", Field::Label), ("composer", Field::Composer), ("c", Field::Composer),
        ("artist", Field::Artist), ("a", Field::Artist), ("album", Field::Album), ("al", Field::Album),
        ("title", Field::Title), ("t", Field::Title), ("rating", Field::Rating), ("r", Field::Rating),
        ("added", Field::Added), ("plays", Field::Plays), ("p", Field::Plays),
        ("type", Field::Type), ("ty", Field::Type), ("quality", Field::Quality), ("q", Field::Quality),
        ("words", Field::Words), ("genre=", Field::ExactGenre),
    ];

    /// The name a query is printed with.
    pub fn name(self) -> &'static str {
        match self {
            Field::Year => "year", Field::Genre => "genre", Field::Label => "label", Field::Composer => "composer",
            Field::Artist => "artist", Field::Album => "album", Field::Title => "title", Field::Rating => "rating",
            Field::Added => "added", Field::Text => "",
            Field::Words => "words", Field::ExactGenre => "genre=",
            Field::Plays => "plays", Field::Type => "type", Field::Quality => "quality",
        }
    }
}

/// What a field has to hold.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Values {
    /// Any of these: names, years or stars.
    Any(Vec<String>),
    /// A number between two bounds, both included; a missing bound is open.
    Range(Option<i64>, Option<i64>),
    /// Within this many seconds before now (`added:30d`).
    Within(i64),
    /// Nothing at all: no genre, no label, no year.
    Missing,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Node {
    /// Explicit parentheses retained for editors; evaluation ignores them.
    Group(Box<Node>),
    All(Vec<Node>),
    Any(Vec<Node>),
    Not(Box<Node>),
    Is(Field, Values),
}

/// How many conditions a query may hold, and how deep its groups may go:
/// enough for anyone typing, short of a statement SQLite would refuse.
pub const MOST_CONDITIONS: usize = 64;
pub const DEEPEST: usize = 8;

type Args = Vec<Box<dyn ToSql>>;

fn marks(n: usize) -> String {
    vec!["?"; n].join(",")
}

const ALBUM_STARS: &str = "COALESCE((SELECT stars FROM album_ratings WHERE album_id = a.id), 0)";

impl Node {
    /// I execute the same simplified tree with either parser entry.
    pub fn ungrouped(&self) -> Node { parse::simplify(self.clone()) }

    /// The `WHERE` condition this tree makes over `t` (tracks) and `a`
    /// (albums), with its parameters in order; `now` is the Unix time a
    /// relative date counts back from.
    pub fn compile(&self, now: i64) -> (String, Args) {
        let mut args = Args::new();
        let sql = self.ungrouped().predicate(now, &mut args);
        (sql, args)
    }

    fn predicate(&self, now: i64, args: &mut Args) -> String {
        match self {
            Node::Group(node) => node.predicate(now, args),
            Node::All(nodes) if nodes.is_empty() => "1".into(),
            Node::Any(nodes) if nodes.is_empty() => "0".into(),
            Node::All(nodes) => format!("({})", nodes.iter().map(|n| n.predicate(now, args)).collect::<Vec<_>>().join(" AND ")),
            Node::Any(nodes) if nodes.iter().all(|n| matches!(n, Node::Is(..))) => {
                let sets: Vec<String> = nodes.iter().map(|n| match n { Node::Is(field, values) => set(*field, values, now, args), _ => unreachable!() }).collect();
                format!("t.id IN ({})", sets.join(" UNION "))
            }
            Node::Any(nodes) => format!("({})", nodes.iter().map(|n| n.predicate(now, args)).collect::<Vec<_>>().join(" OR ")),
            Node::Not(node) => format!("NOT {}", node.predicate(now, args)),
            Node::Is(field, values) => format!("t.id IN ({})", set(*field, values, now, args)),
        }
    }

    /// How many leaves and how many levels the tree has.
    pub fn size(&self) -> (usize, usize) {
        match self {
            Node::Group(node) => { let (n, d) = node.size(); (n, d + 1) }
            Node::Is(..) => (1, 1),
            Node::Not(node) => { let (n, d) = node.size(); (n, d + 1) }
            Node::All(nodes) | Node::Any(nodes) => {
                let (n, d) = nodes.iter().map(Node::size).fold((0, 0), |(n, d), (m, e)| (n + m, d.max(e)));
                (n, d + 1)
            }
        }
    }
}

/// The ids of the tracks that hold a field's values, read from its index.
fn set(field: Field, values: &Values, now: i64, args: &mut Args) -> String {
    let tracks = "SELECT t.id FROM tracks t JOIN albums a ON a.id = t.album_id WHERE";
    match (field, values) {
        (Field::Plays, Values::Any(_) | Values::Range(..)) => buckets::plays(values, args),
        (Field::Type | Field::Quality, Values::Any(names)) => buckets::records(field, names, args),
        (Field::Genre | Field::ExactGenre, Values::Missing) => "SELECT id FROM tracks WHERE id NOT IN (SELECT track_id FROM track_genres)".into(),
        (Field::Composer, Values::Missing) => "SELECT id FROM tracks WHERE id NOT IN (SELECT track_id FROM track_composers)".into(),
        (Field::Label, Values::Missing) => "SELECT id FROM tracks WHERE coalesce(label, '') = ''".into(),
        (Field::Year, Values::Missing) => format!("{tracks} a.year IS NULL"),
        (Field::Genre | Field::ExactGenre, Values::Any(names)) => {
            args.extend(names.iter().map(|n| Box::new(n.clone()) as Box<dyn ToSql>));
            format!("SELECT tg.track_id FROM track_genres tg JOIN genres g ON g.id = tg.genre_id WHERE g.name COLLATE NOCASE IN ({})", marks(names.len()))
        }
        (Field::Composer, Values::Any(names)) => {
            args.extend(names.iter().map(|n| Box::new(n.clone()) as Box<dyn ToSql>));
            format!("SELECT tc.track_id FROM track_composers tc JOIN composers c ON c.id = tc.composer_id WHERE c.name COLLATE NOCASE IN ({})", marks(names.len()))
        }
        (Field::Label, Values::Any(names)) => {
            args.extend(names.iter().map(|n| Box::new(n.clone()) as Box<dyn ToSql>));
            format!("SELECT id FROM tracks WHERE label COLLATE NOCASE IN ({})", marks(names.len()))
        }
        (Field::Artist, Values::Any(names)) => {
            args.extend(names.iter().map(|n| Box::new(n.clone()) as Box<dyn ToSql>));
            format!("SELECT id FROM tracks WHERE album_id IN (SELECT album_id FROM album_artist_rollup WHERE artist COLLATE NOCASE IN ({}))", marks(names.len()))
        }
        (Field::Album | Field::Title | Field::Text | Field::Words, Values::Any(words)) => {
            // Every word starts a word of the title, the artist or the
            // record, accents aside: the full-text index answers in a few
            // milliseconds what LIKE '%word%' reads every track for.
            let columns = match field { Field::Album => "album", Field::Title => "title", _ => "title artist album" };
            let words: Vec<String> = words.iter().map(|w| w.trim()).filter(|w| !w.is_empty())
                .map(|w| {
                    let quote = |word: &str| format!("\"{}\"*", word.replace('"', "\"\""));
                    if field == Field::Album { format!("({})", w.split_whitespace().map(quote).collect::<Vec<_>>().join(" ")) }
                    else { quote(w) }
                }).collect();
            if words.is_empty() { return format!("{tracks} 1"); }
            let join = if field == Field::Album { " OR " } else { " " };
            args.push(Box::new(format!("{{{columns}}} : ({})", words.join(join))));
            "SELECT rowid FROM track_search WHERE track_search MATCH ?".into()
        }
        (Field::Year, Values::Any(years)) => {
            let years: Vec<i64> = years.iter().filter_map(|y| y.trim().parse().ok()).collect();
            let sql = format!("{tracks} a.year IN ({})", marks(years.len()));
            args.extend(years.into_iter().map(|y| Box::new(y) as Box<dyn ToSql>));
            sql
        }
        (Field::Rating, Values::Any(stars)) => {
            let stars: Vec<i64> = stars.iter().filter_map(|s| s.trim().parse().ok()).collect();
            let sql = format!("{tracks} {ALBUM_STARS} IN ({})", marks(stars.len()));
            args.extend(stars.into_iter().map(|s| Box::new(s) as Box<dyn ToSql>));
            sql
        }
        (Field::Year | Field::Rating | Field::Added, Values::Range(from, to)) => {
            let column = match field { Field::Year => "a.year", Field::Rating => ALBUM_STARS, _ => "t.added_at" };
            let mut parts = Vec::new();
            if let Some(from) = from { parts.push(format!("{column} >= ?")); args.push(Box::new(*from)); }
            if let Some(to) = to { parts.push(format!("{column} <= ?")); args.push(Box::new(*to)); }
            if parts.is_empty() { parts.push(format!("{column} IS NOT NULL")); }
            format!("{tracks} {}", parts.join(" AND "))
        }
        (Field::Added, Values::Within(seconds)) => {
            args.push(Box::new(now - seconds));
            format!("{tracks} t.added_at >= ?")
        }
        // A field that cannot hold such values holds nothing.
        _ => "SELECT NULL WHERE 0".into(),
    }
}
