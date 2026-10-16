//! A real plugin, on disk, end to end.
//!
//! The sandbox tests live in `chilpa-plugin` and touch no disk. This one
//! checks the other half: that a plugin dropped in its folder loads, can be
//! asked for something, and that what Chilpa lends it -- the store, the
//! time, the log -- arrives and works.
//!
//! It is a single test because it points `XDG_DATA_HOME` at a temporary
//! directory, and that variable belongs to the whole process.

use std::path::{Path, PathBuf};

/// The minimum a plugin has to export, plus a scratchpad.
const PRELUDE: &str = r#"
  (memory (export "memory") 1)
  (global $next (mut i32) (i32.const 1024))
  (func (export "chilpa_alloc") (param i32) (result i32)
    (local i32)
    (local.set 1 (global.get $next))
    (global.set $next (i32.add (global.get $next) (local.get 0)))
    (local.get 1))
  (data (i32.const 0) "{\"greeting\":\"hi\"}")
  (data (i32.const 64) "last-sync")
  (data (i32.const 96) "42")
"#;

fn install(root: &Path, id: &str, capabilities: &str, body: &str) {
    let dir = root.join(id);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("manifest.json"),
        format!(
            r#"{{
                "id": "{id}",
                "name": "Test plugin",
                "version": "0.1.0",
                "api_version": 1,
                "kind": "source",
                "capabilities": [{capabilities}]
            }}"#
        ),
    )
    .unwrap();
    let wat = format!(
        r#"(module
            (import "chilpa" "store_get" (func $get (param i32 i32 i32 i32) (result i32)))
            (import "chilpa" "store_set" (func $set (param i32 i32 i32 i32) (result i32)))
            (import "chilpa" "log" (func $log (param i32 i32 i32)))
            (import "chilpa" "now" (func $now (result i64)))
            {PRELUDE}
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            {body})"#
    );
    std::fs::write(dir.join("plugin.wasm"), wat::parse_str(&wat).expect("it compiles")).unwrap();
}

#[test]
fn a_plugin_in_its_folder_loads_and_answers() {
    let dir = std::env::temp_dir().join("chilpa-api-plugins");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("XDG_DATA_HOME", dir.join("data"));
    let root: PathBuf = chilpa_api::Chilpa::plugins_dir();
    std::fs::create_dir_all(&root).unwrap();

    // One that keeps something, reads it back, writes to the log and answers.
    install(
        &root,
        "com.test.good",
        r#"{"type": "storage"}"#,
        r#"(func (export "greet") (param i32 i32) (result i64)
             (drop (call $set (i32.const 64) (i32.const 9) (i32.const 96) (i32.const 2)))
             (drop (call $get (i32.const 64) (i32.const 9) (i32.const 2048) (i32.const 64)))
             (call $log (i32.const 1) (i32.const 64) (i32.const 9))
             (drop (call $now))
             (i64.const 17))"#,
    );
    // And one that asks for an API version that does not exist.
    install(&root, "com.test.old", "", "(func (export \"greet\") (param i32 i32) (result i64) (i64.const 17))");
    let old = root.join("com.test.old").join("manifest.json");
    let m = std::fs::read_to_string(&old).unwrap().replace(r#""api_version": 1"#, r#""api_version": 42"#);
    std::fs::write(&old, m).unwrap();

    let mut chilpa = chilpa_api::Chilpa::new();
    assert_eq!(chilpa.load_plugins(), 0, "nobody runs without approval (ADR-008)");

    // Nothing can be approved for the old one: its manifest is not even
    // read, so there is nothing to show the user to decide on.
    assert_eq!(chilpa.approve_plugin("com.test.old"), 0);
    let usable = chilpa.approve_plugin("com.test.good");
    assert_eq!(usable, 1, "one loaded and the other did not");

    let list = chilpa.plugins();
    assert_eq!(list.len(), 2, "both are listed, loaded or not");
    let good = list.iter().find(|p| p.id == "com.test.good").unwrap();
    let old = list.iter().find(|p| p.id == "com.test.old").unwrap();
    assert_eq!(good.state, "fine");
    assert_eq!(good.name, "Test plugin");
    assert_eq!(old.state, "quarantined");
    assert!(old.problem.contains("version"), "{}", old.problem);

    // It is asked for something and answers its JSON.
    let r = chilpa.call_plugin("com.test.good", "greet", b"{}").expect("it answers");
    assert_eq!(String::from_utf8_lossy(&r), r#"{"greeting":"hi"}"#);

    // And what it kept ended up in its folder, not in anyone else's.
    let saved = root.join("com.test.good").join("store").join("last-sync");
    assert_eq!(std::fs::read(&saved).unwrap(), b"42");
    assert!(!root.join("com.test.old").join("store").exists());

    // The quarantined one is asked for nothing, however much one insists.
    let e = chilpa.call_plugin("com.test.old", "greet", b"{}").expect_err("it is not called");
    assert!(e.contains("quarantine"), "{e}");

    // Until the user turns it back on; then it fails to load again,
    // because it never loaded.
    chilpa.reactivate_plugin("com.test.old");
    let e = chilpa.call_plugin("com.test.old", "greet", b"{}").expect_err("it still does not load");
    assert!(e.contains("did not load"), "{e}");

    // One that does not exist is said and that is that.
    let e = chilpa.call_plugin("com.does.not.exist", "greet", b"{}").expect_err("it is not there");
    assert!(e.contains("not installed"), "{e}");

    let _ = std::fs::remove_dir_all(dir);
}
