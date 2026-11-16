//! Optional native navigation augments presentation without changing library integration.
use crate::sources::PanelChild;
use serde_json::Value;
use std::collections::HashSet;

const MAX_ENTRIES: usize = 32;
const MAX_DEPTH: usize = 2;

fn text<'a>(value: &'a Value, key: &str, limit: usize) -> Option<&'a str> {
    value[key]
        .as_str()
        .filter(|s| !s.trim().is_empty() && s.len() <= limit && !s.chars().any(char::is_control))
}

pub(crate) fn apply(
    plugin: &str,
    capability: &Value,
    mut fallback: Vec<PanelChild>,
) -> Vec<PanelChild> {
    let Some(nodes) = capability.as_array() else {
        return fallback;
    };
    let mut count = 0;
    let mut identities = HashSet::new();
    let parsed = nodes
        .iter()
        .filter(|node| node["layout"] != "web")
        .map(|node| parse(plugin, node, "", 1, &mut count, &mut identities))
        .collect::<Option<Vec<_>>>();
    let Some(parsed) = parsed else {
        return fallback;
    };
    for node in parsed {
        if let Some(slot) = fallback.iter_mut().find(|entry| entry.id == node.id) {
            *slot = node;
        } else {
            fallback.push(node);
        }
    }
    fallback
}

fn parse(
    plugin: &str,
    node: &Value,
    parent: &str,
    depth: usize,
    count: &mut usize,
    identities: &mut HashSet<String>,
) -> Option<PanelChild> {
    *count += 1;
    if depth > MAX_DEPTH || *count > MAX_ENTRIES {
        return None;
    }
    let id = text(node, "id", 64)?;
    if !id
        .bytes()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
    {
        return None;
    }
    let path = if parent.is_empty() {
        id.into()
    } else {
        format!("{parent}/{id}")
    };
    if !identities.insert(path.clone()) {
        return None;
    }
    let reference = text(node, "ref", 4096)?;
    if reference.contains('|') || reference.chars().any(char::is_whitespace) {
        return None;
    }
    let children = match node.get("children") {
        None => Vec::new(),
        Some(Value::Array(children)) => children
            .iter()
            .filter(|child| child["layout"] != "web")
            .map(|child| parse(plugin, child, &path, depth + 1, count, identities))
            .collect::<Option<Vec<_>>>()?,
        _ => return None,
    };
    let layout = node["layout"].as_str().unwrap_or("");
    if !matches!(layout, "" | "collection") {
        return None;
    }
    let route = if layout.is_empty() {
        format!("{plugin} native:{reference}")
    } else {
        crate::native_page_request::route(plugin, reference, serde_json::json!({}), layout)
    };
    if route.is_empty() {
        return None;
    }
    Some(PanelChild {
        id: format!("view:{path}"),
        title: text(node, "title", 128)?.into(),
        icon: node["icon"]
            .as_str()
            .filter(|s| s.len() <= 64)
            .unwrap_or("")
            .into(),
        route,
        children,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn defaults() -> Vec<PanelChild> {
        vec![
            PanelChild {
                id: "view:discover".into(),
                title: "Home".into(),
                icon: "home".into(),
                route: "fixture".into(),
                children: Vec::new(),
            },
            PanelChild {
                id: "view:library".into(),
                title: "Library".into(),
                icon: "columns3".into(),
                route: "fixture section:library".into(),
                children: Vec::new(),
            },
        ]
    }
    #[test]
    fn a_native_tree_replaces_only_its_named_section_and_keeps_home() {
        let nodes = json!([{"id":"library","title":"Library","ref":"saved:all","children":[{"id":"all","title":"All","ref":"saved:all"},{"id":"topics","title":"Topics","ref":"saved:topics"}]},
            {"id":"journal","title":"Journal","ref":"journal:home"}]);
        let tree = apply("fixture", &nodes, defaults());
        assert_eq!(tree.len(), 3);
        assert_eq!(tree[0].route, "fixture");
        assert_eq!(tree[1].route, "fixture native:saved:all");
        assert_eq!(tree[1].children[1].id, "view:library/topics");
    }
    #[test]
    fn malformed_or_oversized_trees_leave_legacy_navigation_intact() {
        for nodes in [
            json!([{"id":"a","title":"A","ref":"contains|section"}]),
            json!([{"id":"a","title":"A","ref":"a","children":[{"id":"b","title":"B","ref":"b","children":[{"id":"c","title":"C","ref":"c"}]}]}]),
            json!(
                (0..33)
                    .map(|i| json!({"id":format!("n{i}"),"title":"N","ref":"n"}))
                    .collect::<Vec<_>>()
            ),
        ] {
            assert_eq!(apply("fixture", &nodes, defaults()).len(), 2);
        }
    }

    #[test]
    fn obsolete_web_entries_do_not_discard_native_library_children() {
        let nodes = json!([
            {"id":"library","title":"Library","ref":"saved:all","layout":"collection","children":[
                {"id":"all","title":"All","ref":"saved:all","layout":"collection"},
                {"id":"journal","title":"Journal","ref":"journal:home","layout":"web"},
                {"id":"artists","title":"Artists","ref":"saved:artists","layout":"collection"}
            ]},
            {"id":"journal","title":"Journal","ref":"journal:home","layout":"web"}
        ]);
        let tree = apply("fixture", &nodes, defaults());
        assert_eq!(tree.len(), 2);
        assert_eq!(tree[0].route, "fixture");
        assert_eq!(tree[1].children.len(), 2);
        assert_eq!(tree[1].children[0].id, "view:library/all");
        assert_eq!(tree[1].children[1].id, "view:library/artists");
        for child in &tree[1].children {
            let (_, reference) = child.route.split_once(" native:").unwrap();
            assert_eq!(crate::native_page_request::decode(reference).unwrap().layout, "collection");
        }
    }
}
