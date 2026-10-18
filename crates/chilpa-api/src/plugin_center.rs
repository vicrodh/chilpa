//! Background catalog, installation and compatibility settings workflow.
#[path = "plugin_export.rs"]
pub(crate) mod export;
use crate::{plugins::Plugins, Chilpa, PluginRepositories, PreparedPlugin};
use serde_json::{json, Value};
use std::{path::PathBuf, sync::{Arc, Mutex, mpsc, atomic::{AtomicBool, AtomicUsize, Ordering}}};

pub(crate) struct PluginCenter {
    commands: Arc<mpsc::SyncSender<(String, Value)>>,
    view: Arc<Mutex<Value>>,
    busy: Arc<AtomicUsize>,
    foreground: Arc<AtomicBool>,
    library_dirty: Arc<AtomicBool>,
    operation: Arc<Mutex<String>>,
    removed: Arc<Mutex<Vec<String>>>,
    refreshed: Arc<Mutex<Vec<String>>>,
    inbox: Arc<Mutex<crate::plugin_notifications::Inbox>>,
    playback: Arc<AtomicBool>,
}

impl PluginCenter {
    fn start(plugins: Arc<Mutex<Plugins>>, root: PathBuf, catalog: Arc<crate::source_catalog::Catalog>) -> Self {
        let (sender, receiver) = mpsc::sync_channel::<(String, Value)>(2);
        let commands = Arc::new(sender);
        let view = Arc::new(Mutex::new(json!({})));
        let shared = view.clone();
        let busy = Arc::new(AtomicUsize::new(0));
        let active = busy.clone();
        let foreground = Arc::new(AtomicBool::new(false));
        let visible = foreground.clone();
        let library_dirty = Arc::new(AtomicBool::new(false));
        let changed = library_dirty.clone();
        let operation = Arc::new(Mutex::new(String::new()));
        let current = operation.clone();
        let removed = Arc::new(Mutex::new(Vec::new()));
        let deleted = removed.clone();
        let refreshed = Arc::new(Mutex::new(Vec::new()));
        let reload = refreshed.clone();
        let inbox = Arc::new(Mutex::new(crate::plugin_notifications::Inbox::default()));
        let notifications = inbox.clone();
        let playback = Arc::new(AtomicBool::new(true));
        let playing = playback.clone();
        let completions = Arc::downgrade(&commands);
        std::thread::spawn(move || {
            let launcher = plugins.lock().ok().and_then(|host| host.compatibility_launcher());
            let mut evidence = crate::plugin_evidence::Evidence::load(&root.join(".repositories"),
                launcher.and_then(|launcher| launcher.inspection_revision()));
            let mut pending: Option<PreparedPlugin> = None;
            let mut state = json!({"repositories":[],"catalog":[],"installed":[],"form":null,"proposal":null});
            let mut updates = crate::plugin_notifications::Inbox::load(&root.join(".repositories"));
            *notifications.lock().unwrap() = updates.clone();
            loop {
                let (action, input) = match receiver.recv_timeout(std::time::Duration::from_secs(60)) {
                    Ok(command) => command,
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        if active.compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire).is_err() { continue; }
                        ("updates_tick".to_string(), json!({}))
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                };
                if action.starts_with("updates_") {
                    let result = (|| {
                        let repositories = PluginRepositories::load(root.join(".repositories"))?;
                        if matches!(action.as_str(), "updates_automatic" | "updates_read" | "updates_apply" | "updates_app_read") {
                            updates.action(&action, &input, &root.join(".repositories"))?;
                        }
                        let checked = action == "updates_check" || updates.due();
                        if checked { let _ = updates.check(&root, &plugins, &repositories); }
                        let applied = updates.apply(&root, &plugins, &repositories, &playing, &mut state)?;
                        if checked || applied {
                            run("filter", &json!({}), &root, &plugins, &mut pending, &mut state, &mut evidence, &playing)?;
                            let states = plugins.lock().map_err(|_| "The plugin host is unavailable.")?.catalog_states();
                            catalog.set_source_states(states);
                            changed.store(true, Ordering::Release);
                        }
                        Ok::<(), String>(())
                    })();
                    *notifications.lock().unwrap() = updates.clone();
                    if let Err(error) = result { state["error"] = error.into(); }
                    *shared.lock().unwrap() = state.clone();
                    if action != "updates_tick" {
                        current.lock().unwrap().clear(); visible.store(false, Ordering::Release);
                    }
                    active.fetch_sub(1, Ordering::AcqRel);
                    continue;
                }
                if action == "native_settings_result" {
                    crate::plugin_evidence::log_event(&root.join(".repositories"), input["action"].as_str().unwrap_or("settings"),
                        input.get("form").is_some(), std::time::Duration::from_millis(input["elapsed_ms"].as_u64().unwrap_or(0)));
                    if state["settings_loading_id"] == input["id"] {
                        state["settings_loading_id"] = Value::Null;
                        if let Some(form) = input.get("form") {
                            state["form"] = form.clone();
                            state["pages"] = json!([]);
                            state["error"] = Value::Null;
                            state["maintenance_failed"] = false.into();
                            if input["action"] == "submit" {
                                if let Some(id) = input["id"].as_str() { reload.lock().unwrap().push(id.to_string()); }
                            }
                        } else {
                            state["error"] = input["error"].clone();
                        }
                        if let Ok(mut output) = shared.lock() { *output = state.clone(); }
                    }
                    continue;
                }
                if matches!(action.as_str(), "settings" | "submit") {
                    let id = input["id"].as_str().unwrap_or("");
                    let native = plugins.lock().ok().and_then(|host| host.natives.is_native(id).then(|| host.natives.clone()));
                    if let Some(natives) = native {
                        let method = if action == "submit" { "settings.submit" } else { "settings.open" };
                        let params = if action == "submit" { input["submission"].clone() } else { json!({}) };
                        state["settings_loading_id"] = id.into();
                        state["settings_id"] = id.into();
                        state["form"] = Value::Null;
                        state["error"] = Value::Null;
                        if let Ok(mut output) = shared.lock() { *output = state.clone(); }
                        let id = id.to_string();
                        let completed_action = action.clone();
                        if let Some(sender) = completions.upgrade() {
                            std::thread::spawn(move || {
                                let started = std::time::Instant::now();
                                let result = natives.call(&id, method, params, std::time::Duration::from_secs(15));
                                let input = match result {
                                    Ok(form) => json!({"id":id,"action":completed_action,"elapsed_ms":started.elapsed().as_millis() as u64,"form":form}),
                                    Err(error) => json!({"id":id,"action":completed_action,"elapsed_ms":started.elapsed().as_millis() as u64,
                                        "error":if error == "the plugin did not answer in time" { chilpa_i18n::t("The plugin did not answer within 15 seconds. Its account status is unknown.") }
                                            else { error }}),
                                };
                                let _ = sender.send(("native_settings_result".into(), input));
                            });
                        }
                        if let Ok(mut operation) = current.lock() { operation.clear(); }
                        visible.store(false, Ordering::Release);
                        active.fetch_sub(1, Ordering::AcqRel);
                        continue;
                    }
                    state["settings_loading_id"] = Value::Null;
                }
                let started = std::time::Instant::now();
                let result = run(&action, &input, &root, &plugins, &mut pending, &mut state, &mut evidence, &playing);
                if result.is_ok() && matches!(action.as_str(), "enable" | "disable" | "install" | "install_native" | "install_local" | "restart" | "grant_network") {
                    let states = plugins.lock().ok().map(|host| host.catalog_states());
                    if let Some(states) = states { catalog.set_source_states(states); }
                }
                if result.is_ok() && matches!(action.as_str(), "submit" | "integration" | "enable" | "restart" | "grant_network") { if let Some(id) = input["id"].as_str() { reload.lock().unwrap().push(id.to_string()); } }
                if result.is_ok() && action == "uninstall" { if let Some(id) = input["id"].as_str() { deleted.lock().unwrap().push(id.to_string()); } }
                if result.is_ok() && matches!(action.as_str(), "library_visibility" | "integration" | "sidebar_position" | "download_access" | "enable" | "disable" | "restart" | "grant_network" | "uninstall" | "tint" | "icon") { changed.store(true, Ordering::Release); }
                if action == "prepare" {
                    state["validation"] = json!({"id":input["id"],"repository_id":input["repository"],
                        "state":if result.is_ok() {"passed"} else {"failed"},
                        "light":if result.is_ok() {"green"} else {"red"},
                        "problem":result.as_ref().err()});
                }
                if !matches!(action.as_str(), "tick" | "downloads_tick") || result.is_err() {
                    crate::plugin_evidence::log_event(&root.join(".repositories"), &action, result.is_ok(), started.elapsed());
                }
                if action == "tick" && result.is_err() { state["maintenance_failed"] = true.into(); }
                if matches!(action.as_str(), "settings" | "restart") && result.is_ok() { state["maintenance_failed"] = false.into(); }
                if !matches!(action.as_str(), "tick" | "downloads_tick") || result.is_err() { state["error"] = result.err().unwrap_or_default().into(); }
                if let Ok(host) = plugins.lock() {
                    if let Ok(requests) = host.network_requests.lock() { state["network_requests"] = json!(*requests); }
                }
                if let Ok(mut output) = shared.lock() { *output = state.clone(); }
                if !matches!(action.as_str(), "tick" | "downloads_tick") {
                    if let Ok(mut operation) = current.lock() { operation.clear(); }
                    visible.store(false, Ordering::Release);
                }
                active.fetch_sub(1, Ordering::AcqRel);
            }
        });
        Self { commands, view, busy, foreground, operation, library_dirty, removed, refreshed, inbox, playback }
    }
    fn command(&self, action: &str, input: Value) {
        let background = matches!(action, "tick" | "downloads_tick" | "updates_tick");
        if background {
            if self.foreground.load(Ordering::Acquire) || self.busy.compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire).is_err() { return; }
        } else {
            if self.foreground.swap(true, Ordering::AcqRel) { return; }
            self.busy.fetch_add(1, Ordering::AcqRel);
            if let Ok(mut operation) = self.operation.lock() { *operation = action.into(); }
        }
        // One foreground action can wait behind maintenance without being lost.
        if self.commands.try_send((action.to_string(), input)).is_err() {
            if !background { self.foreground.store(false, Ordering::Release); }
            self.busy.fetch_sub(1, Ordering::AcqRel);
        }
    }

    fn snapshot(&self) -> Value {
        let mut value = self.view.lock().map(|value| value.clone()).unwrap_or(json!({}));
        value["operation"] = self.operation.lock().map(|operation| operation.clone()).unwrap_or_default().into();
        value["busy"] = self.foreground.load(Ordering::Acquire).into();
        value["working"] = (self.busy.load(Ordering::Acquire) != 0).into();
        value
    }
}

impl Chilpa {
    pub fn plugin_notifications(&mut self, revision: u64) -> Option<(u64, String)> {
        self.ensure_plugin_center();
        self.plugin_center.as_ref()?.inbox.lock().ok()?.snapshot(revision)
    }
    pub fn plugin_updates_command(&mut self, action: &str, input: &str) {
        self.ensure_plugin_center();
        self.plugin_center_command(action, input);
    }
    fn ensure_plugin_center(&mut self) {
        if self.plugin_center.is_none() {
            if self.plugins.is_none() { self.start_plugins(); }
            if let Some(plugins) = &self.plugins {
                let center = PluginCenter::start(plugins.clone(), Self::plugins_dir(), self.catalog.clone());
                center.command("refresh", json!({"initial":true}));
                self.plugin_center = Some(center);
            }
        }
    }
    pub(crate) fn plugin_update_activity(&self, playing: bool) {
        if let Some(center) = &self.plugin_center { center.playback.store(playing, Ordering::Release); }
    }
    pub fn plugin_center_json(&mut self) -> String {
        self.advance_plugins();
        self.ensure_plugin_center();
        self.advance_plugin_changes();
        self.plugin_center.as_ref().map(|center| center.snapshot().to_string()).unwrap_or_else(|| "{}".into())
    }
    pub(crate) fn advance_plugin_changes(&mut self) {
        let refreshed = self.plugin_center.as_ref().map(|c| std::mem::take(&mut *c.refreshed.lock().unwrap())).unwrap_or_default();
        for id in refreshed { if self.natives.is_native(&id) {
            for kind in self.natives.library_kinds(&id) { self.catalog.retry(&format!("{id} section:{kind}")); }
            if self.natives.has_pages(&id) { self.catalog.retry(&format!("{id} native:discover:home")); }
            self.mark_library_changed();
        } }
        let removed = self.plugin_center.as_ref().map(|c| std::mem::take(&mut *c.removed.lock().unwrap())).unwrap_or_default();
        for id in removed { self.sources.native_actions.remove(&id); self.forget_source_preferences(&id); self.purge_queue(|path| crate::source_opener::of_plugin(path).is_some_and(|(plugin, _)| plugin == id)); self.catalog.forget_plugin(&id); self.invalidate_source_menus(); }
        if self.plugin_center.as_ref().is_some_and(|center| center.library_dirty.swap(false, Ordering::AcqRel)) { self.mark_library_changed(); }
    }
    pub fn plugin_center_command(&mut self, action: &str, input: &str) {
        let Ok(input) = serde_json::from_str::<Value>(input) else { return };
        if matches!(action, "disable" | "uninstall" | "restart") {
            if let Some(id) = input["id"].as_str() { self.sources.native_actions.remove(id); }
        }
        if matches!(action, "install" | "install_native" | "install_local" | "enable" | "disable" | "uninstall" | "grant_network" | "restart") { self.invalidate_source_menus(); }
        if let Some(center) = &self.plugin_center { center.command(action, input); }
    }
}

fn run(action: &str, input: &Value, root: &std::path::Path, plugins: &Arc<Mutex<Plugins>>,
    pending: &mut Option<PreparedPlugin>, state: &mut Value, evidence: &mut crate::plugin_evidence::Evidence, playing: &AtomicBool) -> Result<(), String> {
    let mut repositories = PluginRepositories::load(root.join(".repositories"))?;
    let id = input["id"].as_str().unwrap_or("");
    let mut failure = None;
    match action {
        "add" => { let source = repositories.add(input["url"].as_str().unwrap_or(""))?; repositories.refresh(&source.id, "EN")?; }
        "remove_repository" => repositories.remove(id)?,
        "refresh_repository" => { repositories.refresh(id, "EN")?; }
        "prepare" => {
            eprintln!("plugin-inspection: started");
            *pending = None;
            state["proposal"] = Value::Null;
            let repo = input["repository"].as_str().unwrap_or("");
            let entry = repositories.cached(repo)?.into_iter().find(|entry| entry.id == id).ok_or("The catalog entry is missing.")?;
            let package = repositories.stage(repo, id)?;
            let launcher = plugins.lock().map_err(|_| "The plugin host is unavailable.")?.compatibility_launcher()
                .ok_or("The compatibility runtime is not installed.")?;
            let inspected = package.inspect(launcher.as_ref(), "EN");
            evidence.record(&root.join(".repositories"), repo, &entry, &inspected)?;
            match inspected {
                Ok(inspection) => {
                    let domains = crate::plugin_domains::candidates(&package);
                    let prepared = package.prepare_extension(repo, &entry, &inspection, domains.clone())?;
                    state["proposal"] = json!({"id":prepared.manifest().id,"catalog_id":entry.id,"repository_id":repo,"name":entry.name,"domains":domains,
                        "permissions":prepared.manifest().capabilities.iter().map(|cap| cap.describe()).collect::<Vec<_>>(),
                        "inspection":inspection,"light":"green"});
                    *pending = Some(prepared);
                }
                Err(error) => failure = Some(error),
            }
        }
        "install" => {
            let prepared = pending.take().ok_or("Inspect a package before installing it.")?;
            if root.join(&prepared.manifest().id).exists() && playing.load(Ordering::Acquire) { return Err("Stop playback before updating a plugin.".into()); }
            let mut host = plugins.lock().map_err(|_| "The plugin host is unavailable.")?;
            crate::plugin_updates::compatible(root, &mut host, prepared, true)?;
            state["proposal"] = Value::Null;
        }
        "install_native" => crate::plugin_updates::native(root, plugins, &repositories, input, state, false, playing)?,
        "install_local" => {
            let address = input["path"].as_str().unwrap_or("");
            let path = url::Url::parse(address).ok().and_then(|url| url.to_file_path().ok())
                .ok_or_else(|| chilpa_i18n::t("Choose a local ZIP file."))?;
            let package = crate::StagedPackage::local(&root.join(".repositories/staging"), &path)?;
            let (package_root, manifest) = crate::native_install::inspect(&package)?;
            let entry = chilpa_plugin::catalog::CatalogEntry {
                id: manifest.id.clone(), name: manifest.name.clone(), version: manifest.version.clone(),
                description: manifest.description.clone(), category: String::new(), author: String::new(),
                repository: String::new(), package: address.into(),
                digest: chilpa_plugin::catalog::Digest { algorithm: "sha256".into(), value: package.sha256.clone() },
                adapter: "native".into(), problem: String::new(),
            };
            let mut host = plugins.lock().map_err(|_| "The plugin host is unavailable.")?;
            let installed = crate::native_install::installed_version(root, &manifest.id);
            if installed.as_ref().is_some_and(|version| !crate::plugin_updates::newer(&manifest.version, version)) {
                return Err("Choose a newer plugin version.".into());
            }
            if installed.is_some() && playing.load(Ordering::Acquire) { return Err("Stop playback before updating a plugin.".into()); }
            let needs_review = installed.is_some() && !host.covers_update(&manifest);
            use sha2::Digest;
            let revision = format!("{:x}", sha2::Sha256::digest(std::fs::read(package_root.join("manifest.json")).map_err(|_| "The manifest could not be read.")?));
            let reviewed = input["permission_revision"].as_str() == Some(revision.as_str());
            if needs_review && !reviewed {
                state["approval"] = json!({"id":manifest.id,"name":manifest.name,"description":manifest.description,"version":manifest.version,
                    "permissions":manifest.capabilities.iter().map(|cap|cap.describe()).collect::<Vec<_>>(),"revision":revision,
                    "update":true,"local_update":true,"path":address});
            } else {
                let enabled = host.enabled(&manifest.id);
                crate::plugin_updates::activate_native(root, &mut host, &package, &entry, "local", needs_review && reviewed, enabled)?;
                state["approval"] = if installed.is_none() { permission_review(root, &manifest.id)? } else { Value::Null };
            }
            state["form"] = Value::Null;
        }
        "restart" => {
            plugins.lock().map_err(|_| "The plugin host is unavailable.")?.reactivate(id);
            state["form"] = Value::Null;
        }
        "review_permissions" => state["approval"] = permission_review(root, id)?,
        "cancel_permissions" => state["approval"] = Value::Null,
        "enable" | "disable" => {
            if action == "enable" && input["permission_revision"] != permission_review(root, id)?["revision"] { return Err("Review and accept the plugin permissions before enabling it.".into()); }
            let mut plugins = plugins.lock().map_err(|_| "The plugin host is unavailable.")?;
            if action == "enable" { plugins.approve(id)?; state["approval"] = Value::Null; } else { plugins.revoke(id)?; }
            plugins.reload_one(id);
            if action == "enable" && !plugins.update_healthy(id, true) {
                plugins.stop_for_update(id);
                if crate::plugin_replacement::rollback_previous(root, id)? {
                    plugins.restore_update(id);
                    return Err("The plugin could not start. The previous version was restored.".into());
                }
                plugins.revoke(id)?;
                plugins.reload_one(id);
                return Err("The plugin could not start.".into());
            }
            state["form"] = Value::Null;
        }
        "uninstall" => {
            let mut plugins = plugins.lock().map_err(|_| "The plugin host is unavailable.")?;
            plugins.revoke(id)?;
            plugins.reload_one(id);
            plugins.forget_credential(id);
            if !plugins.uninstall(id) { return Err("The plugin could not be removed.".into()); }
            state["settings_loading_id"] = Value::Null;
            export::clear(root, id);
            crate::plugin_access::clear(root, id);
            state["form"] = Value::Null;
            state["settings_id"] = Value::Null;
            state["approval"] = Value::Null;
        }
        "grant_network" => {
            plugins.lock().map_err(|_| "The plugin host is unavailable.")?.grant_network(id, input["domain"].as_str().unwrap_or(""))?;
            state["form"] = Value::Null;
        }
        "downloads_open" | "downloads_action" | "downloads_tick" => {
            let plugins = plugins.lock().map_err(|_| "The plugin host is unavailable.")?;
            if !plugins.enabled(id) || !plugins.natives.has_downloads(id) {
                return Err("The plugin's download view is unavailable.".into());
            }
            let natives = plugins.natives.clone();
            drop(plugins);
            let method = if action == "downloads_action" { "downloads.action" } else { "downloads.open" };
            let params = if action == "downloads_action" { input["request"].clone() } else { json!({}) };
            state["downloads"] = natives.call(id, method, params, std::time::Duration::from_secs(30))?;
            state["downloads_id"] = id.into();
            if action == "downloads_tick" { return Ok(()); }
        }
        "settings" | "submit" | "tick" => {
            let mut plugins = plugins.lock().map_err(|_| "The plugin host is unavailable.")?;
            if plugins.natives.is_native(id) {
                if action == "tick" { return Ok(()); }
                if !plugins.enabled(id) { return Err(chilpa_i18n::t("the plugin is not running")); }
                let natives = plugins.natives.clone();
                drop(plugins);
                if action == "settings" { state["form"] = Value::Null; }
                let method = if action == "submit" { "settings.submit" } else { "settings.open" };
                let params = if action == "submit" { input["submission"].clone() } else { json!({}) };
                state["form"] = natives.call(id, method, params, std::time::Duration::from_secs(30))?;
                state["pages"] = json!([]);
                state["settings_id"] = id.into();
            } else if action == "tick" {
                if plugins.enabled(id) && plugins.worker_domains(id).is_some() {
                    plugins.call_waiting(id, "tick", b"{}")?;
                }
                return Ok(());
            } else {
                if action == "settings" { state["form"] = Value::Null; }
                let operation = if action == "submit" { "settings.submit" } else { "settings.open" };
                let request = if action == "submit" { input["submission"].clone() } else {
                    let inspection: Value = serde_json::from_slice(&plugins.call_waiting(id, "inspect", b"{}")?).map_err(|_| "Invalid inspection.")?;
                    let pages = inspection["settings_pages"].as_array().ok_or("No settings pages.")?;
                    let selected = input["page"].as_str().filter(|page| pages.iter().any(|candidate| candidate.as_str() == Some(page)))
                        .or_else(|| pages.iter().filter_map(Value::as_str).find(|page| page.ends_with("/settings.html")))
                        .or_else(|| pages.first().and_then(Value::as_str)).ok_or("No settings pages.")?;
                    state["pages"] = inspection["settings_pages"].clone();
                    json!({"page":selected})
                };
                let bytes = plugins.call_waiting(id, operation, &serde_json::to_vec(&request).map_err(|_| "Invalid settings request.")?)?;
                state["form"] = serde_json::from_slice(&bytes).map_err(|_| "Invalid settings response.")?;
                state["settings_id"] = id.into();
            }
        }
        "icon" => {
            let path = input["path"].as_str().unwrap_or("");
            let path = if path.is_empty() { None } else {
                Some(url::Url::parse(path).ok().and_then(|url| url.to_file_path().ok()).ok_or("Choose a local image file.")?)
            };
            crate::plugin_icons::set(root, id, path.as_deref())?;
            if let Ok(library) = chilpa_core::open_library() {
                if let Ok(Some(source)) = library.find_source(crate::source_opener::PROVIDER, id) {
                    library.set_source_icon(source, &crate::plugin_icons::image_url(root, id)).map_err(|_| "The source icon could not be saved.")?;
                }
            }
        }
        "export_folder" => {
            let address = input["path"].as_str().ok_or("Choose a folder.")?;
            let path = url::Url::parse(address).ok().and_then(|url| url.to_file_path().ok()).ok_or("Choose a local folder.")?;
            export::set(root, id, &path)?;
            plugins.lock().map_err(|_| "The plugin host is unavailable.")?.reload_one(id);
            state["form"] = Value::Null;
        }
        "sidebar_position" => {
            let active = plugins.lock().map_err(|_| "The plugin host is unavailable.")?.panels().into_iter().map(|p|p.0).collect();
            crate::plugin_sidebar::place(root,id,input["position"].as_u64().ok_or("Invalid sidebar position.")? as usize,active)?;
        },
        "download_access" => {
            let plugins = plugins.lock().map_err(|_| "The plugin host is unavailable.")?;
            if !plugins.natives.has_downloads(id) || !plugins.list().iter().any(|plugin| plugin.id == id) { return Err("This plugin has no download view.".into()); }
            crate::plugin_access::set(root, id, input["value"].as_str().ok_or("Choose a download location.")?)?;
        },
        "integration" => crate::source_integration::save(root, id, input["field"].as_str().unwrap_or_default(), input["enabled"].as_bool().ok_or("Invalid integration preference.")?)?,
        "library_visibility" => {
            let library = chilpa_core::open_library().map_err(|_| "The library could not be opened.")?;
            let source = library.find_source(crate::source_opener::PROVIDER, id).map_err(|_| "The source could not be read.")?.ok_or("Import a collection first.")?;
            library.show_imported_source(source, input["enabled"].as_bool().ok_or("Invalid library preference.")?).map_err(|_| "The library preference could not be saved.")?;
        }
        "tint" => crate::plugin_icons::set_tint(root, id, input["enabled"].as_bool().ok_or("Invalid icon preference.")?)?,
        "filter" => {
            state["query"] = input["text"].as_str().unwrap_or("").to_lowercase().into();
            state["category"] = input["category"].as_str().unwrap_or("").into();
            state["tab"] = input["tab"].as_str().unwrap_or("uninspected").into();
        }
        "refresh" => {
            // A fresh installation has no cache yet. Keep the manager usable
            // offline if the public catalog is temporarily unavailable.
            if input["initial"] != true {
                repositories.refresh("native", "EN")?;
            } else if repositories.cached("native")?.is_empty() {
                let _ = repositories.refresh("native", "EN");
            }
        },
        _ => return Err("Unknown plugin operation.".into()),
    }
    state["repositories"] = json!(repositories.compatible());
    state["native_repository"] = repositories.native_catalog().into();
    let mut catalog = Vec::new();
    let mut categories = std::collections::BTreeSet::new();
    let installed_ids = plugins.lock().map_err(|_| "The plugin host is unavailable.")?.list().into_iter()
        .filter_map(|plugin| std::fs::read(root.join(plugin.id).join("origin.json")).ok())
        .filter_map(|bytes| serde_json::from_slice::<Value>(&bytes).ok()).collect::<Vec<_>>();
    let mut counts = json!({"installed":0,"valid":0,"uninspected":0,"incompatible":0});
    let mut native_catalog = Vec::new();
    for entry in repositories.cached("native")? {
        if entry.adapter != "native" { continue; }
        let installed = crate::native_install::installed_version(root, &entry.id);
        if installed.as_ref().is_some_and(|version| !crate::plugin_updates::newer(&entry.version, version)) { continue; }
        let mut row = json!(entry);
        row["repository_id"] = "native".into();
        row["update"] = installed.is_some().into();
        native_catalog.push(row);
    }
    let compatibility_available = plugins.lock().map_err(|_| "The plugin host is unavailable.")?.compatibility_launcher().is_some();
    for repository in repositories.compatible() {
        for entry in repositories.cached(&repository.id)? {
            // A native plugin is offered in its own section, to install or
            // to update when the repository has a newer version.
            if entry.adapter == "native" {
                let installed = crate::native_install::installed_version(root, &entry.id);
                if installed.as_ref().is_some_and(|version| !crate::plugin_updates::newer(&entry.version, version)) { continue; }
                let mut row = json!(entry);
                row["repository_id"] = repository.id.clone().into();
                row["update"] = installed.is_some().into();
                native_catalog.push(row);
                continue;
            }
            if !compatibility_available { continue; }
            let category = if entry.category.is_empty() { "uncategorized" } else { &entry.category };
            categories.insert(category.to_owned());
            if installed_ids.iter().any(|origin| origin["repository"] == repository.id && origin["catalog_entry"]["id"] == entry.id
                && !crate::plugin_updates::newer(&entry.version, origin["catalog_entry"]["version"].as_str().unwrap_or(""))) { continue; }
            let inspection = evidence.get(&repository.id, &entry);
            let tab = if !entry.problem.is_empty() || inspection.is_some_and(|record| record["valid"] == false) { "incompatible" }
                else if inspection.is_some_and(|record| record["valid"] == true) { "valid" } else { "uninspected" };
            counts[tab] = (counts[tab].as_u64().unwrap_or(0) + 1).into();
            if tab != state["tab"].as_str().unwrap_or("uninspected") || !matches_filter(&entry.name, &entry.description, category, state) { continue; }
            let mut row = json!(entry);
            row["repository_id"] = repository.id.clone().into();
            row["light"] = match tab { "incompatible" => "red", "valid" => "green", _ => "unknown" }.into();
            if let Some(record) = inspection { if let Some(problem) = record["problem"].as_str() { row["problem"] = problem.into(); } }
            catalog.push(row);
        }
    }
    state["categories"] = json!(categories);
    state["native_catalog"] = json!(native_catalog);
    state["catalog"] = json!(catalog);
    let plugins = plugins.lock().map_err(|_| "The plugin host is unavailable.")?;
    state["network_requests"] = json!(*plugins.network_requests.lock().map_err(|_| "Network permissions are unavailable.")?);
    let sidebar_order = crate::plugin_sidebar::ordered(root,plugins.panels().into_iter().map(|p|p.0).collect());
    let mut installed = plugins.list().into_iter().map(|plugin| { let enabled = plugins.enabled(&plugin.id); let limited_network = plugins.worker_domains(&plugin.id).is_some_and(|domains| !domains.iter().any(|domain| domain == "*")); (plugins.natives.is_native(&plugin.id), enabled && plugins.natives.has_settings(&plugin.id), plugins.natives.has_downloads(&plugin.id), plugins.natives.library_kinds(&plugin.id), plugin, enabled, limited_network) }).collect::<Vec<_>>();
    installed.sort_by_key(|(_, _, _, _, plugin, _, _)| sidebar_order.iter().position(|id| id == &plugin.id).unwrap_or(usize::MAX));
    drop(plugins);
    // A catalog snapshot never migrates or maintains the library, nor holds the guest lock during database queries.
    let library = chilpa_library::Library::open_read_only(&chilpa_library::Library::default_path()).ok();
    state["installed"] = json!(installed.into_iter().map(|(native, native_settings, downloads, library_kinds, plugin, enabled, limited_network)| {
        let origin: Value = std::fs::read(root.join(&plugin.id).join("origin.json")).ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or(Value::Null);
        let integration = library.as_ref().and_then(|lib| lib.find_source(crate::source_opener::PROVIDER, &plugin.id).ok().flatten()
            .and_then(|source| lib.imported_source_state(source).ok())).unwrap_or_default();
        json!({"sidebar_position":sidebar_order.iter().position(|id|id==&plugin.id),"sidebar_count":sidebar_order.len(),"library_kinds":library_kinds,"downloads":downloads,"download_access":crate::plugin_access::read(root,&plugin.id),"integration_available":sidebar_order.contains(&plugin.id) || !library_kinds.is_empty(),"integration":crate::source_integration::read(root, &plugin.id),"native":native,"limited_network":limited_network,"enabled":enabled,"folder_write":export::enabled(root,&plugin.id),"export_folder":export::read(root,&plugin.id).map(|p|p.display().to_string()),"library_count":integration.0,"library_visible":integration.1,"id":plugin.id,"name":plugin.name,"version":plugin.version,"author":plugin.author,"description":plugin.description,"state":plugin.state,
            "problem":plugin.problem,"permissions":plugin.asks,"origin":origin,
            "icon":crate::plugin_icons::image_url(root, &plugin.id),"tint":crate::plugin_icons::tint(root, &plugin.id),
            "settings":native_settings || origin["inspection"]["settings_pages"].as_array().is_some_and(|pages| !pages.is_empty()),
            "light":if !plugin.problem.is_empty() {"red"} else if origin.is_null() {"native"} else if plugin.state == "fine" {"green"} else {"yellow"}})
    }).collect::<Vec<_>>());
    state["installed_catalog"] = json!(state["installed"].as_array().into_iter().flatten().filter(|row| {
        let entry = &row["origin"]["catalog_entry"];
        !row["origin"].is_null() && matches_filter(row["name"].as_str().unwrap_or(""),
            row["description"].as_str().unwrap_or(""), entry["category"].as_str().filter(|v| !v.is_empty()).unwrap_or("uncategorized"), state)
    }).cloned().collect::<Vec<_>>());
    counts["installed"] = json!(state["installed"].as_array().into_iter().flatten().filter(|row| !row["origin"].is_null()).count());
    state["counts"] = counts;
    failure.map_or(Ok(()), Err)
}

fn matches_filter(name: &str, description: &str, category: &str, state: &Value) -> bool {
    let query = state["query"].as_str().unwrap_or("");
    let selected = state["category"].as_str().unwrap_or("");
    (selected.is_empty() || category == selected)
        && (name.to_lowercase().contains(query) || description.to_lowercase().contains(query))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chilpa_plugin::bridge::{BridgeError, WorkerLauncher, WorkerTransport};
    use std::{time::{Duration, Instant}, io::{Cursor, Read, Write}, net::TcpListener};
    use sha2::{Digest, Sha256};
    struct Launcher;
    struct Transport { id: u64, operation: String }
    impl WorkerLauncher for Launcher {
        fn guest_root(&self) -> &str { "/guest" }
        fn inspection_revision(&self) -> Option<String> { Some("fixture".into()) }
        fn launch(&self, _: &std::path::Path) -> Result<Box<dyn WorkerTransport>, BridgeError> {
            Ok(Box::new(Transport {id:0, operation:String::new()}))
        }
    }
    impl WorkerTransport for Transport {
        fn send(&mut self, bytes: &[u8], _: Instant) -> Result<(), BridgeError> {
            let request: Value = serde_json::from_slice(bytes).unwrap();
            self.id = request["id"].as_u64().unwrap(); self.operation = request["operation"].as_str().unwrap().into(); Ok(())
        }
        fn receive(&mut self, _: Instant, _: usize) -> Result<Vec<u8>, BridgeError> {
            let result = if self.operation == "settings.open" { json!({"revision":1,"form":{"fields":[],"actions":[],"unsupported":[]},"values":{}}) }
                else { json!({"features":{"browse":true,"resolve":true},"settings_pages":["plugins/Guest/settings.html"],"limitations":[],"diagnostics":{}}) };
            Ok(serde_json::to_vec(&json!({"type":"reply","id":self.id,"result":result})).unwrap())
        }
        fn stop(&mut self) {}
    }
    fn wait(center: &PluginCenter) -> Value {
        let view = wait_finished(center);
        assert_eq!(view["error"], "", "{view}");
        view
    }
    fn wait_finished(center: &PluginCenter) -> Value {
        let deadline = Instant::now() + Duration::from_secs(10);
        while center.busy.load(Ordering::Acquire) != 0 {
            assert!(Instant::now() < deadline, "manager operation timed out");
            std::thread::sleep(Duration::from_millis(5));
        }
        center.snapshot()
    }
    #[test]
    fn maintenance_is_invisible_and_does_not_drop_a_foreground_action() {
        let (commands, received) = mpsc::sync_channel(2);
        let center = PluginCenter { commands: Arc::new(commands), view: Arc::new(Mutex::new(json!({}))),
            busy: Arc::new(AtomicUsize::new(0)), foreground: Arc::new(AtomicBool::new(false)), library_dirty: Arc::new(AtomicBool::new(false)), removed: Arc::new(Mutex::new(Vec::new())), refreshed: Arc::new(Mutex::new(Vec::new())),
            operation: Arc::new(Mutex::new(String::new())), inbox: Arc::new(Mutex::new(Default::default())), playback: Arc::new(AtomicBool::new(false)) };
        center.command("tick", json!({"id":"guest"}));
        assert_eq!(center.snapshot()["busy"], false);
        assert_eq!(center.snapshot()["working"], true);
        center.command("settings", json!({"id":"guest"}));
        assert_eq!(center.snapshot()["busy"], true);
        assert_eq!(received.try_recv().unwrap().0, "tick");
        assert_eq!(received.try_recv().unwrap().0, "settings");
        assert!(received.try_recv().is_err());
    }
    #[test]
    fn live_filters_combine_category_with_name_or_description() {
        let query = json!({"query":"radio", "category":"music"});
        assert!(matches_filter("Radio browser", "", "music", &query));
        assert!(matches_filter("Guest", "Live RADIO", "music", &query));
        assert!(!matches_filter("Radio browser", "", "tools", &query));
        assert!(!matches_filter("Guest", "Unrelated", "music", &query));
    }
    #[test]
    fn maintenance_ignores_wasm_plugins_and_requests_left_after_uninstall() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("plugins");
        let id = "app.test.wasm";
        let folder = root.join(id);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("manifest.json"), format!(r#"{{"id":"{id}","name":"WASM","version":"1","api_version":1,"kind":"source","capabilities":[]}}"#)).unwrap();
        let wasm = wat::parse_str(r#"(module (memory (export "memory") 1)
            (func (export "chilpa_alloc") (param i32) (result i32) (i32.const 4096))
            (func (export "chilpa_api_version") (result i32) (i32.const 1)))"#).unwrap();
        std::fs::write(folder.join("plugin.wasm"), wasm).unwrap();
        let mut host = Plugins::new(root.clone()).unwrap();
        host.load_all(); host.approve(id).unwrap(); host.reload_one(id);
        let host = Arc::new(Mutex::new(host));
        let center = PluginCenter::start(host.clone(), root, crate::source_catalog::Catalog::new());
        center.command("tick", json!({"id":id}));
        let view = wait_finished(&center);
        assert_ne!(view["maintenance_failed"], true, "{view}");
        assert!(view["error"].as_str().unwrap_or_default().is_empty(), "{view}");
        assert!(host.lock().unwrap().enabled(id));
        assert_eq!(host.lock().unwrap().list()[0].state, "fine");
        center.command("uninstall", json!({"id":id}));
        assert!(wait(&center)["installed"].as_array().unwrap().is_empty());
        center.command("tick", json!({"id":id}));
        let view = wait(&center);
        assert_ne!(view["maintenance_failed"], true, "{view}");
        assert!(view["installed"].as_array().unwrap().is_empty());
    }
    #[test]
    fn a_catalog_selection_is_inspected_approved_installed_and_opens_settings() {
        let temporary = tempfile::tempdir().unwrap();
        let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, text) in [("install.xml", "<extension><module>Plugins::Guest::Plugin</module><version>1</version></extension>"), ("Plugin.pm", "original") ] {
            archive.start_file(name, zip::write::SimpleFileOptions::default()).unwrap(); archive.write_all(text.as_bytes()).unwrap();
        }
        let bytes = archive.finish().unwrap().into_inner();
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = server.local_addr().unwrap();
        let digest = format!("{:x}", Sha256::digest(&bytes));
        let catalog = format!("<extensions><plugins><plugin name=\"Guest\" version=\"1\" sha=\"{}\" url=\"http://{address}/package.zip\"><title lang=\"EN\">Guest</title></plugin></plugins></extensions>", "0".repeat(40));
        // The legacy catalog uses SHA-1 for archive verification.
        use sha1::Sha1;
        let catalog = catalog.replace(&"0".repeat(40), &format!("{:x}", Sha1::digest(&bytes)));
        let serving = std::thread::spawn(move || {
            for _ in 0..2 {
                let (mut socket, _) = server.accept().unwrap(); let mut request = [0; 4096]; let size = socket.read(&mut request).unwrap();
                let body = if String::from_utf8_lossy(&request[..size]).contains("/package.zip") { bytes.as_slice() } else { catalog.as_bytes() };
                write!(socket, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap(); socket.write_all(body).unwrap();
            }
        });
        let mut plugins = Plugins::new(temporary.path().join("plugins")).unwrap();
        plugins.set_worker_launcher(Arc::new(Launcher));
        let root = temporary.path().join("plugins");
        let center = PluginCenter::start(Arc::new(Mutex::new(plugins)), root.clone(), crate::source_catalog::Catalog::new());
        center.command("add", json!({"url":format!("http://{address}/catalog.xml")}));
        let view = wait(&center); let repository = view["repositories"][0]["id"].clone();
        assert_eq!(view["catalog"].as_array().unwrap().len(), 1);
        center.command("prepare", json!({"repository":repository,"id":"Guest"}));
        let view = wait(&center); assert_eq!(view["proposal"]["light"], "green");
        assert_eq!(view["validation"]["state"], "passed");
        assert!(view["installed"].as_array().unwrap().is_empty());
        assert_eq!(view["counts"]["valid"], 1);
        assert_eq!(view["counts"]["uninspected"], 0);
        center.command("install", json!({"domains":["example.org"]}));
        let view = wait(&center); let id = view["installed"][0]["id"].as_str().unwrap();
        assert_eq!(view["installed"][0]["state"], "fine");
        assert_eq!(view["installed"][0]["light"], "green");
        assert_eq!(view["installed"][0]["origin"]["package_sha256"], digest);
        assert_eq!(std::fs::read(root.join(id).join("Plugin.pm")).unwrap(), b"original");
        center.command("settings", json!({"id":id})); assert_eq!(wait(&center)["form"]["revision"], 1);
        assert_eq!(view["installed"][0]["enabled"], true);
        center.command("tick", json!({"id":id})); wait(&center);
        center.command("disable", json!({"id":id}));
        assert_eq!(wait(&center)["installed"][0]["enabled"], false);
        center.command("tick", json!({"id":id})); wait(&center);
        center.command("review_permissions", json!({"id":id}));
        let revision = wait(&center)["approval"]["revision"].clone();
        center.command("enable", json!({"id":id,"permission_revision":revision}));
        assert_eq!(wait(&center)["installed"][0]["enabled"], true);
        center.command("uninstall", json!({"id":id}));
        assert!(wait(&center)["installed"].as_array().unwrap().is_empty());
        assert!(!root.join(id).exists());
        center.command("tick", json!({"id":id})); wait(&center);
        serving.join().unwrap();
    }
    #[test]
    fn a_local_zip_installs_without_a_repository_or_execution_and_rejects_bad_replacements() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("plugins");
        let host = Arc::new(Mutex::new(Plugins::new(root.clone()).unwrap()));
        let center = PluginCenter::start(host.clone(), root.clone(), crate::source_catalog::Catalog::new());
        let manifest = r#"{"id":"app.test.local","name":"Local","version":"1.0","api_version":1,"kind":"source","runtime":"process","capabilities":[{"type":"register_source"}]}"#;
        let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in [("wrapper/manifest.json", manifest.as_bytes()), ("wrapper/plugin", b"must not run".as_slice())] {
            archive.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
            archive.write_all(bytes).unwrap();
        }
        archive.start_file("wrapper/runtime/engine", zip::write::SimpleFileOptions::default().unix_permissions(0o755)).unwrap();
        archive.write_all(b"bundled program").unwrap();
        let path = temporary.path().join("local package.zip");
        std::fs::write(&path, archive.finish().unwrap().into_inner()).unwrap();
        let address = url::Url::from_file_path(&path).unwrap().to_string();
        center.command("install_local", json!({"path":address}));
        let view = wait(&center);
        assert_eq!(view["error"], "", "{view}");
        assert_eq!(view["installed"][0]["id"], "app.test.local");
        assert_eq!(view["installed"][0]["enabled"], false);
        assert!(view["repositories"].as_array().unwrap().is_empty());
        assert_eq!(std::fs::read(root.join("app.test.local/plugin")).unwrap(), b"must not run");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(root.join("app.test.local/runtime/engine")).unwrap().permissions().mode() & 0o111, 0o111);
        }
        let source: Value = serde_json::from_slice(&std::fs::read(root.join("app.test.local/source.json")).unwrap()).unwrap();
        assert_eq!(source["repository"], "local");
        host.lock().unwrap().approve("app.test.local").unwrap();
        std::fs::write(&path, b"not a zip").unwrap();
        center.command("install_local", json!({"path":address}));
        let view = wait_finished(&center);
        assert_ne!(view["error"], "");
        assert!(host.lock().unwrap().enabled("app.test.local"));
        assert_eq!(std::fs::read(root.join("app.test.local/plugin")).unwrap(), b"must not run");
        center.command("install_local", json!({"path":"https://example.org/package.zip"}));
        assert_ne!(wait_finished(&center)["error"], "");
    }

}

pub(crate) fn permission_review(root: &std::path::Path, id: &str) -> Result<Value, String> {
    if id.is_empty() || id.contains("..") || !id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'-')) { return Err("Invalid plugin id.".into()); }
    let bytes = std::fs::read(root.join(id).join("manifest.json")).map_err(|e| e.to_string())?;
    let value: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let runtime = match value["runtime"].as_str() { Some("process") => chilpa_plugin::Runtime::Process, Some("bridge") => chilpa_plugin::Runtime::Bridge, _ => chilpa_plugin::Runtime::Wasm };
    let manifest = chilpa_plugin::Manifest::parse_for_runtime(std::str::from_utf8(&bytes).map_err(|e| e.to_string())?, runtime).map_err(|e| e.to_string())?;
    use sha2::Digest;
    Ok(json!({"id":id,"name":manifest.name,"description":value["description"],"version":manifest.version,
        "permissions":manifest.capabilities.iter().map(|cap| cap.describe()).collect::<Vec<_>>(),
        "revision":format!("{:x}", sha2::Sha256::digest(&bytes))}))
}
