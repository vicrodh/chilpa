//! I project column roles and edits from the Library's one query.
use super::{read, refine::{self, field, field_name}, ValueState};
use crate::{FacetEntry, Facets, Filter};
use chilpa_library::store::query::Node;
use serde_json::json;

#[derive(Default)]
pub struct ColumnSelection {
    pub included: Vec<String>,
    pub excluded: Vec<String>,
    pub active: bool,
}

pub fn entries(facets: &Facets, name: &str) -> Vec<FacetEntry> {
    match name {
        "year" => &facets.years, "genre" => &facets.genres, "label" => &facets.labels,
        "composer" => &facets.composers, "rating" => &facets.ratings,
        "artist" => &facets.artists, "album" => &facets.albums, _ => return vec![],
    }.clone()
}

fn value<'a>(name: &str, entry: &'a FacetEntry) -> &'a str {
    if name == "album" { &entry.label } else { &entry.key }
}

pub fn selection(filter: &Filter, name: &str, entries: &[FacetEntry]) -> ColumnSelection {
    let mut result = ColumnSelection::default();
    let (Some(query), Some(f)) = (&filter.query, field(name)) else { return result };
    result.active = refine::has_field(query, f);
    for entry in entries {
        match refine::state(query, f, value(name, entry)) {
            ValueState::Include => result.included.push(entry.key.clone()),
            ValueState::Exclude => result.excluded.push(entry.key.clone()),
            ValueState::None => {}
        }
    }
    result
}

/// I preserve groups when clearing a field, including its exclusions.
pub(crate) fn without_field(node: Node, field: chilpa_library::store::query::Field) -> Option<Node> {
    strip_field(node, field, false, false)
}

fn strip_field(node: Node, field: chilpa_library::store::query::Field, keep_excluded: bool, negated: bool) -> Option<Node> {
    let strip = |n| strip_field(n, field, keep_excluded, negated);
    match node {
        Node::Is(f, _) if field_name(f) == field_name(field) && !(keep_excluded && negated) => None,
        Node::All(nodes) => refine::tree(false, nodes.into_iter().filter_map(strip).collect()),
        Node::Any(nodes) => refine::tree(true, nodes.into_iter().filter_map(strip).collect()),
        Node::Group(inner) => strip(*inner).map(|n| Node::Group(Box::new(n))),
        Node::Not(inner) => strip_field(*inner, field, keep_excluded, !negated).map(|n| Node::Not(Box::new(n))),
        other => Some(other),
    }
}

fn edit(filter: &mut Filter, action: serde_json::Value) {
    let text = filter.query.as_ref().map(ToString::to_string).unwrap_or_default();
    let edited = refine::refine_edit(&text, &action.to_string());
    if let Ok(query) = read(&edited) { filter.query = query; }
}

pub fn clear(filter: &mut Filter, name: &str) {
    if let Some(f) = field(name) { filter.query = filter.query.take().and_then(|n| without_field(n, f)); }
}

/// Plain/Ctrl/Cmd toggle; Shift replaces the include span, Ctrl+Shift adds it.
/// The anchor belongs to the visible list and is reset when its entries change.
pub fn choose(filter: &mut Filter, name: &str, entries: &[FacetEntry], anchor: Option<usize>, row: usize, modifiers: i32) -> Option<usize> {
    if row == 0 {
        if modifiers & 4 == 0 { clear(filter, name); }
        return None;
    }
    let Some(entry) = entries.get(row - 1) else { return anchor };
    let selected = selection(filter, name, entries);
    if modifiers & 4 != 0 {
        let state = if selected.excluded.contains(&entry.key) { "none" } else { "exclude" };
        edit(filter, json!({"op":"value","field":name,"value":value(name,entry),"state":state}));
        return anchor;
    }
    if modifiers & 2 == 0 {
        let state = if selected.included.contains(&entry.key) { "none" } else { "include" };
        edit(filter, json!({"op":"value","field":name,"value":value(name,entry),"state":state}));
        return Some(row - 1);
    }
    let (keys, anchor) = crate::select_facets(entries, &selected.included, anchor.filter(|i| *i < entries.len()), row, modifiers);
    if modifiers & 1 == 0 {
        if let Some(f) = field(name) { filter.query = filter.query.take().and_then(|n| strip_field(n, f, true, false)); }
    }
    for entry in entries.iter().filter(|e| keys.contains(&e.key)) {
        edit(filter, json!({"op":"value","field":name,"value":value(name,entry),"state":"include"}));
    }
    anchor
}

#[cfg(test)]
mod tests;
