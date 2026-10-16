//! Approval, state and quarantine must survive a different guest ABI.

use chilpa_plugin::{Host, Limits, Manifest, PluginError, Registry, Reply, RuntimePlugin, Services, TestHost};
use std::sync::{Arc, Mutex};

const MANIFEST: &str = r#"{
    "id":"org.example.foreign","name":"Foreign","version":"1",
    "api_version":1,"kind":"source","capabilities":[{"type":"register_panel"}]
}"#;

struct Stateful {
    manifest: Manifest,
    calls: u32,
}

impl RuntimePlugin for Stateful {
    fn manifest(&self) -> &Manifest { &self.manifest }

    fn call(
        &mut self,
        _host: &Host,
        _services: &Services,
        operation: &str,
        _input: &[u8],
        _limits: Limits,
    ) -> Result<Reply, PluginError> {
        match operation {
            "browse" => {
                self.calls += 1;
                Ok(Reply::Ok(format!("{{\"calls\":{}}}", self.calls).into_bytes()))
            }
            "crash" => Err(PluginError::Crashed("guest stopped".into())),
            _ => Err(PluginError::MissingExport(operation.into())),
        }
    }
}

fn load(manifest: Manifest) -> Result<Box<dyn RuntimePlugin>, PluginError> {
    Ok(Box::new(Stateful { manifest, calls: 0 }))
}

#[test]
fn pending_approval_never_starts_an_adapter() {
    let mut registry = Registry::new();
    assert!(!registry.install_with(MANIFEST, |_| false, |_| panic!("unapproved guest started")));
    assert!(registry.usable().is_empty());
    assert!(registry.panels().is_empty());
    assert_eq!(registry.state("org.example.foreign"), chilpa_plugin::State::PendingApproval);
}

#[test]
fn changing_to_a_worker_requires_new_approval_and_an_explicit_host_adapter() {
    let native = Manifest::parse(MANIFEST).unwrap();
    let foreign = MANIFEST.replace("\"kind\":\"source\"", "\"kind\":\"source\",\"runtime\":\"bridge\"");
    assert!(Manifest::parse(&foreign).is_err(), "the native loader cannot launch workers");
    let bridge = Manifest::parse_for_runtime(&foreign, chilpa_plugin::Runtime::Bridge).unwrap();
    assert!(!bridge.asks_for().beyond(&native.asks_for()).is_empty());
    let mut registry = Registry::new();
    assert!(!registry.install_adapter(chilpa_plugin::Runtime::Bridge, &foreign, |_| false,
        |_| panic!("unapproved worker started")));
    assert!(registry.install_adapter(chilpa_plugin::Runtime::Bridge, &foreign, |_| true, load));
}

#[test]
fn changing_the_manifest_in_the_adapter_is_refused() {
    let mut registry = Registry::new();
    assert!(!registry.install_with(MANIFEST, |_| true, |mut manifest| {
        manifest.id = "org.example.other".into();
        load(manifest)
    }));
    assert!(registry.usable().is_empty());
}

#[test]
fn state_survives_calls_and_the_shared_watchdog_stops_a_broken_guest() {
    let host = Host::new().unwrap();
    let services: Services = Arc::new(Mutex::new(TestHost::new()));
    let mut registry = Registry::new();
    assert!(registry.install_with(MANIFEST, |_| true, load));
    assert_eq!(registry.panels().len(), 1);
    for calls in [1, 2] {
        let result = registry.call(&host, &services, "org.example.foreign", "browse", b"{}", Limits::default()).unwrap();
        assert_eq!(result, Reply::Ok(format!("{{\"calls\":{calls}}}").into_bytes()));
    }
    for _ in 0..3 {
        assert!(registry.call(&host, &services, "org.example.foreign", "absent", b"{}", Limits::default()).is_err());
    }
    assert_eq!(registry.usable().len(), 1, "missing optional operations do not quarantine a guest");
    for _ in 0..3 {
        assert!(registry.call(&host, &services, "org.example.foreign", "crash", b"{}", Limits::default()).is_err());
    }
    assert!(registry.usable().is_empty());
    assert!(registry.panels().is_empty());
}
