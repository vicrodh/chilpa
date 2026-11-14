//! Conservative name recognition; parents never come from edit distance.
use super::taxonomy::{graph, ALIASES_VERSION};
use crate::library_query::genre_key;

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
    let mut rows = super::distance::Rows::default();
    resolve_with(original, |a, b, limit| rows.between(a, b, limit))
}

fn resolve_with(original: &str, mut distance: impl FnMut(&[char], &[char], usize) -> usize) -> Resolution {
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

    #[test]
    fn bounded_matching_preserves_recognition_and_ambiguity() {
        let names = ["Progresive Metal", "Atmospheric Blak Metal", "Wardcore", "Folk Rok",
            "My Sunday collection", "électro", "音楽", "PROGRESSIVE-METAL", "dnb"];
        for name in names {
            let old = resolve_with(name, crate::library_query::distance);
            assert_eq!(serde_json::to_value(resolve(name)).unwrap(), serde_json::to_value(old).unwrap(), "{name}");
        }
    }

    #[test]
    fn cold_recognition_timing() {
        let start = std::time::Instant::now();
        let _ = graph();
        eprintln!("taxonomy_init_ms={:.3}", start.elapsed().as_secs_f64() * 1000.0);
        let tags: Vec<_> = (0..80).map(|at| format!("Personal collection {at:03}")).collect();
        let mut timings = Vec::new();
        for bounded in [false, true] {
            let start = std::time::Instant::now();
            for tag in &tags {
                let found = if bounded { resolve(tag) } else { resolve_with(tag, crate::library_query::distance) };
                assert!(found.recognized.is_none());
                std::hint::black_box(found);
            }
            eprintln!("bounded={bounded} tags={} elapsed_ms={:.3}", tags.len(), start.elapsed().as_secs_f64() * 1000.0);
            timings.push(serde_json::json!({"bounded":bounded,"tags":tags.len(),"elapsedMs":start.elapsed().as_secs_f64()*1000.0}));
        }
        if let Some(directory) = std::env::var_os("CHILPA_DISCOVERY_MEASUREMENT_DIR") {
            let directory = std::path::Path::new(&directory);
            std::fs::create_dir_all(directory).unwrap();
            std::fs::write(directory.join("genre-recognition.json"), serde_json::to_vec_pretty(&timings).unwrap()).unwrap();
        }
    }
}
