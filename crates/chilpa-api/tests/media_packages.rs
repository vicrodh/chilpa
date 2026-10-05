//! I opt into a read-only smoke of built medium plugins with CHILPA_MEDIA_PACKAGES.
//! No account, library, audio output or installed plugin is touched.
use chilpa_plugin::{Host, HostServices, Level, Limits, Manifest, Reply, Services};
use chilpa_plugin::presentation::SourceDocument;
use chilpa_plugin::messages::{Browse, EntryKind};
use std::{collections::BTreeMap, sync::{Arc, Mutex}};

#[derive(Default)]
struct Network { store: BTreeMap<String, Vec<u8>>, requests: usize }
impl HostServices for Network {
    fn log(&mut self, _: &str, _: Level, _: &str) {}
    fn now_unix(&mut self) -> i64 { 0 }
    fn store_get(&mut self, _: &str, key: &str) -> Option<Vec<u8>> { self.store.get(key).cloned() }
    fn store_set(&mut self, _: &str, key: &str, value: &[u8]) -> bool { self.store.insert(key.into(), value.into()); true }
    fn http(&mut self, _: &str, request: &[u8]) -> Vec<u8> {
        self.requests += 1;
        let value: serde_json::Value = serde_json::from_slice(request).unwrap();
        assert_eq!(value["method"].as_str().unwrap_or("GET"), "GET");
        chilpa_core::send_request(request, None)
    }
}

#[test]
fn packaged_media_browse_through_the_real_sandbox_and_clean_contract() {
    let Some(root) = std::env::var_os("CHILPA_MEDIA_PACKAGES") else { return };
    let root = std::path::PathBuf::from(root);
    let mut seen = 0;
    for directory in std::fs::read_dir(root).unwrap().flatten().map(|entry| entry.path()).filter(|path| path.is_dir()) {
        let manifest = Manifest::parse(&std::fs::read_to_string(directory.join("manifest.json")).unwrap()).unwrap();
        let host = Host::new().unwrap();
        let plugin = host.load(manifest, &std::fs::read(directory.join("plugin.wasm")).unwrap()).unwrap();
        let network = Arc::new(Mutex::new(Network::default()));
        let services: Services = network.clone();
        let ask = |request: &Browse| {
            let result = plugin.call(&host, &services, "chilpa_browse", &serde_json::to_vec(request).unwrap(), Limits::long()).unwrap();
            let bytes = match result { Reply::Ok(bytes) => bytes, Reply::Error(error) => panic!("{error}") };
            serde_json::from_slice::<SourceDocument>(&bytes).unwrap().clean(request.cursor.as_deref()).unwrap()
        };
        let document = ask(&Browse { language: "es-MX".into(), ..Browse::root() });
        assert!(["radio", "podcasts"].contains(&document.medium.as_str()));
        assert!(!document.attribution.is_empty());
        assert!(!document.listing.entries.is_empty());
        assert!(document.listing.entries.iter().filter(|entry| entry.kind != EntryKind::Search).count() <= 20);
        let requests = network.lock().unwrap().requests;
        let again = ask(&Browse { language: "es-MX".into(), ..Browse::root() });
        assert_eq!(again.listing.entries.len(), document.listing.entries.len());
        assert_eq!(network.lock().unwrap().requests, requests, "Fresh instances must reuse the directory cache");
        if document.medium == "podcasts" {
            let show = document.listing.entries.iter().find(|entry| entry.kind == EntryKind::Folder).unwrap();
            let episodes = ask(&Browse { language: "es".into(), ..Browse::at(&show.id) });
            assert!(episodes.listing.entries.iter().any(|entry| entry.kind == EntryKind::Item));
            assert_eq!(episodes.medium, "podcasts");
        } else {
            assert!(document.listing.entries.iter().any(|entry| entry.live));
            assert!(document.facets.iter().any(|column| column.id == "order"));
        }
        seen += 1;
    }
    assert_eq!(seen, 2);
}
