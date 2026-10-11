//! Process plugins, each reached on its own channel.
//!
//! The sandbox lock (`Mutex<Plugins>`) is held for the whole length of a
//! call into a wasm plugin or a compatibility worker, and everything else
//! waits behind it. A native plugin is not reached through it: its client
//! lives here, behind a lock held only to look it up, and any thread calls
//! it at the same time as any other.

use chilpa_plugin::native::{NativeClient, NativePlugin};
use chilpa_plugin::{Manifest, PluginError, Registry, Runtime, RuntimePlugin};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};
use crate::source_layout::{Layout, Route, Section};

/// Protocol version the host speaks with native plugins.
pub(crate) const PROTOCOL: u32 = 1;
/// Address space a native plugin may reserve. Its runtime threads reserve
/// stacks up front, so this is well above what it uses.
const MEMORY: usize = 2 << 30;
/// How long a plugin has to answer `initialize`.
const GREETING: Duration = Duration::from_secs(15);
/// Waits between restarts of a plugin that keeps ending: 1, 2, 5, then 30 s.
const RESTART_WAITS: [u64; 4] = [1, 2, 5, 30];

#[derive(Default)]
pub(crate) struct Natives {
    lifecycle: Mutex<()>,
    pub(crate) broker: crate::plugin_broker::Broker,
    personal_changed: Mutex<std::collections::HashSet<String>>,
    reports: std::sync::OnceLock<Arc<crate::native_reporting::Reports>>,
    live: RwLock<HashMap<String, Arc<NativeClient>>>,
    /// What each was started from, to start it again.
    known: Mutex<HashMap<String, Known>>,
}

struct Known {
    manifest: Manifest,
    folder: PathBuf,
    /// What the plugin said it can do, in its answer to `initialize`.
    capabilities: Value,
    /// The titles of its home's shelves, as last read.
    discover: Vec<String>,
    restarts: usize,
    restarting: bool,
    last_start: Instant,
}

impl Natives {
    pub(crate) fn history_changed(&self, plugin: &str) { self.personal_changed.lock().unwrap_or_else(|e| e.into_inner()).insert(plugin.into()); }
    pub(crate) fn take_history_changes(&self) -> Vec<String> { self.personal_changed.lock().unwrap_or_else(|e| e.into_inner()).drain().collect() }

    pub(crate) fn playback(self: &Arc<Self>, plugin: &str, resolution: &str) -> Option<Arc<dyn chilpa_core::playback_events::Observer>> {
        if resolution.is_empty() || resolution.len() > 128 { return None; }
        let supported = self.known.lock().unwrap_or_else(|e| e.into_inner()).get(plugin)
            .is_some_and(|known| known.capabilities["playback_events"] == 1);
        if !supported { return None; }
        Some(self.reports.get_or_init(|| crate::native_reporting::Reports::start(Arc::downgrade(self))).observer(plugin, resolution))
    }

    pub(crate) fn flush_playback(&self) {
        if let Some(reports) = self.reports.get() { reports.flush(); }
    }

    /// The plugin's channel, if it is running. A plugin found ended is
    /// started again on another thread; meanwhile it is not there.
    pub(crate) fn client(self: &Arc<Self>, id: &str) -> Option<Arc<NativeClient>> {
        let client = self.live.read().unwrap_or_else(|e| e.into_inner()).get(id).cloned()?;
        if client.alive() {
            return Some(client);
        }
        self.restart(id);
        None
    }

    /// The domains a plugin's manifest names, which its covers are fetched within.
    pub(crate) fn domains(&self, id: &str) -> Vec<String> {
        self.known.lock().unwrap_or_else(|e| e.into_inner()).get(id)
            .map(|k| k.manifest.allowed_domains().into_iter().map(str::to_string).collect()).unwrap_or_default()
    }

    pub(crate) fn is_native(&self, id: &str) -> bool {
        self.live.read().unwrap_or_else(|e| e.into_inner()).contains_key(id)
    }

    pub(crate) fn has_settings(&self, id: &str) -> bool {
        self.known.lock().unwrap_or_else(|e| e.into_inner()).get(id).is_some_and(|known| {
            known.capabilities["settings"] == true || known.capabilities["auth"] == "browser"
        })
    }

    pub(crate) fn has_downloads(&self, id: &str) -> bool {
        self.known.lock().unwrap_or_else(|e| e.into_inner()).get(id)
            .is_some_and(|known| known.capabilities["downloads"] == true)
    }

    pub(crate) fn library_kinds(&self, id: &str) -> Vec<String> {
        self.known.lock().unwrap_or_else(|e| e.into_inner()).get(id).map(|p| p.capabilities["library"].as_array().into_iter().flatten().filter_map(|v| v.as_str().map(str::to_owned)).collect()).unwrap_or_default()
    }

    pub(crate) fn has_track_quality(&self, id: &str) -> bool {
        self.known.lock().unwrap_or_else(|e| e.into_inner()).get(id).is_some_and(|p| p.capabilities["track_quality"] == true)
    }
    pub(crate) fn has_favorites(&self, id: &str) -> bool {
        self.known.lock().unwrap_or_else(|e| e.into_inner()).get(id).is_some_and(|p| p.capabilities["favorites"] == true)
    }
    pub(crate) fn has_pages(&self, id: &str) -> bool {
        self.known.lock().unwrap_or_else(|e| e.into_inner()).get(id).is_some_and(|p| p.capabilities["pages"] == true)
    }
    pub(crate) fn edits_playlists(&self, id: &str) -> bool {
        self.known.lock().unwrap_or_else(|e| e.into_inner()).get(id).is_some_and(|p| p.capabilities["playlist_edit"] == true)
    }
    pub(crate) fn ids(&self) -> Vec<String> {
        self.live.read().unwrap_or_else(|e| e.into_inner()).keys().cloned().collect()
    }
    pub(crate) fn searches(&self, id: &str) -> bool {
        self.known.lock().unwrap_or_else(|e| e.into_inner()).get(id).is_some_and(|k| k.capabilities["search"] == true)
    }

    pub(crate) fn permits(&self, id: &str, capability: &chilpa_plugin::Capability) -> bool {
        self.is_native(id) && self.known.lock().unwrap_or_else(|e| e.into_inner()).get(id).is_some_and(|k| k.manifest.has(capability))
    }
    pub(crate) fn imports_playlists(&self, id: &str) -> bool {
        self.known.lock().unwrap_or_else(|e| e.into_inner()).get(id).is_some_and(|k| k.capabilities["playlist_import"] == 1)
    }
    pub(crate) fn display_name(&self, id: &str) -> String {
        self.known.lock().unwrap_or_else(|e| e.into_inner()).get(id).map(|k| k.manifest.name.clone()).unwrap_or_else(|| id.into())
    }
    pub(crate) fn playlist_limit(&self, id: &str) -> usize {
        self.known.lock().unwrap_or_else(|e| e.into_inner()).get(id).and_then(|k| k.capabilities["playlist_limit"].as_u64()).unwrap_or(2000).clamp(1, 10000) as usize
    }
    /// Calls a plugin, or says it is not running.
    pub(crate) fn call(self: &Arc<Self>, id: &str, method: &str, params: Value, timeout: Duration) -> Result<Value, String> {
        let client = self.client(id).ok_or_else(|| chilpa_i18n::t("the plugin is not running"))?;
        client.call(method, params, timeout).map_err(|e| e.to_string())
    }

    /// Calls a plugin and keeps its error whole, with what it added to it.
    pub(crate) fn call_raw(self: &Arc<Self>, id: &str, method: &str, params: Value, timeout: Duration) -> Result<Value, chilpa_plugin::native::NativeError> {
        let client = self.client(id).ok_or(chilpa_plugin::native::NativeError::Closed)?;
        client.call(method, params, timeout)
    }
    /// A source that has lyrics for its own tracks.
    pub(crate) fn has_track_lyrics(&self, id: &str) -> bool {
        self.known.lock().unwrap_or_else(|e| e.into_inner()).get(id).is_some_and(|k| k.capabilities["track_lyrics"] == 1)
    }

    /// Remembers the titles of the plugin's home shelves, which are the
    /// routes of its Discover page.
    pub(crate) fn set_discover(&self, id: &str, titles: Vec<String>) {
        if let Some(known) = self.known.lock().unwrap_or_else(|e| e.into_inner()).get_mut(id) {
            known.discover = titles;
        }
    }

    /// The pages a plugin's music is drawn in, the same way a layout file
    /// maps a compatibility source onto them: Discover, its home's shelves;
    /// Library, with a tab for each kind of favorite it lists.
    pub(crate) fn layout(&self, id: &str) -> Option<Layout> {
        let known = self.known.lock().unwrap_or_else(|e| e.into_inner());
        let known = known.get(id)?;
        let section = |title: &str, icon: &str, entity: &str, order: i32, under: &str, paths: Vec<Route>| Section {
            title: chilpa_i18n::t(title), icon: icon.into(), entity: entity.into(), paths, order, under: under.into() };
        let navigation = &known.capabilities["navigation"];
        let title = |key: &str, fallback: &str| navigation[key].as_str()
            .filter(|value| !value.trim().is_empty()).unwrap_or(fallback).to_string();
        let mut sections = std::collections::BTreeMap::new();
        if known.capabilities.get("home").and_then(Value::as_bool).unwrap_or(false) {
            let routes = known.discover.iter().map(|title| Route { path: vec![title.clone()], membership: String::new() }).collect();
            sections.insert("discover".to_string(), section(&title("discover", chilpa_i18n::mark("Discover")), "home", "album", 0, "", routes));
        }
        let kinds: Vec<String> = known.capabilities.get("library").and_then(Value::as_array).into_iter().flatten()
            .filter_map(Value::as_str).map(str::to_string).collect();
        if !kinds.is_empty() {
            sections.insert("library".to_string(), section(&title("library", chilpa_i18n::mark("Library")), "columns3", "album", 1, "", Vec::new()));
            let tabs = [("albums", chilpa_i18n::mark("Releases"), "album"), ("tracks", chilpa_i18n::mark("Tracks"), "track"),
                ("artists", chilpa_i18n::mark("Artists"), "artist"), ("labels", chilpa_i18n::mark("Labels"), "label"), ("playlists", chilpa_i18n::mark("Playlists"), "playlist")];
            for (order, (kind, fallback, entity)) in tabs.into_iter().enumerate() {
                if kinds.iter().any(|k| k == kind) {
                    sections.insert(kind.to_string(), section(&title(kind, fallback), "", entity, order as i32 + 2, "library",
                        vec![Route { path: vec![kind.to_string()], membership: String::new() }]));
                }
            }
        }
        Some(Layout { sections })
    }

    pub(crate) fn stop(&self, id: &str) {
        let _guard = self.lifecycle.lock().unwrap_or_else(|e| e.into_inner());
        self.broker.revoke(id);
        self.known.lock().unwrap_or_else(|e| e.into_inner()).remove(id);
        if let Some(client) = self.live.write().unwrap_or_else(|e| e.into_inner()).remove(id) { client.stop(); }
    }
    pub(crate) fn forget_reports(&self, id: &str) {
        if let Some(reports) = self.reports.get() { reports.forget(id); }
    }
    /// Starts a plugin and greets it. It runs on the caller's thread, which
    /// is never the window's: plugins load on their own thread.
    fn start(self: &Arc<Self>, manifest: &Manifest, folder: &Path) -> Result<Arc<NativeClient>, PluginError> {
        let _guard = self.lifecycle.lock().unwrap_or_else(|e| e.into_inner());
        self.start_locked(manifest, folder)
    }
    fn start_locked(self: &Arc<Self>, manifest: &Manifest, folder: &Path) -> Result<Arc<NativeClient>, PluginError> {
        let client = launch(manifest, folder, Arc::downgrade(self))?;
        let greeting = client.call("initialize", json!({
            "protocol": PROTOCOL,
            "playback_events": 1,
            "host": {"name": "chilpa", "version": env!("CARGO_PKG_VERSION")},
            "data_dir": "/data",
            "cache_dir": "/cache",
            "locale": chilpa_i18n::current_language(),
        }), GREETING).map_err(|e| PluginError::Crashed(e.to_string()))?;
        if greeting.get("protocol").and_then(Value::as_u64) != Some(PROTOCOL as u64) {
            client.stop();
            return Err(PluginError::Crashed(format!("it speaks another protocol than {PROTOCOL}")));
        }
        let id = manifest.id.clone();
        let mut known = self.known.lock().unwrap_or_else(|e| e.into_inner());
        let restarts = known.get(&id).map(|k| k.restarts).unwrap_or(0);
        let discover = known.get(&id).map(|k| k.discover.clone()).unwrap_or_default();
        known.insert(id.clone(), Known { manifest: manifest.clone(), folder: folder.to_path_buf(),
            capabilities: greeting.get("capabilities").cloned().unwrap_or(Value::Null),
            discover, restarts, restarting: false, last_start: Instant::now() });
        drop(known);
        self.live.write().unwrap_or_else(|e| e.into_inner()).insert(id, client.clone());
        self.reports.get_or_init(|| crate::native_reporting::Reports::start(Arc::downgrade(self)));
        Ok(client)
    }

    fn restart(self: &Arc<Self>, id: &str) {
        let (manifest, folder, wait) = {
            let mut known = self.known.lock().unwrap_or_else(|e| e.into_inner());
            let Some(k) = known.get_mut(id) else { return };
            if k.restarting {
                return;
            }
            // A plugin that ran a good while before ending starts over.
            if k.last_start.elapsed() > Duration::from_secs(300) {
                k.restarts = 0;
            }
            k.restarting = true;
            let wait = RESTART_WAITS[k.restarts.min(RESTART_WAITS.len() - 1)];
            k.restarts += 1;
            (k.manifest.clone(), k.folder.clone(), wait)
        };
        let me = self.clone();
        let id = id.to_string();
        let _ = std::thread::Builder::new().name("chilpa-native-restart".into()).spawn(move || {
            std::thread::sleep(Duration::from_secs(wait));
            let _guard = me.lifecycle.lock().unwrap_or_else(|e| e.into_inner());
            if !me.known.lock().unwrap_or_else(|e| e.into_inner()).contains_key(&id) { return; }
            let started = me.start_locked(&manifest, &folder);
            eprintln!("native: plugin={id} restarted ok={}", started.is_ok());
            if let Some(k) = me.known.lock().unwrap_or_else(|e| e.into_inner()).get_mut(&id) {
                k.restarting = false;
            }
        });
    }
}

/// Installs a process plugin through the same approval and watchdog as the
/// rest. A plugin nobody approved is never started.
pub(crate) fn install(
    registry: &mut Registry,
    manifest_json: &str,
    folder: &Path,
    natives: &Arc<Natives>,
    approved: impl FnOnce(&Manifest) -> bool,
) -> bool {
    registry.install_adapter(Runtime::Process, manifest_json, approved, |manifest| {
        let client = natives.start(&manifest, folder)?;
        Ok(Box::new(NativePlugin::new(manifest, client)) as Box<dyn RuntimePlugin>)
    })
}

#[cfg(target_os = "linux")]
fn launch(manifest: &Manifest, folder: &Path, natives: std::sync::Weak<Natives>) -> Result<Arc<NativeClient>, PluginError> {
    let id = manifest.id.clone();
    let paths = chilpa_bwrap::NativePaths {
        folder: folder.to_path_buf(),
        data: chilpa_dirs::data().join("plugin-data").join(&id),
        cache: chilpa_dirs::cache().join("plugins").join(&id),
        export: if manifest.has(&chilpa_plugin::Capability::FolderWrite) {
            crate::plugin_center::export::read(folder.parent().unwrap_or(folder), &id)
        } else { None },
    };
    let name = id.clone();
    chilpa_bwrap::launch_native(&paths, &id, MEMORY, move |method, params| {
        if method == "host.request" {
            if let Some(natives) = natives.upgrade() { crate::plugin_broker::dispatch(&natives, &name, params.clone()); }
        } else if method == "log" {
            eprintln!("plugin[{name}]: {}", params.get("message").and_then(Value::as_str).unwrap_or(""));
        }
    })
    .map_err(|_| PluginError::Crashed("the plugin's sandbox could not start".into()))
}

/// Native plugins run only inside the Linux sandbox for now; elsewhere they
/// are listed with the reason and never started (ADR-010 debt).
#[cfg(not(target_os = "linux"))]
fn launch(_: &Manifest, _: &Path, _: std::sync::Weak<Natives>) -> Result<Arc<NativeClient>, PluginError> {
    Err(PluginError::NotAllowed("native plugins need a sandbox this system does not have yet".into()))
}
