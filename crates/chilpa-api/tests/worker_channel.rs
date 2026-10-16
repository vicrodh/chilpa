//! Real Perl framing and encrypted host storage, using a trusted tiny fixture.
//! This transport belongs to the test; it does not launch arbitrary plugins.

use chilpa_api::WorkerContext;
use chilpa_plugin::{bridge::{BridgeError, BridgeSession, WorkerTransport}, Manifest};
use chilpa_secrets::{PrivateStore, SecretBox};
use serde_json::json;
use std::{io::{Read, Write}, process::{Child, ChildStdin, Command, Stdio}, sync::{mpsc, Arc}, time::{Duration, Instant}};

struct FixtureTransport {
    child: Child,
    input: ChildStdin,
    replies: mpsc::Receiver<Result<Vec<u8>, BridgeError>>,
}

impl FixtureTransport {
    fn start() -> Self {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../compat/perl");
        let mut child = Command::new("perl")
            .arg(root.join("t/worker_fixture.pl")).arg(&root)
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().expect("Perl test runtime");
        let input = child.stdin.take().unwrap();
        let mut output = child.stdout.take().unwrap();
        let (sender, replies) = mpsc::sync_channel(1);
        std::thread::spawn(move || loop {
            let result = (|| {
                let mut size = [0; 4];
                output.read_exact(&mut size).map_err(|_| BridgeError::Disconnected)?;
                let size = u32::from_be_bytes(size) as usize;
                if size > chilpa_plugin::bridge::MAX_FRAME { return Err(BridgeError::TooLarge); }
                let mut bytes = vec![0; size];
                output.read_exact(&mut bytes).map_err(|_| BridgeError::Disconnected)?;
                Ok(bytes)
            })();
            let failed = result.is_err();
            if sender.send(result).is_err() || failed { break; }
        });
        Self { child, input, replies }
    }
}

impl WorkerTransport for FixtureTransport {
    fn send(&mut self, bytes: &[u8], deadline: Instant) -> Result<(), BridgeError> {
        if Instant::now() >= deadline { return Err(BridgeError::Timeout); }
        // Only small, trusted fixture inputs use this test-only pipe writer.
        self.input.write_all(&(bytes.len() as u32).to_be_bytes()).map_err(|_| BridgeError::Disconnected)?;
        self.input.write_all(bytes).map_err(|_| BridgeError::Disconnected)?;
        self.input.flush().map_err(|_| BridgeError::Disconnected)
    }
    fn receive(&mut self, deadline: Instant, _maximum: usize) -> Result<Vec<u8>, BridgeError> {
        let remaining = deadline.checked_duration_since(Instant::now()).ok_or(BridgeError::Timeout)?;
        self.replies.recv_timeout(remaining).map_err(|_| BridgeError::Timeout)?
    }
    fn stop(&mut self) { let _ = self.child.kill(); let _ = self.child.wait(); }
}

fn manifest(id: &str) -> Manifest {
    Manifest::parse(&json!({"id":id,"name":"Fixture","version":"1","api_version":1,
        "kind":"source","capabilities":[{"type":"storage"}]}).to_string()).unwrap()
}

#[test]
fn a_worker_restores_its_own_encrypted_preferences_after_restart() {
    let directory = tempfile::tempdir().unwrap();
    let secrets = Arc::new(SecretBox::derived_only("worker-test", directory.path()).unwrap());
    let records = directory.path().join("private");
    let store = Arc::new(PrivateStore::new(records.clone(), secrets));
    let mut context = WorkerContext::new(manifest("org.example.first"), store.clone());
    let input = json!({"namespace":"plugin.guest", "key":"accounts", "value":{"token":"private fixture token"}});
    {
        let mut worker = BridgeSession::new(FixtureTransport::start());
        let result = worker.call("set", input.clone(), Duration::from_secs(5), &mut context).unwrap();
        assert_eq!(result, json!({"saved":true}));
    }
    for owner in std::fs::read_dir(&records).unwrap() {
        for record in std::fs::read_dir(owner.unwrap().path()).unwrap() {
            let bytes = std::fs::read(record.unwrap().path()).unwrap();
            assert!(!bytes.windows(21).any(|slice| slice == b"private fixture token"));
        }
    }
    let mut restarted = BridgeSession::new(FixtureTransport::start());
    assert_eq!(restarted.call("get", input.clone(), Duration::from_secs(5), &mut context).unwrap(), input["value"]);
    let mut other_context = WorkerContext::new(manifest("org.example.second"), store);
    let mut other = BridgeSession::new(FixtureTransport::start());
    assert_eq!(other.call("get", input, Duration::from_secs(5), &mut other_context).unwrap(), serde_json::Value::Null);
    assert_eq!(other.call("wrong_id", json!({}), Duration::from_secs(5), &mut other_context), Err(BridgeError::InvalidMessage));
    assert_eq!(other.call("get", json!({}), Duration::from_secs(5), &mut other_context), Err(BridgeError::Disconnected));
}

#[test]
fn the_shared_registry_dispatches_a_native_browse_call_to_a_real_worker() {
    use chilpa_plugin::{bridge_plugin::BridgePlugin, Host, Limits, Registry, Reply, Runtime, RuntimePlugin, Services, TestHost};
    let directory = tempfile::tempdir().unwrap();
    let secrets = Arc::new(SecretBox::derived_only("registry-test", directory.path()).unwrap());
    let store = Arc::new(PrivateStore::new(directory.path().join("private"), secrets));
    let manifest = json!({"id":"org.example.worker","name":"Worker","version":"1","api_version":1,
        "runtime":"bridge","kind":"source","capabilities":[{"type":"storage"},{"type":"register_source"}]}).to_string();
    let mut registry = Registry::new();
    assert!(registry.install_adapter(Runtime::Bridge, &manifest, |_| true, |manifest| {
        let services = WorkerContext::new(manifest.clone(), store);
        BridgePlugin::start(manifest, FixtureTransport::start(), services, json!({}), Duration::from_secs(5))
            .map(|plugin| Box::new(plugin) as Box<dyn RuntimePlugin>)
    }));
    let host = Host::new().unwrap();
    let services: Services = Arc::new(std::sync::Mutex::new(TestHost::new()));
    let reply = registry.call(&host, &services, "org.example.worker", chilpa_plugin::op::BROWSE, b"{}", Limits::default()).unwrap();
    let Reply::Ok(bytes) = reply else { panic!("worker browse failed") };
    let listing: chilpa_plugin::messages::Listing = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(listing.entries[0].id, "guest://recording:17");
    assert_eq!(listing.title, "Library");
    assert_eq!(listing.entries[1].title, "7", "numeric guest labels cross the boundary as text");
    let reply = registry.call(&host, &services, "org.example.worker", "source.metadata", b"{}", Limits::default()).unwrap();
    let Reply::Ok(bytes) = reply else { panic!("worker metadata failed") };
    let metadata: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(metadata["presentation"]["advertised_quality"]["sample_rate_hz"], 88200);
    assert!(metadata.get("private").is_none());
    assert!(metadata["presentation"].get("private").is_none());
    assert_eq!(metadata["presentation"]["actions"][0]["enabled"], false);
}


#[test]
fn staged_inspection_acknowledges_initial_preferences_in_the_real_channel() {
    use chilpa_plugin::{bridge::WorkerLauncher, catalog::Digest};
    use sha2::{Digest as _, Sha256};
    use std::io::Cursor;
    struct Launcher;
    impl WorkerLauncher for Launcher {
        fn guest_root(&self) -> &str { "/guest" }
        fn launch(&self, _: &std::path::Path) -> Result<Box<dyn WorkerTransport>, BridgeError> {
            Ok(Box::new(FixtureTransport::start()))
        }
    }
    let root = tempfile::tempdir().unwrap();
    let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, contents) in [
        ("install.xml", "<extension><module>Plugins::Guest::Plugin</module><version>1</version></extension>"),
        ("Plugin.pm", "trusted fixture transport owns initialization"),
    ] {
        archive.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        archive.write_all(contents.as_bytes()).unwrap();
    }
    let bytes = archive.finish().unwrap().into_inner();
    let digest = Digest { algorithm: "sha256".into(), value: format!("{:x}", Sha256::digest(&bytes)) };
    let package = chilpa_api::StagedPackage::unpack(root.path(), &bytes, &digest).unwrap();
    let inspection = package.inspect(&Launcher, "EN").unwrap();
    assert_eq!(inspection.features.get("browse"), Some(&true));
    assert_eq!(inspection.settings_pages, ["settings.html"]);
}
