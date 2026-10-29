//! Update selection and startup validation share the normal plugin host.
use crate::{plugins::Plugins, PluginRepositories};
use serde_json::{json, Value};
use std::{path::Path, sync::{Arc, Mutex}};

pub(crate) fn newer(candidate: &str, installed: &str) -> bool {
    fn version(value: &str) -> Option<semver::Version> {
        let mut normalized = value.to_string();
        if value.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
            for _ in value.split('.').count()..3 { normalized.push_str(".0"); }
        }
        semver::Version::parse(&normalized).ok()
    }
    match (version(candidate), version(installed)) { (Some(next), Some(current)) => next.cmp_precedence(&current).is_gt(), _ => false }
}

pub(crate) fn native(root: &Path, plugins: &Arc<Mutex<Plugins>>, repositories: &PluginRepositories,
    input: &Value, state: &mut Value, automatic: bool, active: &std::sync::atomic::AtomicBool) -> Result<Option<String>, String> {
    let repo = input["repository"].as_str().unwrap_or("");
    let id = input["id"].as_str().unwrap_or("");
    let entry = repositories.cached(repo)?.into_iter().find(|entry| entry.id == id && entry.adapter == "native")
        .ok_or("The catalog entry is missing.")?;
    let installed = crate::native_install::installed_version(root, id);
    if installed.as_ref().is_some_and(|version| !newer(&entry.version, version)) {
        return Err("The catalog does not offer a newer plugin version.".into());
    }
    if installed.is_some() {
        let origin: Value = std::fs::read(root.join(id).join("source.json")).ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default();
        if origin["repository"] != repo { return Err("Use the repository that installed this plugin to update it.".into()); }
    }
    let package = repositories.stage(repo, id)?;
    let (package_root, manifest) = crate::native_install::check(&package, &entry)?;
    if installed.is_some() && active.load(std::sync::atomic::Ordering::Acquire) { return Err("Waiting for playback to stop.".into()); }
    let mut host = plugins.lock().map_err(|_| "The plugin host is unavailable.")?;
    let was_enabled = host.enabled(id);
    let needs_review = installed.is_some() && !host.covers_update(&manifest);
    let revision = {
        use sha2::Digest;
        format!("{:x}", sha2::Sha256::digest(std::fs::read(package_root.join("manifest.json")).map_err(|_| "The manifest could not be read.")?))
    };
    let reviewed = input["permission_revision"].as_str() == Some(revision.as_str());
    if needs_review && !reviewed {
        if automatic { return Err("The plugin update needs permission review.".into()); }
        state["approval"] = json!({"id":id,"name":manifest.name,"description":manifest.description,"version":manifest.version,
            "permissions":manifest.capabilities.iter().map(|cap|cap.describe()).collect::<Vec<_>>(),"revision":revision,
            "update":true,"repository":repo});
        return Ok(None);
    }
    activate_native(root, &mut host, &package, &entry, repo, needs_review && reviewed, was_enabled)?;
    state["approval"] = if installed.is_none() { super::plugin_center::permission_review(root, id)? } else { Value::Null };
    state["form"] = Value::Null;
    state["settings_loading_id"] = Value::Null;
    state["settings_id"] = Value::Null;
    Ok(host.natives.is_native(id).then(|| id.to_string()))
}

pub(crate) fn activate_native(root: &Path, host: &mut Plugins, package: &crate::StagedPackage,
    entry: &chilpa_plugin::catalog::CatalogEntry, repo: &str, grant: bool, was_enabled: bool) -> Result<(), String> {
    let (_, manifest) = crate::native_install::check(package, entry)?;
    let id = &entry.id;
    host.stop_for_update(id);
    let result = (|| {
        let transaction = crate::native_install::replace(root, package, entry, repo)?;
        if grant { host.approve_update(&manifest)?; }
        host.reload_one(id);
        if !host.update_healthy(id, was_enabled || grant) {
            host.stop_for_update(id);
            return Err("The new plugin could not start. The previous version was restored.".into());
        }
        transaction.commit()
    })();
    if result.is_err() { host.restore_update(id); }
    result
}

pub(crate) fn compatible(root: &Path, host: &mut Plugins, prepared: crate::PreparedPlugin, reviewed: bool) -> Result<(), String> {
    let id = prepared.manifest().id.clone();
    let old = crate::native_install::installed_version(root, &id);
    let needs_review = old.is_some() && !host.covers_update(prepared.manifest());
    if let Some(version) = &old {
        if !newer(&prepared.manifest().version, version) { return Err("The catalog does not offer a newer plugin version.".into()); }
        if needs_review && !reviewed { return Err("The plugin update needs permission review.".into()); }
    }
    let was_enabled = host.enabled(&id);
    let manifest = prepared.manifest().clone();
    host.stop_for_update(&id);
    let result = (|| {
        let transaction = prepared.replace(root)?;
        if needs_review && reviewed { host.approve_update(&manifest)?; }
        host.reload_one(&id);
        if old.is_none() { host.approve(&id)?; host.reload_one(&id); }
        if !host.update_healthy(&id, was_enabled || old.is_none() || needs_review && reviewed) {
            host.stop_for_update(&id);
            return Err("The new plugin could not start. The previous version was restored.".into());
        }
        transaction.commit()
    })();
    if result.is_err() { host.restore_update(&id); }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    fn package(root: &Path, version: &str, program: &[u8], extra: bool) -> (crate::StagedPackage, chilpa_plugin::catalog::CatalogEntry) {
        use sha2::{Digest, Sha256};
        let manifest = json!({"id":"app.test.update","name":"Fixture","version":version,"api_version":1,"kind":"source",
            "capabilities":if extra {json!([{"type":"register_source"},{"type":"storage"}])} else {json!([{"type":"register_source"}])}});
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, bytes) in [("manifest.json", serde_json::to_vec(&manifest).unwrap()), ("plugin.wasm", program.to_vec())] {
            zip.start_file(name, zip::write::SimpleFileOptions::default()).unwrap(); zip.write_all(&bytes).unwrap();
        }
        let bytes = zip.finish().unwrap().into_inner();
        let entry = chilpa_plugin::catalog::CatalogEntry {id:"app.test.update".into(),name:"Fixture".into(),version:version.into(),description:String::new(),category:String::new(),author:String::new(),repository:String::new(),package:"https://example.org/plugin.zip".into(),digest:chilpa_plugin::catalog::Digest {algorithm:"sha256".into(),value:format!("{:x}",Sha256::digest(&bytes))},adapter:"native".into(),problem:String::new()};
        (crate::StagedPackage::unpack(&root.join("staging"), &bytes, &entry.digest).unwrap(), entry)
    }
    #[test]
    fn updates_preserve_approval_and_disabled_state_and_restore_failed_startup() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("plugins");
        let program = wat::parse_str(r#"(module (memory (export "memory") 1)
            (func (export "chilpa_alloc") (param i32) (result i32) (i32.const 4096))
            (func (export "chilpa_api_version") (result i32) (i32.const 1)))"#).unwrap();
        let (first, entry) = package(directory.path(), "1.0.0", &program, false);
        crate::native_install::install(&root, &first, &entry, "native").unwrap();
        let mut host = Plugins::new(root.clone()).unwrap();
        host.load_all(); host.approve(&entry.id).unwrap(); host.reload_one(&entry.id);
        assert!(host.update_healthy(&entry.id, true));
        let approvals = std::fs::read(root.join("approvals.json")).unwrap();
        let (second, next) = package(directory.path(), "2.0.0", &program, false);
        activate_native(&root, &mut host, &second, &next, "native", false, true).unwrap();
        assert!(host.enabled(&entry.id));
        assert_eq!(std::fs::read(root.join("approvals.json")).unwrap(), approvals);
        let (bad, next) = package(directory.path(), "3.0.0", b"broken wasm", true);
        assert!(activate_native(&root, &mut host, &bad, &next, "native", true, true).is_err());
        assert_eq!(crate::native_install::installed_version(&root, &entry.id).as_deref(), Some("2.0.0"));
        assert!(host.update_healthy(&entry.id, true));
        assert_eq!(std::fs::read(root.join("approvals.json")).unwrap(), approvals, "failed updates restore the old permissions");
        host.revoke(&entry.id).unwrap(); host.reload_one(&entry.id);
        let (third, next) = package(directory.path(), "3.0.0", &program, false);
        activate_native(&root, &mut host, &third, &next, "native", false, false).unwrap();
        assert!(!host.enabled(&entry.id));
        assert!(host.update_healthy(&entry.id, false));
        let (bad, next) = package(directory.path(), "4.0.0", b"broken wasm", false);
        activate_native(&root, &mut host, &bad, &next, "native", false, false).unwrap();
        host.approve(&entry.id).unwrap(); host.reload_one(&entry.id);
        assert!(!host.update_healthy(&entry.id, true));
        let other = chilpa_plugin::Manifest::parse(r#"{"id":"app.test.other","name":"Other","version":"1.0.0","api_version":1,"kind":"source","capabilities":[]}"#).unwrap();
        host.approve_update(&other).unwrap();
        host.stop_for_update(&entry.id);
        assert!(crate::plugin_replacement::rollback_previous(&root, &entry.id).unwrap());
        host.restore_update(&entry.id);
        assert_eq!(crate::native_install::installed_version(&root, &entry.id).as_deref(), Some("3.0.0"));
        assert!(!host.enabled(&entry.id));
        let approvals: Value = serde_json::from_slice(&std::fs::read(root.join("approvals.json")).unwrap()).unwrap();
        assert!(!approvals["app.test.other"].is_null(), "delayed rollback must preserve other plugins' later approvals");
    }
    #[test]
    fn updates_use_version_precedence_and_never_downgrade() {
        assert!(newer("0.10.0", "0.9.0"));
        assert!(newer("1.0.0", "1.0.0-beta.2"));
        assert!(newer("2", "1"));
        for (next, current) in [("1.0.0", "1.0.0"), ("0.9.0", "0.10.0"),
            ("1.0.0-beta.2", "1.0.0"), ("1.0.0+new", "1.0.0+old"), ("broken", "1.0.0")] {
            assert!(!newer(next, current), "{next} must not replace {current}");
        }
    }
}
