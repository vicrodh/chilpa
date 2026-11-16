//! Exact column selections and bounded, searchable checklist pages for Tracks.

use super::{Filter, Library};
use rusqlite::{params_from_iter, types::ToSql};
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

pub const PAGE_SIZE: usize = 100;
pub const FIELDS: &[&str] = &["title", "artist", "album", "source", "rating", "format", "quality", "bits", "rate"];

/// A screen owns this selection. Empty lists mean all, including empty names.
/// Source favorites are a shared snapshot of the facade's marks, never JSON input.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    pub values: BTreeMap<String, Vec<String>>,
    pub favorite: Option<bool>,
    pub source_favorites: Arc<HashSet<i64>>,
    pub invalid: bool,
}

impl Selection {
    pub fn parse(text: &str) -> Self {
        if text.is_empty() { return Self::default(); }
        let Ok(serde_json::Value::Object(object)) = serde_json::from_str(text) else {
            return Self { invalid: true, ..Default::default() };
        };
        let mut out = Self::default();
        for (field, value) in object {
            if field == "favorite" {
                out.favorite = value.as_bool();
                out.invalid |= !value.is_null() && !value.is_boolean();
                continue;
            }
            if !FIELDS.contains(&field.as_str()) { out.invalid = true; continue; }
            let Some(values) = value.as_array() else { out.invalid = true; continue; };
            let mut chosen = Vec::new();
            for value in values {
                let name = match value {
                    serde_json::Value::String(name) => name.clone(),
                    serde_json::Value::Number(n) if ["rating", "bits", "rate"].contains(&field.as_str()) => n.to_string(),
                    _ => { out.invalid = true; continue; }
                };
                if field == "rating" && !name.parse::<u8>().is_ok_and(|n| n <= 5) { out.invalid = true; }
                if ["bits", "rate"].contains(&field.as_str()) && !name.is_empty()
                    && !name.parse::<u32>().is_ok_and(|n| n > 0 && n.to_string() == name) { out.invalid = true; }
                chosen.push(name);
            }
            chosen.sort(); chosen.dedup();
            if !chosen.is_empty() { out.values.insert(field, chosen); }
        }
        out
    }

    pub fn accepts(&self, field: &str, value: &str) -> bool {
        self.values.get(field).is_none_or(|values| values.binary_search_by(|v| v.as_str().cmp(value)).is_ok())
    }

    pub fn without(&mut self, field: &str) {
        self.values.remove(field);
        if field == "favorite" { self.favorite = None; }
    }

    pub(super) fn append(&self, sql: &mut String, args: &mut Vec<Box<dyn ToSql>>) {
        if self.invalid { sql.push_str(" AND 0"); return; }
        for (field, values) in &self.values {
            if values.is_empty() { continue; }
            if field == "title" && !values.iter().any(|value| value.contains('\0')) {
                let phrases: Vec<_> = values.iter().map(|value| format!("\"{}\"", value.replace('"', "\"\""))).collect();
                if values.iter().all(|value| has_known_token(value)) {
                    sql.push_str(" AND t.id IN (SELECT rowid FROM track_search WHERE track_search MATCH ?)");
                    args.push(Box::new(format!("title : ({})", phrases.join(" OR "))));
                } else {
                    // Missing exact keys take the fallback too: punctuation, empty
                    // names and unknown token categories must never exclude a match.
                    sql.push_str(" AND t.id IN (WITH hits AS MATERIALIZED (SELECT rowid FROM track_search WHERE track_search MATCH ?),
                        missing AS MATERIALIZED (SELECT value FROM json_each(?) EXCEPT SELECT ft.title COLLATE BINARY FROM hits CROSS JOIN tracks ft ON ft.id=hits.rowid)
                        SELECT rowid FROM hits UNION ALL
                        SELECT fallback.id FROM (SELECT 1 FROM missing LIMIT 1) guard CROSS JOIN tracks fallback
                        WHERE fallback.title COLLATE BINARY IN (SELECT value FROM missing))");
                    args.push(Box::new(format!("title : ({})", phrases.join(" OR "))));
                    args.push(Box::new(serde_json::to_string(values).expect("string selections")));
                }
            }
            if field == "album" {
                sql.push_str(" AND (t.album_id IN (SELECT a.id FROM json_each(?) chosen JOIN albums a ON a.title=chosen.value COLLATE NOCASE WHERE a.title=chosen.value COLLATE BINARY)");
                args.push(Box::new(serde_json::to_string(values).expect("string selections")));
                if values.iter().any(String::is_empty) { sql.push_str(" OR t.album_id IS NULL"); }
                sql.push(')');
                continue;
            }
            if field == "source" {
                sql.push_str(" AND t.source_id IN (SELECT id FROM sources WHERE (provider || ':' || instance) IN (SELECT value FROM json_each(?)))");
                args.push(Box::new(serde_json::to_string(values).expect("string selections")));
                continue;
            }
            if field == "bits" || field == "rate" {
                let column = if field == "bits" { "t.bit_depth" } else { "t.sample_rate_hz" };
                if values.len() == 1 && !values[0].is_empty() {
                    sql.push_str(&format!(" AND {column}=?"));
                    args.push(Box::new(values[0].parse::<i64>().expect("validated number")));
                    continue;
                }
                sql.push_str(&format!(" AND ({column} IN (SELECT CAST(value AS INTEGER) FROM json_each(?))"));
                args.push(Box::new(serde_json::to_string(values).expect("numeric selections")));
                if values.iter().any(String::is_empty) { sql.push_str(&format!(" OR {column} IS NULL OR {column}<=0")); }
                sql.push(')');
                continue;
            }
            let Some(expression) = expression(field) else { sql.push_str(" AND 0"); continue; };
            // One bound value avoids JSON decoding for the common single selection.
            if values.len() == 1 {
                sql.push_str(&format!(" AND ({expression}) COLLATE BINARY = ?"));
                args.push(Box::new(values[0].clone()));
            } else {
                // A single parameter also keeps arbitrarily large selections valid.
                sql.push_str(&format!(" AND ({expression}) COLLATE BINARY IN (SELECT value FROM json_each(?))"));
                args.push(Box::new(serde_json::to_string(values).expect("string selections")));
            }
        }
        if let Some(on) = self.favorite {
            sql.push_str(if on { " AND t.id IN (SELECT track_id FROM track_favorites)" }
                else { " AND t.id NOT IN (SELECT track_id FROM track_favorites)" });
        }
    }
}

/// These ranges contain letters/digits already assigned in Unicode 6.1,
/// the tokenizer fixed by schema::steps. One such character guarantees a token;
/// all other names still use the exact fallback, independent of the host Unicode version.
fn has_known_token(name: &str) -> bool {
    name.chars().any(|c| c.is_ascii_alphanumeric() || matches!(c as u32,
        0x00c0..=0x00d6 | 0x00d8..=0x00f6 | 0x00f8..=0x00ff |
        0x3041..=0x3096 | 0x30a1..=0x30fa | 0x4e00..=0x9fcb | 0xac00..=0xd7a3))
}

/// The expressions deliberately match track cells, not album-wide Refine fields.
fn expression(field: &str) -> Option<&'static str> {
    Some(match field {
        "title" => "t.title",
        "artist" => "t.artist",
        "album" => "coalesce(a.title,'')",
        "source" => "(SELECT provider || ':' || instance FROM sources WHERE id=t.source_id)",
        "rating" => "CAST(coalesce((SELECT stars FROM track_ratings WHERE track_id=t.id),0) AS TEXT)",
        "format" => "chilpa_format(t.external_id,t.codec,t.bit_depth,t.sample_rate_hz)",
        "quality" => "CASE chilpa_quality(t.external_id,t.codec,t.bit_depth,t.sample_rate_hz) WHEN 1 THEN 'lossy' WHEN 2 THEN 'lossless' WHEN 3 THEN 'hires' WHEN 4 THEN 'dsd' ELSE 'unknown' END",
        "bits" => "CASE WHEN t.bit_depth>0 THEN CAST(t.bit_depth AS TEXT) ELSE '' END",
        "rate" => "CASE WHEN t.sample_rate_hz>0 THEN CAST(t.sample_rate_hz AS TEXT) ELSE '' END",
        "favorite" => "CASE WHEN t.id IN (SELECT track_id FROM track_favorites) THEN 'true' ELSE 'false' END",
        _ => return None,
    })
}

fn folded(expression: &str) -> String {
    format!("CASE WHEN ({expression}) NOT GLOB '*[^ -~]*' THEN lower({expression}) ELSE chilpa_fold({expression}) END")
}

impl Library {
    /// Returns at most 101 distinct keys. The extra key indicates another page.
    /// The cursor is the last exact key, encoded as JSON so an empty name is valid.
    /// SQLite owns grouping scratch space only for this statement; no names cache remains.
    pub fn track_sheet_choices(&self, filter: &Filter, field: &str, search: &str, after: Option<&str>)
        -> rusqlite::Result<Vec<(String, usize)>> {
        if field == "artist" && !search.is_empty() { return self.track_sheet_artists(filter, search, after); }
        let name = expression(field).ok_or(rusqlite::Error::InvalidQuery)?;
        let order = if field == "bits" || field == "rate" { format!("CAST(({name}) AS INTEGER)") } else { format!("({name}) COLLATE BINARY") };
        let mut filter = filter.clone();
        filter.sheet.without(field);
        let (mut clause, mut args) = super::browse::clause(&filter);
        if !search.is_empty() {
            if field == "source" {
                let label = "CASE WHEN label<>'' THEN label WHEN instance='default' THEN provider ELSE instance END";
                clause.push_str(&format!(" AND t.source_id IN (SELECT id FROM sources WHERE instr({},?)>0 OR instr({},?)>0)",
                    folded("provider || ':' || instance"), folded(label)));
                args.push(Box::new(super::query::fold(search)));
            } else if field == "album" {
                clause.push_str(&format!(" AND t.album_id IN (SELECT id FROM albums WHERE instr({},?)>0)", folded("title")));
            } else { clause.push_str(&format!(" AND instr({},?)>0", folded(name))); }
            args.push(Box::new(super::query::fold(search)));
        }
        if let Some(after) = after {
            clause.push_str(&format!(" AND {order} > {}", if field == "bits" || field == "rate" { "CAST(? AS INTEGER)" } else { "?" }));
            args.push(Box::new(after.to_string()));
        }
        let sql = format!("SELECT {name} AS name,count(*) FROM tracks t LEFT JOIN albums a ON a.id=t.album_id
            {clause} GROUP BY name COLLATE BINARY ORDER BY {order} LIMIT {}", PAGE_SIZE + 1);
        let mut statement = self.connection().prepare(&sql)?;
        let rows = statement.query_map(params_from_iter(args.iter()), |row| Ok((row.get(0)?, row.get::<_, i64>(1)? as usize)))?;
        rows.collect()
    }

    fn track_sheet_artists(&self, filter: &Filter, search: &str, after: Option<&str>) -> rusqlite::Result<Vec<(String,usize)>> {
        let mut filter = filter.clone();
        filter.sheet.without("artist");
        let (clause, own) = super::browse::clause(&filter);
        let mut args: Vec<Box<dyn ToSql>> = vec![Box::new(super::query::fold(search))];
        let cursor = if let Some(after) = after {
            args.push(Box::new(after.to_string())); " AND name COLLATE BINARY > ?"
        } else { "" };
        args.extend(own);
        let sql = format!("WITH names AS MATERIALIZED (SELECT artist AS name FROM tracks INDEXED BY tracks_artist GROUP BY artist),
            candidates AS MATERIALIZED (SELECT name FROM names WHERE instr({},?)>0{cursor})
            SELECT candidates.name,count(*) FROM candidates
            CROSS JOIN tracks t INDEXED BY tracks_artist ON t.artist=candidates.name COLLATE BINARY
            LEFT JOIN albums a ON a.id=t.album_id {clause}
            GROUP BY candidates.name COLLATE BINARY ORDER BY candidates.name COLLATE BINARY LIMIT {}",
            folded("name"), PAGE_SIZE+1);
        let mut statement = self.connection().prepare(&sql)?;
        let rows = statement.query_map(params_from_iter(args.iter()), |row| Ok((row.get(0)?,row.get::<_,i64>(1)? as usize)))?;
        rows.collect()
    }

}
