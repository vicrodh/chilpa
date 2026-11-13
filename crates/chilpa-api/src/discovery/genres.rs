//! A small, explicit graph for navigating tags, independent of any catalog.
//! Unknown tags remain roots. Multiple parents are allowed; no suffix guessing.
use crate::library_query::genre_key;
use std::collections::{BTreeMap, HashSet};

const EDGES: &str = include_str!("../../data/genre-tree.tsv");

pub(super) fn children(parent: &str) -> Vec<String> {
    let key = genre_key(parent);
    EDGES.lines().filter_map(|line| line.split_once('\t'))
        .filter(|(p, _)| genre_key(p) == key).map(|(_, child)| child.to_string()).collect()
}

pub(super) fn parents(child: &str) -> Vec<String> {
    let key = genre_key(child);
    EDGES.lines().filter_map(|line| line.split_once('\t'))
        .filter(|(_, c)| genre_key(c) == key).map(|(parent, _)| parent.to_string()).collect()
}

pub(super) fn descendants(parent: &str) -> HashSet<String> {
    let mut found = HashSet::new();
    let mut pending = vec![parent.to_string()];
    while let Some(name) = pending.pop() {
        if found.insert(genre_key(&name)) { pending.extend(children(&name)); }
    }
    found
}

pub(super) fn roots(tags: &[String]) -> Vec<String> {
    let mut roots = BTreeMap::<String, String>::new();
    for tag in tags {
        let mut pending = vec![tag.clone()];
        let mut seen = HashSet::new();
        while let Some(name) = pending.pop() {
            if !seen.insert(genre_key(&name)) { continue; }
            let above = parents(&name);
            if above.is_empty() {
                roots.entry(genre_key(&name)).and_modify(|held| { if name < *held { *held = name.clone(); } }).or_insert(name);
            } else { pending.extend(above); }
        }
    }
    roots.into_values().collect()
}

pub(super) fn matching(parent: &str, tags: &[String]) -> Vec<String> {
    let keys = descendants(parent);
    tags.iter().filter(|tag| keys.contains(&genre_key(tag))).cloned().collect()
}

pub(super) fn breadcrumb(name: &str) -> Vec<String> {
    let mut path = vec![name.to_string()];
    let mut seen = HashSet::from([genre_key(name)]);
    while let Some(parent) = parents(path.last().unwrap()).first() {
        if !seen.insert(genre_key(parent)) { break; }
        path.push(parent.clone());
    }
    path.reverse();
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deep_tags_are_reachable_and_unknown_vocabulary_is_preserved() {
        let tags = vec!["Technical Death Metal".into(), "Local tradition".into()];
        assert!(roots(&tags).contains(&"Metal".into()));
        assert!(roots(&tags).contains(&"Local tradition".into()));
        assert_eq!(matching("Death Metal", &tags), ["Technical Death Metal"]);
        assert!(descendants("Metal").contains(&genre_key("Technical Death Metal")));
        let repeated = vec!["metal".into(), "Heavy Metal".into(), "Metal".into()];
        assert_eq!(roots(&repeated), ["Metal"]);
    }

    #[test]
    fn a_fusion_can_be_reached_through_both_parents_without_cycles() {
        assert!(descendants("Jazz").contains(&genre_key("Jazz Fusion")));
        assert!(descendants("Rock").contains(&genre_key("Jazz Fusion")));
        for line in EDGES.lines() {
            let (parent, child) = line.split_once('\t').unwrap();
            assert_ne!(genre_key(parent), genre_key(child));
            assert!(!descendants(child).contains(&genre_key(parent)), "{line}");
        }
    }
}
