//! I derive Refine rows and positional edits from the query text alone.
mod edits;
mod data;
pub(crate) mod access;
#[cfg(test)]
mod tests;

use super::{read, ValueState};
use chilpa_library::store::query::{Field, Node, Values};
use serde_json::{json, Value};

pub use edits::refine_edit;
pub(crate) use data::{cofactor, has_field};

pub(crate) fn field(name: &str) -> Option<Field> {
    Some(match name {
        "year" => Field::Year, "genre" => Field::Genre, "label" => Field::Label,
        "composer" => Field::Composer, "artist" => Field::Artist, "album" => Field::Album,
        "title" => Field::Title, "words" => Field::Text, "rating" => Field::Rating,
        "added" => Field::Added, "plays" => Field::Plays, "type" => Field::Type,
        "quality" => Field::Quality, _ => return None,
    })
}

pub(crate) fn field_name(field: Field) -> &'static str {
    match field { Field::Text | Field::Words => "words", Field::ExactGenre => "genre", other => other.name() }
}

pub(crate) fn parts(node: Option<Node>) -> (bool, Vec<Node>) {
    match node {
        Some(Node::All(nodes)) => (false, words_together(nodes)), Some(Node::Any(nodes)) => (true, nodes),
        Some(node) => (false, vec![node]), None => (false, vec![]),
    }
}

fn words_together(nodes: Vec<Node>) -> Vec<Node> {
    let mut out=Vec::new(); let mut first=None;
    for node in nodes {
        if let Node::Is(Field::Text,Values::Any(values)) = &node {
            if let Some(at)=first {
                if let Node::Is(_,Values::Any(held))=&mut out[at] { held.extend(values.iter().cloned()); continue; }
            } else { first=Some(out.len()); }
        }
        out.push(node);
    }
    out
}

pub(crate) fn tree(any: bool, mut nodes: Vec<Node>) -> Option<Node> {
    match nodes.len() {
        0 => None, 1 => nodes.pop(), _ => Some(if any { Node::Any(nodes) } else { Node::All(nodes) }),
    }
}

pub(crate) fn printed(any: bool, nodes: Vec<Node>) -> String {
    tree(any, nodes).map(|n| n.to_string()).unwrap_or_default()
}

pub(crate) fn group(node: &Node) -> Option<(bool, Vec<Node>)> {
    match node {
        Node::Group(inner) => {
            let (any, nodes) = parts(Some(*inner.clone()));
            Some((any || !matches!(inner.as_ref(),Node::All(_) | Node::Any(_)), nodes))
        }
        Node::All(nodes) => Some((false, nodes.clone())),
        Node::Any(nodes) => Some((true, nodes.clone())), _ => None,
    }
}

pub(crate) fn grouped(any: bool, nodes: Vec<Node>) -> Option<Node> {
    tree(any, nodes).map(|n| Node::Group(Box::new(n)))
}

fn row(node: &Node, id: String, nested: bool) -> Option<Value> {
    if let Some((any, nodes)) = group(node) {
        if nested || nodes.is_empty() { return None; }
        let rows: Option<Vec<_>> = nodes.iter().enumerate().map(|(i, n)| row(n, format!("{id}.{i}"), true)).collect();
        return Some(json!({"id":id,"group":true,"mode":if any {"any"} else {"all"},"rows":rows?}));
    }
    let (negated, inner) = match node { Node::Not(inner) => (true, inner.as_ref()), other => (false, other) };
    let Node::Is(f, values) = inner else { return None };
    let mut out = json!({"id":id,"field":field_name(*f)});
    match (f, values) {
        (Field::Text | Field::Words | Field::Album | Field::Title, Values::Any(values)) if !negated => {
            out["kind"] = json!("words"); out["values"] = json!(values);
        }
        (Field::Rating, values) if !negated && rating(values).is_some() => {
            let (min, max) = rating(values)?;
            out["kind"] = json!("rating"); out["min"] = json!(min); out["max"] = json!(max);
        }
        (Field::Year | Field::Plays, Values::Range(from, to)) if !negated => {
            if from.zip(*to).is_some_and(|(a,b)| a > b) { return None; }
            out["kind"] = json!("range"); out["from"] = json!(from); out["to"] = json!(to);
        }
        (Field::Added, Values::Within(seconds)) if !negated => {
            let days = seconds / 86_400;
            let (n, unit) = [(365,"y"),(30,"m"),(7,"w"),(1,"d")].into_iter()
                .find_map(|(size, unit)| (days % size == 0).then_some((days / size, unit)))?;
            out["kind"] = json!("added"); out["n"] = json!(n); out["unit"] = json!(unit);
        }
        (_, Values::Missing) if !negated => { out["kind"] = json!("none"); }
        (Field::Genre | Field::ExactGenre | Field::Artist | Field::Composer | Field::Label | Field::Year | Field::Album | Field::Rating | Field::Plays | Field::Type | Field::Quality, Values::Any(values)) => {
            out["kind"] = json!("list"); out["negated"] = json!(negated);
            out["values"] = if *f == Field::Plays { json!(values.iter().map(|s| s.parse::<i64>()).collect::<Result<Vec<_>,_>>().ok()?) } else { json!(values) };
        }
        _ => return None,
    }
    if *f == Field::ExactGenre { out["exact"] = json!(true); }
    Some(out)
}

pub(crate) fn rating(values: &Values) -> Option<(i64, i64)> {
    let (min, max) = match values {
        Values::Range(a,b) => (a.unwrap_or(0), b.unwrap_or(5)),
        Values::Any(values) if values.len() == 1 => { let n = values[0].parse().ok()?; (n,n) }
        _ => return None,
    };
    ((0..=5).contains(&min) && (min..=5).contains(&max)).then_some((min,max))
}

/// I return either editable rows or the applied query as text.
pub fn refine(text: &str) -> Value {
    let parsed = read(text);
    let (any, nodes) = parts(parsed.clone().ok().flatten());
    let rows: Option<Vec<_>> = nodes.iter().enumerate().map(|(i,n)| row(n, i.to_string(), false)).collect();
    if let (Ok(_), Some(rows)) = (&parsed, rows) {
        json!({"mode":if any {"any"} else {"all"},"rows":rows,"complex":null})
    } else {
        let message = parsed.as_ref().err().cloned().unwrap_or_else(||
            chilpa_i18n::t(chilpa_i18n::mark("I keep this filter applied; edit its text to change it.")));
        let text = parsed.ok().flatten().map(|n| n.to_string()).unwrap_or_else(|| text.into());
        json!({"mode":if any {"any"} else {"all"},"rows":[],"complex":{"text":text,"message":message}})
    }
}

/// Positions are Unicode characters in the query, without the command prefix.
pub fn check(text: &str) -> Value {
    match chilpa_library::store::query::parse(text) {
        Ok(_) => match read(text) {
            Ok(_) => json!({"ok":true}),
            Err(message) => json!({"ok":false,"at":0,"message":message}),
        },
        Err(error) => json!({"ok":false,"at":error.at,"message":chilpa_i18n::t(error.what)}),
    }
}

pub(crate) fn state(node: &Node, f: Field, value: &str) -> ValueState {
    match node {
        Node::All(nodes) | Node::Any(nodes) => nodes.iter().map(|n| state(n, f, value)).max_by_key(|s| match s { ValueState::None => 0, ValueState::Include => 1, ValueState::Exclude => 2 }).unwrap_or(ValueState::None),
        Node::Group(inner) => state(inner, f, value),
        Node::Not(inner) => match state(inner, f, value) {
            ValueState::Include => ValueState::Exclude, ValueState::Exclude => ValueState::Include, other => other,
        },
        Node::Is(other, Values::Any(values)) if field_name(*other) == field_name(f) && values.iter().any(|v| v.to_lowercase() == value.to_lowercase()) => ValueState::Include,
        Node::Is(other, Values::Range(from, to)) if *other == f && value.parse::<i64>().is_ok_and(|v| from.is_none_or(|a| v >= a) && to.is_none_or(|b| v <= b)) => ValueState::Include,
        _ => ValueState::None,
    }
}
