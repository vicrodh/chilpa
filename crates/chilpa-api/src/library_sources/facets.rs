//! The Library columns with the integrated items counted in. Each column
//! counts the items the columns to its left leave, as the database's facets
//! do, and an item adds to the entry of the same name whatever its case.

use super::{passes, SourceIndex, SourceItem, Stars};
use crate::{FacetEntry, Facets, Filter};
use chilpa_library::nocase;
use std::collections::{HashMap, HashSet};

/// The filter a column is counted under: the choices to its left, with its
/// own and those to its right cleared, as `Library::facet_*` builds it.
fn left_of(f: &Filter, column: usize) -> Filter {
    let mut left = f.clone();
    left.choose(column, &[]);
    left.queries[column..].fill(String::new());
    left
}

/// How many records each name holds among the items that pass, by name as
/// first seen.
fn tally<'a>(items: impl Iterator<Item = &'a SourceItem>, names: impl Fn(&SourceItem) -> Vec<String>) -> Vec<(String, usize)> {
    let mut records: HashMap<String, (String, HashSet<i64>)> = HashMap::new();
    for item in items {
        for name in names(item) {
            if name.trim().is_empty() { continue; }
            records.entry(name.to_lowercase()).or_insert_with(|| (name.clone(), HashSet::new())).1.insert(item.record());
        }
    }
    records.into_values().map(|(name, set)| (name, set.len())).collect()
}

/// Adds counts to the entries of the same name, and entries for the rest,
/// in the column's order by name.
fn add_named(entries: &mut Vec<FacetEntry>, found: Vec<(String, usize)>) {
    if found.is_empty() { return; }
    for (name, albums) in found {
        let lowered = name.to_lowercase();
        match entries.iter_mut().find(|entry| entry.label.to_lowercase() == lowered) {
            Some(entry) => entry.albums += albums,
            None => entries.push(FacetEntry { key: name.clone(), label: name, albums, year: None, icon: String::new() }),
        }
    }
    entries.sort_by(|a, b| nocase(&a.label, &b.label));
}

/// Counts the index into every column the database already counted.
pub(crate) fn merge(facets: &mut Facets, f: &Filter, index: &SourceIndex, stars: &Stars) {
    if index.items.is_empty() { return; }
    facets.sources.extend(index.sources.iter().cloned());
    let left = |column: usize| { let filter = left_of(f, column); move |item: &&SourceItem| passes(item, &filter, stars) };
    let mut years: HashMap<i32, HashSet<i64>> = HashMap::new();
    for item in &index.items {
        if let Some(year) = item.year { years.entry(year).or_default().insert(item.record()); }
    }
    for (year, records) in years {
        match facets.years.iter_mut().find(|entry| entry.year == Some(year)) {
            Some(entry) => entry.albums += records.len(),
            None => facets.years.push(FacetEntry { key: year.to_string(), label: year.to_string(), albums: records.len(), year: Some(year), icon: String::new() }),
        }
    }
    facets.years.sort_by(|a, b| b.year.cmp(&a.year));
    add_named(&mut facets.genres, tally(index.items.iter().filter(left(1)), |item| item.genres.clone()));
    add_named(&mut facets.labels, tally(index.items.iter().filter(left(2)), |item| vec![item.label.clone()]));
    add_named(&mut facets.composers, tally(index.items.iter().filter(left(3)), |item| item.composers.clone()));
    let mut ratings: HashMap<u8, HashSet<i64>> = HashMap::new();
    for item in index.items.iter().filter(left(4)) {
        ratings.entry(stars.albums.get(&item.album).copied().unwrap_or(0)).or_default().insert(item.record());
    }
    for (stars, records) in ratings {
        let key = stars.to_string();
        match facets.ratings.iter_mut().find(|entry| entry.key == key) {
            Some(entry) => entry.albums += records.len(),
            None => facets.ratings.push(crate::browse::named_stars(FacetEntry { key, label: String::new(), albums: records.len(), year: None, icon: String::new() })),
        }
    }
    facets.ratings.sort_by_key(|entry| std::cmp::Reverse(entry.key.parse::<u8>().unwrap_or(0)));
    add_named(&mut facets.artists, tally(index.items.iter().filter(left(5)), |item| item.artists.clone()));
    let typed = f.queries[6].trim().to_lowercase();
    let mut albums: Vec<FacetEntry> = Vec::new();
    let mut listed = HashSet::new();
    for item in index.items.iter().filter(left(6)).filter(|item| item.album != 0) {
        if !typed.is_empty() && !item.album_title.to_lowercase().contains(&typed) { continue; }
        if listed.insert(item.album) {
            albums.push(FacetEntry { key: item.album.to_string(), label: item.album_title.clone(), albums: 1, year: item.year, icon: String::new() });
        }
    }
    if !albums.is_empty() {
        facets.albums.extend(albums);
        facets.albums.sort_by(|a, b| nocase(&a.label, &b.label));
    }
}
