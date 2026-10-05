//! I exercise cross-plugin requests through real sandboxed native pipes.
#![cfg(target_os = "linux")]
use serde_json::{json, Value};
use std::{os::unix::fs::PermissionsExt, time::Duration};
const PEER: &str = r#"#!/usr/bin/python3
import json,sys
for line in sys.stdin:
    q=json.loads(line);m=q.get('method');p=q.get('params',{})
    if m=='initialize': r={'protocol':1,'capabilities':{'search':True,'playlist_edit':True}}
    elif m=='search': r={'tracks':[{'kind':'track','ref':'track:1','title':'First','artist':'Band','isrc':'ABC','quality':{'codec':'flac','bits':24,'rate':96000},'access_token':'private','url':'signed'}]}
    elif m=='page.action' and p.get('action')=='playlist.create': r={'item':{'ref':'playlist:new'}}
    else: r={}
    print(json.dumps({'jsonrpc':'2.0','id':q['id'],'result':r}),flush=True)
"#;
const IMPORTER: &str = r#"#!/usr/bin/python3
import json,sys
pending={}
for line in sys.stdin:
    q=json.loads(line);m=q.get('method');p=q.get('params',{})
    if m=='host.reply':
        original=pending.pop(p['id']); print(json.dumps({'jsonrpc':'2.0','id':original,'result':p}),flush=True);continue
    if m=='initialize': print(json.dumps({'jsonrpc':'2.0','id':q['id'],'result':{'protocol':1,'capabilities':{'playlist_import':1}}}),flush=True)
    elif m=='forward':
        key=str(q['id']);pending[key]=q['id'];print(json.dumps({'jsonrpc':'2.0','method':'host.request','params':{'id':key,'method':p['method'],'params':p.get('params',{})}}),flush=True)
    else: print(json.dumps({'jsonrpc':'2.0','id':q['id'],'result':{}}),flush=True)
"#;
fn install(id: &str, program: &str, capabilities: Value) {
    let folder = chilpa_api::Chilpa::plugins_dir().join(id);
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("manifest.json"),json!({"id":id,"name":id,"version":"0.1.0","api_version":1,"kind":"source","runtime":"process","capabilities":capabilities}).to_string()).unwrap();
    std::fs::write(folder.join("plugin"), program).unwrap();
    std::fs::set_permissions(
        folder.join("plugin"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
}
#[test]
fn a_plugin_can_read_peer_metadata_only_after_permission_and_cannot_write_without_a_grant() {
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("XDG_DATA_HOME", dir.path().join("data"));
    std::env::set_var("XDG_CACHE_HOME", dir.path().join("cache"));
    install("com.test.peer", PEER, json!([{"type":"register_source"}]));
    install(
        "com.test.import",
        IMPORTER,
        json!([{"type":"plugins_read"},{"type":"library_read"},{"type":"playlists_write"},{"type":"register_command"}]),
    );
    install(
        "com.test.denied",
        IMPORTER,
        json!([{"type":"register_command"}]),
    );
    let mut api = chilpa_api::Chilpa::new();
    api.load_plugins();
    for (index, id) in ["com.test.peer", "com.test.import", "com.test.denied"]
        .into_iter()
        .enumerate()
    {
        assert_eq!(api.approve_plugin(id), index + 1, "{id}");
    }
    let call = |id, method, params| {
        api.native_call(
            id,
            "forward",
            json!({"method":method,"params":params}),
            Duration::from_secs(10),
        )
        .unwrap()
    };
    let found = call(
        "com.test.import",
        "catalog.search",
        json!({"source":"com.test.peer","text":"Band First"}),
    );
    assert_eq!(
        found["result"]["tracks"][0]["quality"]["rate"], 96000,
        "{found}"
    );
    assert!(!found.to_string().contains("private"));
    assert!(!found.to_string().contains("signed"));
    assert!(call("com.test.denied", "sources.list", json!({}))["error"].is_string());
    assert!(call(
        "com.test.import",
        "catalog.search",
        json!({"source":"com.test.absent","text":"First"})
    )["error"]
        .is_string());
    assert!(call(
        "com.test.import",
        "playlists.create",
        json!({"target":"com.test.peer","title":"Unauthorized"})
    )["error"]
        .is_string());
    assert!(call("com.test.import", "secrets.read", json!({}))["error"].is_string());
    if let Some(binary) = std::env::var_os("CHILPA_IMPORTER_TEST_BINARY") {
        let id = "app.chilpa.playlist-import";
        install(
            id,
            IMPORTER,
            json!([{"type":"plugins_read"},{"type":"library_read"},{"type":"playlists_write"},{"type":"register_command"}]),
        );
        std::fs::copy(
            binary,
            chilpa_api::Chilpa::plugins_dir().join(id).join("plugin"),
        )
        .unwrap();
        assert_eq!(api.load_plugins(), 3);
        assert_eq!(api.approve_plugin(id), 4);
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            let state: Value = serde_json::from_str(&api.playlist_import_json()).unwrap();
            if state["options"]["inputs"].is_array() {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "importer options timeout: {state}"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD
            .encode(b"#EXTM3U\n#EXTINF:120,Band - First\nfirst.flac\n");
        api.playlist_import_command(
            "preview",
            &json!({"input":"file","filename":"smoke.m3u","bytes":bytes}).to_string(),
        );
        wait_for(&mut api, "preview");
        api.playlist_import_command(
            "match",
            &json!({"targets":["com.test.peer","mixed"],"title":"Imported"}).to_string(),
        );
        let ready = wait_for(&mut api, "ready");
        assert_eq!(
            ready["rows"][0]["matches"]["mixed"]["source"], "com.test.peer",
            "{ready}"
        );
        api.playlist_import_command("commit", "{}");
        let done = wait_for(&mut api, "done");
        assert_eq!(done["outputs"].as_array().unwrap().len(), 2, "{done}");
        for output in done["outputs"].as_array().unwrap() {
            assert_eq!(output["written"], 1, "{output}");
            assert_eq!(output["error"], "", "{output}");
        }
        let library =
            chilpa_library::Library::open_read_only(&chilpa_library::Library::default_path())
                .unwrap();
        let local = library
            .playlists()
            .unwrap()
            .into_iter()
            .find(|p| p.provider == "local" && p.name == "Imported")
            .unwrap();
        let entries = library.playlist_entries(local.id).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].external_id.contains("com.test.peer"));
    }
}

fn wait_for(api: &mut chilpa_api::Chilpa, phase: &str) -> Value {
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    loop {
        let document: Value = serde_json::from_str(&api.playlist_import_json()).unwrap();
        assert_eq!(document["error"], "", "{document}");
        let state = &document["state"];
        if state["phase"] == phase {
            return state.clone();
        }
        assert!(
            std::time::Instant::now() < deadline,
            "waiting for {phase}: {document}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}
