//! A query of the Library on the integrated items: each condition holds of
//! an item as the library's SQL holds it of a track (`store::query`).

use super::{SourceItem, Stars};
use chilpa_library::store::query::{Field, Node, Values};

/// Whether `node` holds of the item; `now` is what a relative date counts
/// back from.
pub(crate) fn holds(node: &Node, item: &SourceItem, stars: &Stars, now: i64) -> bool {
    match node {
        Node::All(nodes) => nodes.iter().all(|node| holds(node, item, stars, now)),
        Node::Any(nodes) => nodes.iter().any(|node| holds(node, item, stars, now)),
        Node::Not(node) => !holds(node, item, stars, now),
        Node::Is(field, values) => leaf(*field, values, item, stars, now),
    }
}

fn leaf(field: Field, values: &Values, item: &SourceItem, stars: &Stars, now: i64) -> bool {
    let folded = &item.folded;
    let named = |have: &[String], want: &[String]| want.iter().any(|w| { let w = w.trim().to_lowercase(); have.iter().any(|h| *h == w) });
    // As the full-text index reads them: each word starts a word of the text;
    // a phrase is found whole.
    let worded = |text: &str, want: &[String]| want.iter().map(|w| w.trim().to_lowercase()).filter(|w| !w.is_empty()).all(|w| {
        if w.contains(|c: char| !c.is_alphanumeric()) { return text.contains(&w); }
        text.split(|c: char| !c.is_alphanumeric()).any(|token| token.starts_with(&w))
    });
    let number = |text: &str| text.trim().parse::<i64>().ok();
    let record_stars = i64::from(stars.albums.get(&item.album).copied().unwrap_or(0));
    match (field, values) {
        (Field::Genre, Values::Missing) => folded.genres.is_empty(),
        (Field::Composer, Values::Missing) => folded.composers.is_empty(),
        (Field::Label, Values::Missing) => folded.label.is_empty(),
        (Field::Year, Values::Missing) => item.year.is_none(),
        (Field::Genre, Values::Any(want)) => named(&folded.genres, want),
        (Field::Composer, Values::Any(want)) => named(&folded.composers, want),
        (Field::Label, Values::Any(want)) => named(std::slice::from_ref(&folded.label), want),
        (Field::Artist, Values::Any(want)) => named(&folded.artists, want),
        (Field::Album, Values::Any(want)) => want.iter().any(|name| worded(&folded.album_title, &name.split_whitespace().map(String::from).collect::<Vec<_>>())),
        (Field::Title, Values::Any(want)) => worded(&item.title.to_lowercase(), want),
        (Field::Text, Values::Any(want)) => worded(&folded.text, want),
        (Field::Year, Values::Any(want)) => item.year.is_some_and(|year| want.iter().any(|w| number(w) == Some(i64::from(year)))),
        (Field::Rating, Values::Any(want)) => want.iter().any(|w| number(w) == Some(record_stars)),
        (Field::Plays, Values::Any(want)) => item.plays.is_some_and(|n| want.iter().any(|w| number(w) == Some(n))),
        (Field::Type, Values::Any(want)) => !folded.release_kind.is_empty() && want.iter().any(|w| w == folded.release_kind),
        (Field::Quality, Values::Any(want)) => want.iter().any(|w| match w.as_str() {
            "lossy" => folded.quality == 1, "cd" => folded.quality == 2, "hires" => folded.quality == 3, "dsd" => folded.quality == 4, _ => false,
        }),
        (Field::Year | Field::Rating | Field::Added | Field::Plays, Values::Range(from, to)) => {
            let value = match field { Field::Year => item.year.map(i64::from), Field::Rating => Some(record_stars), Field::Plays => item.plays, _ => Some(item.added_at) };
            value.is_some_and(|v| from.is_none_or(|from| v >= from) && to.is_none_or(|to| v <= to))
        }
        (Field::Added, Values::Within(seconds)) => item.added_at > 0 && item.added_at >= now - seconds,
        _ => false,
    }
}

#[cfg(test)]
mod tests;
