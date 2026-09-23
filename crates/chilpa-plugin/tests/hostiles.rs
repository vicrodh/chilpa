//! La suite de plugins hostiles que pide el ADR-003.
//!
//! Cada uno de estos plugins intenta una cosa distinta: tronar, colgarse,
//! comerse la memoria, mentir sobre su versión, salir a un dominio que no
//! declaró, leer un almacén que no pidió, o contestar una dirección que no
//! es suya. En todos los casos se afirman dos cosas:
//!
//! 1. **Chilpa sigue.** La llamada se detiene, no el proceso.
//! 2. **No llegó a donde no debía.** El host de prueba se acuerda de todo lo
//!    que le pidieron, así que se puede afirmar que no le pidieron nada.
//!
//! Los plugins van escritos en texto de wasm para que la suite corra sin
//! ninguna cadena de compilación aparte.

use chilpa_plugin::{
    Detencion, Estado, Host, HostDePrueba, Limits, PluginError, Registry, Respuesta, Servicios,
};
use std::sync::{Arc, Mutex};

/// El manifest de siempre, con las capacidades que se le pidan.
fn manifest(capacidades: &str) -> String {
    format!(
        r#"{{
            "id": "com.prueba.hostil",
            "name": "Hostil",
            "version": "0.1.0",
            "api_version": 1,
            "kind": "source",
            "capabilities": [{capacidades}]
        }}"#
    )
}

/// Lo que todo plugin tiene que traer para que lo carguen.
const PRELUDIO: &str = r#"
  (memory (export "memory") 1)
  (global $siguiente (mut i32) (i32.const 1024))
  (func (export "chilpa_alloc") (param i32) (result i32)
    (local i32)
    (local.set 1 (global.get $siguiente))
    (global.set $siguiente (i32.add (global.get $siguiente) (local.get 0)))
    (local.get 1))
  (data (i32.const 0) "{\"ok\":true}")
"#;

/// La respuesta fija que deja el preludio: `{"ok":true}`, en (0, 11).
const RESPUESTA_FIJA: &str = "(i64.const 11)";

fn compilar(wat: &str) -> Vec<u8> {
    wat::parse_str(wat).expect("el plugin de prueba compila")
}

/// Un host de prueba y la copia con la que la prueba lo mira.
fn servicios() -> (Servicios, HostDePrueba) {
    let cuaderno = HostDePrueba::new();
    let s: Servicios = Arc::new(Mutex::new(cuaderno.clone()));
    (s, cuaderno)
}

/// Instala un plugin y lo llama una vez.
fn correr(
    capacidades: &str,
    wat: &str,
    operacion: &str,
) -> (Registry, HostDePrueba, Result<Respuesta, PluginError>) {
    let host = Host::new().unwrap();
    let mut reg = Registry::new();
    let (svc, cuaderno) = servicios();
    assert!(reg.instalar(&host, &manifest(capacidades), &compilar(wat)), "debía cargar");
    let r = reg.llamar(&host, &svc, "com.prueba.hostil", operacion, b"{}", Limits::cortos());
    (reg, cuaderno, r)
}

#[test]
fn uno_que_se_porta_bien_carga_y_contesta() {
    let wat = format!(
        r#"(module {PRELUDIO}
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (func (export "buscar") (param i32 i32) (result i64) {RESPUESTA_FIJA}))"#
    );
    let (reg, _cuaderno, r) = correr("", &wat, "buscar");
    assert_eq!(r.unwrap(), Respuesta::Ok(br#"{"ok":true}"#.to_vec()));
    assert_eq!(reg.estado("com.prueba.hostil"), Estado::Bien);
    assert_eq!(reg.utilizables(), vec!["com.prueba.hostil"]);
}

#[test]
fn uno_que_truena_se_detiene_y_chilpa_sigue() {
    let wat = format!(
        r#"(module {PRELUDIO}
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (func (export "buscar") (param i32 i32) (result i64) (unreachable)))"#
    );
    let (mut reg, _cuaderno, r) = correr("", &wat, "buscar");
    assert!(matches!(r, Err(PluginError::Trono(_))), "{r:?}");
    // Y se puede seguir pidiendo: una falla no es cuarentena.
    assert!(reg.estado("com.prueba.hostil").usable());
    let host = Host::new().unwrap();
    let (svc, _) = servicios();
    let _ = reg.llamar(&host, &svc, "com.prueba.hostil", "buscar", b"{}", Limits::cortos());
    let _ = reg.llamar(&host, &svc, "com.prueba.hostil", "buscar", b"{}", Limits::cortos());
    assert!(
        matches!(reg.estado("com.prueba.hostil"), Estado::Cuarentena { .. }),
        "a la tercera va a cuarentena"
    );
    // Y en cuarentena ya no se le llama.
    let r = reg.llamar(&host, &svc, "com.prueba.hostil", "buscar", b"{}", Limits::cortos());
    assert!(matches!(r, Err(PluginError::SinPermiso(_))), "{r:?}");
}

#[test]
fn uno_que_se_cuelga_se_corta_por_combustible() {
    let wat = format!(
        r#"(module {PRELUDIO}
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (func (export "buscar") (param i32 i32) (result i64)
              (loop $otra (br $otra))
              (i64.const 0)))"#
    );
    let inicio = std::time::Instant::now();
    let (_reg, _cuaderno, r) = correr("", &wat, "buscar");
    assert_eq!(r, Err(PluginError::Detenido(Detencion::SinCombustible)), "{r:?}");
    assert!(inicio.elapsed() < std::time::Duration::from_secs(5), "se cortó pronto");
}

#[test]
fn a_uno_que_pide_mas_memoria_de_la_permitida_se_le_niega() {
    // Ocho megabytes son 128 páginas; pide 200 de un jalón. Contesta la
    // respuesta buena solo si el motor se lo negó.
    let wat = format!(
        r#"(module {PRELUDIO}
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (func (export "buscar") (param i32 i32) (result i64)
              (if (i32.eq (memory.grow (i32.const 200)) (i32.const -1))
                (then (return {RESPUESTA_FIJA})))
              (i64.const 0)))"#
    );
    let (_reg, _cuaderno, r) = correr("", &wat, "buscar");
    assert_eq!(
        r.unwrap(),
        Respuesta::Ok(br#"{"ok":true}"#.to_vec()),
        "el motor le negó la memoria; si hubiera crecido, contestaría nada"
    );
}

#[test]
fn uno_que_se_come_la_memoria_en_un_lazo_se_detiene() {
    // Crece de a una página para siempre. Con el límite puesto nunca crece,
    // y lo que lo detiene, medido, es el reloj y no el combustible: pedir
    // memoria es caro en tiempo y barato en instrucciones. Por eso hacen
    // falta los dos límites, y por eso aquí se afirma que se detuvo y no
    // con cuál de los dos.
    let wat = format!(
        r#"(module {PRELUDIO}
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (func (export "buscar") (param i32 i32) (result i64)
              (loop $otra
                (drop (memory.grow (i32.const 1)))
                (br $otra))
              (i64.const 0)))"#
    );
    let (_reg, _cuaderno, r) = correr("", &wat, "buscar");
    match r {
        Err(PluginError::Detenido(d)) => {
            assert!(!d.to_string().is_empty(), "y se dice por qué se detuvo");
        }
        otro => panic!("debía detenerse por alguno de sus límites: {otro:?}"),
    }
}

#[test]
fn uno_que_miente_sobre_su_version_no_carga() {
    let wat = format!(
        r#"(module {PRELUDIO}
            (func (export "chilpa_api_version") (result i32) (i32.const 7))
            (func (export "buscar") (param i32 i32) (result i64) {RESPUESTA_FIJA}))"#
    );
    let host = Host::new().unwrap();
    let mut reg = Registry::new();
    assert!(!reg.instalar(&host, &manifest(""), &compilar(&wat)), "no debía cargar");
    let inst = reg.instalados().next().unwrap();
    assert!(!inst.cargado());
    assert!(inst.problema.as_ref().unwrap().contains("no se carga"), "{:?}", inst.problema);
    assert!(reg.utilizables().is_empty());
}

#[test]
fn uno_al_que_le_faltan_exports_no_carga() {
    // Sin memoria ni alloc: no hay forma de hablarle.
    let wat = r#"(module (func (export "chilpa_api_version") (result i32) (i32.const 1)))"#;
    let host = Host::new().unwrap();
    let mut reg = Registry::new();
    assert!(!reg.instalar(&host, &manifest(""), &compilar(wat)));
    let problema = reg.instalados().next().unwrap().problema.clone().unwrap();
    assert!(problema.contains("memory"), "{problema}");
}

/// Un plugin que le pide al host esa URL. El largo se calcula, no se
/// escribe a mano: uno mal puesto trunca el JSON y la prueba pasaría porque
/// la petición no se entiende, no porque el dominio esté prohibido.
fn plugin_que_pide(url: &str) -> String {
    let peticion = format!(r#"{{"url":"{url}"}}"#);
    let escapado = peticion.replace('"', "\\\"");
    let largo = peticion.len();
    format!(
        r#"(module
            (import "chilpa" "http" (func $http (param i32 i32 i32 i32) (result i32)))
            {PRELUDIO}
            (data (i32.const 256) "{escapado}")
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (func (export "buscar") (param i32 i32) (result i64)
              (drop (call $http (i32.const 256) (i32.const {largo}) (i32.const 2048) (i32.const 512)))
              {RESPUESTA_FIJA}))"#
    )
}

#[test]
fn uno_que_sale_a_un_dominio_que_no_declaro_no_llega_al_host() {
    // Declara ejemplo.com y pide otra-cosa.net.
    let wat = plugin_que_pide("https://otra-cosa.net/robar");
    let (_reg, cuaderno, r) =
        correr(r#"{"tipo": "network", "domains": ["ejemplo.com"]}"#, &wat, "buscar");
    assert!(r.is_ok(), "la llamada termina; lo que se niega es la salida: {r:?}");
    let peticiones = cuaderno.mirar(|a| a.peticiones.len());
    assert_eq!(peticiones, 0, "el host nunca vio esa petición");
}

#[test]
fn uno_que_sale_al_dominio_que_si_declaro_llega() {
    let wat = plugin_que_pide("https://api.ejemplo.com/buscar");
    let (_reg, cuaderno, r) =
        correr(r#"{"tipo": "network", "domains": ["ejemplo.com"]}"#, &wat, "buscar");
    assert!(r.is_ok(), "{r:?}");
    let (cuantas, quien) = cuaderno.mirar(|a| {
        (a.peticiones.len(), a.peticiones.first().map(|p| p.0.clone()).unwrap_or_default())
    });
    assert_eq!(cuantas, 1, "el subdominio del dominio declarado sí pasa");
    assert_eq!(quien, "com.prueba.hostil", "la petición va a nombre del plugin, no de otro");
}

#[test]
fn uno_sin_permiso_de_red_no_llega_al_host_aunque_lo_intente() {
    let wat = plugin_que_pide("https://api.ejemplo.com/x");
    let (_reg, cuaderno, r) = correr("", &wat, "buscar");
    assert!(r.is_ok(), "{r:?}");
    assert_eq!(cuaderno.mirar(|a| a.peticiones.len()), 0, "sin capacidad de red no hay red");
}

#[test]
fn uno_sin_almacen_no_lee_ni_escribe() {
    let wat = format!(
        r#"(module
            (import "chilpa" "store_get" (func $get (param i32 i32 i32 i32) (result i32)))
            (import "chilpa" "store_set" (func $set (param i32 i32 i32 i32) (result i32)))
            {PRELUDIO}
            (data (i32.const 256) "token")
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (func (export "buscar") (param i32 i32) (result i64)
              (drop (call $get (i32.const 256) (i32.const 5) (i32.const 2048) (i32.const 512)))
              (drop (call $set (i32.const 256) (i32.const 5) (i32.const 256) (i32.const 5)))
              {RESPUESTA_FIJA}))"#
    );
    let (_reg, cuaderno, r) = correr("", &wat, "buscar");
    assert!(r.is_ok(), "{r:?}");
    let (lecturas, escrituras) = cuaderno.mirar(|a| (a.lecturas.len(), a.escrituras.len()));
    assert_eq!(lecturas, 0, "sin capacidad de almacén no se lee nada");
    assert_eq!(escrituras, 0, "ni se escribe");
}

#[test]
fn el_almacen_de_un_plugin_va_siempre_a_su_nombre() {
    let wat = format!(
        r#"(module
            (import "chilpa" "store_set" (func $set (param i32 i32 i32 i32) (result i32)))
            {PRELUDIO}
            (data (i32.const 256) "llave")
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (func (export "buscar") (param i32 i32) (result i64)
              (drop (call $set (i32.const 256) (i32.const 5) (i32.const 256) (i32.const 5)))
              {RESPUESTA_FIJA}))"#
    );
    let (_reg, cuaderno, r) = correr(r#"{"tipo": "storage"}"#, &wat, "buscar");
    assert!(r.is_ok(), "{r:?}");
    let quien = cuaderno.mirar(|a| a.escrituras.first().map(|e| e.0.clone()).unwrap_or_default());
    assert_eq!(
        quien, "com.prueba.hostil",
        "el id lo pone el host desde el manifest, no el guest"
    );
}

#[test]
fn uno_que_contesta_una_direccion_ajena_no_hace_que_chilpa_lea_de_mas() {
    // Contesta una dirección enorme, muy por fuera de su memoria.
    let wat = format!(
        r#"(module {PRELUDIO}
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (func (export "buscar") (param i32 i32) (result i64)
              (i64.or (i64.shl (i64.const 4000000000) (i64.const 32)) (i64.const 1000))))"#
    );
    let (_reg, _cuaderno, r) = correr("", &wat, "buscar");
    assert_eq!(r, Err(PluginError::MemoriaFuera), "{r:?}");
}

#[test]
fn uno_que_no_contesta_nada_no_se_confunde_con_uno_que_contesto_vacio() {
    let wat = format!(
        r#"(module {PRELUDIO}
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (func (export "buscar") (param i32 i32) (result i64) (i64.const 0)))"#
    );
    let (_reg, _cuaderno, r) = correr("", &wat, "buscar");
    assert!(matches!(&r, Ok(Respuesta::Error(e)) if e.contains("no contestó")), "{r:?}");
}

#[test]
fn una_operacion_que_el_plugin_no_tiene_se_dice_y_ya() {
    let wat = format!(
        r#"(module {PRELUDIO}
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (func (export "buscar") (param i32 i32) (result i64) {RESPUESTA_FIJA}))"#
    );
    let (_reg, _cuaderno, r) = correr("", &wat, "una_que_no_existe");
    assert!(matches!(&r, Err(PluginError::FaltaExport(n)) if n == "una_que_no_existe"), "{r:?}");
}

#[test]
fn un_plugin_roto_no_se_lleva_a_los_demas() {
    let host = Host::new().unwrap();
    let (svc, _) = servicios();
    let mut reg = Registry::new();

    let bueno = format!(
        r#"(module {PRELUDIO}
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (func (export "buscar") (param i32 i32) (result i64) {RESPUESTA_FIJA}))"#
    );
    let roto = format!(
        r#"(module {PRELUDIO}
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (func (export "buscar") (param i32 i32) (result i64) (unreachable)))"#
    );
    let m = |id: &str| manifest("").replace("com.prueba.hostil", id);

    assert!(reg.instalar(&host, &m("com.bueno.plugin"), &compilar(&bueno)));
    assert!(reg.instalar(&host, &m("com.roto.plugin"), &compilar(&roto)));

    for _ in 0..3 {
        let _ = reg.llamar(&host, &svc, "com.roto.plugin", "buscar", b"{}", Limits::cortos());
    }
    assert!(matches!(reg.estado("com.roto.plugin"), Estado::Cuarentena { .. }));

    // Y el bueno sigue trabajando.
    let r = reg.llamar(&host, &svc, "com.bueno.plugin", "buscar", b"{}", Limits::cortos());
    assert_eq!(r.unwrap(), Respuesta::Ok(br#"{"ok":true}"#.to_vec()));
    assert_eq!(reg.utilizables(), vec!["com.bueno.plugin"]);
    assert_eq!(reg.en_cuarentena().len(), 1);
}

#[test]
fn de_la_cuarentena_solo_lo_saca_el_usuario() {
    let wat = format!(
        r#"(module {PRELUDIO}
            (func (export "chilpa_api_version") (result i32) (i32.const 1))
            (func (export "buscar") (param i32 i32) (result i64) (unreachable)))"#
    );
    let host = Host::new().unwrap();
    let (svc, _) = servicios();
    let mut reg = Registry::new();
    reg.instalar(&host, &manifest(""), &compilar(&wat));
    for _ in 0..3 {
        let _ = reg.llamar(&host, &svc, "com.prueba.hostil", "buscar", b"{}", Limits::cortos());
    }
    assert!(reg.utilizables().is_empty());
    reg.reactivar("com.prueba.hostil");
    assert_eq!(reg.utilizables(), vec!["com.prueba.hostil"], "el usuario lo revivió");
}
