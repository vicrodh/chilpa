//! The offline page's listing: every copy, grouped by record under its
//! artist, with the totals, the limit and where each server's files go.
//! It is read and arranged on a thread of its own and handed to the window
//! whole, one listing at a time; the window only draws it.

use super::limit::{choices, size, Limit};
use chilpa_library::{store::offline::Listed, Library, StoredServer};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

/// The most tracks one listing carries; an artist or a server narrows it.
pub(super) const MAX_SHOWN: usize = 2_000;
const MAX_FILTERS_BYTES: usize = 4096;

/// What the page asked to see.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Filters {
    /// `az`, `recent`, `largest` or `smallest`.
    pub(super) sort: String,
    pub(super) failed: bool,
    /// A server's source id, or 0 for all.
    pub(super) server: i64,
    /// An artist as the rail names it, or empty for all.
    pub(super) artist: String,
}

impl Filters {
    pub(super) fn parse(text: &str) -> Filters {
        let value: Value = if text.len() > MAX_FILTERS_BYTES { Value::Null } else { serde_json::from_str(text).unwrap_or(Value::Null) };
        let sort = value["sort"].as_str().filter(|sort| matches!(*sort, "az" | "recent" | "largest" | "smallest")).unwrap_or("az");
        Filters {
            sort: sort.into(),
            failed: value["failed"].as_bool().unwrap_or(false),
            server: value["server"].as_i64().unwrap_or(0),
            artist: value["artist"].as_str().unwrap_or("").chars().take(512).collect(),
        }
    }
}

/// What the window thread knows and the listing needs.
#[derive(Clone)]
pub(super) struct Context {
    pub(super) root: Result<PathBuf, String>,
    /// The offline folder as chosen: a URL, or empty for the default.
    pub(super) chosen: String,
    pub(super) limit: Limit,
}

#[derive(Default)]
pub(crate) struct Manager {
    computed: Option<Filters>,
    pub(super) stale: bool,
    revision: u64,
    doc: Arc<Mutex<Option<(u64, String)>>>,
    task: Option<std::thread::JoinHandle<()>>,
}

impl Manager {
    /// The listing for `filters`. A new one is read when the filters
    /// changed or the copies did; meanwhile the last one stays shown. The
    /// whole listing comes only when its revision is not `seen`. `said` is
    /// what the last change on the page answered, or empty; `waiting` keeps
    /// the page asking while a change of its own is still being written.
    pub(super) fn request(&mut self, seen: &str, filters: Filters, said: &str, waiting: bool, context: impl FnOnce() -> Context) -> String {
        if self.task.as_ref().is_some_and(|task| task.is_finished()) {
            if let Some(task) = self.task.take() {
                let _ = task.join();
            }
        }
        if self.task.is_none() && (self.stale || self.computed.as_ref() != Some(&filters)) {
            self.revision += 1;
            self.stale = false;
            self.computed = Some(filters.clone());
            let (revision, doc, context) = (self.revision, self.doc.clone(), context());
            self.task = std::thread::Builder::new()
                .name("chilpa-offline-listing".into())
                .spawn(move || {
                    let listing = Library::open_read_only(&Library::default_path())
                        .map(|lib| compute(&lib, &context, &filters))
                        .unwrap_or_else(|_| json!({"error": chilpa_i18n::t("the library could not be opened")}));
                    *doc.lock().unwrap_or_else(|error| error.into_inner()) = Some((revision, listing.to_string()));
                })
                .ok();
        }
        let loading = self.task.is_some() || waiting;
        let said = serde_json::to_string(said).unwrap_or_default();
        match &*self.doc.lock().unwrap_or_else(|error| error.into_inner()) {
            Some((revision, listing)) if revision.to_string() != seen => {
                format!(r#"{{"revision":{revision},"loading":{loading},"said":{said},"doc":{listing}}}"#)
            }
            Some((revision, _)) => format!(r#"{{"revision":{revision},"loading":{loading},"said":{said}}}"#),
            None => format!(r#"{{"revision":0,"loading":{loading},"said":{said}}}"#),
        }
    }
}

fn url(path: &std::path::Path) -> String {
    url::Url::from_directory_path(path).map(String::from).unwrap_or_default()
}

fn duration(ms: u64) -> String {
    let seconds = ms / 1000;
    if seconds >= 3600 {
        format!("{}:{:02}:{:02}", seconds / 3600, seconds / 60 % 60, seconds % 60)
    } else {
        format!("{}:{:02}", seconds / 60, seconds % 60)
    }
}

/// The record a copy is filed under, and the artist the rail names.
fn record(row: &Listed) -> (String, String) {
    let artist = if row.album_artist.is_empty() { row.artist.clone() } else { row.album_artist.clone() };
    let key = if row.track == 0 { "gone".to_string() } else if row.album > 0 { format!("album {}", row.album) } else { format!("loose {artist}") };
    (key, artist)
}

fn state(rows: &[&Listed]) -> &'static str {
    if rows.iter().any(|row| row.state == "failed") {
        "failed"
    } else if rows.iter().any(|row| matches!(row.state.as_str(), "downloading" | "queued" | "removing")) {
        "downloading"
    } else {
        "complete"
    }
}

fn server_entry(server: &StoredServer, usage: Option<&chilpa_library::store::offline::SourceUsage>, root: &Result<PathBuf, String>) -> Value {
    let kind = chilpa_servers::Kind::from_provider(&server.provider);
    let name = if server.label.is_empty() { kind.map(|k| k.name().to_string()).unwrap_or_default() } else { server.label.clone() };
    let inherited = match (kind, root) {
        (Some(kind), Ok(root)) => url(&root.join(kind.offline_folder_name(&server.product))),
        _ => String::new(),
    };
    let (files, bytes) = usage.map_or((0, 0), |usage| (usage.files, usage.bytes));
    json!({
        "id": server.source_id, "name": name, "icon": kind.map(|k| k.icon(&server.product)).unwrap_or("server"),
        "files": files, "size": size(bytes), "folder": server.offline_folder, "inherited": inherited,
    })
}

/// The listing, as the page draws it.
pub(super) fn compute(lib: &Library, context: &Context, filters: &Filters) -> Value {
    let servers = lib.servers().unwrap_or_default();
    let usage = lib.offline_usage_by_source().unwrap_or_default();
    let mut listed = lib.offline_listing().unwrap_or_default();
    let more = listed.len() > chilpa_library::store::offline::MAX_LISTED;
    listed.truncate(chilpa_library::store::offline::MAX_LISTED);
    let bytes: u64 = usage.iter().map(|usage| usage.bytes).sum();
    let files: usize = usage.iter().map(|usage| usage.files).sum();
    let failed = listed.iter().filter(|row| row.state == "failed").count();
    let (values, options): (Vec<String>, Vec<String>) = choices().into_iter().unzip();
    let in_scope: Vec<&Listed> = listed
        .iter()
        .filter(|row| (filters.server == 0 || row.source == filters.server) && (!filters.failed || row.state == "failed"))
        .collect();
    // The rail: every artist in scope, whatever artist is chosen.
    let mut artists: HashMap<String, (std::collections::HashSet<String>, usize)> = HashMap::new();
    for row in &in_scope {
        let (key, artist) = record(row);
        let entry = artists.entry(artist).or_default();
        entry.0.insert(key);
        entry.1 += 1;
    }
    let mut rail: Vec<(String, usize, usize)> = artists.into_iter().map(|(name, (albums, tracks))| (name, albums.len(), tracks)).collect();
    rail.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()).then(a.0.cmp(&b.0)));
    // The records of the chosen artist, each with its copies.
    let mut groups: Vec<(String, Vec<&Listed>)> = Vec::new();
    let mut place: HashMap<String, usize> = HashMap::new();
    for row in in_scope.iter().copied().filter(|row| filters.artist.is_empty() || record(row).1 == filters.artist) {
        let key = record(row).0;
        let at = *place.entry(key.clone()).or_insert_with(|| {
            groups.push((key, Vec::new()));
            groups.len() - 1
        });
        groups[at].1.push(row);
    }
    let total = |rows: &[&Listed]| rows.iter().map(|row| row.bytes).sum::<u64>();
    let latest = |rows: &[&Listed]| rows.iter().map(|row| row.changed_at).max().unwrap_or(0);
    let name = |rows: &[&Listed]| {
        let first = rows[0];
        let (_, artist) = record(first);
        (first.track == 0, artist.to_lowercase(), first.album_title.to_lowercase())
    };
    match filters.sort.as_str() {
        "recent" => groups.sort_by(|a, b| latest(&b.1).cmp(&latest(&a.1))),
        "largest" => groups.sort_by(|a, b| total(&b.1).cmp(&total(&a.1))),
        "smallest" => groups.sort_by(|a, b| total(&a.1).cmp(&total(&b.1))),
        _ => groups.sort_by(|a, b| name(&a.1).cmp(&name(&b.1))),
    }
    let matching: usize = groups.iter().map(|group| group.1.len()).sum();
    let mut rows = Vec::new();
    let mut shown = 0;
    for (_, mut copies) in groups {
        if shown >= MAX_SHOWN {
            break;
        }
        copies.sort_by(|a, b| (a.disc, a.number, a.title.to_lowercase()).cmp(&(b.disc, b.number, b.title.to_lowercase())));
        copies.truncate(MAX_SHOWN - shown);
        shown += copies.len();
        let first = copies[0];
        rows.push(json!({
            "kind": "album", "gone": first.track == 0, "album": first.album, "title": first.album_title,
            "artist": record(first).1, "art": first.artwork, "tracks": copies.len(), "size": size(total(&copies)),
            "state": state(&copies), "ids": copies.iter().filter(|row| row.track > 0).map(|row| row.track).collect::<Vec<_>>(),
        }));
        for row in copies {
            rows.push(json!({
                "kind": "track", "id": row.track, "key": row.key, "title": row.title, "artist": row.artist,
                "number": row.number, "duration": duration(row.duration_ms), "size": size(row.bytes),
                "state": row.state, "error": row.error,
            }));
        }
    }
    json!({
        "files": files, "size": size(bytes), "failed": failed, "more": more,
        "usage": context.limit.quota.map_or(0.0, |quota| (bytes as f64 / quota.max(1) as f64).min(1.0)),
        "full": context.limit.quota.is_some_and(|quota| bytes >= quota),
        "limit": context.limit.value(), "limitSize": context.limit.quota.map(size).unwrap_or_default(),
        "limits": {"values": values, "options": options},
        "root": {"folder": context.chosen, "path": context.root.as_ref().map(|root| url(root)).unwrap_or_default(), "error": context.root.as_ref().err()},
        "servers": servers.iter().map(|server| server_entry(server, usage.iter().find(|usage| usage.source == server.source_id), &context.root)).collect::<Vec<_>>(),
        "artists": rail.into_iter().map(|(name, albums, tracks)| json!({"name": name, "albums": albums, "tracks": tracks})).collect::<Vec<_>>(),
        "inScope": in_scope.len(), "matching": matching, "shown": shown, "rows": rows,
    })
}

#[cfg(test)]
mod tests;
