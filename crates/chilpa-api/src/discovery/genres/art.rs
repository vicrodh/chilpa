//! The default card and header of every genre, published on the website
//! (`crate::genre_art::BASE`). The bundled index names 1,182 genres with the
//! digest of both files and their credit; it is parsed on the first genre page
//! and kept, about 1,200 rows. A genre the index does not name takes the
//! nearest ancestor that it does; one outside the taxonomy has none.
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::OnceLock;

const INDEX: &str = include_str!("../../../data/genre-art.tsv");

struct Row {
    name: &'static str,
    key: &'static str,
    card: &'static str,
    header: &'static str,
    credit: &'static str,
    license: &'static str,
    license_url: &'static str,
    page: &'static str,
}

struct Index { rows: Vec<Row>, by_name: HashMap<&'static str, usize> }

fn index() -> &'static Index {
    static INDEX_ROWS: OnceLock<Index> = OnceLock::new();
    INDEX_ROWS.get_or_init(|| {
        let rows: Vec<_> = INDEX.lines().filter(|line| !line.starts_with('#') && !line.is_empty()).map(|line| {
            let fields: Vec<_> = line.split('\t').collect();
            assert_eq!(fields.len(), 8, "bundled genre image row: {line}");
            Row { name:fields[0], key:fields[1], card:fields[2], header:fields[3], credit:fields[4],
                license:fields[5], license_url:fields[6], page:fields[7] }
        }).collect();
        let by_name = rows.iter().enumerate().map(|(at, row)| (row.name, at)).collect();
        Index { rows, by_name }
    })
}

pub(in crate::discovery) fn art(name: &str) -> Option<Value> {
    let graph = super::taxonomy::graph();
    let own = graph.nodes[super::resolve(name).node?].name;
    let index = index();
    let at = std::iter::once(own.to_string()).chain(super::breadcrumb(own).into_iter().rev().skip(1))
        .find_map(|name| index.by_name.get(name.as_str()).copied())?;
    let row = &index.rows[at];
    Some(json!({"key":row.key, "credit":row.credit, "license":row.license, "licenseUrl":row.license_url, "page":row.page}))
}

/// The SHA-256 the published card or header of `key` must have.
pub(crate) fn digest(key: &str, header: bool) -> Option<&'static str> {
    index().rows.iter().find(|row| row.key == key).map(|row| if header { row.header } else { row.card })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_bundled_image_names_one_genre_of_the_taxonomy_and_every_genre_has_one() {
        let graph = super::super::taxonomy::graph();
        let index = index();
        let mut keys = std::collections::HashSet::new();
        for row in &index.rows {
            let node = graph.by_key.get(&crate::library_query::genre_key(row.name)).copied();
            assert_eq!(node.map(|node| graph.nodes[node].name), Some(row.name), "{} is not a taxonomy name", row.name);
            assert!(keys.insert(row.key), "{} repeats a key", row.key);
            assert!(!row.key.is_empty() && row.key.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'));
            for digest in [row.card, row.header] {
                assert!(digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()));
            }
            assert!(!row.credit.is_empty() && !row.license.is_empty() && row.page.starts_with("https://"));
        }
        assert_eq!(index.by_name.len(), index.rows.len());
        for node in &graph.nodes {
            assert!(art(node.name).is_some(), "{} has no image", node.name);
        }
    }

    #[test]
    fn a_style_shows_its_own_image_and_an_unknown_tag_shows_none() {
        assert_eq!(art("Symphonic Metal").unwrap()["key"], "symphonic-metal");
        assert_eq!(art("symphonic-metal").unwrap()["key"], "symphonic-metal");
        assert_eq!(art("Rock").unwrap()["key"], "rock");
        assert!(art("Personal shelf").is_none());
        assert_eq!(digest("rock", false).map(str::len), Some(64));
        assert_ne!(digest("rock", false), digest("rock", true));
        assert!(digest("../rock", false).is_none());
    }
}
