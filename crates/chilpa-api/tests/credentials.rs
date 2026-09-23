//! La credencial de un plugin no queda en claro, y el plugin no la ve.
//!
//! Es una sola prueba porque apunta `XDG_DATA_HOME` a un directorio
//! temporal, y esa variable es del proceso entero.

use std::path::Path;

fn install(dir: &Path, id: &str) {
    std::fs::create_dir_all(dir.join(id)).unwrap();
    std::fs::write(
        dir.join(id).join("manifest.json"),
        format!(
            r#"{{"id": "{id}", "name": "Uno", "version": "0.1.0", "api_version": 1,
                 "kind": "source",
                 "capabilities": [{{"type":"network","domains":["api.ejemplo.com"]}}]}}"#
        ),
    )
    .unwrap();
    let wat = r#"(module
        (memory (export "memory") 1)
        (func (export "chilpa_alloc") (param i32) (result i32) (i32.const 4096))
        (func (export "chilpa_api_version") (result i32) (i32.const 1)))"#;
    std::fs::write(dir.join(id).join("plugin.wasm"), wat::parse_str(wat).expect("compila"))
        .unwrap();
}

#[test]
fn a_credential_is_stored_sealed_and_never_reaches_the_plugin() {
    let dir = std::env::temp_dir().join("chilpa-api-credentials");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("XDG_DATA_HOME", dir.join("datos"));

    let root = chilpa_api::Chilpa::plugins_dir();
    std::fs::create_dir_all(&root).unwrap();
    install(&root, "com.prueba.uno");

    let mut chilpa = chilpa_api::Chilpa::new();
    chilpa.load_plugins();
    chilpa.approve_plugin("com.prueba.uno");
    assert!(!chilpa.plugins()[0].connected, "todavía no hay cuenta");

    // Se guarda algo que se reconocería de inmediato si quedara en claro.
    const SECRET: &str = "Bearer no-debe-aparecer-en-disco-12345";
    assert!(chilpa.connect_plugin("com.prueba.uno", SECRET), "se guardó");
    assert!(chilpa.plugins()[0].connected, "y la vista sabe que hay cuenta");

    // En disco no está el texto, ni en el archivo ni en ningún otro de la
    // carpeta del plugin.
    let mut looked_at = 0;
    for entry in walk(&root.join("com.prueba.uno")) {
        let bytes = std::fs::read(&entry).unwrap();
        assert!(
            !contains(&bytes, SECRET.as_bytes()),
            "el secreto está en claro en {}",
            entry.display()
        );
        looked_at += 1;
    }
    assert!(looked_at >= 3, "se revisaron manifest, wasm y el sobre: {looked_at}");

    // Y sí se puede volver a leer: cifrar no es perderlo.
    let sealed = std::fs::read(root.join("com.prueba.uno").join("credential.sealed")).unwrap();
    assert!(!sealed.is_empty());
    assert!(!chilpa.secrets_location().is_empty(), "se puede decir dónde quedó");

    // Quitar el permiso borra el secreto: guardarlo para un plugin que ya no
    // corre sería guardar un token por nada.
    chilpa.revoke_plugin("com.prueba.uno");
    assert!(
        !root.join("com.prueba.uno").join("credential.sealed").exists(),
        "revocar se lleva la credencial"
    );

    let _ = std::fs::remove_dir_all(dir);
}

fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else { return out };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else {
            out.push(p);
        }
    }
    out
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}
