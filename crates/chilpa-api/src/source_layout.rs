//! User-owned presentation data maps a source tree onto library views.
use crate::plugins::Plugins;
use chilpa_plugin::{messages::Browse, presentation::SourceDocument};
use serde::Deserialize;
use std::{collections::BTreeMap, io::Read, path::Path, sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}}, time::{Duration, Instant}};

#[derive(Clone, Default, Deserialize)]
#[serde(default)]
pub(crate) struct Layout { pub sections: BTreeMap<String, Section> }
#[derive(Clone, Default, Deserialize)]
#[serde(default)]
pub(crate) struct Section { pub title: String, pub icon: String, pub entity: String, pub paths: Vec<Route>, pub order: i32 }
#[derive(Clone, Default, Deserialize)]
#[serde(default)]
pub(crate) struct Route { pub path: Vec<String>, pub membership: String }
impl Layout {
    /// The sections in the order the sidebar lists them: the source's own
    /// page first, then by their `order`, then by name.
    pub fn ordered(&self) -> Vec<(&String, &Section)> {
        let mut sections: Vec<_> = self.sections.iter().collect();
        sections.sort_by_key(|(key, section)| (key.as_str() != "discover", section.order, key.as_str()));
        sections
    }

    pub fn load(root: &Path, id: &str) -> Self {
        let load = || -> Option<Self> {
            let file = std::fs::File::open(root.join(".presentation").join(format!("{id}.json"))).ok()?;
            let mut bytes = Vec::new(); file.take(65537).read_to_end(&mut bytes).ok()?;
            if bytes.len() > 65536 { return None; }
            let layout: Self = serde_json::from_slice(&bytes).ok()?;
            if layout.sections.len() > 8 || layout.sections.values().any(|section| section.paths.len() > 16 || section.paths.iter().any(|route| route.path.len() > 5)) { return None; }
            Some(layout)
        };
        load().unwrap_or_default()
    }
}
pub(crate) fn browse(plugins: &Arc<Mutex<Plugins>>, source: &str, section: &Section, cancelled: &AtomicBool) -> Result<SourceDocument, String> {
    let started = Instant::now();
    let request = |at: &str, cursor: Option<String>| -> Result<SourceDocument, String> {
        let mut host = plugins.lock().map_err(|_| "The plugin host is unavailable.")?;
        if cancelled.load(Ordering::Relaxed) || started.elapsed() > Duration::from_secs(120) { return Err("The library request was cancelled or exceeded its time limit.".into()); }
        let bytes = host.call_waiting(source, chilpa_plugin::op::BROWSE,
            &chilpa_plugin::messages::write(&Browse { at: at.into(), cursor: cursor.clone(), text: None, language: "en".into(), picks: BTreeMap::new() }))?;
        drop(host);
        let document: SourceDocument = chilpa_plugin::messages::read(&bytes)?;
        document.clean(cursor.as_deref()).map_err(str::to_string)
    };
    collect(section, request)
}

pub(crate) fn collect(section: &Section, mut request: impl FnMut(&str, Option<String>) -> Result<SourceDocument, String>) -> Result<SourceDocument, String> {
    let root = request("", None)?;
    let mut out = SourceDocument::default();
    let mut identities = BTreeMap::<String, String>::new();
    'routes: for route in &section.paths {
        let mut document = root.clone();
        let mut location = String::new();
        for title in &route.path {
            let Some(entry) = document.listing.entries.iter().find(|entry| &entry.title == title) else { continue 'routes };
            location = entry.id.clone();
            document = request(&location, None)?;
        }
        let mut cursors = std::collections::BTreeSet::new();
        loop {
            for entry in document.listing.entries {
                let mut presentation = document.presentation.get(&entry.id).cloned().unwrap_or_default();
                use chilpa_plugin::presentation::Entity;
                if !matches!((&*section.entity, presentation.entity), ("album", Entity::Album) | ("playlist", Entity::Playlist) | ("artist", Entity::Artist) | ("track", Entity::Track)) { continue; }
                let identity = if presentation.stable_id.is_empty() { entry.id.clone() } else { presentation.stable_id.clone() };
                if let Some(existing) = identities.get(&identity) {
                    if let Some(display) = out.presentation.get_mut(existing) { display.metadata.entry("membership".into()).and_modify(|value| { value.push(','); value.push_str(&route.membership); }); }
                    continue;
                }
                identities.insert(identity, entry.id.clone());
                presentation.metadata.insert("membership".into(), route.membership.clone());
                out.presentation.insert(entry.id.clone(), presentation);
                out.listing.entries.push(entry);
                if out.listing.entries.len() > 10000 { return Err("This library exceeds the current collection limit.".into()); }
            }
            let Some(cursor) = document.listing.next else { break };
            if !cursors.insert(cursor.clone()) || cursors.len() > 100 { return Err("The library repeats or exceeds its page limit.".into()); }
            document = request(&location, Some(cursor))?;
        }
    }
    out.listing.title = section.title.clone();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn paths_merge_membership_and_follow_pages_without_rewriting_actions() {
        let section: Section = serde_json::from_value(json!({"title":"Library", "entity":"album", "paths":[
            {"path":["Saved", "Records"], "membership":"favorites"}, {"path":["Owned"], "membership":"purchases"}]})).unwrap();
        let menu = |items: serde_json::Value| serde_json::from_value::<SourceDocument>(json!({"entries":items})).unwrap();
        let album = |id: &str, next: Option<&str>| serde_json::from_value::<SourceDocument>(json!({"entries":[{"id":id,"title":"Record"}], "next":next,
            "presentation":{id:{"entity":"album", "stable_id":"album:one"}}})).unwrap();
        let mut requests = Vec::new();
        let result = collect(&section, |at, cursor| {
            requests.push((at.to_string(), cursor.clone()));
            Ok(match (at, cursor.as_deref()) {
                ("", _) => menu(json!([{"id":"saved","title":"Saved"},{"id":"owned","title":"Owned"}])),
                ("saved", _) => menu(json!([{"id":"records","title":"Records"}])),
                ("records", None) => album("handle:first", Some("page:two")),
                ("records", Some("page:two")) => menu(json!([])),
                ("owned", _) => album("handle:duplicate", None),
                _ => panic!("unexpected request"),
            })
        }).unwrap();
        assert_eq!(result.listing.entries.len(), 1);
        assert_eq!(result.listing.entries[0].id, "handle:first");
        assert_eq!(result.presentation["handle:first"].metadata["membership"], "favorites,purchases");
        assert!(requests.contains(&("records".into(), Some("page:two".into()))));
    }
    #[test]
    fn absent_optional_routes_are_empty_and_plugin_failures_remain_errors() {
        let section: Section = serde_json::from_value(json!({"paths":[{"path":["Absent"]}]})).unwrap();
        assert!(collect(&section, |_, _| Ok(SourceDocument::default())).unwrap().listing.entries.is_empty());
        assert!(collect(&section, |_, _| Err("worker failed".into())).is_err());
    }
}
