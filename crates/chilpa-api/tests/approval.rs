//! Nobody runs without permission (ADR-008).
//!
//! What is checked here is not that an unapproved plugin's calls are
//! refused: it is that it **does not load**. Its wasm is not compiled and no
//! instance of it exists in the process.
//!
//! It is a single test because it points `XDG_DATA_HOME` at a temporary
//! directory, and that variable belongs to the whole process.

use std::path::Path;

/// A plugin that answers a fixed JSON, with whatever capabilities it is given.
fn install(dir: &Path, id: &str, capabilities: &str) {
    std::fs::create_dir_all(dir.join(id)).unwrap();
    std::fs::write(
        dir.join(id).join("manifest.json"),
        format!(
            r#"{{"id": "{id}", "name": "One", "version": "0.1.0", "api_version": 1,
                 "kind": "source", "capabilities": [{capabilities}]}}"#
        ),
    )
    .unwrap();
    let reply = r#"{"tracks":[{"external_id":"a","title":"One"}]}"#;
    let wat = format!(
        r#"(module
            (memory (export "memory") 1)
            (global $next (mut i32) (i32.const 4096))
            (func (export "chilpa_alloc") (param i32) (result i32)
              (local i32)
              (local.set 1 (global.get $next))
              (global.set $next (i32.add (global.get $next) (local.get 0)))
              (local.get 1))
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (data (i32.const 8) "{escaped}")
            (func (export "chilpa_search") (param i32 i32) (result i64)
              (i64.or (i64.shl (i64.const 8) (i64.const 32)) (i64.const {len}))))"#,
        escaped = reply.replace('"', "\\\""),
        len = reply.len(),
    );
    std::fs::write(dir.join(id).join("plugin.wasm"), wat::parse_str(&wat).expect("it compiles"))
        .unwrap();
}

#[test]
fn a_plugin_does_not_run_until_the_user_approves_what_it_asks() {
    let dir = std::env::temp_dir().join("chilpa-api-approval");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("XDG_DATA_HOME", dir.join("data"));

    let root = chilpa_api::Chilpa::plugins_dir();
    std::fs::create_dir_all(&root).unwrap();
    install(
        &root,
        "com.test.one",
        r#"{"type":"storage"},{"type":"network","domains":["api.example.com"]}"#,
    );

    let mut chilpa = chilpa_api::Chilpa::new();
    assert_eq!(chilpa.load_plugins(), 0, "being in the folder is not enough to run");

    let listed = chilpa.plugins();
    assert_eq!(listed.len(), 1, "it is listed, so it can be approved");
    let one = &listed[0];
    assert_eq!(one.state, "awaiting approval");
    assert!(!one.wants_more, "it is new, not one that started asking for more");

    // What it asks for is said in words, which is what can be approved.
    assert_eq!(one.asks.len(), 2, "{:?}", one.asks);
    assert_eq!(one.pending, one.asks, "on a new one, everything is still to approve");
    assert!(one.asks.iter().any(|a| a.contains("api.example.com")), "{:?}", one.asks);
    assert!(one.asks.iter().any(|a| a.contains("keep")), "{:?}", one.asks);

    // And it cannot be asked for anything, because there is nobody to ask.
    let e = chilpa
        .call_plugin("com.test.one", "chilpa_search", b"{}")
        .expect_err("there is no instance to call");
    assert!(e.contains("approved"), "{e}");

    // The user says yes: now it loads and answers.
    assert_eq!(chilpa.approve_plugin("com.test.one"), 1, "approving sets it running");
    assert_eq!(chilpa.plugins()[0].state, "fine");
    let r = chilpa.call_plugin("com.test.one", "chilpa_search", b"{}").expect("it answers");
    assert!(String::from_utf8_lossy(&r).contains("One"));

    // A new version that asks for more asks again, and says what is new
    // instead of asking for everything over.
    install(
        &root,
        "com.test.one",
        r#"{"type":"storage"},{"type":"network","domains":["api.example.com"]},
           {"type":"library_read"}"#,
    );
    let mut other = chilpa_api::Chilpa::new();
    assert_eq!(other.load_plugins(), 0, "what was approved before is no longer enough");
    let after = &other.plugins()[0];
    assert_eq!(after.state, "awaiting approval");
    assert!(after.wants_more, "it is not new: it is one that started asking for more");
    assert_eq!(after.pending.len(), 1, "only what is new: {:?}", after.pending);
    assert!(after.pending[0].contains("library"), "{:?}", after.pending);

    // And revoking switches it off right away, not at the next startup.
    assert_eq!(other.approve_plugin("com.test.one"), 1);
    assert_eq!(other.revoke_plugin("com.test.one"), 0, "revoking leaves it out");
    assert_eq!(other.plugins()[0].state, "awaiting approval");
    let e = other
        .call_plugin("com.test.one", "chilpa_search", b"{}")
        .expect_err("there is no instance any more");
    assert!(e.contains("approved"), "{e}");

    // And uninstalling takes its whole folder, not just the permission.
    assert!(other.uninstall_plugin("com.test.one"), "{}", other.now_playing().error);
    assert!(!root.join("com.test.one").exists(), "the folder is gone");
    assert!(other.plugins().is_empty(), "and it is not listed any more");

    let _ = std::fs::remove_dir_all(dir);
}
