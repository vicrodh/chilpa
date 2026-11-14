//! Twenty-one credited family images, available without a network request.
use serde_json::Value;
use std::sync::OnceLock;
pub(in crate::discovery) fn art(name: &str) -> Option<&'static Value> {
    static ART: OnceLock<Vec<Value>> = OnceLock::new();
    let art = ART.get_or_init(|| serde_json::from_str(include_str!("../../../data/genre-art.json")).expect("bundled genre credits"));
    let root = super::breadcrumb(name).into_iter().next()?;
    art.iter().find(|entry| entry["family"] == root)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_asserted_family_has_credited_art_and_styles_inherit_the_family() {
        for node in super::super::taxonomy::graph().nodes.iter().filter(|node| node.parents.is_empty() && !node.children.is_empty()) {
            let art = art(node.name).unwrap_or_else(|| panic!("{} has no family image",node.name));
            assert!(!art["page"].as_str().unwrap().is_empty());
            assert!(!art["license"].as_str().unwrap().is_empty());
            assert!(art["assetSha256"].as_str().unwrap().len() == 64);
        }
        assert_eq!(art("Symphonic Metal").unwrap()["key"], "rock");
        assert!(art("Personal shelf").is_none());
    }
}
