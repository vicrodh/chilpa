//! The shared registry's adapter for an already isolated compatibility worker.

use crate::{abi::read_reply, bridge::{BridgeError, WorkerServices, WorkerTransport}, worker_client::WorkerClient,
    Capability, Halt, Host, Limits, Manifest, PluginError, Reply, Runtime, RuntimePlugin, Services};
use serde_json::{json, Value};
use std::time::Duration;

pub struct BridgePlugin<T: WorkerTransport, S: WorkerServices> {
    manifest: Manifest,
    worker: WorkerClient<T, S>,
    features: Value,
}

impl<T: WorkerTransport, S: WorkerServices> BridgePlugin<T, S> {
    /// Call only inside the registry's approved factory. The transport must
    /// already enforce the worker's fixed address-space and process budget.
    /// Fuel and linear-memory limits apply to WASM; this adapter enforces the
    /// shared per-call clock in addition to its process isolation policy.
    pub fn start(manifest: Manifest, transport: T, services: S, initialization: Value, timeout: Duration) -> Result<Self, PluginError> {
        if manifest.runtime != Runtime::Bridge {
            return Err(PluginError::NotAllowed("a compatibility worker requires the bridge runtime".into()));
        }
        let mut worker = WorkerClient::new(transport, services);
        let inspected = worker.call("initialize", initialization, timeout).map_err(|error| failure(error, timeout))?;
        if inspected.get("error").is_some() {
            return Err(PluginError::Crashed("compatibility worker initialization failed".into()));
        }
        let features = inspected.get("features").filter(|value| value.is_object()).cloned()
            .ok_or_else(|| PluginError::Crashed("compatibility worker returned invalid features".into()))?;
        Ok(Self { manifest, worker, features })
    }
}

impl<T: WorkerTransport, S: WorkerServices> RuntimePlugin for BridgePlugin<T, S> {
    fn manifest(&self) -> &Manifest { &self.manifest }

    fn alive(&self) -> bool { self.worker.alive() }

    fn call(&mut self, _: &Host, _: &Services, operation: &str, input: &[u8], limits: Limits) -> Result<Reply, PluginError> {
        let mut input: Value = serde_json::from_slice(input)
            .map_err(|_| PluginError::NotAllowed("invalid compatibility request".into()))?;
        let mapped = match operation {
            crate::op::BROWSE if self.features["browse"] == true => "source.browse",
            crate::op::SEARCH if self.features["search"] == true => "source.query",
            crate::op::RESOLVER if self.features["resolve"] == true => {
                input = json!({"id":input.get("external_id").and_then(Value::as_str).unwrap_or("")});
                "source.resolve"
            }
            "settings.open" | "settings.submit" | "settings.document" | "inspect" | "tick" => operation,
            "source.metadata" => operation,
            _ => return Err(PluginError::MissingExport(operation.into())),
        };
        if mapped.starts_with("source.") && !self.manifest.has(&Capability::RegisterSource) {
            return Err(PluginError::NotAllowed("source access was not approved".into()));
        }
        if mapped == "settings.submit" && !self.manifest.has(&Capability::GuestCredentials) {
            return Err(PluginError::NotAllowed("guest sign-in access was not approved".into()));
        }
        let cursor = input.get("cursor").and_then(Value::as_str).map(str::to_string);
        let mut reply = match self.worker.call(mapped, input, limits.timeout) {
            Ok(reply) => reply,
            Err(BridgeError::InvalidSubmission) => return Ok(Reply::Error("The settings form changed or contains an invalid value.".into())),
            Err(error) => return Err(failure(error, limits.timeout)),
        };
        if reply.get("error").is_none() && mapped == "source.browse" {
            let mut document: crate::presentation::SourceDocument = serde_json::from_value(reply)
                .map_err(|_| PluginError::Crashed("invalid worker listing".into()))?;
            // This profile delivers finite recordings through the remote
            // opener, which mediates media redirects. A guest cannot select
            // the unrestricted live-radio opening path by setting a flag.
            for entry in &mut document.listing.entries { entry.live = false; }
            let document = document.clean(cursor.as_deref()).map_err(|_| PluginError::Crashed("invalid presentation metadata".into()))?;
            reply = serde_json::to_value(document)
                .map_err(|_| PluginError::Crashed("invalid worker listing".into()))?;
        }
        if reply.get("error").is_none() && mapped == "source.query" {
            let results: crate::Results = serde_json::from_value(reply)
                .map_err(|_| PluginError::Crashed("invalid worker search results".into()))?;
            reply = serde_json::to_value(results.clean())
                .map_err(|_| PluginError::Crashed("invalid worker search results".into()))?;
        }
        if reply.get("error").is_none() && mapped == "source.metadata" {
            if reply.get("available").and_then(Value::as_bool) == Some(true) {
                let mut presentation: crate::presentation::Presentation = serde_json::from_value(reply["presentation"].clone())
                    .map_err(|_| PluginError::Crashed("invalid worker metadata".into()))?;
                presentation.clean().map_err(|_| PluginError::Crashed("invalid worker metadata".into()))?;
                reply = json!({"available":true, "presentation":presentation});
            } else {
                reply = json!({"available":false});
            }
        }
        let bytes = serde_json::to_vec(&reply).map_err(|_| PluginError::Crashed("invalid compatibility response".into()))?;
        Ok(read_reply(bytes))
    }
}

fn failure(error: BridgeError, timeout: Duration) -> PluginError {
    match error {
        BridgeError::Timeout => PluginError::Stopped(Halt::Timeout(timeout)),
        BridgeError::Denied => PluginError::NotAllowed("worker request was denied".into()),
        BridgeError::Disconnected => PluginError::Crashed("the compatibility worker disconnected; restart the plugin".into()),
        BridgeError::TooLarge => PluginError::Crashed("the compatibility worker exceeded the response size limit".into()),
        BridgeError::ServiceLimit => PluginError::Crashed("the compatibility worker exceeded the host request limit".into()),
        BridgeError::Storage => PluginError::Crashed("the compatibility worker could not access private storage".into()),
        _ => PluginError::Crashed("the compatibility worker returned an invalid message".into()),
    }
}
