//! Installing compatibility workers through the same approval and watchdog.

use crate::WorkerContext;
use chilpa_plugin::{bridge::WorkerLauncher, bridge_plugin::BridgePlugin, PluginError, Registry, Runtime};
use chilpa_secrets::PrivateStore;
use serde::Deserialize;
use serde_json::json;
use std::{io::Read, path::Path, sync::Arc, time::Duration};

/// An installer emits only the guest namespace. Runtime executables and
/// runtime library paths are chosen separately by the application.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Descriptor { namespace: String }

pub(crate) fn install(
    registry: &mut Registry,
    manifest_json: &str,
    guest: &Path,
    launcher: Option<&Arc<dyn WorkerLauncher>>,
    store: Option<&Arc<PrivateStore>>,
    requests: crate::worker_services::NetworkRequests,
    approved: impl FnOnce(&chilpa_plugin::Manifest) -> bool,
) -> bool {
    registry.install_adapter(Runtime::Bridge, manifest_json, approved, |manifest| {
        let launcher = launcher.ok_or_else(|| PluginError::NotAllowed("the compatibility runtime is not available".into()))?;
        let store = store.ok_or_else(|| PluginError::NotAllowed("private plugin storage is not available".into()))?;
        let file = std::fs::File::open(guest.join("worker.json"))
            .map_err(|_| PluginError::NotAllowed("the worker descriptor is missing".into()))?;
        let mut bytes = Vec::new();
        file.take(4097).read_to_end(&mut bytes)
            .map_err(|_| PluginError::NotAllowed("the worker descriptor could not be read".into()))?;
        if bytes.len() > 4096 { return Err(PluginError::NotAllowed("the worker descriptor is too large".into())); }
        let descriptor: Descriptor = serde_json::from_slice(&bytes)
            .map_err(|_| PluginError::NotAllowed("the worker descriptor is invalid".into()))?;
        let context = WorkerContext::new(manifest.clone(), store.clone()).with_requests(requests);
        let transport = launcher.launch(guest).map_err(|_| PluginError::Crashed("the isolated worker could not start".into()))?;
        BridgePlugin::start(manifest, transport, context,
            json!({"root":launcher.guest_root(), "namespace":descriptor.namespace, "language":"EN"}), Duration::from_secs(15))
            .map(|plugin| Box::new(plugin) as Box<dyn chilpa_plugin::RuntimePlugin>)
    })
}
