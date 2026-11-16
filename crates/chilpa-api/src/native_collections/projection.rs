//! Small presentation rows carry short tokens with validated content fingerprints.
use super::{
    CHUNK_BYTES, Job,
    store::{Page, Query, error},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub(super) fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub(super) fn content(value: &Value) -> String {
    hash(value["items"].to_string().as_bytes())
}
pub(super) fn revision(value: &Value) -> Option<String> {
    value
        .get("revision")
        .filter(|value| !value.is_null())
        .map(|value| hash(value.to_string().as_bytes()))
}
pub(super) fn next(value: &Value) -> Result<Option<String>, String> {
    match value.get("next") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(next)) if next.is_empty() => Ok(None),
        Some(Value::String(next)) if crate::native_page_request::valid_reference(next) => {
            Ok(Some(next.clone()))
        }
        _ => Err("The collection cursor is invalid.".into()),
    }
}
pub(super) fn label(value: &Value, limit: usize) -> String {
    let name = value.as_str().unwrap_or_default();
    if name.chars().count() <= limit {
        return name.to_string();
    }
    name.chars()
        .take(limit.saturating_sub(1))
        .chain(std::iter::once('…'))
        .collect()
}
pub(super) fn header(value: &Value, query: &Query) -> Result<Value, String> {
    let base = value["query_ref"]
        .as_str()
        .filter(|reference| crate::native_page_request::valid_reference(reference))
        .or_else(|| query.params["ref"].as_str())
        .unwrap_or_default();
    let mut header = json!({"title":label(&value["title"],256),"plugin":query.plugin,
        "base_key":crate::native_page_request::route(&query.plugin,base,json!({}),"collection"),
        "filters":query.params.get("filters").cloned().unwrap_or_else(||json!({})),"order":query.params.get("order").cloned().unwrap_or_else(||json!(""))});
    for field in [
        "filters",
        "order",
        "filter_controls",
        "order_options",
        "actions",
        "playlist_edit",
        "updating",
        "refresh_after_ms",
        "retry_after_ms",
        "request_pending",
        "sync_loaded",
    ] {
        if let Some(value) = value.get(field) {
            header[field] = value.clone();
        }
    }
    // Unknown provider metadata never crosses the display boundary.
    if serde_json::to_vec(&header).map_err(error)?.len() > 128 * 1024 {
        return Err("The collection controls exceed their display limit.".into());
    }
    Ok(header)
}
pub(super) fn token(query: &Query, page: &Page, local: usize) -> String {
    format!(
        "{}:{}:{}:{}:{}",
        query.id, query.generation, page.ordinal, local, page.hash
    )
}
pub(super) fn parse_token(token: &str) -> Option<(i64, i64, usize, usize, &str)> {
    let parts = token.split(':').collect::<Vec<_>>();
    if parts.len() != 5
        || parts[4].len() != 64
        || !parts[4].bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return None;
    }
    Some((
        parts[0].parse().ok()?,
        parts[1].parse().ok()?,
        parts[2].parse().ok()?,
        parts[3].parse().ok()?,
        parts[4],
    ))
}
pub(super) fn row(item: &Value, token: String) -> Value {
    let mut row = json!({"rowToken":token,"kind":label(&item["kind"],64),"title":label(&item["title"],256),
        "artist":label(&item["artist"],256),"subtitle":label(&item["subtitle"],256),"label":label(&item["label"],256),
        "image":item["image"].as_str().filter(|image|image.len()<=4096).unwrap_or_default(),
        "available":item["available"]!=false,"favorite":item["favorite"]==true,"art_fit":label(&item["art_fit"],16)});
    for field in [
        "duration_ms",
        "year",
        "tracks",
        "disc_number",
        "track_number",
    ] {
        if item[field].is_number() {
            row[field] = item[field].clone();
        }
    }
    row
}
pub(super) fn document(query: &Query, job: &Job) -> Value {
    json!({"sessionId":job.session,"generation":query.generation,"dataRevision":query.generation,
        "requestId":job.request,"knownCount":query.known,"total":query.total,"hasMore":query.next.is_some(),"anchorIndex":query.anchor,
        "busy":false,"offline":false,"stale":false,"error":"","header":query.header})
}

/// Enforce the limit on the complete message, including header, errors and
/// escaping. Every omitted row remains reachable at the absolute nextOffset.
pub(super) fn chunk(
    query: &Query,
    job: &Job,
    page: &Page,
    wire: &Value,
    first: usize,
    limit: usize,
) -> Result<Value, String> {
    let local = first
        .checked_sub(page.first)
        .ok_or("The collection position is invalid.")?;
    let rows = wire["items"]
        .as_array()
        .ok_or("The collection has no items.")?;
    let projected = rows
        .iter()
        .enumerate()
        .skip(local)
        .take(limit)
        .map(|(at, item)| row(item, token(query, page, at)));
    fit(
        document(query, job),
        format!(
            "{}:{}:{}:{}",
            query.id, query.generation, page.ordinal, page.hash
        ),
        first,
        projected,
    )
}

pub(super) fn cached(
    query: &Query,
    job: &Job,
    chunk: &Value,
    first: usize,
    limit: usize,
) -> Result<Value, String> {
    cached_document(document(query, job), chunk, first, limit)
}

pub(super) fn cached_document(
    envelope: Value,
    chunk: &Value,
    first: usize,
    limit: usize,
) -> Result<Value, String> {
    let start = chunk["first"].as_u64().unwrap_or(0) as usize;
    let rows = chunk["rows"]
        .as_array()
        .ok_or("The cached collection is invalid.")?;
    let projected = rows
        .iter()
        .skip(first.saturating_sub(start))
        .take(limit)
        .cloned();
    fit(
        envelope,
        chunk["pageIdentity"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        first,
        projected,
    )
}

fn fit(
    mut envelope: Value,
    identity: String,
    first: usize,
    rows: impl Iterator<Item = Value>,
) -> Result<Value, String> {
    let mut chunk =
        json!({"pageIdentity":identity,"first":first,"count":0,"nextOffset":first,"rows":[]});
    let base = serde_json::to_vec(&envelope).map_err(error)?.len()
        + serde_json::to_vec(&chunk).map_err(error)?.len()
        + 256;
    let mut bytes = base;
    let mut projected = Vec::new();
    for row in rows {
        let size = serde_json::to_vec(&row).map_err(error)?.len() + 1;
        if bytes + size > CHUNK_BYTES {
            if projected.is_empty() {
                return Err("A collection row exceeds its display limit.".into());
            }
            break;
        }
        bytes += size;
        projected.push(row);
    }
    chunk["count"] = json!(projected.len());
    chunk["nextOffset"] = json!(first + projected.len());
    chunk["rows"] = json!(projected);
    envelope["page"] = chunk;
    if serde_json::to_vec(&envelope).map_err(error)?.len() > CHUNK_BYTES {
        return Err("The collection window exceeds its display limit.".into());
    }
    Ok(envelope)
}
