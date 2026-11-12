//! Tracks column choices share the snapshot's SQL and integrated source index.

use crate::{library_sources::{Prepared, SourceItem, Stars}, Filter, Reader, TracksQuery};
use chilpa_library::store::{query::fold, track_sheet::{Selection, PAGE_SIZE}};
use serde_json::{json, Value};
use std::collections::BTreeMap;
#[cfg(test)]
use std::collections::HashSet;
use std::sync::Arc;

pub(crate) fn passes(selection: &Selection, item: &SourceItem, stars: &Stars) -> bool {
    if selection.invalid { return false; }
    if selection.values.is_empty() && selection.favorite.is_none() { return true; }
    if !selection.accepts("title", &item.title)
        || !selection.accepts("artist", item.display_artist())
        || !selection.accepts("album", item.display_album())
        || !selection.accepts("format", item.format)
        || !selection.accepts("quality", item.tier) { return false; }
    if selection.values.contains_key("source") && !selection.accepts("source", &item.source_key()) { return false; }
    if selection.values.contains_key("rating") && !selection.accepts("rating", &stars.tracks.get(&item.track).copied().unwrap_or(0).to_string()) { return false; }
    selection.favorite.is_none_or(|on| on == selection.source_favorites.contains(&item.track))
}

fn value(item: &SourceItem, field: &str, stars: &Stars, selection: &Selection) -> String {
    match field {
        "title" => item.title.clone(),
        "artist" => item.display_artist().into(),
        "album" => item.display_album().into(),
        "source" => item.source_key(),
        "rating" => stars.tracks.get(&item.track).copied().unwrap_or(0).to_string(),
        "format" => item.format.into(),
        "quality" => item.tier.into(),
        "favorite" => selection.source_favorites.contains(&item.track).to_string(),
        _ => String::new(),
    }
}

fn fixed_label(field: &str, key: &str) -> String {
    let name = match (field, key) {
        ("favorite", "true") => "Yes", ("favorite", "false") => "No",
        ("rating", "0") => "Unrated",
        ("quality", "lossy") => "Lossy", ("quality", "lossless") => "Lossless",
        ("quality", "hires") => "Hi-Res", ("quality", "dsd") => "DSD", ("quality", "unknown") => "Unknown",
        _ => return key.to_string(),
    };
    chilpa_i18n::t(name)
}

impl Reader {
    /// Completion used when the shared worker cannot open its read-only connection.
    pub fn track_column_failure(field: &str, search: &str, cursor: &str) -> Value {
        json!({"field":field,"search":search,"cursor":cursor,"items":[],"next":"","more":false,
            "error":chilpa_i18n::t("The library could not be read.")})
    }

    /// One keyset page, with the same text and selections as the resulting Tracks list.
    /// The edited column is omitted so unchecked alternatives remain available.
    /// Rust retains at most 101 keys, including while merging sources, and frees them on return.
    pub fn track_column(&self, query: &str, extras: &Filter, field: &str, search: &str, cursor: &str) -> Value {
        self.track_column_using(query, extras, field, search, cursor, &self.source_index().1)
    }

    fn track_column_using(&self, query: &str, extras: &Filter, field: &str, search: &str, cursor: &str,
        index: &crate::library_sources::SourceIndex) -> Value {
        let mut answer = json!({"field":field,"search":search,"cursor":cursor,"items":[],"next":"","more":false,"error":""});
        let after: Option<String> = if cursor.is_empty() { None } else {
            match serde_json::from_str(cursor) {
                Ok(key) => Some(key),
                Err(_) => { answer["error"] = json!(chilpa_i18n::t("The filter cannot be read.")); return answer; }
            }
        };
        let mut filter = TracksQuery::parse(query).filter(extras);
        filter.sheet.without(field);
        let fixed: &[&str] = match field {
            "rating" => &["0", "1", "2", "3", "4", "5"],
            "quality" => &["lossy", "lossless", "hires", "dsd", "unknown"],
            "favorite" => &["false", "true"], _ => &[],
        };
        let typed = if fixed.is_empty() { search } else { "" };
        let local = match self.lib.track_sheet_choices(&filter, field, typed, after.as_deref()) {
            Ok(rows) => rows,
            Err(error) => {
                eprintln!("chilpa-tracks: column query failed: {error}");
                answer["error"] = json!(chilpa_i18n::t("The library could not be read."));
                return answer;
            }
        };
        let stars = Stars::read(&self.lib, index);
        let prepared = Prepared::new(&filter);
        let search = fold(search);
        let source_labels = if field == "source" {
            let mut entries = self.lib.facet_sources().unwrap_or_default();
            entries.extend(index.sources.iter().cloned()); entries
        } else { Vec::new() };
        let matches = |key: &str| search.is_empty() || fold(key).contains(&search)
            || source_labels.iter().any(|entry| entry.key == key && fold(&entry.label).contains(&search))
            || (!fixed.is_empty() && fold(&fixed_label(field, key)).contains(&search));
        let mut choices: BTreeMap<String, usize> = local.into_iter().collect();
        for item in index.items.iter().filter(|item| item.saved_track && item.track != 0 && prepared.passes(item, &stars)) {
            let key = value(item, field, &stars, &filter.sheet);
            if after.as_ref().is_some_and(|after| key <= *after) || (fixed.is_empty() && !matches(&key)) { continue; }
            // Once full, keys past the end cannot enter this page. Removing a large
            // key is safe: the greatest retained key can only decrease during the pass.
            if choices.len() > PAGE_SIZE && choices.last_key_value().is_some_and(|(last, _)| &key > last) { continue; }
            *choices.entry(key).or_default() += 1;
            if choices.len() > PAGE_SIZE + 1 { choices.pop_last(); }
        }
        for key in fixed {
            if after.as_ref().is_none_or(|after| *key > after.as_str()) { choices.entry((*key).into()).or_default(); }
        }
        if !fixed.is_empty() { choices.retain(|key, _| matches(key)); }
        let more = choices.len() > PAGE_SIZE;
        if more { choices.pop_last(); }
        if let Some((last, _)) = choices.last_key_value() {
            answer["next"] = json!(serde_json::to_string(last).expect("string cursor"));
        }
        answer["items"] = json!(choices.into_iter().map(|(key, count)| {
            let label = source_labels.iter().find(|entry| entry.key == key).map(|entry| entry.label.clone()).unwrap_or_else(|| key.clone());
            json!({"key":key,"label":label,"count":count})
        }).collect::<Vec<_>>());
        answer["more"] = json!(more);
        answer
    }
}

impl crate::Chilpa {
    /// The same source hearts the UI shows, including pending optimistic changes.
    /// The request owns this immutable snapshot and releases it with its result.
    pub fn track_sheet_extras(&self, query: &str, column: &str, mut extras: Filter) -> Filter {
        if column == "favorite" || TracksQuery::parse(query).sheet.as_ref().is_some_and(|sheet| sheet.favorite.is_some()) {
            extras.sheet.source_favorites = Arc::new(self.source_track_marks().1);
        }
        extras
    }
}

#[cfg(test)]
mod tests;
