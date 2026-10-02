//! Background catalog, installation and compatibility settings workflow.
use crate::{plugins::Plugins, Chilpa, PluginRepositories, PreparedPlugin};
use serde_json::{json, Value};
use std::{path::PathBuf, sync::{Arc, Mutex, mpsc, atomic::{AtomicBool, AtomicUsize, Ordering}}};

pub(crate) struct PluginCenter {
    commands: mpsc::SyncSender<(String, Value)>,
    view: Arc<Mutex<Value>>,
    busy: Arc<AtomicUsize>,
    foreground: Arc<AtomicBool>,
    library_dirty: Arc<AtomicBool>,
    operation: Arc<Mutex<String>>,
}

impl PluginCenter {
    fn start(plugins: Arc<Mutex<Plugins>>, root: PathBuf) -> Self {
        let (commands, receiver) = mpsc::sync_channel::<(String, Value)>(2);
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
        std::thread::spawn(move || {
            let launcher = plugins.lock().ok().and_then(|host| host.compatibility_launcher());
            let mut evidence = crate::plugin_evidence::Evidence::load(&root.join(".repositories"),
                launcher.and_then(|launcher| launcher.inspection_revision()));
            let mut pending: Option<PreparedPlugin> = None;
            let mut state = json!({"repositories":[],"catalog":[],"installed":[],"form":null,"proposal":null});
            for (action, input) in receiver {
                let started = std::time::Instant::now();
                let result = run(&action, &input, &root, &plugins, &mut pending, &mut state, &mut evidence);
                if result.is_ok() && action == "library_visibility" { changed.store(true, Ordering::Release); }
                if action == "prepare" {
                    state["validation"] = json!({"id":input["id"],"repository_id":input["repository"],
                        "state":if result.is_ok() {"passed"} else {"failed"},
                        "light":if result.is_ok() {"green"} else {"red"},
                        "problem":result.as_ref().err()});
                }
                if action != "tick" || result.is_err() {
                    crate::plugin_evidence::log_event(&root.join(".repositories"), &action, result.is_ok(), started.elapsed());
                }
                if action == "tick" && result.is_err() { state["maintenance_failed"] = true.into(); }
                if matches!(action.as_str(), "settings" | "restart") && result.is_ok() { state["maintenance_failed"] = false.into(); }
                if action != "tick" || result.is_err() { state["error"] = result.err().unwrap_or_default().into(); }
                if let Ok(host) = plugins.lock() {
                    if let Ok(requests) = host.network_requests.lock() { state["network_requests"] = json!(*requests); }
                }
                if let Ok(mut output) = shared.lock() { *output = state.clone(); }
                if action != "tick" {
                    if let Ok(mut operation) = current.lock() { operation.clear(); }
                    visible.store(false, Ordering::Release);
                }
                active.fetch_sub(1, Ordering::AcqRel);
            }
        });
        Self { commands, view, busy, foreground, operation, library_dirty }
    }
    fn command(&self, action: &str, input: Value) {
        let background = action == "tick";
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
    pub fn plugin_center_json(&mut self) -> String {
        self.advance_plugins();
        if self.plugin_center.is_none() {
            if self.plugins.is_none() { self.start_plugins(); }
            if let Some(plugins) = &self.plugins {
                let center = PluginCenter::start(plugins.clone(), Self::plugins_dir());
                center.command("refresh", json!({}));
                self.plugin_center = Some(center);
            }
        }
        if self.plugin_center.as_ref().is_some_and(|center| center.library_dirty.swap(false, Ordering::AcqRel)) { self.mark_library_changed(); }
        self.plugin_center.as_ref().map(|center| center.snapshot().to_string()).unwrap_or_else(|| "{}".into())
    }
    pub fn plugin_center_command(&mut self, action: &str, input: &str) {
        let Ok(input) = serde_json::from_str(input) else { return };
        if matches!(action, "install" | "enable" | "disable" | "uninstall" | "grant_network" | "restart") { self.invalidate_source_menus(); }
        if let Some(center) = &self.plugin_center { center.command(action, input); }
    }
}

fn run(action: &str, input: &Value, root: &std::path::Path, plugins: &Arc<Mutex<Plugins>>,
    pending: &mut Option<PreparedPlugin>, state: &mut Value, evidence: &mut crate::plugin_evidence::Evidence) -> Result<(), String> {
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
            let id = prepared.install_new(root)?;
            let mut plugins = plugins.lock().map_err(|_| "The plugin host is unavailable.")?;
            plugins.reload_one(&id);
            plugins.approve(&id)?;
            plugins.reload_one(&id);
            state["proposal"] = Value::Null;
        }
        "restart" => {
            plugins.lock().map_err(|_| "The plugin host is unavailable.")?.reactivate(id);
            state["form"] = Value::Null;
        }
        "enable" | "disable" => {
            let mut plugins = plugins.lock().map_err(|_| "The plugin host is unavailable.")?;
            if action == "enable" { plugins.approve(id)?; } else { plugins.revoke(id)?; }
            plugins.reload_one(id);
            state["form"] = Value::Null;
        }
        "uninstall" => {
            let mut plugins = plugins.lock().map_err(|_| "The plugin host is unavailable.")?;
            plugins.revoke(id)?;
            plugins.reload_one(id);
            plugins.forget_credential(id);
            if !plugins.uninstall(id) { return Err("The plugin could not be removed.".into()); }
            state["form"] = Value::Null;
            state["settings_id"] = Value::Null;
        }
        "grant_network" => {
            plugins.lock().map_err(|_| "The plugin host is unavailable.")?.grant_network(id, input["domain"].as_str().unwrap_or(""))?;
            state["form"] = Value::Null;
        }
        "settings" | "submit" | "tick" => {
            let mut plugins = plugins.lock().map_err(|_| "The plugin host is unavailable.")?;
            if action == "tick" {
                plugins.call_waiting(id, "tick", b"{}")?;
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
        "refresh" => {},
        _ => return Err("Unknown plugin operation.".into()),
    }
    state["repositories"] = json!(repositories.compatible());
    let mut catalog = Vec::new();
    let mut categories = std::collections::BTreeSet::new();
    let installed_ids = plugins.lock().map_err(|_| "The plugin host is unavailable.")?.list().into_iter()
        .filter_map(|plugin| std::fs::read(root.join(plugin.id).join("origin.json")).ok())
        .filter_map(|bytes| serde_json::from_slice::<Value>(&bytes).ok()).collect::<Vec<_>>();
    let mut counts = json!({"installed":0,"valid":0,"uninspected":0,"incompatible":0});
    for repository in repositories.compatible() {
        for entry in repositories.cached(&repository.id)? {
            let category = if entry.category.is_empty() { "uncategorized" } else { &entry.category };
            categories.insert(category.to_owned());
            if installed_ids.iter().any(|origin| origin["repository"] == repository.id && origin["catalog_entry"]["id"] == entry.id) { continue; }
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
    state["catalog"] = json!(catalog);
    let plugins = plugins.lock().map_err(|_| "The plugin host is unavailable.")?;
    state["network_requests"] = json!(*plugins.network_requests.lock().map_err(|_| "Network permissions are unavailable.")?);
    let installed = plugins.list().into_iter().map(|plugin| { let enabled = plugins.enabled(&plugin.id); let limited_network = plugins.worker_domains(&plugin.id).is_some_and(|domains| !domains.iter().any(|domain| domain == "*")); (plugin, enabled, limited_network) }).collect::<Vec<_>>();
    drop(plugins);
    // A catalog snapshot never migrates or maintains the library, nor holds the guest lock during database queries.
    let library = chilpa_library::Library::open_read_only(&chilpa_library::Library::default_path()).ok();
    state["installed"] = json!(installed.into_iter().map(|(plugin, enabled, limited_network)| {
        let origin: Value = std::fs::read(root.join(&plugin.id).join("origin.json")).ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or(Value::Null);
        let integration = library.as_ref().and_then(|lib| lib.find_source(crate::source_opener::PROVIDER, &plugin.id).ok().flatten()
            .and_then(|source| lib.imported_source_state(source).ok())).unwrap_or_default();
        json!({"limited_network":limited_network,"enabled":enabled,"library_count":integration.0,"library_visible":integration.1,"id":plugin.id,"name":plugin.name,"description":plugin.description,"state":plugin.state,
            "problem":plugin.problem,"permissions":plugin.asks,"origin":origin,
            "icon":crate::plugin_icons::image_url(root, &plugin.id),"tint":crate::plugin_icons::tint(root, &plugin.id),
            "settings":origin["inspection"]["settings_pages"].as_array().is_some_and(|pages| !pages.is_empty()),
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
        let deadline = Instant::now() + Duration::from_secs(10);
        while center.busy.load(Ordering::Acquire) != 0 {
            assert!(Instant::now() < deadline, "manager operation timed out");
            std::thread::sleep(Duration::from_millis(5));
        }
        let view = center.snapshot(); assert_eq!(view["error"], "", "{view}"); view
    }
    #[test]
    fn maintenance_is_invisible_and_does_not_drop_a_foreground_action() {
        let (commands, received) = mpsc::sync_channel(2);
        let center = PluginCenter { commands, view: Arc::new(Mutex::new(json!({}))),
            busy: Arc::new(AtomicUsize::new(0)), foreground: Arc::new(AtomicBool::new(false)), library_dirty: Arc::new(AtomicBool::new(false)),
            operation: Arc::new(Mutex::new(String::new())) };
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
        let center = PluginCenter::start(Arc::new(Mutex::new(plugins)), root.clone());
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
        center.command("disable", json!({"id":id}));
        assert_eq!(wait(&center)["installed"][0]["enabled"], false);
        center.command("enable", json!({"id":id}));
        assert_eq!(wait(&center)["installed"][0]["enabled"], true);
        center.command("uninstall", json!({"id":id}));
        assert!(wait(&center)["installed"].as_array().unwrap().is_empty());
        assert!(!root.join(id).exists());
        serving.join().unwrap();
    }
}
