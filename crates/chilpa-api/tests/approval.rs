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
            r#"{{"id": "{id}", "name": "Uno", "version": "0.1.0", "api_version": 1,
                 "kind": "source", "capabilities": [{capabilities}]}}"#
        ),
    )
    .unwrap();
    let reply = r#"{"tracks":[{"external_id":"a","title":"Una"}]}"#;
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
    std::fs::write(dir.join(id).join("plugin.wasm"), wat::parse_str(&wat).expect("compila"))
        .unwrap();
}

#[test]
fn a_plugin_does_not_run_until_the_user_approves_what_it_asks() {
    let dir = std::env::temp_dir().join("chilpa-api-approval");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("XDG_DATA_HOME", dir.join("datos"));

    let root = chilpa_api::Chilpa::plugins_dir();
    std::fs::create_dir_all(&root).unwrap();
    install(
        &root,
        "com.prueba.uno",
        r#"{"type":"storage"},{"type":"network","domains":["api.ejemplo.com"]}"#,
    );

    let mut chilpa = chilpa_api::Chilpa::new();
    assert_eq!(chilpa.load_plugins(), 0, "estar en la carpeta no alcanza para correr");

    let listed = chilpa.plugins();
    assert_eq!(listed.len(), 1, "se lista, para poder aprobarlo");
    let one = &listed[0];
    assert_eq!(one.state, "pendiente de aprobación");
    assert!(!one.wants_more, "es nuevo, no uno que empezó a pedir más");

    // What it asks for is said in words, which is what can be approved.
    assert_eq!(one.asks.len(), 2, "{:?}", one.asks);
    assert_eq!(one.pending, one.asks, "de uno nuevo, todo está por aprobar");
    assert!(one.asks.iter().any(|a| a.contains("api.ejemplo.com")), "{:?}", one.asks);
    assert!(one.asks.iter().any(|a| a.contains("guardar")), "{:?}", one.asks);

    // And it cannot be asked for anything, because there is nobody to ask.
    let e = chilpa
        .call_plugin("com.prueba.uno", "chilpa_search", b"{}")
        .expect_err("no hay instancia que llamar");
    assert!(e.contains("aprobado"), "{e}");

    // The user says yes: now it loads and answers.
    assert_eq!(chilpa.approve_plugin("com.prueba.uno"), 1, "aprobar lo pone a correr");
    assert_eq!(chilpa.plugins()[0].state, "bien");
    let r = chilpa.call_plugin("com.prueba.uno", "chilpa_search", b"{}").expect("contesta");
    assert!(String::from_utf8_lossy(&r).contains("Una"));

    // A new version that asks for more asks again, and says what is new
    // instead of asking for everything over.
    install(
        &root,
        "com.prueba.uno",
        r#"{"type":"storage"},{"type":"network","domains":["api.ejemplo.com"]},
           {"type":"library_read"}"#,
    );
    let mut other = chilpa_api::Chilpa::new();
    assert_eq!(other.load_plugins(), 0, "lo aprobado antes ya no alcanza");
    let after = &other.plugins()[0];
    assert_eq!(after.state, "pendiente de aprobación");
    assert!(after.wants_more, "no es nuevo: es uno que empezó a pedir más");
    assert_eq!(after.pending.len(), 1, "solo lo nuevo: {:?}", after.pending);
    assert!(after.pending[0].contains("biblioteca"), "{:?}", after.pending);

    // And revoking switches it off right away, not at the next startup.
    assert_eq!(other.approve_plugin("com.prueba.uno"), 1);
    assert_eq!(other.revoke_plugin("com.prueba.uno"), 0, "revocar lo deja fuera");
    assert_eq!(other.plugins()[0].state, "pendiente de aprobación");
    let e = other
        .call_plugin("com.prueba.uno", "chilpa_search", b"{}")
        .expect_err("ya no hay instancia");
    assert!(e.contains("aprobado"), "{e}");

    // And uninstalling takes its whole folder, not just the permission.
    assert!(other.uninstall_plugin("com.prueba.uno"), "{}", other.now_playing().error);
    assert!(!root.join("com.prueba.uno").exists(), "la carpeta se fue");
    assert!(other.plugins().is_empty(), "y ya no se lista");

    let _ = std::fs::remove_dir_all(dir);
}
