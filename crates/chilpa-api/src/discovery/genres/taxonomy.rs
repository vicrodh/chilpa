//! Immutable, bundled vocabulary. Initialized only on first genre navigation.
//! Fewer than 4096 nodes, 8192 aliases and 2048 edges; no library data is cached.
use crate::library_query::genre_key;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::OnceLock;

pub(super) const ALIASES_VERSION: &str = "musicbrainz-aliases-757ba0416924";
pub(super) const VERSION: &str = "chilpa-1+beets-2.14.1+musicbrainz-aliases-757ba0416924";
const LOCAL: &str = include_str!("../../../data/genre-tree.tsv");
const BEETS: &str = include_str!("../../../data/genre-beets.tsv");
const BEETS_MAP: &str = include_str!("../../../data/genre-beets-map.tsv");
const ALIASES: &str = include_str!("../../../data/genre-aliases.tsv");

pub(super) struct Edge { pub node: usize, pub source: &'static str, pub remapped: bool }
pub(super) struct Node {
    pub name: &'static str,
    pub letters: Vec<char>,
    pub parents: Vec<Edge>,
    pub children: Vec<usize>,
}
pub(super) struct Graph {
    pub nodes: Vec<Node>,
    pub by_key: BTreeMap<String, usize>,
    pub aliases: HashMap<String, Vec<usize>>,
}

impl Graph {
    fn add(&mut self, name: &'static str) -> usize {
        let key = genre_key(name);
        if let Some(id) = self.by_key.get(&key) { return *id; }
        let id = self.nodes.len();
        self.nodes.push(Node { name, letters:key.chars().collect(), parents:Vec::new(), children:Vec::new() });
        self.by_key.insert(key, id);
        id
    }

    fn edges(&mut self, text: &'static str, source: &'static str, mappings: &[(&'static str, &'static str)]) {
        for (parent, child) in text.lines().filter(|line| !line.starts_with('#')).filter_map(|line| line.split_once('\t')) {
            let rename = |name| mappings.iter().find(|(from, _)| *from == name).map_or(name, |(_, to)| *to);
            let remapped = rename(parent) != parent || rename(child) != child;
            let (parent, child) = (rename(parent), rename(child));
            let (parent, child) = (self.add(parent), self.add(child));
            if !self.nodes[parent].children.contains(&child) {
                self.nodes[parent].children.push(child);
                self.nodes[child].parents.push(Edge { node:parent, source, remapped });
            }
        }
    }

    fn read() -> Self {
        let mut graph = Self { nodes:Vec::new(), by_key:BTreeMap::new(), aliases:HashMap::new() };
        // Explicit local presentation parents take precedence in breadcrumbs.
        graph.edges(LOCAL, "chilpa-1", &[]);
        let mappings: Vec<_> = BEETS_MAP.lines().filter(|line| !line.starts_with('#')).filter_map(|line| line.split_once('\t')).collect();
        graph.edges(BEETS, "beets-2.14.1", &mappings);
        for line in ALIASES.lines() {
            let names: Vec<_> = line.split('\t').collect();
            let keys: Vec<_> = names.iter().map(|name| genre_key(name)).collect();
            let mut targets: Vec<_> = keys.iter().filter_map(|key| graph.by_key.get(key).copied()).collect();
            targets.sort_unstable(); targets.dedup();
            if targets.is_empty() { targets.push(graph.add(names[0])); }
            // An alias shared by several concepts stays ambiguous.
            for key in keys {
                let held = graph.aliases.entry(key).or_default();
                held.extend(&targets); held.sort_unstable(); held.dedup();
            }
        }
        graph
    }

    pub fn walk(&self, id: usize, up: bool) -> HashSet<usize> {
        let mut found = HashSet::new();
        let mut pending = vec![id];
        while let Some(id) = pending.pop() {
            if found.insert(id) {
                if up { pending.extend(self.nodes[id].parents.iter().map(|edge| edge.node)); }
                else { pending.extend(&self.nodes[id].children); }
            }
        }
        found
    }
}

pub(super) fn graph() -> &'static Graph {
    static GRAPH: OnceLock<Graph> = OnceLock::new();
    GRAPH.get_or_init(Graph::read)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pinned_graph_has_bounded_storage_and_no_cycles() {
        use sha2::{Digest, Sha256};
        let digest = format!("{:x}", Sha256::digest(ALIASES));
        assert!(ALIASES_VERSION.ends_with(&digest[..12]));
        assert!(BEETS.lines().next().unwrap().contains("beets 2.14.1"));
        let graph = graph();
        assert!(graph.nodes.len() < 4096);
        assert!(graph.aliases.len() < 8192);
        assert!(graph.nodes.iter().map(|node| node.children.len()).sum::<usize>() < 2048);
        for (id, node) in graph.nodes.iter().enumerate() {
            for parent in &node.parents {
                assert!(!graph.walk(id, false).contains(&parent.node), "{} -> {}", graph.nodes[parent.node].name, node.name);
                assert!(!parent.source.is_empty());
            }
        }
    }
}
