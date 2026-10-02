//! Explicit package inspection in a disposable worker with no account access.

use chilpa_plugin::bridge::{BridgeError, BridgeSession, ServiceOperation, WorkerLauncher, WorkerServices};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, time::{Duration, Instant}};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PluginInspection {
    pub features: BTreeMap<String, bool>,
    pub settings_pages: Vec<String>,
    pub limitations: Vec<String>,
    #[serde(default)]
    pub diagnostics: BTreeMap<String, u64>,
}

impl crate::StagedPackage {
    /// Called only after choosing to inspect the downloaded package. It
    /// shares neither persistent preferences nor network permissions with
    /// an existing installation. The worker is terminated before returning.
    pub fn inspect(&self, launcher: &dyn WorkerLauncher, language: &str) -> Result<PluginInspection, String> {
        let (root, metadata) = self.extension()?;
        let transport = launcher.launch(&root).map_err(|_| "The isolated inspection runtime could not start.")?;
        let mut session = BridgeSession::new(transport);
        let mut services = InspectionServices::default();
        let result = session.call("initialize", json!({"root": launcher.guest_root(),
            "namespace": metadata.namespace, "language": language}), Duration::from_secs(15), &mut services)
            .map_err(|_| "The isolated plugin inspection did not finish.")?;
        if result.get("error").is_some() {
            return Err("The plugin failed during initialization (inspection.initialize).".into());
        }
        let report: PluginInspection = serde_json::from_value(result)
            .map_err(|_| "The plugin could not initialize with the available compatibility interfaces.")?;
        if report.features.len() > 128 || report.settings_pages.len() > 128 || report.limitations.len() > 128
            || report.diagnostics.len() > 128
            || report.features.keys().chain(report.diagnostics.keys()).chain(report.settings_pages.iter())
                .chain(report.limitations.iter()).any(|item| item.len() > 4096) {
            return Err("The plugin inspection report is too large.".into());
        }
        Ok(report)
    }
}

#[derive(Default)]
struct InspectionServices { values: BTreeMap<String, Value> }

impl WorkerServices for InspectionServices {
    fn handle(&mut self, operation: ServiceOperation, deadline: Instant) -> Result<Value, BridgeError> {
        if Instant::now() >= deadline { return Err(BridgeError::Timeout); }
        match operation {
            ServiceOperation::PrivateGet { key } => Ok(self.values.get(&key).cloned().unwrap_or(Value::Null)),
            ServiceOperation::PrivateSet { key, value } => {
                if key.len() > 256 || (!self.values.contains_key(&key) && self.values.len() >= 128) {
                    return Err(BridgeError::TooLarge);
                }
                let previous = self.values.insert(key.clone(), value);
                if serde_json::to_vec(&self.values).map_err(|_| BridgeError::InvalidMessage)?.len() > 1024 * 1024 {
                    if let Some(previous) = previous { self.values.insert(key, previous); }
                    else { self.values.remove(&key); }
                    return Err(BridgeError::TooLarge);
                }
                Ok(Value::Bool(true))
            }
            ServiceOperation::Http { .. } => Err(BridgeError::Denied),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspection_preferences_are_bounded_disposable_and_have_no_network() {
        let mut services = InspectionServices::default();
        let deadline = Instant::now() + Duration::from_secs(3);
        assert_eq!(services.handle(ServiceOperation::PrivateSet { key: "preferences".into(), value: json!({"fresh":true}) }, deadline).unwrap(), Value::Bool(true));
        assert_eq!(services.handle(ServiceOperation::PrivateGet { key: "preferences".into() }, deadline).unwrap(), json!({"fresh":true}));
        assert_eq!(services.handle(ServiceOperation::Http { request: json!({"url":"https://example.org"}) }, deadline), Err(BridgeError::Denied));
        assert_eq!(services.handle(ServiceOperation::PrivateSet { key: "preferences".into(), value: json!("x".repeat(1024 * 1024)) }, deadline), Err(BridgeError::TooLarge));
        assert_eq!(services.handle(ServiceOperation::PrivateGet { key: "preferences".into() }, deadline).unwrap(), json!({"fresh":true}));
        assert_eq!(InspectionServices::default().handle(ServiceOperation::PrivateGet { key: "preferences".into() }, deadline).unwrap(), Value::Null);
    }
}
