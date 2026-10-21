//! The Library columns with the integrated items counted in. Each column
//! counts the items the columns to its left leave, as the database's facets
//! do, and an item adds to the entry of the same name whatever its case.

use super::{Prepared, SourceIndex, SourceItem, Stars};
use crate::{FacetEntry, Facets, Filter};
use chilpa_library::nocase;
use std::collections::{HashMap, HashSet};

/// The filter a column is counted under: the choices to its left, with its
/// own and those to its right cleared, as `Library::facet_*` builds it.
fn left_of(f: &Filter, column: usize) -> Filter {
    let name = ["year", "genre", "label", "composer", "rating", "artist", "album"][column];
    let mut left = crate::library_query::facet_filter(f, name);
    left.choose(column, &[]);
    left.queries[column..].fill(String::new());
    left
}

/// How many records each name holds among the items that pass, by its
/// folded form and named as first seen. `names` gives an item's names and
/// their folded forms, in step.
fn tally<'a>(items: impl Iterator<Item = &'a SourceItem>, names: impl Fn(&'a SourceItem) -> (&'a [String], &'a [String])) -> Vec<(String, usize)> {
    let mut records: HashMap<&'a str, (&'a str, HashSet<i64>)> = HashMap::new();
    for item in items {
        let (shown, folded) = names(item);
        for (name, key) in shown.iter().zip(folded) {
            if name.trim().is_empty() { continue; }
            records.entry(key.as_str()).or_insert_with(|| (name.as_str(), HashSet::new())).1.insert(item.record());
        }
    }
    records.into_values().map(|(name, set)| (name.to_string(), set.len())).collect()
}

/// Adds counts to the entries of the same name, and entries for the rest,
/// in the column's order by name.
fn add_named(entries: &mut Vec<FacetEntry>, found: Vec<(String, usize)>) {
    if found.is_empty() { return; }
    let mut at: HashMap<String, usize> = entries.iter().enumerate().map(|(i, entry)| (entry.label.to_lowercase(), i)).collect();
    let mut added = false;
    for (name, albums) in found {
        let key = name.to_lowercase();
        match at.get(&key) {
            Some(&i) => entries[i].albums += albums,
            None => {
                at.insert(key, entries.len());
                entries.push(FacetEntry { key: name.clone(), label: name, albums, year: None, icon: String::new() });
                added = true;
            }
        }
    }
    if added { entries.sort_by(|a, b| nocase(&a.label, &b.label)); }
}

/// Counts the index into every column the database already counted. With
/// `grouping`, the records column lists a record kept twice once.
pub(crate) fn merge(facets: &mut Facets, f: &Filter, index: &SourceIndex, stars: &Stars, grouping: Option<super::Grouping>) {
    if index.items.is_empty() { return; }
    facets.sources.extend(index.sources.iter().cloned());
    let left = |column: usize| { let filter = Prepared::new(&left_of(f, column)); move |item: &&SourceItem| filter.passes(item, stars) };
    let mut years: HashMap<i32, HashSet<i64>> = HashMap::new();
    for item in index.items.iter().filter(left(0)) {
        if let Some(year) = item.year { years.entry(year).or_default().insert(item.record()); }
    }
    for (year, records) in years {
        match facets.years.iter_mut().find(|entry| entry.year == Some(year)) {
            Some(entry) => entry.albums += records.len(),
            None => facets.years.push(FacetEntry { key: year.to_string(), label: year.to_string(), albums: records.len(), year: Some(year), icon: String::new() }),
        }
    }
    facets.years.sort_by(|a, b| b.year.cmp(&a.year));
    add_named(&mut facets.genres, tally(index.items.iter().filter(left(1)), |item| (item.genres.as_slice(), item.folded.genres.as_slice())));
    add_named(&mut facets.labels, tally(index.items.iter().filter(left(2)), |item| (std::slice::from_ref(&item.label), std::slice::from_ref(&item.folded.label))));
    add_named(&mut facets.composers, tally(index.items.iter().filter(left(3)), |item| (item.composers.as_slice(), item.folded.composers.as_slice())));
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
    add_named(&mut facets.artists, tally(index.items.iter().filter(left(5)), |item| (item.artists.as_slice(), item.folded.artists.as_slice())));
    let typed = f.queries[6].trim().to_lowercase();
    let mut albums: Vec<FacetEntry> = Vec::new();
    let mut listed = HashSet::new();
    for item in index.items.iter().filter(left(6)).filter(|item| item.album != 0) {
        if !typed.is_empty() && !item.folded.album_title.contains(&typed) { continue; }
        if listed.insert(item.album) {
            albums.push(FacetEntry { key: item.album.to_string(), label: item.album_title.clone(), albums: 1, year: item.year, icon: String::new() });
        }
    }
    if !albums.is_empty() {
        facets.albums.extend(albums);
        if let Some(grouping) = grouping {
            let ids: Vec<i64> = facets.albums.iter().filter_map(|entry| entry.key.parse().ok()).collect();
            let dropped = super::dropped_copies(&ids, grouping, &index.versions);
            facets.albums.retain(|entry| entry.key.parse().is_ok_and(|id: i64| !dropped.contains(&id)));
        }
        facets.albums.sort_by(|a, b| nocase(&a.label, &b.label));
    }
}
