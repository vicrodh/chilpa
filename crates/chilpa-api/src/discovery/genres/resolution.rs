//! Conservative name recognition; parents never come from edit distance.
use super::taxonomy::{graph, ALIASES_VERSION};
use crate::library_query::{distance, genre_key};

#[derive(Clone, Debug, serde::Serialize)]
pub(in crate::discovery) struct Resolution {
    pub original: String,
    pub recognized: Option<&'static str>,
    pub method: &'static str,
    pub source: &'static str,
    pub alternatives: Vec<&'static str>,
    #[serde(skip)]
    pub node: Option<usize>,
}

pub(super) fn resolve(original: &str) -> Resolution {
    let mut result = Resolution { original:original.to_string(), recognized:None, method:"unknown", source:"user", alternatives:Vec::new(), node:None };
    // Personal prose and very long tags are kept literally. DP rows are bounded.
    if original.chars().count() > 128 { return result; }
    let graph = graph();
    let key = genre_key(original);
    let found = if let Some(id) = graph.by_key.get(&key) {
        result.method = if original == graph.nodes[*id].name { "exact" } else { "normalized" };
        result.source = "taxonomy";
        Some(*id)
    } else if let Some(ids) = graph.aliases.get(&key) {
        result.source = ALIASES_VERSION;
        if ids.len() == 1 { result.method = "alias"; Some(ids[0]) }
        else {
            result.method = "ambiguous";
            result.alternatives = ids.iter().map(|id| graph.nodes[*id].name).collect();
            None
        }
    } else {
        let letters: Vec<_> = key.chars().collect();
        if letters.len() < 8 { return result; }
        let most = if letters.len() >= 16 { 2 } else { 1 };
        let mut close: Vec<_> = graph.nodes.iter().enumerate().filter_map(|(id, node)| {
            let score = distance(&letters, &node.letters, most + 1);
            (score <= most + 1).then_some((score, id))
        }).collect();
        close.sort_unstable();
        if let Some(&(best, id)) = close.first().filter(|(score, _)| *score <= most) {
            result.source = "taxonomy";
            if close.get(1).is_none_or(|(next, _)| *next > best + 1) {
                result.method = "fuzzy"; Some(id)
            } else {
                result.method = "ambiguous";
                result.alternatives = close.iter().take_while(|(score, _)| *score <= best + 1).take(8).map(|(_, id)| graph.nodes[*id].name).collect();
                None
            }
        } else { None }
    };
    result.node = found;
    result.recognized = found.map(|id| graph.nodes[id].name);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aliases_and_typos_keep_the_original_and_record_the_decision() {
        let alias = resolve("dnb");
        assert_eq!((alias.original.as_str(), alias.recognized, alias.method), ("dnb", Some("Drum and Bass"), "alias"));
        let typo = resolve("Progresive Metal");
        assert_eq!((typo.recognized, typo.method), (Some("Progressive Metal"), "fuzzy"));
        assert_eq!(typo.original, "Progresive Metal");
        assert_eq!(resolve("PROGRESSIVE-METAL").method, "normalized");
    }

    #[test]
    fn short_custom_and_ambiguous_names_are_not_forced_into_a_genre() {
        for name in ["Zazz", "My Sunday collection", "Metal / my favorites", "Folk Rok"] {
            assert!(resolve(name).recognized.is_none(), "{name}");
        }
        assert!(resolve(&"a".repeat(1024)).recognized.is_none());
        let ambiguous = resolve("Wardcore");
        assert_eq!(ambiguous.method, "ambiguous");
        assert!(ambiguous.recognized.is_none());
    }
}
