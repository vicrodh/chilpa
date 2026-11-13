//! Derived navigation over original tags. Nothing here writes library metadata.
//! Per-query groups contain tag indices, released with the read-only request.
mod resolution;
mod taxonomy;
use resolution::{resolve, Resolution};
use taxonomy::graph;
use std::collections::{HashMap, HashSet};
pub(super) const VERSION: &str = taxonomy::VERSION;

#[derive(Clone, Eq, Hash, PartialEq)]
enum Identity { Known(usize), Literal(String) }
impl Resolution {
    fn identity(&self) -> Identity { self.node.map(Identity::Known).unwrap_or_else(|| Identity::Literal(self.original.to_lowercase())) }
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

    fn label(&self, id: &Identity) -> String {
        match id {
            Identity::Known(id) => graph().nodes[*id].name.to_string(),
            Identity::Literal(_) => self.groups.get(id).and_then(|rows| rows.first()).map(|at| self.records[*at].original.clone()).unwrap_or_default(),
        }
    }

    pub fn matching(&self, name: &str) -> Vec<String> {
        self.records_for(name).into_iter().map(|record| record.original.clone()).collect()
    }

    pub fn records_for(&self, name: &str) -> Vec<&Resolution> {
        self.groups.get(&resolve(name).identity()).into_iter().flatten().map(|at| &self.records[*at]).collect()
    }

    pub fn names(&self, parent: Option<&str>, text: &str) -> Vec<String> {
        let parent = parent.map(|name| resolve(name).identity());
        let within = parent.as_ref().map(|id| match id {
            Identity::Known(id) => graph().walk(*id, false).into_iter().map(Identity::Known).collect::<HashSet<_>>(),
            id => HashSet::from([id.clone()]),
        });
        let mut names: Vec<_> = self.groups.iter().filter(|(id, rows)| {
            if !text.is_empty() {
                return within.as_ref().is_none_or(|keys| keys.contains(*id)) &&
                    (chilpa_library::store::query::fold(&self.label(id)).contains(text) || rows.iter().any(|at| {
                        self.records[*at].identity() == **id && chilpa_library::store::query::fold(&self.records[*at].original).contains(text)
                    }));
            }
            match (&parent, id) {
                (None, Identity::Literal(_)) => true,
                (None, Identity::Known(id)) => graph().nodes[*id].parents.is_empty(),
                (Some(Identity::Known(parent)), Identity::Known(id)) => graph().nodes[*parent].children.contains(id),
                _ => false,
            }
        }).map(|(id, _)| self.label(id)).collect();
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
        assert_eq!(inventory.names(None, ""), ["Local tradition", "Rock"]);
        assert_eq!(inventory.matching("Metal"), ["Progresive Metal", "prog metal"]);
        assert_eq!(inventory.matching("Progressive Metal"), ["Progresive Metal", "prog metal"]);
        assert_eq!(inventory.names(Some("Metal"), "progressive"), ["Progressive Metal"]);
        assert_eq!(inventory.names(None, "progresive"), ["Progressive Metal"]);
        assert_eq!(tags[0], "Progresive Metal");
        assert_eq!(breadcrumb("Metal"), ["Rock", "Metal"]);
        assert_eq!(parents("Symphonic Metal"), ["Metal"]);
        let direct = Inventory::new(&["Symphonic Metal".into(), "Progressive Metal".into(), "Heavy Metal".into()]).names(Some("Metal"), "");
        assert!(direct.contains(&"Heavy Metal".into()));
        assert!(direct.contains(&"Progressive Metal".into()));
        assert!(direct.contains(&"Symphonic Metal".into()));
    }

    #[test]
    fn multiple_parents_do_not_repeat_a_tag_and_personal_names_remain_distinct() {
        let inventory = Inventory::new(&["Jazz Fusion".into(), "My-style".into(), "My style".into()]);
        assert_eq!(inventory.matching("Rock"), ["Jazz Fusion"]);
        assert_eq!(inventory.matching("Jazz"), ["Jazz Fusion"]);
        assert_eq!(inventory.matching("My-style"), ["My-style"]);
        assert_eq!(inventory.matching("My style"), ["My style"]);
        assert!(inventory.matching("Missing category").is_empty());
    }
}
