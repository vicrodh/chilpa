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
    live: RwLock<HashMap<String, Arc<NativeClient>>>,
    /// What each was started from, to start it again.
    known: Mutex<HashMap<String, Known>>,
}

struct Known {
    manifest: Manifest,
    folder: PathBuf,
    restarts: usize,
    restarting: bool,
    last_start: Instant,
}

impl Natives {
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

    /// Starts a plugin and greets it. It runs on the caller's thread, which
    /// is never the window's: plugins load on their own thread.
    fn start(&self, manifest: &Manifest, folder: &Path) -> Result<Arc<NativeClient>, PluginError> {
        let client = launch(manifest, folder)?;
        let greeting = client.call("initialize", json!({
            "protocol": PROTOCOL,
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
        known.insert(id.clone(), Known { manifest: manifest.clone(), folder: folder.to_path_buf(),
            restarts, restarting: false, last_start: Instant::now() });
        drop(known);
        self.live.write().unwrap_or_else(|e| e.into_inner()).insert(id, client.clone());
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
            let started = me.start(&manifest, &folder);
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
fn launch(manifest: &Manifest, folder: &Path) -> Result<Arc<NativeClient>, PluginError> {
    let id = manifest.id.clone();
    let paths = chilpa_bwrap::NativePaths {
        folder: folder.to_path_buf(),
        data: chilpa_dirs::data().join("plugin-data").join(&id),
        cache: chilpa_dirs::cache().join("plugins").join(&id),
    };
    let name = id.clone();
    chilpa_bwrap::launch_native(&paths, &id, MEMORY, move |method, params| {
        if method == "log" {
            eprintln!("plugin[{name}]: {}", params.get("message").and_then(Value::as_str).unwrap_or(""));
        }
    })
    .map_err(|_| PluginError::Crashed("the plugin's sandbox could not start".into()))
}

/// Native plugins run only inside the Linux sandbox for now; elsewhere they
/// are listed with the reason and never started (ADR-010 debt).
#[cfg(not(target_os = "linux"))]
fn launch(_: &Manifest, _: &Path) -> Result<Arc<NativeClient>, PluginError> {
    Err(PluginError::NotAllowed("native plugins need a sandbox this system does not have yet".into()))
}
