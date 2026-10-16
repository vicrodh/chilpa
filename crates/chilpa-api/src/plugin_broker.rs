//! Capability-gated cooperation between process plugins, outside the UI and registry locks.
use crate::natives::Natives;
use chilpa_plugin::Capability;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};
mod library;

#[derive(Default)]
pub(crate) struct Broker {
    grants: Mutex<HashMap<String, Grant>>,
    running: AtomicUsize,
    pub changed: AtomicBool,
}
struct Grant {
    owner: String,
    targets: HashSet<String>,
    catalog_sources: HashSet<String>,
    created: HashSet<(String, String)>,
    expires: Instant,
}
impl Broker {
    pub fn grant(&self, owner: &str, targets: &[String], catalog_sources: &[String]) -> String {
        let token = rand::random::<[u8; 32]>()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let mut grants = self.grants.lock().unwrap_or_else(|e| e.into_inner());
        grants.retain(|_, g| g.expires > Instant::now() && g.owner != owner);
        grants.insert(
            token.clone(),
            Grant {
                owner: owner.into(),
                targets: targets.iter().cloned().collect(),
                catalog_sources: catalog_sources.iter().cloned().collect(),
                created: HashSet::new(),
                expires: Instant::now() + Duration::from_secs(4 * 3600),
            },
        );
        token
    }
    pub fn revoke(&self, owner: &str) {
        self.grants
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|_, g| g.owner != owner);
    }
    fn allowed(
        &self,
        owner: &str,
        input: &Value,
        target: &str,
        reference: Option<&str>,
    ) -> Result<HashSet<String>, String> {
        let grants = self.grants.lock().unwrap_or_else(|e| e.into_inner());
        let grant = grants
            .get(input["grant"].as_str().unwrap_or_default())
            .filter(|g| g.owner == owner && g.expires > Instant::now())
            .ok_or("This import no longer has permission to write playlists.")?;
        if !grant.targets.contains(target) {
            return Err("This destination was not selected for the import.".into());
        }
        if let Some(reference) = reference {
            if !grant.created.contains(&(target.into(), reference.into())) {
                return Err("This playlist was not created by the import.".into());
            }
        }
        Ok(grant.catalog_sources.clone())
    }
    fn created(&self, input: &Value, target: &str, reference: &str) {
        if let Some(grant) = self
            .grants
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_mut(input["grant"].as_str().unwrap_or_default())
        {
            grant.created.insert((target.into(), reference.into()));
        }
        self.changed.store(true, Ordering::Release);
    }
}

pub(crate) fn dispatch(natives: &Arc<Natives>, owner: &str, input: Value) {
    let owner = owner.to_string();
    let Some(client) = natives.client(&owner) else {
        return;
    };
    let id = input["id"].clone();
    if !id.is_string() {
        return;
    }
    if natives.broker.running.fetch_add(1, Ordering::AcqRel) >= 16 {
        natives.broker.running.fetch_sub(1, Ordering::AcqRel);
        client.notify(
            "host.reply",
            json!({"id":id,"error":"The catalog broker is busy. Try again shortly."}),
        );
        return;
    }
    let natives = natives.clone();
    std::thread::spawn(move || {
        let result = handle(
            &natives,
            &owner,
            input["method"].as_str().unwrap_or_default(),
            &input["params"],
        );
        let reply = match result {
            Ok(value) => json!({"id":id,"result":value}),
            Err(error) => json!({"id":id,"error":error}),
        };
        client.notify("host.reply", reply);
        natives.broker.running.fetch_sub(1, Ordering::AcqRel);
    });
}
fn require(natives: &Natives, owner: &str, capability: Capability) -> Result<(), String> {
    if natives.permits(owner, &capability) {
        Ok(())
    } else {
        Err("This plugin has not been granted the required permission.".into())
    }
}
pub(crate) fn sources(natives: &Natives) -> Vec<Value> {
    let mut ids = natives.ids();
    ids.sort();
    ids.into_iter().filter(|id| natives.searches(id)).map(|id| json!({"name":natives.display_name(&id),"writable":natives.edits_playlists(&id),"playlist_limit":natives.playlist_limit(&id),"id":id})).collect()
}
fn track(value: &Value, source: &str) -> Option<Value> {
    if value["kind"] != "track" || value["ref"].as_str()?.is_empty() {
        return None;
    }
    let mut result = json!({"kind":"track","source":source});
    for key in [
        "ref",
        "title",
        "artist",
        "album",
        "album_ref",
        "image",
        "duration_ms",
        "isrc",
        "available",
    ] {
        if let Some(value) = value
            .get(key)
            .filter(|v| v.is_string() || v.is_number() || v.is_boolean())
        {
            result[key] = value.clone();
        }
    }
    if let Some(quality) = value.get("quality") {
        for key in ["codec", "tier", "bits", "rate"] {
            if let Some(value) = quality.get(key).filter(|v| v.is_string() || v.is_number()) {
                result["quality"][key] = value.clone();
            }
        }
    }
    Some(result)
}
fn handle(
    natives: &Arc<Natives>,
    owner: &str,
    method: &str,
    input: &Value,
) -> Result<Value, String> {
    match method {
        "sources.list" => {
            require(natives, owner, Capability::PluginsRead)?;
            Ok(json!({"sources":sources(natives)}))
        }
        "library.search" => {
            require(natives, owner, Capability::LibraryRead)?;
            library::search(input)
        }
        "catalog.search" => {
            require(natives, owner, Capability::PluginsRead)?;
            let source = input["source"].as_str().unwrap_or_default();
            if source == owner || !natives.searches(source) || !natives.is_native(source) {
                return Err("The selected source is not available for search.".into());
            }
            let text = input["text"].as_str().unwrap_or_default();
            if text.len() > 1000 {
                return Err("The search text is too long.".into());
            }
            let answer = natives.call(
                source,
                "search",
                json!({"text":text,"limit":input["limit"].as_u64().unwrap_or(20).clamp(1,50)}),
                Duration::from_secs(30),
            )?;
            Ok(
                json!({"tracks":answer["tracks"].as_array().into_iter().flatten().take(100).filter_map(|item|track(item, source)).collect::<Vec<_>>()}),
            )
        }
        "playlists.create" => {
            require(natives, owner, Capability::PlaylistsWrite)?;
            let target = input["target"].as_str().unwrap_or_default();
            let allowed = natives.broker.allowed(owner, input, target, None)?;
            let title = input["title"].as_str().unwrap_or_default().trim();
            if title.is_empty() || title.len() > 500 {
                return Err("Enter a playlist name of at most 500 bytes.".into());
            }
            let result = if matches!(target, "local" | "mixed") {
                library::create(input, &allowed)?
            } else {
                if !natives.edits_playlists(target) {
                    return Err("This source cannot create playlists now.".into());
                }
                let result = natives.call(target,"page.action",json!({"action":"playlist.create","title":title,"description":input["description"],"public":input["public"].as_bool().unwrap_or(false)}),Duration::from_secs(30))?;
                let reference = result["item"]["ref"].as_str().or_else(||result["ref"].as_str()).ok_or("The source created a playlist without returning its reference. Check the source before retrying.")?;
                // A remote write cannot be undone by a folder-cache failure.
                let warning = library::remember_remote(input, reference).err();
                json!({"ref":reference,"warning":warning})
            };
            let reference = result["ref"]
                .as_str()
                .ok_or("The created playlist has no reference.")?;
            natives.broker.created(input, target, reference);
            Ok(result)
        }
        "playlists.add" => {
            require(natives, owner, Capability::PlaylistsWrite)?;
            let target = input["target"].as_str().unwrap_or_default();
            let reference = input["ref"].as_str().unwrap_or_default();
            natives
                .broker
                .allowed(owner, input, target, Some(reference))?;
            if matches!(target, "local" | "mixed") {
                return Err("Local playlist entries are written atomically when created.".into());
            }
            if !natives.edits_playlists(target) {
                return Err("The destination is disconnected.".into());
            }
            let tracks = input["tracks"]
                .as_array()
                .filter(|v| !v.is_empty() && v.len() <= 50)
                .ok_or("Add between one and 50 tracks per batch.")?;
            if tracks.iter().any(|r| !r.is_string()) {
                return Err("Invalid track references.".into());
            }
            natives.call(
                target,
                "page.action",
                json!({"action":"playlist.add","ref":reference,"tracks":tracks}),
                Duration::from_secs(30),
            )?;
            natives.broker.changed.store(true, Ordering::Release);
            Ok(json!({"added":tracks.len()}))
        }
        _ => Err("This host operation is not exposed to plugins.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn writes_require_owner_destination_and_a_playlist_created_in_the_grant() {
        let broker = Broker::default();
        let token = broker.grant("importer", &["source-a".into()], &["source-a".into()]);
        let input = json!({"grant":token});
        assert!(broker.allowed("importer", &input, "source-a", None).is_ok());
        assert!(broker.allowed("other", &input, "source-a", None).is_err());
        assert!(broker
            .allowed("importer", &input, "source-b", None)
            .is_err());
        assert!(broker
            .allowed("importer", &input, "source-a", Some("playlist:old"))
            .is_err());
        broker.created(&input, "source-a", "playlist:new");
        assert!(broker
            .allowed("importer", &input, "source-a", Some("playlist:new"))
            .is_ok());
        broker.revoke("importer");
        assert!(broker
            .allowed("importer", &input, "source-a", None)
            .is_err());
    }
    #[test]
    fn catalog_projection_never_exposes_account_or_playback_data() {
        let value = json!({"kind":"track","ref":"track:1","title":"First","access_token":"secret","url":"signed audio","account":{"password":"secret"},"quality":{"bits":24,"token":"secret"}});
        let projected = track(&value, "source-a").unwrap();
        assert_eq!(projected["quality"]["bits"], 24);
        assert!(!projected.to_string().contains("secret"));
        assert!(!projected.to_string().contains("signed audio"));
        assert!(track(&json!({"kind":"album","ref":"album:1"}), "source-a").is_none());
    }
    #[test]
    fn absent_permissions_and_unknown_operations_fail_closed() {
        let natives = Arc::new(Natives::default());
        for operation in [
            "sources.list",
            "library.search",
            "catalog.search",
            "playlists.create",
            "playlists.add",
            "secrets.read",
            "auth.login",
        ] {
            assert!(
                handle(&natives, "unknown", operation, &json!({})).is_err(),
                "{operation}"
            );
        }
    }
}
