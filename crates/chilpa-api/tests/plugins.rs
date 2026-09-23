//! Un plugin de verdad, en disco, de punta a punta.
//!
//! Las pruebas del sandbox viven en `chilpa-plugin` y no tocan el disco.
//! Esta comprueba lo otro: que un plugin puesto en su carpeta se cargue, se
//! le pueda pedir algo, y que lo que Chilpa le presta —el almacén, la hora,
//! el log— llegue y funcione.
//!
//! Es una sola prueba porque apunta `XDG_DATA_HOME` a un directorio
//! temporal, y esa variable es del proceso entero.

use std::path::{Path, PathBuf};

/// Lo mínimo que un plugin tiene que exportar, más un bloc de notas.
const PRELUDIO: &str = r#"
  (memory (export "memory") 1)
  (global $siguiente (mut i32) (i32.const 1024))
  (func (export "chilpa_alloc") (param i32) (result i32)
    (local i32)
    (local.set 1 (global.get $siguiente))
    (global.set $siguiente (i32.add (global.get $siguiente) (local.get 0)))
    (local.get 1))
  (data (i32.const 0) "{\"saludo\":\"hola\"}")
  (data (i32.const 64) "ultima-sync")
  (data (i32.const 96) "42")
"#;

fn instalar(raiz: &Path, id: &str, capacidades: &str, cuerpo: &str) {
    let dir = raiz.join(id);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("manifest.json"),
        format!(
            r#"{{
                "id": "{id}",
                "name": "Plugin de prueba",
                "version": "0.1.0",
                "api_version": 1,
                "kind": "source",
                "capabilities": [{capacidades}]
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
            {PRELUDIO}
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            {cuerpo})"#
    );
    std::fs::write(dir.join("plugin.wasm"), wat::parse_str(&wat).expect("compila")).unwrap();
}

#[test]
fn un_plugin_en_su_carpeta_carga_y_contesta() {
    let dir = std::env::temp_dir().join("chilpa-api-plugins");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("XDG_DATA_HOME", dir.join("datos"));
    let raiz: PathBuf = chilpa_api::Chilpa::plugins_dir();
    std::fs::create_dir_all(&raiz).unwrap();

    // Uno que guarda algo, lo vuelve a leer, anota en el log y contesta.
    instalar(
        &raiz,
        "com.prueba.bueno",
        r#"{"tipo": "storage"}"#,
        r#"(func (export "saludar") (param i32 i32) (result i64)
             (drop (call $set (i32.const 64) (i32.const 11) (i32.const 96) (i32.const 2)))
             (drop (call $get (i32.const 64) (i32.const 11) (i32.const 2048) (i32.const 64)))
             (call $log (i32.const 1) (i32.const 64) (i32.const 11))
             (drop (call $now))
             (i64.const 17))"#,
    );
    // Y uno que pide una versión de API que no existe.
    instalar(&raiz, "com.prueba.viejo", "", "(func (export \"saludar\") (param i32 i32) (result i64) (i64.const 17))");
    let viejo = raiz.join("com.prueba.viejo").join("manifest.json");
    let m = std::fs::read_to_string(&viejo).unwrap().replace(r#""api_version": 1"#, r#""api_version": 42"#);
    std::fs::write(&viejo, m).unwrap();

    let mut chilpa = chilpa_api::Chilpa::new();
    let utilizables = chilpa.load_plugins();
    assert_eq!(utilizables, 1, "uno cargó y el otro no");

    let lista = chilpa.plugins();
    assert_eq!(lista.len(), 2, "los dos se listan, cargue o no");
    let bueno = lista.iter().find(|p| p.id == "com.prueba.bueno").unwrap();
    let viejo = lista.iter().find(|p| p.id == "com.prueba.viejo").unwrap();
    assert_eq!(bueno.state, "bien");
    assert_eq!(bueno.name, "Plugin de prueba");
    assert_eq!(viejo.state, "en cuarentena");
    assert!(viejo.problem.contains("versión"), "{}", viejo.problem);

    // Se le pide algo y contesta su JSON.
    let r = chilpa.call_plugin("com.prueba.bueno", "saludar", b"{}").expect("contesta");
    assert_eq!(String::from_utf8_lossy(&r), r#"{"saludo":"hola"}"#);

    // Y lo que guardó quedó en su carpeta, no en la de nadie más.
    let guardado = raiz.join("com.prueba.bueno").join("almacen").join("ultima-sync");
    assert_eq!(std::fs::read(&guardado).unwrap(), b"42");
    assert!(!raiz.join("com.prueba.viejo").join("almacen").exists());

    // Al que está en cuarentena no se le pide nada, ni aunque se insista.
    let e = chilpa.call_plugin("com.prueba.viejo", "saludar", b"{}").expect_err("no se le llama");
    assert!(e.contains("cuarentena"), "{e}");

    // Hasta que el usuario lo reactive; entonces vuelve a fallar al cargar,
    // porque nunca cargó.
    chilpa.reactivate_plugin("com.prueba.viejo");
    let e = chilpa.call_plugin("com.prueba.viejo", "saludar", b"{}").expect_err("sigue sin cargar");
    assert!(e.contains("no cargó"), "{e}");

    // Uno que no existe se dice y ya.
    let e = chilpa.call_plugin("com.no.existe", "saludar", b"{}").expect_err("no está");
    assert!(e.contains("no está instalado"), "{e}");

    let _ = std::fs::remove_dir_all(dir);
}
