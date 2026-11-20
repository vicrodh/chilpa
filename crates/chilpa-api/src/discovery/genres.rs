//! Derived navigation over original tags. Nothing here writes library metadata.
//! Per-query groups contain tag indices, released with the read-only request.
mod resolution;
mod taxonomy;
mod distance;
mod art;
pub(super) use art::art;
pub(crate) use art::digest as art_digest;
use resolution::resolve;
pub(super) use resolution::Resolution;
use taxonomy::graph;
use std::collections::{HashMap, HashSet};
pub(super) const VERSION: &str = taxonomy::VERSION;

pub(super) fn strong_identity(original: &str) -> String {
    link(original).0
}

pub(super) fn link(original: &str) -> (String, &str) {
    let key = crate::library_query::genre_key(original);
    let graph = graph();
    let known = graph.by_key.get(&key).copied().or_else(||
        graph.aliases.get(&key).filter(|ids| ids.len() == 1).map(|ids| ids[0]));
    match known {
        Some(node) => (format!("known:{}", graph.nodes[node].name), graph.nodes[node].name),
        None => (format!("literal:{}", if key.is_empty() { original.to_lowercase() } else { key }), original),
    }
}

#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(super) enum Identity { Known(usize), Literal(String) }
impl Identity {
    fn literal(original: &str) -> Self {
        let key = crate::library_query::genre_key(original);
        Self::Literal(if key.is_empty() { original.to_lowercase() } else { key })
    }
}
impl Resolution {
    fn identity(&self) -> Identity {
        self.node.map(Identity::Known).unwrap_or_else(|| Identity::literal(&self.original))
    }
}

fn matches(name: &str, text: &str) -> bool {
    let key = crate::library_query::genre_key(text);
    !key.is_empty() && crate::library_query::genre_key(name).contains(&key) || chilpa_library::store::query::fold(name).contains(text)
}

pub(super) struct Inventory {
    records: Vec<Resolution>,
    groups: HashMap<Identity, Vec<usize>>,
}

impl Inventory {
    pub fn new(tags: &[String]) -> Self {
        let records: Vec<_> = tags.iter().map(|tag| resolve(tag)).collect();
        let mut groups = HashMap::<_, Vec<_>>::new();
        for (index, record) in records.iter().enumerate() {
            let ids: Vec<_> = match record.node {
                Some(id) => graph().walk(id, true).into_iter().map(Identity::Known).collect(),
                None => vec![record.identity()],
            };
            for id in ids { groups.entry(id).or_default().push(index); }
        }
        Self { records, groups }
    }

    pub fn matching(&self, name: &str) -> Vec<String> {
        self.records_for(name).into_iter().map(|record| record.original.clone()).collect()
    }

    pub fn records_for(&self, name: &str) -> Vec<&Resolution> {
        self.groups.get(&resolve(name).identity()).into_iter().flatten().map(|at| &self.records[*at]).collect()
    }

    pub fn original_indices(&self) -> HashMap<&str, usize> {
        self.records.iter().enumerate().map(|(at, record)| (record.original.as_str(), at)).collect()
    }

    pub fn populated_by(&self, name: &str, originals: &HashSet<usize>) -> bool {
        self.groups.get(&resolve(name).identity()).is_some_and(|members| members.iter().any(|at| originals.contains(at)))
    }

    pub fn browse(&self, parent: Option<&str>, text: &str, scope: &str) -> Vec<String> {
        if scope == "tags" {
            let within = parent.map(|name| self.matching(name));
            let mut seen = HashSet::new();
            let mut tags: Vec<_> = self.records.iter().filter(|record|
                within.as_ref().is_none_or(|tags| tags.contains(&record.original)) &&
                (matches(&record.original,text) || record.recognized.is_some_and(|name| matches(name,text))))
                .filter(|record| seen.insert(record.identity()))
                .map(|record| record.recognized.map(str::to_string).unwrap_or_else(|| record.original.clone())).collect();
            tags.sort_by_cached_key(|name| (name.to_lowercase(), name.clone()));
            return tags;
        }
        let graph = graph();
        let named = parent;
        let parent = parent.and_then(|name| resolve(name).node);
        if named.is_some() && parent.is_none() { return Vec::new(); }
        let within = parent.map(|id| graph.walk(id, false));
        let mut names: Vec<_> = graph.nodes.iter().enumerate().filter(|(id, node)| {
            // Alias-only vocabulary entries have no asserted ancestry. They
            // remain searchable, without becoming top-level families.
            let populated = self.groups.contains_key(&Identity::Known(*id));
            if scope == "library" && !populated { return false; }
            if !text.is_empty() {
                return within.as_ref().is_none_or(|ids| ids.contains(id)) &&
                    (matches(node.name,text) ||
                        self.groups.get(&Identity::Known(*id)).into_iter().flatten().any(|at|
                            self.records[*at].node == Some(*id) && matches(&self.records[*at].original,text)));
            }
            parent.map_or(node.parents.is_empty() && !node.children.is_empty(), |parent| graph.nodes[parent].children.contains(id))
        }).map(|(_, node)| node.name.to_string()).collect();
        names.sort_by_cached_key(|name| (name.to_lowercase(), name.clone()));
        names
    }
}

pub(super) fn parents(name: &str) -> Vec<String> {
    resolve(name).node.map(|id| graph().nodes[id].parents.iter().map(|edge| graph().nodes[edge.node].name.to_string()).collect()).unwrap_or_default()
}

pub(super) fn parent_relations(name: &str) -> Vec<serde_json::Value> {
    resolve(name).node.map(|id| graph().nodes[id].parents.iter().map(|edge| {
        let mut value = serde_json::json!({"name":graph().nodes[edge.node].name, "source":edge.source});
        if edge.remapped { value["mapping"] = serde_json::json!("chilpa-1"); }
        value
    }).collect()).unwrap_or_default()
}

pub(super) fn breadcrumb(name: &str) -> Vec<String> {
    if name.is_empty() { return Vec::new(); }
    let mut path = vec![name.to_string()];
    let mut seen = HashSet::new();
    while let Some(parent) = parents(path.last().unwrap()).first() {
        if !seen.insert(parent.clone()) { break; }
        path.push(parent.clone());
    }
    path.reverse(); path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_groups_original_spellings_without_replacing_them() {
        let tags = vec!["Progresive Metal".into(), "prog metal".into(), "Local tradition".into()];
        let inventory = Inventory::new(&tags);
        assert_eq!(inventory.browse(None, "", "library"), ["Rock"]);
        assert_eq!(inventory.matching("Metal"), ["Progresive Metal", "prog metal"]);
        assert_eq!(inventory.matching("Progressive Metal"), ["Progresive Metal", "prog metal"]);
        assert_eq!(inventory.browse(Some("Metal"), "progressive", "library"), ["Progressive Metal"]);
        assert_eq!(inventory.browse(None, "progresive", "library"), ["Progressive Metal"]);
        assert_eq!(tags[0], "Progresive Metal");
        assert_eq!(breadcrumb("Metal"), ["Rock", "Metal"]);
        assert_eq!(parents("Symphonic Metal"), ["Metal"]);
        let direct = Inventory::new(&["Symphonic Metal".into(), "Progressive Metal".into(), "Heavy Metal".into()]).browse(Some("Metal"), "", "library");
        assert!(direct.contains(&"Heavy Metal".into()));
        assert!(direct.contains(&"Progressive Metal".into()));
        assert!(direct.contains(&"Symphonic Metal".into()));
    }

    #[test]
    fn multiple_parents_and_format_variants_do_not_repeat_a_style() {
        let inventory = Inventory::new(&["Jazz Fusion".into(), "My-style".into(), "My style".into()]);
        assert_eq!(inventory.matching("Rock"), ["Jazz Fusion"]);
        assert_eq!(inventory.matching("Jazz"), ["Jazz Fusion"]);
        assert_eq!(inventory.matching("My-style"), ["My-style", "My style"]);
        assert_eq!(inventory.matching("My style"), ["My-style", "My style"]);
        assert!(inventory.matching("Missing category").is_empty());
    }

    #[test]
    fn case_and_separators_share_one_style_without_changing_any_original() {
        let tags: Vec<String> = ["heavy metal", "Heavy Metal", "heavy-metal", "HEAVY_METAL", "Heavy  Metal"].map(String::from).into();
        let before = tags.clone();
        let inventory = Inventory::new(&tags);
        assert_eq!(inventory.browse(None,"","tags"), ["Heavy Metal"]);
        for query in ["HEAVY-METAL", "heavy_metal", "heavy metal", "heavymetal"] {
            assert_eq!(inventory.matching(query), tags);
            assert_eq!(inventory.browse(None,query,"library"), ["Heavy Metal"]);
        }
        assert_eq!(tags,before);
        let custom = Inventory::new(&["Personal-Shelf".into(),"personal shelf".into()]);
        assert_eq!(custom.browse(None,"PERSONAL_SHELF","tags").len(),1);
        assert_eq!(custom.matching("Personal Shelf"), ["Personal-Shelf","personal shelf"]);
        assert!(custom.browse(None,"","library").is_empty(), "format recognition does not invent ancestry");
    }
}
